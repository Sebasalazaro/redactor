//! Compares two Burp site map exports (for example admin and normal user)
//! and writes a redacted report for access control review.
//!
//! Exports are read item by item and reduced to a summary, so memory does not
//! grow with the size of the responses. Values never reach the report: it
//! holds methods, templated paths, parameter names, statuses, sizes and the
//! names of JSON fields, and every name goes through the [`Redactor`]. Bodies
//! are hashed in memory only to tell whether two responses match.
//!
//! This compares what each role already requested. It sends nothing, so an
//! endpoint seen by one role only proves nothing until it is replayed.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::io::BufRead;
use std::sync::LazyLock;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use quick_xml::Reader;
use quick_xml::events::Event;
use regex::Regex;
use serde::Serialize;
use thiserror::Error;

use crate::Redactor;

/// Larger bodies are hashed as they are, without parsing or normalizing them.
const MAX_PARSED_BODY: usize = 8 * 1024 * 1024;
/// Bounds per endpoint, so a huge export cannot grow the summary much.
const MAX_HASHES: usize = 64;
const MAX_KEYS: usize = 2000;
const MAX_KEY_DEPTH: usize = 6;
/// Field names listed per row of the report.
const MAX_NOTE_KEYS: usize = 12;
/// `--include-bodies`: responses per file, and characters per response.
const MAX_SAMPLES: usize = 3;
const MAX_SAMPLE_CHARS: usize = 2048;

const STATIC_EXTENSIONS: &[&str] = &[
    "avif", "bmp", "css", "eot", "gif", "ico", "jpeg", "jpg", "js", "map", "mjs", "mp4", "otf",
    "png", "svg", "ttf", "webm", "webp", "woff", "woff2",
];
const STATIC_MIME_TYPES: &[&str] = &[
    "css", "font", "gif", "image", "jpeg", "png", "script", "svg",
];

#[derive(Debug, Error)]
pub enum SitemapError {
    #[error("invalid XML: {0}")]
    Xml(String),
    #[error("not a Burp site map export (no <items> element)")]
    NotBurpExport,
}

/// One function of the application: requests that differ only in ids or
/// parameter values share an endpoint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Endpoint {
    pub origin: String,
    pub path: String,
    pub method: String,
    /// Sorted names of the query parameters, never their values.
    pub query: Vec<String>,
    /// GraphQL operation, since every operation shares one URL.
    pub operation: Option<String>,
}

impl Endpoint {
    /// The unredacted form that `--include-bodies` filters match against.
    pub fn label(&self) -> String {
        let mut label = format!("{} {}", self.method, self.path);
        if !self.query.is_empty() {
            let _ = write!(label, "?{}", self.query.join("&"));
        }
        if let Some(operation) = &self.operation {
            let _ = write!(label, " #{operation}");
        }
        label
    }
}

/// What one role saw of an endpoint.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub requests: usize,
    pub statuses: BTreeSet<u16>,
    pub min_size: usize,
    pub max_size: usize,
    pub keys: BTreeSet<String>,
    exact: HashSet<u64>,
    loose: HashSet<u64>,
    concrete: HashSet<u64>,
}

impl Stats {
    fn allowed(&self) -> bool {
        self.statuses.iter().any(|s| (200..300).contains(s))
    }
}

/// The summary of one export.
#[derive(Debug, Default)]
pub struct SiteMap {
    pub endpoints: BTreeMap<Endpoint, Stats>,
    /// Items Burp found but never requested, so they have no response.
    pub unrequested: usize,
    /// Images, styles, scripts and fonts, left out of the comparison.
    pub static_items: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// Both roles got a 2xx with the same content.
    SameContent,
    /// Both roles got a 2xx with different content.
    DifferentContent,
    /// The low role got a 2xx where the high role did not.
    Inverted,
    /// The high role got a 2xx and the low role did not.
    Enforced,
    /// Requested by the high role only.
    HighOnly,
    /// Requested by the low role only.
    LowOnly,
    /// Neither role got a 2xx.
    NoAccess,
}

