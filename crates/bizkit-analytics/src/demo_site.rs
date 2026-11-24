//! The demo product site for the dev stack, served by `bizkit dev demo-site`.
//!
//! It runs cross-origin to bizkit like a customer's site: landing, pricing,
//! signup and three onboarding steps, each tracked with the snippet.

use axum::Router;
use axum::http::header;
use axum::response::{Html, IntoResponse as _};
use axum::routing::get;

const MARKER: &str = "<!-- bizkit -->";
const PAGES: [(&str, &str); 7] = [
    ("/", include_str!("demo_site/index.html")),
    ("/pricing", include_str!("demo_site/pricing.html")),
    ("/signup", include_str!("demo_site/signup.html")),
    ("/onboarding/project", include_str!("demo_site/project.html")),
    ("/onboarding/invite", include_str!("demo_site/invite.html")),
    ("/onboarding/report", include_str!("demo_site/report.html")),
    ("/app", include_str!("demo_site/app.html")),
];
const SITE_JS: &str = include_str!("demo_site/site.js");
const SITE_CSS: &str = include_str!("demo_site/site.css");

/// The snippet tag for a site key, loading the snippet from `base`, the
/// URL bizkit serves its public routes on.
#[must_use]
pub fn snippet_tag(base: &url::Url, key: &str) -> String {
    let base = base.as_str().trim_end_matches('/');
    format!(r#"<script defer src="{base}/m/analytics/public/s/{key}.js"></script>"#)
}

/// The demo site's routes, with `snippet_tag` in every page's head.
fn demo_site_router(snippet_tag: &str) -> Router {
    let mut router = Router::new()
        .route("/site.js", get(|| async { ([(header::CONTENT_TYPE, "text/javascript")], SITE_JS) }))
        .route("/site.css", get(|| async { ([(header::CONTENT_TYPE, "text/css")], SITE_CSS) }));
    for (path, page) in PAGES {
        let html = page.replace(MARKER, snippet_tag);
        router = router.route(path, get(move || async move { Html(html).into_response() }));
    }
    router
}

/// Serves the demo site on `listener` until `shutdown` resolves.
///
/// # Errors
///
/// When serving fails.
pub async fn serve(
    listener: tokio::net::TcpListener,
    snippet_tag: &str,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, demo_site_router(snippet_tag)).with_graceful_shutdown(shutdown).await
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    use super::*;

    #[tokio::test]
    async fn every_page_carries_the_snippet() {
        let base = url::Url::parse("http://localhost:5203/").unwrap();
        let tag = snippet_tag(&base, "abc");
        assert_eq!(
            tag,
            r#"<script defer src="http://localhost:5203/m/analytics/public/s/abc.js"></script>"#
        );
        let router = demo_site_router(&tag);
        for (path, _) in PAGES {
            let response = router
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let html = String::from_utf8(bytes.to_vec()).unwrap();
            assert!(html.contains(&tag), "{path} has the snippet");
            assert!(!html.contains(MARKER), "{path} has no marker left");
        }
    }
}
