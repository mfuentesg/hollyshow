//! `tauri::generate_context!` needs `frontendDist` (`../dist`) at compile time. It is a build
//! product, so on a fresh clone this writes a placeholder to keep `cargo check/test/clippy`
//! working; `pnpm build:web` (or `tauri build`) replaces it. Release builds refuse it.

use std::fs;
use std::path::PathBuf;
use tauri_build::{AppManifest, Attributes};

/// Every IPC command of the app. Declaring them makes Tauri generate one `allow-<name>`
/// permission each, so a capability can grant a window only the commands it needs (the output and
/// stage windows only render). `tests::manifest_matches_handler` keeps this list equal to the
/// `generate_handler!` list in `src/lib.rs`.
const COMMANDS: &[&str] = &[
    "dispatch",
    "query",
    "get_snapshot",
    "get_setting",
    "set_setting",
    "get_active_playlist_id",
    "save_song",
    "delete_song",
    "import_songs",
    "save_playlist",
    "delete_playlist",
    "save_theme",
    "delete_theme",
    "get_pairing",
    "regenerate_pin",
    "set_remote_enabled",
    "set_pairing_role",
    "list_devices",
    "kick_device",
    "health",
    "report_output_stats",
    "startup_info",
    "usage_report",
    "export_library",
    "import_library",
    "list_bible_catalog",
    "install_bible",
    "import_bible",
    "remove_bible",
    "update_bible_version",
    "reorder_bible_versions",
    "open_output",
    "open_stage",
    "close_window",
    "list_monitors",
    "output_monitors",
    "media_search",
    "media_categories",
    "media_download",
    "media_list",
    "media_remove",
    "media_credits",
    "update_check",
    "update_download",
    "update_status",
    "update_install_now",
    "update_settings_get",
    "update_settings_set",
];

fn main() {
    let dist = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../dist");
    println!("cargo:rerun-if-changed=../dist");
    if !dist.join("index.html").is_file() {
        if std::env::var("PROFILE").as_deref() == Ok("release") {
            panic!("apps/desktop/dist is missing: build the frontend first (`pnpm build:web`)");
        }
        fs::create_dir_all(&dist).expect("create dist");
        fs::write(
            dist.join("index.html"),
            "<!doctype html><meta charset=\"utf-8\"><title>HollyShow</title>\
             <body data-hollyshow-placeholder>Frontend not built. Run pnpm build:web.",
        )
        .expect("write placeholder");
    }
    tauri_build::try_build(Attributes::new().app_manifest(AppManifest::new().commands(COMMANDS)))
        .expect("tauri build");
}
