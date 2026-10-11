//! WebSocket sessions (PLAN §3.4): `hello` -> `welcome` -> `snapshot`, then role-filtered
//! `state`/`event` pushes and acknowledged commands.
//!
//! Each socket runs in its own tokio task. Everything it needs from the application goes through
//! `AppService`, so authorisation and role projection are identical to the Tauri IPC path.

use crate::connections::{ConnGuard, ConnKind, Refused};
use crate::pairing::{Credential, PairError};
use crate::ratelimit::TokenBucket;
use crate::shared::Shared;
use crate::util::{now_ms, sanitize_name};
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use hollyshow_app::{AppService, ClientCtx};
use hollyshow_core::{
    negotiate, Ack, ClientMsg, Command, CommandId, QueryId, QueryRequest, Role, ServerMsg,
    SessionId, WireError, CAPABILITIES,
};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::time::{interval, sleep_until, timeout, Instant, MissedTickBehavior};

/// Largest frame accepted from a client; a hello or a command is a few hundred bytes.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// Consecutive undecodable frames tolerated before the socket is dropped.
const MAX_BAD_FRAMES: u32 = 8;
/// Consecutive rate-limited frames tolerated before the socket is dropped.
const MAX_THROTTLED: u32 = 100;

fn reject(code: &str, message: &str) -> ServerMsg {
    ServerMsg::Reject {
        error: WireError {
            code: code.into(),
            message: message.into(),
        },
    }
}

fn refuse(code: &str, message: &str) -> WireError {
    WireError {
        code: code.into(),
        message: message.into(),
    }
}

struct Admitted {
    guard: ConnGuard,
    role: Role,
    /// Plain session token, only for a device that just paired.
    new_token: Option<String>,
    protocol: u32,
    kind: ConnKind,
    /// Paired device behind the socket; `None` for anonymous clients.
    device_id: Option<String>,
}

/// Authenticates a `hello`: token resume, PIN pairing or anonymous congregation/output access.
/// Pure of I/O apart from the storage trait, so it runs on the blocking pool.
#[allow(clippy::too_many_arguments)]
fn admit(
    shared: &Arc<Shared>,
    ip: IpAddr,
    protocol_min: u32,
    protocol_max: u32,
    role: Role,
    device_name: &str,
    token: Option<&str>,
    pin: Option<&str>,
    capabilities: &[String],
) -> Result<Admitted, WireError> {
    if !shared.hello_limiter.check(ip) {
        return Err(refuse("rate_limited", "demasiados intentos de conexion"));
    }
    let protocol = negotiate(protocol_min, protocol_max)?;
    let mut name = sanitize_name(device_name);
    let now = now_ms();

    let (granted, device_id, kind, new_token) = if let Some(token) = token {
        match shared.devices.authenticate(token) {
            Some(a) => {
                name = a.device.name.clone();
                (
                    a.device.role,
                    Some(a.device.id),
                    ConnKind::Paired,
                    a.new_token,
                )
            }
            // 256-bit tokens cannot be guessed, so this does not feed the PIN lockout (a phone
            // whose token was revoked would otherwise lock itself out of re-pairing). The
            // handshake limiter already throttles retries.
            None => {
                return Err(refuse(
                    "invalid_token",
                    "sesion no valida: vuelve a emparejar",
                ))
            }
        }
    } else if let Some(pin) = pin {
        let role = match shared.pairing.redeem(Credential::Pin(pin), ip) {
            Ok(r) => r,
            Err(PairError::Invalid) => {
                return Err(refuse("invalid_pin", "PIN incorrecto o caducado"))
            }
            Err(PairError::Locked { retry_secs }) => {
                return Err(refuse(
                    "locked",
                    &format!("demasiados intentos; reintenta en {retry_secs} s"),
                ))
            }
            Err(PairError::NoEntropy) => {
                return Err(refuse("internal", "sin fuente de aleatoriedad"))
            }
        };
        let Some((device, token)) = shared.devices.add(&name, role) else {
            return Err(refuse("internal", "no se pudo crear la sesion"));
        };
        (role, Some(device.id), ConnKind::Paired, Some(token))
    } else if role == Role::Congregation && shared.config.congregation_open {
        let kind = if capabilities.iter().any(|c| c == "output") {
            ConnKind::Output
        } else {
            ConnKind::Congregation
        };
        (Role::Congregation, None, kind, None)
    } else {
        return Err(refuse(
            "unauthorized",
            "se requiere emparejar este dispositivo",
        ));
    };

    let guard = shared
        .conns
        .register(ip, device_id.clone(), name, granted, kind, now)
        .map_err(|r| match r {
            Refused::Full => refuse("capacity", "el servidor alcanzo su limite de conexiones"),
            Refused::Banned => refuse("kicked", "dispositivo expulsado temporalmente"),
        })?;
    Ok(Admitted {
        guard,
        role: granted,
        new_token,
        protocol,
        kind,
        device_id,
    })
}

