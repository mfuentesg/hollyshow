//! Versioned schema migrations, tracked with `PRAGMA user_version`.
//!
//! Rules: never edit a shipped migration, append a new one. Each runs in its own transaction, so
//! a failure leaves the database at the previous version. A database written by a *newer*
//! HollyShow is refused rather than silently downgraded.

use crate::map_err;
use hollyshow_storage_api::{StorageError, StorageResult};
use rusqlite::Connection;

// Every FTS table uses `unicode61 remove_diacritics 2` (case and accent insensitive); text is also
// folded (`hollyshow_core::text::fold`) before indexing so `ñ` matches `n`.

const V1: &str = "
CREATE TABLE songs (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    title_sort  TEXT NOT NULL,
    alt_title   TEXT,
    author      TEXT,
    copyright   TEXT,
    ccli        TEXT,
    key         TEXT,
    bpm         INTEGER,
    tags        TEXT NOT NULL DEFAULT '[]',
    theme_id    TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX songs_title_sort ON songs(title_sort);

CREATE TABLE song_sections (
    song_id     TEXT NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    position    INTEGER NOT NULL,
    kind        TEXT NOT NULL,
    label       TEXT NOT NULL,
    lyrics      TEXT NOT NULL,
    translation TEXT,
    PRIMARY KEY (song_id, id)
);

CREATE TABLE song_arrangements (
    song_id     TEXT NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    position    INTEGER NOT NULL,
    name        TEXT NOT NULL,
    sequence    TEXT NOT NULL,
    PRIMARY KEY (song_id, id)
);

-- rowid = songs.rowid; content is accent-folded by the application.
CREATE VIRTUAL TABLE songs_fts USING fts5(
    title, lyrics, author,
    tokenize = \"unicode61 remove_diacritics 2\"
);

CREATE TABLE themes (
    id    TEXT PRIMARY KEY,
    name  TEXT NOT NULL,
    json  TEXT NOT NULL
);

CREATE TABLE playlists (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    date         TEXT,
    is_template  INTEGER NOT NULL DEFAULT 0,
    items        TEXT NOT NULL DEFAULT '[]',
    updated_at   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX playlists_date ON playlists(date);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

-- No foreign key on purpose: CCLI history must survive deleting a song, hence the snapshot.
CREATE TABLE usage_log (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    song_id   TEXT NOT NULL,
    title     TEXT,
    ccli      TEXT,
    shown_at  INTEGER NOT NULL
);
CREATE INDEX usage_log_shown_at ON usage_log(shown_at);
CREATE INDEX usage_log_song ON usage_log(song_id);

CREATE TABLE devices (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL,
    last_seen   INTEGER
);

CREATE TABLE bibles (
    code              TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    language          TEXT NOT NULL,
    license           TEXT NOT NULL,
    copyright_text    TEXT NOT NULL,
    is_public_domain  INTEGER NOT NULL,
    versification     TEXT NOT NULL,
    sort_order        INTEGER NOT NULL DEFAULT 0,
    is_enabled        INTEGER NOT NULL DEFAULT 1,
    is_default        INTEGER NOT NULL DEFAULT 0,
    is_secondary      INTEGER NOT NULL DEFAULT 0
);

-- `id` follows installation order, which importers keep canonical (Genesis ... Revelation);
-- passage ranges across books rely on it. verse = 0 addresses a chapter heading.
CREATE TABLE bible_verses (
    id       INTEGER PRIMARY KEY,
    version  TEXT NOT NULL REFERENCES bibles(code) ON DELETE CASCADE,
    book     TEXT NOT NULL,
    chapter  INTEGER NOT NULL,
    verse    INTEGER NOT NULL,
    text     TEXT NOT NULL,
    markup   TEXT NOT NULL DEFAULT '[]',
    UNIQUE (version, book, chapter, verse)
);

-- rowid = bible_verses.id; `version` is only filtered, never searched.
CREATE VIRTUAL TABLE bible_fts USING fts5(
    text,
    version UNINDEXED,
    tokenize = \"unicode61 remove_diacritics 2\"
);
";

/// Appended in order. Index `i` brings the schema to version `i + 1`.
const MIGRATIONS: &[&str] = &[V1, V2];

/// Paired devices live in the server's `DeviceStore` (settings key `server.devices.v1`); the
/// unused table of V1 was a second, competing schema for the same data.
const V2: &str = "DROP TABLE IF EXISTS devices;";

/// Latest schema version this build understands.
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

pub(crate) fn current_version(conn: &Connection) -> StorageResult<u32> {
    conn.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
        .map_err(map_err)
}

/// Brings `conn` to [`SCHEMA_VERSION`].
pub(crate) fn migrate(conn: &mut Connection) -> StorageResult<()> {
    let from = current_version(conn)?;
    if from > SCHEMA_VERSION {
        return Err(StorageError::Invalid(format!(
            "database schema v{from} is newer than this HollyShow supports (v{SCHEMA_VERSION}); update the app"
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(from as usize) {
        let tx = conn.transaction().map_err(map_err)?;
        tx.execute_batch(sql).map_err(map_err)?;
        tx.pragma_update(None, "user_version", i as u32 + 1)
            .map_err(map_err)?;
        tx.commit().map_err(map_err)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_from_scratch_and_is_idempotent() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c).unwrap();
        assert_eq!(current_version(&c).unwrap(), SCHEMA_VERSION);
        migrate(&mut c).unwrap();
        let tables: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN
                 ('songs','song_sections','song_arrangements','themes','playlists','settings',
                  'usage_log','bibles','bible_verses')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 9);
        let devices: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'devices'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(devices, 0);
    }

    #[test]
    fn refuses_a_newer_database() {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(migrate(&mut c), Err(StorageError::Invalid(_))));
    }
}
