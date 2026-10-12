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

/// Verses per transaction when installing a Bible.
const INSTALL_BATCH: usize = 2000;

/// Prefix of the staging code a Bible is installed under until it is complete. Rows with this
/// prefix are "installing": invisible to every listing and removed at start-up. User codes may
/// not use it.
const STAGING_PREFIX: &str = "~installing~";

fn staging_code(code: &VersionCode) -> String {
    format!("{STAGING_PREFIX}{}", code.0)
}

/// Replaces the installed copy of `version` (metadata and verses) inside `conn`'s transaction.
pub(crate) fn install(
    conn: &Connection,
    version: &BibleVersion,
    verses: &[Verse],
) -> StorageResult<()> {
    let stage = staging_code(&version.code);
    install_header(conn, version, &stage)?;
    install_verses(conn, &stage, verses)?;
    finish_install(conn, version, &stage)
}

/// Step 1: inserts the version row under the staging code `stage`, disabled and hidden. The
/// previous copy of the same code (if any) is untouched and stays usable.
fn install_header(conn: &Connection, version: &BibleVersion, stage: &str) -> StorageResult<()> {
    if version.code.0.trim().is_empty() {
        return Err(StorageError::Invalid("bible version code is empty".into()));
    }
    if version.code.0.starts_with(STAGING_PREFIX) {
        return Err(StorageError::Invalid(format!(
            "bible version code may not start with {STAGING_PREFIX}"
        )));
    }
    // A leftover staging copy of the same code (an earlier crash in this process) is replaced.
    remove_code(conn, stage)?;
    conn.execute(
        "INSERT INTO bibles (code, name, language, license, copyright_text, is_public_domain,
                             versification, sort_order, is_enabled, is_default, is_secondary)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 0, 0)",
        params![
            stage,
            version.name,
            version.language,
            version.license,
            version.copyright_text,
            version.is_public_domain,
            version.versification,
            version.sort_order,
        ],
    )
    .map_err(map_err)?;
    Ok(())
}

/// Code under which the previous copy of a reinstalled Bible waits to be deleted. It carries the
/// staging prefix, so it is hidden from listings and search and swept at start-up.
fn retired_code(code: &VersionCode) -> String {
    format!("{STAGING_PREFIX}old~{}", code.0)
}

/// Step 3, one short transaction: moves the previous copy of the code aside, renames the staged
/// rows to the real code, applies enabled/default/secondary exclusivity and enables the version.
///
/// Only the plain `bibles`/`bible_verses` rows are renamed. The FTS index is keyed by verse id
/// and carries no version (search filters through `bible_verses`), so the swap never re-indexes
/// text, and deleting the old copy (the expensive part) happens afterwards in
/// [`discard_retired`], outside this transaction.
fn finish_install(conn: &Connection, version: &BibleVersion, stage: &str) -> StorageResult<()> {
    let code = &version.code.0;
    let retired = retired_code(&version.code);
    // The renames touch the parent and child rows in turn; check the constraint at commit.
    conn.pragma_update(None, "defer_foreign_keys", true)
        .map_err(map_err)?;
    // A leftover retired copy (crash between swap and discard in this process) goes first.
    remove_code(conn, &retired)?;
    rename_code(conn, code, &retired)?;
    rename_code(conn, stage, code)?;
    let has_default: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bibles WHERE is_default = 1 AND code <> ?1
                                              AND code NOT LIKE ?2)",
            params![code, format!("{STAGING_PREFIX}%")],
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
        "UPDATE bibles SET is_enabled = ?2, is_default = ?3, is_secondary = ?4 WHERE code = ?1",
        params![
            code,
            version.is_enabled,
            make_default,
            version.is_secondary && !make_default
        ],
    )
    .map_err(map_err)?;
    Ok(())
}

/// Renames a Bible's rows (no-op when `from` does not exist). Needs deferred foreign keys.
fn rename_code(conn: &Connection, from: &str, to: &str) -> StorageResult<()> {
    conn.execute("UPDATE bibles SET code = ?2 WHERE code = ?1", [from, to])
        .map_err(map_err)?;
    conn.execute(
        "UPDATE bible_verses SET version = ?2 WHERE version = ?1",
        [from, to],
    )
    .map_err(map_err)?;
    Ok(())
}

/// Step 4: deletes the previous copy retired by [`finish_install`] (its own transaction).
fn discard_retired(conn: &Connection, code: &VersionCode) -> StorageResult<()> {
    remove_code(conn, &retired_code(code))
}

