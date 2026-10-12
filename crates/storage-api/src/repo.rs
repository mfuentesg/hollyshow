//! Repository traits. Every implementation must be `Send + Sync`: they are shared as
//! `Arc<dyn ...>` between the Tauri shell and the LAN server.

use crate::error::StorageResult;
use hollyshow_core::{
    BibleHit, BibleVersion, Passage, Playlist, PlaylistId, PlaylistSummary, Song, SongId,
    SongSummary, Theme, ThemeId, Verse, VersionCode,
};
use std::sync::Arc;

pub trait SongRepo: Send + Sync {
    /// Accent/case-insensitive search over title, lyrics and author. Empty text lists all.
    fn search(&self, text: &str, limit: u32) -> StorageResult<Vec<SongSummary>>;
    fn get(&self, id: &SongId) -> StorageResult<Option<Song>>;
    /// Insert or replace (sections and arrangements included, atomically).
    fn save(&self, song: &Song) -> StorageResult<()>;
    fn delete(&self, id: &SongId) -> StorageResult<()>;
    /// Records that a song was projected (CCLI report).
    fn log_usage(&self, id: &SongId, shown_at_ms: i64) -> StorageResult<()>;
    /// Latest `shown_at` of every song logged at or after `since_ms` (seeds the dedupe on start).
    fn recent_usage(&self, since_ms: i64) -> StorageResult<Vec<(SongId, i64)>>;
}

pub trait BibleRepo: Send + Sync {
    fn list_versions(&self) -> StorageResult<Vec<BibleVersion>>;
    fn get_version(&self, code: &VersionCode) -> StorageResult<Option<BibleVersion>>;
    fn set_enabled(&self, code: &VersionCode, enabled: bool) -> StorageResult<()>;
    fn set_default(&self, code: &VersionCode) -> StorageResult<()>;
    fn set_secondary(&self, code: Option<&VersionCode>) -> StorageResult<()>;
    fn remove_version(&self, code: &VersionCode) -> StorageResult<()>;
    /// Verses of `passage` in `version`, in canonical order. Missing verses are simply absent
    /// (callers render a "not in this version" marker) rather than an error.
    fn passage(&self, version: &VersionCode, passage: &Passage) -> StorageResult<Vec<Verse>>;
    fn search(
        &self,
        text: &str,
        version: Option<&VersionCode>,
        limit: u32,
    ) -> StorageResult<Vec<BibleHit>>;
}

pub trait ThemeRepo: Send + Sync {
    fn list(&self) -> StorageResult<Vec<Theme>>;
    fn get(&self, id: &ThemeId) -> StorageResult<Option<Theme>>;
    fn save(&self, theme: &Theme) -> StorageResult<()>;
    fn delete(&self, id: &ThemeId) -> StorageResult<()>;
}

pub trait PlaylistRepo: Send + Sync {
    fn list(&self) -> StorageResult<Vec<PlaylistSummary>>;
    fn get(&self, id: &PlaylistId) -> StorageResult<Option<Playlist>>;
    fn save(&self, playlist: &Playlist) -> StorageResult<()>;
    fn delete(&self, id: &PlaylistId) -> StorageResult<()>;
}

/// Key/value settings (`settings` table). Values are opaque strings, usually JSON.
pub trait SettingsRepo: Send + Sync {
    fn get(&self, key: &str) -> StorageResult<Option<String>>;
    fn set(&self, key: &str, value: &str) -> StorageResult<()>;
    fn remove(&self, key: &str) -> StorageResult<()>;
}

/// Bundle handed to the server and the Tauri commands.
#[derive(Clone)]
pub struct Repos {
    pub songs: Arc<dyn SongRepo>,
    pub bibles: Arc<dyn BibleRepo>,
    pub themes: Arc<dyn ThemeRepo>,
    pub playlists: Arc<dyn PlaylistRepo>,
    pub settings: Arc<dyn SettingsRepo>,
}
