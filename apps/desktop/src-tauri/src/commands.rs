//! Commands invoked from the webviews. Errors are returned as strings for
//! display; nothing here touches the network.

use redactor_core::Config;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::dto::{
    EngagementDto, ForgottenDto, FreedDto, OverviewDto, ReviewDto, StatusDto, VaultDto,
};
use crate::flow::{self, DASHBOARD, REVIEW};
use crate::memory;
use crate::settings::AppSettings;
use crate::state::{AppState, Forgotten};
use crate::tray;
use crate::vault::Pair;
use redactor_core::Restoration;

type CmdResult<T> = Result<T, String>;

fn err(e: impl ToString) -> String {
    e.to_string()
}

// Review popup

#[tauri::command]
pub fn get_review(state: State<AppState>) -> Option<ReviewDto> {
    let engagement = state.settings().active_engagement;
    let ai = state.ai.installed();
    state.with_pending(|pending| pending.map(|r| ReviewDto::new(r, engagement, ai)))
}

#[tauri::command]
pub fn finish_review(app: AppHandle, text: String, pairs: Vec<Pair>) -> CmdResult<()> {
    flow::finish_review(&app, text, pairs)
}

// Reversible mode

/// Puts the real values back into an LLM answer. May wait on a keychain
/// prompt the first time, so it runs off the main thread.
#[tauri::command]
pub async fn restore_text(app: AppHandle, text: String) -> CmdResult<Restoration> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state.vaults.restore(&state.store, &state.profile(), &text)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn restore_clipboard(app: AppHandle) -> CmdResult<(usize, usize)> {
    tauri::async_runtime::spawn_blocking(move || flow::restore_clipboard(&app))
        .await
        .map_err(err)?
}

/// Profile of the active engagement and how many values its vault holds.
#[tauri::command]
pub async fn vault_info(app: AppHandle) -> CmdResult<VaultDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let profile = state.profile();
        let entries = state.vaults.len(&state.store, &profile)?;
        Ok(VaultDto { profile, entries })
    })
    .await
    .map_err(err)?
}

/// Copies text chosen in the dashboard (a restored answer).
#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> CmdResult<()> {
    app.clipboard().write_text(text).map_err(err)
}

#[tauri::command]
pub fn forget_vault(state: State<AppState>) -> CmdResult<()> {
    state.vaults.forget(&state.store, &state.profile())
}

#[tauri::command]
pub fn cancel_review(app: AppHandle) {
    flow::cancel_review(&app);
}

/// Runs the local AI model over the review text. Blocking work (loading
/// the model, inference) happens off the main thread.
#[tauri::command]
pub async fn deep_scan(app: AppHandle, text: String) -> CmdResult<Vec<crate::ai::AiEntity>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let redactor = state.redactor();
        let result = state.ai.scan(&text, &redactor);
        let _ = app.emit_to(DASHBOARD, "overview-changed", ());
        result
    })
    .await
    .map_err(err)?
}

/// Masks a value the user selected by hand.
#[tauri::command]
pub fn mask_text(state: State<AppState>, text: String) -> String {
    state.redactor().mask_secret(&text)
}

// Overview

#[tauri::command]
pub fn get_overview(state: State<AppState>) -> OverviewDto {
    OverviewDto {
        last: state.last().map(Into::into),
        forgotten: state.forgotten().map(|(reason, ago)| ForgottenDto {
            reason,
            secs_ago: ago.as_secs(),
        }),
        memory: state.memory(),
        redactions: state.redactions(),
        ai_installed: state.ai.installed(),
        ai_loaded: state.ai.loaded(),
        uptime_secs: state.uptime().as_secs(),
    }
}

/// Same as the hotkey: redacts the clipboard and opens the review popup.
#[tauri::command]
pub fn redact_now(app: AppHandle) {
    flow::redact_clipboard(&app);
}

/// Called by a window once it has saved its state and can go away.
#[tauri::command]
pub fn close_window(window: tauri::WebviewWindow) {
    let _ = window.destroy();
}

/// Puts the last redacted text back on the clipboard.
#[tauri::command]
pub fn copy_last(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    let last = state.last().ok_or("nothing to copy")?;
    app.clipboard().write_text(last.output).map_err(err)
}

#[tauri::command]
pub fn clear_last(state: State<AppState>) {
    state.clear_last(Forgotten::Cleared);
}

#[tauri::command]
pub fn clear_clipboard(app: AppHandle) -> CmdResult<()> {
    app.clipboard().write_text(String::new()).map_err(err)
}

/// Drops everything the session holds (last and pending redactions, the
/// hidden review window) and returns freed pages to the OS.
#[tauri::command]
pub fn free_memory(app: AppHandle, state: State<AppState>) -> FreedDto {
    state.clear_last(Forgotten::Freed);
    state.ai.unload();
    state.vaults.unload();
    if let Some(review) = app.get_webview_window(REVIEW) {
        if !review.is_visible().unwrap_or(true) {
            state.set_pending(None);
            let _ = review.destroy();
        }
    }
    let released_bytes = memory::release_free_pages();
    FreedDto {
        released_bytes,
        memory: state.memory(),
    }
}

// Dashboard

#[tauri::command]
pub fn preview(state: State<AppState>, text: String) -> ReviewDto {
    let redaction = state.redactor().redact(&text);
    ReviewDto::new(
        &redaction,
        state.settings().active_engagement,
        state.ai.installed(),
    )
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> StatusDto {
    StatusDto {
        config_dir: state.store.dir().display().to_string(),
        model_dir: state.ai.dir().display().to_string(),
        ai_installed: state.ai.installed(),
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
pub fn list_engagements(state: State<AppState>) -> CmdResult<Vec<EngagementDto>> {
    engagements(&state)
}

pub fn engagements(state: &AppState) -> CmdResult<Vec<EngagementDto>> {
    let mut list = Vec::new();
    for id in state.store.engagements().map_err(err)? {
        let config = state.store.load_engagement(&id).map_err(err)?;
        let name = config.name.clone().unwrap_or_else(|| id.clone());
        list.push(EngagementDto { id, name, config });
    }
    list.sort_by_key(|e| e.name.to_lowercase());
    Ok(list)
}

/// Creates an engagement from any display name and returns its id.
#[tauri::command]
pub fn create_engagement(
    app: AppHandle,
    state: State<AppState>,
    name: String,
) -> CmdResult<String> {
    if name.trim().is_empty() {
        return Err("the name cannot be empty".into());
    }
    let id = state.store.create_engagement(&name).map_err(err)?;
    tray::refresh(&app);
    Ok(id)
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