/// Step 2 (repeatable): inserts a batch of verses under the staging code.
fn install_verses(conn: &Connection, stage: &str, verses: &[Verse]) -> StorageResult<()> {
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
        .prepare_cached("INSERT INTO bible_fts (rowid, text, version) VALUES (?1, ?2, '')")
        .map_err(map_err)?;
    let mut del_fts = conn
        .prepare_cached("DELETE FROM bible_fts WHERE rowid = ?1")
        .map_err(map_err)?;
    for v in verses {
        let id: i64 = ins
            .query_row(
                params![
                    stage,
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
        fts.execute(params![id, fold(&v.text)]).map_err(map_err)?;
    }
    Ok(())
}

/// Deletes every half-installed (staging) copy. Returns how many were removed.
pub(crate) fn cleanup_installing(conn: &Connection) -> StorageResult<usize> {
    let codes: Vec<String> = {
        let mut st = conn
            .prepare("SELECT code FROM bibles WHERE code LIKE ?1")
            .map_err(map_err)?;
        let rows = st
            .query_map([format!("{STAGING_PREFIX}%")], |r| r.get(0))
            .map_err(map_err)?
            .collect::<Result<_, _>>()
            .map_err(map_err)?;
        rows
    };
    for c in &codes {
        remove_code(conn, c)?;
    }
    Ok(codes.len())
}

fn remove(conn: &Connection, code: &VersionCode) -> StorageResult<()> {
    remove_code(conn, &code.0)
}

fn remove_code(conn: &Connection, code: &str) -> StorageResult<()> {
    conn.execute(
        "DELETE FROM bible_fts WHERE rowid IN (SELECT id FROM bible_verses WHERE version = ?1)",
        [code],
    )
    .map_err(map_err)?;
    // Verses go with the version (ON DELETE CASCADE).
    conn.execute("DELETE FROM bibles WHERE code = ?1", [code])
        .map_err(map_err)?;
    Ok(())
}

pub(crate) fn all_versions(conn: &Connection) -> StorageResult<Vec<BibleVersion>> {
    let mut st = conn
        .prepare_cached(&format!(
            "SELECT {VERSION_COLS} FROM bibles WHERE code NOT LIKE '{STAGING_PREFIX}%'
             ORDER BY sort_order, code"
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
    /// Installs (or reinstalls) a Bible version with its verses, crash-safely.
    ///
    /// `verses` must be in canonical order (Genesis to Revelation): the insertion order defines
    /// how passages that span chapters or books are resolved. The first version installed becomes
    /// the default; `is_default`/`is_secondary` on `version` keep their exclusivity.
    ///
    /// The ~31k verses go in batches of [`INSTALL_BATCH`] under a hidden staging code, each batch
    /// in its own short transaction that releases the connection lock in between, so queries and
    /// command acks are not stalled for the whole install. Only the last step, one short
    /// transaction, removes the previous copy of the same code and renames the staged rows into
    /// place. Until then the previous version stays fully usable; a crash leaves only staging
    /// rows, which [`Store::open`] (or [`Store::cleanup_installing`]) deletes.
    pub fn install_bible(&self, version: &BibleVersion, verses: &[Verse]) -> StorageResult<()> {
        self.install_staged(version, verses, None)
    }

    /// Removes leftover half-installed Bibles (an install interrupted by a crash). Runs on
    /// [`Store::open`]; returns how many were deleted.
    pub fn cleanup_installing(&self) -> StorageResult<usize> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_err)?;
        let n = cleanup_installing(&tx)?;
        tx.commit().map_err(map_err)?;
        Ok(n)
    }

    /// `install_bible`; `crash_after` (tests) abandons the install after that many verse
    /// batches without cleaning up, like a killed process would.
    pub(crate) fn install_staged(
        &self,
        version: &BibleVersion,
        verses: &[Verse],
        crash_after: Option<usize>,
    ) -> StorageResult<()> {
        let stage = staging_code(&version.code);
        let step = |f: &dyn Fn(&Connection) -> StorageResult<()>| -> StorageResult<()> {
            let mut conn = self.lock();
            let tx = conn.transaction().map_err(map_err)?;
            f(&tx)?;
            tx.commit().map_err(map_err)
        };
        step(&|c| install_header(c, version, &stage))?;
        let run = || -> StorageResult<bool> {
            for (i, chunk) in verses.chunks(INSTALL_BATCH).enumerate() {
                if crash_after == Some(i) {
                    return Ok(false);
                }
                step(&|c| install_verses(c, &stage, chunk))?;
                std::thread::yield_now();
            }
            if crash_after.is_some() {
                return Ok(false);
            }
            step(&|c| finish_install(c, version, &stage))?;
            // The new copy is live; a failure here only leaves a hidden copy for start-up.
            let _ = step(&|c| discard_retired(c, &version.code));
            Ok(true)
        };
        match run() {
            Ok(_) => Ok(()),
            Err(e) => {
                // Do not leave a half-installed copy behind; the previous version is untouched.
                let _ = step(&|c| remove_code(c, &stage));
                Err(e)
            }
        }
    }

    /// Sets the presentation order of the installed versions: `codes[0]` first. Versions not
    /// listed keep their relative order after the listed ones. Unknown codes are ignored.
    pub fn set_bible_order(&self, codes: &[VersionCode]) -> StorageResult<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_err)?;
        let current: Vec<String> = all_versions(&tx)?.into_iter().map(|v| v.code.0).collect();
        let listed = codes
            .iter()
            .filter(|c| current.contains(&c.0))
            .map(|c| c.0.clone());
        let mut order: Vec<String> = Vec::new();
        for c in listed {
            if !order.contains(&c) {
                order.push(c);
            }
        }
        for c in &current {
            if !order.contains(c) {
                order.push(c.clone());
            }
        }
        for (i, code) in order.iter().enumerate() {
            tx.execute(
                "UPDATE bibles SET sort_order = ?2 WHERE code = ?1",
                params![code, i as i64],
            )
            .map_err(map_err)?;
        }
        tx.commit().map_err(map_err)
    }

    /// Number of verses stored for a version.
    pub fn verse_count(&self, code: &VersionCode) -> StorageResult<u32> {
        self.read()
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
        all_versions(&self.read())
    }

    fn get_version(&self, code: &VersionCode) -> StorageResult<Option<BibleVersion>> {
        self.read()
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
        let conn = self.read();
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
        let conn = self.read();
        let mut st = conn
            .prepare_cached(
                "SELECT v.version, v.book, v.chapter, v.verse, v.text
                 FROM bible_fts f JOIN bible_verses v ON v.id = f.rowid
                 WHERE bible_fts MATCH ?1 AND (?2 IS NULL OR v.version = ?2)
                   AND v.version NOT LIKE ?4
                 ORDER BY f.rank, v.id
                 LIMIT ?3",
            )
            .map_err(map_err)?;
        let rows = st
            .query_map(
                params![
                    query,
                    version.map(|v| v.0.as_str()),
                    limit,
                    format!("{STAGING_PREFIX}%")
                ],
                |r| {
                    Ok(BibleHit {
                        version: VersionCode(r.get(0)?),
                        reference: ref_of(r.get(1)?, r.get(2)?, r.get(3)?),
                        text: r.get(4)?,
                    })
                },
            )
            .map_err(map_err)?
            .collect::<Result<_, _>>()
            .map_err(map_err)?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn bible(code: &str) -> BibleVersion {
        BibleVersion {
            code: VersionCode(code.into()),
            name: code.into(),
            language: "es".into(),
            license: "PD".into(),
            copyright_text: String::new(),
            is_public_domain: true,
            versification: "kjv".into(),
            sort_order: 0,
            is_enabled: true,
            is_default: false,
            is_secondary: false,
        }
    }

    fn full_bible(tag: &str) -> Vec<Verse> {
        (0..31_000u32)
            .map(|i| Verse {
                reference: BibleRef::verse("Gen", (i / 100 + 1) as u16, (i % 100 + 1) as u16),
                text: format!("{tag} en el principio creo Dios los cielos y la tierra {i}"),
                marks: vec![],
            })
            .collect()
    }

    /// The swap holds the writer lock, so it must not scale with the 31k-verse Bible: the FTS
    /// index is never rewritten, and the old copy is deleted outside it.
    #[test]
    fn swap_of_a_full_size_bible_is_short() {
        let s = Store::open_in_memory().unwrap();
        let v = bible("RVR");
        s.install_bible(&v, &full_bible("viejo")).unwrap();

        let stage = staging_code(&v.code);
        let verses = full_bible("nuevo");
        s.lock()
            .transaction()
            .and_then(|tx| {
                install_header(&tx, &v, &stage).unwrap();
                tx.commit()
            })
            .unwrap();
        for chunk in verses.chunks(INSTALL_BATCH) {
            let mut conn = s.lock();
            let tx = conn.transaction().unwrap();
            install_verses(&tx, &stage, chunk).unwrap();
            tx.commit().unwrap();
        }
        let t = Instant::now();
        {
            let mut conn = s.lock();
            let tx = conn.transaction().unwrap();
            finish_install(&tx, &v, &stage).unwrap();
            tx.commit().unwrap();
        }
        let swap = t.elapsed();
        // Re-indexing 31k rows took seconds; a rename of plain rows is a few hundred ms at worst.
        // The bound is deliberately loose so slow CI machines do not flake.
        assert!(swap < Duration::from_secs(2), "swap took {swap:?}");

        // Before the old copy is discarded it is hidden from search and listings.
        let code = &v.code;
        assert_eq!(s.verse_count(code).unwrap(), 31_000);
        assert!(s.search("viejo", None, 5).unwrap().is_empty());
        assert_eq!(s.search("nuevo", Some(code), 5).unwrap().len(), 5);
        let c = s.lock();
        discard_retired(&c, code).unwrap();
        let left: i64 = c
            .query_row("SELECT count(*) FROM bible_verses", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 31_000);
    }
}
