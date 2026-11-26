//! Excel workbooks (`.xlsx`): a ZIP of XML parts.
//!
//! Text lives in a few places and every one of them is rewritten:
//!
//! - `xl/sharedStrings.xml`: most cell text, referenced by index from sheets;
//! - inline strings and cached formula strings inside each sheet;
//! - headers and footers, comments and their authors, threaded comments and
//!   the people who wrote them, pivot table caches (a full copy of the data),
//!   and text in charts and drawings;
//! - `docProps`: the author, who saved it last, the company.
//!
//! Columns are judged by their header ([`crate::table`]): a cell under a
//! person or document column is masked whole, even when Excel stores it as a
//! number (it becomes text). Every other cell goes through the regular
//! engine. Numbers, formulas, styles and sheet names stay as they are, so the
//! workbook keeps working. Parts that hold no text are copied byte for byte.

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read, Write};

use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer, XmlVersion};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::redactor::Redactor;
use crate::table::{ColumnKind, Tally, find_header};

/// Any single part larger than this when unpacked is refused (zip bombs).
const MAX_PART_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PARTS: usize = 10_000;
const XML: XmlVersion = XmlVersion::Explicit1_0;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct XlsxError(String);

fn err(e: impl std::fmt::Display) -> XlsxError {
    XlsxError(e.to_string())
}

/// A redacted workbook and the same content as text, for pasting into a chat.
#[derive(Debug)]
pub struct XlsxOutput {
    pub bytes: Vec<u8>,
    /// Every sheet as a Markdown table, in workbook order.
    pub text: String,
}

/// A sheet's redacted text by `(row, column)`, for the Markdown version.
type Grid = BTreeMap<(u32, u32), String>;

/// One cell, as read from a sheet.
#[derive(Debug, Clone)]
struct Cell {
    row: u32,
    col: u32,
    /// The `t` attribute: `s`, `inlineStr`, `str`, `b`, `e`, `n` or empty.
    kind: String,
    /// Value as stored: a shared string index, a number, or inline text.
    value: String,
    formula: bool,
}

