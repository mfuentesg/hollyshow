//! The embedded remote web app (`apps/remote/dist`, staged by `build.rs`).

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$OUT_DIR/remote"]
struct Remote;

/// Serves a file of the SPA, falling back to `index.html` for client-side routes. Unknown paths
/// that look like files (have an extension) are a real 404 so broken asset links are visible.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = Remote::get(path) {
        return respond(path, file.data.into_owned());
    }
    if path.rsplit('/').next().is_some_and(|f| f.contains('.')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match Remote::get("index.html") {
        Some(index) => respond("index.html", index.data.into_owned()),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn respond(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // Hashed assets never change; the shell page must always be revalidated.
    let cache = if path == "index.html" {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };
    (
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CACHE_CONTROL, cache.to_owned()),
        ],
        Body::from(bytes),
    )
        .into_response()
}
