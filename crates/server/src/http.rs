//! HTTP routes: health, pairing, the operator's device API, diagnostics, OBS output pages and
//! the WebSocket upgrade.

use crate::connections::{ConnectionInfo, Counts};
use crate::devices::PairedDevice;
use crate::net::{self, Peer};
use crate::pairing::{Credential, PairError};
use crate::shared::Shared;
use crate::util::{is_private_ip, sanitize_name};
use crate::{assets, mdns, ws};
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use hollyshow_core::{Role, PROTOCOL_MIN, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// How long an anonymous client stays barred after the operator kicks it.
const KICK_BAN: Duration = Duration::from_secs(5 * 60);

pub fn routes(shared: Arc<Shared>) -> Router {
    let api = Router::new()
        .route("/pair", post(pair))
        .route("/pairing", post(create_offer))
        .route("/devices", get(devices))
        .route("/devices/{id}", delete(revoke_device))
        .route("/connections/{id}/kick", post(kick_connection))
        .route("/diagnostics", get(diagnostics))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(
            shared.clone(),
            api_rate_limit,
        ));

    // Page loads, OBS pages and WebSocket upgrades share one per-address bucket, larger than the
    // API's because a page load fetches several assets.
    let web = Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/output/full", get(|| async { output_page("full") }))
        .route(
            "/output/lower-third",
            get(|| async { output_page("lower-third") }),
        )
        .fallback(assets::serve)
        .layer(middleware::from_fn_with_state(
            shared.clone(),
            web_rate_limit,
        ));

    Router::new()
        .route("/health", get(health))
        .nest("/api", api)
        .merge(web)
        .with_state(shared)
}

// ---- errors ---------------------------------------------------------------------------------

pub struct ApiError(StatusCode, &'static str, String);

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self(status, code, message.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(serde_json::json!({ "code": self.1, "message": self.2 })),
        )
            .into_response()
    }
}

impl From<PairError> for ApiError {
    fn from(e: PairError) -> Self {
        match e {
            PairError::Invalid => Self::new(
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "PIN o codigo QR incorrecto o caducado",
            ),
            PairError::Locked { retry_secs } => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                "locked",
                format!("demasiados intentos; reintenta en {retry_secs} s"),
            ),
            PairError::NoEntropy => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "sin fuente de aleatoriedad",
            ),
        }
    }
}

async fn api_rate_limit(
    State(s): State<Arc<Shared>>,
    ConnectInfo(Peer(addr)): ConnectInfo<Peer>,
    req: Request,
    next: Next,
) -> Response {
    rate_limited(&s.http_limiter, addr, req, next).await
}

async fn web_rate_limit(
    State(s): State<Arc<Shared>>,
    ConnectInfo(Peer(addr)): ConnectInfo<Peer>,
    req: Request,
    next: Next,
) -> Response {
    rate_limited(&s.web_limiter, addr, req, next).await
}

async fn rate_limited(
    limiter: &crate::ratelimit::IpLimiter,
    addr: SocketAddr,
    req: Request,
    next: Next,
) -> Response {
    if !limiter.check(addr.ip()) {
        return ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "demasiadas solicitudes",
        )
        .into_response();
    }
    next.run(req).await
}

/// Bearer-token check for the operator-only endpoints.
fn require_operator(s: &Shared, headers: &HeaderMap) -> Result<PairedDevice, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "falta la sesion")
        })?;
    let device = s.devices.peek(token.trim()).ok_or_else(|| {
        ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "sesion no valida")
    })?;
    if device.role != Role::Operator {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "solo el operador",
        ));
    }
    Ok(device)
}

// ---- health -----------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    ok: bool,
    protocol: u32,
    protocol_min: u32,
    name: String,
    version: &'static str,
}

