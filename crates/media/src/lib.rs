//! Open image bank (PLAN §E2): search free-licensed backgrounds from Openverse and Wikimedia
//! Commons, download them into the local media library with their license metadata, and export
//! an image credits page.
//!
//! Layout:
//! - [`types`]: normalized [`MediaResult`], [`SearchRequest`], [`SearchPage`], [`LibraryItem`].
//! - [`license`]: license parsing and the [`LicensePolicy`] (default: CC0, PDM, CC BY, CC BY-SA).
//! - [`keywords`]: Spanish to English keyword mapping and the curated quick [`categories`].
//! - [`openverse`], [`commons`]: request builders and response parsers (pure, fixture-tested).
//! - [`http`]: the [`HttpFetcher`] abstraction, the reqwest (rustls) implementation and 429 backoff.
//! - [`client`]: [`MediaClient`], search with cache, backoff and license filtering.
//! - [`library`]: [`MediaLibrary`], downloads, sidecar JSON, thumbnails, credits.
//! - [`service`]: [`MediaService`], the façade the Tauri shell wires to four commands:
//!   `media_search`, `media_download`, `media_list` and `media_credits` (plus `media_categories`
//!   and `media_remove`).
//!
//! Everything is projected from disk: nothing here is needed during a service.
//! The crate performs no network access in tests; the fetcher is injected.

pub mod cache;
pub mod client;
pub mod commons;
pub mod error;
pub mod html;
pub mod http;
pub mod keywords;
pub mod library;
pub mod license;
pub mod openverse;
pub mod service;
pub mod types;

pub use client::{MediaClient, MediaConfig};
pub use error::MediaError;
pub use http::{HttpFetcher, HttpRequest, HttpResponse, ReqwestFetcher};
pub use keywords::{categories, translate_query, Category};
pub use library::{credits_markdown, MediaLibrary};
pub use license::{LicenseKind, LicensePolicy};
pub use service::{DefaultMediaService, MediaService};
pub use types::{
    ImageSource, LibraryItem, MediaResult, SearchPage, SearchRequest, SearchResponse,
    SourceFailure, SourceFilter,
};

/// `User-Agent` sent on every request (identifiable, per Openverse and Wikimedia policy).
pub fn user_agent() -> String {
    format!(
        "HollyShow/{} (+https://github.com/mfuentesg/hollyshow)",
        env!("CARGO_PKG_VERSION")
    )
}
