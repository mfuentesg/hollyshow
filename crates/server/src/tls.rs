//! Optional HTTPS: a self-signed certificate generated on first use and kept in the app data
//! directory.
//!
//! The certificate lists `hollyshow.local`, `localhost` and every LAN address of the machine as
//! subject alternative names. It is regenerated when an address it does not cover appears (new
//! Wi-Fi, DHCP change) or when it is close to the end of its life; otherwise the same certificate
//! is reused so a phone that accepted it once keeps working. The SHA-256 fingerprint of the DER
//! certificate is what the operator compares with the one the phone's browser shows.

use crate::util::hex;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const CERT_FILE: &str = "cert.pem";
const KEY_FILE: &str = "key.pem";
const META_FILE: &str = "sans.txt";
/// Validity of a generated certificate (kept under the 825 days mobile browsers tolerate).
const VALIDITY_DAYS: i64 = 800;
/// Renewed this long before it expires.
const RENEW_BEFORE_DAYS: u64 = 60;
const DAY_SECS: u64 = 24 * 60 * 60;

/// A loaded certificate, ready to build a TLS acceptor from.
pub struct TlsIdentity {
    pub config: Arc<rustls::ServerConfig>,
    /// `AB:CD:...` SHA-256 of the DER certificate.
    pub fingerprint: String,
    /// The names and addresses the certificate covers.
    #[cfg_attr(not(test), allow(dead_code))]
    pub sans: BTreeSet<String>,
}

/// Names the certificate must cover for `ips` (the machine's LAN addresses).
pub fn wanted_sans(ips: &[IpAddr]) -> BTreeSet<String> {
    let mut sans: BTreeSet<String> = ips.iter().map(ToString::to_string).collect();
    sans.insert(crate::mdns::HOSTNAME.to_string());
    sans.insert("localhost".into());
    sans.insert("127.0.0.1".into());
    sans
}

/// Formats a digest as `AB:CD:EF`.
pub fn format_fingerprint(digest: &[u8]) -> String {
    let h = hex(digest).to_ascii_uppercase();
    h.as_bytes()
        .chunks(2)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(":")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn io_err(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

/// Metadata kept next to the certificate: creation time, then one SAN per line.
fn parse_meta(text: &str) -> Option<(u64, BTreeSet<String>)> {
    let mut lines = text.lines();
    let created = lines
        .next()?
        .strip_prefix("created=")?
        .trim()
        .parse()
        .ok()?;
    Some((
        created,
        lines
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect(),
    ))
}

fn render_meta(created: u64, sans: &BTreeSet<String>) -> String {
    let mut s = format!("created={created}\n");
    for san in sans {
        s.push_str(san);
        s.push('\n');
    }
    s
}

/// Reuses the certificate in `dir` when it still covers `wanted` and is not about to expire;
/// otherwise generates a fresh one. Blocking (disk and key generation).
pub fn load_or_create(dir: &Path, wanted: &BTreeSet<String>) -> io::Result<TlsIdentity> {
    fs::create_dir_all(dir)?;
    if let Some(id) = try_load(dir, wanted) {
        return Ok(id);
    }
    generate(dir, wanted)
}

fn try_load(dir: &Path, wanted: &BTreeSet<String>) -> Option<TlsIdentity> {
    let (created, sans) = parse_meta(&fs::read_to_string(dir.join(META_FILE)).ok()?)?;
    let age = now_secs().saturating_sub(created);
    if !wanted.is_subset(&sans)
        || age + RENEW_BEFORE_DAYS * DAY_SECS > VALIDITY_DAYS as u64 * DAY_SECS
    {
        return None;
    }
    let cert_pem = fs::read(dir.join(CERT_FILE)).ok()?;
    let key_pem = fs::read(dir.join(KEY_FILE)).ok()?;
    build(&cert_pem, &key_pem, sans).ok()
}

fn generate(dir: &Path, sans: &BTreeSet<String>) -> io::Result<TlsIdentity> {
    let key = KeyPair::generate().map_err(io_err)?;
    let mut params =
        CertificateParams::new(sans.iter().cloned().collect::<Vec<_>>()).map_err(io_err)?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "HollyShow");
    dn.push(DnType::OrganizationName, "HollyShow (local)");
    params.distinguished_name = dn;
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(VALIDITY_DAYS);
    let cert = params.self_signed(&key).map_err(io_err)?;
    let (cert_pem, key_pem) = (cert.pem(), key.serialize_pem());

    write_private(&dir.join(KEY_FILE), key_pem.as_bytes())?;
    fs::write(dir.join(CERT_FILE), cert_pem.as_bytes())?;
    fs::write(dir.join(META_FILE), render_meta(now_secs(), sans))?;
    build(cert_pem.as_bytes(), key_pem.as_bytes(), sans.clone())
}

/// Writes a secret readable by the owner only.
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data)
    }
    #[cfg(not(unix))]
    {
        fs::write(path, data)
    }
}

