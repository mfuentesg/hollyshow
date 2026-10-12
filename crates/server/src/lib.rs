//! HollyShow LAN server (PLAN §3.4, §5.1): axum HTTP + WebSocket on the local network.
//!
//! ```text
//! phone ──HTTP──► /                  embedded remote web app (apps/remote/dist, rust-embed)
//!       ──HTTP──► /api/pair          PIN / QR token ─► session token (stored hashed)
//!       ──WS────► /ws                hello ─► welcome ─► snapshot, then state/event pushes
//! OBS   ──HTTP──► /output/full, /output/lower-third   transparent browser-source pages
//! ```
//!
//! Contracts kept from `docs/ARCHITECTURE.md`:
//! * Depends on `hollyshow-app` (plus the `core` types and the `storage-api` settings trait for
//!   the paired-device list); never on the engine or a database directly.
//! * Every command goes through `AppService::dispatch` (role enforcement lives there) and every
//!   byte of state through `AppService::snapshot_msg` / `RoleFeed`, which are role-projected.
//! * A socket creates its `RoleFeed` before sending the snapshot and sends a fresh snapshot on
//!   `resync`.
//! * Each connection runs in its own tokio task: a panic there is isolated from the engine, and
//!   its capacity slot is released by an RAII guard.
//!
//! # Roles on the wire
//!
//! | Role | How it joins | May do |
//! |---|---|---|
//! | Operator | PIN / QR created for `operator` | everything (`AppService` enforces) |
//! | Pastor | PIN / QR for `pastor` | search + preview Bible, request `showBible` |
//! | Musician, Stage | PIN / QR for that role | read-only, with chords / stage feed |
//! | Congregation | no pairing (unless `congregation_open = false`) | read-only live slide; connection limit |
//!
//! The role of a connection is **always the one stored with the pairing**, never the one the
//! client asks for in `hello`.
//!
//! # HTTP API (JSON, camelCase)
//!
//! * `GET /health` public.
//! * `POST /api/pair` `{pin | token, deviceName}` -> `{token, deviceId, role}`; 401/429 on failure.
//! * `POST /api/pairing` `{role}` (Operator bearer token) -> [`PairingOffer`] with PIN, QR token, URLs and QR SVG.
//! * `GET /api/devices` (Operator): paired devices, live connections and counts.
//! * `DELETE /api/devices/{id}` (Operator): forget a device and disconnect it.
//! * `POST /api/connections/{id}/kick` `{revoke?: bool}` (Operator): "expulsar".
//! * `GET /api/diagnostics`: all local IPs, per-address port reachability, mDNS state and hints.
//!
//! # Transport and session security
//!
//! By default the LAN transport is plain HTTP/WS (HTTPS is opt-in): session tokens and the show travel in cleartext, so pair
//! devices only on a Wi-Fi you trust (the pairing screen says so). Tokens are hashed at rest,
//! expire after 30 idle days / 180 days overall, and are rotated on reconnect once a week (the
//! new token arrives in `welcome.token`). Raw sockets are budgeted (global and per address), HTTP
//! headers must arrive within `header_read_timeout`, and every route family has a per-address
//! rate limit. A paired device keeps one command-dedupe window across reconnects (`dev-<id>`),
//! released `session_grace` after its last socket closes.
//!
//! # Optional HTTPS
//!
//! With [`ServerConfig::tls`] set the same port speaks HTTPS (rustls) and redirects plain HTTP to
//! it. The certificate is self-signed, covers `hollyshow.local` and the machine's LAN addresses,
//! lives in the configured directory and is renewed when an address it lacks appears. Phones show
//! a one-time warning; the operator can compare [`Server::tls_fingerprint`] with the certificate
//! the browser shows. HTTPS gives phones a secure context (wake lock, install as app).
//!
//! TODO(post-v1): a local CA (PLAN §5.1) and WebRTC/QUIC transports are out of scope.

mod assets;
mod config;
mod connections;
mod devices;
mod http;
mod mdns;
mod net;
mod pairing;
mod ratelimit;
mod serve;
mod shared;
mod tls;
mod util;
mod ws;

