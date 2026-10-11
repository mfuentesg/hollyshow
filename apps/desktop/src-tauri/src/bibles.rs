//! Bible versions: first-run installation of the bundled public-domain texts, the catalog the
//! settings screen lists, and user imports.

use crate::state::{blocking, AppState};
use base64::Engine as _;
use hollyshow_bible::import::{importer_for, HsbImporter};
use hollyshow_bible::{import_auto, install_bundled, BibleImporter, BibleSink, ImportMeta};
use hollyshow_core::{BibleVersion, LibraryScope, Verse, VersionCode, WireError};
use hollyshow_storage_api::BibleRepo;
use hollyshow_storage_sqlite::Store;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

fn err(code: &str, message: impl ToString) -> WireError {
    WireError {
        code: code.into(),
        message: message.to_string(),
    }
}

/// Adapter so `hollyshow_bible::install_bundled` can write into the SQLite store.
pub struct SqliteSink<'a>(pub &'a Store);

impl BibleSink for SqliteSink<'_> {
    fn installed(&self) -> Result<Vec<VersionCode>, String> {
        let versions = BibleRepo::list_versions(self.0).map_err(|e| e.to_string())?;
        Ok(versions.into_iter().map(|v| v.code).collect())
    }

    fn install(&self, version: &BibleVersion, verses: &[Verse]) -> Result<(), String> {
        self.0
            .install_bible(version, verses)
            .map_err(|e| e.to_string())
    }
}

/// Installs the bundled Bibles that are missing. Idempotent and cheap once they are installed
/// (it only lists the folder and the installed codes). Returns how many were installed.
pub fn install_bundled_bibles(state: &AppState) -> usize {
    let Some(dir) = state.paths.bundled_bibles() else {
        log::warn!(
            "no bundled Bible folder found: {:?}",
            state.paths.bible_dirs
        );
        return 0;
    };
    match install_bundled(dir, &SqliteSink(&state.store)) {
        Ok(report) => {
            for (file, why) in &report.failed {
                log::error!("bundled Bible {file} could not be installed: {why}");
            }
            if !report.installed.is_empty() {
                log::info!("installed bundled Bibles: {:?}", report.installed);
            }
            report.installed.len()
        }
        Err(e) => {
            log::error!("bundled Bible installation failed: {e}");
            0
        }
    }
}

/// Runs [`install_bundled_bibles`] without delaying the first window; tells every client when
/// the new versions are available.
pub fn install_in_background(app: AppHandle, state: AppState) {
    std::thread::Builder::new()
        .name("bible-install".into())
        .spawn(move || {
            let n = install_bundled_bibles(&state);
            if n > 0 {
                state.app.library_changed(LibraryScope::Bible);
                let _ = app.emit("hollyshow://bibles-installed", n);
            }
        })
        .ok();
}

// ---------------------------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub code: VersionCode,
    pub name: String,
    pub language: String,
    pub license: String,
    pub is_public_domain: bool,
    pub source: String,
    pub installed: bool,
}

/// Where each bundled text comes from (kept in sync with `resources/bibles/SOURCES.md`).
fn source_of(code: &str) -> &'static str {
    match code {
        "RVR1909" | "KJV" | "RV1865" => "scrollmapper/bible_databases (CrossWire/SWORD)",
        _ => "Incluida con HollyShow",
    }
}

/// Reads `key=value` headers of an `.hsb`/`.hsb.gz` without decoding the verses.
pub fn read_hsb_header(path: &Path) -> Option<std::collections::HashMap<String, String>> {
    let file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(4096);
    let name = path.file_name()?.to_string_lossy().into_owned();
    if name.ends_with(".gz") {
        flate2::read::GzDecoder::new(file)
            .take(8192)
            .read_to_end(&mut buf)
            .ok()?;
    } else {
        file.take(8192).read_to_end(&mut buf).ok()?;
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.lines();
    if lines.next()?.trim() != HSB_MAGIC {
        return None;
    }
    let mut out = std::collections::HashMap::new();
    for line in lines {
        if line.trim().is_empty() {
            break;
        }
        let (k, v) = line.split_once('=')?;
        out.insert(k.trim().to_owned(), v.trim().to_owned());
    }
    Some(out)
}

const HSB_MAGIC: &str = "#hsbible 1";

fn bundled_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".hsb.gz") || n.ends_with(".hsb"))
        })
        .collect();
    files.sort();
    files
}

