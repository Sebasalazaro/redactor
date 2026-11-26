//! Redacting files and folders: Postman collections, JSON, TXT, logs, HAR,
//! CSV and Excel workbooks. CSV and Excel are read as tables: columns titled
//! like people or documents are masked whole (see `redactor_core::table`).
//!
//! Files and folders come from the native pickers or are dropped on the
//! dashboard window; either way the paths are received here and never come
//! from the webview, which only sees names and results. Everything is
//! redacted in memory first and written only when the user accepts:
//!
//! - a file gets a copy next to it: `collection.json` →
//!   `collection.redacted.json`;
//! - a folder gets a sister folder with the same tree: `Cliente/` →
//!   `Cliente.redacted/`. Files that cannot be redacted (PDF, images,
//!   binaries) are left out and listed.
//!
//! Names holding configured terms are redacted too (`Globex_contrato.json` →
//! `CLIENT_contrato.json`). Originals are never written to.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use redactor_core::table::Tally;
use redactor_core::{Category, Redactor};
use serde::Serialize;

use tauri::{AppHandle, DragDropEvent, Emitter, Manager};

use crate::flow::DASHBOARD;
use crate::state::AppState;
use crate::vault::Pair;

/// Larger files are refused: this is for collections and exports, not dumps.
const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// A folder with more files than this, or more bytes, is refused whole.
const MAX_FOLDER_FILES: usize = 5000;
const MAX_FOLDER_BYTES: u64 = 500 * 1024 * 1024;
/// Folders not worth reading: tool state, not documents.
const SKIPPED_FOLDERS: &[&str] = &[".git", "node_modules", "__pycache__", ".venv", ".svn"];
/// Characters of each redacted file shown in the page.
const PREVIEW_CHARS: usize = 4000;
const BOM: &str = "\u{feff}";

#[derive(Debug, Default)]
pub struct Files {
    items: Vec<Item>,
    next_id: u32,
}

#[derive(Debug)]
struct Item {
    id: u32,
    source: PathBuf,
    /// The folder the output lands in, and the output's path below it.
    base: PathBuf,
    segments: Vec<String>,
    /// The source's path below `base`, for display.
    source_label: String,
    state: ItemState,
}

impl Item {
    fn target(&self) -> PathBuf {
        self.segments
            .iter()
            .fold(self.base.clone(), |p, s| p.join(s))
    }
}

#[derive(Debug)]
enum ItemState {
    Ready(Box<Redacted>),
    /// Left out: binary, unsupported, too large, or a folder not read.
    Skipped(String),
    Saved {
        values: usize,
        text: Option<String>,
    },
    Failed(String),
}

#[derive(Debug)]
struct Redacted {
    /// The new file content, BOM included if the original had one.
    output: Vec<u8>,
    /// What the page shows of it.
    preview: String,
    /// The content as text to paste into a chat, for workbooks.
    text: Option<String>,
    /// `http_request`, `postman`, `text`..., or `csv` and `xlsx`.
    format: String,
    categories: Vec<(Category, usize)>,
    values: usize,
    pairs: Vec<Pair>,
}

#[derive(Debug, Serialize)]
pub struct FileDto {
    pub id: u32,
    /// Where outputs land: the original's folder, or a dropped folder's parent.
    pub base: String,
    /// The output below `base`: `["Cliente.redacted", "2026", "pagos.xlsx"]`.
    pub segments: Vec<String>,
    /// The original below `base`: `Cliente/2026/pagos.xlsx`.
    pub source: String,
    /// `ready`, `skipped`, `saved` or `failed`.
    pub status: &'static str,
    pub message: Option<String>,
    pub format: Option<String>,
    pub values: usize,
    pub categories: Vec<(Category, usize)>,
    pub preview: Option<String>,
    /// A text version can be copied (workbooks).
    pub has_text: bool,
    /// A folder left out (`.git`, `node_modules`), not a file.
    pub folder: bool,
}

