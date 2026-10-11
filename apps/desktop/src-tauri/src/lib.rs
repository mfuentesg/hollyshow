//! HollyShow desktop shell. Wiring only: it composes the storage, the use-case layer, the LAN
//! server, the image bank and the updater, owns the windows and bridges engine updates to every
//! webview. Business rules live in `hollyshow-app`.

mod bibles;
mod commands;
mod health;
mod host;
mod media;
mod power;
mod recovery;
mod remote;
mod state;
mod updater;
mod windows;

use hollyshow_core::Update;
use state::{AppState, Paths};
use std::path::PathBuf;
use tauri::{Emitter, Manager};
use tokio::sync::broadcast::error::RecvError;

/// Webview event carrying an [`Update`]. Mirrored by `UPDATE_EVENT` in apps/desktop/src/api/tauri.ts.
pub const UPDATE_EVENT: &str = "hollyshow://update";

/// Folders that may hold the bundled Bibles: the installed resource directory first, then (debug
/// builds only) the source tree, so `cargo run` works without bundling.
fn bible_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        dirs.push(res.join("resources").join("bibles"));
        dirs.push(res.join("bibles"));
        dirs.push(res.join("_up_").join("resources").join("bibles"));
    }
    if cfg!(debug_assertions) {
        dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/bibles"));
    }
    dirs
}

/// Wayland compositors do not let a client choose its window position, which breaks "open the
/// output on monitor 2". When XWayland is available (`DISPLAY` is set) and the user did not pick a
/// GDK backend, run through it. A pure Wayland session keeps working (windowed placement only).
pub fn should_force_x11(
    wayland_display: Option<&str>,
    display: Option<&str>,
    gdk_backend: Option<&str>,
) -> bool {
    let set = |v: Option<&str>| v.is_some_and(|s| !s.is_empty());
    set(wayland_display) && set(display) && !set(gdk_backend)
}

pub fn run() {
    #[cfg(target_os = "linux")]
    {
        let var = |k: &str| std::env::var(k).ok();
        if should_force_x11(
            var("WAYLAND_DISPLAY").as_deref(),
            var("DISPLAY").as_deref(),
            var("GDK_BACKEND").as_deref(),
        ) {
            std::env::set_var("GDK_BACKEND", "x11");
        }
    }
    hollyshow_app::install_panic_hook();
    let builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("zbus", log::LevelFilter::Warn)
                .level_for("tracing", log::LevelFilter::Warn)
                .level_for("mdns_sd", log::LevelFilter::Warn)
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }));
    let app = updater::plugins(builder)
        .register_asynchronous_uri_scheme_protocol(media::SCHEME, media::handle)
        .on_window_event(windows::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::dispatch,
            commands::query,
            commands::get_snapshot,
            host::get_setting,
            host::set_setting,
            host::get_active_playlist_id,
            host::save_song,
            host::delete_song,
            host::import_songs,
            host::save_playlist,
            host::delete_playlist,
            host::save_theme,
            host::delete_theme,
            host::get_pairing,
            host::regenerate_pin,
            host::set_remote_enabled,
            host::set_pairing_role,
            host::list_devices,
            host::kick_device,
            host::health,
            host::report_output_stats,
            host::startup_info,
            bibles::list_bible_catalog,
            bibles::install_bible,
            bibles::import_bible,
            bibles::remove_bible,
            bibles::update_bible_version,
            bibles::reorder_bible_versions,
            windows::open_output,
            windows::open_stage,
            windows::close_window,
            windows::list_monitors,
            windows::output_monitors,
            media::media_search,
            media::media_categories,
            media::media_download,
            media::media_list,
            media::media_remove,
            media::media_credits,
            updater::update_check,
            updater::update_download,
            updater::update_status,
            updater::update_install_now,
            updater::update_settings_get,
            updater::update_settings_set,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = handle.path().app_data_dir()?;
            let paths = Paths::new(data_dir, bible_dirs(&handle));
            let state = AppState::open(paths).map_err(|e| {
                log::error!("could not start HollyShow: {e}");
                e
            })?;
            app.manage(state.clone());

            recovery::install_panic_flush(state.app.clone());
            recovery::spawn_autosave(
                state.app.clone(),
                state.store.clone(),
                state.paths.backups.clone(),
            );
            bibles::install_in_background(handle.clone(), state.clone());
            bridge_updates(handle.clone(), state.clone());
            start_remote(state.clone());
            windows::restore_windows(&handle);
            windows::watch_monitors(handle.clone());
            updater::init(&handle);
            if state.startup.crashed {
                log::warn!("the previous run did not exit cleanly; session restored");
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building HollyShow");

    app.run(|handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(state) = handle.try_state::<AppState>() {
                state.shutdown();
            }
        }
        updater::on_run_event(handle, &event);
    });
}

/// Starts the LAN server when the user wants it (default yes). A failure is logged and never
/// stops the app: the operator window works without the network.
fn start_remote(state: AppState) {
    if !state.remote.wanted() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        match state.remote.start().await {
            Ok(addr) => log::info!("remote control listening on {addr}"),
            Err(e) => log::error!("remote control unavailable: {e}"),
        }
    });
}

/// Forwards every engine update to all webviews and applies the side effects of "modo culto"
/// (keep the machine awake). Slow consumers that lag are resynced with the current view instead
/// of replaying missed events, so a window can never get stuck on stale state.
fn bridge_updates(app: tauri::AppHandle, state: AppState) {
    let engine = state.app.engine().clone();
    let mut rx = engine.subscribe();
    let mut mode = power::ModeEdge::default();
    // The restored session may already be in service mode.
    if mode.changed(engine.view().service_mode) == Some(true) {
        state.power.set_awake(true);
    }
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(update) => {
                    if let Some(on) = mode.changed(update.view.service_mode) {
                        let power = state.power.clone();
                        let _ = tauri::async_runtime::spawn_blocking(move || {
                            power.set_awake(on);
                        })
                        .await;
                    }
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

#[cfg(test)]
mod tests {
    use super::should_force_x11;

    #[test]
    fn x11_is_forced_only_on_wayland_with_xwayland_and_no_user_choice() {
        assert!(should_force_x11(Some("wayland-0"), Some(":0"), None));
        assert!(
            !should_force_x11(Some("wayland-0"), None, None),
            "no XWayland"
        );
        assert!(!should_force_x11(None, Some(":0"), None), "already X11");
        assert!(!should_force_x11(
            Some("wayland-0"),
            Some(":0"),
            Some("wayland")
        ));
        assert!(!should_force_x11(Some(""), Some(":0"), None));
    }
}