impl Redactor {
    pub fn redact_xlsx(&self, input: &[u8], tally: &mut Tally) -> Result<XlsxOutput, XlsxError> {
        let mut zip = ZipArchive::new(Cursor::new(input)).map_err(|_| err("not an .xlsx file"))?;
        if zip.len() > MAX_PARTS {
            return Err(err("too many parts inside the workbook"));
        }
        let names: Vec<String> = (0..zip.len())
            .filter_map(|i| {
                zip.by_index_raw(i)
                    .ok()
                    .and_then(|f| f.name().ok().map(|n| n.into_owned()))
            })
            .collect();
        let read = |zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str| -> Result<String, XlsxError> {
            let mut file = zip.by_name(name).map_err(err)?;
            if file.size() > MAX_PART_BYTES {
                return Err(err(format!("{name} is too large")));
            }
            let mut text = String::new();
            file.read_to_string(&mut text).map_err(err)?;
            Ok(text)
        };
        if !names.iter().any(|n| n == "xl/workbook.xml") {
            return Err(err("not an Excel workbook"));
        }

        // Sheets, in workbook order, with the part that holds each one.
        let workbook = read(&mut zip, "xl/workbook.xml")?;
        let rels = read(&mut zip, "xl/_rels/workbook.xml.rels").unwrap_or_default();
        let sheets = sheet_parts(&workbook, &rels);

        let shared_xml = if names.iter().any(|n| n == "xl/sharedStrings.xml") {
            read(&mut zip, "xl/sharedStrings.xml")?
        } else {
            String::new()
        };
        let shared = shared_strings(&shared_xml)?;

        // Pass 1: find each sheet's header and the kind of every column, and
        // which shared strings sit under a person or document column.
        let mut layouts = Vec::new();
        let mut shared_kinds: HashMap<usize, ColumnKind> = HashMap::new();
        let mut people: Vec<String> = Vec::new();
        for (_, part) in &sheets {
            let xml = read(&mut zip, part).unwrap_or_default();
            let cells = read_cells(&xml)?;
            let layout = Layout::new(&cells, &shared);
            for cell in &cells {
                let Some(kind) = layout.kind(cell.row, cell.col) else {
                    continue;
                };
                if cell.kind == "s" {
                    if let Ok(i) = cell.value.trim().parse::<usize>() {
                        shared_kinds.entry(i).or_insert(kind);
                        people.extend(shared.get(i).cloned());
                    }
                } else if !cell.formula {
                    people.push(cell.value.clone());
                }
            }
            layouts.push(layout);
        }
        // Names and documents found in those columns are hidden everywhere
        // else too: notes, comments, other sheets.
        let r = &self.with_people(crate::table::people_terms(
            people.iter().map(String::as_str),
        ));

        // Pass 2: rewrite.
        let mut parts: BTreeMap<String, String> = BTreeMap::new();
        let (shared_out, shared_redacted) = if shared_xml.is_empty() {
            (String::new(), Vec::new())
        } else {
            rewrite_shared(r, &shared_xml, &shared_kinds, tally)?
        };
        if !shared_xml.is_empty() {
            parts.insert("xl/sharedStrings.xml".into(), shared_out);
        }

        let mut text = String::new();
        for ((name, part), layout) in sheets.iter().zip(&layouts) {
            let Ok(xml) = read(&mut zip, part) else {
                continue;
            };
            let (out, grid) = rewrite_sheet(r, &xml, layout, &shared_redacted, tally)?;
            parts.insert(part.clone(), out);
            text.push_str(&markdown(&r.redact(name).output, &grid));
        }

        for name in &names {
            if parts.contains_key(name) {
                continue;
            }
            let rewritten = match name.as_str() {
                "docProps/core.xml" => {
                    Some(rewrite_text(&read(&mut zip, name)?, |el, t| match el {
                        "creator" | "lastModifiedBy" => Some(String::new()),
                        "title" | "subject" | "description" | "keywords" | "category" => {
                            Some(r.redact_cell(t, None, tally))
                        }
                        _ => None,
                    })?)
                }
                "docProps/app.xml" => Some(rewrite_text(&read(&mut zip, name)?, |el, _| {
                    matches!(el, "Company" | "Manager").then(String::new)
                })?),
                n if n.starts_with("xl/comments") => {
                    Some(rewrite_text(&read(&mut zip, name)?, |el, t| match el {
                        "author" => Some("Author".into()),
                        "t" => Some(r.redact_cell(t, None, tally)),
                        _ => None,
                    })?)
                }
                n if n.starts_with("xl/threadedComments/") => {
                    Some(rewrite_text(&read(&mut zip, name)?, |el, t| {
                        (el == "text").then(|| r.redact_cell(t, None, tally))
                    })?)
                }
                n if n.starts_with("xl/persons/") => {
                    Some(rewrite_attrs(&read(&mut zip, name)?, |el, attr, _| {
                        (el == "person" && matches!(attr, "displayName" | "userId"))
                            .then(|| "Person".into())
                    })?)
                }
                n if n.starts_with("xl/pivotCache/") => {
                    Some(rewrite_pivot(r, &read(&mut zip, name)?, tally)?)
                }
                n if n.starts_with("xl/charts/") || n.starts_with("xl/drawings/") => {
                    if n.ends_with(".xml") {
                        Some(rewrite_text(&read(&mut zip, name)?, |el, t| {
                            matches!(el, "t" | "v").then(|| r.redact_cell(t, None, tally))
                        })?)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(xml) = rewritten {
                parts.insert(name.clone(), xml);
            }
        }

        // Sheets named after the client get a clean name, and every
        // reference to them (formulas, defined names, pivot sources, the
        // list of parts) follows.
        let renames = sheet_renames(r, &sheets);
        if !renames.is_empty() {
            for name in &names {
                if !(name.ends_with(".xml") || name.ends_with(".rels")) {
                    continue;
                }
                let xml = match parts.get(name) {
                    Some(xml) => xml.clone(),
                    None => read(&mut zip, name)?,
                };
                let renamed = rename_sheets(&xml, &renames);
                if renamed != xml {
                    parts.insert(name.clone(), renamed);
                }
            }
        }

        // Same parts, same order; untouched ones are copied as they were.
        let mut out = ZipWriter::new(Cursor::new(Vec::with_capacity(input.len())));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for i in 0..zip.len() {
            let file = zip.by_index_raw(i).map_err(err)?;
            let name = file.name().map_err(err)?.into_owned();
            match parts.get(&name) {
                Some(xml) => {
                    drop(file);
                    out.start_file(name, options).map_err(err)?;
                    out.write_all(xml.as_bytes()).map_err(err)?;
                }
                None => out.raw_copy_file(file).map_err(err)?,
            }
        }
        let bytes = out.finish().map_err(err)?.into_inner();
        Ok(XlsxOutput { bytes, text })
    }
}

/// Sheet names and their parts, from `workbook.xml` and its relationships.
fn sheet_parts(workbook: &str, rels: &str) -> Vec<(String, String)> {
    let mut targets = HashMap::new();
    for_each_start(rels, |name, attrs| {
        if name == "Relationship"
            && let (Some(id), Some(target)) = (attrs.get("Id"), attrs.get("Target"))
        {
            let path = match target.strip_prefix('/') {
                Some(abs) => abs.to_string(),
                None => format!("xl/{target}"),
            };
            targets.insert(id.clone(), path);
        }
    });
    let mut sheets = Vec::new();
    for_each_start(workbook, |name, attrs| {
        if name == "sheet"
            && let (Some(title), Some(id)) = (attrs.get("name"), attrs.get("id"))
            && let Some(part) = targets.get(id)
        {
            sheets.push((title.clone(), part.clone()));
        }
    });
    sheets
}

/// `(old, new)` names of sheets whose name holds a hidden value. Excel
/// forbids `[]:*?/\` in sheet names and caps them at 31 characters.
fn sheet_renames(r: &Redactor, sheets: &[(String, String)]) -> Vec<(String, String)> {
    let mut taken: Vec<String> = sheets.iter().map(|(n, _)| n.to_lowercase()).collect();
    let mut renames = Vec::new();
    for (name, _) in sheets {
        let redacted = r.redact(name).output;
        if &redacted == name {
            continue;
        }
        let clean: String = redacted
            .chars()
            .filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\'))
            .collect();
        let base: String = clean.trim().chars().take(31).collect();
        let mut new = if base.is_empty() {
            "Sheet".to_string()
        } else {
            base
        };
        let mut n = 2;
        while taken.contains(&new.to_lowercase()) {
            let suffix = format!(" ({n})");
            new = format!(
                "{}{suffix}",
                new.chars().take(31 - suffix.len()).collect::<String>()
            );
            n += 1;
        }
        taken.push(new.to_lowercase());
        renames.push((name.clone(), new));
    }
    renames
}

fn escape_xml(text: &str) -> String {
    quick_xml::escape::escape(text).into_owned()
}

/// Replaces a sheet's name where workbooks refer to it: its `name`, pivot
/// `sheet` sources, the part list in `app.xml`, and formulas (`'Old name'!A1`
/// or `Old!A1`).
fn rename_sheets(xml: &str, renames: &[(String, String)]) -> String {
    let mut out = xml.to_string();
    for (old, new) in renames {
        let (o, n) = (escape_xml(old), escape_xml(new));
        let quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
        let o_ref = if old.chars().all(|c| c.is_alphanumeric() || c == '_') {
            o.clone()
        } else {
            escape_xml(&quote(old))
        };
        let n_ref = escape_xml(&quote(new));
        out = out
            .replace(&format!("name=\"{o}\""), &format!("name=\"{n}\""))
            .replace(&format!("sheet=\"{o}\""), &format!("sheet=\"{n}\""))
            .replace(&format!(">{o}<"), &format!(">{n}<"))
            .replace(&format!("{o_ref}!"), &format!("{n_ref}!"));
    }
    out
}

/// Calls `f` with the local name and unescaped attributes (local names too)
/// of every start or empty element.
fn for_each_start(xml: &str, mut f: impl FnMut(&str, &HashMap<String, String>)) {
    let mut reader = Reader::from_str(xml);
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let attrs = e
                    .attributes()
                    .flatten()
                    .map(|a| {
                        let value = a
                            .normalized_value(XML)
                            .map(|v| v.into_owned())
                            .unwrap_or_default();
                        (local(a.key.0).to_string(), value)
                    })
                    .collect();
                f(local(e.name().0), &attrs);
            }
            Event::Eof => break,
            _ => {}
        }
    }
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Text of each shared string, runs joined.
fn shared_strings(xml: &str) -> Result<Vec<String>, XlsxError> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut in_t = false;
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event().map_err(err)? {
            Event::Start(e) => match local(e.name().0) {
                "si" => current = Some(String::new()),
                "t" => in_t = true,
                _ => {}
            },
            Event::Empty(e) if local(e.name().0) == "si" => out.push(String::new()),
            Event::Text(t) if in_t => {
                if let Some(s) = &mut current {
                    s.push_str(&t);
                }
            }
            Event::GeneralRef(r) if in_t => {
                if let Some(s) = &mut current {
                    push_ref(s, &r);
                }
            }
            Event::End(e) => match local(e.name().0) {
                "t" => in_t = false,
                "si" => out.push(current.take().unwrap_or_default()),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

fn push_ref(text: &mut String, r: &quick_xml::events::BytesRef) {
    if let Ok(Some(c)) = r.resolve_char_ref() {
        text.push(c);
        return;
    }
    text.push(match &**r {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => return,
    });
}

/// `B12` → column 2 (1-based).
fn column_of(reference: &str) -> Option<u32> {
    let letters: String = reference
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    if letters.is_empty() {
        return None;
    }
    Some(letters.bytes().fold(0, |n, b| {
        n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
    }))
}

fn row_of(reference: &str) -> Option<u32> {
    reference
        .trim_start_matches(|c: char| c.is_ascii_alphabetic())
        .parse()
        .ok()
}

fn read_cells(xml: &str) -> Result<Vec<Cell>, XlsxError> {
    let mut cells = Vec::new();
    let mut reader = Reader::from_str(xml);
    let (mut row, mut col) = (0u32, 0u32);
    let mut cell: Option<Cell> = None;
    let mut capture = false;
    loop {
        match reader.read_event().map_err(err)? {
            Event::Start(e) | Event::Empty(e) if local(e.name().0) == "row" => {
                row = attr(&e, "r")
                    .and_then(|r| r.parse().ok())
                    .unwrap_or(row + 1);
                col = 0;
            }
            Event::Start(e) if local(e.name().0) == "c" => {
                let reference = attr(&e, "r");
                col = reference.as_deref().and_then(column_of).unwrap_or(col + 1);
                row = reference.as_deref().and_then(row_of).unwrap_or(row);
                cell = Some(Cell {
                    row,
                    col,
                    kind: attr(&e, "t").unwrap_or_default(),
                    value: String::new(),
                    formula: false,
                });
            }
            Event::Empty(e) if local(e.name().0) == "c" => {
                col = attr(&e, "r")
                    .as_deref()
                    .and_then(column_of)
                    .unwrap_or(col + 1);
            }
            Event::Start(e) | Event::Empty(e) if local(e.name().0) == "f" => {
                if let Some(c) = &mut cell {
                    c.formula = true;
                }
            }
            Event::Start(e) if matches!(local(e.name().0), "v" | "t") => capture = cell.is_some(),
            Event::Text(t) if capture => {
                if let Some(c) = &mut cell {
                    c.value.push_str(&t);
                }
            }
            Event::GeneralRef(r) if capture => {
                if let Some(c) = &mut cell {
                    push_ref(&mut c.value, &r);
                }
            }
            Event::End(e) => match local(e.name().0) {
                "v" | "t" => capture = false,
                "c" => cells.extend(cell.take()),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(cells)
}

fn attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.0 == name)
        .and_then(|a| a.normalized_value(XML).ok().map(|v| v.into_owned()))
}

/// Where a sheet's header is and what each column holds.
#[derive(Debug, Default)]
struct Layout {
    header_row: u32,
    kinds: HashMap<u32, ColumnKind>,
}

impl Layout {
    fn new(cells: &[Cell], shared: &[String]) -> Self {
        // The top of the sheet as text, for the header search.
        let mut top: BTreeMap<u32, BTreeMap<u32, String>> = BTreeMap::new();
        for c in cells {
            let first_rows = top.len() < 20 || top.contains_key(&c.row);
            if first_rows {
                let value = match c.kind.as_str() {
                    "s" => c
                        .value
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| shared.get(i))
                        .cloned()
                        .unwrap_or_default(),
                    _ => c.value.clone(),
                };
                top.entry(c.row).or_default().insert(c.col, value);
            }
        }
        let rows: Vec<(u32, Vec<String>)> = top
            .into_iter()
            .map(|(r, cols)| {
                let width = cols.keys().max().copied().unwrap_or(0) as usize;
                let mut row = vec![String::new(); width];
                for (c, v) in cols {
                    row[c as usize - 1] = v;
                }
                (r, row)
            })
            .collect();
        let grid: Vec<Vec<String>> = rows.iter().map(|(_, r)| r.clone()).collect();
        let Some((i, kinds)) = find_header(&grid) else {
            return Self::default();
        };
        Self {
            header_row: rows[i].0,
            kinds: kinds
                .into_iter()
                .enumerate()
                .filter_map(|(c, k)| k.map(|k| (c as u32 + 1, k)))
                .collect(),
        }
    }

    fn kind(&self, row: u32, col: u32) -> Option<ColumnKind> {
        if row > self.header_row {
            self.kinds.get(&col).copied()
        } else {
            None
        }
    }
}

/// Rewrites every `<t>` of the shared strings. Returns the new part and the
/// redacted text of each string.
fn rewrite_shared(
    r: &Redactor,
    xml: &str,
    kinds: &HashMap<usize, ColumnKind>,
    tally: &mut Tally,
) -> Result<(String, Vec<String>), XlsxError> {
    let mut index = 0usize;
    let mut texts: Vec<String> = Vec::new();
    let mut current = String::new();
    let out = rewrite(xml, |event, stack, text| match event {
        Step::Text => {
            if stack.last().map(String::as_str) == Some("t") && stack.iter().any(|s| s == "si") {
                let redacted = r.redact_cell(text, kinds.get(&index).copied(), tally);
                current.push_str(&redacted);
                Some(redacted)
            } else {
                None
            }
        }
        Step::End("si") => {
            texts.push(std::mem::take(&mut current));
            index += 1;
            None
        }
        Step::Empty("si") => {
            texts.push(String::new());
            index += 1;
            None
        }
        _ => None,
    })?;
    Ok((out, texts))
}

enum Step<'a> {
    Text,
    End(&'a str),
    Empty(&'a str),
}

/// Streams `xml` through unchanged, except text nodes `f` returns a
/// replacement for. `f` also sees element ends and empty elements, with the
/// stack of open element local names.
fn rewrite(
    xml: &str,
    mut f: impl FnMut(Step, &[String], &str) -> Option<String>,
) -> Result<String, XlsxError> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut stack: Vec<String> = Vec::new();
    let mut text = String::new();
    // The text events as read, written back untouched unless `f` changes
    // the text: re-escaping could turn the line break after the XML
    // declaration into `&#13;`, which is invalid outside the root element.
    let mut raw: Vec<Event<'static>> = Vec::new();
    loop {
        let event = reader.read_event().map_err(err)?;
        match &event {
            Event::Text(t) => text.push_str(t),
            Event::CData(t) => text.push_str(t),
            Event::GeneralRef(r) => push_ref(&mut text, r),
            _ => {}
        }
        if matches!(
            event,
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_)
        ) {
            raw.push(event.into_owned());
            continue;
        }
        if !raw.is_empty() {
            match f(Step::Text, &stack, &text) {
                Some(new) if new != text => {
                    writer
                        .write_event(Event::Text(BytesText::new(&new)))
                        .map_err(err)?;
                }
                _ => {
                    for e in raw.drain(..) {
                        writer.write_event(e).map_err(err)?;
                    }
                }
            }
            raw.clear();
            text.clear();
        }
        match &event {
            Event::Start(e) => stack.push(local(e.name().0).to_string()),
            Event::End(e) => {
                f(Step::End(local(e.name().0)), &stack, "");
                stack.pop();
            }
            Event::Empty(e) => {
                f(Step::Empty(local(e.name().0)), &stack, "");
            }
            Event::Eof => break,
            _ => {}
        }
        writer.write_event(event).map_err(err)?;
    }
    String::from_utf8(writer.into_inner()).map_err(err)
}

/// Rewrites the text of elements by local name.
fn rewrite_text(
    xml: &str,
    mut f: impl FnMut(&str, &str) -> Option<String>,
) -> Result<String, XlsxError> {
    rewrite(xml, |step, stack, text| match step {
        Step::Text => stack.last().and_then(|el| f(el, text)),
        _ => None,
    })
}

/// Rewrites attribute values: `f(element, attribute, value)`.
fn rewrite_attrs(
    xml: &str,
    mut f: impl FnMut(&str, &str, &str) -> Option<String>,
) -> Result<String, XlsxError> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    loop {
        let event = reader.read_event().map_err(err)?;
        let event = match event {
            Event::Start(e) => Event::Start(map_attrs(&e, &mut f)),
            Event::Empty(e) => Event::Empty(map_attrs(&e, &mut f)),
            Event::Eof => break,
            other => other,
        };
        writer.write_event(event).map_err(err)?;
    }
    String::from_utf8(writer.into_inner()).map_err(err)
}

fn map_attrs(
    e: &BytesStart,
    f: &mut impl FnMut(&str, &str, &str) -> Option<String>,
) -> BytesStart<'static> {
    let name = e.name().0.to_string();
    let el = local(&name).to_string();
    let mut out = BytesStart::new(name);
    for a in e.attributes().flatten() {
        let key = a.key.0.to_string();
        let value = a
            .normalized_value(XML)
            .map(|v| v.into_owned())
            .unwrap_or_default();
        match f(&el, local(&key), &value) {
            Some(new) => out.push_attribute((key.as_str(), new.as_str())),
            None => out.push_attribute(a),
        }
    }
    out.into_owned()
}