const CLASSES: [Class; 7] = [
    Class::SameContent,
    Class::DifferentContent,
    Class::Inverted,
    Class::Enforced,
    Class::HighOnly,
    Class::LowOnly,
    Class::NoAccess,
];

#[derive(Debug)]
pub struct Row<'a> {
    pub endpoint: &'a Endpoint,
    pub class: Class,
    pub high: Option<&'a Stats>,
    pub low: Option<&'a Stats>,
    /// Same body byte for byte in at least one pair of responses.
    pub identical: bool,
    /// At least one concrete URL was requested by both roles.
    pub same_object: bool,
}

/// A redacted response body requested with `--include-bodies`.
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub role: String,
    pub endpoint: String,
    pub text: String,
}

/// A finished comparison. Every string in it is already redacted.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub high_label: String,
    pub low_label: String,
    pub left_out: LeftOut,
    pub sections: Vec<ReportSection>,
    pub samples: Vec<Sample>,
}

/// Items of each export that were not compared.
#[derive(Debug, Clone, Serialize)]
pub struct LeftOut {
    pub high_unrequested: usize,
    pub high_static: usize,
    pub low_unrequested: usize,
    pub low_static: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportSection {
    pub class: Class,
    pub title: String,
    pub hint: String,
    pub prefixes: Vec<(String, usize)>,
    pub rows: Vec<ReportRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportRow {
    pub origin: String,
    pub endpoint: String,
    pub high: Option<Side>,
    pub low: Option<Side>,
    pub notes: String,
}

/// What one role got from an endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct Side {
    pub statuses: String,
    pub size: String,
    pub requests: usize,
}

#[derive(Debug, Clone)]
pub struct ReportOptions {
    pub high_label: String,
    pub low_label: String,
    /// Rows per section; the rest is only counted.
    pub max_rows: usize,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            high_label: "admin".into(),
            low_label: "user".into(),
            max_rows: 150,
        }
    }
}

/// Reads an export into its summary.
pub fn read_map(input: impl BufRead) -> Result<SiteMap, SitemapError> {
    let mut map = SiteMap::default();
    for_each_item(input, |item| {
        let Some(status) = item.requested_status() else {
            map.unrequested += 1;
            return true;
        };
        if item.is_static() {
            map.static_items += 1;
            return true;
        }
        let endpoint = item.endpoint();
        let body = message_body(&item.response);
        let stats = map.endpoints.entry(endpoint).or_default();
        stats.min_size = if stats.requests == 0 {
            body.len()
        } else {
            stats.min_size.min(body.len())
        };
        stats.max_size = stats.max_size.max(body.len());
        stats.requests += 1;
        stats.statuses.insert(status);
        insert_bounded(&mut stats.concrete, hash_of(item.path.as_bytes()));
        if (200..300).contains(&status) {
            insert_bounded(&mut stats.exact, hash_of(body));
            insert_bounded(&mut stats.loose, loose_hash(body));
            collect_keys(body, &mut stats.keys);
        }
        true
    })?;
    Ok(map)
}

/// Finds responses of the endpoints whose label contains `filter` and redacts
/// them. They are redacted whole and cut afterwards, so that a cut can never
/// hide a secret from the detectors.
pub fn read_samples(
    input: impl BufRead,
    role: &str,
    filter: &str,
    redactor: &Redactor,
) -> Result<Vec<Sample>, SitemapError> {
    let mut samples = Vec::new();
    let names = Names::new(redactor);
    for_each_item(input, |item| {
        if item.requested_status().is_none() || item.is_static() {
            return true;
        }
        let endpoint = item.endpoint();
        if !endpoint.label().contains(filter) || item.response.contains(&0) {
            return true;
        }
        let response = String::from_utf8_lossy(&item.response);
        let redacted = redactor.redact(&response).output;
        samples.push(Sample {
            role: role.to_string(),
            endpoint: names.endpoint(&endpoint),
            text: redacted.chars().take(MAX_SAMPLE_CHARS).collect(),
        });
        samples.len() < MAX_SAMPLES
    })?;
    Ok(samples)
}

