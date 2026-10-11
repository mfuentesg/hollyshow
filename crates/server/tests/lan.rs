//! End-to-end tests: a real server on loopback, real WebSocket clients (tokio-tungstenite).

use futures_util::future::join_all;
use futures_util::{SinkExt, StreamExt};
use hollyshow_app::AppService;
use hollyshow_core::{ItemId, Role, ShowEntry, Slide, SlideId, SlideKind};
use hollyshow_engine::Engine;
use hollyshow_server::{App, RunningServer, Server, ServerConfig};
use hollyshow_songs::DefaultSplitter;
use hollyshow_storage_api::memory::MemoryStore;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Harness {
    server: Server,
    running: RunningServer,
    service: Arc<AppService>,
    addr: SocketAddr,
}

fn slide(i: usize) -> Slide {
    let mut s = Slide::blank(format!("s{i}"));
    s.id = SlideId::new(format!("s{i}"));
    s.kind = SlideKind::Lyrics;
    s.lines = vec![format!("linea {i}")];
    s
}

async fn start_with(tweak: impl FnOnce(&mut ServerConfig)) -> Harness {
    let service = Arc::new(AppService::new(
        MemoryStore::repos(),
        Arc::new(Engine::default()),
        Arc::new(DefaultSplitter),
    ));
    service
        .load_entries(vec![ShowEntry {
            item_id: ItemId::new("song"),
            title: "Cancion".into(),
            slides: (0..30).map(slide).collect(),
            duration_secs: None,
        }])
        .unwrap();
    let mut config = ServerConfig {
        bind: SocketAddr::from(([127, 0, 0, 1], 0)),
        mdns: false,
        ..ServerConfig::default()
    };
    tweak(&mut config);
    let server = Server::new(App {
        service: service.clone(),
        config,
    });
    let running = server.start().await.unwrap();
    let addr = running.local_addr();
    Harness {
        server,
        running,
        service,
        addr,
    }
}

async fn start() -> Harness {
    start_with(|_| {}).await
}

async fn connect(h: &Harness) -> Ws {
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", h.addr))
        .await
        .unwrap();
    ws
}

async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::text(v.to_string())).await.unwrap();
}

/// Next JSON frame, `None` when the socket closed. Panics on a 5 s silence.
async fn recv(ws: &mut Ws) -> Option<Value> {
    loop {
        let m = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for a frame");
        match m {
            Some(Ok(Message::Text(t))) => return Some(serde_json::from_str(t.as_str()).unwrap()),
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
            Some(Ok(_)) => {}
        }
    }
}

async fn recv_type(ws: &mut Ws, ty: &str) -> Value {
    loop {
        let m = recv(ws)
            .await
            .unwrap_or_else(|| panic!("closed while waiting for {ty}"));
        if m["type"] == ty {
            return m;
        }
    }
}

fn hello(role: &str, token: Option<&str>, pin: Option<&str>) -> Value {
    json!({"type":"hello","protocolMin":1,"protocolMax":1,"role":role,"deviceName":"Test","token":token,"pin":pin,"capabilities":[]})
}

/// Pairs a new device over the WebSocket with a fresh offer; returns its session token.
async fn pair(h: &Harness, role: Role, name: &str) -> String {
    let offer = h.server.pairing_offer(role).unwrap();
    let mut ws = connect(h).await;
    let mut hi = hello("operator", None, Some(&offer.pin));
    hi["deviceName"] = json!(name);
    send(&mut ws, hi).await;
    let w = recv_type(&mut ws, "welcome").await;
    w["token"]
        .as_str()
        .expect("pairing returns a token")
        .to_owned()
}

/// Connects with a token and consumes welcome + snapshot; returns the socket and snapshot version.
async fn join(h: &Harness, role: &str, token: Option<&str>) -> (Ws, u64) {
    let mut ws = connect(h).await;
    send(&mut ws, hello(role, token, None)).await;
    let w = recv_type(&mut ws, "welcome").await;
    assert_eq!(w["protocol"], 1);
    let snap = recv_type(&mut ws, "snapshot").await;
    let v = snap["snapshot"]["view"]["version"].as_u64().unwrap();
    (ws, v)
}

