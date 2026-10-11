//! Paired devices. Only the SHA-256 of each session token is stored (in the `settings` table via
//! the storage trait), so a copied database cannot be replayed against the server.
//!
//! Tokens are bearer secrets that travel over the (cleartext) LAN, so they are short-lived by
//! design: a device unused for `idle` or older than `max_age` must pair again, and the token is
//! replaced by a fresh one when the device reconnects after `rotate_after`. The old token keeps
//! working for [`PREV_GRACE_MS`] so a reconnect that dies before the new token arrives does not
//! lock the phone out. The `last seen` stamp is kept in memory and written to storage at most
//! every `persist_every`.

use crate::util::{hash_token, now_ms, random_hex};
use hollyshow_core::Role;
use hollyshow_storage_api::SettingsRepo;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the previous token stays valid after a rotation.
const PREV_GRACE_MS: i64 = 24 * 60 * 60 * 1000;

/// Lifetime rules for session tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub idle: Duration,
    pub max_age: Duration,
    pub rotate_after: Duration,
    pub persist_every: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        let cfg = crate::config::ServerConfig::default();
        Self::from_config(&cfg)
    }
}

impl Policy {
    pub fn from_config(c: &crate::config::ServerConfig) -> Self {
        Self {
            idle: c.device_idle_expiry,
            max_age: c.device_max_age,
            rotate_after: c.token_rotate_after,
            persist_every: c.last_seen_write_interval,
        }
    }
}

fn ms(d: Duration) -> i64 {
    i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
}

/// Result of a successful token check on the WebSocket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticated {
    pub device: PairedDevice,
    /// Plain replacement token the client must store, when this resume rotated it.
    pub new_token: Option<String>,
}

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
    /// When the current token was issued (`0` in records saved before rotation existed).
    #[serde(default)]
    token_issued_ms: i64,
    /// Hash of the token this one replaced, accepted until the new one is used (or the grace
    /// ends) so a lost `welcome` cannot lock a device out.
    #[serde(default)]
    prev_hash: Option<String>,
}

impl Record {
    fn issued(&self) -> i64 {
        if self.token_issued_ms > 0 {
            self.token_issued_ms
        } else {
            self.created_ms
        }
    }

    fn expired(&self, now: i64, p: &Policy) -> bool {
        now.saturating_sub(self.last_seen_ms) > ms(p.idle)
            || now.saturating_sub(self.created_ms) > ms(p.max_age)
    }

    fn matches(&self, hash: &str, now: i64) -> Option<bool> {
        if self.token_hash == hash {
            return Some(false);
        }
        if self.prev_hash.as_deref() == Some(hash)
            && now.saturating_sub(self.issued()) < PREV_GRACE_MS
        {
            return Some(true);
        }
        None
    }
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

struct State {
    records: Vec<Record>,
    last_persist_ms: i64,
}

pub struct DeviceStore {
    settings: Arc<dyn SettingsRepo>,
    state: Mutex<State>,
    max: usize,
    policy: Policy,
}

impl DeviceStore {
    #[cfg(test)]
    pub fn load(settings: Arc<dyn SettingsRepo>, max: usize) -> Self {
        Self::load_with(settings, max, Policy::default())
    }

