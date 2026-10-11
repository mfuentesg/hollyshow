//! LAN server lifecycle and the pairing data shown in the operator window.
//!
//! The server shares the one `Arc<AppService>` with the Tauri commands, so a phone and the
//! operator window drive exactly the same command bus.

use crate::state::now_ms;
use hollyshow_app::AppService;
use hollyshow_core::Role;
use hollyshow_server::{App, PairingOffer, RunningServer, Server, ServerConfig, DEFAULT_PORT};
use hollyshow_storage_api::SettingsRepo;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

pub const ENABLED_KEY: &str = "remote.enabled";
pub const PORT_KEY: &str = "remote.port";
pub const ROLE_KEY: &str = "remote.pairingRole";

/// An offer is replaced when it has less than this left, so the PIN on screen is always usable.
const MIN_REMAINING_MS: i64 = 20_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PairingInfo {
    pub enabled: bool,
    /// LAN URL phones open (no secrets in it).
    pub url: String,
    /// Same URL carrying the one-time token, for the QR code (pairs without typing the PIN).
    pub qr_url: String,
    pub pin: String,
    pub pin_expires_ms: Option<i64>,
    /// Every address phones could try, best first (default-route adapter before virtual ones).
    pub urls: Vec<String>,
    /// `false` when the machine has no LAN address: the UI shows "sin red" instead of a QR.
    pub network_available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub paired_at_ms: i64,
    pub last_seen_ms: i64,
    pub connected: bool,
}

struct CachedOffer {
    offer: PairingOffer,
    role: Role,
    expires_ms: i64,
    /// Address the offer's URLs and QR were built for; the offer is replaced when it changes
    /// (Wi-Fi switch, DHCP renewal) so the QR never points at a stale IP.
    base_url: String,
}

/// Roles a PIN may grant. Congregation joins without pairing, so it is not offered. Full control
/// (`operator`) must be chosen explicitly; a missing or unknown value gets the least privilege.
pub fn parse_pairing_role(s: Option<&str>) -> Role {
    match s {
        Some("operator") => Role::Operator,
        Some("pastor") => Role::Pastor,
        Some("musician") => Role::Musician,
        _ => Role::Stage,
    }
}

/// Whether a cached offer can still be shown (right role, enough time left).
pub fn offer_is_usable(role_wanted: Role, cached_role: Role, expires_ms: i64, now: i64) -> bool {
    role_wanted == cached_role && expires_ms - now > MIN_REMAINING_MS
}

/// Worker threads of the LAN server's own runtime.
const SERVER_WORKERS: usize = 2;

pub struct Remote {
    /// The LAN server runs on its own runtime, never on Tauri's: IPC commands do blocking SQLite
    /// work on Tauri's workers, and a busy library (backup, Bible install) must not delay
    /// accepts, heartbeats or acks for phones.
    rt: Option<tokio::runtime::Runtime>,
    service: Arc<AppService>,
    settings: Arc<dyn SettingsRepo>,
    server: Mutex<Server>,
    running: Mutex<Option<RunningServer>>,
    offer: Mutex<Option<CachedOffer>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn config_for(port: u16) -> ServerConfig {
    ServerConfig {
        bind: SocketAddr::from(([0, 0, 0, 0], port)),
        ..ServerConfig::default()
    }
}

impl Remote {
    pub fn new(service: Arc<AppService>, settings: Arc<dyn SettingsRepo>) -> Self {
        let port = settings
            .get(PORT_KEY)
            .ok()
            .flatten()
            .and_then(|p| p.trim().parse::<u16>().ok())
            .filter(|p| *p >= 1024)
            .unwrap_or(DEFAULT_PORT);
        let server = Server::new(App {
            service: service.clone(),
            config: config_for(port),
        });
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(SERVER_WORKERS)
            .thread_name("lan-server")
            .enable_all()
            .build()
            .expect("the LAN server runtime starts");
        Self {
            rt: Some(rt),
            service,
            settings,
            server: Mutex::new(server),
            running: Mutex::new(None),
            offer: Mutex::new(None),
        }
    }

    fn rt(&self) -> &tokio::runtime::Handle {
        self.rt.as_ref().expect("runtime lives until drop").handle()
    }

    fn server(&self) -> Server {
        lock(&self.server).clone()
    }

    /// Whether the user wants the remote on. Off until they opt in: a fresh install must not
    /// listen on the shared network.
    pub fn wanted(&self) -> bool {
        self.settings
            .get(ENABLED_KEY)
            .ok()
            .flatten()
            .is_some_and(|v| v == "1")
    }

    pub fn is_running(&self) -> bool {
        lock(&self.running).is_some()
    }

    /// Starts listening (no-op when already running). When the preferred port is taken the
    /// server takes a free one so the app still starts; the pairing URL always shows the real
    /// port.
    pub async fn start(&self) -> Result<SocketAddr, String> {
        if let Some(r) = lock(&self.running).as_ref() {
            return Ok(r.local_addr());
        }
        // `start` spawns the accept loop on the ambient runtime: enter the server's own.
        let server = self.server();
        let first = self
            .rt()
            .spawn(async move { server.start().await })
            .await
            .map_err(|e| e.to_string())?;
        let running = match first {
            Ok(r) => r,
            Err(e) => {
                log::warn!("LAN server could not use its port ({e}); trying a free one");
                let fallback = Server::new(App {
                    service: self.service.clone(),
                    config: config_for(0),
                });
                let again = fallback.clone();
                let r = self
                    .rt()
                    .spawn(async move { again.start().await })
                    .await
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?;
                *lock(&self.server) = fallback;
                r
            }
        };
        let addr = running.local_addr();
        *lock(&self.running) = Some(running);
        *lock(&self.offer) = None;
        Ok(addr)
    }

