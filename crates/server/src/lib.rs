//! LAN server. Owner: builder "server". The architect fixed:
//! * [`ServerConfig`] and [`App`]: the only things the server needs from the host. It depends on
//!   `hollyshow-app` only, never on storage or the engine directly.
//! * Every command goes through `AppService::dispatch` (role enforcement lives there) and every
//!   byte of state through `AppService::snapshot_msg` / `RoleFeed`, which are role-projected.
//! * A WebSocket session creates its `RoleFeed` before sending the snapshot, forwards each
//!   batch, and on a `Resync` message from the feed expects the client to ask for a snapshot.
//! * The remote web app (apps/remote `dist`) is embedded with rust-embed (see `build.rs`) and
//!   served at `/`.
//! * `/health` is public; everything else requires a paired token (except the pairing endpoints).
//! * Each connection runs in its own tokio task: a panic there is isolated from the engine.

mod assets;

use axum::{routing::get, Json, Router};
use hollyshow_app::AppService;
use hollyshow_core::PROTOCOL_VERSION;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub server_name: String,
    /// Maximum simultaneous read-only congregation sockets.
    pub max_congregation: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 7878)),
            server_name: "HollyShow".into(),
            max_congregation: 200,
        }
    }
}

/// Everything the server shares with the host application.
#[derive(Clone)]
pub struct App {
    pub service: Arc<AppService>,
    pub config: ServerConfig,
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    protocol: u32,
    name: String,
}

pub fn router(app: App) -> Router {
    Router::new()
        .route(
            "/health",
            get(move || {
                let h = Health {
                    ok: true,
                    protocol: PROTOCOL_VERSION,
                    name: app.config.server_name.clone(),
                };
                async move { Json(h) }
            }),
        )
        .fallback(assets::serve)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use hollyshow_engine::Engine;
    use hollyshow_songs::DefaultSplitter;
    use hollyshow_storage_api::memory::MemoryStore;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_app() -> App {
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
        let res = router(test_app())
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
}
