//! `tauri::generate_context!` needs `frontendDist` (`../dist`) at compile time. It is a build
//! product, so on a fresh clone this writes a placeholder to keep `cargo check/test/clippy`
//! working; `pnpm build:web` (or `tauri build`) replaces it. Release builds refuse it.

use std::fs;
use std::path::PathBuf;

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
    tauri_build::build()
}
