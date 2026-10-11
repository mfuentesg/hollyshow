//! Small helpers: secrets, hashing, clocks and address classification.

use sha2::{Digest, Sha256};
use std::net::{IpAddr, Ipv6Addr};
use std::time::{SystemTime, UNIX_EPOCH};

/// `n` random bytes from the operating system, hex encoded (so `2 * n` characters).
///
/// If the OS random source is unavailable the server must not hand out guessable secrets, so this
/// returns `None` and callers refuse to pair.
pub fn random_hex(n: usize) -> Option<String> {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).ok()?;
    Some(hex(&buf))
}

/// Uniform random number in `0..bound` by rejection sampling (no modulo bias).
pub fn random_below(bound: u32) -> Option<u32> {
    debug_assert!(bound > 0);
    let zone = u32::MAX - (u32::MAX % bound);
    loop {
        let mut b = [0u8; 4];
        getrandom::fill(&mut b).ok()?;
        let v = u32::from_le_bytes(b);
        if v < zone {
            return Some(v % bound);
        }
    }
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// SHA-256 of a session token, hex encoded. Tokens carry 256 bits of entropy, so a fast hash is
/// enough to make a leaked settings file useless for impersonation.
pub fn hash_token(token: &str) -> String {
    hex(Sha256::digest(token.as_bytes()).as_slice())
}

/// Comparison whose time does not depend on where the inputs differ.
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Whether `ip` belongs to a private network: loopback, RFC 1918, link-local, carrier-grade NAT
/// (Tailscale and friends) and IPv6 unique-local / link-local. The server never talks to anything
/// else unless explicitly configured to.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || (o[0] == 100 && (64..128).contains(&o[1]))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = to_ipv4(&v6) {
                return is_private_ip(IpAddr::V4(v4));
            }
            let seg = v6.segments();
            v6.is_loopback() || (seg[0] & 0xfe00) == 0xfc00 || (seg[0] & 0xffc0) == 0xfe80
        }
    }
}

fn to_ipv4(v6: &Ipv6Addr) -> Option<std::net::Ipv4Addr> {
    v6.to_ipv4_mapped()
}

/// Keeps device names short and printable: they end up in the operator's device list.
pub fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .take(48)
        .collect::<String>()
        .trim()
        .to_owned();
    if cleaned.is_empty() {
        "Dispositivo".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_ranges() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.9",
            "192.168.1.20",
            "169.254.3.4",
            "100.100.1.1",
            "::1",
            "fd12::1",
            "fe80::1",
            "::ffff:192.168.0.2",
        ] {
            assert!(is_private_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "8.8.8.8",
            "172.32.0.1",
            "100.128.0.1",
            "2001:db8::1",
            "::ffff:8.8.8.8",
        ] {
            assert!(!is_private_ip(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn hashing_and_comparison() {
        assert_eq!(hash_token("abc").len(), 64);
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), hash_token("abd"));
        assert!(ct_eq("123456", "123456"));
        assert!(!ct_eq("123456", "123457"));
        assert!(!ct_eq("1234", "123456"));
    }

    #[test]
    fn randomness_has_the_requested_shape() {
        let a = random_hex(16).unwrap();
        assert_eq!(a.len(), 32);
        assert_ne!(a, random_hex(16).unwrap());
        for _ in 0..200 {
            assert!(random_below(1_000_000).unwrap() < 1_000_000);
        }
    }

    #[test]
    fn names_are_sanitised() {
        assert_eq!(sanitize_name("  Pastor\u{7} Juan "), "Pastor Juan");
        assert_eq!(sanitize_name("\n\t"), "Dispositivo");
        assert_eq!(sanitize_name(&"x".repeat(200)).chars().count(), 48);
    }
}
