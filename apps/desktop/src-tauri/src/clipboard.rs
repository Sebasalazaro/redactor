//! Every clipboard write goes through here. On Windows it is kept out of
//! clipboard history (Win+V) and Cloud Clipboard sync: restored text holds
//! real values, and nothing redactor writes should outlive the next copy.

use tauri::AppHandle;

pub fn read(app: &AppHandle) -> String {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().read_text().unwrap_or_default()
}

#[cfg(not(windows))]
pub fn write(app: &AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn write(_app: &AppHandle, text: String) -> Result<(), String> {
    use arboard::SetExtWindows;
    arboard::Clipboard::new()
        .and_then(|mut clipboard| {
            clipboard
                .set()
                .exclude_from_history()
                .exclude_from_cloud()
                .text(text)
        })
        .map_err(|e| e.to_string())
}