impl Files {
    /// Reads and redacts files and whole folders. Adding a source again
    /// redacts it again, so changed rules apply.
    pub fn add(&mut self, paths: impl IntoIterator<Item = PathBuf>, redactor: &Redactor) {
        for path in paths {
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                self.add_folder(&path, redactor);
            } else if meta.is_file() {
                let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
                let name = copy_name(redactor, &file_name(&path));
                let label = file_name(&path);
                self.upsert(path, base, vec![name], label, |p| redact(p, redactor));
            }
        }
    }

    fn add_folder(&mut self, dir: &Path, redactor: &Redactor) {
        let base = dir.parent().map(Path::to_path_buf).unwrap_or_default();
        let dir_name = file_name(dir);
        let root = format!("{}.redacted", clean_name(redactor, &dir_name));

        let mut found = Vec::new();
        walk(dir, &mut Vec::new(), &mut found);
        let files = found.iter().filter(|f| !f.skipped_folder).count();
        let bytes: u64 = found.iter().map(|f| f.size).sum();
        if files > MAX_FOLDER_FILES || bytes > MAX_FOLDER_BYTES {
            let reason = format!(
                "too large to redact at once ({files} files, {} MB; the limit is {MAX_FOLDER_FILES} files and {} MB)",
                bytes / (1024 * 1024),
                MAX_FOLDER_BYTES / (1024 * 1024)
            );
            self.upsert(dir.to_path_buf(), base, vec![root], dir_name, |_| {
                Err(reason.clone())
            });
            return;
        }

        for f in found {
            let mut segments = vec![root.clone()];
            segments.extend(f.rel.iter().map(|part| clean_name(redactor, part)));
            let label = std::iter::once(dir_name.as_str())
                .chain(f.rel.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("/");
            if f.skipped_folder {
                self.upsert(f.path, base.clone(), segments, label, |_| {
                    Err("folder not read (tool files)".into())
                });
            } else {
                self.upsert(f.path, base.clone(), segments, label, |p| {
                    redact(p, redactor)
                });
            }
        }
    }

    /// Adds or replaces the entry for `source`. Two sources that would land
    /// on the same output get ` (2)`, ` (3)`... before the extension.
    fn upsert(
        &mut self,
        source: PathBuf,
        base: PathBuf,
        mut segments: Vec<String>,
        source_label: String,
        redact: impl FnOnce(&Path) -> Result<Redacted, String>,
    ) {
        let state = match redact(&source) {
            Ok(redacted) => ItemState::Ready(Box::new(redacted)),
            Err(reason) => ItemState::Skipped(reason),
        };
        let taken = |segments: &[String], items: &[Item]| {
            items
                .iter()
                .any(|i| i.source != source && i.base == base && i.segments == segments)
        };
        let mut n = 2;
        let last = segments.len() - 1;
        let original = segments[last].clone();
        while taken(&segments, &self.items) {
            segments[last] = numbered(&original, n);
            n += 1;
        }
        if let Some(item) = self.items.iter_mut().find(|i| i.source == source) {
            item.base = base;
            item.segments = segments;
            item.source_label = source_label;
            item.state = state;
        } else {
            self.next_id += 1;
            self.items.push(Item {
                id: self.next_id,
                source,
                base,
                segments,
                source_label,
                state,
            });
        }
    }

    pub fn remove(&mut self, id: u32) {
        self.items.retain(|i| i.id != id);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Writes every ready entry, creating folders as needed. Returns the
    /// values to remember in the vault.
    pub fn save(&mut self) -> Vec<Pair> {
        let mut pairs = Vec::new();
        let mut saved = 0;
        for item in &mut self.items {
            let target = item.target();
            let ItemState::Ready(redacted) = &mut item.state else {
                continue;
            };
            let result = if target == item.source {
                Err("would overwrite the original".to_string())
            } else {
                target
                    .parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .map_err(|e| format!("cannot create its folder: {e}"))
                    .and_then(|()| write_new(&target, &redacted.output))
            };
            item.state = match result {
                Ok(()) => {
                    saved += 1;
                    pairs.append(&mut redacted.pairs);
                    ItemState::Saved {
                        values: redacted.values,
                        text: redacted.text.take(),
                    }
                }
                Err(e) => ItemState::Failed(e),
            };
        }
        if saved > 0 {
            crate::log::info(format_args!("saved {saved} redacted files"));
        }
        pairs
    }

    pub fn dto(&self) -> Vec<FileDto> {
        self.items.iter().map(Item::dto).collect()
    }

    /// The text version of a workbook, to copy.
    pub fn text(&self, id: u32) -> Option<String> {
        let item = self.items.iter().find(|i| i.id == id)?;
        match &item.state {
            ItemState::Ready(r) => r.text.clone(),
            ItemState::Saved { text, .. } => text.clone(),
            _ => None,
        }
    }
}

impl Item {
    fn dto(&self) -> FileDto {
        let mut dto = FileDto {
            id: self.id,
            base: self.base.display().to_string(),
            segments: self.segments.clone(),
            source: self.source_label.clone(),
            status: "ready",
            message: None,
            format: None,
            values: 0,
            categories: Vec::new(),
            preview: None,
            has_text: false,
            folder: self.source.is_dir(),
        };
        match &self.state {
            ItemState::Ready(r) => {
                dto.format = Some(r.format.clone());
                dto.values = r.values;
                dto.categories = r.categories.clone();
                dto.preview = Some(r.preview.chars().take(PREVIEW_CHARS).collect());
                dto.has_text = r.text.is_some();
            }
            ItemState::Skipped(reason) => {
                dto.status = "skipped";
                dto.message = Some(reason.clone());
            }
            ItemState::Saved { values, text } => {
                dto.status = "saved";
                dto.values = *values;
                dto.has_text = text.is_some();
            }
            ItemState::Failed(e) => {
                dto.status = "failed";
                dto.message = Some(e.clone());
            }
        }
        dto
    }
}

/// A file or skipped folder found under a dropped folder.
struct Found {
    path: PathBuf,
    /// Path components below the dropped folder.
    rel: Vec<String>,
    size: u64,
    skipped_folder: bool,
}

/// Every file below `dir`, sorted by name. Symbolic links are not followed.
fn walk(dir: &Path, rel: &mut Vec<String>, out: &mut Vec<Found>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if kind.is_symlink() {
            continue;
        }
        rel.push(name.clone());
        if kind.is_dir() {
            if SKIPPED_FOLDERS.contains(&name.as_str()) {
                out.push(Found {
                    path: entry.path(),
                    rel: rel.clone(),
                    size: 0,
                    skipped_folder: true,
                });
            } else {
                walk(&entry.path(), rel, out);
            }
        } else if kind.is_file() {
            out.push(Found {
                path: entry.path(),
                rel: rel.clone(),
                size: entry.metadata().map(|m| m.len()).unwrap_or(0),
                skipped_folder: false,
            });
        }
        rel.pop();
    }
}

