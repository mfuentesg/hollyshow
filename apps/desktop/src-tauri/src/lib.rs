//! HollyShow desktop shell. Wiring only: windows, IPC and the engine -> webview event bridge.

mod commands;
mod state;
mod windows;

use hollyshow_core::Update;
use state::AppState;
use tauri::{Emitter, Manager};
use tokio::sync::broadcast::error::RecvError;

/// Webview event carrying an [`Update`]. Mirrored by `UPDATE_EVENT` in apps/desktop/src/api/tauri.ts.
pub const UPDATE_EVENT: &str = "hollyshow://update";

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::dispatch,
            commands::query,
            commands::get_snapshot,
            windows::open_output,
            windows::open_stage,
            windows::close_window,
            windows::list_monitors,
        ])
        .setup(|app| {
            bridge_updates(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running HollyShow");
}

/// Forwards every engine update to all webviews. Slow consumers that lag are resynced with the
/// current view instead of replaying missed events, so a window can never get stuck on stale state.
fn bridge_updates(app: tauri::AppHandle) {
    let engine = app.state::<AppState>().engine.clone();
    let mut rx = engine.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(update) => {
                    let _ = app.emit(UPDATE_EVENT, &update);
                }
                Err(RecvError::Lagged(missed)) => {
                    log::warn!("event bridge lagged by {missed} updates; resyncing");
                    let view = engine.view();
                    let _ = app.emit(
                        UPDATE_EVENT,
                        &Update {
                            version: view.version,
                            view,
                            events: vec![],
                        },
                    );
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}