pub use config::{
    clamp_pairing_ttl, ServerConfig, TlsConfig, DEFAULT_PAIRING_TTL, DEFAULT_PORT,
    PAIRING_TTL_MINUTES,
};
pub use connections::{ConnKind, ConnectionInfo, Counts};
pub use devices::PairedDevice;
pub use http::{Diagnostics, PairingOffer};
pub use net::{local_interfaces, Interface, Peer};
pub use pairing::PairError;

use axum::Router;
use hollyshow_app::AppService;
use hollyshow_core::Role;
use shared::Shared;
use std::io;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Everything the server shares with the host application.
#[derive(Clone)]
pub struct App {
    pub service: Arc<AppService>,
    pub config: ServerConfig,
}

/// The LAN server. Cheap to clone; the host keeps one to create pairing codes and manage devices
/// while [`Server::start`] runs the listener.
#[derive(Clone)]
pub struct Server {
    shared: Arc<Shared>,
}

impl Server {
    pub fn new(app: App) -> Self {
        Self {
            shared: Shared::new(app.service, app.config),
        }
    }

    /// The HTTP + WebSocket routes. Serve it with
    /// a `ConnectInfo<net::Peer>` extension on every request (`Extension(ConnectInfo(Peer(addr)))`);
    /// [`Server::start`] does that.
    pub fn router(&self) -> Router {
        http::routes(self.shared.clone())
    }

    /// Binds, advertises over mDNS and serves until [`RunningServer::shutdown`].
    pub async fn start(&self) -> io::Result<RunningServer> {
        let cfg = &self.shared.config;
        let ip = cfg.bind.ip();
        if !cfg.allow_public_peers && !ip.is_unspecified() && !util::is_private_ip(ip) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing to bind to non-private address {ip}"),
            ));
        }
        if let Some(t) = &cfg.tls {
            let id = refresh_tls(&t.dir).await?;
            log::info!("HTTPS enabled, certificate SHA-256 {}", id.fingerprint);
            self.shared.set_tls_identity(Some(id));
        } else {
            self.shared.set_tls_identity(None);
        }
        let listener = TcpListener::bind(cfg.bind).await?;
        let addr = listener.local_addr()?;
        self.shared
            .port
            .store(addr.port(), std::sync::atomic::Ordering::Relaxed);
        self.shared.shutdown.send_replace(false);

        let router = self.router();
        let join = tokio::spawn(serve::serve(listener, router, self.shared.clone()));
        let supervisor = tokio::spawn(supervise_network(self.shared.clone(), addr.port()));
        log::info!("LAN server listening on {addr}");
        Ok(RunningServer {
            addr,
            shared: self.shared.clone(),
            join: Some(join),
            supervisor: Some(supervisor),
        })
    }

    // ---- host-side management (the operator's "Remote" screen in the desktop app) ----------

    /// New PIN + QR for `role`. Show them to the person pairing; they expire and are single-use
    /// (QR) or few-use (PIN).
    pub fn pairing_offer(&self, role: Role) -> Result<PairingOffer, PairError> {
        http::offer(&self.shared, role)
    }

    /// Changes the lifetime of PINs / QR codes created from now on (clamped to 1-10 minutes).
    pub fn set_pairing_ttl_minutes(&self, minutes: u64) {
        self.shared.pairing.set_ttl(clamp_pairing_ttl(minutes));
    }

    /// Whether the offer with this QR token can still be used (not spent, not expired).
    pub fn offer_is_live(&self, token: &str) -> bool {
        self.shared.pairing.is_live(token)
    }

    /// SHA-256 fingerprint (`AB:CD:...`) of the HTTPS certificate in use; `None` over plain HTTP
    /// (or before [`Server::start`]).
    pub fn tls_fingerprint(&self) -> Option<String> {
        self.shared.tls_identity().map(|t| t.fingerprint.clone())
    }

    /// Invalidates every open PIN / QR.
    pub fn cancel_pairing(&self) {
        self.shared.pairing.clear_offers();
    }

    /// Paired devices with whether each is connected right now.
    pub fn paired_devices(&self) -> Vec<(PairedDevice, bool)> {
        http::devices_snapshot(&self.shared).0
    }

    pub fn connections(&self) -> Vec<ConnectionInfo> {
        self.shared.conns.list()
    }

    pub fn counts(&self) -> Counts {
        self.shared.conns.counts()
    }

    /// Disconnects a connection; `revoke_token` also forgets the paired device behind it.
    pub fn kick(&self, connection_id: u64, revoke_token: bool) -> bool {
        http::kick(&self.shared, connection_id, revoke_token)
    }

    /// Forgets a paired device and disconnects it.
    pub fn revoke_device(&self, device_id: &str) -> bool {
        http::revoke(&self.shared, device_id)
    }

    pub async fn diagnostics(&self) -> Diagnostics {
        http::diagnose(&self.shared, None).await
    }

    /// Whether the machine has a LAN address right now (`false`: show "no network", not a QR).
    pub fn has_network(&self) -> bool {
        http::has_network()
    }

    /// Base URLs a phone can use (`http://192.168.1.20:7420`, `http://hollyshow.local:7420`;
    /// `https://` while TLS is on).
    pub fn urls(&self) -> Vec<String> {
        http::base_urls(&self.shared)
    }
}

