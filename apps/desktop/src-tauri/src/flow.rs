//! The hotkey flow: clipboard → redact → review popup → clipboard.

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::state::AppState;

pub const REVIEW: &str = "review";
pub const DASHBOARD: &str = "dashboard";

/// Redacts the clipboard. In review mode the result waits in the popup;
/// otherwise it replaces the clipboard right away.
pub fn redact_clipboard(app: &AppHandle) {
    let state = app.state::<AppState>();
    let text = app.clipboard().read_text().unwrap_or_default();
    let redaction = state.redactor().redact(&text);

    if state.settings().review {
        state.set_pending(Some(redaction));
        show(app, REVIEW);
        let _ = app.emit_to(REVIEW, "review-ready", ());
    } else {
        let _ = app.clipboard().write_text(redaction.output);
    }
}

/// Copies the reviewed text and closes the popup.
pub fn finish_review(app: &AppHandle, text: String) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| e.to_string())?;
    cancel_review(app);
    Ok(())
}

pub fn cancel_review(app: &AppHandle) {
    app.state::<AppState>().set_pending(None);
    if let Some(window) = app.get_webview_window(REVIEW) {
        let _ = window.hide();
    }
}

pub fn show(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Replaces the registered shortcut. The old one is kept if the new one is
/// invalid or already taken by another app.
pub fn register_hotkey(
    app: &AppHandle,
    previous: Option<&str>,
    hotkey: &str,
) -> Result<(), String> {
    let shortcut: Shortcut = hotkey
        .parse()
        .map_err(|e| format!("invalid shortcut {hotkey:?}: {e}"))?;
    let shortcuts = app.global_shortcut();
    if let Some(previous) = previous.and_then(|p| p.parse::<Shortcut>().ok()) {
        if previous == shortcut {
            return Ok(());
        }
        let _ = shortcuts.unregister(previous);
    }
    shortcuts.register(shortcut).map_err(|e| {
        if let Some(previous) = previous.and_then(|p| p.parse::<Shortcut>().ok()) {
            let _ = shortcuts.register(previous);
        }
        format!("cannot register {hotkey:?}: {e}")
    })
}

/// `CommandOrControl+Alt+R` → `⌥⌘R` on macOS, `Ctrl+Alt+R` elsewhere.
pub fn pretty_hotkey(hotkey: &str) -> String {
    let parts: Vec<&str> = hotkey.split('+').collect();
    if cfg!(target_os = "macos") {
        let mut mods = String::new();
        let mut key = "";
        for (symbol, names) in [
            ("⌃", &["control", "ctrl"][..]),
            ("⌥", &["alt", "option"][..]),
            ("⇧", &["shift"][..]),
            (
                "⌘",
                &[
                    "commandorcontrol",
                    "cmdorctrl",
                    "command",
                    "cmd",
                    "super",
                    "meta",
                ][..],
            ),
        ] {
            if parts
                .iter()
                .any(|p| names.contains(&p.to_ascii_lowercase().as_str()))
            {
                mods.push_str(symbol);
            }
        }
        if let Some(last) = parts.last() {
            key = last;
        }
        format!("{mods}{}", key.trim_start_matches("Key").to_uppercase())
    } else {
        hotkey
            .replace("CommandOrControl", "Ctrl")
            .replace("CmdOrCtrl", "Ctrl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn pretty_prints_hotkeys() {
        assert_eq!(pretty_hotkey("CommandOrControl+Alt+R"), "⌥⌘R");
        assert_eq!(pretty_hotkey("Shift+Control+KeyK"), "⌃⇧K");
    }
}
