//! Backups: consistent snapshots with `VACUUM INTO`, daily rotation and restore.

use crate::{map_err, Store};
use hollyshow_storage_api::{StorageError, StorageResult};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const PREFIX: &str = "hollyshow-";
const SUFFIX: &str = ".db";

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn io(ctx: &str, e: std::io::Error) -> StorageError {
    StorageError::Backend(format!("{ctx}: {e}"))
}

/// Writes a consistent copy of the live database to `dest` (which must not exist), going through
/// a temporary name so a crash never leaves a half-written backup under the final name.
pub(crate) fn vacuum_into(conn: &Connection, dest: &Path) -> StorageResult<()> {
    if dest.exists() {
        return Err(StorageError::Conflict(format!("{} exists", dest.display())));
    }
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let _ = std::fs::remove_file(&tmp);
    conn.execute("VACUUM INTO ?1", [tmp.to_string_lossy().as_ref()])
        .map_err(map_err)?;
    std::fs::rename(&tmp, dest).map_err(|e| io("finalize backup", e))
}

/// `YYYY-MM-DD` (UTC) of a Unix-millisecond timestamp (Howard Hinnant's civil-from-days).
pub(crate) fn utc_date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// A backup file found in a backup directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    pub path: PathBuf,
    /// `YYYY-MM-DD` taken from the file name.
    pub date: String,
    pub size_bytes: u64,
}

fn parse_name(name: &str) -> Option<&str> {
    let date = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    let ok = date.len() == 10
        && date.char_indices().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    ok.then_some(date)
}

impl Store {
    /// Writes a snapshot of the database to `dest` (must not exist).
    pub fn backup_to(&self, dest: impl AsRef<Path>) -> StorageResult<()> {
        vacuum_into(&self.lock(), dest.as_ref())
    }

    /// Creates today's backup in `dir` unless one exists, then keeps only the newest `keep`.
    /// Returns the new file, or `None` when today's backup was already there. Call at startup
    /// and, for long-running sessions, once an hour; it is cheap when nothing is to be done.
    pub fn daily_backup(
        &self,
        dir: impl AsRef<Path>,
        keep: usize,
        now_ms: i64,
    ) -> StorageResult<Option<PathBuf>> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| io("create backup dir", e))?;
        let path = dir.join(format!("{PREFIX}{}{SUFFIX}", utc_date(now_ms)));
        let created = if path.exists() {
            None
        } else {
            self.backup_to(&path)?;
            Some(path)
        };
        // Rotation also runs when nothing was created, so lowering `keep` takes effect.
        let all = list_backups(dir)?;
        for old in all.iter().skip(keep.max(1)) {
            std::fs::remove_file(&old.path).map_err(|e| io("remove old backup", e))?;
        }
        Ok(created)
    }

    /// Backups in `dir`, newest first.
    pub fn list_backups(&self, dir: impl AsRef<Path>) -> StorageResult<Vec<BackupInfo>> {
        list_backups(dir.as_ref())
    }
}

/// Backups found in `dir`, newest first, without needing an open store (used to recover when the
/// main database cannot be opened). A missing or unreadable folder yields an empty list.
pub fn list_backup_files(dir: impl AsRef<Path>) -> Vec<BackupInfo> {
    list_backups(dir.as_ref()).unwrap_or_default()
}

fn list_backups(dir: &Path) -> StorageResult<Vec<BackupInfo>> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(io("read backup dir", e)),
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(date) = parse_name(&name) {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(BackupInfo {
                path: entry.path(),
                date: date.to_owned(),
                size_bytes: size,
            });
        }
    }
    out.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(out)
}

