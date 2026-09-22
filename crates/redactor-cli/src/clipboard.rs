//! Clipboard access through the system tools, so the CLI has no GUI
//! dependencies. The desktop app uses native APIs instead.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

#[cfg(target_os = "macos")]
const PASTE: (&str, &[&str]) = ("pbpaste", &[]);
#[cfg(target_os = "macos")]
const COPY: (&str, &[&str]) = ("pbcopy", &[]);

#[cfg(not(target_os = "macos"))]
const PASTE: (&str, &[&str]) = ("xclip", &["-selection", "clipboard", "-o"]);
#[cfg(not(target_os = "macos"))]
const COPY: (&str, &[&str]) = ("xclip", &["-selection", "clipboard"]);

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
