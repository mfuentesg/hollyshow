//! Whole-library export and import as one JSON document (`hollyshow-library`, version 1).
//!
//! Contains songs, themes, services, settings (the volatile `session.*` keys are left out) and,
//! optionally, installed Bibles and the usage log. Import runs in a single transaction: either all
//! of it lands or none.

use crate::{bible, map_err, playlists, songs, Store};
use hollyshow_core::{BibleVersion, Playlist, Song, Theme, Verse};
use hollyshow_storage_api::{StorageError, StorageResult};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const LIBRARY_FORMAT: &str = "hollyshow-library";
pub const LIBRARY_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BibleExport {
    pub version: BibleVersion,
    pub verses: Vec<Verse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEntry {
    pub song_id: String,
    pub title: Option<String>,
    pub ccli: Option<String>,
    pub shown_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub format: String,
    pub version: u32,
    pub exported_at: i64,
    #[serde(default)]
    pub songs: Vec<Song>,
    #[serde(default)]
    pub themes: Vec<Theme>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub bibles: Vec<BibleExport>,
    #[serde(default)]
    pub usage: Vec<UsageEntry>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LibraryOptions {
    /// Bibles are large (a full text is ~4 MB of JSON); off by default.
    pub include_bibles: bool,
    pub include_usage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    /// Upsert by id; a song already present with a newer `updated_at` is kept.
    Merge,
    /// Delete songs, themes, services and settings first. Bibles and devices are never wiped.
    Replace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub songs_added: u32,
    pub songs_updated: u32,
    pub songs_skipped: u32,
    pub themes: u32,
    pub playlists: u32,
    pub settings: u32,
    pub bibles: u32,
    pub usage: u32,
}

fn is_volatile(key: &str) -> bool {
    key.starts_with("session.")
}

impl Store {
    pub fn export_library(&self, opts: LibraryOptions) -> StorageResult<Library> {
        let conn = self.lock();
        let mut songs_out = Vec::new();
        for id in songs::all_song_ids(&conn)? {
            if let Some(s) = songs::load_song(&conn, &id)? {
                songs_out.push(s);
            }
        }
        let mut lists = Vec::new();
        for id in playlists::all_playlist_ids(&conn)? {
            if let Some(p) = playlists::load_playlist(&conn, &id)? {
                lists.push(p);
            }
        }
        let mut bibles = Vec::new();
        if opts.include_bibles {
            for version in bible::all_versions(&conn)? {
                let verses = bible::all_verses(&conn, &version.code)?;
                bibles.push(BibleExport { version, verses });
            }
        }
        let mut usage = Vec::new();
        if opts.include_usage {
            let mut st = conn
                .prepare_cached("SELECT song_id, title, ccli, shown_at FROM usage_log ORDER BY id")
                .map_err(map_err)?;
            usage = st
                .query_map([], |r| {
                    Ok(UsageEntry {
                        song_id: r.get(0)?,
                        title: r.get(1)?,
                        ccli: r.get(2)?,
                        shown_at: r.get(3)?,
                    })
                })
                .map_err(map_err)?
                .collect::<Result<_, _>>()
                .map_err(map_err)?;
        }
        let mut settings = playlists::all_settings(&conn)?;
        settings.retain(|k, _| !is_volatile(k));
        Ok(Library {
            format: LIBRARY_FORMAT.into(),
            version: LIBRARY_VERSION,
            exported_at: crate::backup::now_ms(),
            songs: songs_out,
            themes: playlists::all_themes(&conn)?,
            playlists: lists,
            settings,
            bibles,
            usage,
        })
    }

    pub fn export_library_json(&self, opts: LibraryOptions) -> StorageResult<String> {
        serde_json::to_string_pretty(&self.export_library(opts)?)
            .map_err(|e| StorageError::Invalid(e.to_string()))
    }

    pub fn import_library(&self, lib: &Library, mode: ImportMode) -> StorageResult<ImportReport> {
        if lib.format != LIBRARY_FORMAT {
            return Err(StorageError::Invalid(format!(
                "not a HollyShow library (format `{}`)",
                lib.format
            )));
        }
        if lib.version > LIBRARY_VERSION {
            return Err(StorageError::Invalid(format!(
                "library version {} is newer than supported ({LIBRARY_VERSION})",
                lib.version
            )));
        }
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_err)?;
        let mut report = ImportReport::default();
        if mode == ImportMode::Replace {
            for table in ["songs_fts", "songs", "themes", "playlists"] {
                tx.execute(&format!("DELETE FROM {table}"), [])
                    .map_err(map_err)?;
            }
            tx.execute("DELETE FROM settings WHERE key NOT LIKE 'session.%'", [])
                .map_err(map_err)?;
        }
        for song in &lib.songs {
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT updated_at FROM songs WHERE id = ?1",
                    [song.id.as_str()],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
                .map_err(map_err)?;
            match existing {
                Some(at) if at > song.updated_at => report.songs_skipped += 1,
                Some(_) => {
                    songs::save_song(&tx, song)?;
                    report.songs_updated += 1;
                }
                None => {
                    songs::save_song(&tx, song)?;
                    report.songs_added += 1;
                }
            }
        }
        for t in &lib.themes {
            playlists::save_theme(&tx, t)?;
            report.themes += 1;
        }
        for p in &lib.playlists {
            playlists::save_playlist(&tx, p, lib.exported_at)?;
            report.playlists += 1;
        }
        for (k, v) in lib.settings.iter().filter(|(k, _)| !is_volatile(k)) {
            playlists::set_setting(&tx, k, v)?;
            report.settings += 1;
        }
        for b in &lib.bibles {
            bible::install(&tx, &b.version, &b.verses)?;
            report.bibles += 1;
        }
        for u in &lib.usage {
            // Idempotent: merging the same export twice must not double the CCLI counts.
            let n = tx
                .execute(
                    "INSERT INTO usage_log (song_id, title, ccli, shown_at)
                     SELECT ?1, ?2, ?3, ?4
                     WHERE NOT EXISTS (SELECT 1 FROM usage_log WHERE song_id = ?1 AND shown_at = ?4)",
                    params![u.song_id, u.title, u.ccli, u.shown_at],
                )
                .map_err(map_err)?;
            report.usage += n as u32;
        }
        tx.commit().map_err(map_err)?;
        Ok(report)
    }

    pub fn import_library_json(&self, json: &str, mode: ImportMode) -> StorageResult<ImportReport> {
        let lib: Library = serde_json::from_str(json)
            .map_err(|e| StorageError::Invalid(format!("library JSON: {e}")))?;
        self.import_library(&lib, mode)
    }
}
