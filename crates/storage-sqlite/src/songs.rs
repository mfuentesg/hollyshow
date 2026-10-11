//! `SongRepo`: songs, sections, arrangements, FTS5 index and the CCLI usage log.

use crate::{from_json, fts_query, map_err, to_json, Store};
use hollyshow_core::text::fold;
use hollyshow_core::{
    Arrangement, ArrangementId, Section, SectionId, SectionKind, Song, SongId, SongSummary, ThemeId,
};
use hollyshow_storage_api::{SongRepo, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// One row of the CCLI report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub song_id: SongId,
    pub title: Option<String>,
    pub ccli: Option<String>,
    pub times_shown: u32,
    pub last_shown_at: i64,
}

fn kind_to_str(k: SectionKind) -> StorageResult<String> {
    match serde_json::to_value(k) {
        Ok(serde_json::Value::String(s)) => Ok(s),
        _ => Err(StorageError::Invalid("section kind".into())),
    }
}

fn kind_from_str(s: &str) -> SectionKind {
    // An unknown kind (written by a newer version) degrades to `Other` instead of failing.
    serde_json::from_value(serde_json::Value::String(s.to_owned())).unwrap_or(SectionKind::Other)
}

/// Removes inline ChordPro chords for indexing and snippets.
fn strip_chords(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_chord = false;
    for c in line.chars() {
        match c {
            '[' => in_chord = true,
            ']' if in_chord => in_chord = false,
            _ if in_chord => {}
            _ => out.push(c),
        }
    }
    out
}

fn validate(song: &Song) -> StorageResult<()> {
    if song.id.as_str().is_empty() {
        return Err(StorageError::Invalid("song id is empty".into()));
    }
    let mut seen = HashSet::new();
    if !song.sections.iter().all(|s| seen.insert(s.id.as_str())) {
        return Err(StorageError::Invalid(format!(
            "song {} has duplicate section ids",
            song.id
        )));
    }
    let mut seen = HashSet::new();
    if !song.arrangements.iter().all(|a| seen.insert(a.id.as_str())) {
        return Err(StorageError::Invalid(format!(
            "song {} has duplicate arrangement ids",
            song.id
        )));
    }
    Ok(())
}

