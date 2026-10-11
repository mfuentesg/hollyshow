use std::future::Future;
use std::time::Duration;

use crate::error::MediaError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// The body is rejected once it exceeds this many bytes.
    pub max_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub retry_after: Option<Duration>,
    pub body: Vec<u8>,
}

/// Minimal HTTP GET abstraction so tests run with recorded fixtures and no network.
pub trait HttpFetcher: Send + Sync + 'static {
    fn get(
        &self,
        req: &HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, MediaError>> + Send;
}

/// Host suffixes images may be downloaded from (and redirected to): Wikimedia Commons and the
/// Openverse providers. Anything else, in particular IP literals and internal names, is refused.
pub const DOWNLOAD_HOSTS: &[&str] = &[
    "wikimedia.org",
    "openverse.org",
    "flickr.com",
    "staticflickr.com",
    "rawpixel.com",
    "stocksnap.io",
    "nappy.co",
];

/// `host` equals one of `suffixes` or is a subdomain of it.
pub fn host_allowed(host: &str, suffixes: &[String]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    !host.is_empty()
        && suffixes.iter().any(|s| {
            let s = s.trim_start_matches('.').to_ascii_lowercase();
            host == s || host.ends_with(&format!(".{s}"))
        })
}

/// Checks a download URL: https, no credentials, default port, host on the allow-list.
pub fn check_download_url(url: &str, suffixes: &[String]) -> Result<(), MediaError> {
    let bad = |m: &str| Err(MediaError::InvalidDownload(m.into()));
    let Ok(u) = url::Url::parse(url) else {
        return bad("invalid URL");
    };
    if u.scheme() != "https" {
        return bad("only https downloads are allowed");
    }
    if !u.username().is_empty() || u.password().is_some() {
        return bad("credentials in URL are not allowed");
    }
    if u.port().is_some() {
        return bad("custom ports are not allowed");
    }
    match u.host() {
        Some(url::Host::Domain(d)) if host_allowed(d, suffixes) => Ok(()),
        _ => bad("host is not an allowed image source"),
    }
}

/// True for loopback, private, link-local, unique-local, multicast and unspecified addresses.
pub fn is_internal_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_multicast()
                || v.is_broadcast()
                || o[0] == 0
                || (o[0] == 100 && (64..128).contains(&o[1]))
        }
        IpAddr::V6(v) => {
            if let Some(m) = v.to_ipv4_mapped() {
                return is_internal_ip(IpAddr::V4(m));
            }
            let seg = v.segments();
            v.is_loopback()
                || v.is_unspecified()
                || v.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00
                || (seg[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Resolver that refuses names pointing at internal addresses (DNS rebinding, split-horizon
/// names), so a public-looking host cannot reach the LAN.
struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_owned();
            let addrs: Vec<std::net::SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            if addrs.iter().any(|a| is_internal_ip(a.ip())) {
                return Err(format!("{host} resolves to an internal address").into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Parses a `Retry-After` header (delta-seconds form; the HTTP-date form is ignored).
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    let secs: f64 = value.trim().parse().ok()?;
    (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs.min(86_400.0)))
}

/// Production fetcher: reqwest with rustls and an identifiable `User-Agent`.
pub struct ReqwestFetcher {
    client: reqwest::Client,
}

impl ReqwestFetcher {
    pub fn new() -> Result<Self, MediaError> {
        Self::with_user_agent(&crate::user_agent())
    }

    pub fn with_user_agent(ua: &str) -> Result<Self, MediaError> {
        let client = reqwest::Client::builder()
            .user_agent(ua)
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let hosts: Vec<String> = DOWNLOAD_HOSTS.iter().map(|h| (*h).to_owned()).collect();
                if attempt.previous().len() >= 5 {
                    attempt.error("too many redirects")
                } else if check_download_url(attempt.url().as_str(), &hosts).is_ok() {
                    attempt.follow()
                } else {
                    attempt.error("redirect to a host that is not allowed")
                }
            }))
            .dns_resolver(PublicOnlyResolver)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| MediaError::Network(e.to_string()))?;
        Ok(Self { client })
    }
}

impl HttpFetcher for ReqwestFetcher {
    async fn get(&self, req: &HttpRequest) -> Result<HttpResponse, MediaError> {
        let mut builder = self.client.get(&req.url);
        for (k, v) in &req.headers {
            builder = builder.header(k, v);
        }
        let mut resp = builder
            .send()
            .await
            .map_err(|e| MediaError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);
        if resp
            .content_length()
            .is_some_and(|n| n > req.max_bytes as u64)
        {
            return Err(MediaError::InvalidDownload(format!(
                "response larger than {} bytes",
                req.max_bytes
            )));
        }
        let mut body = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| MediaError::Network(e.to_string()))?
        {
            body.extend_from_slice(&chunk);
            if body.len() > req.max_bytes {
                return Err(MediaError::InvalidDownload(format!(
                    "response larger than {} bytes",
                    req.max_bytes
                )));
            }
        }
        Ok(HttpResponse {
            status,
            retry_after,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts() -> Vec<String> {
        DOWNLOAD_HOSTS.iter().map(|h| (*h).to_owned()).collect()
    }

    #[test]
    fn download_urls_are_restricted_to_allowed_https_hosts() {
        let h = hosts();
        for ok in [
            "https://upload.wikimedia.org/wikipedia/commons/a/a9/x.jpg",
            "https://live.staticflickr.com/1/2.jpg",
            "https://api.openverse.org/v1/images/x/thumb/",
        ] {
            assert!(check_download_url(ok, &h).is_ok(), "{ok}");
        }
        for bad in [
            "http://upload.wikimedia.org/x.jpg",
            "https://127.0.0.1/x.jpg",
            "https://192.168.1.10/x.jpg",
            "https://[::1]/x.jpg",
            "https://localhost/x.jpg",
            "https://router.local/x.jpg",
            "https://evilwikimedia.org/x.jpg",
            "https://wikimedia.org.evil.example/x.jpg",
            "https://user:pw@upload.wikimedia.org/x.jpg",
            "https://upload.wikimedia.org:8443/x.jpg",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(check_download_url(bad, &h).is_err(), "{bad}");
        }
    }

    #[test]
    fn internal_addresses_are_recognised() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.0.5",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:10.0.0.1",
        ] {
            assert!(is_internal_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "93.184.216.34", "2606:4700::1111"] {
            assert!(!is_internal_ip(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn parses_retry_after() {
        assert_eq!(parse_retry_after("5"), Some(Duration::from_secs(5)));
        assert_eq!(
            parse_retry_after(" 1.5 "),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after("-3"), None);
    }
}