/// Replaces the database file at `db_path` with `backup`, after checking the backup opens and
/// passes `integrity_check`. The current file is kept as `<db>.before-restore`. The database must
/// not be open in this process while restoring.
pub fn restore_backup(backup: impl AsRef<Path>, db_path: impl AsRef<Path>) -> StorageResult<()> {
    let (backup, db_path) = (backup.as_ref(), db_path.as_ref());
    {
        let conn = Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(map_err)?;
        let res: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(map_err)?;
        if res != "ok" {
            return Err(StorageError::Invalid(format!("backup is corrupt: {res}")));
        }
        let v = crate::migrations::current_version(&conn)?;
        if v > crate::migrations::SCHEMA_VERSION {
            return Err(StorageError::Invalid(format!(
                "backup schema v{v} is newer than this HollyShow"
            )));
        }
    }
    if db_path.exists() {
        let mut keep = db_path.as_os_str().to_owned();
        keep.push(".before-restore");
        std::fs::copy(db_path, PathBuf::from(keep)).map_err(|e| io("save current db", e))?;
    }
    for ext in ["-wal", "-shm"] {
        let mut side = db_path.as_os_str().to_owned();
        side.push(ext);
        let _ = std::fs::remove_file(PathBuf::from(side));
    }
    std::fs::copy(backup, db_path).map_err(|e| io("restore", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_core::{Section, SectionId, SectionKind, Song, SongId};
    use hollyshow_storage_api::SongRepo;

    fn song(id: &str) -> Song {
        Song {
            id: SongId::new(id),
            title: format!("Canción {id}"),
            alt_title: None,
            author: None,
            copyright: None,
            ccli: None,
            key: None,
            bpm: None,
            tags: vec![],
            theme_id: None,
            sections: vec![Section {
                id: SectionId::new("v1"),
                kind: SectionKind::Verse,
                label: "Verso 1".into(),
                lyrics: "letra".into(),
                translation: None,
            }],
            arrangements: vec![],
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn utc_date_known_values() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(951_782_400_000), "2000-02-29");
        assert_eq!(utc_date(1_760_227_200_000), "2025-10-12");
        assert_eq!(utc_date(-1), "1969-12-31");
    }

    #[test]
    fn daily_backup_is_idempotent_per_day_and_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open(dir.path().join("lib.db")).unwrap();
        db.save(&song("a")).unwrap();
        let backups = dir.path().join("backups");
        let day = 86_400_000;
        let t0 = 1_760_227_200_000;
        assert!(db.daily_backup(&backups, 3, t0).unwrap().is_some());
        assert!(db.daily_backup(&backups, 3, t0 + 1000).unwrap().is_none());
        for i in 1..=4 {
            db.daily_backup(&backups, 3, t0 + i * day).unwrap();
        }
        let list = db.list_backups(&backups).unwrap();
        let dates: Vec<_> = list.iter().map(|b| b.date.as_str()).collect();
        assert_eq!(dates, ["2025-10-16", "2025-10-15", "2025-10-14"]);
        assert!(!std::fs::read_dir(&backups).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }

    #[test]
    fn restore_brings_data_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lib.db");
        let backup = dir.path().join("b.db");
        {
            let db = Store::open(&path).unwrap();
            db.save(&song("a")).unwrap();
            db.backup_to(&backup).unwrap();
            db.save(&song("b")).unwrap();
            assert!(db.backup_to(&backup).is_err(), "never overwrite a backup");
        }
        restore_backup(&backup, &path).unwrap();
        let db = Store::open(&path).unwrap();
        assert_eq!(db.song_count().unwrap(), 1);
        assert!(dir.path().join("lib.db.before-restore").exists());
    }

    #[test]
    fn restore_rejects_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.db");
        std::fs::write(&bad, b"not a database at all, definitely").unwrap();
        assert!(restore_backup(&bad, dir.path().join("x.db")).is_err());
    }

    #[test]
    fn file_name_parsing() {
        assert_eq!(parse_name("hollyshow-2026-01-02.db"), Some("2026-01-02"));
        assert_eq!(parse_name("hollyshow-2026-01-02.db.tmp"), None);
        assert_eq!(parse_name("other.db"), None);
    }
}