/// The version a well-behaved client would echo back (what its last snapshot said).
fn cur(h: &Harness) -> u64 {
    h.service.engine().with_state(|s| s.version)
}

fn next_cmd(id: &str, expected: Option<u64>) -> Value {
    json!({"type":"command","id":id,"cmd":{"kind":"next","expectedVersion":expected}})
}

async fn ack(ws: &mut Ws, id: &str) -> Value {
    loop {
        let m = recv(ws).await.expect("closed while waiting for ack");
        if m["type"] == "ack" && m["id"] == id {
            return m["ack"].clone();
        }
    }
}

// ---- raw HTTP helper (no HTTP client dependency) -------------------------------------------

async fn http(
    addr: SocketAddr,
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<Value>,
) -> (u16, Value) {
    let mut s = TcpStream::connect(addr).await.unwrap();
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let auth = bearer
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).await.unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status = text.split_whitespace().nth(1).unwrap().parse().unwrap();
    let payload = text.split("\r\n\r\n").nth(1).unwrap_or("");
    // Responses may be chunked; the JSON object is the part between the outermost braces.
    let json = payload
        .find('{')
        .zip(payload.rfind('}'))
        .and_then(|(a, b)| serde_json::from_str(&payload[a..=b]).ok())
        .unwrap_or(Value::Null);
    (status, json)
}

// ---- tests ------------------------------------------------------------------------------------

#[tokio::test]
async fn congregation_joins_without_pairing_and_is_read_only_and_filtered() {
    let h = start().await;
    let mut ws = connect(&h).await;
    send(&mut ws, hello("congregation", None, None)).await;
    let w = recv_type(&mut ws, "welcome").await;
    assert_eq!(w["role"], "congregation");
    assert_eq!(w["token"], Value::Null);
    assert_eq!(w["heartbeatMs"], 5000);
    let snap = recv_type(&mut ws, "snapshot").await;
    assert_eq!(
        snap["snapshot"]["show"],
        Value::Null,
        "the audience never receives the show"
    );

    send(&mut ws, next_cmd("c1", Some(cur(&h)))).await;
    let a = ack(&mut ws, "c1").await;
    assert_eq!(a["ok"], false);
    assert_eq!(a["error"]["code"], "forbidden");
    h.running.shutdown().await;
}

#[tokio::test]
async fn anonymous_operator_and_unknown_tokens_are_rejected() {
    let h = start().await;
    for hi in [
        hello("operator", None, None),
        hello("musician", None, None),
        hello("operator", Some("deadbeef"), None),
    ] {
        let mut ws = connect(&h).await;
        send(&mut ws, hi).await;
        let r = recv_type(&mut ws, "reject").await;
        assert!(["unauthorized", "invalid_token"].contains(&r["error"]["code"].as_str().unwrap()));
        assert!(
            recv(&mut ws).await.is_none(),
            "socket closes after a reject"
        );
    }
    h.running.shutdown().await;
}

#[tokio::test]
async fn pairing_by_pin_issues_a_token_that_resumes_the_session_with_the_stored_role() {
    let h = start().await;
    let offer = h.server.pairing_offer(Role::Musician).unwrap();

    let mut ws = connect(&h).await;
    // The client asks for operator; the PIN was minted for a musician, who is what it gets.
    send(&mut ws, hello("operator", None, Some(&offer.pin))).await;
    let w = recv_type(&mut ws, "welcome").await;
    assert_eq!(w["role"], "musician");
    let token = w["token"].as_str().unwrap().to_owned();
    assert_eq!(token.len(), 64);

    let (mut again, _) = join(&h, "operator", Some(&token)).await;
    send(&mut again, next_cmd("c1", Some(cur(&h)))).await;
    assert_eq!(
        ack(&mut again, "c1").await["error"]["code"],
        "forbidden",
        "musicians are read-only"
    );

    // A wrong PIN is rejected.
    let mut bad = connect(&h).await;
    send(&mut bad, hello("operator", None, Some("12345x"))).await;
    assert_eq!(
        recv_type(&mut bad, "reject").await["error"]["code"],
        "invalid_pin"
    );
    h.running.shutdown().await;
}

