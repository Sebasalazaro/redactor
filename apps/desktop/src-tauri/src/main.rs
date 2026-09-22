//! redactor desktop: a menu bar app that redacts the clipboard on a global
//! hotkey and shows the result for review before it is pasted anywhere.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod commands;
mod dto;
mod flow;
mod memory;
mod settings;
mod state;
mod tray;

use std::time::Duration;

use redactor_core::Store;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::flow::{DASHBOARD, REVIEW};
use crate::state::AppState;

/// `redactor-desktop --ner-worker <model dir>` runs the AI model for the
/// app in a child process (see `ai.rs`), then exits.
const WORKER_FLAG: &str = "--ner-worker";

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == WORKER_FLAG) {
        let dir = std::path::PathBuf::from(args.get(2).cloned().unwrap_or_default());
        let stdin = std::io::stdin().lock();
        let stdout = std::io::stdout().lock();
        let code = i32::from(redactor_ner::worker::serve(&dir, stdin, stdout).is_err());
        std::process::exit(code);
    }

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

            // Housekeeping: forget the last redaction once it expires and
            // unload the AI model when idle, even if nobody looks.
            let handle = handle.clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_secs(30));
                    let state = handle.state::<AppState>();
                    let expired = state.forget_expired();
                    let unloaded = state.ai.unload_if_idle();
                    if expired || unloaded {
                        let _ = handle.emit_to(DASHBOARD, "overview-changed", ());
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // The app lives in the menu bar: closing a window unloads it
            // (or hides it, if the user prefers faster reopening).
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if window.label() == REVIEW {
                    api.prevent_close();
                    flow::cancel_review(app);
                } else if app.state::<AppState>().settings().unload_windows {
                    // Let the page flush unsaved edits first.
                    api.prevent_close();
                    flow::request_dashboard_close(app);
                } else {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_review,
            commands::finish_review,
            commands::cancel_review,
            commands::mask_text,
            commands::get_overview,
            commands::copy_last,
            commands::clear_last,
            commands::clear_clipboard,
            commands::free_memory,
            commands::create_engagement,
            commands::redact_now,
            commands::close_window,
            commands::deep_scan,
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