/// A listening server. Dropping it without [`RunningServer::shutdown`] leaves the task running
/// until the runtime ends.
pub struct RunningServer {
    addr: SocketAddr,
    shared: Arc<Shared>,
    join: Option<JoinHandle<()>>,
    supervisor: Option<JoinHandle<()>>,
}

impl RunningServer {
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Closes every socket (clients see `reject{shutdown}` and reconnect elsewhere) and stops.
    pub async fn shutdown(mut self) {
        self.shared.shutdown.send_replace(true);
        self.shared
            .mdns_active
            .store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(sup) = self.supervisor.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), sup).await;
        }
        if let Some(join) = self.join.take() {
            if tokio::time::timeout(Duration::from_secs(5), join)
                .await
                .is_err()
            {
                log::warn!("LAN server did not stop in time");
            }
        }
    }
}

/// Keeps the machine's LAN addresses and the mDNS announcement in step. Phones keep resolving
/// `hollyshow.local` after DHCP renewals, Wi-Fi drops and a PC that booted before its Wi-Fi came
/// up: the address set is re-read every `network_poll` and the service is re-announced (or
/// withdrawn when the machine is offline) whenever it changes.
async fn supervise_network(shared: Arc<Shared>, port: u16) {
    use std::sync::atomic::Ordering::Relaxed;
    let cfg = &shared.config;
    let mut stop = shared.shutdown.subscribe();
    let mut mdns: Option<mdns::Mdns> = None;
    let mut last: Option<Vec<IpAddr>> = None;
    loop {
        if let Some(t) = &cfg.tls {
            // A new Wi-Fi or DHCP lease adds an address the certificate does not cover.
            match refresh_tls(&t.dir).await {
                Ok(id) => {
                    let changed = shared
                        .tls_identity()
                        .is_none_or(|cur| cur.fingerprint != id.fingerprint);
                    if changed {
                        log::info!("HTTPS certificate renewed, SHA-256 {}", id.fingerprint);
                        shared.set_tls_identity(Some(id));
                    }
                }
                Err(e) => log::warn!("HTTPS certificate refresh failed: {e}"),
            }
        }
        if cfg.mdns {
            let addrs = tokio::task::spawn_blocking(net::lan_addrs)
                .await
                .unwrap_or_default();
            let ips = net::mdns_ips(&addrs);
            if last.as_ref() != Some(&ips) {
                if ips.is_empty() {
                    log::info!("no LAN address: mDNS withdrawn until the network returns");
                } else {
                    log::info!("LAN addresses changed: advertising {ips:?}");
                }
                match mdns.as_mut() {
                    Some(m) => {
                        if let Err(e) = m.set_addresses(&ips) {
                            log::warn!("mDNS update failed: {e}");
                        }
                    }
                    None if !ips.is_empty() => {
                        match mdns::Mdns::advertise(&cfg.server_name, port, &ips) {
                            Ok(m) => mdns = Some(m),
                            Err(e) => log::warn!(
                                "mDNS advertisement failed (the server still works by IP): {e}"
                            ),
                        }
                    }
                    None => {}
                }
                shared.mdns_active.store(
                    mdns.as_ref().is_some_and(mdns::Mdns::is_advertising),
                    Relaxed,
                );
                last = Some(ips);
            }
        }
        tokio::select! {
            () = tokio::time::sleep(cfg.network_poll) => {}
            _ = stop.wait_for(|down| *down) => break,
        }
    }
    shared.mdns_active.store(false, Relaxed);
}

