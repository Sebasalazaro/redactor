//! Commands invoked from the webviews. Errors are returned as strings for
//! display; nothing here touches the network.

use redactor_core::Config;
use tauri::{AppHandle, Emitter, State};

use crate::dto::{ReviewDto, StatusDto};
use crate::flow::{self, DASHBOARD};
use crate::settings::AppSettings;
use crate::state::AppState;
use crate::tray;

type CmdResult<T> = Result<T, String>;

fn err(e: impl ToString) -> String {
    e.to_string()
}

// Review popup

#[tauri::command]
pub fn get_review(state: State<AppState>) -> Option<ReviewDto> {
    let engagement = state.settings().active_engagement;
    state.with_pending(|pending| pending.map(|r| ReviewDto::new(r, engagement)))
}

#[tauri::command]
pub fn finish_review(app: AppHandle, text: String) -> CmdResult<()> {
    flow::finish_review(&app, text)
}

#[tauri::command]
pub fn cancel_review(app: AppHandle) {
    flow::cancel_review(&app);
}

/// Masks a value the user selected by hand.
#[tauri::command]
pub fn mask_text(state: State<AppState>, text: String) -> String {
    state.redactor().mask_secret(&text)
}

// Dashboard

#[tauri::command]
pub fn preview(state: State<AppState>, text: String) -> ReviewDto {
    let redaction = state.redactor().redact(&text);
    ReviewDto::new(&redaction, state.settings().active_engagement)
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> StatusDto {
    StatusDto {
        config_dir: state.store.dir().display().to_string(),
        error: state.error(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> AppSettings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: AppSettings,
) -> CmdResult<()> {
    if let Some(name) = &settings.active_engagement {
        state.store.engagement_path(name).map_err(err)?;
    }
    let previous = state.settings();
    flow::register_hotkey(&app, Some(&previous.hotkey), &settings.hotkey)?;
    state.set_settings(settings)?;
    tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn get_global(state: State<AppState>) -> CmdResult<Config> {
    state.store.load_global().map_err(err)
}

#[tauri::command]
pub fn save_global(state: State<AppState>, config: Config) -> CmdResult<()> {
    state.store.save_global(&config).map_err(err)?;
    state.reload();
    Ok(())
}

#[tauri::command]
pub fn list_engagements(state: State<AppState>) -> CmdResult<Vec<String>> {
    state.store.engagements().map_err(err)
}

#[tauri::command]
pub fn get_engagement(state: State<AppState>, name: String) -> CmdResult<Config> {
    state.store.load_engagement(&name).map_err(err)
}

#[tauri::command]
pub fn save_engagement(
    app: AppHandle,
    state: State<AppState>,
    name: String,
    config: Config,
) -> CmdResult<()> {
    state.store.save_engagement(&name, &config).map_err(err)?;
    state.reload();
    tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn delete_engagement(app: AppHandle, state: State<AppState>, name: String) -> CmdResult<()> {
    state.store.delete_engagement(&name).map_err(err)?;
    let mut settings = state.settings();
    if settings.active_engagement.as_deref() == Some(name.as_str()) {
        settings.active_engagement = None;
        state.set_settings(settings)?;
        let _ = app.emit_to(DASHBOARD, "settings-changed", ());
    }
    tray::refresh(&app);
    Ok(())
}