    /// Stops listening and drops every socket and pending offer.
    pub async fn stop(&self) {
        let running = lock(&self.running).take();
        self.server().cancel_pairing();
        *lock(&self.offer) = None;
        if let Some(r) = running {
            let _ = self.rt().spawn(r.shutdown()).await;
        }
    }

    fn pairing_role(&self) -> Role {
        parse_pairing_role(self.settings.get(ROLE_KEY).ok().flatten().as_deref())
    }

    /// The offer to show: the current one while it is still good, otherwise (or when `fresh`) a
    /// new PIN and QR for the configured role.
    pub fn pairing(&self, fresh: bool) -> Result<PairingInfo, String> {
        if !self.is_running() {
            return Ok(PairingInfo {
                enabled: false,
                url: String::new(),
                qr_url: String::new(),
                pin: String::new(),
                pin_expires_ms: None,
                urls: Vec::new(),
                network_available: false,
            });
        }
        let role = self.pairing_role();
        let now = now_ms();
        let server = self.server();
        let urls = server.urls();
        let base = urls.first().cloned().unwrap_or_default();
        let mut cache = lock(&self.offer);
        let reuse = !fresh
            && cache.as_ref().is_some_and(|c| {
                offer_is_usable(role, c.role, c.expires_ms, now) && c.base_url == base
            });
        if !reuse {
            if fresh {
                server.cancel_pairing();
            }
            let offer = server
                .pairing_offer(role)
                .map_err(|e| format!("no se pudo crear el PIN: {e:?}"))?;
            let expires_ms = now + i64::try_from(offer.expires_in_secs).unwrap_or(600) * 1000;
            *cache = Some(CachedOffer {
                offer,
                role,
                expires_ms,
                base_url: base.clone(),
            });
        }
        let c = cache.as_ref().expect("offer was just created");
        Ok(PairingInfo {
            enabled: true,
            url: base,
            urls,
            network_available: server.has_network(),
            qr_url: c.offer.urls.first().cloned().unwrap_or_default(),
            pin: c.offer.pin.clone(),
            pin_expires_ms: Some(c.expires_ms),
        })
    }

    pub fn devices(&self) -> Vec<RemoteDevice> {
        let server = self.server();
        let live = server.connections();
        server
            .paired_devices()
            .into_iter()
            .map(|(d, connected)| RemoteDevice {
                connected: connected
                    || live
                        .iter()
                        .any(|c| c.device_id.as_deref() == Some(d.id.as_str())),
                id: d.id,
                name: d.name,
                role: d.role,
                paired_at_ms: d.created_ms,
                last_seen_ms: d.last_seen_ms,
            })
            .collect()
    }

    /// Forgets a paired device and disconnects it.
    pub fn kick(&self, id: &str) -> bool {
        self.server().revoke_device(id)
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        // Dropping a runtime inside an async context panics; this never blocks.
        if let Some(rt) = self.rt.take() {
            rt.shutdown_background();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hollyshow_engine::Engine;
    use hollyshow_songs::DefaultSplitter;
    use hollyshow_storage_api::memory::MemoryStore;

    #[test]
    fn pairing_role_setting_never_grants_congregation_or_unknown_roles() {
        assert_eq!(parse_pairing_role(None), Role::Stage);
        assert_eq!(parse_pairing_role(Some("operator")), Role::Operator);
        assert_eq!(parse_pairing_role(Some("pastor")), Role::Pastor);
        assert_eq!(parse_pairing_role(Some("musician")), Role::Musician);
        assert_eq!(parse_pairing_role(Some("stage")), Role::Stage);
        assert_eq!(parse_pairing_role(Some("root")), Role::Stage);
        assert_eq!(parse_pairing_role(Some("congregation")), Role::Stage);
    }

    #[test]
    fn offers_are_replaced_when_stale_or_for_another_role() {
        assert!(offer_is_usable(Role::Operator, Role::Operator, 100_000, 0));
        assert!(!offer_is_usable(Role::Operator, Role::Operator, 10_000, 0));
        assert!(!offer_is_usable(Role::Pastor, Role::Operator, 100_000, 0));
    }

    fn remote() -> Remote {
        let repos = MemoryStore::repos();
        let svc = Arc::new(AppService::new(
            repos.clone(),
            Arc::new(Engine::default()),
            Arc::new(DefaultSplitter),
        ));
        // Port 0 and no mDNS interference in tests: the setting below picks a free port.
        repos.settings.set(PORT_KEY, "65000").unwrap();
        Remote::new(svc, repos.settings)
    }

    #[test]
    fn the_remote_is_off_until_the_user_opts_in() {
        let r = remote();
        assert!(!r.wanted());
        r.settings.set(ENABLED_KEY, "1").unwrap();
        assert!(r.wanted());
        r.settings.set(ENABLED_KEY, "0").unwrap();
        assert!(!r.wanted());
    }

    #[tokio::test]
    async fn start_pair_stop() {
        let r = remote();
        assert!(!r.pairing(false).unwrap().enabled);
        let addr = r.start().await.expect("server starts");
        assert!(r.is_running());
        let p = r.pairing(false).unwrap();
        assert!(p.enabled);
        assert_eq!(p.pin.len(), 6);
        assert!(p.qr_url.contains("?pair="), "{}", p.qr_url);
        assert!(!p.url.contains("pair="));
        assert!(p.url.ends_with(&format!(":{}", addr.port())));
        // Same offer until regenerated.
        assert_eq!(r.pairing(false).unwrap().pin, p.pin);
        assert_ne!(r.pairing(true).unwrap().pin, p.pin);
        assert!(r.devices().is_empty());
        r.stop().await;
        assert!(!r.is_running());
        assert!(!r.pairing(false).unwrap().enabled);
    }
}
