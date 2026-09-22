//! redactor desktop: a menu bar app that redacts the clipboard on a global
//! hotkey and shows the result for review before it is pasted anywhere.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dto;
mod flow;
mod settings;
mod state;
mod tray;

use redactor_core::Store;
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::flow::{DASHBOARD, REVIEW};
use crate::state::AppState;

fn main() {
    let store =
        Store::open_default().expect("cannot locate the config dir; set REDACTOR_CONFIG_DIR");
    let first_run = !store.global_path().exists();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        flow::redact_clipboard(app);
                    }
                })
                .build(),
        )
        .manage(AppState::new(store))
        .setup(move |app| {
            // Menu bar only: no Dock icon, no app switcher entry.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle();
            tray::create(handle)?;
            let hotkey = handle.state::<AppState>().settings().hotkey;
            if let Err(e) = flow::register_hotkey(handle, None, &hotkey) {
                eprintln!("redactor: {e}");
            }
            if first_run {
                flow::show(handle, DASHBOARD);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing a window hides it; the app lives in the menu bar.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.label() == REVIEW {
                    flow::cancel_review(window.app_handle());
                } else {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_review,
            commands::finish_review,
            commands::cancel_review,
            commands::mask_text,
            commands::preview,
            commands::get_status,
            commands::get_settings,
            commands::save_settings,
            commands::get_global,
            commands::save_global,
            commands::list_engagements,
            commands::get_engagement,
            commands::save_engagement,
            commands::delete_engagement,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the app");

    app.run(|_, event| {
        // Keep running with every window closed; only "Quit" exits.
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
