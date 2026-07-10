use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// Vite build output, embedded in release builds; read from disk in debug
/// builds (rust-embed default), which is the local dev loop.
#[derive(RustEmbed)]
#[folder = "web/dist"]
struct Assets;

pub async fn serve(uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let requested = if raw.is_empty() { "index.html" } else { raw };
    // Unknown paths fall back to the SPA entry point.
    let (path, file) = match Assets::get(requested) {
        Some(file) => (requested, file),
        None => match Assets::get("index.html") {
            Some(file) => ("index.html", file),
            None => {
                return StatusCode::NOT_FOUND.into_response();
            }
        },
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    ([(header::CONTENT_TYPE, mime.as_ref())], file.data.to_vec()).into_response()
}
