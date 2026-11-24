//! Module upload routes take large bodies inside authentication, while
//! operations keep their 1 MiB limit.

#[cfg(test)]
mod upload_routes {
    use axum::extract::{Multipart, Path};
    use axum::response::{IntoResponse as _, Response};
    use axum::routing::post;
    use axum::{Extension, Router};
    use bizkit_core::error::ErrorBody;
    use bizkit_core::ops::{Level, Scope, authorize_workspace};
    use bizkit_core::{ApiError, Authn, MAX_UPLOAD_BYTES, Module, ModuleContext, Platform};
    use bizkit_testkit::{Persona, RawBody, SpecClient, TestApp};
    use http::{Method, StatusCode};
    use serde_json::json;
    use utoipa::OpenApi;

    const WRITE: Scope = Scope::new("probe", Level::Write);
    const BOUNDARY: &str = "probe-boundary";
    const FIVE_MIB: usize = 5 * 1024 * 1024;

    async fn upload(
        Extension(platform): Extension<Platform>,
        Extension(authn): Extension<Authn>,
        Path(workspace_id): Path<String>,
        mut multipart: Multipart,
    ) -> Response {
        if let Err(err) =
            authorize_workspace(&platform, authn.principal.as_ref(), &workspace_id, &WRITE).await
        {
            return err.into_response();
        }
        loop {
            match multipart.next_field().await {
                Ok(Some(field)) if field.name() == Some("file") => {
                    return match field.bytes().await {
                        Ok(bytes) => (StatusCode::CREATED, bytes.len().to_string()).into_response(),
                        Err(err) => err.into_response(),
                    };
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    return ApiError::BadRequest("no file part".to_owned()).into_response();
                }
                Err(err) => return err.into_response(),
            }
        }
    }

