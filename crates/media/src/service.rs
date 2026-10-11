use std::path::PathBuf;
use std::sync::Arc;

use crate::client::{MediaClient, MediaConfig};
use crate::error::MediaError;
use crate::http::{HttpFetcher, ReqwestFetcher};
use crate::keywords::{categories, Category};
use crate::library::{credits_markdown, MediaLibrary};
use crate::license::LicensePolicy;
use crate::types::{LibraryItem, MediaResult, SearchRequest, SearchResponse};

/// Façade for the Tauri shell: one method per IPC command.
///
/// | IPC command        | method                         |
/// |--------------------|--------------------------------|
/// | `media_search`     | [`MediaService::search`]       |
/// | `media_categories` | [`MediaService::categories`]   |
/// | `media_download`   | [`MediaService::download`]     |
/// | `media_list`       | [`MediaService::list`]         |
/// | `media_remove`     | [`MediaService::remove`]       |
/// | `media_credits`    | [`MediaService::credits`]      |
///
/// The shell keeps one `Arc<DefaultMediaService>` in Tauri state, builds it with the app data
/// `media/` directory and calls [`MediaService::set_openverse_token`] when the setting changes.
pub struct MediaService<F: HttpFetcher> {
    client: Arc<MediaClient<F>>,
    library: MediaLibrary,
}

pub type DefaultMediaService = MediaService<ReqwestFetcher>;

impl DefaultMediaService {
    /// Production service: reqwest (rustls), identifiable User-Agent, default configuration.
    pub fn open(
        dir: impl Into<PathBuf>,
        openverse_token: Option<String>,
    ) -> Result<Self, MediaError> {
        let config = MediaConfig {
            openverse_token,
            ..MediaConfig::default()
        };
        Self::with_fetcher(ReqwestFetcher::new()?, dir, config)
    }
}

impl<F: HttpFetcher> MediaService<F> {
    pub fn with_fetcher(
        fetcher: F,
        dir: impl Into<PathBuf>,
        config: MediaConfig,
    ) -> Result<Self, MediaError> {
        Ok(Self {
            client: Arc::new(MediaClient::new(fetcher, config)),
            library: MediaLibrary::new(dir)?,
        })
    }

    pub fn library(&self) -> &MediaLibrary {
        &self.library
    }

    pub fn set_openverse_token(&self, token: Option<String>) {
        self.client.set_openverse_token(token);
    }

    pub async fn search(&self, req: SearchRequest) -> SearchResponse {
        self.client.search(&req).await
    }

    pub fn categories(&self) -> Vec<Category> {
        categories()
    }

    /// Downloads into the library. `allow_noncommercial` mirrors the search toggle.
    pub async fn download(
        &self,
        result: MediaResult,
        allow_noncommercial: bool,
    ) -> Result<LibraryItem, MediaError> {
        let policy = if allow_noncommercial {
            LicensePolicy::permissive()
        } else {
            LicensePolicy::default()
        };
        self.library.download(&self.client, &result, &policy).await
    }

    pub fn list(&self) -> Result<Vec<LibraryItem>, MediaError> {
        self.library.list()
    }

    pub fn remove(&self, id: &str) -> Result<bool, MediaError> {
        self.library.remove(id)
    }

    /// Markdown credits page for `ids` (all downloaded images when `None`).
    pub fn credits(&self, ids: Option<&[String]>) -> Result<String, MediaError> {
        let mut items = self.library.list()?;
        if let Some(ids) = ids {
            items.retain(|i| ids.contains(&i.id));
        }
        Ok(credits_markdown(&items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::testing::FakeFetcher;
    use crate::types::SourceFilter;

    #[tokio::test]
    async fn search_download_list_credits_roundtrip() {
        let ov = include_bytes!("../tests/fixtures/openverse_cross.json");
        let img = crate::library::tests::png(1920, 1080);
        let f = FakeFetcher::default()
            .route("api.openverse.org", vec![FakeFetcher::ok(ov)])
            .route("example.org/2/full.jpg", vec![FakeFetcher::ok(&img)]);
        let dir = tempfile::tempdir().unwrap();
        let svc = MediaService::with_fetcher(f, dir.path(), MediaConfig::default()).unwrap();
        let res = svc
            .search(SearchRequest {
                query: "cruz".into(),
                source: SourceFilter::Openverse,
                ..Default::default()
            })
            .await;
        assert_eq!(res.translated_query, "cross");
        let pick = res.results.iter().find(|r| r.id == "ov-2").unwrap().clone();
        let item = svc.download(pick, false).await.unwrap();
        assert_eq!(svc.list().unwrap(), vec![item.clone()]);
        let md = svc.credits(None).unwrap();
        assert!(md.contains("Sunrise cross") && md.contains("CC0 1.0"));
        assert!(!svc
            .credits(Some(&["nope".to_string()]))
            .unwrap()
            .contains("Sunrise"));
        assert!(svc.remove(&item.id).unwrap());
        assert!(svc.list().unwrap().is_empty());
        assert_eq!(svc.categories().len(), 11);
    }

    #[test]
    fn wire_format_is_camel_case() {
        let r = crate::library::tests::sample("w", "CC0 1.0");
        let v = serde_json::to_value(&r).unwrap();
        for k in [
            "source",
            "id",
            "title",
            "thumbUrl",
            "fullUrl",
            "width",
            "height",
            "creator",
            "creatorUrl",
            "license",
            "licenseUrl",
            "attribution",
            "sourcePage",
        ] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(v["source"], "openverse");
        let req: SearchRequest = serde_json::from_str(r#"{"query":"cruz"}"#).unwrap();
        assert_eq!(
            (req.page, req.source, req.allow_noncommercial),
            (1, SourceFilter::All, false)
        );
    }
}