    pub fn load_with(settings: Arc<dyn SettingsRepo>, max: usize, policy: Policy) -> Self {
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
            state: Mutex::new(State {
                records,
                last_persist_ms: now_ms(),
            }),
            max: max.max(1),
            policy,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn persist(&self, st: &mut State, now: i64) {
        st.last_persist_ms = now;
        match serde_json::to_string(&st.records) {
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
            token_issued_ms: now,
            prev_hash: None,
        };
        let device = PairedDevice::from(&record);
        let mut st = self.lock();
        let policy = self.policy;
        st.records.retain(|r| !r.expired(now, &policy));
        let records = &mut st.records;
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
        self.persist(&mut st, now);
        Some((device, token))
    }

    /// Finds the device owning `token`, stamps its last-seen time and rotates the token when it
    /// is due. Expired devices are forgotten and refused.
    pub fn authenticate(&self, token: &str) -> Option<Authenticated> {
        self.authenticate_at(token, now_ms())
    }

    pub(crate) fn authenticate_at(&self, token: &str, now: i64) -> Option<Authenticated> {
        let hash = hash_token(token);
        let mut st = self.lock();
        let policy = self.policy;
        let idx = st
            .records
            .iter()
            .position(|r| r.matches(&hash, now).is_some())?;
        if st.records[idx].expired(now, &policy) {
            st.records.remove(idx);
            self.persist(&mut st, now);
            return None;
        }
        let was_prev = st.records[idx].matches(&hash, now) == Some(true);
        let mut dirty = false;
        let mut new_token = None;
        {
            let rec = &mut st.records[idx];
            if !was_prev && rec.prev_hash.take().is_some() {
                // The device proved it holds the new token: the old one is dead.
                dirty = true;
            }
            if was_prev || now.saturating_sub(rec.issued()) > ms(policy.rotate_after) {
                if let Some(fresh) = random_hex(32) {
                    rec.prev_hash = Some(hash);
                    rec.token_hash = hash_token(&fresh);
                    rec.token_issued_ms = now;
                    new_token = Some(fresh);
                    dirty = true;
                }
            }
            rec.last_seen_ms = now;
        }
        let device = PairedDevice::from(&st.records[idx]);
        if dirty || now.saturating_sub(st.last_persist_ms) >= ms(policy.persist_every) {
            self.persist(&mut st, now);
        }
        Some(Authenticated { device, new_token })
    }

    /// Verifies a token without touching storage (HTTP API calls).
    pub fn peek(&self, token: &str) -> Option<PairedDevice> {
        self.peek_at(token, now_ms())
    }

    pub(crate) fn peek_at(&self, token: &str, now: i64) -> Option<PairedDevice> {
        let hash = hash_token(token);
        let policy = self.policy;
        self.lock()
            .records
            .iter()
            .find(|r| r.matches(&hash, now).is_some() && !r.expired(now, &policy))
            .map(PairedDevice::from)
    }

    pub fn revoke(&self, id: &str) -> bool {
        let mut st = self.lock();
        let before = st.records.len();
        st.records.retain(|r| r.id != id);
        let removed = st.records.len() != before;
        if removed {
            self.persist(&mut st, now_ms());
        }
        removed
    }

    pub fn list(&self) -> Vec<PairedDevice> {
        let (now, policy) = (now_ms(), self.policy);
        self.lock()
            .records
            .iter()
            .filter(|r| !r.expired(now, &policy))
            .map(PairedDevice::from)
            .collect()
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
        assert_eq!(store.authenticate(&token).unwrap().device.id, dev.id);
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

    const DAY: i64 = 24 * 60 * 60 * 1000;

    fn aged_store() -> (DeviceStore, Arc<dyn SettingsRepo>) {
        let s = settings();
        (DeviceStore::load(s.clone(), 8), s)
    }

    #[test]
    fn idle_and_absolute_expiry_force_a_new_pairing() {
        let (store, _) = aged_store();
        let (dev, token) = store.add("a", Role::Operator).unwrap();
        let t0 = dev.created_ms;
        assert!(store.peek_at(&token, t0 + 29 * DAY).is_some());
        assert!(store.peek_at(&token, t0 + 31 * DAY).is_none(), "idle");
        assert!(store.authenticate_at(&token, t0 + 31 * DAY).is_none());
        assert!(store.list().is_empty(), "expired devices are forgotten");

        let (dev, token) = store.add("b", Role::Operator).unwrap();
        // Kept alive by activity every 20 days, it still dies at the absolute limit.
        let mut now = dev.created_ms;
        let mut token = token;
        while now - dev.created_ms < 170 * DAY {
            now += 20 * DAY;
            token = match store.authenticate_at(&token, now) {
                Some(a) => a.new_token.unwrap_or(token),
                None => panic!("still valid at day {}", (now - dev.created_ms) / DAY),
            };
        }
        assert!(store
            .authenticate_at(&token, dev.created_ms + 181 * DAY)
            .is_none());
    }

    #[test]
    fn tokens_rotate_and_the_previous_one_dies_once_the_new_one_is_used() {
        let (store, s) = aged_store();
        let (dev, old) = store.add("a", Role::Musician).unwrap();
        let t0 = dev.created_ms;
        // Fresh tokens are not rotated.
        assert!(store
            .authenticate_at(&old, t0 + DAY)
            .unwrap()
            .new_token
            .is_none());
        let a = store.authenticate_at(&old, t0 + 8 * DAY).unwrap();
        let new = a.new_token.expect("rotated after a week");
        assert_ne!(new, old);
        // The reply carrying `new` is lost: the old token still resumes (and re-issues).
        let again = store.authenticate_at(&old, t0 + 8 * DAY + 1000).unwrap();
        let newer = again
            .new_token
            .expect("re-issued because the old token was used");
        assert!(
            store.peek_at(&new, t0 + 8 * DAY + 2000).is_none(),
            "{new} was superseded"
        );
        // Using the newest token kills the previous one.
        assert!(store
            .authenticate_at(&newer, t0 + 8 * DAY + 3000)
            .unwrap()
            .new_token
            .is_none());
        assert!(store.peek_at(&old, t0 + 8 * DAY + 4000).is_none());
        let raw = s.get(SETTINGS_KEY).unwrap().unwrap();
        assert!(!raw.contains(&newer) && !raw.contains(&old));
    }

    #[test]
    fn a_stale_previous_token_stops_working_after_the_grace() {
        let (store, _) = aged_store();
        let (dev, old) = store.add("a", Role::Stage).unwrap();
        let t0 = dev.created_ms;
        store
            .authenticate_at(&old, t0 + 8 * DAY)
            .unwrap()
            .new_token
            .unwrap();
        assert!(store.peek_at(&old, t0 + 8 * DAY + 1000).is_some());
        assert!(store.peek_at(&old, t0 + 10 * DAY).is_none());
    }

    #[test]
    fn last_seen_is_written_to_storage_only_every_few_minutes() {
        let (store, s) = aged_store();
        let (dev, token) = store.add("a", Role::Stage).unwrap();
        let t0 = dev.created_ms;
        let stored = |s: &Arc<dyn SettingsRepo>| -> Vec<Record> {
            serde_json::from_str(&s.get(SETTINGS_KEY).unwrap().unwrap()).unwrap()
        };
        store.authenticate_at(&token, t0 + 60_000).unwrap();
        assert_eq!(stored(&s)[0].last_seen_ms, t0, "not persisted after 1 min");
        // In-memory value is current.
        assert_eq!(store.list()[0].last_seen_ms, t0 + 60_000);
        store.authenticate_at(&token, t0 + 6 * 60_000).unwrap();
        assert_eq!(stored(&s)[0].last_seen_ms, t0 + 6 * 60_000);
    }
}