/// Pivot caches: `<s v="…">` items, masked whole under a person or document
/// field (judged by the field's name), redacted like any cell otherwise.
fn rewrite_pivot(r: &Redactor, xml: &str, tally: &mut Tally) -> Result<String, XlsxError> {
    let mut field: Option<ColumnKind> = None;
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    loop {
        let event = reader.read_event().map_err(err)?;
        let event = match event {
            Event::Start(e) if local(e.name().0) == "cacheField" => {
                field = attr(&e, "name")
                    .as_deref()
                    .and_then(crate::table::column_kind);
                Event::Start(e)
            }
            Event::Empty(e) if local(e.name().0) == "cacheField" => {
                field = attr(&e, "name")
                    .as_deref()
                    .and_then(crate::table::column_kind);
                Event::Empty(e)
            }
            Event::Start(e) if local(e.name().0) == "s" => {
                Event::Start(map_attrs(&e, &mut |_, a, v| {
                    (a == "v").then(|| r.redact_cell(v, field, tally))
                }))
            }
            Event::Empty(e) if local(e.name().0) == "s" => {
                Event::Empty(map_attrs(&e, &mut |_, a, v| {
                    (a == "v").then(|| r.redact_cell(v, field, tally))
                }))
            }
            Event::Start(e) if local(e.name().0) == "r" => {
                // A record row: inline items are not tied to a field here.
                field = None;
                Event::Start(e)
            }
            Event::Eof => break,
            other => other,
        };
        writer.write_event(event).map_err(err)?;
    }
    String::from_utf8(writer.into_inner()).map_err(err)
}