async fn send(socket: &mut WebSocket, shared: &Shared, msg: &ServerMsg) -> bool {
    let Ok(text) = serde_json::to_string(msg) else {
        log::error!("could not serialise a server message");
        return true;
    };
    matches!(
        timeout(
            shared.config.write_timeout,
            socket.send(Message::text(text))
        )
        .await,
        Ok(Ok(()))
    )
}

async fn close(socket: &mut WebSocket, shared: &Shared, msg: Option<ServerMsg>) {
    if let Some(m) = msg {
        let _ = send(socket, shared, &m).await;
    }
    let _ = timeout(
        std::time::Duration::from_secs(1),
        socket.send(Message::Close(Some(CloseFrame {
            code: 1000,
            reason: "".into(),
        }))),
    )
    .await;
}

pub async fn run(shared: Arc<Shared>, ip: IpAddr, mut socket: WebSocket) {
    let mut shutdown = shared.shutdown.subscribe();

    // 1. The first frame must be a hello, soon.
    let hello = match timeout(shared.config.hello_timeout, first_text(&mut socket)).await {
        Ok(Some(t)) => t,
        _ => {
            return close(
                &mut socket,
                &shared,
                Some(reject("hello_timeout", "se esperaba hello")),
            )
            .await
        }
    };
    let Ok(ClientMsg::Hello {
        protocol_min,
        protocol_max,
        role,
        device_name,
        token,
        pin,
        capabilities,
    }) = serde_json::from_str::<ClientMsg>(&hello)
    else {
        return close(
            &mut socket,
            &shared,
            Some(reject("bad_request", "el primer mensaje debe ser hello")),
        )
        .await;
    };

    let admit_shared = shared.clone();
    let admitted = tokio::task::spawn_blocking(move || {
        admit(
            &admit_shared,
            ip,
            protocol_min,
            protocol_max,
            role,
            &device_name,
            token.as_deref(),
            pin.as_deref(),
            &capabilities,
        )
    })
    .await;
    let Admitted {
        guard,
        role,
        new_token,
        protocol,
        kind,
        device_id,
    } = match admitted {
        Ok(Ok(a)) => a,
        Ok(Err(error)) => {
            return close(&mut socket, &shared, Some(ServerMsg::Reject { error })).await
        }
        Err(_) => {
            return close(
                &mut socket,
                &shared,
                Some(reject("internal", "error interno")),
            )
            .await
        }
    };

    // A paired device keeps one session identity across sockets, so a command resent after a
    // reconnect hits the same dedupe window. Anonymous clients cannot send commands and get a
    // per-socket identity.
    let session = SessionId::new(match &device_id {
        Some(id) => format!("dev-{id}"),
        None => format!("ws-{}", guard.id),
    });
    let ctx = ClientCtx::new(session.clone(), role);
    let service = shared.service.clone();

    // 2. Subscribe BEFORE the snapshot so no update can fall in the gap (see `RoleFeed`).
    let mut feed = service.feed(role);
    let welcome = ServerMsg::Welcome {
        protocol,
        role,
        token: new_token,
        server_name: shared.config.server_name.clone(),
        heartbeat_ms: u32::try_from(shared.config.heartbeat.as_millis()).unwrap_or(5000),
        capabilities: CAPABILITIES.iter().map(|c| (*c).to_owned()).collect(),
        now_ms: service.engine().now_ms(),
    };
    if !send(&mut socket, &shared, &welcome).await
        || !send(&mut socket, &shared, &service.snapshot_msg(role)).await
    {
        drop(guard);
        release_session(&shared, &session, device_id);
        return;
    }
    log::info!(
        "{kind:?} client {} connected as {role:?} from {ip}",
        guard.id
    );

    session_loop(
        &shared,
        &service,
        &ctx,
        guard,
        &mut socket,
        &mut feed,
        &mut shutdown,
    )
    .await;
    release_session(&shared, &session, device_id);
}

