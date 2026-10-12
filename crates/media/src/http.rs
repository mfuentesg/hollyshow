use std::future::Future;
use std::time::Duration;

use crate::error::MediaError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// The body is rejected once it exceeds this many bytes.
    pub max_bytes: usize,
    /// When non-empty, a successful response must carry one of these `Content-Type`s.
    pub accept_types: &'static [&'static str],
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

/// Content types accepted for image downloads.
pub const IMAGE_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];

/// Redirect hops followed per request (each hop is re-validated).
pub const MAX_REDIRECTS: usize = 3;

/// Host names that never leave the local machine or LAN, whatever DNS says.
fn is_internal_name(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    !h.contains('.')
        || h == "localhost"
        || [
            ".localhost",
            ".local",
            ".internal",
            ".localdomain",
            ".lan",
            ".home.arpa",
        ]
        .iter()
        .any(|s| h.ends_with(s))
}

/// Single check shared by the initial request and every redirect hop: https, no credentials,
/// default port and a host that is not an internal name or IP literal. Names are additionally
/// checked at connect time by [`PublicOnlyResolver`]. `trusted` is an exact origin exempted
/// from all of this (test servers only).
fn check_url(url: &str, trusted: Option<&(String, u16)>) -> Result<(), MediaError> {
    let bad = |m: &str| Err(MediaError::InvalidDownload(m.into()));
    let Ok(u) = url::Url::parse(url) else {
        return bad("invalid URL");
    };
    if let (Some(t), Some(h), Some(p)) = (trusted, u.host_str(), u.port_or_known_default()) {
        if t.0 == h && t.1 == p {
            return Ok(());
        }
    }
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
        Some(url::Host::Domain(d)) if !is_internal_name(d) => Ok(()),
        Some(url::Host::Ipv4(ip)) if !is_internal_ip(ip.into()) => Ok(()),
        Some(url::Host::Ipv6(ip)) if !is_internal_ip(ip.into()) => Ok(()),
        _ => bad("host is not a public address"),
    }
}

/// Checks a download URL (see [`check_url`]). Any public https host is acceptable.
pub fn check_download_url(url: &str) -> Result<(), MediaError> {
    check_url(url, None)
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

/// Resolves `host`, failing when any answer is an internal address.
async fn resolve_public(host: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, 0)).await?.collect();
    if addrs.is_empty() || addrs.iter().any(|a| is_internal_ip(a.ip())) {
        return Err(std::io::Error::other(format!(
            "{host} does not resolve only to public addresses"
        )));
    }
    Ok(addrs)
}

/// Resolver that refuses names pointing at internal addresses (DNS rebinding, split-horizon
/// names), so a public-looking host cannot reach the LAN.
struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let addrs = resolve_public(name.as_str()).await?;
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
    /// Exact origin exempt from the URL policy (tests only).
    trusted: Option<(String, u16)>,
}

impl ReqwestFetcher {
    pub fn new() -> Result<Self, MediaError> {
        Self::with_user_agent(&crate::user_agent())
    }

    pub fn with_user_agent(ua: &str) -> Result<Self, MediaError> {
        Self::build(ua, None)
    }

    fn build(ua: &str, trusted: Option<(String, u16)>) -> Result<Self, MediaError> {
        let hop_trust = trusted.clone();
        let client = reqwest::Client::builder()
            .user_agent(ua)
            .timeout(Duration::from_secs(90))
            .read_timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() > MAX_REDIRECTS {
                    attempt.error("too many redirects")
                } else if let Err(e) = check_url(attempt.url().as_str(), hop_trust.as_ref()) {
                    attempt.error(e.to_string())
                } else {
                    attempt.follow()
                }
            }))
            .dns_resolver(PublicOnlyResolver)
            .build()
            .map_err(|e| MediaError::Network(e.to_string()))?;
        Ok(Self { client, trusted })
    }
}