/// Pairs the endpoints of two site maps and classifies each of them.
pub fn compare<'a>(high: &'a SiteMap, low: &'a SiteMap) -> Vec<Row<'a>> {
    let endpoints: BTreeSet<&Endpoint> =
        high.endpoints.keys().chain(low.endpoints.keys()).collect();
    endpoints
        .into_iter()
        .map(|endpoint| {
            let (h, l) = (high.endpoints.get(endpoint), low.endpoints.get(endpoint));
            let pair = h.zip(l);
            let identical = pair.is_some_and(|(h, l)| !h.exact.is_disjoint(&l.exact));
            let similar = pair.is_some_and(|(h, l)| !h.loose.is_disjoint(&l.loose));
            let class = match pair {
                None if h.is_some() => Class::HighOnly,
                None => Class::LowOnly,
                Some((h, l)) => match (h.allowed(), l.allowed()) {
                    (true, true) if identical || similar => Class::SameContent,
                    (true, true) => Class::DifferentContent,
                    (true, false) => Class::Enforced,
                    (false, true) => Class::Inverted,
                    (false, false) => Class::NoAccess,
                },
            };
            Row {
                endpoint,
                class,
                high: h,
                low: l,
                identical,
                same_object: pair.is_some_and(|(h, l)| !h.concrete.is_disjoint(&l.concrete)),
            }
        })
        .collect()
}

/// Writes the Markdown report.
pub fn report(
    high: &SiteMap,
    low: &SiteMap,
    redactor: &Redactor,
    options: &ReportOptions,
    samples: &[Sample],
) -> String {
    build_report(high, low, redactor, options, samples).to_markdown(options.max_rows)
}

/// Classifies both site maps into a report. Everything in it is redacted.
pub fn build_report(
    high: &SiteMap,
    low: &SiteMap,
    redactor: &Redactor,
    options: &ReportOptions,
    samples: &[Sample],
) -> Report {
    let mut rows = compare(high, low);
    rows.sort_by(|a, b| (a.class, a.endpoint).cmp(&(b.class, b.endpoint)));
    let names = Names::new(redactor);
    let h = names.piece(&options.high_label);
    let l = names.piece(&options.low_label);

    let sections = CLASSES
        .into_iter()
        .map(|class| {
            let section: Vec<&Row> = rows.iter().filter(|r| r.class == class).collect();
            ReportSection {
                class,
                title: title(class, &h, &l),
                hint: hint(class, &h, &l),
                prefixes: prefixes(&section, &names),
                rows: section.iter().map(|row| report_row(row, &names)).collect(),
            }
        })
        .collect();

    Report {
        left_out: LeftOut {
            high_unrequested: high.unrequested,
            high_static: high.static_items,
            low_unrequested: low.unrequested,
            low_static: low.static_items,
        },
        sections,
        samples: samples
            .iter()
            .map(|sample| Sample {
                role: names.piece(&sample.role),
                ..sample.clone()
            })
            .collect(),
        high_label: h,
        low_label: l,
    }
}

impl Report {
    /// The report as Markdown, listing up to `max_rows` rows per section.
    pub fn to_markdown(&self, max_rows: usize) -> String {
        let (h, l) = (&self.high_label, &self.low_label);
        let mut out = format!("# Site map comparison: {h} vs {l}\n\n");
        out.push_str(
            "Redacted offline by redactor from two Burp exports. It holds methods, templated \
             paths, parameter names, statuses, body sizes and JSON field names, not values. \
             `{id}`, `{uuid}`, `{hex}`, `{token}`, `{email}`, `{value}` and `{key}` stand for \
             values that were removed.\n\n\
             This only compares what each role already requested. Nothing was replayed, so an \
             endpoint seen by one role says nothing about whether the other can reach it.\n\n",
        );

        let _ = writeln!(out, "| Result | Endpoints |\n|---|---:|");
        for section in &self.sections {
            let _ = writeln!(out, "| {} | {} |", section.title, section.rows.len());
        }
        let left_out = &self.left_out;
        let _ = writeln!(
            out,
            "\nLeft out: {} never requested and {} static files from {h}; {} and {} from {l}.\n",
            left_out.high_unrequested,
            left_out.high_static,
            left_out.low_unrequested,
            left_out.low_static
        );

        for section in self.sections.iter().filter(|s| !s.rows.is_empty()) {
            write_section(&mut out, section, max_rows, h, l);
        }

        if !self.samples.is_empty() {
            out.push_str("## Selected responses\n\n");
            for sample in &self.samples {
                let _ = writeln!(
                    out,
                    "**{}** `{}`\n\n~~~~\n{}\n~~~~\n",
                    sample.role,
                    sample.endpoint,
                    sample.text.replace("~~~~", "~ ~ ~ ~")
                );
            }
        }
        out
    }
}

