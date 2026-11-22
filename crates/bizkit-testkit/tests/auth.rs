//! Sign-in, sessions, policies and the CSRF rule.

#[cfg(test)]
mod auth {
    use std::time::Duration;

    use bizkit_core::config::{JoinConfig, PolicyConfig};
    use bizkit_core::ops::Role;
    use bizkit_testkit::TestApp;
    use http::StatusCode;
    use serde_json::json;

    fn policy(name: &str, matches: &str, sign_up: bool, join: Vec<JoinConfig>) -> PolicyConfig {
        PolicyConfig { name: name.to_owned(), matches: vec![matches.to_owned()], sign_up, join }
    }

    #[tokio::test]
    async fn sign_in_through_the_mailer_sets_a_session() {
        let app = TestApp::new().await;
        let client = app.sign_in("a@example.com").await;
        assert!(client.session_token().is_some(), "a session cookie was set");
        let me = client.get("/api/v1/me").await;
        me.expect(StatusCode::OK);
        assert_eq!(me["email"], "a@example.com", "signed in as a");
    }

    #[tokio::test]
    async fn emails_are_normalized() {
        let app = TestApp::new().await;
        let client = app.sign_in("  A@Example.COM ").await;
        let mail = app.mail.sent();
        assert_eq!(mail.last().unwrap().to, "a@example.com", "mail goes to the normalized address");
        assert_eq!(client.get("/api/v1/me").await["email"], "a@example.com", "stored normalized");
    }

    #[tokio::test]
    async fn unmatched_email_gets_202_and_no_mail() {
        let app = TestApp::new().await;
        app.client()
            .post("/api/auth/sign-in", &json!({ "email": "a@nowhere.test" }))
            .await
            .expect(StatusCode::ACCEPTED);
        app.drain_jobs().await;
        assert!(app.mail.sent().is_empty(), "no mail for an email no policy admits");
    }

    #[tokio::test]
    async fn sign_up_false_sends_no_mail_for_unknown_emails() {
        let app = TestApp::builder()
            .config(|c| {
                c.auth.policies = vec![policy("closed", "*@example.com", false, Vec::new())];
            })
            .build()
            .await;
        assert!(app.request_link("new@example.com").await.is_none(), "no mail for a new email");
    }