/// Inserts or replaces a song with its sections, arrangements and FTS row. Call inside a
/// transaction.
pub(crate) fn save_song(conn: &Connection, song: &Song) -> StorageResult<()> {
    validate(song)?;
    conn.execute(
        "INSERT INTO songs (id, title, title_sort, alt_title, author, copyright, ccli, key, bpm,
                            tags, theme_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title, title_sort = excluded.title_sort,
            alt_title = excluded.alt_title, author = excluded.author,
            copyright = excluded.copyright, ccli = excluded.ccli, key = excluded.key,
            bpm = excluded.bpm, tags = excluded.tags, theme_id = excluded.theme_id,
            created_at = excluded.created_at, updated_at = excluded.updated_at",
        params![
            song.id.as_str(),
            song.title,
            fold(&song.title),
            song.alt_title,
            song.author,
            song.copyright,
            song.ccli,
            song.key,
            song.bpm,
            to_json(&song.tags)?,
            song.theme_id.as_ref().map(ThemeId::as_str),
            song.created_at,
            song.updated_at,
        ],
    )
    .map_err(map_err)?;
    conn.execute(
        "DELETE FROM song_sections WHERE song_id = ?1",
        [song.id.as_str()],
    )
    .map_err(map_err)?;
    conn.execute(
        "DELETE FROM song_arrangements WHERE song_id = ?1",
        [song.id.as_str()],
    )
    .map_err(map_err)?;
    {
        let mut ins = conn
            .prepare_cached(
                "INSERT INTO song_sections (song_id, id, position, kind, label, lyrics, translation)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .map_err(map_err)?;
        for (i, s) in song.sections.iter().enumerate() {
            ins.execute(params![
                song.id.as_str(),
                s.id.as_str(),
                i as i64,
                kind_to_str(s.kind)?,
                s.label,
                s.lyrics,
                s.translation
            ])
            .map_err(map_err)?;
        }
        let mut ins = conn
            .prepare_cached(
                "INSERT INTO song_arrangements (song_id, id, position, name, sequence)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(map_err)?;
        for (i, a) in song.arrangements.iter().enumerate() {
            ins.execute(params![
                song.id.as_str(),
                a.id.as_str(),
                i as i64,
                a.name,
                to_json(&a.sequence)?
            ])
            .map_err(map_err)?;
        }
    }
    let rowid: i64 = conn
        .query_row(
            "SELECT rowid FROM songs WHERE id = ?1",
            [song.id.as_str()],
            |r| r.get(0),
        )
        .map_err(map_err)?;
    conn.execute("DELETE FROM songs_fts WHERE rowid = ?1", [rowid])
        .map_err(map_err)?;
    let title = match &song.alt_title {
        Some(a) => format!("{} {}", song.title, a),
        None => song.title.clone(),
    };
    let lyrics = song
        .sections
        .iter()
        .flat_map(|s| {
            std::iter::once(strip_chords(&s.lyrics))
                .chain(s.translation.as_deref().map(strip_chords))
        })
        .collect::<Vec<_>>()
        .join("\n");
    conn.execute(
        "INSERT INTO songs_fts (rowid, title, lyrics, author) VALUES (?1, ?2, ?3, ?4)",
        params![
            rowid,
            fold(&title),
            fold(&lyrics),
            fold(song.author.as_deref().unwrap_or(""))
        ],
    )
    .map_err(map_err)?;
    Ok(())
}

pub(crate) fn load_song(conn: &Connection, id: &str) -> StorageResult<Option<Song>> {
    let head = conn
        .query_row(
            "SELECT id, title, alt_title, author, copyright, ccli, key, bpm, tags, theme_id,
                    created_at, updated_at FROM songs WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    Song {
                        id: SongId::new(r.get::<_, String>(0)?),
                        title: r.get(1)?,
                        alt_title: r.get(2)?,
                        author: r.get(3)?,
                        copyright: r.get(4)?,
                        ccli: r.get(5)?,
                        key: r.get(6)?,
                        bpm: r.get(7)?,
                        tags: vec![],
                        theme_id: r.get::<_, Option<String>>(9)?.map(ThemeId::new),
                        sections: vec![],
                        arrangements: vec![],
                        created_at: r.get(10)?,
                        updated_at: r.get(11)?,
                    },
                    r.get::<_, String>(8)?,
                ))
            },
        )
        .optional()
        .map_err(map_err)?;
    let Some((mut song, tags)) = head else {
        return Ok(None);
    };
    song.tags = from_json(&tags)?;
    let mut st = conn
        .prepare_cached(
            "SELECT id, kind, label, lyrics, translation FROM song_sections
             WHERE song_id = ?1 ORDER BY position",
        )
        .map_err(map_err)?;
    song.sections = st
        .query_map([id], |r| {
            Ok(Section {
                id: SectionId::new(r.get::<_, String>(0)?),
                kind: kind_from_str(&r.get::<_, String>(1)?),
                label: r.get(2)?,
                lyrics: r.get(3)?,
                translation: r.get(4)?,
            })
        })
        .map_err(map_err)?
        .collect::<Result<_, _>>()
        .map_err(map_err)?;
    let mut st = conn
        .prepare_cached(
            "SELECT id, name, sequence FROM song_arrangements WHERE song_id = ?1 ORDER BY position",
        )
        .map_err(map_err)?;
    let rows: Vec<(String, String, String)> = st
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(map_err)?
        .collect::<Result<_, _>>()
        .map_err(map_err)?;
    for (aid, name, seq) in rows {
        song.arrangements.push(Arrangement {
            id: ArrangementId::new(aid),
            name,
            sequence: from_json(&seq)?,
        });
    }
    Ok(Some(song))
}

pub(crate) fn all_song_ids(conn: &Connection) -> StorageResult<Vec<String>> {
    let mut st = conn
        .prepare_cached("SELECT id FROM songs ORDER BY title_sort, id")
        .map_err(map_err)?;
    let ids = st
        .query_map([], |r| r.get(0))
        .map_err(map_err)?
        .collect::<Result<_, _>>()
        .map_err(map_err)?;
    Ok(ids)
}

/// First lyric line (chords removed) that contains one of the query terms.
fn snippet(conn: &Connection, id: &str, terms: &[String]) -> StorageResult<Option<String>> {
    let mut st = conn
        .prepare_cached("SELECT lyrics FROM song_sections WHERE song_id = ?1 ORDER BY position")
        .map_err(map_err)?;
    let lyrics: Vec<String> = st
        .query_map([id], |r| r.get(0))
        .map_err(map_err)?
        .collect::<Result<_, _>>()
        .map_err(map_err)?;
    for block in lyrics {
        for line in block.lines() {
            let plain = strip_chords(line);
            let f = fold(&plain);
            if terms.iter().any(|t| f.contains(t.as_str())) {
                let t = plain.trim();
                if !t.is_empty() {
                    return Ok(Some(t.to_owned()));
                }
            }
        }
    }
    Ok(None)
}