/// Rewrites one sheet. Returns the new part and the redacted grid
/// `(row, col) → text` for the Markdown version.
fn rewrite_sheet(
    r: &Redactor,
    xml: &str,
    layout: &Layout,
    shared: &[String],
    tally: &mut Tally,
) -> Result<(String, Grid), XlsxError> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut grid = BTreeMap::new();
    let (mut row, mut col) = (0u32, 0u32);
    // Events of the cell being read, written once it is complete.
    let mut cell: Option<Vec<Event<'static>>> = None;
    let mut text_in: Option<String> = None;
    let mut text = String::new();
    // Text events as read; see `rewrite`.
    let mut raw: Vec<Event<'static>> = Vec::new();
    loop {
        let event = reader.read_event().map_err(err)?.into_owned();
        match &event {
            Event::Text(t) => text.push_str(t),
            Event::GeneralRef(ref_) => push_ref(&mut text, ref_),
            _ => {}
        }
        if matches!(event, Event::Text(_) | Event::GeneralRef(_)) {
            raw.push(event);
            continue;
        }
        if !raw.is_empty() {
            let content = std::mem::take(&mut text);
            let new = match text_in.as_deref() {
                // Headers and footers hold free text.
                Some(el)
                    if el.starts_with("odd")
                        || el.starts_with("even")
                        || el.starts_with("first") =>
                {
                    r.redact_cell(&content, None, tally)
                }
                _ => content.clone(),
            };
            match &mut cell {
                // Inside a cell, one text event per value; rewrite_cell
                // reads and replaces it.
                Some(events) => {
                    events.push(Event::Text(BytesText::new(&new).into_owned()));
                    raw.clear();
                }
                None if new != content => {
                    writer
                        .write_event(Event::Text(BytesText::new(&new)))
                        .map_err(err)?;
                    raw.clear();
                }
                None => {
                    for e in raw.drain(..) {
                        writer.write_event(e).map_err(err)?;
                    }
                }
            }
        }
        match &event {
            Event::Start(e) | Event::Empty(e) if local(e.name().0) == "row" => {
                row = attr(e, "r").and_then(|v| v.parse().ok()).unwrap_or(row + 1);
                col = 0;
            }
            Event::Start(e) if local(e.name().0) == "c" => {
                let reference = attr(e, "r");
                col = reference.as_deref().and_then(column_of).unwrap_or(col + 1);
                row = reference.as_deref().and_then(row_of).unwrap_or(row);
                cell = Some(vec![event]);
                continue;
            }
            Event::Empty(e) if local(e.name().0) == "c" => {
                col = attr(e, "r")
                    .as_deref()
                    .and_then(column_of)
                    .unwrap_or(col + 1);
            }
            Event::End(e) if local(e.name().0) == "c" => {
                if let Some(mut events) = cell.take() {
                    events.push(event);
                    let (events, shown) =
                        rewrite_cell(r, events, layout.kind(row, col), shared, tally);
                    if let Some(shown) = shown {
                        grid.insert((row, col), shown);
                    }
                    for e in events {
                        writer.write_event(e).map_err(err)?;
                    }
                }
                continue;
            }
            Event::Start(e) => text_in = Some(local(e.name().0).to_string()),
            Event::End(_) => text_in = None,
            Event::Eof => break,
            _ => {}
        }
        match &mut cell {
            Some(events) => events.push(event),
            None => writer.write_event(event).map_err(err)?,
        }
    }
    let out = String::from_utf8(writer.into_inner()).map_err(err)?;
    Ok((out, grid))
}