async fn health(State(s): State<Arc<Shared>>) -> Json<Health> {
    Json(Health {
        ok: true,
        protocol: PROTOCOL_VERSION,
        protocol_min: PROTOCOL_MIN,
        name: s.config.server_name.clone(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

// ---- pairing ----------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairRequest {
    pin: Option<String>,
    /// One-time token from the QR code.
    token: Option<String>,
    #[serde(default)]
    device_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairResponse {
    token: String,
    device_id: String,
    role: Role,
}

/// Redeems a PIN or QR token for a session token (the remote app stores it and then sends it in
/// `hello`). Equivalent to pairing inside `hello`, for pages that pair before opening the socket.
async fn pair(
    State(s): State<Arc<Shared>>,
    ConnectInfo(Peer(addr)): ConnectInfo<Peer>,
    Json(req): Json<PairRequest>,
) -> Result<Json<PairResponse>, ApiError> {
    let (pin, token) = match (req.pin.clone(), req.token.clone()) {
        (Some(p), None) => (Some(p), None),
        (None, Some(t)) => (None, Some(t)),
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "bad_request",
                "envia pin o token",
            ))
        }
    };
    let ip = addr.ip();
    let name = sanitize_name(&req.device_name);
    let s2 = s.clone();
    let out = tokio::task::spawn_blocking(move || {
        let cred = match (&pin, &token) {
            (Some(p), _) => Credential::Pin(p),
            (_, Some(t)) => Credential::Token(t),
            _ => return Err(PairError::Invalid),
        };
        let role = s2.pairing.redeem(cred, ip)?;
        let (device, token) = s2.devices.add(&name, role).ok_or(PairError::NoEntropy)?;
        Ok::<_, PairError>(PairResponse {
            token,
            device_id: device.id,
            role,
        })
    })
    .await
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "error interno",
        )
    })??;
    Ok(Json(out))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfferRequest {
    role: Role,
}

async fn create_offer(
    State(s): State<Arc<Shared>>,
    headers: HeaderMap,
    Json(req): Json<OfferRequest>,
) -> Result<Json<PairingOffer>, ApiError> {
    require_operator(&s, &headers)?;
    offer(&s, req.role).map(Json).map_err(ApiError::from)
}

/// What the operator screen shows to pair a device: the PIN, the QR (SVG) and its URL.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PairingOffer {
    pub pin: String,
    pub token: String,
    pub role: Role,
    pub expires_in_secs: u64,
    /// Every URL a phone could open to pair (one per LAN address, plus `hollyshow.local`).
    pub urls: Vec<String>,
    /// SVG of the QR for the first URL; empty when the machine has no LAN address (a QR for
    /// `127.0.0.1` would be useless to a phone).
    pub qr_svg: String,
    /// Whether any LAN address exists. When `false`, `urls` only holds the loopback address and
    /// the UI should say "no network" instead of showing a QR.
    pub network_available: bool,
}

/// Base URLs (`http://ip:port`) at which this server is reachable on the LAN, best first (the
/// default-route interface, then other real adapters, then virtual ones; see
/// [`net::rank_lan`]). Without any LAN address it is the loopback URL only; check
/// [`has_network`] to tell.
pub fn base_urls(s: &Shared) -> Vec<String> {
    let port = s.port.load(Ordering::Relaxed);
    let ips = net::lan_ipv4();
    let mut urls: Vec<String> = ips.iter().map(|ip| format!("http://{ip}:{port}")).collect();
    if s.mdns_active.load(Ordering::Relaxed) && !ips.is_empty() {
        urls.push(format!("http://{}:{port}", mdns::HOSTNAME));
    }
    if urls.is_empty() {
        urls.push(format!("http://127.0.0.1:{port}"));
    }
    urls
}

/// Whether the machine currently has an address a phone could reach.
pub fn has_network() -> bool {
    !net::lan_ipv4().is_empty()
}

pub fn offer(s: &Shared, role: Role) -> Result<PairingOffer, PairError> {
    let o = s.pairing.create_offer(role)?;
    let urls: Vec<String> = base_urls(s)
        .into_iter()
        .map(|b| format!("{b}/?pair={}", o.token))
        .collect();
    let network_available = has_network();
    let qr_svg = if network_available {
        qr_svg(urls.first().map_or("", String::as_str))
    } else {
        String::new()
    };
    Ok(PairingOffer {
        pin: o.pin,
        token: o.token,
        role,
        expires_in_secs: o.expires_in.as_secs(),
        urls,
        qr_svg,
        network_available,
    })
}