fn title(class: Class, h: &str, l: &str) -> String {
    match class {
        Class::SameContent => format!("Same 2xx content for {h} and {l}"),
        Class::DifferentContent => "2xx for both, different content".to_string(),
        Class::Inverted => format!("2xx for {l} but not for {h}"),
        Class::Enforced => format!("2xx for {h} but not for {l}"),
        Class::HighOnly => format!("Requested by {h} only"),
        Class::LowOnly => format!("Requested by {l} only"),
        Class::NoAccess => "No 2xx for either".to_string(),
    }
}

fn hint(class: Class, h: &str, l: &str) -> String {
    match class {
        Class::SameContent => format!(
            "Possible missing access control, or simply shared content: check whether {l} \
             should reach these at all."
        ),
        Class::DifferentContent => format!(
            "Check what each role sees. + fields appear only for {h}, - fields only for {l}."
        ),
        Class::Inverted => format!("Unusual: {l} gets more than {h}."),
        Class::Enforced => {
            format!("The control looks enforced. Check that {l}'s responses are real denials.")
        }
        Class::HighOnly => format!("Not a finding yet: replay these with {l}'s session."),
        Class::LowOnly => format!("Functions {h} did not visit."),
        Class::NoAccess => "Little signal.".to_string(),
    }
}