/// One `<c>` element, start to end. Returns its new events and the text it
/// shows, for the Markdown version.
fn rewrite_cell(
    r: &Redactor,
    events: Vec<Event<'static>>,
    kind: Option<ColumnKind>,
    shared: &[String],
    tally: &mut Tally,
) -> (Vec<Event<'static>>, Option<String>) {
    let Some(Event::Start(start)) = events.first() else {
        return (events, None);
    };
    let t = attr(start, "t").unwrap_or_default();
    let has_formula = events
        .iter()
        .any(|e| matches!(e, Event::Start(s) | Event::Empty(s) if local(s.name().0) == "f"));
    let value_of = |el: &str| -> Option<String> {
        let mut inside = false;
        let mut out: Option<String> = None;
        for e in &events {
            match e {
                Event::Start(s) if local(s.name().0) == el => inside = true,
                Event::End(s) if local(s.name().0) == el => inside = false,
                Event::Text(x) if inside => out
                    .get_or_insert_default()
                    .push_str(&x.xml_content(Default::default())),
                _ => {}
            }
        }
        out
    };

    match t.as_str() {
        "s" => {
            let shown = value_of("v")
                .and_then(|i| i.trim().parse::<usize>().ok())
                .and_then(|i| shared.get(i).cloned());
            (events, shown)
        }
        "inlineStr" | "str" => {
            // Inline text sits in <is><t>, a cached formula string in <v>.
            let el = if t == "str" { "v" } else { "t" };
            let mut inside = false;
            let mut shown = String::new();
            let events = events
                .into_iter()
                .map(|e| match e {
                    Event::Start(s) if local(s.name().0) == el => {
                        inside = true;
                        Event::Start(s)
                    }
                    Event::End(s) if local(s.name().0) == el => {
                        inside = false;
                        Event::End(s)
                    }
                    Event::Text(x) if inside => {
                        let value = x.xml_content(Default::default()).into_owned();
                        let new = r.redact_cell(&value, kind, tally);
                        shown.push_str(&new);
                        Event::Text(BytesText::new(&new).into_owned())
                    }
                    other => other,
                })
                .collect();
            (events, Some(shown))
        }
        "" | "n" if kind.is_some() && !has_formula => {
            // A document or person stored as a number: becomes masked text.
            let Some(value) = value_of("v") else {
                return (events, None);
            };
            let masked = r.redact_cell(&value, kind, tally);
            let mut c = BytesStart::new("c");
            for a in start.attributes().flatten() {
                if a.key.0 != "t" {
                    c.push_attribute(a);
                }
            }
            c.push_attribute(("t", "inlineStr"));
            let out = vec![
                Event::Start(c.into_owned()),
                Event::Start(BytesStart::new("is")),
                Event::Start(BytesStart::new("t")),
                Event::Text(BytesText::new(&masked).into_owned()),
                Event::End(quick_xml::events::BytesEnd::new("t")),
                Event::End(quick_xml::events::BytesEnd::new("is")),
                Event::End(quick_xml::events::BytesEnd::new("c")),
            ];
            (out, Some(masked))
        }
        _ => {
            let shown = value_of("v");
            (events, shown)
        }
    }
}

