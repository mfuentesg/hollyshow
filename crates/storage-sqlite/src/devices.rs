//! Paired remote devices (`devices` table). Only a hash of the token is stored.

use crate::{map_err, Store};
use hollyshow_core::DeviceId;
use hollyshow_storage_api::{StorageError, StorageResult};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRecord {
    pub id: DeviceId,
    pub name: String,
    /// Wire role name (`operator`, `pastor`, ...); kept as text so adding roles needs no migration.
    pub role: String,
    pub token_hash: String,
    pub created_at: i64,
    pub last_seen: Option<i64>,
}

fn device_row(r: &Row<'_>) -> rusqlite::Result<DeviceRecord> {
    Ok(DeviceRecord {
        id: DeviceId::new(r.get::<_, String>(0)?),
        name: r.get(1)?,
        role: r.get(2)?,
        token_hash: r.get(3)?,
        created_at: r.get(4)?,
        last_seen: r.get(5)?,
    })
}

const COLS: &str = "id, name, role, token_hash, created_at, last_seen";

impl Store {
    /// Inserts a device or updates name, role and token of an existing one.
    pub fn save_device(&self, d: &DeviceRecord) -> StorageResult<()> {
        self.lock()
            .execute(
                "INSERT INTO devices (id, name, role, token_hash, created_at, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, role = excluded.role,
                    token_hash = excluded.token_hash, last_seen = excluded.last_seen",
                params![
                    d.id.as_str(),
                    d.name,
                    d.role,
                    d.token_hash,
                    d.created_at,
                    d.last_seen
                ],
            )
            .map_err(map_err)?;
        Ok(())
    }

    pub fn list_devices(&self) -> StorageResult<Vec<DeviceRecord>> {
        let conn = self.lock();
        let mut st = conn
            .prepare_cached(&format!(
                "SELECT {COLS} FROM devices ORDER BY created_at, id"
            ))
            .map_err(map_err)?;
        let rows = st
            .query_map([], device_row)
            .map_err(map_err)?
            .collect::<Result<_, _>>()
            .map_err(map_err)?;
        Ok(rows)
    }

    pub fn find_device_by_token_hash(&self, hash: &str) -> StorageResult<Option<DeviceRecord>> {
        self.lock()
            .query_row(
                &format!("SELECT {COLS} FROM devices WHERE token_hash = ?1"),
                [hash],
                device_row,
            )
            .optional()
            .map_err(map_err)
    }

    pub fn touch_device(&self, id: &DeviceId, seen_at_ms: i64) -> StorageResult<()> {
        let n = self
            .lock()
            .execute(
                "UPDATE devices SET last_seen = ?2 WHERE id = ?1",
                params![id.as_str(), seen_at_ms],
            )
            .map_err(map_err)?;
        if n == 0 {
            return Err(StorageError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// Unpairs a device (idempotent).
    pub fn remove_device(&self, id: &DeviceId) -> StorageResult<()> {
        self.lock()
            .execute("DELETE FROM devices WHERE id = ?1", [id.as_str()])
            .map_err(map_err)?;
        Ok(())
    }
}
