//! Pairing hardening (one-time codes, expiry, lockout) and the optional HTTPS listener, against a
//! real server on loopback.

use hollyshow_app::AppService;
use hollyshow_core::Role;
use hollyshow_engine::Engine;
use hollyshow_server::{App, RunningServer, Server, ServerConfig, TlsConfig};
use hollyshow_songs::DefaultSplitter;
use hollyshow_storage_api::memory::MemoryStore;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

struct Harness {
    server: Server,
    running: RunningServer,
    addr: SocketAddr,
}

async fn start_with(tweak: impl FnOnce(&mut ServerConfig)) -> Harness {
    let service = Arc::new(AppService::new(
        MemoryStore::repos(),
        Arc::new(Engine::default()),
        Arc::new(DefaultSplitter),
    ));
    let mut config = ServerConfig {
        bind: SocketAddr::from(([127, 0, 0, 1], 0)),
        mdns: false,
        ..ServerConfig::default()
    };
    tweak(&mut config);
    let server = Server::new(App { service, config });
    let running = server.start().await.unwrap();
    let addr = running.local_addr();
    Harness {
        server,
        running,
        addr,
    }
}

fn plain_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

async fn pair(c: &reqwest::Client, base: &str, body: Value) -> (u16, Value) {
    let res = c
        .post(format!("{base}/api/pair"))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = res.status().as_u16();
    (status, res.json().await.unwrap_or(Value::Null))
}

fn other_pin(pin: &str) -> &'static str {
    if pin == "000000" {
        "000001"
    } else {
        "000000"
    }
}

#[test]
fn defaults_are_two_minutes_and_ten_is_the_ceiling() {
    let cfg = ServerConfig::default();
    assert_eq!(cfg.pairing_ttl, Duration::from_secs(120));
    assert_eq!(cfg.max_failures_per_ip, 5);
    assert_eq!(cfg.lockout, Duration::from_secs(300));
    assert!(cfg.tls.is_none(), "HTTPS is off by default");
    assert_eq!(
        hollyshow_server::clamp_pairing_ttl(0),
        Duration::from_secs(60)
    );
    assert_eq!(
        hollyshow_server::clamp_pairing_ttl(5),
        Duration::from_secs(300)
    );
    assert_eq!(
        hollyshow_server::clamp_pairing_ttl(99),
        Duration::from_secs(600)
    );
}

#[tokio::test]
async fn a_pin_pairs_exactly_one_device() {
    let h = start_with(|_| {}).await;
    let base = format!("http://{}", h.addr);
    let c = plain_client();
    let offer = h.server.pairing_offer(Role::Musician).unwrap();
    assert_eq!(offer.expires_in_secs, 120);
    let (status, body) = pair(&c, &base, json!({"pin": offer.pin, "deviceName": "A"})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["role"], "musician");
    let (status, _) = pair(&c, &base, json!({"pin": offer.pin, "deviceName": "B"})).await;
    assert_eq!(status, 401, "a second device cannot reuse the PIN");
    assert!(!h.server.offer_is_live(&offer.token));
    h.running.shutdown().await;
}

#[tokio::test]
async fn pairing_with_the_qr_token_also_spends_the_pin_and_vice_versa() {
    let h = start_with(|_| {}).await;
    let base = format!("http://{}", h.addr);
    let c = plain_client();

    let a = h.server.pairing_offer(Role::Stage).unwrap();
    assert!(h.server.offer_is_live(&a.token));
    let (status, _) = pair(&c, &base, json!({"token": a.token})).await;
    assert_eq!(status, 200);
    let (status, _) = pair(&c, &base, json!({"token": a.token})).await;
    assert_eq!(status, 401, "the QR token is one-time");
    let (status, _) = pair(&c, &base, json!({"pin": a.pin})).await;
    assert_eq!(status, 401, "the PIN died with the token");

    let b = h.server.pairing_offer(Role::Stage).unwrap();
    let (status, _) = pair(&c, &base, json!({"pin": b.pin})).await;
    assert_eq!(status, 200);
    let (status, _) = pair(&c, &base, json!({"token": b.token})).await;
    assert_eq!(status, 401, "the token died with the PIN");
    h.running.shutdown().await;
}