impl HttpFetcher for ReqwestFetcher {
    async fn get(&self, req: &HttpRequest) -> Result<HttpResponse, MediaError> {
        check_url(&req.url, self.trusted.as_ref())?;
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
        if (200..300).contains(&status) && !req.accept_types.is_empty() {
            let ct = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(';').next())
                .map(|v| v.trim().to_ascii_lowercase())
                .unwrap_or_default();
            if !req.accept_types.contains(&ct.as_str()) {
                return Err(MediaError::InvalidDownload(format!(
                    "unexpected content type {ct:?}"
                )));
            }
        }
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[test]
    fn download_urls_must_be_public_https() {
        for ok in [
            "https://upload.wikimedia.org/wikipedia/commons/a/a9/x.jpg",
            "https://live.staticflickr.com/1/2.jpg",
            "https://images.example.net/some/photo.png?w=1",
            "https://93.184.216.34/x.jpg",
        ] {
            assert!(check_download_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://upload.wikimedia.org/x.jpg",
            "https://127.0.0.1/x.jpg",
            "https://192.168.1.10/x.jpg",
            "https://0x7f.1/x.jpg",
            "https://[::1]/x.jpg",
            "https://[::ffff:10.0.0.1]/x.jpg",
            "https://localhost/x.jpg",
            "https://nas/x.jpg",
            "https://router.local/x.jpg",
            "https://db.internal/x.jpg",
            "https://user:pw@upload.wikimedia.org/x.jpg",
            "https://upload.wikimedia.org:8443/x.jpg",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(check_download_url(bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn names_resolving_to_loopback_are_refused() {
        assert!(resolve_public("localhost").await.is_err());
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

    /// Minimal HTTP/1.1 server: every connection gets the next scripted raw response.
    struct TestServer {
        port: u16,
        hits: Arc<AtomicUsize>,
    }

    async fn serve(responses: Vec<Vec<u8>>) -> TestServer {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let n = h.fetch_add(1, Ordering::SeqCst);
                let resp = responses[n.min(responses.len() - 1)].clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    let _ = sock.write_all(&resp).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        TestServer { port, hits }
    }

    fn raw(status: &str, headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
        for (k, v) in headers {
            out.push_str(&format!("{k}: {v}\r\n"));
        }
        out.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
        let mut v = out.into_bytes();
        v.extend_from_slice(body);
        v
    }

    fn image_ok(body: &[u8]) -> Vec<u8> {
        raw("200 OK", &[("Content-Type", "image/png".into())], body)
    }

    fn redirect(to: &str) -> Vec<u8> {
        raw("302 Found", &[("Location", to.into())], b"")
    }

    fn fetcher_for(srv: &TestServer) -> ReqwestFetcher {
        ReqwestFetcher::build("test", Some(("127.0.0.1".into(), srv.port))).unwrap()
    }

    fn req(srv: &TestServer, max_bytes: usize) -> HttpRequest {
        HttpRequest {
            url: format!("http://127.0.0.1:{}/a.png", srv.port),
            headers: Vec::new(),
            max_bytes,
            accept_types: IMAGE_TYPES,
        }
    }

    #[tokio::test]
    async fn downloads_an_image_from_a_trusted_origin() {
        let srv = serve(vec![image_ok(b"PNGDATA")]).await;
        let r = fetcher_for(&srv).get(&req(&srv, 1024)).await.unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, &b"PNGDATA"[..]));
    }

    #[tokio::test]
    async fn refuses_private_ip_and_plain_http_without_connecting() {
        let srv = serve(vec![image_ok(b"x")]).await;
        // Same server, but no trusted origin: loopback over http is refused outright.
        let strict = ReqwestFetcher::with_user_agent("test").unwrap();
        for url in [
            format!("http://127.0.0.1:{}/a.png", srv.port),
            format!("https://127.0.0.1:{}/a.png", srv.port),
            "https://10.0.0.5/a.png".to_owned(),
            "http://example.com/a.png".to_owned(),
        ] {
            let mut r = req(&srv, 1024);
            r.url = url.clone();
            assert!(
                matches!(strict.get(&r).await, Err(MediaError::InvalidDownload(_))),
                "{url}"
            );
        }
        assert_eq!(srv.hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn refuses_wrong_content_type() {
        for ct in ["text/html", "image/svg+xml", "application/octet-stream"] {
            let srv = serve(vec![raw("200 OK", &[("Content-Type", ct.into())], b"x")]).await;
            let e = fetcher_for(&srv).get(&req(&srv, 1024)).await.unwrap_err();
            assert!(matches!(e, MediaError::InvalidDownload(_)), "{ct}");
        }
        let srv = serve(vec![raw("200 OK", &[], b"x")]).await;
        assert!(fetcher_for(&srv).get(&req(&srv, 1024)).await.is_err());
        let srv = serve(vec![raw(
            "200 OK",
            &[("Content-Type", "IMAGE/JPEG; charset=binary".into())],
            b"x",
        )])
        .await;
        assert!(fetcher_for(&srv).get(&req(&srv, 1024)).await.is_ok());
    }

    #[tokio::test]
    async fn refuses_oversized_bodies_declared_or_streamed() {
        let body = vec![7u8; 4096];
        let srv = serve(vec![image_ok(&body)]).await;
        let e = fetcher_for(&srv).get(&req(&srv, 1000)).await.unwrap_err();
        assert!(matches!(e, MediaError::InvalidDownload(_)));
        // Chunked: no Content-Length, so the cap must be enforced while streaming.
        let mut chunked =
            b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                .to_vec();
        for _ in 0..8 {
            chunked.extend_from_slice(b"200\r\n");
            chunked.extend_from_slice(&[1u8; 512]);
            chunked.extend_from_slice(b"\r\n");
        }
        chunked.extend_from_slice(b"0\r\n\r\n");
        let srv = serve(vec![chunked]).await;
        let e = fetcher_for(&srv).get(&req(&srv, 1000)).await.unwrap_err();
        assert!(matches!(e, MediaError::InvalidDownload(_)));
    }

    #[tokio::test]
    async fn refuses_redirects_to_private_or_insecure_targets() {
        for target in [
            "https://10.0.0.5/a.png",
            "https://localhost/a.png",
            "https://[::1]/a.png",
            "http://example.com/a.png",
            "https://169.254.169.254/latest/meta-data",
        ] {
            let srv = serve(vec![redirect(target)]).await;
            let e = fetcher_for(&srv).get(&req(&srv, 1024)).await.unwrap_err();
            assert!(matches!(e, MediaError::Network(_)), "{target}: {e:?}");
            assert_eq!(srv.hits.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn follows_at_most_three_redirects() {
        let ok_after = |n: usize| {
            let mut v = vec![redirect("/b.png"); n];
            v.push(image_ok(b"done"));
            v
        };
        // The scripted target is a relative URL, so every hop stays on the trusted origin.
        let srv = serve(ok_after(3)).await;
        assert!(fetcher_for(&srv).get(&req(&srv, 1024)).await.is_ok());
        let srv = serve(ok_after(4)).await;
        assert!(fetcher_for(&srv).get(&req(&srv, 1024)).await.is_err());
    }
}