/// Loads (or creates / renews) the certificate in `dir` for the machine's current addresses.
async fn refresh_tls(dir: &std::path::Path) -> io::Result<Arc<tls::TlsIdentity>> {
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let ips = net::lan_ipv4();
        tls::load_or_create(&dir, &tls::wanted_sans(&ips)).map(Arc::new)
    })
    .await
    .map_err(io::Error::other)?
}

/// Convenience: the routes for `app` with no host handle (tests, embedding). Needs
/// `ConnectInfo<Peer>` (axum's `MockConnectInfo` in tests).
pub fn router(app: App) -> Router {
    Server::new(app).router()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::{Request, StatusCode};
    use hollyshow_core::PROTOCOL_VERSION;
    use hollyshow_engine::Engine;
    use hollyshow_songs::DefaultSplitter;
    use hollyshow_storage_api::memory::MemoryStore;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    pub(crate) fn test_app() -> App {
        App {
            service: Arc::new(AppService::new(
                MemoryStore::repos(),
                Arc::new(Engine::default()),
                Arc::new(DefaultSplitter),
            )),
            config: ServerConfig::default(),
        }
    }

    async fn get_path(path: &str) -> (StatusCode, Vec<u8>) {
        let router = router(test_app()).layer(MockConnectInfo(net::Peer(SocketAddr::from((
            [192, 168, 1, 9],
            5000,
        )))));
        let res = router
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        (
            status,
            res.into_body().collect().await.unwrap().to_bytes().to_vec(),
        )
    }

    #[tokio::test]
    async fn health_reports_protocol_version() {
        let (status, body) = get_path("/health").await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["protocol"], PROTOCOL_VERSION);
    }

    #[tokio::test]
    async fn serves_the_embedded_remote_app_with_spa_fallback() {
        let (status, body) = get_path("/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(String::from_utf8_lossy(&body).contains("<html") || body.starts_with(b"<!doctype"));
        let (status, _) = get_path("/some/client/route").await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = get_path("/assets/missing.js").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn obs_pages_are_transparent_and_distinct() {
        for (path, mode) in [
            ("/output/full", "full"),
            ("/output/lower-third", "lower-third"),
        ] {
            let (status, body) = get_path(path).await;
            assert_eq!(status, StatusCode::OK);
            let html = String::from_utf8(body).unwrap();
            assert!(html.contains("background: transparent"));
            assert!(html.contains(&format!("<body class=\"{mode}\">")));
            assert!(!html.contains("__MODE__"));
        }
    }

    #[tokio::test]
    async fn operator_endpoints_require_a_token() {
        let router = router(test_app()).layer(MockConnectInfo(net::Peer(SocketAddr::from((
            [192, 168, 1, 9],
            5000,
        )))));
        for (method, path) in [
            ("GET", "/api/devices"),
            ("DELETE", "/api/devices/x"),
            ("POST", "/api/connections/1/kick"),
        ] {
            let res = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {path}");
        }
    }

    #[tokio::test]
    async fn diagnostics_lists_interfaces_without_secrets() {
        let (status, body) = get_path("/api/diagnostics").await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(v["interfaces"].is_array());
        assert_eq!(v["clientAddr"], "192.168.1.9");
        assert!(v["hints"].is_array());
    }
}