#[tokio::test]
async fn an_expired_offer_is_refused() {
    let h = start_with(|c| c.pairing_ttl = Duration::from_millis(300)).await;
    let base = format!("http://{}", h.addr);
    let c = plain_client();
    let offer = h.server.pairing_offer(Role::Stage).unwrap();
    assert!(h.server.offer_is_live(&offer.token));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!h.server.offer_is_live(&offer.token));
    let (status, _) = pair(&c, &base, json!({"pin": offer.pin})).await;
    assert_eq!(status, 401);
    let (status, _) = pair(&c, &base, json!({"token": offer.token})).await;
    assert_eq!(status, 401);
    h.running.shutdown().await;
}

#[tokio::test]
async fn five_wrong_pins_lock_the_address_even_for_the_right_one() {
    let h = start_with(|_| {}).await;
    let base = format!("http://{}", h.addr);
    let c = plain_client();
    let offer = h.server.pairing_offer(Role::Operator).unwrap();
    for i in 0..5 {
        let (status, _) = pair(&c, &base, json!({"pin": other_pin(&offer.pin)})).await;
        assert_eq!(status, 401, "attempt {i}");
    }
    let (status, body) = pair(&c, &base, json!({"pin": offer.pin})).await;
    assert_eq!(status, 429, "{body}");
    // The offer was not spent by the refused attempt: only the lock stood in the way.
    assert!(h.server.offer_is_live(&offer.token));
    h.running.shutdown().await;
}

#[tokio::test]
async fn four_wrong_pins_do_not_lock() {
    let h = start_with(|_| {}).await;
    let base = format!("http://{}", h.addr);
    let c = plain_client();
    let offer = h.server.pairing_offer(Role::Operator).unwrap();
    for _ in 0..4 {
        let (status, _) = pair(&c, &base, json!({"pin": other_pin(&offer.pin)})).await;
        assert_eq!(status, 401);
    }
    let (status, _) = pair(&c, &base, json!({"pin": offer.pin})).await;
    assert_eq!(status, 200);
    h.running.shutdown().await;
}

// ---- HTTPS -------------------------------------------------------------------------------------

fn tls_config(dir: &Path) -> impl FnOnce(&mut ServerConfig) + '_ {
    move |c| {
        c.tls = Some(TlsConfig {
            dir: dir.to_path_buf(),
        })
    }
}

fn trusting_client(dir: &Path) -> reqwest::Client {
    let pem = std::fs::read(dir.join("cert.pem")).unwrap();
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .tls_certs_only([reqwest::Certificate::from_pem(&pem).unwrap()])
        .build()
        .unwrap()
}