/// Bundled files plus anything the user imported.
pub fn catalog(state: &AppState) -> Result<Vec<CatalogEntry>, WireError> {
    let installed = BibleRepo::list_versions(&*state.store).map_err(|e| err("storage", e))?;
    let mut out: Vec<CatalogEntry> = Vec::new();
    if let Some(dir) = state.paths.bundled_bibles() {
        for path in bundled_files(dir) {
            let Some(h) = read_hsb_header(&path) else {
                continue;
            };
            let Some(code) = h.get("code").cloned() else {
                continue;
            };
            out.push(CatalogEntry {
                installed: installed.iter().any(|v| v.code.0 == code),
                name: h.get("name").cloned().unwrap_or_else(|| code.clone()),
                language: h.get("language").cloned().unwrap_or_else(|| "und".into()),
                license: h.get("license").cloned().unwrap_or_default(),
                is_public_domain: h.get("public_domain").is_some_and(|v| v == "true"),
                source: source_of(&code).into(),
                code: VersionCode(code),
            });
        }
    }
    for v in &installed {
        if !out.iter().any(|e| e.code == v.code) {
            out.push(CatalogEntry {
                code: v.code.clone(),
                name: v.name.clone(),
                language: v.language.clone(),
                license: v.license.clone(),
                is_public_domain: v.is_public_domain,
                source: "Importada".into(),
                installed: true,
            });
        }
    }
    Ok(out)
}

/// Installs one bundled version again (it was removed) by code.
pub fn install_one(state: &AppState, code: &VersionCode) -> Result<BibleVersion, WireError> {
    let dir = state
        .paths
        .bundled_bibles()
        .ok_or_else(|| err("not_found", "no hay Biblias incluidas"))?;
    for path in bundled_files(dir) {
        if read_hsb_header(&path)
            .and_then(|h| h.get("code").cloned())
            .as_deref()
            != Some(code.0.as_str())
        {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|e| err("io", e))?;
        let mut b = HsbImporter::new(ImportMeta::default())
            .import(&bytes)
            .map_err(|e| err("invalid", e))?;
        // Reinstalled versions never steal the default / secondary slots.
        b.version.is_default = false;
        b.version.is_secondary = false;
        state
            .store
            .install_bible(&b.version, &b.verses)
            .map_err(|e| err("storage", e))?;
        return state
            .store
            .get_version(code)
            .map_err(|e| err("storage", e))?
            .ok_or_else(|| err("not_found", code.0.clone()));
    }
    Err(err("not_found", format!("version {} no incluida", code.0)))
}

// ---------------------------------------------------------------------------------------------
// User imports
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub file_name: String,
    pub data_base64: String,
    pub format: Option<String>,
    pub code: Option<String>,
    pub name: Option<String>,
    pub language: Option<String>,
    pub is_public_domain: Option<bool>,
    pub license: Option<String>,
}

/// Largest Bible file accepted (a full OSIS Bible is ~10 MB).
const MAX_IMPORT_BYTES: usize = 64 * 1024 * 1024;

