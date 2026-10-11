use serde::{Deserialize, Serialize};

use crate::license::LicensePolicy;

/// Where an image comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageSource {
    Openverse,
    Commons,
}

impl ImageSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageSource::Openverse => "openverse",
            ImageSource::Commons => "commons",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ImageSource::Openverse => "Openverse",
            ImageSource::Commons => "Wikimedia Commons",
        }
    }
}

/// Which sources a search should query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceFilter {
    /// Openverse first, Wikimedia Commons merged in (and used as fallback when Openverse fails).
    #[default]
    All,
    Openverse,
    Commons,
}

/// Normalized search hit, identical for every source. Field names are the wire contract with
/// `packages/protocol/src/media.ts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaResult {
    pub source: ImageSource,
    /// Provider id (Openverse uuid or Commons page id), unique together with `source`.
    pub id: String,
    pub title: String,
    pub thumb_url: String,
    pub full_url: String,
    /// Pixels; 0 when the provider did not report it.
    pub width: u32,
    pub height: u32,
    pub creator: Option<String>,
    pub creator_url: Option<String>,
    /// Display form, e.g. `CC BY-SA 4.0`, `CC0 1.0`, `PDM`.
    pub license: String,
    pub license_url: Option<String>,
    /// Ready-to-print credit line.
    pub attribution: String,
    /// Landing page of the image at the provider.
    pub source_page: String,
}

impl MediaResult {
    /// Stable key `source-id` (used for file names and as the media id of a background).
    pub fn key(&self) -> String {
        format!("{}-{}", self.source.as_str(), sanitize_id(&self.id))
    }
}

/// Keeps only `[A-Za-z0-9_-]`, so a provider id can never escape the library directory.
pub fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchRequest {
    /// Free text, usually Spanish; translated to English keywords before querying.
    pub query: String,
    pub source: SourceFilter,
    /// 1-based.
    pub page: u32,
    /// Overrides the license policy (default CC0/PDM/CC BY/CC BY-SA).
    pub allow_noncommercial: bool,
}

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            source: SourceFilter::All,
            page: 1,
            allow_noncommercial: false,
        }
    }
}

impl SearchRequest {
    pub fn policy(&self) -> LicensePolicy {
        if self.allow_noncommercial {
            LicensePolicy::permissive()
        } else {
            LicensePolicy::default()
        }
    }
}

/// One page of results from one source.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub results: Vec<MediaResult>,
    pub page: u32,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFailure {
    pub source: ImageSource,
    pub message: String,
    /// Seconds to wait when the provider rate limited us.
    pub retry_after_secs: Option<u64>,
}

/// Merged answer of `media_search`: partial results plus per-source failures.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<MediaResult>,
    pub page: u32,
    pub has_more: bool,
    /// The English query actually sent to the providers.
    pub translated_query: String,
    pub failures: Vec<SourceFailure>,
}

/// An image stored in the local media library (sidecar JSON next to the file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    /// Media id (= [`MediaResult::key`]); this is what a theme background references.
    pub id: String,
    /// Absolute path of the image on disk.
    pub path: String,
    /// Absolute path of the generated thumbnail (JPEG).
    pub thumb_path: String,
    /// Real pixel size of the stored file.
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub downloaded_at_ms: u64,
    pub result: MediaResult,
}

/// Builds the credit line when the provider gives none.
pub fn build_attribution(
    title: &str,
    creator: Option<&str>,
    license: &crate::license::License,
) -> String {
    let verb = match license.kind {
        crate::license::LicenseKind::Cc0 | crate::license::LicenseKind::Pdm => "is marked with",
        _ => "is licensed under",
    };
    match creator.filter(|c| !c.is_empty()) {
        Some(c) => format!("\"{title}\" by {c} {verb} {}.", license.display()),
        None => format!("\"{title}\" {verb} {}.", license.display()),
    }
}
