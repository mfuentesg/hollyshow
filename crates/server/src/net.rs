//! Network helpers: local interfaces and their ranking, the socket budget and reachability probes.

use crate::util::is_private_ip;
use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpStream;

/// Budget of raw TCP connections: a global cap and a cap per client address. A permit is held for
/// the whole life of a connection, so half-open and idle sockets count too; when the budget is
/// spent new sockets are closed at `accept` and the live service keeps its file descriptors.
pub struct SocketLimiter {
    total: usize,
    per_ip: usize,
    state: Arc<Mutex<SocketState>>,
}

#[derive(Default)]
struct SocketState {
    open: usize,
    by_ip: HashMap<IpAddr, usize>,
}

/// Releases its slot on drop.
pub struct SocketPermit {
    ip: IpAddr,
    state: Arc<Mutex<SocketState>>,
}

impl Drop for SocketPermit {
    fn drop(&mut self) {
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        st.open = st.open.saturating_sub(1);
        if let Some(n) = st.by_ip.get_mut(&self.ip) {
            *n -= 1;
            if *n == 0 {
                st.by_ip.remove(&self.ip);
            }
        }
    }
}

impl SocketLimiter {
    pub fn new(total: usize, per_ip: usize) -> Self {
        Self {
            total: total.max(1),
            per_ip: per_ip.max(1),
            state: Arc::default(),
        }
    }

    pub fn try_acquire(&self, ip: IpAddr) -> Option<SocketPermit> {
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if st.open >= self.total || st.by_ip.get(&ip).copied().unwrap_or(0) >= self.per_ip {
            return None;
        }
        st.open += 1;
        *st.by_ip.entry(ip).or_insert(0) += 1;
        Some(SocketPermit {
            ip,
            state: self.state.clone(),
        })
    }

    #[cfg(test)]
    pub fn open(&self) -> usize {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).open
    }
}

/// Remote address of a connection, as extracted by `ConnectInfo<Peer>`.
#[derive(Debug, Clone, Copy)]
pub struct Peer(pub SocketAddr);

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

/// Interface names of virtual adapters (VMs, containers, VPN tunnels, WSL): private IPv4
/// addresses a phone on the Wi-Fi can almost never reach.
pub fn is_virtual_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    const CONTAINS: [&str; 12] = [
        "vethernet",
        "docker",
        "vmnet",
        "vmware",
        "virtualbox",
        "vboxnet",
        "hyper-v",
        "wsl",
        "tailscale",
        "zerotier",
        "podman",
        "virbr",
    ];
    const PREFIXES: [&str; 14] = [
        "br-", "veth", "cni", "flannel", "utun", "tun", "tap", "wg", "zt", "awdl", "llw", "bridge",
        "vnic", "lxc",
    ];
    CONTAINS.iter().any(|p| n.contains(p)) || PREFIXES.iter().any(|p| n.starts_with(p))
}

/// IPv4 address the OS would use to reach the internet, i.e. the interface owning the default
/// route. A UDP `connect` sends nothing; it only makes the kernel pick a source address.
pub fn default_route_ip() -> Option<IpAddr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("192.0.2.1:9").ok()?;
    let ip = sock.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// A LAN address with the interface that carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanAddr {
    pub ip: IpAddr,
    pub name: String,
    pub is_virtual: bool,
}

/// Private, routable-on-LAN IPv4 addresses, best first: real adapters before virtual ones, and
/// within each group the default-route interface first. Pure so the ranking is testable.
pub fn rank_lan(ifaces: Vec<Interface>, default_ip: Option<IpAddr>) -> Vec<LanAddr> {
    let mut v: Vec<LanAddr> = ifaces
        .into_iter()
        .filter(|i| i.ip.is_ipv4() && i.is_private && !i.is_loopback && !i.is_link_local)
        .map(|i| LanAddr {
            ip: i.ip,
            is_virtual: is_virtual_name(&i.name),
            name: i.name,
        })
        .collect();
    v.sort_by_key(|a| (a.is_virtual, Some(a.ip) != default_ip));
    v.dedup_by_key(|a| a.ip);
    v
}