pub fn import(state: &AppState, req: ImportRequest) -> Result<BibleVersion, WireError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(req.data_base64.as_bytes())
        .map_err(|e| err("invalid", format!("{}: base64: {e}", req.file_name)))?;
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(err("invalid", "el archivo es demasiado grande"));
    }
    let clean = |s: Option<String>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let meta = ImportMeta {
        code: clean(req.code),
        name: clean(req.name),
        language: clean(req.language),
        license: clean(req.license),
        public_domain: req.is_public_domain,
        ..ImportMeta::default()
    };
    let fixed = req
        .format
        .as_deref()
        .filter(|f| !matches!(*f, "auto" | "tabular"));
    let imported = match fixed.and_then(|f| importer_for(f, meta.clone())) {
        Some(importer) => importer.import(&bytes),
        None => import_auto(&bytes, meta),
    }
    .map_err(|e| err("invalid", format!("{}: {e}", req.file_name)))?;
    if imported.verses.is_empty() {
        return Err(err(
            "invalid",
            format!("{}: no contiene versiculos", req.file_name),
        ));
    }
    state
        .store
        .install_bible(&imported.version, &imported.verses)
        .map_err(|e| err("storage", e))?;
    state
        .store
        .get_version(&imported.version.code)
        .map_err(|e| err("storage", e))?
        .ok_or_else(|| err("not_found", imported.version.code.0.clone()))
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

fn changed(state: &AppState) {
    state.app.library_changed(LibraryScope::Bible);
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionPatch {
    pub is_enabled: Option<bool>,
    pub is_default: Option<bool>,
    pub is_secondary: Option<bool>,
}

#[tauri::command(async)]
pub fn list_bible_catalog(state: State<'_, AppState>) -> Result<Vec<CatalogEntry>, WireError> {
    catalog(&state)
}

#[tauri::command]
pub async fn install_bible(
    state: State<'_, AppState>,
    code: VersionCode,
) -> Result<BibleVersion, WireError> {
    state.ensure_unlocked()?;
    let inner = state.inner().clone();
    let v = blocking(move || {
        let v = install_one(&inner, &code)?;
        changed(&inner);
        Ok::<_, WireError>(v)
    })
    .await??;
    Ok(v)
}

#[tauri::command]
pub async fn import_bible(
    state: State<'_, AppState>,
    req: ImportRequest,
) -> Result<BibleVersion, WireError> {
    state.ensure_unlocked()?;
    let inner = state.inner().clone();
    let v = blocking(move || {
        let v = import(&inner, req)?;
        changed(&inner);
        Ok::<_, WireError>(v)
    })
    .await??;
    Ok(v)
}

#[tauri::command(async)]
pub fn remove_bible(state: State<'_, AppState>, code: VersionCode) -> Result<(), WireError> {
    state.ensure_unlocked()?;
    BibleRepo::remove_version(&*state.store, &code).map_err(|e| err("storage", e))?;
    changed(&state);
    Ok(())
}

#[tauri::command(async)]
pub fn update_bible_version(
    state: State<'_, AppState>,
    code: VersionCode,
    patch: VersionPatch,
) -> Result<Vec<BibleVersion>, WireError> {
    state.ensure_unlocked()?;
    let repo: &dyn BibleRepo = &*state.store;
    let s = |e: hollyshow_storage_api::StorageError| err("storage", e);
    if let Some(on) = patch.is_enabled {
        repo.set_enabled(&code, on).map_err(s)?;
    }
    if patch.is_default == Some(true) {
        repo.set_default(&code).map_err(s)?;
    }
    match patch.is_secondary {
        Some(true) => repo.set_secondary(Some(&code)).map_err(s)?,
        Some(false) => {
            let is_current = repo
                .get_version(&code)
                .map_err(s)?
                .is_some_and(|v| v.is_secondary);
            if is_current {
                repo.set_secondary(None).map_err(s)?;
            }
        }
        None => {}
    }
    changed(&state);
    repo.list_versions().map_err(s)
}

#[tauri::command(async)]
pub fn reorder_bible_versions(
    state: State<'_, AppState>,
    codes: Vec<VersionCode>,
) -> Result<Vec<BibleVersion>, WireError> {
    state.ensure_unlocked()?;
    state
        .store
        .set_bible_order(&codes)
        .map_err(|e| err("storage", e))?;
    changed(&state);
    BibleRepo::list_versions(&*state.store).map_err(|e| err("storage", e))
}