/// Forgets a socket's dedupe window. A paired device's window outlives its socket for
/// `session_grace` (and is kept while another socket of the device is open), which is what makes
/// a command resent after a dropped connection idempotent.
fn release_session(shared: &Arc<Shared>, session: &SessionId, device_id: Option<String>) {
    let engine = shared.service.engine().clone();
    let Some(device_id) = device_id else {
        engine.end_session(session);
        return;
    };
    let grace = shared.config.session_grace;
    if grace.is_zero() {
        if !shared.conns.is_connected(&device_id) {
            engine.end_session(session);
        }
        return;
    }
    let (shared, session) = (shared.clone(), session.clone());
    let mut stop = shared.shutdown.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            () = tokio::time::sleep(grace) => {}
            _ = stop.wait_for(|down| *down) => {}
        }
        if !shared.conns.is_connected(&device_id) {
            engine.end_session(&session);
        }
    });
}

async fn first_text(socket: &mut WebSocket) -> Option<String> {
    loop {
        match socket.recv().await? {
            Ok(Message::Text(t)) => return Some(t.as_str().to_owned()),
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            _ => return None,
        }
    }
}

async fn session_loop(
    shared: &Arc<Shared>,
    service: &Arc<AppService>,
    ctx: &ClientCtx,
    mut guard: ConnGuard,
    socket: &mut WebSocket,
    feed: &mut hollyshow_app::RoleFeed,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) {
    let cfg = &shared.config;
    if *shutdown.borrow() {
        return;
    }
    let mut beat = interval(cfg.heartbeat);
    beat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut deadline = Instant::now() + cfg.idle_timeout();
    let mut bucket = TokenBucket::new(cfg.frame_burst, cfg.frames_per_sec);
    let (mut bad_frames, mut throttled) = (0u32, 0u32);

    loop {
        tokio::select! {
            biased;
            _ = shutdown.changed() => {
                return close(socket, shared, Some(reject("shutdown", "servidor detenido"))).await;
            }
            _ = guard.kicked.changed() => {
                let reason = guard.kicked.borrow().clone().unwrap_or_else(|| "kicked".into());
                return close(socket, shared, Some(reject(&reason, "desconectado por el operador"))).await;
            }
            _ = sleep_until(deadline) => {
                log::info!("client {} timed out", guard.id);
                return close(socket, shared, None).await;
            }
            batch = feed.next() => {
                let Some(msgs) = batch else { return close(socket, shared, None).await };
                for m in &msgs {
                    if !send(socket, shared, m).await { return; }
                }
            }
            _ = beat.tick() => {
                // Transport-level ping keeps NATs and phones' radios awake; the reply (a pong
                // frame) also counts as liveness.
                if timeout(cfg.write_timeout, socket.send(Message::Ping(Vec::new().into()))).await.map_or(true, |r| r.is_err()) {
                    return;
                }
            }
            frame = socket.recv() => {
                let Some(Ok(frame)) = frame else { return };
                deadline = Instant::now() + cfg.idle_timeout();
                let text = match frame {
                    Message::Text(t) => t,
                    Message::Close(_) => return,
                    Message::Binary(_) => { bad_frames += 1; if bad_frames > MAX_BAD_FRAMES { return } else { continue } }
                    _ => continue,
                };
                let msg = match serde_json::from_str::<ClientMsg>(text.as_str()) {
                    Ok(m) => { bad_frames = 0; m }
                    Err(e) => {
                        log::debug!("client {} sent an undecodable frame: {e}", guard.id);
                        bad_frames += 1;
                        if bad_frames > MAX_BAD_FRAMES {
                            return close(socket, shared, Some(reject("bad_request", "demasiados mensajes invalidos"))).await;
                        }
                        continue;
                    }
                };
                let reply = match msg {
                    // Heartbeat answers are never throttled: that would turn a busy client into
                    // a "dead" one.
                    ClientMsg::Ping { t } => vec![ServerMsg::Pong { t }],
                    ClientMsg::Hello { .. } | ClientMsg::Unknown => Vec::new(),
                    other => {
                        if !bucket.take() {
                            throttled += 1;
                            if throttled > MAX_THROTTLED {
                                return close(socket, shared, Some(reject("rate_limited", "demasiados mensajes"))).await;
                            }
                            throttle_reply(service, other)
                        } else {
                            throttled = 0;
                            handle(service, ctx, other).await
                        }
                    }
                };
                for m in &reply {
                    if !send(socket, shared, m).await { return; }
                }
            }
        }
    }
}

