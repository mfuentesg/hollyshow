//! `BibleRepo`: installed versions, verses and per-version FTS5 search.

use crate::{from_json, fts_query, map_err, to_json, Store};
use hollyshow_core::text::fold;
use hollyshow_core::{
    BibleHit, BibleRef, BibleVersion, BookId, Passage, TextMark, Verse, VersionCode,
};
use hollyshow_storage_api::{BibleRepo, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension, Row};

const VERSION_COLS: &str = "code, name, language, license, copyright_text, is_public_domain,
    versification, sort_order, is_enabled, is_default, is_secondary";

fn version_row(r: &Row<'_>) -> rusqlite::Result<BibleVersion> {
    Ok(BibleVersion {
        code: VersionCode(r.get(0)?),
        name: r.get(1)?,
        language: r.get(2)?,
        license: r.get(3)?,
        copyright_text: r.get(4)?,
        is_public_domain: r.get(5)?,
        versification: r.get(6)?,
        sort_order: r.get(7)?,
        is_enabled: r.get(8)?,
        is_default: r.get(9)?,
        is_secondary: r.get(10)?,
    })
}

fn ref_of(book: String, chapter: u16, verse: u16) -> BibleRef {
    BibleRef {
        book: BookId(book),
        chapter,
        verse: (verse != 0).then_some(verse),
    }
}

fn verse_row(r: &Row<'_>) -> rusqlite::Result<(BibleRef, String, String)> {
    Ok((
        ref_of(r.get(0)?, r.get(1)?, r.get(2)?),
        r.get(3)?,
        r.get(4)?,
    ))
}

fn to_verse((reference, text, markup): (BibleRef, String, String)) -> StorageResult<Verse> {
    Ok(Verse {
        reference,
        text,
        marks: from_json::<Vec<TextMark>>(&markup)?,
    })
}

fn exists(conn: &Connection, code: &VersionCode) -> StorageResult<bool> {
    conn.query_row(
        "SELECT 1 FROM bibles WHERE code = ?1",
        [&code.0],
        |_| Ok(()),
    )
    .optional()
    .map(|o| o.is_some())
    .map_err(map_err)
}

/// Replaces the installed copy of `version` (metadata and verses) inside `conn`'s transaction.
pub(crate) fn install(
    conn: &Connection,
    version: &BibleVersion,
    verses: &[Verse],
) -> StorageResult<()> {
    if version.code.0.trim().is_empty() {
        return Err(StorageError::Invalid("bible version code is empty".into()));
    }
    remove(conn, &version.code)?;
    let has_default: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bibles WHERE is_default = 1)",
            [],
            |r| r.get(0),
        )
        .map_err(map_err)?;
    // The first version ever installed becomes the default so the app always has one.
    let make_default = version.is_default || !has_default;
    if make_default {
        conn.execute("UPDATE bibles SET is_default = 0", [])
            .map_err(map_err)?;
    }
    if version.is_secondary {
        conn.execute("UPDATE bibles SET is_secondary = 0", [])
            .map_err(map_err)?;
    }
    conn.execute(
        "INSERT INTO bibles (code, name, language, license, copyright_text, is_public_domain,
                             versification, sort_order, is_enabled, is_default, is_secondary)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            version.code.0,
            version.name,
            version.language,
            version.license,
            version.copyright_text,
            version.is_public_domain,
            version.versification,
            version.sort_order,
            version.is_enabled,
            make_default,
            version.is_secondary && !make_default,
        ],
    )
    .map_err(map_err)?;
    let mut ins = conn
        .prepare_cached(
            "INSERT INTO bible_verses (version, book, chapter, verse, text, markup)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(version, book, chapter, verse) DO UPDATE SET
                text = excluded.text, markup = excluded.markup
             RETURNING id",
        )
        .map_err(map_err)?;
    let mut fts = conn
        .prepare_cached("INSERT INTO bible_fts (rowid, text, version) VALUES (?1, ?2, ?3)")
        .map_err(map_err)?;
    let mut del_fts = conn
        .prepare_cached("DELETE FROM bible_fts WHERE rowid = ?1")
        .map_err(map_err)?;
    for v in verses {
        let id: i64 = ins
            .query_row(
                params![
                    version.code.0,
                    v.reference.book.0,
                    v.reference.chapter,
                    v.reference.verse.unwrap_or(0),
                    v.text,
                    to_json(&v.marks)?
                ],
                |r| r.get(0),
            )
            .map_err(map_err)?;
        // A duplicated reference in the input replaced the text above; keep one FTS row.
        del_fts.execute([id]).map_err(map_err)?;
        fts.execute(params![id, fold(&v.text), version.code.0])
            .map_err(map_err)?;
    }
    Ok(())
}

