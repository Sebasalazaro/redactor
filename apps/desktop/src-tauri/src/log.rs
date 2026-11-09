//! A small diagnostic log, `redactor.log` in the config dir. Release builds on
//! Windows have no console, so this is the only trace of what the app did.
//!
//! It records app events (start, hotkey, windows, errors), never clipboard
//! contents or redacted values.

use std::fmt::Display;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const FILE: &str = "redactor.log";
/// Past this size the log is moved to `redactor.log.1` and starts over.
const MAX_BYTES: u64 = 512 * 1024;

static PATH: OnceLock<PathBuf> = OnceLock::new();
static WRITE: Mutex<()> = Mutex::new(());

pub fn init(dir: &Path) {
    let path = dir.join(FILE);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join(format!("{FILE}.1")));
    }
    let _ = PATH.set(path);
}

pub fn path() -> Option<&'static Path> {
    PATH.get().map(PathBuf::as_path)
}

/// Appends a line. Logging must never break the app, so errors are ignored.
pub fn info(message: impl Display) {
    let line = format!("{} {message}\n", timestamp());
    eprintln!("redactor: {message}");
    let Some(path) = PATH.get() else { return };
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// UTC, `YYYY-MM-DD HH:MM:SS`.
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_days_to_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(20_766), (2026, 11, 9));
    }
}