fn qr_svg(data: &str) -> String {
    qrcode::QrCode::new(data.as_bytes())
        .map(|c| {
            c.render::<qrcode::render::svg::Color>()
                .min_dimensions(220, 220)
                .quiet_zone(true)
                .build()
        })
        .unwrap_or_default()
}

// ---- devices ----------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DevicesResponse {
    devices: Vec<DeviceEntry>,
    connections: Vec<ConnectionInfo>,
    counts: Counts,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceEntry {
    #[serde(flatten)]
    device: PairedDevice,
    connected: bool,
}

pub fn devices_snapshot(s: &Shared) -> (Vec<(PairedDevice, bool)>, Vec<ConnectionInfo>, Counts) {
    let devices = s
        .devices
        .list()
        .into_iter()
        .map(|d| {
            let on = s.conns.is_connected(&d.id);
            (d, on)
        })
        .collect();
    (devices, s.conns.list(), s.conns.counts())
}

async fn devices(
    State(s): State<Arc<Shared>>,
    headers: HeaderMap,
) -> Result<Json<DevicesResponse>, ApiError> {
    require_operator(&s, &headers)?;
    let (devices, connections, counts) = devices_snapshot(&s);
    Ok(Json(DevicesResponse {
        devices: devices
            .into_iter()
            .map(|(device, connected)| DeviceEntry { device, connected })
            .collect(),
        connections,
        counts,
    }))
}

async fn revoke_device(
    State(s): State<Arc<Shared>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_operator(&s, &headers)?;
    if revoke(&s, &id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "dispositivo desconocido",
        ))
    }
}

/// Forgets a device's token and disconnects it.
pub fn revoke(s: &Shared, id: &str) -> bool {
    let removed = s.devices.revoke(id);
    s.conns.kick_device(id, "revoked");
    removed
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct KickRequest {
    /// Also forget the device's token so it cannot simply reconnect. Default `true`.
    revoke: Option<bool>,
}

async fn kick_connection(
    State(s): State<Arc<Shared>>,
    headers: HeaderMap,
    Path(id): Path<u64>,
    body: axum::body::Bytes,
) -> Result<StatusCode, ApiError> {
    require_operator(&s, &headers)?;
    // The body is optional: an empty one (or none) means the default, revoke.
    let revoke_token = if body.is_empty() {
        true
    } else {
        serde_json::from_slice::<KickRequest>(&body)
            .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request", "cuerpo no valido"))?
            .revoke
            .unwrap_or(true)
    };
    if kick(&s, id, revoke_token) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "conexion desconocida",
        ))
    }
}

/// Disconnects one connection ("expulsar"); `revoke_token` also forgets a paired device.
pub fn kick(s: &Shared, conn_id: u64, revoke_token: bool) -> bool {
    let device = s
        .conns
        .list()
        .into_iter()
        .find(|c| c.id == conn_id)
        .and_then(|c| c.device_id);
    let found = s.conns.kick(conn_id, "kicked", KICK_BAN);
    if found && revoke_token {
        if let Some(d) = device {
            s.devices.revoke(&d);
        }
    }
    found
}

// ---- diagnostics ----------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InterfaceReport {
    #[serde(flatten)]
    iface: net::Interface,
    /// Whether this machine can open the server port through this address. `None` for addresses
    /// not probed (loopback, IPv6).
    reachable: Option<bool>,
    url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    name: String,
    protocol: u32,
    port: u16,
    hostname: &'static str,
    mdns_active: bool,
    /// Address the request came from, as the server sees it.
    client_addr: Option<String>,
    interfaces: Vec<InterfaceReport>,
    connections: Counts,
    /// Stable codes the UI translates: `no_lan_address`, `port_unreachable`, `mdns_inactive`.
    hints: Vec<&'static str>,
}

