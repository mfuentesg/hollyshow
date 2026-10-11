//! Host operations the operator window needs beyond show control: settings, library writes,
//! remote pairing, health and start-up information. Mirrors `HostApi` in
//! `apps/desktop/src/api/host.ts` (command names in `HOST_COMMANDS`).
//!
//! Every library write (a) is refused in service mode ("modo culto") and (b) tells the app layer
//! what changed so the show is rebuilt and every client (windows and phones) refreshes.

use crate::remote::{PairingInfo, RemoteDevice, ROLE_KEY};
use crate::state::{blocking, now_ms, AppState, StartupInfo};
use hollyshow_core::{LibraryScope, Playlist, PlaylistId, Song, SongId, Theme, ThemeId, WireError};
use hollyshow_songs::{
    ChordProImporter, ImportError, OpenLyricsImporter, OpenSongImporter, SongImporter, TextImporter,
};
use hollyshow_storage_sqlite::{ImportMode, ImportReport, LibraryOptions, UsageRow};
use serde::{Deserialize, Serialize};
use tauri::State;

fn err(code: &str, message: impl ToString) -> WireError {
    WireError {
        code: code.into(),
        message: message.to_string(),
    }
}

fn storage(e: hollyshow_storage_api::StorageError) -> WireError {
    use hollyshow_storage_api::StorageError as E;
    match e {
        E::NotFound(m) => err("not_found", m),
        E::Conflict(m) => err("conflict", m),
        E::Invalid(m) => err("invalid", m),
        E::Backend(m) => {
            log::error!("storage failure: {m}");
            err("storage", "no se pudo acceder a la biblioteca")
        }
    }
}

// ---- settings --------------------------------------------------------------------------------

/// Keys the webview must not touch: session recovery, paired devices and crash detection are
/// owned by the host.
fn is_reserved(key: &str) -> bool {
    key.starts_with("session.") || key.starts_with("server.") || key.starts_with("windows.")
}

#[tauri::command(async)]
pub fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>, WireError> {
    if is_reserved(&key) {
        return Ok(None);
    }
    state.app.repos().settings.get(&key).map_err(storage)
}

#[tauri::command(async)]
pub fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), WireError> {
    if is_reserved(&key) || key.len() > 128 || value.len() > 64 * 1024 {
        return Err(err("forbidden", "ajuste no permitido"));
    }
    state
        .app
        .repos()
        .settings
        .set(&key, &value)
        .map_err(storage)?;
    if key == "media.openverseToken" {
        let t = value.trim();
        state
            .media
            .set_openverse_token((!t.is_empty()).then(|| t.to_owned()));
    }
    Ok(())
}

#[tauri::command(async)]
pub fn get_active_playlist_id(state: State<'_, AppState>) -> Option<PlaylistId> {
    state.app.active_playlist()
}

// ---- songs ---------------------------------------------------------------------------------

#[tauri::command(async)]
pub fn save_song(state: State<'_, AppState>, mut song: Song) -> Result<Song, WireError> {
    state.ensure_unlocked()?;
    if song.title.trim().is_empty() {
        return Err(err("invalid", "la cancion necesita un titulo"));
    }
    let now = now_ms();
    if song.created_at == 0 {
        song.created_at = now;
    }
    song.updated_at = now;
    state.app.repos().songs.save(&song).map_err(storage)?;
    state.app.library_changed(LibraryScope::Songs);
    Ok(song)
}

