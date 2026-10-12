use std::sync::RwLock;
use std::time::Duration;

use crate::cache::TtlCache;
use crate::error::MediaError;
use crate::http::{HttpFetcher, HttpRequest, HttpResponse};
use crate::keywords::translate_query;
use crate::license::LicensePolicy;
use crate::types::{
    ImageSource, MediaResult, SearchPage, SearchRequest, SearchResponse, SourceFailure,
    SourceFilter,
};
use crate::{commons, openverse};

/// Tunables of the client. `Default` is the production configuration.
#[derive(Debug, Clone)]
pub struct MediaConfig {
    pub openverse_base: String,
    pub commons_base: String,
    /// Optional Openverse bearer token (settings). Only ever sent to the Openverse API host.
    pub openverse_token: Option<String>,
    /// Minimum image width in pixels (PLAN: 1920).
    pub min_width: u32,
    pub page_size: u32,
    pub cache_ttl: Duration,
    pub cache_entries: usize,
    /// Retries after HTTP 429/503 before giving up.
    pub max_retries: u32,
    /// Longest wait the client is willing to sit through; a larger `Retry-After` is surfaced to the UI instead.
    pub max_wait: Duration,
    /// Base delay of the exponential backoff when the server sends no `Retry-After`.
    pub base_backoff: Duration,
    pub max_download_bytes: usize,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            openverse_base: openverse::DEFAULT_BASE.to_string(),
            commons_base: commons::DEFAULT_BASE.to_string(),
            openverse_token: None,
            min_width: 1920,
            page_size: 20,
            cache_ttl: Duration::from_secs(15 * 60),
            cache_entries: 64,
            max_retries: 3,
            max_wait: Duration::from_secs(20),
            base_backoff: Duration::from_secs(1),
            max_download_bytes: 40 * 1024 * 1024,
        }
    }
}

/// Searches both providers with caching, rate limit backoff and license filtering.
pub struct MediaClient<F: HttpFetcher> {
    fetcher: F,
    config: MediaConfig,
    token: RwLock<Option<String>>,
    cache: TtlCache<SearchPage>,
}

impl<F: HttpFetcher> MediaClient<F> {
    pub fn new(fetcher: F, config: MediaConfig) -> Self {
        let cache = TtlCache::new(config.cache_ttl, config.cache_entries);
        let token = RwLock::new(
            config
                .openverse_token
                .clone()
                .filter(|t| !t.trim().is_empty()),
        );
        Self {
            fetcher,
            config,
            token,
            cache,
        }
    }

    pub fn config(&self) -> &MediaConfig {
        &self.config
    }

