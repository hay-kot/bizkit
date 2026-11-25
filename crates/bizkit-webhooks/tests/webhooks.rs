//! Capture, the inspector, forwarding, replay and the hook point.

#[cfg(test)]
mod webhooks {
    use std::time::Duration;

    use bizkit_testkit::{FakeHooks, Manifest, Persona, RawBody, SpecClient, TestApp};
    use bizkit_webhooks::Webhooks;
    use bizkit_webhooks::capture::{KEEP_PER_ENDPOINT, MAX_BODY_BYTES, STORED_PER_HOUR};
    use bizkit_webhooks::hooks::REQUEST_RECEIVED;
    use http::{HeaderName, Method, StatusCode, header};
    use serde_json::{Value, json};

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        hooks: FakeHooks,
    }

    async fn setup() -> Setup {
        let hooks = FakeHooks::default();
        let app = TestApp::builder().module(Webhooks).hook_runner(hooks.clone()).build().await;
        let team = app.seed(Persona::Team, "hooks").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        Setup { app, team, admin, hooks }
    }

    fn id(value: &Value) -> &str {
        value["id"].as_str().unwrap()
    }

    impl Setup {
        fn endpoints(&self) -> String {
            format!("/api/v1/workspaces/{}/webhooks", self.team.workspaces["main"].id)
        }

        async fn endpoint(&self, extra: Value) -> Value {
            let mut body = json!({"name": "Stripe", "project_id": self.team.projects["site"].id});
            if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
                body.extend(extra.clone());
            }
            self.admin.post(&self.endpoints(), &body).await.expect(StatusCode::CREATED).json()
        }

        fn public(endpoint: &Value) -> String {
            format!("/m/webhooks/public/{}", id(endpoint))
        }

        async fn capture(&self, endpoint: &Value, raw: RawBody<'_>) -> bizkit_testkit::ApiResponse {
            self.app.client().send_body(Method::POST, &Self::public(endpoint), raw).await
        }

        async fn requests(&self, endpoint: &Value) -> Vec<Value> {
            let path = format!("{}/{}/requests?limit=200", self.endpoints(), id(endpoint));
            let list: Value = self.admin.get(&path).await.expect(StatusCode::OK).json();
            list["items"].as_array().unwrap().clone()
        }

        async fn request(&self, endpoint: &Value, request: &Value) -> Value {
            let path = format!("{}/{}/requests/{}", self.endpoints(), id(endpoint), id(request));
            self.admin.get(&path).await.expect(StatusCode::OK).json()
        }
    }

    fn json_body(text: &str) -> RawBody<'_> {
        RawBody::new("application/json", text.to_owned())
    }

    #[tokio::test]
    async fn capture_stores_the_request_without_credentials() {
        let s = setup().await;
        let endpoint = s.endpoint(json!({})).await;
        assert!(endpoint["url"].as_str().unwrap().ends_with(&Setup::public(&endpoint)), "url");

        let headers = [
            (header::AUTHORIZATION, "Bearer secret"),
            (header::COOKIE, "session=1"),
            (HeaderName::from_static("stripe-signature"), "t=1,v1=abc"),
        ];
        let raw = RawBody { headers: &headers, ..json_body(r#"{"type":"charge"}"#) };
        let path = format!("{}?a=1&b=2", Setup::public(&endpoint));
        let res = s.app.client().send_body(Method::PUT, &path, raw).await;
        assert_eq!((res.status, res.body.clone()), (StatusCode::OK, json!({"ok": true})), "stored");

        let list = s.requests(&endpoint).await;
        assert_eq!(list.len(), 1, "one request");
        assert_eq!(list[0]["method"], "PUT", "method");
        assert_eq!(list[0]["query"], "a=1&b=2", "query");
        assert_eq!(list[0]["size"], 17, "size");
        assert!(list[0].get("headers").is_none(), "summaries have no headers");

        let full = s.request(&endpoint, &list[0]).await;
        assert_eq!(full["body"], r#"{"type":"charge"}"#, "body");
        assert_eq!(full["body_encoding"], "utf8", "encoding");
        let names: Vec<&str> = full["headers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"stripe-signature"), "signature kept: {names:?}");
        for dropped in ["authorization", "cookie", "x-request-id", "content-length"] {
            assert!(!names.contains(&dropped), "{dropped} dropped: {names:?}");
        }

        let endpoint: Value = s
            .admin
            .get(&format!("{}/{}", s.endpoints(), id(&endpoint)))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(endpoint["request_count"], 1, "count");
        assert!(endpoint["last_received_at"].is_string(), "last received");
    }

    #[tokio::test]
    async fn binary_bodies_come_back_as_base64() {
        let s = setup().await;
        let endpoint = s.endpoint(json!({})).await;
        let res = s
            .capture(&endpoint, RawBody::new("application/octet-stream", vec![0xff, 0x00, 0xfe]))
            .await;
        res.expect(StatusCode::OK);
        let list = s.requests(&endpoint).await;
        let full = s.request(&endpoint, &list[0]).await;
        assert_eq!(
            (full["body"].as_str(), full["body_encoding"].as_str()),
            (Some("/wD+"), Some("base64")),
            "base64"
        );
        assert_eq!(full["size"], 3, "size counts bytes");
    }

    #[tokio::test]
    async fn a_workspace_with_webhooks_off_captures_nothing() {
        let s = setup().await;
        let endpoint = s.endpoint(json!({})).await;
        s.app.turn_off(s.team.workspaces["main"].id, "webhooks").await;
        assert_eq!(
            s.capture(&endpoint, json_body("{}")).await.status,
            StatusCode::NOT_FOUND,
            "as if the endpoint didn't exist"
        );
    }

    #[tokio::test]
    async fn unknown_disabled_and_oversized_requests_are_refused() {
        let s = setup().await;
        let endpoint = s.endpoint(json!({})).await;
        let missing = s
            .app
            .client()
            .send_body(Method::POST, "/m/webhooks/public/nope", json_body("{}"))
            .await;
        assert_eq!(missing.status, StatusCode::NOT_FOUND, "bad id");

        let too_big = RawBody::new("text/plain", "a".repeat(MAX_BODY_BYTES + 1));
        assert_eq!(
            s.capture(&endpoint, too_big).await.status,
            StatusCode::PAYLOAD_TOO_LARGE,
            "413"
        );

        let path = format!("{}/{}", s.endpoints(), id(&endpoint));
        s.admin.patch(&path, &json!({"enabled": false})).await.expect(StatusCode::OK);
        assert_eq!(
            s.capture(&endpoint, json_body("{}")).await.status,
            StatusCode::NOT_FOUND,
            "disabled"
        );
        assert!(s.requests(&endpoint).await.is_empty(), "nothing stored");
    }

    #[tokio::test]
    async fn the_hourly_cap_answers_429_and_old_requests_are_pruned() {
        let s = setup().await;
        let endpoint = s.endpoint(json!({})).await;
        s.capture(&endpoint, json_body("{}")).await.expect(StatusCode::OK);
        let first = s.requests(&endpoint).await[0].clone();
        for _ in 1..STORED_PER_HOUR {
            s.capture(&endpoint, json_body("{}")).await.expect(StatusCode::OK);
        }
        let limited = s.capture(&endpoint, json_body("{}")).await;
        assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS, "capped");
        assert_eq!(limited.headers[header::RETRY_AFTER], "60", "retry after");

        s.app.clock.advance(Duration::from_mins(61));
        let more = usize::try_from(KEEP_PER_ENDPOINT).unwrap()
            - usize::try_from(STORED_PER_HOUR).unwrap()
            + 1;
        for _ in 0..more {
            s.capture(&endpoint, json_body("{}")).await.expect(StatusCode::OK);
        }
        let path = format!("{}/{}", s.endpoints(), id(&endpoint));
        let got: Value = s.admin.get(&path).await.expect(StatusCode::OK).json();
        assert_eq!(got["request_count"], KEEP_PER_ENDPOINT, "kept");
        let gone = format!("{}/{}/requests/{}", s.endpoints(), id(&endpoint), id(&first));
        s.admin.get(&gone).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn captures_forward_with_their_headers_and_record_the_delivery() {
        let s = setup().await;
        let target = "https://hooks.example.com/in";
        s.app.http.respond(Method::PATCH, target, StatusCode::NO_CONTENT, "");
        let endpoint = s.endpoint(json!({"forward_url": target})).await;
        assert_eq!(endpoint["forward_url"], target, "forward url");

        let headers = [
            (header::AUTHORIZATION, "Bearer secret"),
            (HeaderName::from_static("x-hub-signature-256"), "sha256=abc"),
        ];
        let raw = RawBody { headers: &headers, ..json_body(r#"{"a":1}"#) };
        s.app
            .client()
            .send_body(Method::PATCH, &Setup::public(&endpoint), raw)
            .await
            .expect(StatusCode::OK);
        s.app.drain_jobs().await;

        let sent = s.app.http.requests();
        assert_eq!(sent.len(), 1, "forwarded once");
        assert_eq!(
            (&sent[0].0, sent[0].1.as_str(), &sent[0].2[..]),
            (&Method::PATCH, target, &br#"{"a":1}"#[..]),
            "request"
        );
        let sent_headers = &s.app.http.request_headers()[0];
        assert_eq!(sent_headers["x-hub-signature-256"], "sha256=abc", "signature forwarded");
        assert_eq!(sent_headers[header::CONTENT_TYPE], "application/json", "content type");
        assert!(sent_headers.get(header::AUTHORIZATION).is_none(), "no credentials");

        let list = s.requests(&endpoint).await;
        let delivery = &list[0]["delivery"];
        assert_eq!(
            (delivery["status"].as_u64(), delivery["error"].as_str()),
            (Some(204), None),
            "{delivery}"
        );
    }

    #[tokio::test]
    async fn failed_forwards_retry_on_server_errors_only() {
        let s = setup().await;
        let target = "https://hooks.example.com/in";
        s.app.http.respond(Method::POST, target, StatusCode::BAD_GATEWAY, "");
        let endpoint = s.endpoint(json!({"forward_url": target})).await;
        s.capture(&endpoint, json_body("{}")).await.expect(StatusCode::OK);
        for _ in 0..4 {
            s.app.drain_jobs().await;
            s.app.clock.advance(Duration::from_hours(1));
        }
        assert_eq!(s.app.http.requests().len(), 3, "three attempts");
        let delivery = s.requests(&endpoint).await[0]["delivery"].clone();
        assert_eq!(delivery["error"], "the target answered 502", "{delivery}");

        s.app.http.respond(Method::POST, target, StatusCode::NOT_FOUND, "");
        s.capture(&endpoint, json_body("{}")).await.expect(StatusCode::OK);
        for _ in 0..3 {
            s.app.drain_jobs().await;
            s.app.clock.advance(Duration::from_hours(1));
        }
        assert_eq!(s.app.http.requests().len(), 4, "a 4xx is final");
    }

    #[tokio::test]
    async fn replay_sends_to_a_url_or_the_forward_url() {
        let s = setup().await;
        let endpoint = s.endpoint(json!({})).await;
        s.capture(&endpoint, json_body(r#"{"n":1}"#)).await.expect(StatusCode::OK);
        let request = s.requests(&endpoint).await[0].clone();
        let replay =
            format!("{}/{}/requests/{}/replay", s.endpoints(), id(&endpoint), id(&request));

        assert_eq!(
            s.admin.post(&replay, &json!({})).await.status,
            StatusCode::CONFLICT,
            "no target"
        );
        let private = s.admin.post(&replay, &json!({"url": "http://127.0.0.1/hook"})).await;
        assert_eq!(private.status, StatusCode::UNPROCESSABLE_ENTITY, "private address");

        s.app.http.respond(Method::POST, "https://other.example.com/", StatusCode::NOT_FOUND, "");
        let delivery: Value = s
            .admin
            .post(&replay, &json!({"url": "https://other.example.com/hook"}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(
            (delivery["status"].as_u64(), delivery["error"].as_str()),
            (Some(404), Some("the target answered 404")),
            "{delivery}"
        );
        assert_eq!(
            s.request(&endpoint, &request).await["delivery"]["url"],
            "https://other.example.com/hook",
            "recorded"
        );

        let path = format!("{}/{}", s.endpoints(), id(&endpoint));
        s.admin
            .patch(&path, &json!({"forward_url": "https://hooks.example.com/in"}))
            .await
            .expect(StatusCode::OK);
        s.app.http.respond(Method::POST, "https://hooks.example.com/", StatusCode::OK, "");
        let forwarded: Value =
            s.admin.post(&replay, &json!({})).await.expect(StatusCode::OK).json();
        assert_eq!(forwarded["status"], 200, "forward url used");

        let cleared: Value =
            s.admin.patch(&path, &json!({"forward_url": null})).await.expect(StatusCode::OK).json();
        assert_eq!(cleared["forward_url"], Value::Null, "null clears");
    }

    #[tokio::test]
    async fn captures_dispatch_the_hook_for_the_endpoint() {
        let s = setup().await;
        let endpoint = s.endpoint(json!({})).await;
        s.capture(&endpoint, json_body("{}")).await.expect(StatusCode::OK);
        s.app.drain_jobs().await;
        let dispatches = s.hooks.dispatches();
        assert_eq!(dispatches.len(), 1, "{dispatches:?}");
        assert_eq!(dispatches[0].point, REQUEST_RECEIVED.id, "point");
        assert_eq!(dispatches[0].target.entity.as_deref(), Some(id(&endpoint)), "entity");
    }

    #[tokio::test]
    async fn deleting_an_endpoint_deletes_its_requests() {
        let s = setup().await;
        let endpoint = s.endpoint(json!({})).await;
        s.capture(&endpoint, json_body("{}")).await.expect(StatusCode::OK);
        let path = format!("{}/{}", s.endpoints(), id(&endpoint));
        s.admin.delete(&path).await.expect(StatusCode::NO_CONTENT);
        s.admin.get(&format!("{path}/requests")).await.expect(StatusCode::NOT_FOUND);
        assert_eq!(
            s.capture(&endpoint, json_body("{}")).await.status,
            StatusCode::NOT_FOUND,
            "gone"
        );
    }

    #[tokio::test]
    async fn forward_urls_must_be_public_http() {
        let s = setup().await;
        let body = json!({"name": "X", "project_id": s.team.projects["site"].id, "forward_url": "ftp://example.com/x"});
        s.admin.post(&s.endpoints(), &body).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let private = json!({"name": "X", "project_id": s.team.projects["site"].id, "forward_url": "http://10.0.0.1/x"});
        s.admin.post(&s.endpoints(), &private).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
    }
}