fn remove(conn: &Connection, code: &VersionCode) -> StorageResult<()> {
    conn.execute(
        "DELETE FROM bible_fts WHERE rowid IN (SELECT id FROM bible_verses WHERE version = ?1)",
        [&code.0],
    )
    .map_err(map_err)?;
    // Verses go with the version (ON DELETE CASCADE).
    conn.execute("DELETE FROM bibles WHERE code = ?1", [&code.0])
        .map_err(map_err)?;
    Ok(())
}

pub(crate) fn all_versions(conn: &Connection) -> StorageResult<Vec<BibleVersion>> {
    let mut st = conn
        .prepare_cached(&format!(
            "SELECT {VERSION_COLS} FROM bibles ORDER BY sort_order, code"
        ))
        .map_err(map_err)?;
    let rows = st
        .query_map([], version_row)
        .map_err(map_err)?
        .collect::<Result<_, _>>()
        .map_err(map_err)?;
    Ok(rows)
}

pub(crate) fn all_verses(conn: &Connection, code: &VersionCode) -> StorageResult<Vec<Verse>> {
    let mut st = conn
        .prepare_cached(
            "SELECT book, chapter, verse, text, markup FROM bible_verses
             WHERE version = ?1 ORDER BY id",
        )
        .map_err(map_err)?;
    let rows: Vec<_> = st
        .query_map([&code.0], verse_row)
        .map_err(map_err)?
        .collect::<Result<_, _>>()
        .map_err(map_err)?;
    rows.into_iter().map(to_verse).collect()
}

impl Store {
    /// Installs (or reinstalls) a Bible version with its verses, atomically.
    ///
    /// `verses` must be in canonical order (Genesis to Revelation): the insertion order defines
    /// how passages that span chapters or books are resolved. The first version installed becomes
    /// the default; `is_default`/`is_secondary` on `version` keep their exclusivity.
    pub fn install_bible(&self, version: &BibleVersion, verses: &[Verse]) -> StorageResult<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_err)?;
        install(&tx, version, verses)?;
        tx.commit().map_err(map_err)
    }

    /// Number of verses stored for a version.
    pub fn verse_count(&self, code: &VersionCode) -> StorageResult<u32> {
        self.lock()
            .query_row(
                "SELECT count(*) FROM bible_verses WHERE version = ?1",
                [&code.0],
                |r| r.get(0),
            )
            .map_err(map_err)
    }
}

impl BibleRepo for Store {
    fn list_versions(&self) -> StorageResult<Vec<BibleVersion>> {
        all_versions(&self.lock())
    }

    fn get_version(&self, code: &VersionCode) -> StorageResult<Option<BibleVersion>> {
        self.lock()
            .query_row(
                &format!("SELECT {VERSION_COLS} FROM bibles WHERE code = ?1"),
                [&code.0],
                version_row,
            )
            .optional()
            .map_err(map_err)
    }

    fn set_enabled(&self, code: &VersionCode, enabled: bool) -> StorageResult<()> {
        let n = self
            .lock()
            .execute(
                "UPDATE bibles SET is_enabled = ?2 WHERE code = ?1",
                params![code.0, enabled],
            )
            .map_err(map_err)?;
        if n == 0 {
            return Err(StorageError::NotFound(code.0.clone()));
        }
        Ok(())
    }

