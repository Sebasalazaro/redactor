//! Desktop-only preferences, stored next to the shared config as `app.toml`.

use std::path::{Path, PathBuf};

use redactor_core::Store;
use serde::{Deserialize, Serialize};

const FILE: &str = "app.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppSettings {
    /// Global shortcut, in Tauri accelerator syntax.
    pub hotkey: String,
    /// Engagement id layered on top of the global config.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_engagement: Option<String>,
    /// Show the review popup (true) or replace the clipboard silently.
    pub review: bool,
    /// Keep the last redacted text in memory so it can be copied again.
    pub keep_last: bool,
    /// Forget the last redaction after this many minutes (0 = never).
    pub forget_last_after_minutes: u32,
    /// Empty the clipboard when a review is cancelled, so the unredacted
    /// text is not left behind.
    pub clear_clipboard_on_cancel: bool,
    /// Destroy windows when they close instead of hiding them. Frees the
    /// web view processes (~30-40 MB each) at the cost of a slower reopen.
    pub unload_windows: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Alt+R".into(),
            active_engagement: None,
            review: true,
            keep_last: true,
            forget_last_after_minutes: 60,
            clear_clipboard_on_cancel: false,
            unload_windows: true,
        }
    }
}

impl AppSettings {
    pub fn load(store: &Store) -> Result<Self, String> {
        let path = path(store);
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn save(&self, store: &Store) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        write(&path(store), &text)
    }
}

fn path(store: &Store) -> PathBuf {
    store.dir().join(FILE)
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_settings_files_still_load() {
        let old = "hotkey = \"CommandOrControl+Control+R\"\nreview = true\n";
        let settings: AppSettings = toml::from_str(old).unwrap();
        assert_eq!(settings.hotkey, "CommandOrControl+Control+R");
        assert!(settings.keep_last && settings.unload_windows);
    }
}