fn rate_error() -> WireError {
    WireError {
        code: "rate_limited".into(),
        message: "demasiados mensajes".into(),
    }
}

fn throttle_reply(service: &AppService, msg: ClientMsg) -> Vec<ServerMsg> {
    match msg {
        ClientMsg::Command { id, .. } => vec![ServerMsg::ack(
            id,
            Ack {
                ok: false,
                version: service.engine().with_state(|s| s.version),
                error: Some(rate_error()),
                duplicate: false,
            },
        )],
        ClientMsg::Query { id, .. } => vec![ServerMsg::QueryResult {
            id,
            result: None,
            error: Some(rate_error()),
        }],
        _ => Vec::new(),
    }
}

async fn handle(service: &Arc<AppService>, ctx: &ClientCtx, msg: ClientMsg) -> Vec<ServerMsg> {
    match msg {
        ClientMsg::Command { id, cmd } => vec![dispatch(service, ctx, id, cmd).await],
        ClientMsg::Query { id, query } => vec![run_query(service, ctx, id, query).await],
        ClientMsg::Resync => vec![service.snapshot_msg(ctx.role)],
        ClientMsg::Ping { t } => vec![ServerMsg::Pong { t }],
        ClientMsg::Hello { .. } | ClientMsg::Unknown => Vec::new(),
    }
}

async fn dispatch(
    service: &Arc<AppService>,
    ctx: &ClientCtx,
    id: CommandId,
    cmd: Command,
) -> ServerMsg {
    // Dispatch may read the Bible from SQLite: keep it off the async workers.
    let (svc, c, cid) = (service.clone(), ctx.clone(), id.clone());
    let ack = tokio::task::spawn_blocking(move || svc.dispatch(&c, Some(&cid), cmd)).await;
    let ack = ack.unwrap_or_else(|_| Ack {
        ok: false,
        version: service.engine().with_state(|s| s.version),
        error: Some(WireError {
            code: "internal".into(),
            message: "internal error".into(),
        }),
        duplicate: false,
    });
    ServerMsg::ack(id, ack)
}

async fn run_query(
    service: &Arc<AppService>,
    ctx: &ClientCtx,
    id: QueryId,
    q: QueryRequest,
) -> ServerMsg {
    let (svc, c) = (service.clone(), ctx.clone());
    let out = tokio::task::spawn_blocking(move || svc.query(&c, &q)).await;
    match out {
        Ok(Ok(result)) => ServerMsg::QueryResult {
            id,
            result: Some(result),
            error: None,
        },
        Ok(Err(error)) => ServerMsg::QueryResult {
            id,
            result: None,
            error: Some(error),
        },
        Err(_) => ServerMsg::QueryResult {
            id,
            result: None,
            error: Some(WireError {
                code: "internal".into(),
                message: "internal error".into(),
            }),
        },
    }
}