fn redact(path: &Path, redactor: &Redactor) -> Result<Redacted, String> {
    let size = fs::metadata(path)
        .map_err(|e| format!("cannot read it: {e}"))?
        .len();
    if size > MAX_BYTES {
        return Err(format!(
            "too large ({} MB, the limit is {} MB)",
            size / (1024 * 1024),
            MAX_BYTES / (1024 * 1024)
        ));
    }
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if matches!(extension.as_str(), "xls" | "xlsb" | "ods" | "numbers") {
        return Err("this spreadsheet format is not supported; save it as .xlsx first".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("cannot read it: {e}"))?;
    if extension == "xlsx" {
        return redact_workbook(&bytes, redactor);
    }
    let text = text_of(bytes)?;
    let (bom, body) = match text.strip_prefix(BOM) {
        Some(body) => (true, body),
        None => (false, text.as_str()),
    };

    let mut tally = Tally::default();
    let (output, format) = if extension == "csv" {
        (redactor.redact_csv(body, &mut tally), "csv".to_string())
    } else {
        let redaction = redactor.redact(body);
        for f in &redaction.findings {
            *tally.categories.entry(f.category).or_default() += 1;
            tally
                .pairs
                .push((f.replacement.clone(), redaction.original(f).to_string()));
        }
        let format = serde_json::to_value(redaction.format)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "text".into());
        (redaction.output, format)
    };

    // A collection or export that stops being valid JSON would be useless.
    if serde_json::from_str::<serde::de::IgnoredAny>(body).is_ok()
        && serde_json::from_str::<serde::de::IgnoredAny>(&output).is_err()
    {
        return Err("its redacted version is not valid JSON; it was left as it is".into());
    }

    let content = if bom {
        format!("{BOM}{output}")
    } else {
        output.clone()
    };
    Ok(finish(content.into_bytes(), output, None, format, tally))
}

