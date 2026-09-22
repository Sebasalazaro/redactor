//! The hotkey flow (clipboard → redact → review → clipboard) and window
//! lifecycle.
//!
//! Windows are created on demand. With `unload_windows` on, closing one
//! destroys it, which ends its WebKit process and frees its memory.

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::state::{AppState, LastRedaction};

pub const REVIEW: &str = "review";
pub const DASHBOARD: &str = "dashboard";

/// Redacts the clipboard. In review mode the result waits in the popup;
/// otherwise it replaces the clipboard right away.
pub fn redact_clipboard(app: &AppHandle) {
    let state = app.state::<AppState>();
    let text = app.clipboard().read_text().unwrap_or_default();
    let redaction = state.redactor().redact(&text);
    drop(text);

    if state.settings().review {
        state.set_pending(Some(redaction));
        show(app, REVIEW);
        let _ = app.emit_to(REVIEW, "review-ready", ());
    } else {
        let engagement = state.settings().active_engagement;
        let last = LastRedaction::new(redaction.output.clone(), &redaction, engagement);
        if app.clipboard().write_text(redaction.output).is_ok() {
            state.record(last);
            let _ = app.emit_to(DASHBOARD, "overview-changed", ());
        }
    }
}

/// Copies the reviewed text and closes the popup.
pub fn finish_review(app: &AppHandle, text: String) -> Result<(), String> {
    app.clipboard()
        .write_text(text.clone())
        .map_err(|e| e.to_string())?;
    app.state::<AppState>().complete_review(&text);
    close_review(app);
    let _ = app.emit_to(DASHBOARD, "overview-changed", ());
    Ok(())
}

/// Closes the popup without copying. The clipboard still holds the
/// unredacted text unless `clear_clipboard_on_cancel` is on.
pub fn cancel_review(app: &AppHandle) {
    if app.state::<AppState>().settings().clear_clipboard_on_cancel {
        let _ = app.clipboard().write_text(String::new());
    }
    close_review(app);
}

fn close_review(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.set_pending(None);
    if let Some(window) = app.get_webview_window(REVIEW) {
        if state.settings().unload_windows {
            let _ = window.destroy();
        } else {
            let _ = window.hide();
        }
    }
}

/// Asks the dashboard to save pending edits before it is destroyed. It calls
/// `close_window` when done; if it does not answer, it is closed anyway.
pub fn request_dashboard_close(app: &AppHandle) {
    let _ = app.emit_to(DASHBOARD, "close-requested", ());
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        if let Some(window) = app.get_webview_window(DASHBOARD) {
            let _ = window.destroy();
        }
    });
}

/// Shows a window, creating it if needed.
pub fn show(app: &AppHandle, label: &str) {
    let window = match app.get_webview_window(label) {
        Some(window) => Ok(window),
        None => build(app, label),
    };
    match window {
        Ok(window) => {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("redactor: cannot open {label}: {e}"),
    }
}

fn build(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    let review = label == REVIEW;
    let page = if review {
        "review.html"
    } else {
        "dashboard.html"
    };
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()));
    let builder = if review {
        builder
            .title("redactor · Review")
            .inner_size(880.0, 600.0)
            .min_inner_size(560.0, 360.0)
            .always_on_top(true)
            .skip_taskbar(true)
    } else {
        builder
            .title("redactor")
            .inner_size(1120.0, 760.0)
            .min_inner_size(860.0, 560.0)
    };
    builder.center().visible(false).build()
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
