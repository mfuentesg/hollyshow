//! Paired devices. Only the SHA-256 of each session token is stored (in the `settings` table via
//! the storage trait), so a copied database cannot be replayed against the server.

use crate::util::{hash_token, now_ms, random_hex};
use hollyshow_core::Role;
use hollyshow_storage_api::SettingsRepo;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

const SETTINGS_KEY: &str = "server.devices.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Record {
    id: String,
    name: String,
    role: Role,
    token_hash: String,
    created_ms: i64,
    last_seen_ms: i64,
}

/// A paired device as shown to the operator (never carries the token or its hash).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub created_ms: i64,
    pub last_seen_ms: i64,
}

impl From<&Record> for PairedDevice {
    fn from(r: &Record) -> Self {
        Self {
            id: r.id.clone(),
            name: r.name.clone(),
            role: r.role,
            created_ms: r.created_ms,
            last_seen_ms: r.last_seen_ms,
        }
    }
}

pub struct DeviceStore {
    settings: Arc<dyn SettingsRepo>,
    records: Mutex<Vec<Record>>,
    max: usize,
}

impl DeviceStore {
    pub fn load(settings: Arc<dyn SettingsRepo>, max: usize) -> Self {
        let records = match settings.get(SETTINGS_KEY) {
            Ok(Some(raw)) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                log::warn!("ignoring unreadable paired-device list: {e}");
                Vec::new()
            }),
            Ok(None) => Vec::new(),
            Err(e) => {
                log::warn!("could not read paired devices: {e}");
                Vec::new()
            }
        };
        Self {
            settings,
            records: Mutex::new(records),
            max: max.max(1),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Record>> {
        self.records.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn persist(&self, records: &[Record]) {
        match serde_json::to_string(records) {
            Ok(json) => {
                if let Err(e) = self.settings.set(SETTINGS_KEY, &json) {
                    log::warn!("could not save paired devices: {e}");
                }
            }
            Err(e) => log::warn!("could not serialise paired devices: {e}"),
        }
    }

    /// Registers a device and returns it with its **plain** session token (shown once, to the
    /// device). `None` only if the OS cannot supply randomness.
    pub fn add(&self, name: &str, role: Role) -> Option<(PairedDevice, String)> {
        let token = random_hex(32)?;
        let now = now_ms();
        let record = Record {
            id: format!("d-{}", random_hex(6)?),
            name: name.to_owned(),
            role,
            token_hash: hash_token(&token),
            created_ms: now,
            last_seen_ms: now,
        };
        let device = PairedDevice::from(&record);
        let mut records = self.lock();
        records.push(record);
        while records.len() > self.max {
            // Forget the device not seen for longest.
            if let Some(oldest) = records
                .iter()
                .enumerate()
                .min_by_key(|(_, r)| r.last_seen_ms)
                .map(|(i, _)| i)
            {
                records.remove(oldest);
            }
        }
        self.persist(&records);
        Some((device, token))
    }

    /// Finds the device owning `token` and stamps its last-seen time.
    pub fn authenticate(&self, token: &str) -> Option<PairedDevice> {
        let hash = hash_token(token);
        let mut records = self.lock();
        let rec = records.iter_mut().find(|r| r.token_hash == hash)?;
        rec.last_seen_ms = now_ms();
        let device = PairedDevice::from(&*rec);
        self.persist(&records);
        Some(device)
    }

    /// Verifies a token without touching storage (HTTP API calls).
    pub fn peek(&self, token: &str) -> Option<PairedDevice> {
        let hash = hash_token(token);
        self.lock()
            .iter()
            .find(|r| r.token_hash == hash)
            .map(PairedDevice::from)
    }

    pub fn revoke(&self, id: &str) -> bool {
        let mut records = self.lock();
        let before = records.len();
        records.retain(|r| r.id != id);
        let removed = records.len() != before;
        if removed {
            self.persist(&records);
        }
        removed
    }

    pub fn list(&self) -> Vec<PairedDevice> {
        self.lock().iter().map(PairedDevice::from).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_storage_api::memory::MemoryStore;

    fn settings() -> Arc<dyn SettingsRepo> {
        MemoryStore::repos().settings
    }

    #[test]
    fn tokens_authenticate_and_only_the_hash_is_stored() {
        let s = settings();
        let store = DeviceStore::load(s.clone(), 8);
        let (dev, token) = store.add("iPad", Role::Musician).unwrap();
        assert_eq!(store.authenticate(&token).unwrap().id, dev.id);
        assert!(store.authenticate("nope").is_none());
        let raw = s.get(SETTINGS_KEY).unwrap().unwrap();
        assert!(!raw.contains(&token), "plain token must never be persisted");
        assert!(raw.contains(&hash_token(&token)));
    }

    #[test]
    fn devices_survive_a_restart_and_can_be_revoked() {
        let s = settings();
        let (token, id) = {
            let store = DeviceStore::load(s.clone(), 8);
            let (d, t) = store.add("Pastor", Role::Pastor).unwrap();
            (t, d.id)
        };
        let store = DeviceStore::load(s, 8);
        assert_eq!(store.peek(&token).unwrap().role, Role::Pastor);
        assert!(store.revoke(&id));
        assert!(store.peek(&token).is_none());
        assert!(!store.revoke(&id));
    }

    #[test]
    fn the_oldest_device_is_forgotten_past_the_cap() {
        let store = DeviceStore::load(settings(), 2);
        let (a, ta) = store.add("a", Role::Stage).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
        store.add("b", Role::Stage).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
        store.add("c", Role::Stage).unwrap();
        assert_eq!(store.list().len(), 2);
        assert!(store.peek(&ta).is_none(), "{} was the oldest", a.id);
    }

    #[test]
    fn corrupt_storage_starts_empty() {
        let s = settings();
        s.set(SETTINGS_KEY, "{not json").unwrap();
        assert!(DeviceStore::load(s, 4).list().is_empty());
    }
}
