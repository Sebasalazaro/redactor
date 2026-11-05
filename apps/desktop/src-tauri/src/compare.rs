//! The Compare page: two Burp site map exports, one redacted report.
//!
//! The webview only ever sees file names and the redacted report. It cannot
//! name a path: files are chosen in a native dialog and the paths stay here.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use redactor_core::Redactor;
use redactor_core::sitemap::{self, Report, ReportOptions, SiteMap};
use serde::Serialize;

/// Rows per section in the copied or saved Markdown. The page lists them all.
const MARKDOWN_ROWS: usize = 150;

#[derive(Debug, Default)]
pub struct Comparison {
    high: Option<PathBuf>,
    low: Option<PathBuf>,
    report: Option<Report>,
}

/// What the page needs to draw itself, also after navigating away and back.
#[derive(Debug, Serialize)]
pub struct ComparisonDto {
    pub high_file: Option<String>,
    pub low_file: Option<String>,
    pub report: Option<Report>,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

impl Comparison {
    pub fn dto(&self) -> ComparisonDto {
        ComparisonDto {
            high_file: self.high.as_deref().map(file_name),
            low_file: self.low.as_deref().map(file_name),
            report: self.report.clone(),
        }
    }

    /// Choosing another export makes the current report out of date.
    pub fn set_export(&mut self, high: bool, path: PathBuf) {
        *(if high { &mut self.high } else { &mut self.low }) = Some(path);
        self.report = None;
    }

    pub fn exports(&self) -> Option<(PathBuf, PathBuf)> {
        Some((self.high.clone()?, self.low.clone()?))
    }

    pub fn set_report(&mut self, report: Report) {
        self.report = Some(report);
    }

    pub fn markdown(&self) -> Option<String> {
        Some(self.report.as_ref()?.to_markdown(MARKDOWN_ROWS))
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn open(path: &Path) -> Result<BufReader<File>, String> {
    File::open(path)
        .map(BufReader::new)
        .map_err(|e| format!("{}: {e}", file_name(path)))
}

fn read(path: &Path) -> Result<SiteMap, String> {
    sitemap::read_map(open(path)?).map_err(|e| format!("{}: {e}", file_name(path)))
}

/// Reads both exports and builds the report. Can take a while on large ones.
pub fn run(
    (high, low): (PathBuf, PathBuf),
    redactor: &Redactor,
    options: &ReportOptions,
    include_bodies: Option<&str>,
) -> Result<Report, String> {
    let high_map = read(&high)?;
    let low_map = read(&low)?;
    let mut samples = Vec::new();
    if let Some(filter) = include_bodies {
        for (path, role) in [(&high, &options.high_label), (&low, &options.low_label)] {
            let found = sitemap::read_samples(open(path)?, role, filter, redactor)
                .map_err(|e| format!("{}: {e}", file_name(path)))?;
            samples.extend(found);
        }
    }
    Ok(sitemap::build_report(
        &high_map, &low_map, redactor, options, &samples,
    ))
}