#[tokio::test]
async fn pin_guessing_is_locked_out() {
    let h = start_with(|c| c.max_failures_per_ip = 3).await;
    let offer = h.server.pairing_offer(Role::Operator).unwrap();
    let wrong = if offer.pin == "000000" {
        "000001"
    } else {
        "000000"
    };
    for _ in 0..3 {
        let mut ws = connect(&h).await;
        send(&mut ws, hello("operator", None, Some(wrong))).await;
        assert_eq!(
            recv_type(&mut ws, "reject").await["error"]["code"],
            "invalid_pin"
        );
    }
    // Now even the right PIN is refused for this address.
    let mut ws = connect(&h).await;
    send(&mut ws, hello("operator", None, Some(&offer.pin))).await;
    assert_eq!(
        recv_type(&mut ws, "reject").await["error"]["code"],
        "locked"
    );
    h.running.shutdown().await;
}

#[tokio::test]
async fn http_pairing_with_the_qr_token_is_single_use_and_devices_can_be_managed() {
    let h = start().await;
    let op_offer = h.server.pairing_offer(Role::Operator).unwrap();
    let (status, body) = http(
        h.addr,
        "POST",
        "/api/pair",
        None,
        Some(json!({"token": op_offer.token, "deviceName": "Celular"})),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["role"], "operator");
    let op_token = body["token"].as_str().unwrap().to_owned();
    // The QR token is spent.
    let (status, _) = http(
        h.addr,
        "POST",
        "/api/pair",
        None,
        Some(json!({"token": op_offer.token})),
    )
    .await;
    assert_eq!(status, 401);

    // The operator mints an offer for a stage display through the API and it embeds a QR.
    let (status, body) = http(
        h.addr,
        "POST",
        "/api/pairing",
        Some(&op_token),
        Some(json!({"role":"stage"})),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    // A QR is only drawn when a phone could reach the address (CI machines may have no LAN).
    assert_eq!(
        body["qrSvg"].as_str().unwrap().contains("<svg"),
        body["networkAvailable"] == true
    );
    assert_eq!(body["pin"].as_str().unwrap().len(), 6);
    let pin = body["pin"].as_str().unwrap().to_owned();
    let (_, stage) = http(
        h.addr,
        "POST",
        "/api/pair",
        None,
        Some(json!({"pin": pin, "deviceName": "Pantalla"})),
    )
    .await;
    let stage_token = stage["token"].as_str().unwrap().to_owned();
    // A non-operator token cannot use the operator API.
    let (status, _) = http(h.addr, "GET", "/api/devices", Some(&stage_token), None).await;
    assert_eq!(status, 403);

    // The stage display connects, then shows up in the list and can be kicked + revoked.
    let (mut stage_ws, _) = join(&h, "stage", Some(&stage_token)).await;
    let (status, list) = http(h.addr, "GET", "/api/devices", Some(&op_token), None).await;
    assert_eq!(status, 200);
    assert_eq!(list["devices"].as_array().unwrap().len(), 2);
    let conn = list["connections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["role"] == "stage")
        .unwrap();
    assert_eq!(conn["name"], "Pantalla");
    let conn_id = conn["id"].as_u64().unwrap();

    let (status, _) = http(
        h.addr,
        "POST",
        &format!("/api/connections/{conn_id}/kick"),
        Some(&op_token),
        None,
    )
    .await;
    assert_eq!(status, 204);
    assert_eq!(
        recv_type(&mut stage_ws, "reject").await["error"]["code"],
        "kicked"
    );
    assert!(recv(&mut stage_ws).await.is_none());

    // Kick revokes by default: the token no longer works.
    let mut ws = connect(&h).await;
    send(&mut ws, hello("stage", Some(&stage_token), None)).await;
    assert_eq!(
        recv_type(&mut ws, "reject").await["error"]["code"],
        "invalid_token"
    );
    h.running.shutdown().await;
}

#[tokio::test]
async fn revoking_a_device_disconnects_it() {
    let h = start().await;
    let token = pair(&h, Role::Pastor, "Pastor").await;
    let (mut ws, _) = join(&h, "pastor", Some(&token)).await;
    let (device, connected) = h.server.paired_devices().remove(0);
    assert!(connected);
    assert!(h.server.revoke_device(&device.id));
    assert_eq!(
        recv_type(&mut ws, "reject").await["error"]["code"],
        "revoked"
    );
    assert!(h.server.paired_devices().is_empty());
    h.running.shutdown().await;
}

#[tokio::test]
async fn only_the_operator_can_navigate_and_pastor_is_limited() {
    let h = start().await;
    let pastor = pair(&h, Role::Pastor, "Pastor").await;
    let (mut ws, _) = join(&h, "pastor", Some(&pastor)).await;
    send(
        &mut ws,
        json!({"type":"command","id":"b1","cmd":{"kind":"blackout","on":true}}),
    )
    .await;
    assert_eq!(ack(&mut ws, "b1").await["error"]["code"], "forbidden");
    send(
        &mut ws,
        json!({"type":"query","id":"q1","query":{"kind":"searchSongs","text":"a","limit":5}}),
    )
    .await;
    let r = recv_type(&mut ws, "queryResult").await;
    assert_eq!(r["error"]["code"], "forbidden");
    send(
        &mut ws,
        json!({"type":"query","id":"q2","query":{"kind":"listBibleVersions"}}),
    )
    .await;
    let r = recv_type(&mut ws, "queryResult").await;
    assert_eq!(r["error"], Value::Null);
    assert!(r["result"]["items"].is_array());
    h.running.shutdown().await;
}

#[tokio::test]
async fn concurrent_next_with_the_same_expected_version_advances_exactly_one_slide() {
    let h = start().await;
    let token = pair(&h, Role::Operator, "Operador").await;
    let mut clients = Vec::new();
    for _ in 0..8 {
        clients.push(join(&h, "operator", Some(&token)).await);
    }
    let v0 = clients[0].1;
    let results = join_all(
        clients
            .iter_mut()
            .enumerate()
            .map(|(i, (ws, _))| async move {
                let id = format!("n{i}");
                send(ws, next_cmd(&id, Some(v0))).await;
                ack(ws, &id).await
            }),
    )
    .await;

    let accepted = results.iter().filter(|a| a["ok"] == true).count();
    assert_eq!(accepted, 1, "{results:?}");
    for a in results.iter().filter(|a| a["ok"] == false) {
        assert_eq!(a["error"]["code"], "version_mismatch");
    }
    let live = h
        .service
        .engine()
        .with_state(|s| (s.version, s.live.as_ref().map(|c| c.slide_index)));
    assert_eq!(live, (v0 + 1, Some(0)));
    h.running.shutdown().await;
}

#[tokio::test]
async fn remote_relative_commands_without_expected_version_are_rejected() {
    let h = start().await;
    let token = pair(&h, Role::Operator, "Operador").await;
    let (mut ws, v0) = join(&h, "operator", Some(&token)).await;
    send(&mut ws, next_cmd("n0", None)).await;
    let a = ack(&mut ws, "n0").await;
    assert_eq!(a["ok"], false, "{a}");
    assert_eq!(a["error"]["code"], "invalid");
    assert_eq!(h.service.engine().with_state(|s| s.version), v0);
    h.running.shutdown().await;
}

#[tokio::test]
async fn duplicate_command_ids_are_not_applied_twice() {
    let h = start().await;
    let token = pair(&h, Role::Operator, "Operador").await;
    let (mut ws, _) = join(&h, "operator", Some(&token)).await;
    send(&mut ws, next_cmd("same", Some(cur(&h)))).await;
    let first = ack(&mut ws, "same").await;
    send(&mut ws, next_cmd("same", Some(cur(&h)))).await;
    let second = ack(&mut ws, "same").await;
    assert_eq!(first["duplicate"], false);
    assert_eq!(second["duplicate"], true);
    assert_eq!(first["version"], second["version"]);
    h.running.shutdown().await;
}

#[tokio::test]
async fn every_client_sees_the_change_with_consecutive_versions_and_role_filtering() {
    let h = start().await;
    let op = pair(&h, Role::Operator, "Operador").await;
    let stage = pair(&h, Role::Stage, "Escenario").await;
    let (mut operator, v0) = join(&h, "operator", Some(&op)).await;
    let (mut stage_ws, _) = join(&h, "stage", Some(&stage)).await;
    let (mut crowd, _) = join(&h, "congregation", None).await;

    for i in 0..3u64 {
        let id = format!("n{i}");
        send(&mut operator, next_cmd(&id, Some(cur(&h)))).await;
        assert_eq!(ack(&mut operator, &id).await["ok"], true);
    }
    for ws in [&mut stage_ws, &mut crowd] {
        // One command may emit several events at the same version (live + preview); versions
        // must never skip or go backwards.
        let mut versions: Vec<u64> = Vec::new();
        let mut last_state = Value::Null;
        while versions.last() != Some(&(v0 + 3)) {
            let m = recv(ws).await.unwrap();
            match m["type"].as_str().unwrap() {
                "event" => {
                    let v = m["version"].as_u64().unwrap();
                    if versions.last() != Some(&v) {
                        versions.push(v);
                    }
                }
                "state" => last_state = m["view"].clone(),
                _ => {}
            }
        }
        assert_eq!(versions, vec![v0 + 1, v0 + 2, v0 + 3]);
        assert_eq!(last_state["live"]["slideIndex"], 2);
    }
    // The audience gets no preview / timers; the stage does get the preview ("next").
    send(&mut crowd, json!({"type":"resync"})).await;
    let snap = recv_type(&mut crowd, "snapshot").await;
    assert_eq!(snap["snapshot"]["view"]["preview"], Value::Null);
    assert_eq!(snap["snapshot"]["show"], Value::Null);
    send(&mut stage_ws, json!({"type":"resync"})).await;
    let snap = recv_type(&mut stage_ws, "snapshot").await;
    assert_ne!(snap["snapshot"]["view"]["preview"], Value::Null);
    h.running.shutdown().await;
}

#[tokio::test]
async fn congregation_connection_limit_applies_and_frees_slots() {
    let h = start_with(|c| c.max_congregation = 2).await;
    let (a, _) = join(&h, "congregation", None).await;
    let (_b, _) = join(&h, "congregation", None).await;
    let mut third = connect(&h).await;
    send(&mut third, hello("congregation", None, None)).await;
    assert_eq!(
        recv_type(&mut third, "reject").await["error"]["code"],
        "capacity"
    );
    // Paired devices are not subject to the audience limit.
    let op = pair(&h, Role::Operator, "Operador").await;
    join(&h, "operator", Some(&op)).await;
    // Leaving frees a slot.
    drop(a);
    tokio::time::sleep(Duration::from_millis(200)).await;
    join(&h, "congregation", None).await;
    h.running.shutdown().await;
}

#[tokio::test]
async fn kicked_anonymous_clients_are_barred_for_a_while() {
    let h = start().await;
    let (mut ws, _) = join(&h, "congregation", None).await;
    let id = h.server.connections()[0].id;
    assert!(h.server.kick(id, true));
    assert_eq!(
        recv_type(&mut ws, "reject").await["error"]["code"],
        "kicked"
    );
    let mut again = connect(&h).await;
    send(&mut again, hello("congregation", None, None)).await;
    assert_eq!(
        recv_type(&mut again, "reject").await["error"]["code"],
        "kicked"
    );
    h.running.shutdown().await;
}

#[tokio::test]
async fn heartbeat_pings_are_answered_and_silent_clients_are_dropped() {
    let h = start_with(|c| c.heartbeat = Duration::from_millis(100)).await;
    let (mut ws, _) = join(&h, "congregation", None).await;
    send(&mut ws, json!({"type":"ping","t":42})).await;
    assert_eq!(recv_type(&mut ws, "pong").await["t"], 42);

    // Not reading means no automatic WebSocket pongs either: after 3 heartbeats of silence
    // the server hangs up.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let mut closed = false;
    for _ in 0..50 {
        match tokio::time::timeout(Duration::from_secs(2), ws.next()).await {
            Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => {
                closed = true;
                break;
            }
            Ok(Some(Ok(_))) => {}
            Err(_) => break,
        }
    }
    assert!(closed, "silent client should have been disconnected");
    h.running.shutdown().await;
}

#[tokio::test]
async fn a_live_client_survives_many_heartbeats() {
    let h = start_with(|c| c.heartbeat = Duration::from_millis(100)).await;
    let (mut ws, _) = join(&h, "congregation", None).await;
    for t in 0..8 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        send(&mut ws, json!({"type":"ping","t":t})).await;
        assert_eq!(recv_type(&mut ws, "pong").await["t"], t);
    }
    h.running.shutdown().await;
}

#[tokio::test]
async fn flooding_commands_is_throttled_not_fatal() {
    let h = start_with(|c| {
        c.frame_burst = 5;
        c.frames_per_sec = 0.1;
    })
    .await;
    let token = pair(&h, Role::Operator, "Operador").await;
    let (mut ws, _) = join(&h, "operator", Some(&token)).await;
    for i in 0..12 {
        send(
            &mut ws,
            json!({"type":"command","id":format!("b{i}"),"cmd":{"kind":"blackout","on":true}}),
        )
        .await;
    }
    let mut limited = 0;
    for i in 0..12 {
        let a = ack(&mut ws, &format!("b{i}")).await;
        if a["error"]["code"] == "rate_limited" {
            limited += 1;
        }
    }
    assert!(limited >= 6, "limited = {limited}");
    // Heartbeats still work while throttled.
    send(&mut ws, json!({"type":"ping","t":1})).await;
    recv_type(&mut ws, "pong").await;
    h.running.shutdown().await;
}

#[tokio::test]
async fn malformed_and_oversized_frames_do_not_hurt_the_show() {
    let h = start().await;
    let token = pair(&h, Role::Operator, "Operador").await;
    let (mut ws, _) = join(&h, "operator", Some(&token)).await;
    ws.send(Message::text("this is not json")).await.unwrap();
    ws.send(Message::text(
        json!({"type":"somethingNew","x":1}).to_string(),
    ))
    .await
    .unwrap();
    send(&mut ws, next_cmd("ok", Some(cur(&h)))).await;
    assert_eq!(ack(&mut ws, "ok").await["ok"], true);

    // A frame over the limit closes that socket only.
    let mut big = connect(&h).await;
    let _ = big.send(Message::text("x".repeat(200 * 1024))).await;
    while recv(&mut big).await.is_some() {}
    send(&mut ws, next_cmd("ok2", Some(cur(&h)))).await;
    assert_eq!(ack(&mut ws, "ok2").await["ok"], true);
    h.running.shutdown().await;
}

#[tokio::test]
async fn protocol_mismatch_and_missing_hello_are_rejected() {
    let h = start().await;
    let mut ws = connect(&h).await;
    send(&mut ws, json!({"type":"hello","protocolMin":7,"protocolMax":9,"role":"congregation","deviceName":"x","token":null,"pin":null,"capabilities":[]})).await;
    assert_eq!(
        recv_type(&mut ws, "reject").await["error"]["code"],
        "protocol_mismatch"
    );

    let mut ws = connect(&h).await;
    send(&mut ws, json!({"type":"ping","t":1})).await;
    assert_eq!(
        recv_type(&mut ws, "reject").await["error"]["code"],
        "bad_request"
    );
    h.running.shutdown().await;
}

#[tokio::test]
async fn silent_sockets_get_a_hello_timeout() {
    let h = start_with(|c| c.hello_timeout = Duration::from_millis(150)).await;
    let mut ws = connect(&h).await;
    assert_eq!(
        recv_type(&mut ws, "reject").await["error"]["code"],
        "hello_timeout"
    );
    h.running.shutdown().await;
}

#[tokio::test]
async fn cross_origin_websocket_handshakes_are_refused() {
    let h = start().await;
    let mut req = format!("ws://{}/ws", h.addr).into_client_request().unwrap();
    req.headers_mut()
        .insert("Origin", "http://evil.example".parse().unwrap());
    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("handshake must fail");
    assert!(err.to_string().contains("403"), "{err}");

    let mut ok = format!("ws://{}/ws", h.addr).into_client_request().unwrap();
    ok.headers_mut()
        .insert("Origin", format!("http://{}", h.addr).parse().unwrap());
    assert!(tokio_tungstenite::connect_async(ok).await.is_ok());
    h.running.shutdown().await;
}

#[tokio::test]
async fn obs_output_pages_join_as_outputs_with_their_own_budget() {
    let h = start_with(|c| {
        c.max_congregation = 1;
        c.max_outputs = 1;
    })
    .await;
    let mut obs = connect(&h).await;
    send(&mut obs, json!({"type":"hello","protocolMin":1,"protocolMax":1,"role":"congregation","deviceName":"Salida full","token":null,"pin":null,"capabilities":["output"]})).await;
    recv_type(&mut obs, "welcome").await;
    recv_type(&mut obs, "snapshot").await;
    assert_eq!(h.server.counts().outputs, 1);
    // The audience limit is untouched by the OBS page.
    join(&h, "congregation", None).await;
    let mut second = connect(&h).await;
    send(&mut second, json!({"type":"hello","protocolMin":1,"protocolMax":1,"role":"congregation","deviceName":"x","token":null,"pin":null,"capabilities":["output"]})).await;
    assert_eq!(
        recv_type(&mut second, "reject").await["error"]["code"],
        "capacity"
    );

    let (status, _) = http(h.addr, "GET", "/output/lower-third", None, None).await;
    assert_eq!(status, 200);
    h.running.shutdown().await;
}

#[tokio::test]
async fn diagnostics_endpoint_reports_the_listening_port() {
    let h = start().await;
    let (status, body) = http(h.addr, "GET", "/api/diagnostics", None, None).await;
    assert_eq!(status, 200);
    assert_eq!(body["port"], h.addr.port());
    let ifaces = body["interfaces"].as_array().unwrap();
    let lo = ifaces
        .iter()
        .find(|i| i["ip"] == "127.0.0.1")
        .expect("loopback listed");
    assert_eq!(lo["isLoopback"], true);
    assert_eq!(lo["reachable"], Value::Null, "loopback is not probed");
    let d = h.server.diagnostics().await;
    assert!(serde_json::to_value(&d).unwrap()["connections"]["total"].is_number());
    h.running.shutdown().await;
}

#[tokio::test]
async fn binding_a_public_address_is_refused() {
    let service = Arc::new(AppService::new(
        MemoryStore::repos(),
        Arc::new(Engine::default()),
        Arc::new(DefaultSplitter),
    ));
    let server = Server::new(App {
        service,
        config: ServerConfig {
            bind: SocketAddr::from(([8, 8, 8, 8], 0)),
            mdns: false,
            ..ServerConfig::default()
        },
    });
    let Err(err) = server.start().await else {
        panic!("must refuse")
    };
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[tokio::test]
async fn shutdown_closes_open_sockets_promptly() {
    let h = start().await;
    let (mut ws, _) = join(&h, "congregation", None).await;
    let started = std::time::Instant::now();
    h.running.shutdown().await;
    assert_eq!(
        recv_type(&mut ws, "reject").await["error"]["code"],
        "shutdown"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

// ---- idempotency across reconnects ---------------------------------------------------------

#[tokio::test]
async fn a_command_resent_after_reconnecting_is_recognised_as_a_duplicate() {
    let h = start().await;
    let token = pair(&h, Role::Operator, "Operador").await;
    let (mut ws, v) = join(&h, "operator", Some(&token)).await;
    send(&mut ws, next_cmd("c-abc-1", Some(v))).await;
    let first = ack(&mut ws, "c-abc-1").await;
    assert_eq!(first["duplicate"], false);
    let advanced = h.service.engine().with_state(|s| s.version);
    // The ack was "lost": the socket drops and the client resends the same id on a new socket.
    drop(ws);
    let (mut ws2, _) = join(&h, "operator", Some(&token)).await;
    send(&mut ws2, next_cmd("c-abc-1", Some(v))).await;
    let again = ack(&mut ws2, "c-abc-1").await;
    assert_eq!(again["duplicate"], true, "{again}");
    assert_eq!(again["version"], first["version"]);
    assert_eq!(h.service.engine().with_state(|s| s.version), advanced);
    h.running.shutdown().await;
}

#[tokio::test]
async fn devices_do_not_share_dedupe_windows_and_the_window_ends_after_the_grace() {
    let h = start_with(|c| c.session_grace = Duration::from_millis(200)).await;
    let ta = pair(&h, Role::Operator, "A").await;
    let tb = pair(&h, Role::Operator, "B").await;
    let (mut a, _) = join(&h, "operator", Some(&ta)).await;
    let (mut b, _) = join(&h, "operator", Some(&tb)).await;
    let now_v = |h: &Harness| h.service.engine().with_state(|s| s.version);
    send(&mut a, next_cmd("c-1", Some(now_v(&h)))).await;
    assert_eq!(ack(&mut a, "c-1").await["duplicate"], false);
    // Same id from another device is a different command.
    send(&mut b, next_cmd("c-1", Some(now_v(&h)))).await;
    assert_eq!(ack(&mut b, "c-1").await["duplicate"], false);
    drop(a);
    tokio::time::sleep(Duration::from_millis(600)).await;
    let (mut a2, _) = join(&h, "operator", Some(&ta)).await;
    send(&mut a2, next_cmd("c-1", Some(now_v(&h)))).await;
    assert_eq!(
        ack(&mut a2, "c-1").await["duplicate"],
        false,
        "the window was released once the grace ran out"
    );
    h.running.shutdown().await;
}

// ---- connection flood / slowloris ----------------------------------------------------------

/// True when the peer closes the socket (EOF or reset) within `wait`.
async fn closed_by_peer(s: &mut TcpStream, wait: Duration) -> bool {
    let mut buf = [0u8; 64];
    match tokio::time::timeout(wait, s.read(&mut buf)).await {
        Ok(Ok(0) | Err(_)) => true,
        Ok(Ok(_)) | Err(_) => false,
    }
}

#[tokio::test]
async fn silent_tcp_connections_cannot_exhaust_the_server() {
    let h = start_with(|c| {
        c.max_sockets_per_ip = 5;
        c.header_read_timeout = Duration::from_secs(30);
    })
    .await;
    let mut idle = Vec::new();
    for _ in 0..5 {
        idle.push(TcpStream::connect(h.addr).await.unwrap());
    }
    // Further sockets from the same address are closed at accept, before any byte is read.
    let mut extra = TcpStream::connect(h.addr).await.unwrap();
    assert!(closed_by_peer(&mut extra, Duration::from_secs(2)).await);
    // The held ones are still open (the server did not drop them to make room).
    assert!(!closed_by_peer(&mut idle[0], Duration::from_millis(200)).await);
    // Releasing the idle sockets frees the budget again.
    drop(idle);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (status, body) = http(h.addr, "GET", "/health", None, None).await;
    assert_eq!(status, 200);
    assert_eq!(body["ok"], true);
    h.running.shutdown().await;
}

#[tokio::test]
async fn a_client_dribbling_request_headers_is_cut_off() {
    let h = start_with(|c| c.header_read_timeout = Duration::from_millis(300)).await;
    let mut s = TcpStream::connect(h.addr).await.unwrap();
    s.write_all(b"GET /health HTTP/1.1\r\nHost: x\r\nX-Slow: ")
        .await
        .unwrap();
    assert!(
        closed_by_peer(&mut s, Duration::from_secs(3)).await,
        "an unfinished header block must not hold the connection open"
    );
    // A well-behaved client is unaffected.
    assert_eq!(http(h.addr, "GET", "/health", None, None).await.0, 200);
    h.running.shutdown().await;
}

#[tokio::test]
async fn page_loads_and_websocket_upgrades_are_rate_limited_per_address() {
    let h = start_with(|c| {
        c.http_burst = 1; // web budget is 4x
        c.http_per_sec = 0.0;
    })
    .await;
    let mut limited = false;
    for _ in 0..8 {
        if http(h.addr, "GET", "/", None, None).await.0 == 429 {
            limited = true;
            break;
        }
    }
    assert!(limited, "static routes share the per-address limiter");
    // /health stays open for monitoring.
    assert_eq!(http(h.addr, "GET", "/health", None, None).await.0, 200);
    let r = tokio_tungstenite::connect_async(format!("ws://{}/ws", h.addr)).await;
    assert!(r.is_err(), "the upgrade is limited too");
    h.running.shutdown().await;
}