pub fn lan_addrs() -> Vec<LanAddr> {
    rank_lan(local_interfaces(), default_route_ip())
}

/// Addresses a phone could use, best first (see [`rank_lan`]).
pub fn lan_ipv4() -> Vec<IpAddr> {
    lan_addrs().into_iter().map(|a| a.ip).collect()
}

/// Addresses worth advertising over mDNS: the real adapters, or everything when only virtual
/// ones exist.
pub fn mdns_ips(addrs: &[LanAddr]) -> Vec<IpAddr> {
    let real: Vec<IpAddr> = addrs
        .iter()
        .filter(|a| !a.is_virtual)
        .map(|a| a.ip)
        .collect();
    if real.is_empty() {
        addrs.iter().map(|a| a.ip).collect()
    } else {
        real
    }
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
    use tokio::net::TcpListener;

    fn iface(name: &str, ip: [u8; 4]) -> Interface {
        let ip = IpAddr::from(ip);
        Interface {
            name: name.into(),
            ip,
            is_private: is_private_ip(ip),
            is_loopback: ip.is_loopback(),
            is_link_local: false,
        }
    }

    #[test]
    fn virtual_adapters_rank_below_the_default_route_interface() {
        let ifaces = vec![
            iface("vEthernet (WSL)", [172, 24, 0, 1]),
            iface("docker0", [172, 17, 0, 1]),
            iface("Ethernet 2", [10, 0, 5, 7]),
            iface("Wi-Fi", [192, 168, 1, 20]),
            iface("lo", [127, 0, 0, 1]),
            iface("Wi-Fi", [8, 8, 4, 4]),
        ];
        let ranked = rank_lan(ifaces.clone(), Some(IpAddr::from([192, 168, 1, 20])));
        let ips: Vec<_> = ranked.iter().map(|a| a.ip.to_string()).collect();
        assert_eq!(
            ips,
            ["192.168.1.20", "10.0.5.7", "172.24.0.1", "172.17.0.1"]
        );
        // Without a default route the real adapters still beat the virtual ones.
        let ranked = rank_lan(ifaces, None);
        assert!(!ranked[0].is_virtual && !ranked[1].is_virtual);
        assert!(ranked[2].is_virtual && ranked[3].is_virtual);
        let advertised = mdns_ips(&ranked);
        assert_eq!(advertised.len(), 2);
        // Only virtual adapters: advertise them rather than nothing.
        let only_virtual = rank_lan(vec![iface("docker0", [172, 17, 0, 1])], None);
        assert_eq!(mdns_ips(&only_virtual).len(), 1);
    }

    #[test]
    fn virtual_names_cover_the_usual_suspects() {
        for n in [
            "vEthernet (Default Switch)",
            "docker0",
            "br-1a2b",
            "veth12",
            "utun3",
            "tailscale0",
            "vmnet8",
            "VirtualBox Host-Only",
            "virbr0",
            "wg0",
        ] {
            assert!(is_virtual_name(n), "{n}");
        }
        for n in [
            "Wi-Fi", "Ethernet", "en0", "eth0", "wlan0", "wlp3s0", "enp5s0",
        ] {
            assert!(!is_virtual_name(n), "{n}");
        }
    }

    #[test]
    fn socket_budget_is_global_and_per_address_and_released_on_drop() {
        let lim = SocketLimiter::new(3, 2);
        let a = IpAddr::from([10, 0, 0, 1]);
        let b = IpAddr::from([10, 0, 0, 2]);
        let p1 = lim.try_acquire(a).unwrap();
        let _p2 = lim.try_acquire(a).unwrap();
        assert!(lim.try_acquire(a).is_none(), "per-address cap");
        let _p3 = lim.try_acquire(b).unwrap();
        assert!(lim.try_acquire(b).is_none(), "global cap");
        drop(p1);
        assert_eq!(lim.open(), 2);
        assert!(lim.try_acquire(b).is_some());
    }

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
