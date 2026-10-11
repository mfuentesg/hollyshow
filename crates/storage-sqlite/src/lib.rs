//! SQLite implementation of the `hollyshow-storage-api` traits (rusqlite with the bundled engine
//! and FTS5).
//!
//! * [`Store`] owns one WAL-mode connection behind a mutex and implements every repository trait.
//! * Schema: versioned migrations (`migrations`), PLAN §6 tables, accent/case-insensitive FTS5
//!   for songs and Bible verses.
//! * Extras beyond the traits: paired devices, the CCLI usage report, Bible installation
//!   ([`Store::install_bible`]), JSON library export/import and rotating daily backups.
//!
//! All methods are blocking; async callers use `tokio::task::spawn_blocking`.

mod backup;
mod bible;
#[cfg(test)]
mod contract_tests;
mod devices;
mod library;
pub mod migrations;
mod playlists;
mod songs;

pub use backup::{restore_backup, BackupInfo};
pub use devices::DeviceRecord;
pub use library::{BibleExport, ImportMode, ImportReport, Library, LibraryOptions, UsageEntry};
pub use songs::UsageRow;

use hollyshow_storage_api::{Repos, StorageError, StorageResult};
use rusqlite::{Connection, ErrorCode};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

/// Maps a rusqlite error into the backend-agnostic [`StorageError`].
///
/// SQLite's message is kept in `Backend` for the logs; callers must not forward it to a remote.
pub fn map_err(e: rusqlite::Error) -> StorageError {
    match &e {
        rusqlite::Error::QueryReturnedNoRows => StorageError::NotFound("row".into()),
        rusqlite::Error::SqliteFailure(f, _) if f.code == ErrorCode::ConstraintViolation => {
            StorageError::Conflict(e.to_string())
        }
        _ => StorageError::Backend(e.to_string()),
    }
}

pub(crate) fn to_json<T: Serialize + ?Sized>(v: &T) -> StorageResult<String> {
    serde_json::to_string(v).map_err(|e| StorageError::Invalid(e.to_string()))
}

pub(crate) fn from_json<T: DeserializeOwned>(s: &str) -> StorageResult<T> {
    serde_json::from_str(s).map_err(|e| StorageError::Invalid(format!("corrupt stored JSON: {e}")))
}

/// SQLite-backed store. Cheap to share: wrap in `Arc` and use [`Store::repos`].
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Opens (creating if needed) the database at `path`, enables WAL and foreign keys and
    /// migrates it. Before applying pending migrations to a non-empty database a
    /// `<file>.pre-v<N>.bak` snapshot is written next to it.
    pub fn open(path: impl AsRef<Path>) -> StorageResult<Self> {
        let path = path.as_ref();
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| StorageError::Backend(format!("create {}: {e}", dir.display())))?;
        }
        let mut conn = Connection::open(path).map_err(map_err)?;
        Self::configure(&conn, true)?;
        let version = migrations::current_version(&conn)?;
        if version > 0 && version < migrations::SCHEMA_VERSION {
            let mut name = path.file_name().unwrap_or_default().to_os_string();
            name.push(format!(".pre-v{version}.bak"));
            let snapshot = path.with_file_name(name);
            if !snapshot.exists() {
                backup::vacuum_into(&conn, &snapshot)?;
            }
        }
        migrations::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Fresh in-memory database (tests, previews).
    pub fn open_in_memory() -> StorageResult<Self> {
        let mut conn = Connection::open_in_memory().map_err(map_err)?;
        Self::configure(&conn, false)?;
        migrations::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn configure(conn: &Connection, wal: bool) -> StorageResult<()> {
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(map_err)?;
        if wal {
            // Returns the resulting mode as a row, so it cannot go through `pragma_update`.
            conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
                .map_err(map_err)?;
            conn.pragma_update(None, "synchronous", "NORMAL")
                .map_err(map_err)?;
        }
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(map_err)
    }

    /// The repository bundle handed to the application layer.
    pub fn repos(self: &Arc<Self>) -> Repos {
        Repos {
            songs: self.clone(),
            bibles: self.clone(),
            themes: self.clone(),
            playlists: self.clone(),
            settings: self.clone(),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Schema version of the open database.
    pub fn schema_version(&self) -> StorageResult<u32> {
        migrations::current_version(&self.lock())
    }

    /// Runs SQLite's `PRAGMA integrity_check`; `Ok(())` when the database is sound.
    pub fn integrity_check(&self) -> StorageResult<()> {
        let conn = self.lock();
        let res: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(map_err)?;
        if res == "ok" {
            Ok(())
        } else {
            Err(StorageError::Backend(format!("integrity_check: {res}")))
        }
    }
}

/// Splits folded text into FTS5 prefix terms: `"cuan dulce"` -> `"cuan"* "dulce"*`.
/// Returns `None` when there is nothing searchable.
pub(crate) fn fts_query(text: &str) -> Option<String> {
    let folded = hollyshow_core::text::fold(text);
    let terms: Vec<String> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_sqlite_has_fts5_with_accent_folding() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE VIRTUAL TABLE t USING fts5(body, tokenize = \"unicode61 remove_diacritics 2\");
             INSERT INTO t(body) VALUES ('Sublime gracia'), ('Cristo es el Señor');",
        )
        .unwrap();
        let hits: i64 = db
            .query_row("SELECT count(*) FROM t WHERE t MATCH 'senor'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn missing_row_maps_to_not_found_and_constraints_to_conflict() {
        assert!(matches!(
            map_err(rusqlite::Error::QueryReturnedNoRows),
            StorageError::NotFound(_)
        ));
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE t(a UNIQUE); INSERT INTO t VALUES (1);")
            .unwrap();
        let e = db.execute("INSERT INTO t VALUES (1)", []).unwrap_err();
        assert!(matches!(map_err(e), StorageError::Conflict(_)));
    }

    #[test]
    fn fts_query_builds_prefix_terms_and_is_injection_safe() {
        assert_eq!(
            fts_query("Cuán DULCE").as_deref(),
            Some("\"cuan\"* \"dulce\"*")
        );
        assert_eq!(fts_query("\" OR *"), Some("\"or\"*".into()));
        assert_eq!(fts_query("  ...  "), None);
    }

    #[test]
    fn open_creates_file_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/db.sqlite");
        let s = Store::open(&path).unwrap();
        assert_eq!(s.schema_version().unwrap(), migrations::SCHEMA_VERSION);
        s.integrity_check().unwrap();
        drop(s);
        Store::open(&path).unwrap();
    }
}