fn build(cert_pem: &[u8], key_pem: &[u8], sans: BTreeSet<String>) -> io::Result<TlsIdentity> {
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem)
        .collect::<Result<_, _>>()
        .map_err(io_err)?;
    let leaf = chain
        .first()
        .ok_or_else(|| io_err("certificate file is empty"))?;
    let fingerprint = format_fingerprint(Sha256::digest(leaf.as_ref()).as_slice());
    let key = PrivateKeyDer::from_pem_slice(key_pem).map_err(io_err)?;
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(io_err)?
    .with_no_client_auth()
    .with_single_cert(chain, key)
    .map_err(io_err)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsIdentity {
        config: Arc::new(config),
        fingerprint,
        sans,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
    fn ips(list: &[&str]) -> Vec<IpAddr> {
        list.iter().map(|s| s.parse().unwrap()).collect()
    }

    #[test]
    fn sans_cover_the_mdns_name_and_every_lan_address() {
        let s = wanted_sans(&ips(&["192.168.1.20", "10.0.0.5"]));
        for want in ["hollyshow.local", "192.168.1.20", "10.0.0.5", "127.0.0.1"] {
            assert!(s.contains(want), "{want}");
        }
    }

    #[test]
    fn fingerprint_is_colon_separated_uppercase_sha256() {
        let f = format_fingerprint(&[0xab, 0x01, 0xff]);
        assert_eq!(f, "AB:01:FF");
        let d = dir();
        let id = load_or_create(d.path(), &wanted_sans(&[])).unwrap();
        assert_eq!(id.fingerprint.len(), 32 * 3 - 1);
        assert!(id
            .fingerprint
            .chars()
            .all(|c| c == ':' || c.is_ascii_digit() || ('A'..='F').contains(&c)));
    }

    #[test]
    fn the_certificate_is_persisted_and_reused() {
        let d = dir();
        let sans = wanted_sans(&ips(&["192.168.1.20"]));
        let a = load_or_create(d.path(), &sans).unwrap();
        let b = load_or_create(d.path(), &sans).unwrap();
        assert_eq!(a.fingerprint, b.fingerprint);
        // A subset of the covered addresses (an adapter went away) keeps the same certificate.
        let fewer = wanted_sans(&[]);
        assert_eq!(
            load_or_create(d.path(), &fewer).unwrap().fingerprint,
            a.fingerprint
        );
    }

    #[test]
    fn a_new_address_regenerates_the_certificate() {
        let d = dir();
        let a = load_or_create(d.path(), &wanted_sans(&ips(&["192.168.1.20"]))).unwrap();
        let b =
            load_or_create(d.path(), &wanted_sans(&ips(&["192.168.1.20", "10.1.1.9"]))).unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
        assert!(b.sans.contains("10.1.1.9"));
    }

    #[test]
    fn a_corrupt_or_expiring_certificate_is_replaced() {
        let d = dir();
        let sans = wanted_sans(&[]);
        let a = load_or_create(d.path(), &sans).unwrap();
        fs::write(d.path().join(CERT_FILE), b"garbage").unwrap();
        let b = load_or_create(d.path(), &sans).unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
        // Pretend it was created long ago.
        fs::write(d.path().join(META_FILE), render_meta(1, &sans)).unwrap();
        let c = load_or_create(d.path(), &sans).unwrap();
        assert_ne!(b.fingerprint, c.fingerprint);
    }

    #[cfg(unix)]
    #[test]
    fn the_private_key_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir();
        load_or_create(d.path(), &wanted_sans(&[])).unwrap();
        let mode = fs::metadata(d.path().join(KEY_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
}
