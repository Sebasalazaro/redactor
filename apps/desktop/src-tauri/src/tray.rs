//! Menu bar icon: redact now, open the dashboard, switch engagement, quit.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::commands;
use crate::flow::{self, DASHBOARD};
use crate::state::AppState;

const TRAY_ID: &str = "main";
const ENGAGEMENT_PREFIX: &str = "engagement:";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("redactor")
        .menu(&menu(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;
    Ok(())
}

/// Rebuilds the menu after settings or engagements change.
pub fn refresh(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let active = settings.active_engagement.as_deref();

    let engagements = Submenu::with_id(app, "engagements", "Engagement", true)?;
    engagements.append(&CheckMenuItem::with_id(
        app,
        ENGAGEMENT_PREFIX,
        "None (global config only)",
        true,
        active.is_none(),
        None::<&str>,
    )?)?;
    let list = commands::engagements(&state).unwrap_or_default();
    if !list.is_empty() {
        engagements.append(&PredefinedMenuItem::separator(app)?)?;
    }
    let mut active_name = None;
    for e in &list {
        let checked = active == Some(e.id.as_str());
        if checked {
            active_name = Some(e.name.as_str());
        }
        let id = format!("{ENGAGEMENT_PREFIX}{}", e.id);
        engagements.append(&CheckMenuItem::with_id(
            app,
            id,
            &e.name,
            true,
            checked,
            None::<&str>,
        )?)?;
    }

    let status = format!("Profile: {}", active_name.unwrap_or("global"));
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(
                app,
                "redact",
                format!(
                    "Redact clipboard   {}",
                    flow::pretty_hotkey(&settings.hotkey)
                ),
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app,
                "restore",
                "Restore clipboard (real values)",
                settings.remember,
                None::<&str>,
            )?,
            &MenuItem::with_id(app, "dashboard", "Open dashboard…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "status", status, false, None::<&str>)?,
            &engagements,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit redactor", true, None::<&str>)?,
        ],
    )
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "redact" => flow::redact_clipboard(app),
        "dashboard" => flow::show(app, DASHBOARD),
        "restore" => {
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(e) = flow::restore_clipboard(&app) {
                    eprintln!("redactor: cannot restore: {e}");
                }
            });
        }
        "quit" => app.exit(0),
        _ => {
            if let Some(name) = id.strip_prefix(ENGAGEMENT_PREFIX) {
                let state = app.state::<AppState>();
                let mut settings = state.settings();
                settings.active_engagement = (!name.is_empty()).then(|| name.to_string());
                if state.set_settings(settings).is_ok() {
                    let _ = app.emit_to(DASHBOARD, "settings-changed", ());
                    let _ = app.emit_to(DASHBOARD, "overview-changed", ());
                }
                refresh(app);
            }
        }
    }
}
