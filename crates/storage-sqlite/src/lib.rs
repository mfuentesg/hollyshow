//! SQLite implementation of the `hollyshow-storage-api` traits (rusqlite with the bundled
//! engine and FTS5). Owner: builder "data". Mapping of `rusqlite::Error` lives here so the API
//! crate stays free of any database dependency.

use hollyshow_storage_api::StorageError;

/// Maps a rusqlite error into the backend-agnostic [`StorageError`].
pub fn map_err(e: rusqlite::Error) -> StorageError {
    match e {
        rusqlite::Error::QueryReturnedNoRows => StorageError::NotFound("row".into()),
        other => StorageError::Backend(other.to_string()),
    }
}

/// Opens an in-memory database; used by tests and as the smallest proof the bundled build has FTS5.
pub fn open_in_memory() -> Result<rusqlite::Connection, StorageError> {
    rusqlite::Connection::open_in_memory().map_err(map_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_sqlite_has_fts5_with_accent_folding() {
        let db = open_in_memory().unwrap();
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
    fn missing_row_maps_to_not_found() {
        assert!(matches!(
            map_err(rusqlite::Error::QueryReturnedNoRows),
            StorageError::NotFound(_)
        ));
    }
}
