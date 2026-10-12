//! Local media library: downloaded backgrounds live on disk (never fetched during a service),
//! each with a sidecar JSON holding the license metadata and a generated thumbnail.
//!
//! Layout inside the library directory, for an image with key `openverse-<id>`:
//! `openverse-<id>.jpg` (the file), `openverse-<id>.thumb.jpg` and `openverse-<id>.json`
//! (written last: its presence marks a complete download).

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::ImageFormat;
use serde::{Deserialize, Serialize};

use crate::client::MediaClient;
use crate::error::MediaError;
use crate::http::HttpFetcher;
use crate::license::LicensePolicy;
use crate::types::{sanitize_id, LibraryItem, MediaResult};

const THUMB_W: u32 = 640;
const THUMB_H: u32 = 360;
/// Anything smaller than this is not usable as a projector background.
const MIN_STORED_WIDTH: u32 = 800;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sidecar {
    file: String,
    thumb: String,
    width: u32,
    height: u32,
    bytes: u64,
    downloaded_at_ms: u64,
    result: MediaResult,
}

#[derive(Debug, Clone)]
pub struct MediaLibrary {
    dir: PathBuf,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn extension(fmt: ImageFormat) -> Option<&'static str> {
    match fmt {
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::Png => Some("png"),
        ImageFormat::WebP => Some("webp"),
        ImageFormat::Gif => Some("gif"),
        _ => None,
    }
}

/// Largest side and decoder allocation accepted: a small file can declare a huge canvas.
const MAX_SIDE: u32 = 12_000;
const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

