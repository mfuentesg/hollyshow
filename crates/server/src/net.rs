//! Network helpers: local interfaces, the private-only listener and the diagnostics report.

use crate::util::is_private_ip;
use axum::extract::connect_info::Connected;
use axum::serve::{IncomingStream, Listener};
use serde::Serialize;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};

/// A listener that silently drops peers outside private ranges (PLAN §5.1: never exposed to the
/// internet, even if the router forwards the port).
pub struct PrivateListener {
    inner: TcpListener,
    allow_public: bool,
}

impl PrivateListener {
    pub fn new(inner: TcpListener, allow_public: bool) -> Self {
        Self {
            inner,
            allow_public,
        }
    }
}

impl Listener for PrivateListener {
    type Io = TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.inner.accept().await {
                Ok((io, addr)) => {
                    if self.allow_public || is_private_ip(addr.ip()) {
                        let _ = io.set_nodelay(true);
                        return (io, addr);
                    }
                    log::warn!("dropped connection from non-private address {}", addr.ip());
                }
                Err(e) => {
                    // EMFILE and friends: back off instead of spinning.
                    log::warn!("accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

/// Remote address of a connection, as extracted by `ConnectInfo<Peer>`.
#[derive(Debug, Clone, Copy)]
pub struct Peer(pub SocketAddr);

impl Connected<IncomingStream<'_, PrivateListener>> for Peer {
    fn connect_info(stream: IncomingStream<'_, PrivateListener>) -> Self {
        Peer(*stream.remote_addr())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub name: String,
    pub ip: IpAddr,
    pub is_private: bool,
    pub is_loopback: bool,
    pub is_link_local: bool,
}

/// Every address of this machine (IPv4 and IPv6), loopback included.
pub fn local_interfaces() -> Vec<Interface> {
    let Ok(list) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    list.into_iter()
        .map(|i| {
            let ip = i.ip();
            Interface {
                name: i.name,
                ip,
                is_private: is_private_ip(ip),
                is_loopback: ip.is_loopback(),
                is_link_local: match ip {
                    IpAddr::V4(v4) => v4.is_link_local(),
                    IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
                },
            }
        })
        .collect()
}

/// Addresses a phone could use: private IPv4, not loopback, not link-local.
pub fn lan_ipv4() -> Vec<IpAddr> {
    local_interfaces()
        .into_iter()
        .filter(|i| i.ip.is_ipv4() && i.is_private && !i.is_loopback && !i.is_link_local)
        .map(|i| i.ip)
        .collect()
}

/// Whether something accepts TCP connections at `ip:port` (the server reaching itself through
/// each interface; a first-use wizard shows the result per address).
pub async fn port_reachable(ip: IpAddr, port: u16) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_millis(500), TcpStream::connect((ip, port))).await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reachability_probe_sees_listening_and_closed_ports() {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(port_reachable(ip, port).await);
        drop(l);
        assert!(!port_reachable(ip, port).await);
    }

    #[test]
    fn lan_addresses_are_never_loopback() {
        assert!(lan_ipv4()
            .iter()
            .all(|ip| !ip.is_loopback() && ip.is_ipv4()));
    }
}