pub async fn diagnose(s: &Shared, client: Option<SocketAddr>) -> Diagnostics {
    let port = s.port.load(Ordering::Relaxed);
    let ifaces = net::local_interfaces();
    let probes = ifaces.iter().map(|i| async move {
        if i.ip.is_ipv4() && !i.is_loopback {
            Some(net::port_reachable(i.ip, port).await)
        } else {
            None
        }
    });
    let results = futures_util::future::join_all(probes).await;
    let interfaces: Vec<InterfaceReport> = ifaces
        .into_iter()
        .zip(results)
        .map(|(iface, reachable)| {
            let url = (iface.ip.is_ipv4() && iface.is_private && !iface.is_loopback)
                .then(|| format!("http://{}:{port}", iface.ip));
            InterfaceReport {
                iface,
                reachable,
                url,
            }
        })
        .collect();

    let mut hints = Vec::new();
    let lan: Vec<_> = interfaces
        .iter()
        .filter(|i| i.url.is_some() && !i.iface.is_link_local)
        .collect();
    if lan.is_empty() {
        hints.push("no_lan_address");
    }
    if lan.iter().any(|i| i.reachable == Some(false)) {
        hints.push("port_unreachable");
    }
    let mdns_active = s.mdns_active.load(Ordering::Relaxed);
    if s.config.mdns && !mdns_active {
        hints.push("mdns_inactive");
    }
    Diagnostics {
        name: s.config.server_name.clone(),
        protocol: PROTOCOL_VERSION,
        port,
        hostname: mdns::HOSTNAME,
        mdns_active,
        client_addr: client
            .map(|c| c.ip().to_string())
            .filter(|_| client.is_some_and(|c| is_private_ip(c.ip()))),
        interfaces,
        connections: s.conns.counts(),
        hints,
    }
}

async fn diagnostics(
    State(s): State<Arc<Shared>>,
    ConnectInfo(Peer(addr)): ConnectInfo<Peer>,
) -> Json<Diagnostics> {
    Json(diagnose(&s, Some(addr)).await)
}

// ---- OBS output pages -------------------------------------------------------------------------

const OUTPUT_HTML: &str = include_str!("pages/output.html");

/// Transparent page for OBS / vMix "browser source". It joins as an anonymous read-only
/// `output` client, so no PIN is needed in OBS.
fn output_page(mode: &'static str) -> Response {
    let mut res = Html(OUTPUT_HTML.replace("__MODE__", mode)).into_response();
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self' ws: wss:",
        ),
    );
    res
}

// ---- websocket upgrade --------------------------------------------------------------------------

/// Browsers send `Origin` on WebSocket handshakes; a page served from elsewhere must not be able
/// to drive the server from a visitor's phone (cross-site WebSocket hijacking).
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true; // not a browser (or a native client)
    };
    let (Ok(origin), Some(host)) = (
        origin.to_str(),
        headers.get(header::HOST).and_then(|h| h.to_str().ok()),
    ) else {
        return false;
    };
    let authority = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .unwrap_or(origin);
    authority.eq_ignore_ascii_case(host)
}

async fn ws_upgrade(
    State(s): State<Arc<Shared>>,
    ConnectInfo(Peer(addr)): ConnectInfo<Peer>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !same_origin(&headers) {
        return ApiError::new(StatusCode::FORBIDDEN, "forbidden", "origen no permitido")
            .into_response();
    }
    let ip = addr.ip();
    upgrade
        .max_message_size(ws::MAX_FRAME_BYTES)
        .max_frame_size(ws::MAX_FRAME_BYTES)
        .on_upgrade(move |socket| ws::run(s, ip, socket))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(origin: Option<&str>, host: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(o) = origin {
            h.insert(header::ORIGIN, o.parse().unwrap());
        }
        if let Some(o) = host {
            h.insert(header::HOST, o.parse().unwrap());
        }
        h
    }

    #[test]
    fn origin_must_match_host() {
        assert!(same_origin(&headers(None, Some("192.168.1.5:7420"))));
        assert!(same_origin(&headers(
            Some("http://192.168.1.5:7420"),
            Some("192.168.1.5:7420")
        )));
        assert!(!same_origin(&headers(
            Some("http://evil.example"),
            Some("192.168.1.5:7420")
        )));
        assert!(!same_origin(&headers(
            Some("null"),
            Some("192.168.1.5:7420")
        )));
        assert!(!same_origin(&headers(
            Some("http://192.168.1.5:7420"),
            None
        )));
    }

    #[test]
    fn qr_is_an_svg() {
        let svg = qr_svg("http://192.168.1.5:7420/?pair=abc");
        assert!(svg.contains("<svg"));
    }
}
