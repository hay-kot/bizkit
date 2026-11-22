//! Module public routes sit outside authentication.

#[cfg(test)]
mod public_routes {
    use axum::Router;
    use axum::routing::{get, post};
    use bizkit_core::{Authn, Module, ModuleContext};
    use bizkit_testkit::{Persona, RawBody, TestApp};
    use http::{Method, StatusCode};
    use utoipa::OpenApi;

    /// Answers with whether the request carried an `Authn`, and whose.
    async fn who(authn: Option<axum::Extension<Authn>>) -> String {
        match authn {
            None => "no authn".to_owned(),
            Some(axum::Extension(authn)) => format!("authn: {}", authn.principal.is_some()),
        }
    }

    #[utoipa::path(post, path = "/hit", responses((status = 200, body = String, content_type = "text/plain")))]
    #[expect(dead_code, reason = "exists for its utoipa docs")]
    fn hit_docs() {}

    #[utoipa::path(get, path = "/me", responses((status = 200, body = String, content_type = "text/plain")))]
    #[expect(dead_code, reason = "exists for its utoipa docs")]
    fn me_docs() {}

    #[derive(OpenApi)]
    #[openapi(paths(hit_docs))]
    struct PublicDocs;

    #[derive(OpenApi)]
    #[openapi(paths(me_docs))]
    struct PrivateDocs;

    #[derive(Debug)]
    struct Probe;

    impl Module for Probe {
        fn name(&self) -> &'static str {
            "probe"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.routes(Router::new().route("/me", get(who)), PrivateDocs::openapi())?;
            cx.public_routes(Router::new().route("/hit", post(who)), PublicDocs::openapi())
        }
    }

    #[tokio::test]
    async fn public_routes_ignore_cookies_and_keys() {
        let app = TestApp::builder().module(Probe).build().await;
        let team = app.seed(Persona::Team, "pub").await;
        let session = app.with_session(&team.accounts["admin"].session_token);

        // A cross-site cookie POST would get 403 anywhere else.
        let cross_site = session.with_origin("https://elsewhere.example");
        let with_cookie = cross_site
            .send_body(Method::POST, "/m/probe/public/hit", RawBody::new("text/plain", ""))
            .await;
        assert_eq!(
            (with_cookie.status, with_cookie.text.as_str()),
            (StatusCode::OK, "no authn"),
            "cookie ignored"
        );

        let key = app.with_key(&team.api_keys["full"].token);
        let with_key = key
            .send_body(Method::POST, "/m/probe/public/hit", RawBody::new("text/plain", ""))
            .await;
        assert_eq!(with_key.text, "no authn", "bearer ignored");

        let private = session.get("/m/probe/me").await;
        assert_eq!(private.text, "authn: true", "private routes still authenticate");
    }

    #[test]
    fn public_docs_nest_under_public() {
        let spec = serde_json::to_value(bizkit_core::openapi(&[Box::new(Probe)]).unwrap()).unwrap();
        assert!(spec["paths"]["/m/probe/public/hit"]["post"].is_object(), "public path documented");
        assert!(spec["paths"]["/m/probe/me"]["get"].is_object(), "private path documented");
    }
}
