//! Live WebSocket connections: limits per kind, the operator's device list and kicking.

use hollyshow_core::Role;
use serde::Serialize;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// How a connection got in; decides which limit it counts against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnKind {
    /// Authenticated with a session token.
    Paired,
    /// Anonymous read-only phone in the audience.
    Congregation,
    /// OBS / browser-source output page.
    Output,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub id: u64,
    pub device_id: Option<String>,
    pub name: String,
    pub role: Role,
    pub kind: ConnKind,
    pub addr: String,
    pub connected_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub congregation: usize,
    pub outputs: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Full,
    Banned,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub total: usize,
    pub paired: usize,
    pub congregation: usize,
    pub outputs: usize,
}

struct Entry {
    info: ConnectionInfo,
    ip: IpAddr,
    kick: watch::Sender<Option<String>>,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<u64, Entry>,
    bans: HashMap<IpAddr, Instant>,
}

pub struct Connections {
    inner: Mutex<Inner>,
    next_id: AtomicU64,
    limits: Limits,
}

/// Held by the connection task; dropping it (normally, by error or by panic unwinding) frees the
/// slot, so a crashed socket can never leak capacity.
pub struct ConnGuard {
    pub id: u64,
    pub kicked: watch::Receiver<Option<String>>,
    conns: Arc<Connections>,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.conns.lock().entries.remove(&self.id);
    }
}

impl Connections {
    pub fn new(limits: Limits) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            next_id: AtomicU64::new(1),
            limits,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn register(
        self: &Arc<Self>,
        ip: IpAddr,
        device_id: Option<String>,
        name: String,
        role: Role,
        kind: ConnKind,
        connected_ms: i64,
    ) -> Result<ConnGuard, Refused> {
        let mut inner = self.lock();
        let now = Instant::now();
        inner.bans.retain(|_, until| *until > now);
        if kind != ConnKind::Paired && inner.bans.contains_key(&ip) {
            return Err(Refused::Banned);
        }
        let count = |k: ConnKind| inner.entries.values().filter(|e| e.info.kind == k).count();
        let full = inner.entries.len() >= self.limits.total
            || match kind {
                ConnKind::Congregation => count(kind) >= self.limits.congregation,
                ConnKind::Output => count(kind) >= self.limits.outputs,
                ConnKind::Paired => false,
            };
        if full {
            return Err(Refused::Full);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = watch::channel(None);
        inner.entries.insert(
            id,
            Entry {
                info: ConnectionInfo {
                    id,
                    device_id,
                    name,
                    role,
                    kind,
                    addr: ip.to_string(),
                    connected_ms,
                },
                ip,
                kick: tx,
            },
        );
        Ok(ConnGuard {
            id,
            kicked: rx,
            conns: self.clone(),
        })
    }

    /// Disconnects one connection. Anonymous clients are also barred from rejoining for `ban`
    /// (paired devices are handled by revoking their token instead).
    pub fn kick(&self, id: u64, reason: &str, ban: Duration) -> bool {
        let mut inner = self.lock();
        let Some((ip, anonymous)) = inner
            .entries
            .get(&id)
            .map(|e| (e.ip, e.info.kind != ConnKind::Paired))
        else {
            return false;
        };
        if anonymous && !ban.is_zero() {
            inner.bans.insert(ip, Instant::now() + ban);
        }
        if let Some(e) = inner.entries.get(&id) {
            let _ = e.kick.send(Some(reason.to_owned()));
        }
        true
    }

    /// Disconnects every connection of a device; returns how many.
    pub fn kick_device(&self, device_id: &str, reason: &str) -> usize {
        let inner = self.lock();
        let mut kicked = 0;
        for e in inner
            .entries
            .values()
            .filter(|e| e.info.device_id.as_deref() == Some(device_id))
        {
            let _ = e.kick.send(Some(reason.to_owned()));
            kicked += 1;
        }
        kicked
    }

    pub fn list(&self) -> Vec<ConnectionInfo> {
        let mut v: Vec<_> = self
            .lock()
            .entries
            .values()
            .map(|e| e.info.clone())
            .collect();
        v.sort_by_key(|c| c.id);
        v
    }

    pub fn is_connected(&self, device_id: &str) -> bool {
        self.lock()
            .entries
            .values()
            .any(|e| e.info.device_id.as_deref() == Some(device_id))
    }

    pub fn counts(&self) -> Counts {
        let inner = self.lock();
        let mut c = Counts {
            total: inner.entries.len(),
            ..Counts::default()
        };
        for e in inner.entries.values() {
            match e.info.kind {
                ConnKind::Paired => c.paired += 1,
                ConnKind::Congregation => c.congregation += 1,
                ConnKind::Output => c.outputs += 1,
            }
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conns() -> Arc<Connections> {
        Connections::new(Limits {
            congregation: 2,
            outputs: 1,
            total: 4,
        })
    }
    fn ip(n: u8) -> IpAddr {
        IpAddr::from([192, 168, 0, n])
    }
    fn join(c: &Arc<Connections>, n: u8, kind: ConnKind) -> Result<ConnGuard, Refused> {
        c.register(ip(n), None, "x".into(), Role::Congregation, kind, 0)
    }

    #[test]
    fn congregation_limit_is_enforced_and_released_on_drop() {
        let c = conns();
        let a = join(&c, 1, ConnKind::Congregation).unwrap();
        let _b = join(&c, 2, ConnKind::Congregation).unwrap();
        assert_eq!(
            join(&c, 3, ConnKind::Congregation).err(),
            Some(Refused::Full)
        );
        // Other kinds have their own budget.
        let _o = join(&c, 4, ConnKind::Output).unwrap();
        drop(a);
        let _again = join(&c, 3, ConnKind::Congregation).unwrap();
        assert_eq!(c.counts().total, 3);
    }

    #[test]
    fn total_cap_applies_to_paired_connections_too() {
        let c = conns();
        let _g: Vec<_> = (0..4)
            .map(|n| join(&c, n, ConnKind::Paired).unwrap())
            .collect();
        assert_eq!(join(&c, 9, ConnKind::Paired).err(), Some(Refused::Full));
    }

    #[test]
    fn kick_signals_the_connection_and_bans_anonymous_addresses() {
        let c = conns();
        let g = join(&c, 1, ConnKind::Congregation).unwrap();
        assert!(c.kick(g.id, "kicked", Duration::from_secs(60)));
        assert_eq!(g.kicked.borrow().as_deref(), Some("kicked"));
        assert_eq!(
            join(&c, 1, ConnKind::Congregation).err(),
            Some(Refused::Banned)
        );
        assert!(join(&c, 2, ConnKind::Congregation).is_ok());
        assert!(!c.kick(999, "x", Duration::ZERO));
    }

    #[test]
    fn devices_are_kicked_by_id() {
        let c = conns();
        let g = c
            .register(
                ip(1),
                Some("d-1".into()),
                "Tab".into(),
                Role::Musician,
                ConnKind::Paired,
                0,
            )
            .unwrap();
        assert!(c.is_connected("d-1"));
        assert_eq!(c.kick_device("d-1", "revoked"), 1);
        assert_eq!(g.kicked.borrow().as_deref(), Some("revoked"));
        drop(g);
        assert!(!c.is_connected("d-1"));
    }
}
