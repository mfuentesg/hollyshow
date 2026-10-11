//! Converts any supported Bible file into the compact bundled format (`.hsb.gz`) and verifies
//! the canon.
//!
//! ```text
//! cargo run -p hollyshow-bible --example pack_bible --release -- \
//!   <input> <output.hsb.gz> <code> <name> <language> <public_domain:true|false> \
//!   <license> <copyright> [versification]
//! ```

use hollyshow_bible::import::{write_hsb, ImportMeta};
use hollyshow_bible::{canon_report, import_auto};
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 8 {
        eprintln!("usage: pack_bible <input> <output.hsb.gz> <code> <name> <language> <public_domain> <license> <copyright> [versification]");
        std::process::exit(2);
    }
    let bytes = std::fs::read(&a[0]).expect("read input");
    let meta = ImportMeta::default()
        .with_code(&a[2])
        .with_name(&a[3])
        .with_language(&a[4])
        .with_public_domain(a[5] == "true")
        .with_license(&a[6])
        .with_copyright(&a[7])
        .with_versification(a.get(8).map_or("kjv", String::as_str));
    let bible = import_auto(&bytes, meta).expect("import");
    let report = canon_report(&bible.verses);
    println!(
        "{}: {} verses, {} missing from the standard canon, {} outside it",
        bible.version.code.0,
        report.total,
        report.missing.len(),
        report.extra.len()
    );
    for m in report.missing.iter().take(20) {
        println!("  missing {m}");
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(write_hsb(&bible).as_bytes()).expect("gzip");
    let gz = gz.finish().expect("gzip");
    std::fs::write(&a[1], &gz).expect("write output");
    println!("wrote {} ({} bytes)", a[1], gz.len());
}
