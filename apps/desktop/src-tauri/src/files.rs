//! Redacting files: Postman collections, JSON, TXT, logs, HAR...
//!
//! Files come from the native picker or are dropped on the dashboard window;
//! either way the paths are received here and never come from the webview,
//! which only sees names and results. A file is redacted in memory first and
//! written, when the user saves, as a new file next to the original:
//! `collection.json` -> `collection.redacted.json`. The original is never
//! written to.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use redactor_core::{Category, InputFormat, Redactor};
use serde::Serialize;

use tauri::{AppHandle, DragDropEvent, Emitter, Manager};

use crate::flow::DASHBOARD;
use crate::state::AppState;
use crate::vault::Pair;

/// Larger files are refused: this is for collections and exports, not dumps.
const MAX_BYTES: u64 = 20 * 1024 * 1024;
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
    path: PathBuf,
    state: ItemState,
}

#[derive(Debug)]
enum ItemState {
    Ready(Box<Redacted>),
    Rejected(String),
    Saved { values: usize, name: String },
    Failed(String),
}

#[derive(Debug)]
struct Redacted {
    /// The new file content, BOM included if the original had one.
    output: String,
    format: InputFormat,
    categories: Vec<(Category, usize)>,
    values: usize,
    pairs: Vec<Pair>,
}

#[derive(Debug, Serialize)]
pub struct FileDto {
    pub id: u32,
    pub name: String,
    pub folder: String,
    /// `ready`, `rejected`, `saved` or `failed`.
    pub status: &'static str,
    pub message: Option<String>,
    /// Name of the redacted copy: where it will go, or where it went.
    pub output_name: String,
    pub format: Option<InputFormat>,
    pub values: usize,
    pub categories: Vec<(Category, usize)>,
    pub preview: Option<String>,
}

impl Files {
    /// Reads and redacts each path. A path already in the list is redacted
    /// again, so changed rules apply.
    pub fn add(&mut self, paths: impl IntoIterator<Item = PathBuf>, redactor: &Redactor) {
        for path in paths {
            if path.is_dir() {
                continue;
            }
            let state = match redact(&path, redactor) {
                Ok(redacted) => ItemState::Ready(Box::new(redacted)),
                Err(reason) => ItemState::Rejected(reason),
            };
            if let Some(item) = self.items.iter_mut().find(|i| i.path == path) {
                item.state = state;
            } else {
                self.next_id += 1;
                self.items.push(Item {
                    id: self.next_id,
                    path,
                    state,
                });
            }
        }
    }