/// An Excel workbook: redacted cells, comments and properties, plus every
/// sheet as text for a chat.
fn redact_workbook(bytes: &[u8], redactor: &Redactor) -> Result<Redacted, String> {
    let mut tally = Tally::default();
    let out = redactor
        .redact_xlsx(bytes, &mut tally)
        .map_err(|e| format!("cannot read the workbook: {e}"))?;
    let preview = out.text.clone();
    Ok(finish(
        out.bytes,
        preview,
        Some(out.text),
        "xlsx".into(),
        tally,
    ))
}

fn finish(
    output: Vec<u8>,
    preview: String,
    text: Option<String>,
    format: String,
    tally: Tally,
) -> Redacted {
    Redacted {
        output,
        preview,
        text,
        format,
        values: tally.values(),
        categories: tally.categories.into_iter().collect(),
        pairs: tally
            .pairs
            .into_iter()
            .map(|(replacement, original)| Pair {
                replacement,
                original,
            })
            .collect(),
    }
}

/// UTF-8 text without NUL bytes; anything else is treated as binary.
fn text_of(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.contains(&0) {
        return Err("not a text file".into());
    }
    String::from_utf8(bytes).map_err(|_| "not UTF-8 text".into())
}

/// Drag and drop on the dashboard window. Hovering tells the page to show
/// the drop zone; dropping redacts off the main thread, then tells the page.
pub fn dropped(app: &AppHandle, event: &DragDropEvent) {
    match event {
        DragDropEvent::Enter { .. } | DragDropEvent::Over { .. } => {
            let _ = app.emit_to(DASHBOARD, "files-hover", true);
        }
        DragDropEvent::Leave => {
            let _ = app.emit_to(DASHBOARD, "files-hover", false);
        }
        DragDropEvent::Drop { paths, .. } => {
            let _ = app.emit_to(DASHBOARD, "files-hover", false);
            let (app, paths) = (app.clone(), paths.clone());
            std::thread::spawn(move || {
                let state = app.state::<AppState>();
                let redactor = state.redactor();
                state.files().add(paths, &redactor);
                let _ = app.emit_to(DASHBOARD, "files-changed", ());
            });
        }
        _ => {}
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A file or folder name with configured terms replaced, safe on every
/// system: `Globex_contrato.json` → `CLIENT_contrato.json`. Placeholders
/// lose their brackets, and characters Windows forbids (`*` from partial
/// masks among them) become `_`.
fn clean_name(redactor: &Redactor, name: &str) -> String {
    let replaced = redactor.replace_terms(name);
    if replaced == name {
        return name.to_string();
    }
    let cleaned: String = replaced
        .chars()
        .filter(|c| !matches!(c, '[' | ']'))
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim_end_matches(['.', ' ']).trim();
    if cleaned.is_empty() {
        "redacted".into()
    } else {
        cleaned.to_string()
    }
}

/// The name of a single file's copy: `collection.json` →
/// `collection.redacted.json`, `notes` → `notes.redacted`.
fn copy_name(redactor: &Redactor, name: &str) -> String {
    let clean = clean_name(redactor, name);
    match clean.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem}.redacted.{ext}"),
        _ => format!("{clean}.redacted"),
    }
}

/// `pagos.xlsx` → `pagos (2).xlsx`.
fn numbered(name: &str, n: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem} ({n}).{ext}"),
        _ => format!("{name} ({n})"),
    }
}

