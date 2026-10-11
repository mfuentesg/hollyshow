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
//! TODO(post-v1): HTTPS with a local CA (PLAN §5.1) and WebRTC/QUIC transports are out of scope.

mod assets;
mod config;
mod connections;
mod devices;
mod http;
mod mdns;
mod net;
mod pairing;
mod ratelimit;
mod shared;
mod util;
mod ws;

pub use config::{ServerConfig, DEFAULT_PORT};
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
    /// `into_make_service_with_connect_info::<net::Peer>()`; [`Server::start`] does that.
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
        let listener = TcpListener::bind(cfg.bind).await?;
        let addr = listener.local_addr()?;
        self.shared
            .port
            .store(addr.port(), std::sync::atomic::Ordering::Relaxed);
        self.shared.shutdown.send_replace(false);

        let mdns = if cfg.mdns {
            let ips = net::lan_ipv4();
            if ips.is_empty() {
                log::info!("no LAN address found: skipping mDNS");
                None
            } else {
                match mdns::Mdns::advertise(&cfg.server_name, addr.port(), &ips) {
                    Ok(m) => {
                        self.shared
                            .mdns_active
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        Some(m)
                    }
                    Err(e) => {
                        log::warn!("mDNS advertisement failed (the server still works by IP): {e}");
                        None
                    }
                }
            }
        } else {
            None
        };

        let listener = net::PrivateListener::new(listener, cfg.allow_public_peers);
        let make = self
            .router()
            .into_make_service_with_connect_info::<net::Peer>();
        let mut stop = self.shared.shutdown.subscribe();
        let join = tokio::spawn(async move {
            let served = axum::serve(listener, make).with_graceful_shutdown(async move {
                let _ = stop.wait_for(|down| *down).await;
            });
            if let Err(e) = served.await {
                log::error!("LAN server stopped: {e}");
            }
        });
        log::info!("LAN server listening on {addr}");
        Ok(RunningServer {
            addr,
            shared: self.shared.clone(),
            join: Some(join),
            _mdns: mdns,
        })
    }

    // ---- host-side management (the operator's "Remote" screen in the desktop app) ----------

    /// New PIN + QR for `role`. Show them to the person pairing; they expire and are single-use
    /// (QR) or few-use (PIN).
    pub fn pairing_offer(&self, role: Role) -> Result<PairingOffer, PairError> {
        http::offer(&self.shared, role)
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

    /// Base URLs a phone can use (`http://192.168.1.20:7420`, `http://hollyshow.local:7420`).
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
    _mdns: Option<mdns::Mdns>,
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