    #[utoipa::path(
        post,
        path = "/workspaces/{workspace_id}/files",
        params(("workspace_id" = String, Path, description = "The workspace")),
        request_body(content = String, content_type = "multipart/form-data"),
        responses(
            (status = 201, description = "The file's length", content_type = "text/plain", body = String),
            (status = 400, description = "No file part", body = ErrorBody),
            (status = 401, description = "No caller", body = ErrorBody),
            (status = 403, description = "Cross-site or missing scope", body = ErrorBody),
            (status = 404, description = "Not a member", body = ErrorBody),
            (status = 408, description = "Timed out"),
            (status = 413, description = "Too large", content_type = "text/plain", body = String),
            (status = 422, description = "Bad multipart", content_type = "text/plain", body = String),
        )
    )]
    #[expect(dead_code, reason = "exists for its utoipa docs")]
    fn upload_docs() {}

    #[derive(OpenApi)]
    #[openapi(paths(upload_docs))]
    struct UploadDocs;

    #[derive(Debug)]
    struct Probe;

    impl Module for Probe {
        fn name(&self) -> &'static str {
            "probe"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.scope_area("probe", "Probe files")?;
            cx.upload_routes(
                Router::new().route("/workspaces/{workspace_id}/files", post(upload)),
                UploadDocs::openapi(),
            )
        }
    }

    fn multipart(file_len: usize) -> RawBody<'static> {
        let mut body = format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes();
        body.resize(body.len() + file_len, b'x');
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
        RawBody::new("multipart/form-data; boundary=probe-boundary", body)
    }

    async fn send(
        client: &SpecClient,
        workspace_id: &str,
        file_len: usize,
    ) -> (StatusCode, String) {
        let path = format!("/m/probe/upload/workspaces/{workspace_id}/files");
        let response = client.send_body(Method::POST, &path, multipart(file_len)).await;
        (response.status, response.text)
    }

    /// Posts a multipart file with a `Content-Length`. The in-process
    /// transport sends none on its own, and real clients do; without it, ops
    /// read the body themselves and answer 400 rather than 413.
    async fn post_declared(
        client: &SpecClient,
        path: &str,
        file_len: usize,
        checked: bool,
    ) -> StatusCode {
        let raw = multipart(file_len);
        let length = raw.body.len().to_string();
        let headers = [(http::header::CONTENT_LENGTH, length.as_str())];
        let raw = RawBody { headers: &headers, ..raw };
        let response = if checked {
            client.send_body(Method::POST, path, raw).await
        } else {
            client.send_body_unchecked(Method::POST, path, raw).await
        };
        response.status
    }

    #[tokio::test]
    async fn uploads_take_large_bodies_and_ops_dont() {
        let app = TestApp::builder().module(Probe).build().await;
        let team = app.seed(Persona::Team, "up").await;
        let ws = team.workspaces["main"].id.to_string();
        let session = app.with_session(&team.accounts["member"].session_token);

        assert_eq!(
            send(&session, &ws, FIVE_MIB).await,
            (StatusCode::CREATED, FIVE_MIB.to_string()),
            "5 MiB upload"
        );

        let op =
            post_declared(&session, &format!("/api/v1/workspaces/{ws}/projects"), FIVE_MIB, false)
                .await;
        assert_eq!(op, StatusCode::PAYLOAD_TOO_LARGE, "ops keep 1 MiB");

        let too_big = MAX_UPLOAD_BYTES + 64 * 1024;
        let (streamed, _) = send(&session, &ws, too_big).await;
        assert_eq!(streamed, StatusCode::PAYLOAD_TOO_LARGE, "over the upload limit, streamed");
        let path = format!("/m/probe/upload/workspaces/{ws}/files");
        let declared = post_declared(&session, &path, too_big, true).await;
        assert_eq!(declared, StatusCode::PAYLOAD_TOO_LARGE, "over the upload limit, by length");
    }

    #[tokio::test]
    async fn uploads_check_csrf_membership_and_scope() {
        let app = TestApp::builder().module(Probe).build().await;
        let team = app.seed(Persona::Team, "up").await;
        let other = app.seed(Persona::Basic, "up2").await;
        let ws = team.workspaces["main"].id.to_string();
        let session = app.with_session(&team.accounts["admin"].session_token);

        // The client always sends an Origin on POST; an empty one is no origin.
        let no_origin = session.with_origin("");
        assert_eq!(send(&no_origin, &ws, 10).await.0, StatusCode::FORBIDDEN, "CSRF");

        let outsider = app.with_session(&other.accounts["admin"].session_token);
        assert_eq!(send(&outsider, &ws, 10).await.0, StatusCode::NOT_FOUND, "non-member");

        assert_eq!(send(&app.client(), &ws, 10).await.0, StatusCode::UNAUTHORIZED, "anonymous");

        let created = session
            .post(
                &format!("/api/v1/workspaces/{ws}/api-keys"),
                &json!({ "name": "read", "scopes": ["probe:read"] }),
            )
            .await;
        let reader = app.with_key(created["token"].as_str().unwrap());
        assert_eq!(send(&reader, &ws, 10).await.0, StatusCode::FORBIDDEN, "missing probe:write");

        let full = app.with_key(&team.api_keys["full"].token);
        assert_eq!(send(&full, &ws, 10).await.0, StatusCode::CREATED, "key with probe:write");
    }

    #[test]
    fn upload_docs_nest_under_upload() {
        let spec = serde_json::to_value(bizkit_core::openapi(&[Box::new(Probe)]).unwrap()).unwrap();
        assert!(
            spec["paths"]["/m/probe/upload/workspaces/{workspace_id}/files"]["post"].is_object(),
            "upload path documented"
        );
    }

    #[test]
    fn upload_routes_register_once() {
        #[derive(Debug)]
        struct Twice;

        impl Module for Twice {
            fn name(&self) -> &'static str {
                "twice"
            }

            fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
                cx.upload_routes(Router::new(), UploadDocs::openapi())?;
                cx.upload_routes(Router::new(), UploadDocs::openapi())
            }
        }

        let Err(err) = bizkit_core::openapi(&[Box::new(Twice)]) else { panic!("registered twice") };
        assert!(
            format!("{err:#}").contains("module twice registered upload routes twice"),
            "{err:#}"
        );
    }
}
