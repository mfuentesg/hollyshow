//! LAN server. Owner: builder "server". The architect fixed:
//! * [`ServerConfig`] and [`App`]: the only things the server needs from the host.
//! * Auth/role enforcement uses `hollyshow_core::Role::can_dispatch` / `can_query`.
//! * The remote web app (apps/remote `dist`) is embedded with rust-embed and served at `/`.
//! * `/health` is public; everything else requires a paired token (except the pairing endpoints).

use axum::{routing::get, Json, Router};
use hollyshow_core::{Engine, PROTOCOL_VERSION};
use hollyshow_storage::Repos;
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
    pub engine: Arc<Engine>,
    pub repos: Repos,
    pub config: ServerConfig,
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    protocol: u32,
    name: String,
}

pub fn router(app: App) -> Router {
    Router::new().route(
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use hollyshow_storage::memory::MemoryStore;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_reports_protocol_version() {
        let app = App {
            engine: Arc::new(Engine::default()),
            repos: MemoryStore::repos(),
            config: ServerConfig::default(),
        };
        let res = router(app)
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["protocol"], PROTOCOL_VERSION);
    }
}