/// Writes to a temporary file next to the target, then renames it over the
/// target, so a crash never leaves a half-written copy behind. An earlier
/// redacted copy is replaced.
fn write_new(path: &Path, content: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("no parent folder")?;
    let name = path.file_name().ok_or("no file name")?.to_string_lossy();
    let tmp = dir.join(format!(".{name}.redactor-tmp"));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| format!("cannot write it: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use redactor_core::Config;

    fn redactor() -> Redactor {
        Redactor::new(&Config {
            client: vec!["Globex".into()],
            ..Default::default()
        })
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("redactor-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: impl AsRef<[u8]>) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn saves_a_redacted_copy_next_to_the_original() {
        let dir = temp_dir("copy");
        let path = dir.join("Globex API.postman_collection.json");
        let original = "\u{feff}{\r\n  \"url\": \"https://api.globex.example/v1\",\r\n  \"token\": \"a7f3e9c1d5b2084f6e1a9c3d7b5f2e80\"\r\n}\r\n";
        write(&path, original);

        let mut files = Files::default();
        files.add([path.clone()], &redactor());
        let dto = &files.dto()[0];
        assert_eq!(dto.status, "ready", "{:?}", dto.message);
        assert_eq!(
            dto.segments,
            ["CLIENT API.postman_collection.redacted.json"]
        );

        let pairs = files.save();
        assert!(!pairs.is_empty());
        // The original is untouched; the copy keeps BOM and line endings.
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let copy =
            fs::read_to_string(dir.join("CLIENT API.postman_collection.redacted.json")).unwrap();
        assert!(copy.starts_with('\u{feff}'));
        assert!(copy.contains("\r\n"));
        assert!(!copy.to_lowercase().contains("globex"), "{copy}");
        assert!(!copy.contains("a7f3e9c1d5b2084f6e1a9c3d7b5f2e80"));
        assert_eq!(files.dto()[0].status, "saved");
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            2,
            "original and copy, no temp file"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn redacts_a_folder_into_a_sister_folder() {
        let dir = temp_dir("folder");
        let src = dir.join("Globex docs");
        write(
            &src.join("2026").join("Globex_contrato.json"),
            r#"{"cliente": "Globex", "valor": 1200}"#,
        );
        write(&src.join("notas.txt"), "Reunión con Globex el martes");
        write(&src.join("foto.png"), b"\x89PNG\r\n\x1a\n\0\0");
        write(&src.join(".git").join("config"), "[core]");

        let mut files = Files::default();
        files.add([src.clone()], &redactor());
        let dtos = files.dto();
        let by_source = |s: &str| dtos.iter().find(|d| d.source == s).unwrap();

        let contract = by_source("Globex docs/2026/Globex_contrato.json");
        assert_eq!(contract.status, "ready");
        assert_eq!(
            contract.segments,
            ["CLIENT docs.redacted", "2026", "CLIENT_contrato.json"]
        );
        assert_eq!(by_source("Globex docs/foto.png").status, "skipped");
        assert_eq!(by_source("Globex docs/.git").status, "skipped");
        assert_eq!(
            by_source("Globex docs/notas.txt").segments,
            ["CLIENT docs.redacted", "notas.txt"]
        );

        files.save();
        let out = dir.join("CLIENT docs.redacted");
        let contract = fs::read_to_string(out.join("2026").join("CLIENT_contrato.json")).unwrap();
        assert!(
            !contract.contains("Globex") && contract.contains("1200"),
            "{contract}"
        );
        assert_eq!(
            fs::read_to_string(out.join("notas.txt")).unwrap(),
            "Reunión con [CLIENT] el martes"
        );
        assert!(
            !out.join("foto.png").exists(),
            "skipped files are not copied"
        );
        assert!(!out.join(".git").exists());
        // The original folder is as it was.
        assert!(src.join("2026").join("Globex_contrato.json").exists());
        assert_eq!(
            fs::read_to_string(src.join("notas.txt")).unwrap(),
            "Reunión con Globex el martes"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn outputs_with_the_same_name_get_numbered() {
        let dir = temp_dir("collide");
        let src = dir.join("docs");
        write(&src.join("Globex.txt"), "a");
        write(&src.join("CLIENT.txt"), "b");
        let mut files = Files::default();
        files.add([src], &redactor());
        let mut names: Vec<_> = files
            .dto()
            .into_iter()
            .map(|d| d.segments[1].clone())
            .collect();
        names.sort();
        assert_eq!(names, ["CLIENT (2).txt", "CLIENT.txt"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_binary_and_non_utf8_files() {
        let dir = temp_dir("binary");
        let png = dir.join("shot.png");
        write(&png, b"\x89PNG\r\n\x1a\n\0\0\0");
        let latin1 = dir.join("notes.txt");
        write(&latin1, b"caf\xe9");

        let mut files = Files::default();
        files.add([png.clone(), latin1.clone()], &redactor());
        let dtos = files.dto();
        assert_eq!(dtos[0].status, "skipped");
        assert_eq!(dtos[1].status, "skipped");
        assert!(files.save().is_empty());
        assert_eq!(fs::read(&png).unwrap(), b"\x89PNG\r\n\x1a\n\0\0\0");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn redacts_workbooks_and_csv_as_tables() {
        let dir = temp_dir("tables");
        let book = dir.join("Pagos.xlsx");
        let report: &[u8] =
            include_bytes!("../../../../crates/redactor-core/tests/xlsx/report.xlsx");
        write(&book, report);
        let csv = dir.join("terceros.csv");
        write(
            &csv,
            "Nombre;Cédula;Valor\nMaría Gómez;1020345678;1.250.000\n",
        );
        let old = dir.join("viejo.xls");
        write(&old, b"\xd0\xcf\x11\xe0");

        let mut files = Files::default();
        files.add([book.clone(), csv.clone(), old], &redactor());
        let dtos = files.dto();
        assert_eq!(
            (dtos[0].status, dtos[0].format.as_deref()),
            ("ready", Some("xlsx")),
            "{:?}",
            dtos[0].message
        );
        assert!(dtos[0].has_text);
        assert!(!dtos[0].preview.as_deref().unwrap().contains("María"));
        assert_eq!(
            (dtos[1].status, dtos[1].format.as_deref()),
            ("ready", Some("csv"))
        );
        assert_eq!(dtos[2].status, "skipped");
        assert!(files.text(dtos[0].id).unwrap().contains("| Fecha |"));

        files.save();
        assert_eq!(fs::read(&book).unwrap(), report, "original untouched");
        assert!(
            fs::read(dir.join("Pagos.redacted.xlsx"))
                .unwrap()
                .starts_with(b"PK")
        );
        let csv_copy = fs::read_to_string(dir.join("terceros.redacted.csv")).unwrap();
        assert!(
            !csv_copy.contains("María") && !csv_copy.contains("1020345678"),
            "{csv_copy}"
        );
        assert!(csv_copy.contains("1.250.000"));
        assert!(
            files.text(dtos[0].id).is_some(),
            "text still there after saving"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn adding_the_same_file_again_updates_it() {
        let dir = temp_dir("again");
        let path = dir.join("notes.txt");
        write(&path, "Globex");
        let mut files = Files::default();
        files.add([path.clone()], &redactor());
        files.add([path.clone()], &redactor());
        assert_eq!(files.dto().len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn names_copies_safely() {
        let r = redactor();
        assert_eq!(copy_name(&r, "b.json"), "b.redacted.json");
        assert_eq!(copy_name(&r, "notes"), "notes.redacted");
        assert_eq!(copy_name(&r, "b.redacted.json"), "b.redacted.redacted.json");
        assert_eq!(
            clean_name(&r, "Globex_contrato.json"),
            "CLIENT_contrato.json"
        );
        assert_eq!(clean_name(&r, "script.py"), "script.py");
        assert_eq!(numbered("pagos.xlsx", 2), "pagos (2).xlsx");
        assert_eq!(numbered("README", 3), "README (3)");
        let users = Redactor::new(&Config {
            users: vec!["qa.tester01".into()],
            ..Default::default()
        });
        let name = clean_name(&users, "qa.tester01 notes.txt");
        assert!(!name.contains('*') && !name.contains("tester01"), "{name}");
    }
}
