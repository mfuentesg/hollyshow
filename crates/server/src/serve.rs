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
    let header_timeout = shared.config.header_read_timeout;
    let mut stop = shared.shutdown.subscribe();
    conns.spawn(async move {
        let _permit = permit;
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
    });
}