    pub fn remove(&mut self, id: u32) {
        self.items.retain(|i| i.id != id);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Writes the redacted copy of every ready file. Returns the values to
    /// remember in the vault.
    pub fn save(&mut self) -> Vec<Pair> {
        let mut pairs = Vec::new();
        for item in &mut self.items {
            let ItemState::Ready(redacted) = &mut item.state else {
                continue;
            };
            let target = redacted_path(&item.path);
            item.state = match write_new(&target, &redacted.output) {
                Ok(()) => {
                    pairs.append(&mut redacted.pairs);
                    crate::log::info(format_args!(
                        "saved a redacted copy ({} values)",
                        redacted.values
                    ));
                    ItemState::Saved {
                        values: redacted.values,
                        name: file_name(&target),
                    }
                }
                Err(e) => ItemState::Failed(e),
            };
        }
        pairs
    }

    pub fn dto(&self) -> Vec<FileDto> {
        self.items.iter().map(Item::dto).collect()
    }
}

impl Item {
    fn dto(&self) -> FileDto {
        let name = file_name(&self.path);
        let folder = self
            .path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let mut dto = FileDto {
            id: self.id,
            name,
            folder,
            status: "ready",
            message: None,
            output_name: file_name(&redacted_path(&self.path)),
            format: None,
            values: 0,
            categories: Vec::new(),
            preview: None,
        };
        match &self.state {
            ItemState::Ready(r) => {
                dto.format = Some(r.format);
                dto.values = r.values;
                dto.categories = r.categories.clone();
                let body = r.output.strip_prefix(BOM).unwrap_or(&r.output);
                dto.preview = Some(body.chars().take(PREVIEW_CHARS).collect());
            }
            ItemState::Rejected(reason) => {
                dto.status = "rejected";
                dto.message = Some(reason.clone());
            }
            ItemState::Saved { values, name } => {
                dto.status = "saved";
                dto.values = *values;
                dto.output_name = name.clone();
            }
            ItemState::Failed(e) => {
                dto.status = "failed";
                dto.message = Some(e.clone());
            }
        }
        dto
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
    let bytes = fs::read(path).map_err(|e| format!("cannot read it: {e}"))?;
    let text = text_of(bytes)?;
    let (bom, body) = match text.strip_prefix(BOM) {
        Some(body) => (true, body),
        None => (false, text.as_str()),
    };

    let redaction = redactor.redact(body);
    let mut counts: BTreeMap<Category, usize> = BTreeMap::new();
    for f in &redaction.findings {
        *counts.entry(f.category).or_default() += 1;
    }
    let pairs = redaction
        .findings
        .iter()
        .map(|f| Pair {
            replacement: f.replacement.clone(),
            original: redaction.original(f).to_string(),
        })
        .collect();

    // A collection or export that stops being valid JSON would be useless.
    if serde_json::from_str::<serde::de::IgnoredAny>(body).is_ok()
        && serde_json::from_str::<serde::de::IgnoredAny>(&redaction.output).is_err()
    {
        return Err("its redacted version is not valid JSON; it was left as it is".into());
    }

    let output = if bom {
        format!("{BOM}{}", redaction.output)
    } else {
        redaction.output
    };
    Ok(Redacted {
        output,
        format: redaction.format,
        values: redaction.findings.len(),
        categories: counts.into_iter().collect(),
        pairs,
    })
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

/// `collection.json` -> `collection.redacted.json`, `notes` -> `notes.redacted`.
/// Never the file itself, even when it is already a redacted copy.
pub fn redacted_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(ext) => format!("{stem}.redacted.{}", ext.to_string_lossy()),
        None => format!("{stem}.redacted"),
    };
    path.with_file_name(name)
}

/// Writes to a temporary file next to the target, then renames it over the
/// target, so a crash never leaves a half-written copy behind. An earlier
/// redacted copy is replaced; the original file is never the target.
fn write_new(path: &Path, content: &str) -> Result<(), String> {
    let dir = path.parent().ok_or("no parent folder")?;
    let name = path.file_name().ok_or("no file name")?.to_string_lossy();
    let tmp = dir.join(format!(".{name}.redactor-tmp"));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
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

    #[test]
    fn saves_a_redacted_copy_next_to_the_original() {
        let dir = temp_dir("copy");
        let path = dir.join("Globex API.postman_collection.json");
        let original = "\u{feff}{\r\n  \"url\": \"https://api.globex.example/v1\",\r\n  \"token\": \"a7f3e9c1d5b2084f6e1a9c3d7b5f2e80\"\r\n}\r\n";
        fs::write(&path, original).unwrap();

        let mut files = Files::default();
        files.add([path.clone()], &redactor());
        let dto = &files.dto()[0];
        assert_eq!(dto.status, "ready", "{:?}", dto.message);
        assert_eq!(
            dto.output_name,
            "Globex API.postman_collection.redacted.json"
        );

        let pairs = files.save();
        assert!(!pairs.is_empty());
        // The original is untouched; the copy keeps BOM and line endings.
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let copy =
            fs::read_to_string(dir.join("Globex API.postman_collection.redacted.json")).unwrap();
        assert!(copy.starts_with('\u{feff}'));
        assert!(copy.contains("\r\n"));
        assert!(!copy.to_lowercase().contains("globex"), "{copy}");
        assert!(!copy.contains("a7f3e9c1d5b2084f6e1a9c3d7b5f2e80"));
        assert!(
            serde_json::from_str::<serde_json::Value>(copy.trim_start_matches('\u{feff}')).is_ok()
        );
        assert_eq!(files.dto()[0].status, "saved");
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            2,
            "original and copy, no temp file"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn names_the_copy() {
        assert_eq!(
            redacted_path(Path::new("a/b.json")),
            Path::new("a/b.redacted.json")
        );
        assert_eq!(
            redacted_path(Path::new("a/notes")),
            Path::new("a/notes.redacted")
        );
        assert_eq!(
            redacted_path(Path::new("a/b.redacted.json")),
            Path::new("a/b.redacted.redacted.json")
        );
    }

    #[test]
    fn refuses_binary_and_non_utf8_files() {
        let dir = temp_dir("binary");
        let png = dir.join("shot.png");
        fs::write(&png, b"\x89PNG\r\n\x1a\n\0\0\0").unwrap();
        let latin1 = dir.join("notes.txt");
        fs::write(&latin1, b"caf\xe9").unwrap();

        let mut files = Files::default();
        files.add([png.clone(), latin1.clone()], &redactor());
        let dtos = files.dto();
        assert_eq!(dtos[0].status, "rejected");
        assert_eq!(dtos[1].status, "rejected");
        assert!(files.save().is_empty());
        assert_eq!(fs::read(&png).unwrap(), b"\x89PNG\r\n\x1a\n\0\0\0");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn adding_the_same_file_again_updates_it() {
        let dir = temp_dir("again");
        let path = dir.join("notes.txt");
        fs::write(&path, "Globex").unwrap();
        let mut files = Files::default();
        files.add([path.clone()], &redactor());
        files.add([path.clone()], &redactor());
        assert_eq!(files.dto().len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