/// A sheet as a Markdown table: rows and columns that hold something.
fn markdown(name: &str, grid: &Grid) -> String {
    let mut out = format!("## {name}\n\n");
    if grid.is_empty() {
        out.push_str("(empty)\n\n");
        return out;
    }
    let cols: Vec<u32> = {
        let mut c: Vec<u32> = grid.keys().map(|(_, c)| *c).collect();
        c.sort_unstable();
        c.dedup();
        c
    };
    let mut rows: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for ((r, c), v) in grid {
        let i = cols.binary_search(c).unwrap_or(0);
        let row = rows
            .entry(*r)
            .or_insert_with(|| vec![String::new(); cols.len()]);
        row[i] = v.replace('|', "\\|").replace(['\r', '\n'], " ");
    }
    for (n, row) in rows.values().enumerate() {
        out.push_str("| ");
        out.push_str(&row.join(" | "));
        out.push_str(" |\n");
        if n == 0 {
            out.push('|');
            out.push_str(&" --- |".repeat(cols.len()));
            out.push('\n');
        }
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Excel writes a line break after the declaration; written back as
    /// `&#13;` it makes the part invalid and Excel refuses the workbook.
    #[test]
    fn keeps_text_outside_elements_as_it_was() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>
<a><t>Tom &amp; Jerry</t></a>";
        let out = rewrite_text(xml, |_, _| None).unwrap();
        assert_eq!(out, xml);
        let changed =
            rewrite_text(xml, |el, t| (el == "t").then(|| t.replace("Tom", "T**"))).unwrap();
        assert!(changed.starts_with(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>
<a>"
        ));
        assert!(changed.contains("<t>T** &amp; Jerry</t>"), "{changed}");
    }

    #[test]
    fn reads_cell_references() {
        assert_eq!(column_of("A1"), Some(1));
        assert_eq!(column_of("AB12"), Some(28));
        assert_eq!(row_of("AB12"), Some(12));
    }
}
