//! Wikimedia Commons through the MediaWiki API: `generator=search` in namespace 6 (File) plus
//! `imageinfo` with `extmetadata` for license, author and attribution.

use std::collections::HashMap;

use serde::Deserialize;
use url::Url;

use crate::error::MediaError;
use crate::html::{first_href, strip_tags};
use crate::license::{License, LicensePolicy};
use crate::types::{build_attribution, ImageSource, MediaResult, SearchPage};

pub const DEFAULT_BASE: &str = "https://commons.wikimedia.org/w/api.php";
const THUMB_WIDTH: u32 = 640;
const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/webp"];

/// Search URL for page `page` (1-based) of `page_size` files at least `min_width` px wide.
pub fn build_url(
    base: &str,
    query_en: &str,
    page: u32,
    page_size: u32,
    min_width: u32,
) -> Result<String, MediaError> {
    let page_size = page_size.clamp(1, 50);
    let offset = page.max(1).saturating_sub(1) * page_size;
    let search = format!(
        "{query_en} filetype:bitmap filew:>{}",
        min_width.saturating_sub(1)
    );
    let mut url = Url::parse(base).map_err(|e| MediaError::Parse(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("action", "query")
        .append_pair("format", "json")
        .append_pair("formatversion", "2")
        .append_pair("generator", "search")
        .append_pair("gsrsearch", &search)
        .append_pair("gsrnamespace", "6")
        .append_pair("gsrlimit", &page_size.to_string())
        .append_pair("gsroffset", &offset.to_string())
        .append_pair("prop", "imageinfo")
        .append_pair("iiprop", "url|size|mime|extmetadata")
        .append_pair("iiurlwidth", &THUMB_WIDTH.to_string())
        .append_pair(
            "iiextmetadatafilter",
            "LicenseShortName|LicenseUrl|Artist|Attribution|AttributionRequired|ObjectName",
        );
    Ok(url.into())
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    query: Option<Query>,
    #[serde(rename = "continue", default)]
    cont: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Query {
    #[serde(default)]
    pages: Vec<Page>,
}

#[derive(Deserialize)]
struct Page {
    pageid: u64,
    title: String,
    #[serde(default)]
    index: u32,
    #[serde(default)]
    imageinfo: Vec<Info>,
}

#[derive(Deserialize)]
struct Info {
    thumburl: Option<String>,
    url: Option<String>,
    descriptionurl: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    mime: Option<String>,
    #[serde(default)]
    extmetadata: HashMap<String, Meta>,
}

#[derive(Deserialize)]
struct Meta {
    value: Option<serde_json::Value>,
}

fn meta_str(m: &HashMap<String, Meta>, key: &str) -> Option<String> {
    match m.get(key)?.value.as_ref()? {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

fn file_title(raw: &str) -> String {
    let t = raw.strip_prefix("File:").unwrap_or(raw);
    let t = t
        .rsplit_once('.')
        .map_or(t, |(stem, ext)| if ext.len() <= 5 { stem } else { t });
    t.replace('_', " ").trim().to_string()
}

fn is_https(u: &str) -> bool {
    u.starts_with("https://") || u.starts_with("http://")
}

/// Parses one response page. Files without license metadata, with a license outside the policy,
/// non-bitmap mime types, portrait or narrower than `min_width` are dropped.
pub fn parse(
    body: &[u8],
    policy: &LicensePolicy,
    min_width: u32,
    page: u32,
) -> Result<SearchPage, MediaError> {
    let resp: Response = serde_json::from_slice(body)?;
    let has_more = resp.cont.is_some();
    let mut pages = resp.query.map(|q| q.pages).unwrap_or_default();
    pages.sort_by_key(|p| p.index);
    let mut results = Vec::new();
    for p in pages {
        let Some(info) = p.imageinfo.into_iter().next() else {
            continue;
        };
        let (Some(url), Some(w), Some(h)) =
            (info.url.filter(|u| is_https(u)), info.width, info.height)
        else {
            continue;
        };
        if w < min_width || h > w {
            continue;
        }
        if !info
            .mime
            .as_deref()
            .is_some_and(|m| ALLOWED_MIME.contains(&m))
        {
            continue;
        }
        let Some(short) = meta_str(&info.extmetadata, "LicenseShortName") else {
            continue;
        };
        let license = License::parse(&short, None);
        if !policy.allows(license.kind) {
            continue;
        }
        let title = file_title(&p.title);
        let artist_html = meta_str(&info.extmetadata, "Artist");
        let creator = artist_html
            .as_deref()
            .map(strip_tags)
            .filter(|s| !s.is_empty());
        let creator_url = artist_html.as_deref().and_then(first_href);
        let attribution = meta_str(&info.extmetadata, "Attribution")
            .map(|a| strip_tags(&a))
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| build_attribution(&title, creator.as_deref(), &license));
        let source_page = info
            .descriptionurl
            .filter(|u| is_https(u))
            .unwrap_or_else(|| format!("https://commons.wikimedia.org/?curid={}", p.pageid));
        results.push(MediaResult {
            source: ImageSource::Commons,
            id: p.pageid.to_string(),
            title,
            thumb_url: info
                .thumburl
                .filter(|u| is_https(u))
                .unwrap_or_else(|| url.clone()),
            full_url: url,
            width: w,
            height: h,
            creator,
            creator_url,
            license: license.display(),
            license_url: meta_str(&info.extmetadata, "LicenseUrl")
                .filter(|u| is_https(u))
                .or_else(|| license.url()),
            attribution,
            source_page,
        });
    }
    Ok(SearchPage {
        results,
        page,
        has_more,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/commons_cross.json");

    #[test]
    fn builds_generator_search_url() {
        let u = build_url(DEFAULT_BASE, "stained glass", 3, 20, 1920).unwrap();
        for part in [
            "generator=search",
            "gsrnamespace=6",
            "gsrlimit=20",
            "gsroffset=40",
            "prop=imageinfo",
            "extmetadata",
            "filew%3A%3E1919",
            "filetype%3Abitmap",
            "formatversion=2",
        ] {
            assert!(u.contains(part), "{u} lacks {part}");
        }
    }

    #[test]
    fn parses_orders_and_filters_fixture() {
        let page = parse(FIXTURE, &LicensePolicy::default(), 1920, 1).unwrap();
        let ids: Vec<_> = page.results.iter().map(|r| r.id.as_str()).collect();
        // Ordered by search rank; dropped: narrow, svg, NC, no imageinfo.
        assert_eq!(ids, ["111", "222"]);
        assert!(page.has_more);

        let pd = &page.results[0];
        assert_eq!(pd.license, "PDM");
        assert_eq!(pd.title, "Pantocrator icon");
        assert_eq!(pd.creator.as_deref(), Some("Unknown iconographer"));
        assert_eq!(pd.creator_url, None);
        assert_eq!(
            pd.license_url.as_deref(),
            Some("https://creativecommons.org/publicdomain/mark/1.0/")
        );
        assert!(pd
            .attribution
            .starts_with("\"Pantocrator icon\" by Unknown iconographer"));

        let by = &page.results[1];
        assert_eq!(by.license, "CC BY-SA 4.0");
        assert_eq!(by.title, "Stained glass window");
        assert_eq!(by.creator.as_deref(), Some("María & Co"));
        assert_eq!(
            by.creator_url.as_deref(),
            Some("https://commons.wikimedia.org/wiki/User:Maria")
        );
        assert_eq!(by.attribution, "Photo: Maria & Co / CC BY-SA 4.0");
        assert_eq!(
            by.source_page,
            "https://commons.wikimedia.org/wiki/File:Stained_glass_window.jpg"
        );
        assert_eq!(
            by.thumb_url,
            "https://upload.wikimedia.org/thumb/222/640px-Stained.jpg"
        );
        assert_eq!((by.width, by.height), (4000, 2250));
    }

    #[test]
    fn empty_result_is_ok() {
        let page = parse(
            br#"{"batchcomplete":true}"#,
            &LicensePolicy::default(),
            1920,
            1,
        )
        .unwrap();
        assert!(page.results.is_empty());
        assert!(!page.has_more);
    }

    #[test]
    fn file_title_strips_prefix_and_extension() {
        assert_eq!(
            file_title("File:My_photo.of.cross.JPG"),
            "My photo.of.cross"
        );
        assert_eq!(file_title("File:NoExt"), "NoExt");
    }
}