    /// Updates the Openverse token (from settings). Empty clears it.
    pub fn set_openverse_token(&self, token: Option<String>) {
        *self.token.write().unwrap_or_else(|e| e.into_inner()) = token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Performs a search: translates the query, queries the selected sources (concurrently for
    /// `All`) and merges the pages. A failing source is reported in `failures`, the other one still
    /// answers (Commons is the fallback for Openverse and vice versa).
    pub async fn search(&self, req: &SearchRequest) -> SearchResponse {
        let translated = translate_query(&req.query);
        let page = req.page.max(1);
        let mut out = SearchResponse {
            page,
            translated_query: translated.clone(),
            ..Default::default()
        };
        if translated.is_empty() {
            return out;
        }
        let policy = req.policy();
        let nc = req.allow_noncommercial;
        let (ov, cm) = match req.source {
            SourceFilter::Openverse => (
                Some(
                    self.search_source(ImageSource::Openverse, &translated, page, &policy, nc)
                        .await,
                ),
                None,
            ),
            SourceFilter::Commons => (
                None,
                Some(
                    self.search_source(ImageSource::Commons, &translated, page, &policy, nc)
                        .await,
                ),
            ),
            SourceFilter::All => {
                let (a, b) = tokio::join!(
                    self.search_source(ImageSource::Openverse, &translated, page, &policy, nc),
                    self.search_source(ImageSource::Commons, &translated, page, &policy, nc)
                );
                (Some(a), Some(b))
            }
        };
        let mut lists: Vec<Vec<crate::types::MediaResult>> = Vec::new();
        for (source, res) in [(ImageSource::Openverse, ov), (ImageSource::Commons, cm)] {
            match res {
                Some(Ok(p)) => {
                    out.has_more |= p.has_more;
                    lists.push(p.results);
                }
                Some(Err(e)) => out.failures.push(SourceFailure {
                    source,
                    retry_after_secs: match &e {
                        MediaError::RateLimited { retry_after } => {
                            Some(retry_after.as_secs().max(1))
                        }
                        _ => None,
                    },
                    message: e.to_string(),
                }),
                None => {}
            }
        }
        out.results = interleave(lists);
        out
    }

    /// One provider, one page. Cached for `cache_ttl`.
    pub async fn search_source(
        &self,
        source: ImageSource,
        query_en: &str,
        page: u32,
        policy: &LicensePolicy,
        noncommercial: bool,
    ) -> Result<SearchPage, MediaError> {
        let key = format!(
            "{}|{}|{}|{}",
            source.as_str(),
            query_en,
            page,
            noncommercial
        );
        if let Some(hit) = self.cache.get(&key) {
            return Ok(hit);
        }
        let (url, headers, name) = match source {
            ImageSource::Openverse => {
                let mut h = vec![("Accept".to_string(), "application/json".to_string())];
                if let Some(t) = self
                    .token
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                {
                    h.push(("Authorization".to_string(), format!("Bearer {t}")));
                }
                (
                    openverse::build_url(
                        &self.config.openverse_base,
                        query_en,
                        page,
                        self.config.page_size,
                    )?,
                    h,
                    "Openverse",
                )
            }
            ImageSource::Commons => (
                commons::build_url(
                    &self.config.commons_base,
                    query_en,
                    page,
                    self.config.page_size,
                    self.config.min_width,
                )?,
                vec![("Accept".to_string(), "application/json".to_string())],
                "Wikimedia Commons",
            ),
        };
        let resp = self
            .get_with_backoff(
                HttpRequest {
                    url,
                    headers,
                    max_bytes: 4 * 1024 * 1024,
                    accept_types: &[],
                },
                name,
            )
            .await?;
        let parsed = match source {
            ImageSource::Openverse => {
                openverse::parse(&resp.body, policy, self.config.min_width, page)?
            }
            ImageSource::Commons => {
                commons::parse(&resp.body, policy, self.config.min_width, page)?
            }
        };
        self.cache.put(key, parsed.clone());
        Ok(parsed)
    }

    /// Downloads an image file (no credentials are ever attached). Any public https host is
    /// accepted; the fetcher re-validates every redirect hop, the content type and the size.
    pub async fn fetch_image(&self, url: &str) -> Result<Vec<u8>, MediaError> {
        crate::http::check_download_url(url)?;
        let req = HttpRequest {
            url: url.to_string(),
            headers: Vec::new(),
            max_bytes: self.config.max_download_bytes,
            accept_types: crate::http::IMAGE_TYPES,
        };
        Ok(self.get_with_backoff(req, "image host").await?.body)
    }

    /// Downloads the image of `result`. When the original URL fails and the image comes from
    /// Openverse, falls back to Openverse's own proxy for that id and then to its thumbnail.
    /// The first error is returned when every candidate fails.
    pub async fn fetch_result_image(&self, result: &MediaResult) -> Result<Vec<u8>, MediaError> {
        let first = match self.fetch_image(&result.full_url).await {
            Ok(b) => return Ok(b),
            Err(e) => e,
        };
        let mut fallbacks = Vec::new();
        if result.source == ImageSource::Openverse {
            let id = crate::types::sanitize_id(&result.id);
            if !id.is_empty() {
                let base = self.config.openverse_base.trim_end_matches('/');
                fallbacks.push(format!("{base}/{id}/thumb/"));
            }
            fallbacks.push(result.thumb_url.clone());
        }
        for url in fallbacks.into_iter().filter(|u| *u != result.full_url) {
            if let Ok(bytes) = self.fetch_image(&url).await {
                return Ok(bytes);
            }
        }
        Err(first)
    }

    /// GET with HTTP 429/503 handling: waits `Retry-After` (or exponential backoff) and retries up
    /// to `max_retries`. A wait longer than `max_wait` is returned as [`MediaError::RateLimited`].
    async fn get_with_backoff(
        &self,
        req: HttpRequest,
        source_name: &'static str,
    ) -> Result<HttpResponse, MediaError> {
        let mut attempt = 0u32;
        loop {
            let resp = self.fetcher.get(&req).await?;
            match resp.status {
                200..=299 => return Ok(resp),
                429 | 503 => {
                    let wait = resp.retry_after.unwrap_or_else(|| {
                        self.config
                            .base_backoff
                            .saturating_mul(1u32 << attempt.min(6))
                    });
                    if wait > self.config.max_wait || attempt >= self.config.max_retries {
                        return Err(MediaError::RateLimited { retry_after: wait });
                    }
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                }
                status => {
                    return Err(MediaError::Http {
                        source_name,
                        status,
                    })
                }
            }
        }
    }
}

/// Round-robin merge, preserving each source's own ranking.
fn interleave<T>(lists: Vec<Vec<T>>) -> Vec<T> {
    let mut iters: Vec<_> = lists.into_iter().map(Vec::into_iter).collect();
    let mut out = Vec::new();
    loop {
        let mut any = false;
        for it in &mut iters {
            if let Some(x) = it.next() {
                out.push(x);
                any = true;
            }
        }
        if !any {
            return out;
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;

    /// Scripted fetcher: responses are matched by URL substring; each match is consumed in order
    /// (the last one repeats). Records every request.
    type Route = (String, VecDeque<Result<HttpResponse, String>>);

    #[derive(Clone, Default)]
    pub struct FakeFetcher {
        pub routes: Arc<Mutex<Vec<Route>>>,
        pub requests: Arc<Mutex<Vec<HttpRequest>>>,
    }

    impl FakeFetcher {
        pub fn route(self, needle: &str, responses: Vec<Result<HttpResponse, String>>) -> Self {
            self.routes
                .lock()
                .unwrap()
                .push((needle.to_string(), responses.into()));
            self
        }
        pub fn ok(body: &[u8]) -> Result<HttpResponse, String> {
            Ok(HttpResponse {
                status: 200,
                retry_after: None,
                body: body.to_vec(),
            })
        }
        pub fn status(status: u16, retry_after: Option<u64>) -> Result<HttpResponse, String> {
            Ok(HttpResponse {
                status,
                retry_after: retry_after.map(Duration::from_secs),
                body: Vec::new(),
            })
        }
        pub fn count(&self, needle: &str) -> usize {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.url.contains(needle))
                .count()
        }
    }

    impl HttpFetcher for FakeFetcher {
        async fn get(&self, req: &HttpRequest) -> Result<HttpResponse, MediaError> {
            self.requests.lock().unwrap().push(req.clone());
            let mut routes = self.routes.lock().unwrap();
            for (needle, q) in routes.iter_mut() {
                if req.url.contains(needle.as_str()) {
                    let r = if q.len() > 1 {
                        q.pop_front().unwrap()
                    } else {
                        q.front().cloned().unwrap()
                    };
                    return r.map_err(MediaError::Network);
                }
            }
            Err(MediaError::Network(format!("no route for {}", req.url)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FakeFetcher;
    use super::*;

    const OV: &[u8] = include_bytes!("../tests/fixtures/openverse_cross.json");
    const CM: &[u8] = include_bytes!("../tests/fixtures/commons_cross.json");

    fn client(f: FakeFetcher) -> MediaClient<FakeFetcher> {
        MediaClient::new(f, MediaConfig::default())
    }

    fn req(q: &str) -> SearchRequest {
        SearchRequest {
            query: q.into(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn searches_both_sources_translated_and_interleaved() {
        let f = FakeFetcher::default()
            .route("openverse", vec![FakeFetcher::ok(OV)])
            .route("commons", vec![FakeFetcher::ok(CM)]);
        let c = client(f.clone());
        let r = c.search(&req("Adoración")).await;
        assert_eq!(r.translated_query, "worship");
        assert!(r.failures.is_empty());
        let keys: Vec<_> = r.results.iter().map(|x| x.key()).collect();
        assert_eq!(
            keys,
            [
                "openverse-ov-1",
                "commons-111",
                "openverse-ov-2",
                "commons-222",
                "openverse-ov-unknown-size"
            ]
        );
        assert!(r.has_more);
        let reqs = f.requests.lock().unwrap();
        assert!(reqs.iter().all(|r| r.url.contains("worship")));
        assert!(reqs
            .iter()
            .all(|r| !r.headers.iter().any(|(k, _)| k == "Authorization")));
    }

    #[tokio::test]
    async fn caches_results_until_ttl() {
        tokio::time::pause();
        let f = FakeFetcher::default().route("openverse", vec![FakeFetcher::ok(OV)]);
        let c = client(f.clone());
        let q = SearchRequest {
            query: "cruz".into(),
            source: SourceFilter::Openverse,
            ..Default::default()
        };
        c.search(&q).await;
        c.search(&q).await;
        assert_eq!(f.count("openverse"), 1);
        tokio::time::advance(Duration::from_secs(16 * 60)).await;
        c.search(&q).await;
        assert_eq!(f.count("openverse"), 2);
        // A different page is a different cache entry.
        c.search(&SearchRequest {
            page: 2,
            ..q.clone()
        })
        .await;
        assert_eq!(f.count("openverse"), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_429_respecting_retry_after() {
        let f = FakeFetcher::default().route(
            "openverse",
            vec![
                FakeFetcher::status(429, Some(3)),
                FakeFetcher::status(429, Some(2)),
                FakeFetcher::ok(OV),
            ],
        );
        let c = client(f.clone());
        let start = tokio::time::Instant::now();
        let r = c
            .search(&SearchRequest {
                query: "cross".into(),
                source: SourceFilter::Openverse,
                ..Default::default()
            })
            .await;
        assert!(r.failures.is_empty());
        assert_eq!(r.results.len(), 3);
        assert_eq!(f.count("openverse"), 3);
        assert_eq!(start.elapsed(), Duration::from_secs(5));
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_max_retries_with_exponential_backoff() {
        let f = FakeFetcher::default().route("openverse", vec![FakeFetcher::status(429, None)]);
        let c = client(f.clone());
        let start = tokio::time::Instant::now();
        let r = c
            .search(&SearchRequest {
                query: "cross".into(),
                source: SourceFilter::Openverse,
                ..Default::default()
            })
            .await;
        assert_eq!(f.count("openverse"), 4);
        assert_eq!(start.elapsed(), Duration::from_secs(1 + 2 + 4));
        assert_eq!(r.failures.len(), 1);
        assert_eq!(r.failures[0].source, ImageSource::Openverse);
        assert_eq!(r.failures[0].retry_after_secs, Some(8));
    }

    #[tokio::test(start_paused = true)]
    async fn long_retry_after_is_reported_not_slept() {
        let f =
            FakeFetcher::default().route("openverse", vec![FakeFetcher::status(429, Some(120))]);
        let c = client(f.clone());
        let start = tokio::time::Instant::now();
        let r = c
            .search(&SearchRequest {
                query: "cross".into(),
                source: SourceFilter::Openverse,
                ..Default::default()
            })
            .await;
        assert_eq!(f.count("openverse"), 1);
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_eq!(r.failures[0].retry_after_secs, Some(120));
    }

    #[tokio::test]
    async fn one_source_failing_still_returns_the_other() {
        let f = FakeFetcher::default()
            .route("openverse", vec![FakeFetcher::status(500, None)])
            .route("commons", vec![FakeFetcher::ok(CM)]);
        let r = client(f).search(&req("cross")).await;
        assert_eq!(r.results.len(), 2);
        assert_eq!(r.failures.len(), 1);
        assert!(r.failures[0].message.contains("500"));
    }

    #[tokio::test]
    async fn bearer_token_goes_only_to_openverse() {
        let f = FakeFetcher::default()
            .route("openverse", vec![FakeFetcher::ok(OV)])
            .route("commons", vec![FakeFetcher::ok(CM)]);
        let c = client(f.clone());
        c.set_openverse_token(Some("  secret ".into()));
        c.search(&req("cross")).await;
        let reqs = f.requests.lock().unwrap();
        for r in reqs.iter() {
            let auth = r.headers.iter().find(|(k, _)| k == "Authorization");
            if r.url.contains("openverse") {
                assert_eq!(auth.unwrap().1, "Bearer secret");
            } else {
                assert!(auth.is_none());
            }
        }
    }

    #[tokio::test]
    async fn empty_query_makes_no_requests() {
        let f = FakeFetcher::default();
        let r = client(f.clone()).search(&req("  de la ")).await;
        assert!(r.results.is_empty());
        assert!(f.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn noncommercial_toggle_changes_filtering() {
        let f = FakeFetcher::default().route("openverse", vec![FakeFetcher::ok(OV)]);
        let c = client(f);
        let base = SearchRequest {
            query: "cross".into(),
            source: SourceFilter::Openverse,
            ..Default::default()
        };
        assert_eq!(c.search(&base).await.results.len(), 3);
        let nc = c
            .search(&SearchRequest {
                allow_noncommercial: true,
                ..base
            })
            .await;
        assert!(nc.results.iter().any(|r| r.id == "ov-nc"));
    }

    #[tokio::test]
    async fn rejects_non_public_hosts() {
        let f = FakeFetcher::default().route("", vec![FakeFetcher::ok(b"x")]);
        let c = client(f.clone());
        for u in [
            "https://169.254.169.254/latest/meta-data",
            "https://intranet/a.jpg",
            "https://localhost/a.jpg",
        ] {
            assert!(
                matches!(c.fetch_image(u).await, Err(MediaError::InvalidDownload(_))),
                "{u}"
            );
        }
        assert!(f.requests.lock().unwrap().is_empty(), "nothing was fetched");
        for ok in [
            "https://upload.wikimedia.org/a.jpg",
            "https://cdn.some-photographer.example/a.jpg",
        ] {
            assert!(c.fetch_image(ok).await.is_ok(), "{ok}");
        }
        let reqs = f.requests.lock().unwrap();
        assert!(reqs
            .iter()
            .all(|r| r.accept_types == crate::http::IMAGE_TYPES));
    }

    #[tokio::test]
    async fn rejects_non_https_downloads() {
        let c = client(FakeFetcher::default());
        assert!(matches!(
            c.fetch_image("http://upload.wikimedia.org/a.jpg").await,
            Err(MediaError::InvalidDownload(_))
        ));
        assert!(matches!(
            c.fetch_image("file:///etc/passwd").await,
            Err(MediaError::InvalidDownload(_))
        ));
    }

    fn ov_result() -> MediaResult {
        MediaResult {
            source: ImageSource::Openverse,
            id: "abc-123".into(),
            title: "t".into(),
            thumb_url: "https://thumbs.example.com/t.jpg".into(),
            full_url: "https://photos.example.com/full.jpg".into(),
            width: 2000,
            height: 1000,
            creator: None,
            creator_url: None,
            license: "CC0".into(),
            license_url: None,
            source_page: String::new(),
            attribution: String::new(),
        }
    }

    #[tokio::test]
    async fn falls_back_to_the_openverse_proxy_when_the_original_fails() {
        let f = FakeFetcher::default()
            .route("photos.example.com", vec![FakeFetcher::status(404, None)])
            .route("/abc-123/thumb/", vec![FakeFetcher::ok(b"proxied")]);
        let c = client(f.clone());
        assert_eq!(
            c.fetch_result_image(&ov_result()).await.unwrap(),
            b"proxied"
        );
        assert_eq!(f.count("photos.example.com"), 1);
        assert_eq!(f.count("api.openverse.org/v1/images/abc-123/thumb/"), 1);
    }

    #[tokio::test]
    async fn fallback_reports_the_original_error_when_everything_fails() {
        let f = FakeFetcher::default().route("", vec![FakeFetcher::status(404, None)]);
        let c = client(f.clone());
        let e = c.fetch_result_image(&ov_result()).await.unwrap_err();
        assert!(matches!(e, MediaError::Http { status: 404, .. }));
        assert_eq!(f.count("full.jpg"), 1);
        assert_eq!(f.count("t.jpg"), 1);
        assert_eq!(f.count("/thumb/"), 1);
    }

    #[tokio::test]
    async fn commons_images_have_no_fallback() {
        let f = FakeFetcher::default().route("", vec![FakeFetcher::status(404, None)]);
        let c = client(f.clone());
        let mut r = ov_result();
        r.source = ImageSource::Commons;
        assert!(c.fetch_result_image(&r).await.is_err());
        assert_eq!(f.requests.lock().unwrap().len(), 1);
    }
}
