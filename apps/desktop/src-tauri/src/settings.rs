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
    /// Engagement layered on top of the global config.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_engagement: Option<String>,
    /// Show the review popup (true) or replace the clipboard silently.
    pub review: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Alt+R".into(),
            active_engagement: None,
            review: true,
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
