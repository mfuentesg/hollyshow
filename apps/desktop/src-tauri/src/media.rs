//! Open image bank commands (`media_*`) and the `hollyshow-media://` protocol that serves
//! downloaded images to the output windows.
//!
//! A theme background references an image by its media id (`Background::Image { media_id }`);
//! the renderer resolves it to `hollyshow-media://localhost/<id>` (`http://hollyshow-media.localhost/<id>`
//! on Windows) and this module answers from the local library, so a projector never needs the
//! network during a service.

use crate::state::AppState;
use hollyshow_core::WireError;
use hollyshow_media::{Category, LibraryItem, MediaResult, SearchRequest, SearchResponse};
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, State, UriSchemeContext, UriSchemeResponder};

pub const SCHEME: &str = "hollyshow-media";

fn err(code: &str, e: impl ToString) -> WireError {
    WireError {
        code: code.into(),
        message: e.to_string(),
    }
}

#[tauri::command]
pub async fn media_search(
    state: State<'_, AppState>,
    req: SearchRequest,
) -> Result<SearchResponse, WireError> {
    Ok(state.media.search(req).await)
}

#[tauri::command(async)]
pub fn media_categories(state: State<'_, AppState>) -> Vec<Category> {
    state.media.categories()
}

#[tauri::command]
pub async fn media_download(
    state: State<'_, AppState>,
    result: MediaResult,
    allow_noncommercial: Option<bool>,
) -> Result<LibraryItem, WireError> {
    let item = state
        .media
        .download(result, allow_noncommercial.unwrap_or(false))
        .await
        .map_err(|e| err("media", e))?;
    state
        .app
        .library_changed(hollyshow_core::LibraryScope::Media);
    Ok(item)
}

#[tauri::command(async)]
pub fn media_list(state: State<'_, AppState>) -> Result<Vec<LibraryItem>, WireError> {
    state.media.list().map_err(|e| err("media", e))
}

#[tauri::command(async)]
pub fn media_remove(state: State<'_, AppState>, id: String) -> Result<bool, WireError> {
    state.ensure_unlocked()?;
    let removed = state.media.remove(&id).map_err(|e| err("media", e))?;
    if removed {
        state
            .app
            .library_changed(hollyshow_core::LibraryScope::Media);
    }
    Ok(removed)
}

#[tauri::command(async)]
pub fn media_credits(
    state: State<'_, AppState>,
    ids: Option<Vec<String>>,
) -> Result<String, WireError> {
    state
        .media
        .credits(ids.as_deref())
        .map_err(|e| err("media", e))
}

// ---------------------------------------------------------------------------------------------
// Protocol
// ---------------------------------------------------------------------------------------------

/// Media id requested by a URL path (`/openverse-abc` -> `openverse-abc`). Percent escapes are
/// decoded; anything with a path separator is rejected so a URL can never leave the library.
pub fn id_from_path(path: &str) -> Option<String> {
    let raw = path.trim_start_matches('/');
    let mut out = Vec::with_capacity(raw.len());
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    let id = String::from_utf8(out).ok()?;
    let bad = id.is_empty() || id.contains(['/', '\\', '\0']) || id.contains("..");
    (!bad).then_some(id)
}

fn content_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "image/jpeg",
    }
}

fn reply(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        // The WebGL path reads pixels, which needs a CORS-clean image.
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// `hollyshow-media://localhost/<id>` -> the stored image.
pub fn handle<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let path = request.uri().path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let found = id_from_path(&path).and_then(|id| {
            let state = app.try_state::<AppState>()?;
            let item = state.media.library().get(&id).ok().flatten()?;
            let bytes = std::fs::read(&item.path).ok()?;
            Some((content_type(std::path::Path::new(&item.path)), bytes))
        });
        responder.respond(match found {
            Some((ct, bytes)) => reply(StatusCode::OK, ct, bytes),
            None => reply(StatusCode::NOT_FOUND, "text/plain", b"not found".to_vec()),
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_taken_from_the_path_and_never_escape_the_library() {
        assert_eq!(
            id_from_path("/openverse-abc123").as_deref(),
            Some("openverse-abc123")
        );
        assert_eq!(
            id_from_path("openverse-abc").as_deref(),
            Some("openverse-abc")
        );
        assert_eq!(id_from_path("/a%2Db").as_deref(), Some("a-b"));
        for bad in [
            "/",
            "",
            "/../etc/passwd",
            "/a/b",
            "/a%2Fb",
            "/a%5Cb",
            "/..",
            "/%zz",
            "/a%0",
        ] {
            assert_eq!(id_from_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn content_types_follow_the_extension() {
        let ct = |p: &str| content_type(std::path::Path::new(p));
        assert_eq!(ct("a.PNG"), "image/png");
        assert_eq!(ct("a.webp"), "image/webp");
        assert_eq!(ct("a.jpg"), "image/jpeg");
        assert_eq!(ct("a"), "image/jpeg");
    }

    #[test]
    fn replies_allow_cross_origin_reads() {
        let r = reply(StatusCode::OK, "image/png", vec![1]);
        assert_eq!(r.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    }
}
