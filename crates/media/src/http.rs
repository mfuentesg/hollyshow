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