/// The largest path prefixes, once a section is too long to read row by row.
fn prefixes(rows: &[&Row], names: &Names) -> Vec<(String, usize)> {
    if rows.len() <= 20 {
        return Vec::new();
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        *counts.entry(names.prefix(&row.endpoint.path)).or_default() += 1;
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts.truncate(10);
    counts
}

fn report_row(row: &Row, names: &Names) -> ReportRow {
    let side = |stats: &Stats| Side {
        statuses: statuses(stats),
        size: sizes(stats),
        requests: stats.requests,
    };
    ReportRow {
        origin: names.piece(&row.endpoint.origin),
        endpoint: names.endpoint(row.endpoint),
        high: row.high.map(side),
        low: row.low.map(side),
        notes: match (row.high, row.low) {
            (Some(high), Some(low)) => notes(row, high, low, names),
            _ => String::new(),
        },
    }
}

fn write_section(out: &mut String, section: &ReportSection, max_rows: usize, h: &str, l: &str) {
    let rows = &section.rows;
    let _ = writeln!(
        out,
        "## {} ({})\n\n{}\n",
        section.title,
        rows.len(),
        section.hint
    );

    if !section.prefixes.is_empty() {
        let listed: Vec<String> = section
            .prefixes
            .iter()
            .map(|(prefix, count)| format!("`{prefix}` ({count})"))
            .collect();
        let _ = writeln!(out, "By prefix: {}\n", listed.join(", "));
    }

    let paired = !matches!(section.class, Class::HighOnly | Class::LowOnly);
    if paired {
        let _ = writeln!(
            out,
            "| Endpoint | {h} | {l} | {h} size | {l} size | Notes |\n|---|---|---|---:|---:|---|"
        );
    } else {
        let _ = writeln!(
            out,
            "| Endpoint | Status | Size | Requests |\n|---|---|---:|---:|"
        );
    }

    let mut origin = None;
    for row in rows.iter().take(max_rows) {
        let label = if origin == Some(&row.origin) {
            format!("`{}`", row.endpoint)
        } else {
            origin = Some(&row.origin);
            format!("`{}` on {}", row.endpoint, row.origin)
        };
        match (&row.high, &row.low) {
            (Some(high), Some(low)) => {
                let _ = writeln!(
                    out,
                    "| {label} | {} | {} | {} | {} | {} |",
                    high.statuses, low.statuses, high.size, low.size, row.notes
                );
            }
            (Some(side), None) | (None, Some(side)) => {
                let _ = writeln!(
                    out,
                    "| {label} | {} | {} | {} |",
                    side.statuses, side.size, side.requests
                );
            }
            (None, None) => {}
        }
    }
    if rows.len() > max_rows {
        let _ = writeln!(
            out,
            "\n{} more not listed (raise `--max-rows` to see them).",
            rows.len() - max_rows
        );
    }
    out.push('\n');
}

fn statuses(stats: &Stats) -> String {
    let list: Vec<String> = stats.statuses.iter().map(u16::to_string).collect();
    list.join(", ")
}

fn sizes(stats: &Stats) -> String {
    if stats.min_size == stats.max_size {
        stats.max_size.to_string()
    } else {
        format!("{}-{}", stats.min_size, stats.max_size)
    }
}

fn notes(row: &Row, high: &Stats, low: &Stats, names: &Names) -> String {
    let mut notes = Vec::new();
    match row.class {
        Class::SameContent if row.identical => notes.push("identical body".to_string()),
        Class::SameContent => notes.push("same once ids and tokens are ignored".to_string()),
        Class::DifferentContent => {
            notes.extend(key_note('+', high.keys.difference(&low.keys), names));
            notes.extend(key_note('-', low.keys.difference(&high.keys), names));
            if row.endpoint.path.contains('{') && !row.same_object {
                notes.push("different objects, so some difference is expected".to_string());
            }
        }
        _ => {}
    }
    notes.join("; ")
}

fn key_note<'a>(
    sign: char,
    keys: impl Iterator<Item = &'a String>,
    names: &Names,
) -> Option<String> {
    let keys: Vec<&String> = keys.collect();
    if keys.is_empty() {
        return None;
    }
    let listed: Vec<String> = keys
        .iter()
        .take(MAX_NOTE_KEYS)
        .map(|key| format!("{sign}{}", names.key(key)))
        .collect();
    let more = keys.len().saturating_sub(MAX_NOTE_KEYS);
    Some(if more > 0 {
        format!("{} and {more} more", listed.join(" "))
    } else {
        listed.join(" ")
    })
}

/// Redacts the names that reach the report, one piece at a time: as a whole,
/// a field path like `user.id` would look like a host name.
struct Names<'a> {
    redactor: &'a Redactor,
    cache: RefCell<HashMap<String, String>>,
}

impl<'a> Names<'a> {
    fn new(redactor: &'a Redactor) -> Self {
        Self {
            redactor,
            cache: RefCell::default(),
        }
    }

    fn piece(&self, value: &str) -> String {
        if value.is_empty() || is_placeholder(value) {
            return value.to_string();
        }
        self.cache
            .borrow_mut()
            .entry(value.to_string())
            .or_insert_with(|| escape_cell(&self.redactor.redact(value).output))
            .clone()
    }

    fn path(&self, path: &str) -> String {
        let parts: Vec<String> = path.split('/').map(|p| self.piece(p)).collect();
        parts.join("/")
    }

    fn prefix(&self, path: &str) -> String {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).take(2).collect();
        self.path(&format!("/{}", parts.join("/")))
    }

    fn key(&self, key: &str) -> String {
        let parts: Vec<String> = key.split('.').map(|p| self.piece(p)).collect();
        parts.join(".")
    }

    fn endpoint(&self, endpoint: &Endpoint) -> String {
        let mut label = format!("{} {}", endpoint.method, self.path(&endpoint.path));
        if !endpoint.query.is_empty() {
            let query: Vec<String> = endpoint.query.iter().map(|q| self.piece(q)).collect();
            let _ = write!(label, "?{}", query.join("&"));
        }
        if let Some(operation) = &endpoint.operation {
            let _ = write!(label, " #{}", self.piece(operation));
        }
        label
    }
}

