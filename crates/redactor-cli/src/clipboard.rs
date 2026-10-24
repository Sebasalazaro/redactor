//! Clipboard access. macOS and Linux go through the system tools, so the CLI
//! has no GUI dependencies there; Windows uses the native API, which also
//! keeps what redactor writes out of clipboard history and Cloud Clipboard.

#[cfg(not(windows))]
use std::io::Write;
#[cfg(not(windows))]
use std::process::{Command, Stdio};

use anyhow::Result;
#[cfg(not(windows))]
use anyhow::{Context, bail};

#[cfg(target_os = "macos")]
const PASTE: (&str, &[&str]) = ("pbpaste", &[]);
#[cfg(target_os = "macos")]
const COPY: (&str, &[&str]) = ("pbcopy", &[]);

#[cfg(all(unix, not(target_os = "macos")))]
const PASTE: (&str, &[&str]) = ("xclip", &["-selection", "clipboard", "-o"]);
#[cfg(all(unix, not(target_os = "macos")))]
const COPY: (&str, &[&str]) = ("xclip", &["-selection", "clipboard"]);

#[cfg(not(windows))]
pub fn read() -> Result<String> {
    let output = Command::new(PASTE.0)
        .args(PASTE.1)
        .output()
        .with_context(|| format!("running {}", PASTE.0))?;
    if !output.status.success() {
        bail!("{} failed", PASTE.0);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(windows))]
pub fn write(text: &str) -> Result<()> {
    let mut child = Command::new(COPY.0)
        .args(COPY.1)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("running {}", COPY.0))?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    if !child.wait()?.success() {
        bail!("{} failed", COPY.0);
    }
    Ok(())
}

#[cfg(windows)]
pub fn read() -> Result<String> {
    Ok(arboard::Clipboard::new()?.get_text()?)
}

/// Restored text holds real values: it must not stay in Win+V history or
/// be synced to other devices.
#[cfg(windows)]
pub fn write(text: &str) -> Result<()> {
    use arboard::SetExtWindows;
    arboard::Clipboard::new()?
        .set()
        .exclude_from_history()
        .exclude_from_cloud()
        .text(text)?;
    Ok(())
}