#[tauri::command(async)]
pub fn delete_song(state: State<'_, AppState>, id: SongId) -> Result<(), WireError> {
    state.ensure_unlocked()?;
    state.app.repos().songs.delete(&id).map_err(storage)?;
    state.app.library_changed(LibraryScope::Songs);
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct SongImportFile {
    pub name: String,
    pub content: String,
    pub format: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SongImportResult {
    pub imported: Vec<Song>,
    pub failed: Vec<(String, String)>,
}

/// Largest song file accepted.
const MAX_SONG_FILE: usize = 4 * 1024 * 1024;

fn parse_songs(file: &SongImportFile) -> Result<Vec<Song>, ImportError> {
    match file.format.as_deref() {
        Some("openlyrics") => OpenLyricsImporter::default().import(&file.content),
        Some("chordpro") => ChordProImporter::default().import(&file.content),
        Some("opensong") => OpenSongImporter::default().import(&file.content),
        Some("text") => TextImporter::default().import(&file.content),
        _ => hollyshow_songs::import_auto(&file.content),
    }
}

/// Parses and stores every file; a file that fails never blocks the others.
pub fn import_song_files(
    repos: &hollyshow_storage_api::Repos,
    files: &[SongImportFile],
    now: i64,
) -> SongImportResult {
    let mut result = SongImportResult {
        imported: vec![],
        failed: vec![],
    };
    for f in files {
        if f.content.len() > MAX_SONG_FILE {
            result
                .failed
                .push((f.name.clone(), "archivo demasiado grande".into()));
            continue;
        }
        match parse_songs(f) {
            Ok(songs) if songs.is_empty() => result
                .failed
                .push((f.name.clone(), "no se encontro ninguna cancion".into())),
            Ok(songs) => {
                for mut s in songs {
                    if s.created_at == 0 {
                        s.created_at = now;
                    }
                    s.updated_at = now;
                    match repos.songs.save(&s) {
                        Ok(()) => result.imported.push(s),
                        Err(e) => result.failed.push((f.name.clone(), e.to_string())),
                    }
                }
            }
            Err(e) => result.failed.push((f.name.clone(), e.to_string())),
        }
    }
    result
}

#[tauri::command(async)]
pub fn import_songs(
    state: State<'_, AppState>,
    files: Vec<SongImportFile>,
) -> Result<SongImportResult, WireError> {
    state.ensure_unlocked()?;
    let r = import_song_files(state.app.repos(), &files, now_ms());
    if !r.imported.is_empty() {
        state.app.library_changed(LibraryScope::Songs);
    }
    Ok(r)
}

// ---- playlists and themes ---------------------------------------------------------------------

#[tauri::command(async)]
pub fn save_playlist(
    state: State<'_, AppState>,
    playlist: Playlist,
) -> Result<Playlist, WireError> {
    state.ensure_unlocked()?;
    state
        .app
        .repos()
        .playlists
        .save(&playlist)
        .map_err(storage)?;
    state.app.playlist_saved(&playlist.id);
    Ok(playlist)
}

#[tauri::command(async)]
pub fn delete_playlist(state: State<'_, AppState>, id: PlaylistId) -> Result<(), WireError> {
    state.ensure_unlocked()?;
    if state.app.active_playlist().as_ref() == Some(&id) {
        return Err(err("conflict", "no se puede borrar el servicio abierto"));
    }
    state.app.repos().playlists.delete(&id).map_err(storage)?;
    state.app.playlist_saved(&id);
    Ok(())
}

#[tauri::command(async)]
pub fn save_theme(state: State<'_, AppState>, theme: Theme) -> Result<Theme, WireError> {
    state.ensure_unlocked()?;
    if theme.id.as_str().is_empty() {
        return Err(err("invalid", "el tema necesita un id"));
    }
    state.app.repos().themes.save(&theme).map_err(storage)?;
    state.app.library_changed(LibraryScope::Themes);
    Ok(theme)
}

#[tauri::command(async)]
pub fn delete_theme(state: State<'_, AppState>, id: ThemeId) -> Result<(), WireError> {
    state.ensure_unlocked()?;
    if id.as_str() == "default" {
        return Err(err("conflict", "el tema predeterminado no se puede borrar"));
    }
    state.app.repos().themes.delete(&id).map_err(storage)?;
    state.app.library_changed(LibraryScope::Themes);
    Ok(())
}

// ---- CCLI report and library backup -----------------------------------------------------------

/// Largest library file accepted by `import_library`.
const MAX_LIBRARY_FILE: usize = 256 * 1024 * 1024;

/// Songs shown in `[from_ms, to_ms)`, most shown first (the CCLI report).
#[tauri::command]
pub async fn usage_report(
    state: State<'_, AppState>,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<UsageRow>, WireError> {
    let store = state.store.clone();
    blocking(move || store.usage_report(from_ms, to_ms))
        .await?
        .map_err(storage)
}

/// Writes the whole library to `<data dir>/exports/hollyshow-library-<ms>.json` and returns the
/// path. Paired devices and other machine-owned settings are never included.
#[tauri::command]
pub async fn export_library(
    state: State<'_, AppState>,
    include_bibles: bool,
    include_usage: bool,
) -> Result<String, WireError> {
    let store = state.store.clone();
    let dir = state.paths.data_dir.join("exports");
    blocking(move || {
        let json = store
            .export_library_json(LibraryOptions {
                include_bibles,
                include_usage,
            })
            .map_err(storage)?;
        std::fs::create_dir_all(&dir).map_err(|e| err("io", e))?;
        let path = dir.join(format!("hollyshow-library-{}.json", now_ms()));
        std::fs::write(&path, json).map_err(|e| err("io", e))?;
        Ok::<_, WireError>(path.to_string_lossy().into_owned())
    })
    .await?
}

/// Imports a library file. `replace` wipes songs, themes, services and settings first; the
/// default merges by id and keeps the newer song.
#[tauri::command]
pub async fn import_library(
    state: State<'_, AppState>,
    json: String,
    replace: bool,
) -> Result<ImportReport, WireError> {
    state.ensure_unlocked()?;
    if json.len() > MAX_LIBRARY_FILE {
        return Err(err("invalid", "el archivo es demasiado grande"));
    }
    let inner = state.inner().clone();
    blocking(move || {
        let mode = if replace {
            ImportMode::Replace
        } else {
            ImportMode::Merge
        };
        let report = inner
            .store
            .import_library_json(&json, mode)
            .map_err(storage)?;
        inner.app.library_changed(LibraryScope::Themes);
        if report.bibles > 0 {
            inner.app.library_changed(LibraryScope::Bible);
        }
        // Rebuilds the open service (its songs may have changed) and tells every client.
        inner.app.library_changed(LibraryScope::Songs);
        Ok::<_, WireError>(report)
    })
    .await?
}

// ---- remote control --------------------------------------------------------------------------

#[tauri::command(async)]
pub fn get_pairing(state: State<'_, AppState>) -> Result<PairingInfo, WireError> {
    state.remote.pairing(false).map_err(|e| err("remote", e))
}

#[tauri::command(async)]
pub fn regenerate_pin(state: State<'_, AppState>) -> Result<PairingInfo, WireError> {
    state.remote.pairing(true).map_err(|e| err("remote", e))
}

#[tauri::command]
pub async fn set_remote_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<PairingInfo, WireError> {
    let inner = state.inner().clone();
    blocking(move || {
        inner
            .app
            .repos()
            .settings
            .set(crate::remote::ENABLED_KEY, if enabled { "1" } else { "0" })
    })
    .await?
    .map_err(storage)?;
    if enabled {
        state.remote.start().await.map_err(|e| err("remote", e))?;
    } else {
        state.remote.stop().await;
    }
    state.remote.pairing(false).map_err(|e| err("remote", e))
}

#[tauri::command(async)]
pub fn list_devices(state: State<'_, AppState>) -> Vec<RemoteDevice> {
    state.remote.devices()
}

#[tauri::command(async)]
pub fn kick_device(state: State<'_, AppState>, id: String) -> Result<(), WireError> {
    if state.remote.kick(&id) {
        Ok(())
    } else {
        Err(err("not_found", "dispositivo desconocido"))
    }
}

/// Changing the role a new PIN grants also invalidates the old PIN.
#[tauri::command(async)]
pub fn set_pairing_role(
    state: State<'_, AppState>,
    role: String,
) -> Result<PairingInfo, WireError> {
    if !matches!(role.as_str(), "operator" | "pastor" | "musician" | "stage") {
        return Err(err("invalid", "rol desconocido"));
    }
    state
        .app
        .repos()
        .settings
        .set(ROLE_KEY, &role)
        .map_err(storage)?;
    state.remote.pairing(true).map_err(|e| err("remote", e))
}

// ---- health and start-up ------------------------------------------------------------------

#[tauri::command(async)]
pub fn health(state: State<'_, AppState>) -> crate::health::HealthInfo {
    state.health.sample()
}

/// Frame statistics measured by an output window.
#[tauri::command(async)]
pub fn report_output_stats(state: State<'_, AppState>, fps: f64, dropped: u32) {
    state.health.report_frames(fps, dropped);
}

#[tauri::command(async)]
pub fn startup_info(state: State<'_, AppState>) -> StartupInfo {
    state.startup.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_storage_api::memory::MemoryStore;

    #[test]
    fn webviews_cannot_read_or_write_host_owned_settings() {
        for k in [
            "session.v1",
            "session.running",
            "server.devices.v1",
            "windows.v1",
        ] {
            assert!(is_reserved(k), "{k}");
        }
        for k in ["ui.language", "media.openverseToken", "remote.pairingRole"] {
            assert!(!is_reserved(k), "{k}");
        }
    }

    const OPENLYRICS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<song xmlns="http://openlyrics.info/namespace/2009/song" version="0.8">
 <properties><titles><title>Grande es tu fidelidad</title></titles></properties>
 <lyrics><verse name="v1"><lines>Gran fidelidad<br/>Oh Dios mi Padre</lines></verse></lyrics>
</song>"#;

    #[test]
    fn importing_stores_good_files_and_reports_bad_ones() {
        let repos = MemoryStore::repos();
        let files = vec![
            SongImportFile {
                name: "a.xml".into(),
                content: OPENLYRICS.into(),
                format: None,
            },
            SongImportFile {
                name: "b.txt".into(),
                content: "Sublime gracia\n\nVerso 1\nSublime gracia del Senor\n".into(),
                format: Some("text".into()),
            },
            SongImportFile {
                name: "broken.xml".into(),
                content: "<song><lyrics>".into(),
                format: Some("openlyrics".into()),
            },
            SongImportFile {
                name: "huge.txt".into(),
                content: "x".repeat(MAX_SONG_FILE + 1),
                format: None,
            },
        ];
        let r = import_song_files(&repos, &files, 42);
        assert_eq!(r.imported.len(), 2, "{:?}", r.failed);
        assert_eq!(r.failed.len(), 2);
        assert!(r.failed.iter().any(|(n, _)| n == "broken.xml"));
        assert!(r.failed.iter().any(|(n, _)| n == "huge.txt"));
        assert!(r.imported.iter().all(|s| s.updated_at == 42));
        assert_eq!(repos.songs.search("", 10).unwrap().len(), 2);
        // Importing the same file again does not duplicate the song.
        import_song_files(&repos, &files[..1], 43);
        assert_eq!(repos.songs.search("", 10).unwrap().len(), 2);
    }
}