/// `{id}`, `{id}.json` or `{key}[]`, which this module wrote itself.
fn is_placeholder(value: &str) -> bool {
    let head = value.split('.').next().unwrap_or_default();
    matches!(
        head.trim_end_matches("[]"),
        "{id}" | "{uuid}" | "{hex}" | "{token}" | "{email}" | "{value}" | "{key}"
    )
}

fn escape_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace('`', "'")
        .replace('\n', " ")
}

#[derive(Default)]
struct Item {
    host: String,
    port: String,
    protocol: String,
    method: String,
    path: String,
    status: String,
    mime: String,
    extension: String,
    request: Vec<u8>,
    response: Vec<u8>,
}

impl Item {
    /// The status, unless Burp only discovered this item without requesting it.
    fn requested_status(&self) -> Option<u16> {
        if self.response.is_empty() {
            return None;
        }
        self.status.trim().parse().ok()
    }

    fn is_static(&self) -> bool {
        let path = self.path.split(['?', ';']).next().unwrap_or_default();
        let extension = path
            .rsplit('/')
            .next()
            .and_then(|file| file.rsplit_once('.'))
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_else(|| self.extension.trim().to_ascii_lowercase());
        STATIC_EXTENSIONS.contains(&extension.as_str())
            || STATIC_MIME_TYPES.contains(&self.mime.trim().to_ascii_lowercase().as_str())
    }

    fn endpoint(&self) -> Endpoint {
        let (path, query) = self.path.split_once('?').unwrap_or((&self.path, ""));
        let query: BTreeSet<String> = query
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|p| clean_key(p.split('=').next().unwrap_or_default()))
            .collect();
        Endpoint {
            origin: self.origin(),
            path: template_path(path),
            method: self.method.trim().to_ascii_uppercase(),
            query: query.into_iter().collect(),
            operation: self.graphql_operation(path),
        }
    }

    fn origin(&self) -> String {
        let protocol = self.protocol.trim().to_ascii_lowercase();
        let host = self.host.trim().to_ascii_lowercase();
        let port = self.port.trim();
        let default = matches!((protocol.as_str(), port), ("https", "443") | ("http", "80"));
        if default || port.is_empty() {
            format!("{protocol}://{host}")
        } else {
            format!("{protocol}://{host}:{port}")
        }
    }

    fn graphql_operation(&self, path: &str) -> Option<String> {
        if !path.to_ascii_lowercase().contains("graphql") {
            return None;
        }
        let body = message_body(&self.request);
        if body.len() > MAX_PARSED_BODY {
            return None;
        }
        let json: serde_json::Value = serde_json::from_slice(body).ok()?;
        Some(clean_key(json.get("operationName")?.as_str()?))
    }
}

