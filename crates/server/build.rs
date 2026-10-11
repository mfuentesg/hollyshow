//! Stages the remote web app for `rust-embed`.
//!
//! `apps/remote/dist` is a build product (gitignored). To keep `cargo check/test/clippy` working
//! on a fresh clone, this copies it into `OUT_DIR/remote` when present and otherwise writes a
//! placeholder page there. A release build refuses the placeholder so an installer can never
//! ship without the remote app.

use std::fs;
use std::path::{Path, PathBuf};

const PLACEHOLDER: &str = "<!doctype html><meta charset=\"utf-8\"><title>HollyShow</title>\
<body data-hollyshow-placeholder><p>Remote app not built. Run <code>pnpm build:web</code> and rebuild.</p>";

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dist = manifest.join("../../apps/remote/dist");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("remote");
    println!("cargo:rerun-if-changed={}", dist.display());
    println!("cargo:rerun-if-changed=build.rs");

    let _ = fs::remove_dir_all(&out);
    let built = dist.join("index.html").is_file();
    if built {
        copy_dir(&dist, &out).expect("copy apps/remote/dist");
    } else {
        if std::env::var("PROFILE").as_deref() == Ok("release") {
            panic!("apps/remote/dist is missing: run `pnpm build:web` before a release build");
        }
        fs::create_dir_all(&out).expect("create placeholder dir");
        fs::write(out.join("index.html"), PLACEHOLDER).expect("write placeholder");
    }
}