fn decode_bounded(bytes: &[u8], fmt: ImageFormat) -> Result<image::DynamicImage, MediaError> {
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), fmt);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| MediaError::InvalidDownload(e.to_string()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), MediaError> {
    let tmp = path.with_extension("part");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

impl MediaLibrary {
    /// Opens (creating it if needed) the library directory.
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self, MediaError> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn sidecar_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    fn item_from(&self, sc: Sidecar) -> Option<LibraryItem> {
        // File names come from our own sidecar, but never trust a path separator.
        if [&sc.file, &sc.thumb]
            .iter()
            .any(|f| f.contains('/') || f.contains('\\') || f.contains(".."))
        {
            return None;
        }
        let path = self.dir.join(&sc.file);
        if !path.is_file() {
            return None;
        }
        Some(LibraryItem {
            id: sc.result.key(),
            path: path.to_string_lossy().into_owned(),
            thumb_path: self.dir.join(&sc.thumb).to_string_lossy().into_owned(),
            width: sc.width,
            height: sc.height,
            bytes: sc.bytes,
            downloaded_at_ms: sc.downloaded_at_ms,
            result: sc.result,
        })
    }

    /// One stored image by media id.
    pub fn get(&self, id: &str) -> Result<Option<LibraryItem>, MediaError> {
        if sanitize_id(id) != id || id.is_empty() {
            return Ok(None);
        }
        let p = self.sidecar_path(id);
        if !p.is_file() {
            return Ok(None);
        }
        let sc: Sidecar = serde_json::from_slice(&fs::read(p)?)?;
        Ok(self.item_from(sc))
    }

    /// Every complete download, newest first. Damaged sidecars are skipped.
    pub fn list(&self) -> Result<Vec<LibraryItem>, MediaError> {
        let mut items = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else { continue };
            let Ok(sc) = serde_json::from_slice::<Sidecar>(&bytes) else {
                continue;
            };
            if let Some(item) = self.item_from(sc) {
                items.push(item);
            }
        }
        items.sort_by(|a, b| {
            b.downloaded_at_ms
                .cmp(&a.downloaded_at_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(items)
    }

    /// Deletes an image with its thumbnail and sidecar. Returns whether something was removed.
    pub fn remove(&self, id: &str) -> Result<bool, MediaError> {
        let Some(item) = self.get(id)? else {
            return Ok(false);
        };
        for p in [item.path.as_str(), item.thumb_path.as_str()] {
            match fs::remove_file(p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        fs::remove_file(self.sidecar_path(id))?;
        Ok(true)
    }

    /// Validates and stores already downloaded bytes: decodes the image (so a broken or fake file
    /// is rejected), writes it with its thumbnail and finally the sidecar.
    pub fn store(&self, result: &MediaResult, bytes: &[u8]) -> Result<LibraryItem, MediaError> {
        let key = result.key();
        let fmt =
            image::guess_format(bytes).map_err(|e| MediaError::InvalidDownload(e.to_string()))?;
        let ext = extension(fmt).ok_or_else(|| {
            MediaError::InvalidDownload(format!("unsupported image format {fmt:?}"))
        })?;
        let img = decode_bounded(bytes, fmt)?;
        if img.width() < MIN_STORED_WIDTH {
            return Err(MediaError::InvalidDownload(format!(
                "image too small ({}x{})",
                img.width(),
                img.height()
            )));
        }
        let thumb = img.thumbnail(THUMB_W, THUMB_H).to_rgb8();
        let mut thumb_bytes = Vec::new();
        thumb
            .write_to(&mut Cursor::new(&mut thumb_bytes), ImageFormat::Jpeg)
            .map_err(|e| MediaError::Io(e.to_string()))?;

        let file = format!("{key}.{ext}");
        let thumb_file = format!("{key}.thumb.jpg");
        write_atomic(&self.dir.join(&file), bytes)?;
        write_atomic(&self.dir.join(&thumb_file), &thumb_bytes)?;
        let sc = Sidecar {
            file,
            thumb: thumb_file,
            width: img.width(),
            height: img.height(),
            bytes: bytes.len() as u64,
            downloaded_at_ms: now_ms(),
            result: result.clone(),
        };
        write_atomic(&self.sidecar_path(&key), &serde_json::to_vec_pretty(&sc)?)?;
        self.item_from(sc)
            .ok_or_else(|| MediaError::Io("stored image vanished".into()))
    }

    /// Downloads `result` into the library. Idempotent: an image already stored is returned
    /// without touching the network. Refuses licenses outside `policy`.
    pub async fn download<F: HttpFetcher>(
        &self,
        client: &MediaClient<F>,
        result: &MediaResult,
        policy: &LicensePolicy,
    ) -> Result<LibraryItem, MediaError> {
        if !policy.allows_text(&result.license) {
            return Err(MediaError::LicenseNotAllowed(result.license.clone()));
        }
        if let Some(existing) = self.get(&result.key())? {
            return Ok(existing);
        }
        let bytes = client.fetch_result_image(result).await?;
        let lib = self.clone();
        let result = result.clone();
        tokio::task::spawn_blocking(move || lib.store(&result, &bytes))
            .await
            .map_err(|e| MediaError::Io(e.to_string()))?
    }
}

fn escape_md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '#' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

fn link(text: &str, url: Option<&str>) -> String {
    match url.filter(|u| u.starts_with("http://") || u.starts_with("https://")) {
        Some(u) => format!(
            "[{}]({})",
            escape_md(text),
            u.replace(')', "%29").replace(' ', "%20")
        ),
        None => escape_md(text),
    }
}

/// Markdown "Créditos de imágenes" page for `items` (sorted by title).
pub fn credits_markdown(items: &[LibraryItem]) -> String {
    let mut out = String::from("# Créditos de imágenes\n\n");
    if items.is_empty() {
        out.push_str("No hay imágenes descargadas todavía.\n");
        return out;
    }
    out.push_str("Imágenes de fondo con licencia abierta usadas en esta presentación.\n\n");
    let mut sorted: Vec<&LibraryItem> = items.iter().collect();
    sorted.sort_by_key(|i| i.result.title.to_lowercase());
    for it in sorted {
        let r = &it.result;
        let creator = match r.creator.as_deref().filter(|c| !c.is_empty()) {
            Some(c) => link(c, r.creator_url.as_deref()),
            None => "Autor desconocido".to_string(),
        };
        out.push_str(&format!(
            "- **{}** — {} · {} · {}\n  - {}\n",
            escape_md(&r.title),
            creator,
            link(&r.license, r.license_url.as_deref()),
            link(r.source.label(), Some(&r.source_page)),
            escape_md(&r.attribution),
        ));
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Duration;

    use super::*;
    use crate::client::testing::FakeFetcher;
    use crate::client::MediaConfig;
    use crate::types::ImageSource;

    pub fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 90])
        });
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    /// Default configuration that also trusts the fake image host used by the fixtures.
    pub fn test_config() -> MediaConfig {
        MediaConfig::default()
    }

    pub fn sample(id: &str, license: &str) -> MediaResult {
        MediaResult {
            source: ImageSource::Openverse,
            id: id.into(),
            title: "Cruz [al] amanecer".into(),
            thumb_url: "https://example.org/t.jpg".into(),
            full_url: format!("https://example.org/{id}.png"),
            width: 1920,
            height: 1080,
            creator: Some("Ana *López*".into()),
            creator_url: Some("https://example.org/ana".into()),
            license: license.into(),
            license_url: Some("https://creativecommons.org/licenses/by/4.0/".into()),
            attribution: "\"Cruz\" by Ana is licensed under CC BY 4.0.".into(),
            source_page: "https://example.org/page".into(),
        }
    }

    #[test]
    fn oversized_canvases_are_rejected_before_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let lib = MediaLibrary::new(dir.path()).unwrap();
        // A tiny PNG whose header declares 20000x20000.
        let mut bytes = png(900, 500);
        bytes[16..20].copy_from_slice(&20_000u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&20_000u32.to_be_bytes());
        let err = lib.store(&sample("big", "CC0 1.0"), &bytes).unwrap_err();
        assert!(matches!(err, MediaError::InvalidDownload(_)), "{err}");
        assert!(lib.list().unwrap().is_empty());
    }

    #[test]
    fn store_writes_file_thumbnail_and_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let lib = MediaLibrary::new(dir.path().join("media")).unwrap();
        let item = lib
            .store(&sample("abc", "CC BY 4.0"), &png(1920, 1080))
            .unwrap();
        assert_eq!(item.id, "openverse-abc");
        assert_eq!((item.width, item.height), (1920, 1080));
        assert!(item.path.ends_with("openverse-abc.png"));
        let thumb = image::open(&item.thumb_path).unwrap();
        assert!(thumb.width() <= 640 && thumb.height() <= 360);
        let sidecar = fs::read_to_string(dir.path().join("media/openverse-abc.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&sidecar).unwrap();
        assert_eq!(v["result"]["license"], "CC BY 4.0");
        assert_eq!(v["result"]["creator"], "Ana *López*");
        assert_eq!(v["file"], "openverse-abc.png");
        assert_eq!(lib.get("openverse-abc").unwrap().unwrap(), item);
    }

    #[test]
    fn list_orders_newest_first_and_remove_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let lib = MediaLibrary::new(dir.path()).unwrap();
        lib.store(&sample("a", "CC0 1.0"), &png(900, 500)).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        lib.store(&sample("b", "CC0 1.0"), &png(900, 500)).unwrap();
        let ids: Vec<_> = lib.list().unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(ids, ["openverse-b", "openverse-a"]);
        assert!(lib.remove("openverse-a").unwrap());
        assert!(!lib.remove("openverse-a").unwrap());
        assert_eq!(lib.list().unwrap().len(), 1);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }

    #[test]
    fn rejects_fake_small_and_unsupported_files() {
        let dir = tempfile::tempdir().unwrap();
        let lib = MediaLibrary::new(dir.path()).unwrap();
        assert!(matches!(
            lib.store(&sample("x", "CC0 1.0"), b"<html>not an image</html>"),
            Err(MediaError::InvalidDownload(_))
        ));
        assert!(matches!(
            lib.store(&sample("x", "CC0 1.0"), &png(300, 200)),
            Err(MediaError::InvalidDownload(_))
        ));
        assert!(matches!(
            lib.store(&sample("x", "CC0 1.0"), b"GIF89a\x01\x00\x01\x00"),
            Err(MediaError::InvalidDownload(_))
        ));
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            0,
            "nothing is left behind"
        );
    }

    #[test]
    fn ids_cannot_escape_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let lib = MediaLibrary::new(dir.path().join("lib")).unwrap();
        let mut r = sample("../../evil", "CC0 1.0");
        r.id = "../../evil".into();
        let item = lib.store(&r, &png(900, 500)).unwrap();
        assert_eq!(item.id, "openverse-______evil");
        assert!(Path::new(&item.path).starts_with(dir.path().join("lib")));
        assert!(lib.get("../x").unwrap().is_none());
        assert!(!lib.remove("../lib/openverse-______evil").unwrap());
    }

    #[test]
    fn damaged_sidecar_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let lib = MediaLibrary::new(dir.path()).unwrap();
        fs::write(dir.path().join("broken.json"), b"{nope").unwrap();
        lib.store(&sample("ok", "CC0 1.0"), &png(900, 500)).unwrap();
        assert_eq!(lib.list().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn download_is_idempotent_and_license_gated() {
        let dir = tempfile::tempdir().unwrap();
        let lib = MediaLibrary::new(dir.path()).unwrap();
        let f = FakeFetcher::default().route("example.org", vec![FakeFetcher::ok(&png(1000, 600))]);
        let client = MediaClient::new(f.clone(), test_config());
        let r = sample("dl", "CC BY 4.0");
        let a = lib
            .download(&client, &r, &LicensePolicy::default())
            .await
            .unwrap();
        let b = lib
            .download(&client, &r, &LicensePolicy::default())
            .await
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(f.count("example.org"), 1);

        let nc = sample("nc", "CC BY-NC 4.0");
        assert!(matches!(
            lib.download(&client, &nc, &LicensePolicy::default()).await,
            Err(MediaError::LicenseNotAllowed(_))
        ));
        assert_eq!(f.count("nc.png"), 0, "the gate fires before any request");
        assert!(lib
            .download(&client, &nc, &LicensePolicy::permissive())
            .await
            .is_ok());
        assert_eq!(f.count("nc.png"), 1);
    }

    #[test]
    fn credits_markdown_escapes_and_links() {
        let dir = tempfile::tempdir().unwrap();
        let lib = MediaLibrary::new(dir.path()).unwrap();
        let mut anon = sample("anon", "CC0 1.0");
        anon.creator = None;
        anon.title = "A first".into();
        let items = vec![
            lib.store(&sample("a", "CC BY 4.0"), &png(900, 500))
                .unwrap(),
            lib.store(&anon, &png(900, 500)).unwrap(),
        ];
        let md = credits_markdown(&items);
        assert!(md.starts_with("# Créditos de imágenes\n"));
        let first = md.find("**A first**").unwrap();
        let second = md.find("**Cruz \\[al\\] amanecer**").unwrap();
        assert!(first < second, "sorted by title");
        assert!(md.contains("[Ana \\*López\\*](https://example.org/ana)"));
        assert!(md.contains("Autor desconocido"));
        assert!(md.contains("[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)"));
        assert!(md.contains("[Openverse](https://example.org/page)"));
        assert!(credits_markdown(&[]).contains("No hay imágenes"));
    }
}