    #[tokio::test]
    async fn bad_emails_are_rejected() {
        let app = TestApp::new().await;
        let response = app.client().post("/api/auth/sign-in", &json!({ "email": "nope" })).await;
        response.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.body["error"]["fields"]["email"][0], "must be an email address",
            "field error"
        );
    }

    #[tokio::test]
    async fn links_work_once() {
        let app = TestApp::new().await;
        let token = app.request_link("a@example.com").await.unwrap();
        app.verify(&token).await.unwrap();
        assert_eq!(
            app.verify(&token).await.unwrap_err(),
            StatusCode::UNAUTHORIZED,
            "a reused link is refused"
        );
    }

    #[tokio::test]
    async fn links_expire_after_15_minutes() {
        let app = TestApp::new().await;
        let token = app.request_link("a@example.com").await.unwrap();
        app.clock.advance(Duration::from_mins(15) + Duration::from_secs(1));
        assert_eq!(
            app.verify(&token).await.unwrap_err(),
            StatusCode::UNAUTHORIZED,
            "an expired link is refused"
        );
    }

    #[tokio::test]
    async fn a_second_link_within_a_minute_is_not_sent() {
        let app = TestApp::new().await;
        assert!(app.request_link("a@example.com").await.is_some(), "first link");
        assert!(app.request_link("a@example.com").await.is_none(), "cooldown");
        app.clock.advance(Duration::from_secs(61));
        assert!(app.request_link("a@example.com").await.is_some(), "after the cooldown");
    }

    #[tokio::test]
    async fn the_next_path_rides_along_in_the_link() {
        let app = TestApp::new().await;
        app.client()
            .post("/api/auth/sign-in", &json!({ "email": "a@example.com", "next": "/invite/abc" }))
            .await
            .expect(StatusCode::ACCEPTED);
        app.drain_jobs().await;
        let text = app.mail.last_to("a@example.com").unwrap().text;
        assert!(text.contains("next=%2Finvite%2Fabc"), "next is in the link: {text}");
    }

    #[tokio::test]
    async fn sign_out_ends_the_session() {
        let app = TestApp::new().await;
        let client = app.sign_in("a@example.com").await;
        client.post("/api/auth/sign-out", &json!({})).await.expect(StatusCode::NO_CONTENT);
        client.get("/api/v1/me").await.expect(StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn sessions_expire() {
        let app = TestApp::new().await;
        let client = app.sign_in("a@example.com").await;
        app.clock.advance(Duration::from_hours(31 * 24));
        client.get("/api/v1/me").await.expect(StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cross_site_cookie_posts_are_refused() {
        let app = TestApp::new().await;
        let client = app.sign_in("a@example.com").await;
        let evil = client.with_origin("https://evil.example");
        let response =
            evil.post("/api/v1/workspaces", &json!({ "name": "Acme", "slug": "acme" })).await;
        response.expect(StatusCode::FORBIDDEN);
        client
            .post("/api/v1/workspaces", &json!({ "name": "Acme", "slug": "acme" }))
            .await
            .expect(StatusCode::CREATED);
    }

    #[tokio::test]
    async fn policy_joins_add_new_accounts_to_existing_workspaces() {
        let joins = vec![
            JoinConfig { workspace: "acme".to_owned(), role: Role::Member },
            JoinConfig { workspace: "missing".to_owned(), role: Role::Admin },
        ];
        let app = TestApp::builder()
            .config(|c| {
                c.auth.policies = vec![
                    policy("founder", "founder@example.com", true, Vec::new()),
                    policy("team", "*@example.com", true, joins),
                ];
            })
            .build()
            .await;
        let founder = app.sign_in("founder@example.com").await;
        founder
            .post("/api/v1/workspaces", &json!({ "name": "Acme", "slug": "acme" }))
            .await
            .expect(StatusCode::CREATED);

        let member = app.sign_in("b@example.com").await;
        let me = member.get("/api/v1/me").await;
        let memberships = me["memberships"].as_array().unwrap();
        assert_eq!(memberships.len(), 1, "joined acme and skipped the missing slug");
        assert_eq!(memberships[0]["workspace"]["slug"], "acme", "acme");
        assert_eq!(memberships[0]["role"], "member", "with the policy's role");
    }

    #[tokio::test]
    async fn a_removed_member_stays_removed_after_signing_in_again() {
        let joins = vec![JoinConfig { workspace: "acme".to_owned(), role: Role::Member }];
        let app = TestApp::builder()
            .config(|c| {
                c.auth.policies = vec![
                    policy("founder", "founder@example.com", true, Vec::new()),
                    policy("team", "*@example.com", true, joins),
                ];
            })
            .build()
            .await;
        let founder = app.sign_in("founder@example.com").await;
        let workspace =
            founder.post("/api/v1/workspaces", &json!({ "name": "Acme", "slug": "acme" })).await;
        let ws = workspace["id"].as_str().unwrap().to_owned();
        let member = app.sign_in("b@example.com").await;
        let user_id = member.get("/api/v1/me").await["id"].as_str().unwrap().to_owned();

        founder
            .delete(&format!("/api/v1/workspaces/{ws}/members/{user_id}"))
            .await
            .expect(StatusCode::NO_CONTENT);
        app.clock.advance(Duration::from_secs(61));
        let again = app.sign_in("b@example.com").await;
        let me = again.get("/api/v1/me").await;
        assert!(
            me["memberships"].as_array().unwrap().is_empty(),
            "joins run only when the account is created"
        );
    }

    #[tokio::test]
    async fn anonymous_calls_to_user_ops_get_401() {
        let app = TestApp::new().await;
        let response = app.client().get("/api/v1/me").await;
        response.expect(StatusCode::UNAUTHORIZED);
        assert_eq!(response.error_code(), Some("unauthenticated"), "error code");
    }

    #[tokio::test]
    async fn api_typos_get_json_404s() {
        let app = TestApp::new().await;
        let response =
            app.client().send_unchecked(http::Method::GET, "/api/v2/nothing", None).await;
        assert_eq!(response.status, StatusCode::NOT_FOUND, "404");
        assert_eq!(response.error_code(), Some("not_found"), "JSON error body");
    }
}