fn der_fingerprint(dir: &Path) -> String {
    let pem = std::fs::read_to_string(dir.join("cert.pem")).unwrap();
    let b64: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let der = decode_base64(&b64);
    Sha256::digest(&der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn decode_base64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| *c != b'=') {
        acc = (acc << 6) | u32::from(val(c));
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

#[tokio::test]
async fn https_serves_the_app_with_the_generated_certificate() {
    let dir = tempfile::tempdir().unwrap();
    let h = start_with(tls_config(dir.path())).await;
    let fingerprint = h.server.tls_fingerprint().expect("TLS is on");
    assert_eq!(fingerprint, der_fingerprint(dir.path()));

    let c = trusting_client(dir.path());
    let port = h.addr.port();
    for host in ["localhost", "127.0.0.1"] {
        let res = c
            .get(format!("https://{host}:{port}/health"))
            .send()
            .await
            .unwrap_or_else(|e| panic!("{host}: {e:?}"));
        assert_eq!(res.status(), 200);
        let v: Value = res.json().await.unwrap();
        assert!(v["protocol"].is_number());
    }

    // Pairing works over TLS and the offer advertises https URLs and the fingerprint.
    let offer = h.server.pairing_offer(Role::Stage).unwrap();
    assert_eq!(offer.fingerprint.as_deref(), Some(fingerprint.as_str()));
    assert!(
        offer.urls.iter().all(|u| u.starts_with("https://")),
        "{:?}",
        offer.urls
    );
    assert!(h.server.urls().iter().all(|u| u.starts_with("https://")));
    let (status, _) = pair(
        &c,
        &format!("https://localhost:{port}"),
        json!({"pin": offer.pin, "deviceName": "Tls"}),
    )
    .await;
    assert_eq!(status, 200);

    // A client that does not trust the certificate refuses it.
    let strict = reqwest::Client::builder().no_proxy().build().unwrap();
    assert!(strict
        .get(format!("https://localhost:{port}/health"))
        .send()
        .await
        .is_err());
    h.running.shutdown().await;
}

/// The real remote flow over TLS: pair through HTTPS, then open `wss://` and complete the
/// hello/welcome handshake with the issued token, trusting only the generated certificate.
#[tokio::test]
async fn wss_hello_and_welcome_work_over_tls() {
    use futures_util::{SinkExt, StreamExt};
    use rustls::pki_types::pem::PemObject;
    use tokio_tungstenite::tungstenite::Message;

    let dir = tempfile::tempdir().unwrap();
    let h = start_with(tls_config(dir.path())).await;
    let port = h.addr.port();

    let offer = h.server.pairing_offer(Role::Stage).unwrap();
    let c = trusting_client(dir.path());
    let (status, body) = pair(
        &c,
        &format!("https://localhost:{port}"),
        json!({"pin": offer.pin, "deviceName": "Wss"}),
    )
    .await;
    assert_eq!(status, 200);
    let token = body["token"].as_str().expect("pairing issues a token");

    // Trust exactly the generated certificate.
    let pem = std::fs::read(dir.path().join("cert.pem")).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls::pki_types::CertificateDer::pem_slice_iter(&pem) {
        roots.add(cert.unwrap()).unwrap();
    }
    let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));

    let tcp = tokio::net::TcpStream::connect(h.addr).await.unwrap();
    let name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let tls = connector.connect(name, tcp).await.expect("TLS handshake");
    let (mut ws, _) = tokio_tungstenite::client_async(format!("wss://localhost:{port}/ws"), tls)
        .await
        .expect("wss upgrade");

    let hello = json!({"type":"hello","protocolMin":1,"protocolMax":1,"role":"stage",
        "deviceName":"Wss","token":token,"pin":null,"capabilities":[]});
    ws.send(Message::text(hello.to_string())).await.unwrap();
    let welcome = loop {
        let m = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for welcome")
            .expect("socket closed before welcome")
            .unwrap();
        if let Message::Text(t) = m {
            let v: Value = serde_json::from_str(t.as_str()).unwrap();
            if v["type"] == "welcome" {
                break v;
            }
        }
    };
    assert_eq!(welcome["protocol"], 1);
    drop(ws);
    h.running.shutdown().await;
}

#[tokio::test]
async fn plain_http_is_redirected_to_https_on_the_same_port() {
    let dir = tempfile::tempdir().unwrap();
    let h = start_with(tls_config(dir.path())).await;
    let port = h.addr.port();
    let res = plain_client()
        .get(format!("http://127.0.0.1:{port}/some/path?x=1"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 307);
    assert_eq!(
        res.headers()["location"],
        format!("https://127.0.0.1:{port}/some/path?x=1").as_str()
    );
    h.running.shutdown().await;
}

#[tokio::test]
async fn the_certificate_survives_a_restart_and_https_is_off_without_config() {
    let dir = tempfile::tempdir().unwrap();
    let a = start_with(tls_config(dir.path())).await;
    let first = a.server.tls_fingerprint().unwrap();
    a.running.shutdown().await;
    let b = start_with(tls_config(dir.path())).await;
    assert_eq!(b.server.tls_fingerprint().unwrap(), first);
    b.running.shutdown().await;

    let off = start_with(|_| {}).await;
    assert!(off.server.tls_fingerprint().is_none());
    let res = plain_client()
        .get(format!("http://{}/health", off.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    off.running.shutdown().await;
}
