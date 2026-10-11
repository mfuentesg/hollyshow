//! First-run installation of the bundled Bibles through a minimal storage port.
//!
//! The bundled files live in `apps/desktop/src-tauri/resources/bibles/*.hsb.gz` (see `SOURCES.md`
//! there) and are shipped as Tauri resources. The shell calls [`install_bundled`] once at start-up
//! with the resolved resource directory and an adapter over its store. The call is idempotent:
//! versions already installed are left alone (the user's enabled/default choices survive).

use crate::import::{HsbImporter, ImportMeta};
use crate::{BibleError, BibleImporter};
use hollyshow_core::{BibleVersion, Verse, VersionCode};
use std::path::Path;

/// What installation needs from the store. `hollyshow_storage_sqlite::Store` already offers
/// `install_bible(&BibleVersion, &[Verse])` and `BibleRepo::list_versions`; the shell wraps them:
///
/// ```ignore
/// struct SqliteSink(Arc<Store>);
/// impl BibleSink for SqliteSink {
///     fn installed(&self) -> Result<Vec<VersionCode>, String> {
///         let versions = BibleRepo::list_versions(&*self.0).map_err(|e| e.to_string())?;
///         Ok(versions.into_iter().map(|v| v.code).collect())
///     }
///     fn install(&self, v: &BibleVersion, verses: &[Verse]) -> Result<(), String> {
///         self.0.install_bible(v, verses).map_err(|e| e.to_string())
///     }
/// }
/// ```
pub trait BibleSink {
    fn installed(&self) -> Result<Vec<VersionCode>, String>;
    /// Replaces/creates `version` with `verses` (canonical order), atomically.
    fn install(&self, version: &BibleVersion, verses: &[Verse]) -> Result<(), String>;
}

/// Presentation order of the bundled versions; the first becomes the default, the second the
/// secondary (parallel) version when they are installed on a fresh library.
pub const BUNDLED_ORDER: &[&str] = &["RVR1909", "KJV", "RV1865"];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct InstallReport {
    pub installed: Vec<VersionCode>,
    pub already_present: Vec<VersionCode>,
    /// `(file name, reason)` for files that could not be read or stored.
    pub failed: Vec<(String, String)>,
}

/// Installs every `*.hsb.gz` found in `dir` that is not installed yet. A broken file is
/// reported in [`InstallReport::failed`] and never prevents the others from installing.
pub fn install_bundled(dir: &Path, sink: &dyn BibleSink) -> Result<InstallReport, BibleError> {
    let mut report = InstallReport::default();
    let present = sink.installed().map_err(BibleError::Install)?;
    let first_run = present.is_empty();
    let entries = std::fs::read_dir(dir)
        .map_err(|e| BibleError::Install(format!("{}: {e}", dir.display())))?;
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".hsb.gz") || n.ends_with(".hsb"))
        })
        .collect();
    files.sort();
    let mut loaded = Vec::new();
    for path in files {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let parsed = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                HsbImporter::new(ImportMeta::default())
                    .import(&bytes)
                    .map_err(|e| e.to_string())
            });
        match parsed {
            Ok(b) => loaded.push(b),
            Err(e) => report.failed.push((name, e)),
        }
    }
    let rank = |code: &str| {
        BUNDLED_ORDER
            .iter()
            .position(|c| *c == code)
            .unwrap_or(BUNDLED_ORDER.len())
    };
    loaded.sort_by_key(|b| (rank(&b.version.code.0), b.version.code.0.clone()));
    for mut b in loaded {
        if present.contains(&b.version.code) {
            report.already_present.push(b.version.code);
            continue;
        }
        b.version.sort_order = i64::try_from(rank(&b.version.code.0)).unwrap_or(i64::MAX);
        if first_run {
            b.version.is_default = b.version.code.0 == BUNDLED_ORDER[0];
            b.version.is_secondary = b.version.code.0 == BUNDLED_ORDER[1];
        }
        match sink.install(&b.version, &b.verses) {
            Ok(()) => report.installed.push(b.version.code),
            Err(e) => report.failed.push((b.version.code.0, e)),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::write_hsb;
    use crate::ImportedBible;
    use hollyshow_core::BibleRef;
    use std::io::Write;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Mem(Mutex<Vec<(BibleVersion, usize)>>);
    impl BibleSink for Mem {
        fn installed(&self) -> Result<Vec<VersionCode>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|(v, _)| v.code.clone())
                .collect())
        }
        fn install(&self, v: &BibleVersion, verses: &[Verse]) -> Result<(), String> {
            self.0.lock().unwrap().push((v.clone(), verses.len()));
            Ok(())
        }
    }

    fn bible(code: &str) -> ImportedBible {
        let mut b = crate::import::CsvImporter::new(
            ImportMeta::default()
                .with_code(code)
                .with_language("en")
                .with_public_domain(true),
        )
        .import(b"Gen,1,1,In the beginning\nGen,1,2,And the earth\n")
        .unwrap();
        b.verses[0].reference = BibleRef::verse("Gen", 1, 1);
        b
    }

    fn write(dir: &Path, name: &str, b: &ImportedBible) {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(write_hsb(b).as_bytes()).unwrap();
        std::fs::write(dir.join(name), gz.finish().unwrap()).unwrap();
    }

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("hs-bible-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn installs_in_order_once_and_marks_defaults() {
        let dir = tmp("install");
        write(&dir, "kjv.hsb.gz", &bible("KJV"));
        write(&dir, "rvr.hsb.gz", &bible("RVR1909"));
        std::fs::write(dir.join("broken.hsb.gz"), b"nope").unwrap();
        std::fs::write(dir.join("readme.txt"), b"ignored").unwrap();
        let sink = Mem::default();
        let r = install_bundled(&dir, &sink).unwrap();
        assert_eq!(
            r.installed,
            [VersionCode("RVR1909".into()), VersionCode("KJV".into())]
        );
        assert_eq!(r.failed.len(), 1);
        let rows = sink.0.lock().unwrap().clone();
        assert!(rows[0].0.is_default && !rows[0].0.is_secondary);
        assert!(rows[1].0.is_secondary && !rows[1].0.is_default);
        assert_eq!(rows[0].1, 2);
        drop(rows);
        // Second run: nothing to do, nothing duplicated.
        let r2 = install_bundled(&dir, &sink).unwrap();
        assert!(r2.installed.is_empty());
        assert_eq!(r2.already_present.len(), 2);
        assert_eq!(sink.0.lock().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_library_keeps_its_defaults() {
        let dir = tmp("existing");
        write(&dir, "rvr.hsb.gz", &bible("RVR1909"));
        let sink = Mem::default();
        sink.install(&bible("OTHER").version, &[]).unwrap();
        let r = install_bundled(&dir, &sink).unwrap();
        assert_eq!(r.installed.len(), 1);
        let rows = sink.0.lock().unwrap().clone();
        assert!(!rows[1].0.is_default);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_is_an_error() {
        let r = install_bundled(Path::new("/definitely/not/here"), &Mem::default());
        assert!(matches!(r, Err(BibleError::Install(_))));
    }
}