    fn set_default(&self, code: &VersionCode) -> StorageResult<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_err)?;
        if !exists(&tx, code)? {
            return Err(StorageError::NotFound(code.0.clone()));
        }
        tx.execute(
            "UPDATE bibles SET is_default = (code = ?1),
                               is_secondary = CASE WHEN code = ?1 THEN 0 ELSE is_secondary END",
            [&code.0],
        )
        .map_err(map_err)?;
        tx.commit().map_err(map_err)
    }

    fn set_secondary(&self, code: Option<&VersionCode>) -> StorageResult<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_err)?;
        if let Some(c) = code {
            if !exists(&tx, c)? {
                return Err(StorageError::NotFound(c.0.clone()));
            }
        }
        let c = code.map(|c| c.0.as_str());
        tx.execute(
            "UPDATE bibles SET is_secondary = (COALESCE(code = ?1, 0) AND is_default = 0)",
            params![c],
        )
        .map_err(map_err)?;
        tx.commit().map_err(map_err)
    }

    fn remove_version(&self, code: &VersionCode) -> StorageResult<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_err)?;
        remove(&tx, code)?;
        // Never leave the library without a default while versions remain.
        tx.execute(
            "UPDATE bibles SET is_default = 1 WHERE code = (
                SELECT code FROM bibles ORDER BY is_enabled DESC, sort_order, code LIMIT 1)
             AND NOT EXISTS (SELECT 1 FROM bibles WHERE is_default = 1)",
            [],
        )
        .map_err(map_err)?;
        tx.commit().map_err(map_err)
    }

    fn passage(&self, version: &VersionCode, passage: &Passage) -> StorageResult<Vec<Verse>> {
        let conn = self.lock();
        let (s, e) = (&passage.start, &passage.end);
        // Resolve both ends to row ids, then read the contiguous range: this is canonical across
        // chapters and books because ids follow installation order. A missing boundary verse
        // snaps inward to the nearest existing one.
        let first: Option<i64> = conn
            .query_row(
                "SELECT MIN(id) FROM bible_verses
                 WHERE version = ?1 AND book = ?2 AND chapter = ?3 AND verse >= ?4",
                params![version.0, s.book.0, s.chapter, s.verse.unwrap_or(0)],
                |r| r.get(0),
            )
            .map_err(map_err)?;
        let last: Option<i64> = conn
            .query_row(
                "SELECT MAX(id) FROM bible_verses
                 WHERE version = ?1 AND book = ?2 AND chapter = ?3 AND verse <= ?4",
                params![version.0, e.book.0, e.chapter, e.verse.unwrap_or(u16::MAX)],
                |r| r.get(0),
            )
            .map_err(map_err)?;
        let (Some(first), Some(last)) = (first, last) else {
            return Ok(vec![]);
        };
        if first > last {
            return Ok(vec![]);
        }
        let mut st = conn
            .prepare_cached(
                "SELECT book, chapter, verse, text, markup FROM bible_verses
                 WHERE version = ?1 AND id BETWEEN ?2 AND ?3 ORDER BY id",
            )
            .map_err(map_err)?;
        let rows: Vec<_> = st
            .query_map(params![version.0, first, last], verse_row)
            .map_err(map_err)?
            .collect::<Result<_, _>>()
            .map_err(map_err)?;
        rows.into_iter().map(to_verse).collect()
    }

    fn search(
        &self,
        text: &str,
        version: Option<&VersionCode>,
        limit: u32,
    ) -> StorageResult<Vec<BibleHit>> {
        let Some(query) = fts_query(text) else {
            return Ok(vec![]);
        };
        let conn = self.lock();
        let mut st = conn
            .prepare_cached(
                "SELECT v.version, v.book, v.chapter, v.verse, v.text
                 FROM bible_fts f JOIN bible_verses v ON v.id = f.rowid
                 WHERE bible_fts MATCH ?1 AND (?2 IS NULL OR f.version = ?2)
                 ORDER BY f.rank, v.id
                 LIMIT ?3",
            )
            .map_err(map_err)?;
        let rows = st
            .query_map(params![query, version.map(|v| v.0.as_str()), limit], |r| {
                Ok(BibleHit {
                    version: VersionCode(r.get(0)?),
                    reference: ref_of(r.get(1)?, r.get(2)?, r.get(3)?),
                    text: r.get(4)?,
                })
            })
            .map_err(map_err)?
            .collect::<Result<_, _>>()
            .map_err(map_err)?;
        Ok(rows)
    }
}
