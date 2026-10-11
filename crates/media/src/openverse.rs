//! Openverse images API (<https://api.openverse.org/v1/images/>).

use serde::Deserialize;
use url::Url;

use crate::error::MediaError;
use crate::license::{License, LicensePolicy};
use crate::types::{build_attribution, ImageSource, MediaResult, SearchPage};

pub const DEFAULT_BASE: &str = "https://api.openverse.org/v1/images/";
/// Openverse caps `page_size` at 20 for anonymous clients.
pub const MAX_PAGE_SIZE: u32 = 20;

/// Search URL: commercial + modification licenses, wide, large, `mature=false`.
pub fn build_url(
    base: &str,
    query_en: &str,
    page: u32,
    page_size: u32,
) -> Result<String, MediaError> {
    let mut url = Url::parse(base).map_err(|e| MediaError::Parse(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("q", query_en)
        .append_pair("license_type", "commercial,modification")
        .append_pair("aspect_ratio", "wide")
        .append_pair("size", "large")
        .append_pair("mature", "false")
        .append_pair("page_size", &page_size.clamp(1, MAX_PAGE_SIZE).to_string())
        .append_pair("page", &page.max(1).to_string());
    Ok(url.into())
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    results: Vec<Item>,
    page_count: Option<u32>,
}

#[derive(Deserialize)]
struct Item {
    id: Option<String>,
    title: Option<String>,
    foreign_landing_url: Option<String>,
    url: Option<String>,
    thumbnail: Option<String>,
    creator: Option<String>,
    creator_url: Option<String>,
    license: Option<String>,
    license_version: Option<String>,
    license_url: Option<String>,
    attribution: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    #[serde(default)]
    mature: bool,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn is_http(u: &str) -> bool {
    u.starts_with("https://") || u.starts_with("http://")
}

/// Parses one response page. Drops mature items, disallowed licenses, portrait images and images
/// narrower than `min_width` (an unreported size is kept; the download verifies the real size).
pub fn parse(
    body: &[u8],
    policy: &LicensePolicy,
    min_width: u32,
    page: u32,
) -> Result<SearchPage, MediaError> {
    let resp: Response = serde_json::from_slice(body)?;
    let mut results = Vec::new();
    for it in resp.results {
        let (Some(id), Some(full)) = (non_empty(it.id), non_empty(it.url)) else {
            continue;
        };
        if it.mature || !is_http(&full) {
            continue;
        }
        let license = License::parse(
            it.license.as_deref().unwrap_or(""),
            it.license_version.as_deref(),
        );
        if !policy.allows(license.kind) {
            continue;
        }
        let (w, h) = (it.width.unwrap_or(0), it.height.unwrap_or(0));
        if w > 0 && (w < min_width || (h > 0 && h > w)) {
            continue;
        }
        let title = non_empty(it.title).unwrap_or_else(|| "Sin título".to_string());
        let creator = non_empty(it.creator);
        let attribution = non_empty(it.attribution)
            .unwrap_or_else(|| build_attribution(&title, creator.as_deref(), &license));
        let thumb = non_empty(it.thumbnail)
            .filter(|t| is_http(t))
            .unwrap_or_else(|| full.clone());
        results.push(MediaResult {
            source: ImageSource::Openverse,
            id,
            thumb_url: thumb,
            source_page: non_empty(it.foreign_landing_url)
                .filter(|u| is_http(u))
                .unwrap_or_else(|| full.clone()),
            full_url: full,
            title,
            width: w,
            height: h,
            creator,
            creator_url: non_empty(it.creator_url).filter(|u| is_http(u)),
            license: license.display(),
            license_url: non_empty(it.license_url).or_else(|| license.url()),
            attribution,
        });
    }
    let has_more = resp.page_count.is_some_and(|pc| page < pc);
    Ok(SearchPage {
        results,
        page,
        has_more,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/openverse_cross.json");

    #[test]
    fn builds_the_documented_query() {
        let u = build_url(DEFAULT_BASE, "cross sky", 2, 50).unwrap();
        assert!(u.starts_with("https://api.openverse.org/v1/images/?q=cross+sky&"));
        for part in [
            "license_type=commercial%2Cmodification",
            "aspect_ratio=wide",
            "size=large",
            "mature=false",
            "page_size=20",
            "page=2",
        ] {
            assert!(u.contains(part), "{u} lacks {part}");
        }
    }

    #[test]
    fn parses_and_filters_fixture() {
        let page = parse(FIXTURE, &LicensePolicy::default(), 1920, 1).unwrap();
        let ids: Vec<_> = page.results.iter().map(|r| r.id.as_str()).collect();
        // Dropped: NC license, mature, too narrow, portrait, no url.
        assert_eq!(ids, ["ov-1", "ov-2", "ov-unknown-size"]);
        assert!(page.has_more);
        let first = &page.results[0];
        assert_eq!(first.source, ImageSource::Openverse);
        assert_eq!(first.license, "CC BY 2.0");
        assert_eq!(first.creator.as_deref(), Some("Jane Doe"));
        assert_eq!(
            first.creator_url.as_deref(),
            Some("https://example.org/jane")
        );
        assert_eq!(
            first.thumb_url,
            "https://api.openverse.org/v1/images/ov-1/thumb/"
        );
        assert_eq!((first.width, first.height), (3000, 2000));
        assert_eq!(
            first.attribution,
            "\"Wooden cross\" by Jane Doe is licensed under CC BY 2.0."
        );
        let second = &page.results[1];
        assert_eq!(second.license, "CC0 1.0");
        assert!(second.attribution.contains("is marked with CC0 1.0"));
        assert_eq!(
            second.license_url.as_deref(),
            Some("https://creativecommons.org/publicdomain/zero/1.0/")
        );
        assert_eq!(page.results[2].width, 0);
    }

    #[test]
    fn permissive_policy_lets_nc_through() {
        let page = parse(FIXTURE, &LicensePolicy::permissive(), 1920, 1).unwrap();
        assert!(page.results.iter().any(|r| r.id == "ov-nc"));
    }

    #[test]
    fn last_page_has_no_more() {
        assert!(
            !parse(FIXTURE, &LicensePolicy::default(), 1920, 3)
                .unwrap()
                .has_more
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(
            parse(b"<html>", &LicensePolicy::default(), 1920, 1),
            Err(MediaError::Parse(_))
        ));
    }
}
