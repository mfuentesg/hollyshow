//! `ThemeRepo`, `PlaylistRepo` and `SettingsRepo`.

use crate::{from_json, map_err, to_json, Store};
use hollyshow_core::{Playlist, PlaylistId, PlaylistSummary, Theme, ThemeId};
use hollyshow_storage_api::{PlaylistRepo, SettingsRepo, StorageResult, ThemeRepo};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

pub(crate) fn save_theme(conn: &Connection, theme: &Theme) -> StorageResult<()> {
    conn.execute(
        "INSERT INTO themes (id, name, json) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, json = excluded.json",
        params![theme.id.as_str(), theme.name, to_json(theme)?],
    )
    .map_err(map_err)?;
    Ok(())
}

pub(crate) fn all_themes(conn: &Connection) -> StorageResult<Vec<Theme>> {
    let mut st = conn
        .prepare_cached("SELECT json FROM themes ORDER BY name, id")
        .map_err(map_err)?;
    let rows: Vec<String> = st
        .query_map([], |r| r.get(0))
        .map_err(map_err)?
        .collect::<Result<_, _>>()
        .map_err(map_err)?;
    rows.iter().map(|j| from_json(j)).collect()
}

impl ThemeRepo for Store {
    fn list(&self) -> StorageResult<Vec<Theme>> {
        all_themes(&self.lock())
    }

    fn get(&self, id: &ThemeId) -> StorageResult<Option<Theme>> {
        let json: Option<String> = self
            .lock()
            .query_row(
                "SELECT json FROM themes WHERE id = ?1",
                [id.as_str()],
                |r| r.get(0),
            )
            .optional()
            .map_err(map_err)?;
        json.map(|j| from_json(&j)).transpose()
    }

    fn save(&self, theme: &Theme) -> StorageResult<()> {
        save_theme(&self.lock(), theme)
    }

    fn delete(&self, id: &ThemeId) -> StorageResult<()> {
        self.lock()
            .execute("DELETE FROM themes WHERE id = ?1", [id.as_str()])
            .map_err(map_err)?;
        Ok(())
    }
}

pub(crate) fn save_playlist(conn: &Connection, p: &Playlist, now_ms: i64) -> StorageResult<()> {
    conn.execute(
        "INSERT INTO playlists (id, name, date, is_template, items, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, date = excluded.date,
            is_template = excluded.is_template, items = excluded.items,
            updated_at = excluded.updated_at",
        params![
            p.id.as_str(),
            p.name,
            p.date,
            p.is_template,
            to_json(&p.items)?,
            now_ms
        ],
    )
    .map_err(map_err)?;
    Ok(())
}

pub(crate) fn load_playlist(conn: &Connection, id: &str) -> StorageResult<Option<Playlist>> {
    let row = conn
        .query_row(
            "SELECT id, name, date, is_template, items FROM playlists WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(map_err)?;
    row.map(|(id, name, date, is_template, items)| {
        Ok(Playlist {
            id: PlaylistId::new(id),
            name,
            date,
            items: from_json(&items)?,
            is_template,
        })
    })
    .transpose()
}

pub(crate) fn all_playlist_ids(conn: &Connection) -> StorageResult<Vec<String>> {
    let mut st = conn
        .prepare_cached("SELECT id FROM playlists ORDER BY id")
        .map_err(map_err)?;
    let ids = st
        .query_map([], |r| r.get(0))
        .map_err(map_err)?
        .collect::<Result<_, _>>()
        .map_err(map_err)?;
    Ok(ids)
}

impl PlaylistRepo for Store {
    /// Dated services newest first, then undated ones; templates are flagged, not segregated.
    fn list(&self) -> StorageResult<Vec<PlaylistSummary>> {
        let conn = self.lock();
        let mut st = conn
            .prepare_cached(
                "SELECT id, name, date, json_array_length(items), is_template FROM playlists
                 ORDER BY date IS NULL, date DESC, name, id",
            )
            .map_err(map_err)?;
        let rows = st
            .query_map([], |r| {
                Ok(PlaylistSummary {
                    id: PlaylistId::new(r.get::<_, String>(0)?),
                    name: r.get(1)?,
                    date: r.get(2)?,
                    item_count: r.get(3)?,
                    is_template: r.get(4)?,
                })
            })
            .map_err(map_err)?
            .collect::<Result<_, _>>()
            .map_err(map_err)?;
        Ok(rows)
    }

    fn get(&self, id: &PlaylistId) -> StorageResult<Option<Playlist>> {
        load_playlist(&self.lock(), id.as_str())
    }

    fn save(&self, playlist: &Playlist) -> StorageResult<()> {
        save_playlist(&self.lock(), playlist, crate::backup::now_ms())
    }

    fn delete(&self, id: &PlaylistId) -> StorageResult<()> {
        self.lock()
            .execute("DELETE FROM playlists WHERE id = ?1", [id.as_str()])
            .map_err(map_err)?;
        Ok(())
    }
}

pub(crate) fn all_settings(conn: &Connection) -> StorageResult<BTreeMap<String, String>> {
    let mut st = conn
        .prepare_cached("SELECT key, value FROM settings ORDER BY key")
        .map_err(map_err)?;
    let rows = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(map_err)?
        .collect::<Result<_, _>>()
        .map_err(map_err)?;
    Ok(rows)
}

pub(crate) fn set_setting(conn: &Connection, key: &str, value: &str) -> StorageResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(map_err)?;
    Ok(())
}

impl SettingsRepo for Store {
    fn get(&self, key: &str) -> StorageResult<Option<String>> {
        self.lock()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(map_err)
    }

    fn set(&self, key: &str, value: &str) -> StorageResult<()> {
        set_setting(&self.lock(), key, value)
    }

    fn remove(&self, key: &str) -> StorageResult<()> {
        self.lock()
            .execute("DELETE FROM settings WHERE key = ?1", [key])
            .map_err(map_err)?;
        Ok(())
    }
}