/// Calls `visit` with each item until it returns false.
fn for_each_item(
    input: impl BufRead,
    mut visit: impl FnMut(Item) -> bool,
) -> Result<(), SitemapError> {
    let mut reader = Reader::from_reader(input);
    let mut buf = Vec::new();
    let mut text = String::new();
    let mut root = false;
    let mut item: Option<Item> = None;
    let mut field: Option<(String, bool)> = None;
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| SitemapError::Xml(e.to_string()))?;
        match event {
            Event::Start(start) => match start.name().0 {
                "items" => root = true,
                "item" => item = Some(Item::default()),
                name if item.is_some() => {
                    let base64 = start
                        .attributes()
                        .flatten()
                        .any(|a| a.key.0 == "base64" && a.value == "true");
                    field = Some((name.to_string(), base64));
                    text.clear();
                }
                _ => {}
            },
            Event::Text(t) if field.is_some() => text.push_str(&t),
            Event::CData(t) if field.is_some() => text.push_str(&t),
            Event::GeneralRef(r) if field.is_some() => match &*r {
                "amp" => text.push('&'),
                "lt" => text.push('<'),
                "gt" => text.push('>'),
                "quot" => text.push('"'),
                "apos" => text.push('\''),
                _ => {}
            },
            Event::End(end) => {
                if end.name().0 == "item" {
                    if let Some(done) = item.take()
                        && !visit(done)
                    {
                        return Ok(());
                    }
                } else if let (Some(current), Some((name, base64))) = (&mut item, field.take()) {
                    store_field(current, &name, base64, &text);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if root {
        Ok(())
    } else {
        Err(SitemapError::NotBurpExport)
    }
}

fn store_field(item: &mut Item, name: &str, base64: bool, value: &str) {
    let string = || value.to_string();
    let message = || {
        if base64 {
            let compact: Vec<u8> = value.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
            STANDARD.decode(compact).unwrap_or_default()
        } else {
            value.as_bytes().to_vec()
        }
    };
    match name {
        "host" => item.host = string(),
        "port" => item.port = string(),
        "protocol" => item.protocol = string(),
        "method" => item.method = string(),
        "path" => item.path = string(),
        "status" => item.status = string(),
        "mimetype" => item.mime = string(),
        "extension" => item.extension = string(),
        "request" => item.request = message(),
        "response" => item.response = message(),
        _ => {}
    }
}

static UUID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$").unwrap()
});
static HEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[0-9a-f]{16,}$").unwrap());
static DIGIT_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d{4,}").unwrap());
static SAFE_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_-]{0,63}$").unwrap());
static VOLATILE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|[A-Za-z0-9+/_=-]{24,}|[0-9a-f]{16,}|\d+",
    )
    .unwrap()
});

/// `/users/88127/orders` -> `/users/{id}/orders`.
fn template_path(path: &str) -> String {
    let segments: Vec<String> = path.split('/').map(template_segment).collect();
    let path = segments.join("/");
    if path.is_empty() {
        "/".to_string()
    } else {
        path
    }
}

fn template_segment(segment: &str) -> String {
    // Matrix parameters such as `;jsessionid=` are values, not functions.
    let segment = segment.split(';').next().unwrap_or_default();
    if let Some((stem, extension)) = segment.rsplit_once('.')
        && !stem.is_empty()
        && extension.len() <= 5
        && extension.chars().all(|c| c.is_ascii_alphanumeric())
    {
        let template = template_stem(stem);
        if template.starts_with('{') {
            return format!("{template}.{extension}");
        }
    }
    template_stem(segment)
}

fn template_stem(segment: &str) -> String {
    let lower = segment.to_ascii_lowercase();
    if segment.is_empty() {
        String::new()
    } else if segment.chars().all(|c| c.is_ascii_digit()) {
        "{id}".to_string()
    } else if UUID.is_match(segment) {
        "{uuid}".to_string()
    } else if HEX.is_match(segment) {
        "{hex}".to_string()
    } else if segment.contains('@') || lower.contains("%40") {
        "{email}".to_string()
    } else if segment.contains('=') || lower.contains("%3d") {
        "{value}".to_string()
    } else if segment.len() >= 20
        && segment.chars().any(|c| c.is_ascii_digit())
        && segment.chars().any(|c| c.is_ascii_alphabetic())
    {
        "{token}".to_string()
    } else if DIGIT_RUN.is_match(segment) {
        "{id}".to_string()
    } else {
        segment.to_string()
    }
}

/// A parameter or field name, unless it looks like data used as a key.
fn clean_key(key: &str) -> String {
    if SAFE_KEY.is_match(key) && !template_stem(key).starts_with('{') {
        key.to_string()
    } else {
        "{key}".to_string()
    }
}

