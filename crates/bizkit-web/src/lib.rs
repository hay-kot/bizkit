//! The bizkit web app, embedded in the binary. Debug builds read `web/dist`
//! from disk, so `mise run web:build` shows up without a recompile.

use std::borrow::Cow;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse as _, Response};

#[expect(
    clippy::same_name_method,
    reason = "the RustEmbed derive defines inherent and trait `get`/`iter`"
)]
mod embedded {
    #[derive(rust_embed::RustEmbed)]
    #[folder = "../../web/dist"]
    #[allow_missing = true]
    pub(super) struct Assets;
}

use embedded::Assets;

/// Serves a built asset, or `index.html` for any other path so the SPA router
/// can handle it. Used as the app's fallback.
pub fn router() -> Router {
    Router::new().fallback(serve)
}

async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty()
        && let Some(file) = Assets::get(path)
    {
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return file_response(path, file.data, cache);
    }
    match Assets::get("index.html") {
        Some(index) => file_response("index.html", index.data, "no-cache"),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "The web app was not built into this binary. Run `mise run web:build` and rebuild.",
        )
            .into_response(),
    }
}

fn file_response(path: &str, data: Cow<'static, [u8]>, cache: &'static str) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut response = Body::from(data).into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(mime.as_ref()) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

#[cfg(test)]
mod tests {
    use axum::http::Request;
    use tower::ServiceExt as _;

    use super::*;

    #[tokio::test]
    async fn unknown_paths_get_the_app_or_a_503() {
        let response = router()
            .oneshot(Request::builder().uri("/w/acme/projects").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let has_index = Assets::get("index.html").is_some();
        let expected = if has_index { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
        assert_eq!(response.status(), expected, "SPA routes fall back to index.html");
    }
}