fn summary_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SongSummary> {
    Ok(SongSummary {
        id: SongId::new(r.get::<_, String>(0)?),
        title: r.get(1)?,
        author: r.get(2)?,
        key: r.get(3)?,
        snippet: None,
    })
}

impl SongRepo for Store {
    fn search(&self, text: &str, limit: u32) -> StorageResult<Vec<SongSummary>> {
        let conn = self.lock();
        let limit = i64::from(limit);
        if text.trim().is_empty() {
            let mut st = conn
                .prepare_cached(
                    "SELECT id, title, author, key FROM songs ORDER BY title_sort, id LIMIT ?1",
                )
                .map_err(map_err)?;
            let rows = st
                .query_map([limit], summary_row)
                .map_err(map_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_err)?;
            return Ok(rows);
        }
        let Some(query) = fts_query(text) else {
            return Ok(vec![]);
        };
        let mut st = conn
            .prepare_cached(
                "SELECT s.id, s.title, s.author, s.key
                 FROM songs_fts JOIN songs s ON s.rowid = songs_fts.rowid
                 WHERE songs_fts MATCH ?1
                 ORDER BY bm25(songs_fts, 8.0, 1.0, 3.0), s.title_sort
                 LIMIT ?2",
            )
            .map_err(map_err)?;
        let mut rows = st
            .query_map(params![query, limit], summary_row)
            .map_err(map_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_err)?;
        let terms: Vec<String> = fold(text)
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect();
        for r in &mut rows {
            r.snippet = snippet(&conn, r.id.as_str(), &terms)?;
        }
        Ok(rows)
    }

    fn get(&self, id: &SongId) -> StorageResult<Option<Song>> {
        load_song(&self.lock(), id.as_str())
    }

    fn save(&self, song: &Song) -> StorageResult<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_err)?;
        save_song(&tx, song)?;
        tx.commit().map_err(map_err)
    }

    fn delete(&self, id: &SongId) -> StorageResult<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_err)?;
        let rowid: Option<i64> = tx
            .query_row(
                "SELECT rowid FROM songs WHERE id = ?1",
                [id.as_str()],
                |r| r.get(0),
            )
            .optional()
            .map_err(map_err)?;
        let Some(rowid) = rowid else {
            return Err(StorageError::NotFound(id.to_string()));
        };
        tx.execute("DELETE FROM songs_fts WHERE rowid = ?1", [rowid])
            .map_err(map_err)?;
        tx.execute("DELETE FROM songs WHERE id = ?1", [id.as_str()])
            .map_err(map_err)?;
        tx.commit().map_err(map_err)
    }

    fn log_usage(&self, id: &SongId, shown_at_ms: i64) -> StorageResult<()> {
        let conn = self.lock();
        // Snapshot title and CCLI number so reports survive edits and deletions.
        conn.execute(
            "INSERT INTO usage_log (song_id, title, ccli, shown_at)
             VALUES (?1, (SELECT title FROM songs WHERE id = ?1), (SELECT ccli FROM songs WHERE id = ?1), ?2)",
            params![id.as_str(), shown_at_ms],
        )
        .map_err(map_err)?;
        Ok(())
    }
}

impl Store {
    /// CCLI report: songs shown in `[from_ms, to_ms)`, most shown first.
    pub fn usage_report(&self, from_ms: i64, to_ms: i64) -> StorageResult<Vec<UsageRow>> {
        let conn = self.lock();
        let mut st = conn
            .prepare_cached(
                "SELECT song_id, MAX(title), MAX(ccli), COUNT(*), MAX(shown_at)
                 FROM usage_log WHERE shown_at >= ?1 AND shown_at < ?2
                 GROUP BY song_id ORDER BY COUNT(*) DESC, MAX(shown_at) DESC",
            )
            .map_err(map_err)?;
        let rows = st
            .query_map(params![from_ms, to_ms], |r| {
                Ok(UsageRow {
                    song_id: SongId::new(r.get::<_, String>(0)?),
                    title: r.get(1)?,
                    ccli: r.get(2)?,
                    times_shown: r.get(3)?,
                    last_shown_at: r.get(4)?,
                })
            })
            .map_err(map_err)?
            .collect::<Result<_, _>>()
            .map_err(map_err)?;
        Ok(rows)
    }

    /// Number of songs in the library.
    pub fn song_count(&self) -> StorageResult<u32> {
        self.lock()
            .query_row("SELECT count(*) FROM songs", [], |r| r.get(0))
            .map_err(map_err)
    }
}