fn message_body(message: &[u8]) -> &[u8] {
    let crlf = message
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4);
    let lf = message.windows(2).position(|w| w == b"\n\n").map(|i| i + 2);
    match (crlf, lf) {
        (Some(a), Some(b)) => &message[a.min(b)..],
        (Some(i), None) | (None, Some(i)) => &message[i..],
        (None, None) => &[],
    }
}

fn hash_of(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// The hash of a body without ids, tokens, numbers and whitespace changes.
fn loose_hash(body: &[u8]) -> u64 {
    if body.len() > MAX_PARSED_BODY {
        return hash_of(body);
    }
    let text = String::from_utf8_lossy(body);
    let stable = VOLATILE.replace_all(&text, "#");
    let words: Vec<&str> = stable.split_whitespace().collect();
    hash_of(words)
}

fn insert_bounded(set: &mut HashSet<u64>, value: u64) {
    if set.len() < MAX_HASHES {
        set.insert(value);
    }
}

fn collect_keys(body: &[u8], keys: &mut BTreeSet<String>) {
    if body.len() > MAX_PARSED_BODY {
        return;
    }
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
        walk_json(&json, "", 0, keys);
    }
}

fn walk_json(value: &serde_json::Value, prefix: &str, depth: usize, keys: &mut BTreeSet<String>) {
    if depth > MAX_KEY_DEPTH || keys.len() >= MAX_KEYS {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let key = clean_key(key);
                let path = if prefix.is_empty() {
                    key
                } else {
                    format!("{prefix}.{key}")
                };
                if keys.len() < MAX_KEYS {
                    keys.insert(path.clone());
                }
                walk_json(child, &path, depth + 1, keys);
            }
        }
        serde_json::Value::Array(items) => {
            let path = format!("{prefix}[]");
            for item in items.iter().take(50) {
                walk_json(item, &path, depth + 1, keys);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_values_in_paths() {
        for (path, expected) in [
            ("/users/88127/orders", "/users/{id}/orders"),
            ("/o/1b4e28ba-2fa1-41d2-883f-0016d3cca427", "/o/{uuid}"),
            ("/f/a7f3e9c1d5b2084f6e1a9c3d7b5f2e80", "/f/{hex}"),
            ("/u/maria.gomez@globex.example/edit", "/u/{email}/edit"),
            ("/u/maria%40globex.example", "/u/{email}"),
            ("/a;jsessionid=A1B2C3", "/a"),
            ("/acct/ACC-00123", "/acct/{id}"),
            ("/users/88127.json", "/users/{id}.json"),
            ("/api/v2/oauth2/app.config", "/api/v2/oauth2/app.config"),
        ] {
            assert_eq!(template_path(path), expected);
        }
    }

    #[test]
    fn keeps_only_names_that_do_not_look_like_data() {
        assert_eq!(clean_key("deviceFingerprint"), "deviceFingerprint");
        assert_eq!(clean_key("88127"), "{key}");
        assert_eq!(clean_key("maria@globex.example"), "{key}");
        assert_eq!(clean_key("a.b"), "{key}");
    }

    #[test]
    fn ignores_ids_and_tokens_in_the_loose_hash() {
        assert_eq!(
            loose_hash(br#"{"id": 1, "csrf": "a7f3e9c1d5b2084f6e1a9c3d7b5f2e80"}"#),
            loose_hash(br#"{"id": 22,  "csrf": "0000e9c1d5b2084f6e1a9c3d7b5f2e80"}"#)
        );
        assert_ne!(loose_hash(b"allowed"), loose_hash(b"denied"));
    }

    #[test]
    fn finds_bodies_with_either_line_ending() {
        assert_eq!(message_body(b"HTTP/1.1 200\r\nA: b\r\n\r\nhi"), b"hi");
        assert_eq!(message_body(b"HTTP/1.1 200\nA: b\n\nhi"), b"hi");
        assert_eq!(message_body(b"HTTP/1.1 204\r\nA: b"), b"");
    }

    #[test]
    fn rejects_files_that_are_not_burp_exports() {
        assert!(matches!(
            read_map(&b"<html></html>"[..]),
            Err(SitemapError::NotBurpExport)
        ));
    }
}
