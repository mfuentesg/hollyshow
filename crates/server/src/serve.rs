//! The accept loop. `axum::serve` offers no connection budget and no header timeout, so the
//! server drives hyper itself:
//!
//! * a [`SocketLimiter`](crate::net::SocketLimiter) permit is taken at `accept` and held until the
//!   connection ends (global and per-address caps), so a flood of idle or half-open sockets
//!   cannot reach the file-descriptor limit;
//! * `header_read_timeout` closes clients that dribble a request (slowloris);
//! * peers outside private ranges are dropped before any byte is read.

use crate::net::Peer;
use crate::shared::Shared;
use crate::util::is_private_ip;
use axum::extract::ConnectInfo;
use axum::{Extension, Router};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tower::Service;

/// Serves `router` on `listener` until the shared shutdown flag flips, then waits for the open
/// connections to finish (upgraded WebSockets close themselves on the same flag).
pub async fn serve(listener: TcpListener, router: Router, shared: Arc<Shared>) {
    let mut stop = shared.shutdown.subscribe();
    let mut conns = JoinSet::new();
    loop {
        let accepted = tokio::select! {
            r = listener.accept() => r,
            _ = stop.wait_for(|down| *down) => break,
        };
        match accepted {
            Ok((io, addr)) => admit(&shared, &router, &mut conns, io, addr),
            Err(e) => {
                // EMFILE and friends: back off instead of spinning.
                log::warn!("accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        while conns.try_join_next().is_some() {}
    }
    drop(listener);
    while conns.join_next().await.is_some() {}
}

fn admit(
    shared: &Arc<Shared>,
    router: &Router,
    conns: &mut JoinSet<()>,
    io: TcpStream,
    addr: SocketAddr,
) {
    if !shared.config.allow_public_peers && !is_private_ip(addr.ip()) {
        log::warn!("dropped connection from non-private address {}", addr.ip());
        return;
    }
    let Some(permit) = shared.sockets.try_acquire(addr.ip()) else {
        log::debug!("socket budget spent: refused {addr}");
        return;
    };
    let _ = io.set_nodelay(true);
    let service = router.clone().layer(Extension(ConnectInfo(Peer(addr))));
    let redirect = redirect_router().layer(Extension(ConnectInfo(Peer(addr))));
    let header_timeout = shared.config.header_read_timeout;
    let tls = shared.tls_identity();
    let mut stop = shared.shutdown.subscribe();
    conns.spawn(async move {
        let _permit = permit;
        let Some(tls) = tls else {
            serve_connection(io, service, header_timeout, &mut stop, addr).await;
            return;
        };
        // One port speaks both: a TLS ClientHello starts with 0x16, anything else is plain HTTP
        // and is sent to the HTTPS address.
        let mut first = [0u8; 1];
        match tokio::time::timeout(header_timeout, io.peek(&mut first)).await {
            Ok(Ok(1)) => {}
            _ => return,
        }
        if first[0] != 0x16 {
            serve_connection(io, redirect, header_timeout, &mut stop, addr).await;
            return;
        }
        let acceptor = tokio_rustls::TlsAcceptor::from(tls.config.clone());
        match tokio::time::timeout(header_timeout, acceptor.accept(io)).await {
            Ok(Ok(stream)) => {
                serve_connection(stream, service, header_timeout, &mut stop, addr).await
            }
            Ok(Err(e)) => log::debug!("TLS handshake with {addr} failed: {e}"),
            Err(_) => log::debug!("TLS handshake with {addr} timed out"),
        }
    });
}

/// Plain-HTTP answer while HTTPS is on: a temporary redirect to the same host and path over
/// HTTPS (same port). Temporary, so a phone that cached it can still reach the app once HTTPS
/// is turned off again.
fn redirect_router() -> Router {
    async fn redirect(
        headers: axum::http::HeaderMap,
        uri: axum::http::Uri,
    ) -> axum::response::Response {
        use axum::http::{header, StatusCode};
        use axum::response::IntoResponse;
        let host = headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .filter(|h| {
                !h.is_empty()
                    && h.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b))
            });
        let Some(host) = host else {
            return (StatusCode::BAD_REQUEST, "bad host").into_response();
        };
        let path = uri.path_and_query().map_or("/", |p| p.as_str());
        let target = format!("https://{host}{path}");
        (
            StatusCode::TEMPORARY_REDIRECT,
            [(header::LOCATION, target)],
            "",
        )
            .into_response()
    }
    Router::new().fallback(redirect)
}

async fn serve_connection<IO>(
    io: IO,
    service: Router,
    header_timeout: Duration,
    stop: &mut tokio::sync::watch::Receiver<bool>,
    addr: SocketAddr,
) where
    IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let hyper_service = service_fn(move |req: hyper::Request<Incoming>| {
        let mut svc = service.clone();
        async move { svc.call(req).await }
    });
    let mut conn = pin!(hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(header_timeout)
        .serve_connection(TokioIo::new(io), hyper_service)
        .with_upgrades());
    let mut draining = false;
    loop {
        tokio::select! {
            r = conn.as_mut() => {
                if let Err(e) = r {
                    log::trace!("connection {addr} ended: {e}");
                }
                return;
            }
            _ = stop.wait_for(|down| *down), if !draining => {
                draining = true;
                conn.as_mut().graceful_shutdown();
            }
        }
    }
}
