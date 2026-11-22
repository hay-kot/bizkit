//! Scoped API keys.

#[cfg(test)]
mod api_keys {
    use bizkit_testkit::{Persona, TestApp};
    use http::StatusCode;
    use serde_json::json;

    #[tokio::test]
    async fn a_read_key_can_list_but_not_create() {
        let app = TestApp::new().await;
        let seeded = app.seed(Persona::Team, "k1").await;
        let admin = app.with_session(&seeded.accounts["admin"].session_token);
        let ws = seeded.workspaces["main"].id;
        let created = admin
            .post(
                &format!("/api/v1/workspaces/{ws}/api-keys"),
                &json!({ "name": "ci", "scopes": ["projects:read"] }),
            )
            .await;
        created.expect(StatusCode::CREATED);
        let token = created["token"].as_str().unwrap();
        assert!(token.starts_with("bk_"), "token format");
        assert!(
            token.starts_with(created["key"]["prefix"].as_str().unwrap()),
            "prefix is the token's start"
        );

        let key = app.with_key(token);
        let list = key.get(&format!("/api/v1/workspaces/{ws}/projects")).await;
        list.expect(StatusCode::OK);
        assert_eq!(list["items"].as_array().unwrap().len(), 2, "seeded projects");
        key.post(
            &format!("/api/v1/workspaces/{ws}/projects"),
            &json!({ "name": "X", "slug": "x" }),
        )
        .await
        .expect(StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn keys_only_work_in_their_workspace() {
        let app = TestApp::new().await;
        let team = app.seed(Persona::Team, "k1").await;
        let other = app.seed(Persona::Basic, "k2").await;
        let key = app.with_key(&team.api_keys["full"].token);
        let theirs = other.workspaces["main"].id;
        key.get(&format!("/api/v1/workspaces/{theirs}/projects"))
            .await
            .expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn keys_cant_call_user_operations() {
        let app = TestApp::new().await;
        let team = app.seed(Persona::Team, "k1").await;
        app.with_key(&team.api_keys["full"].token)
            .get("/api/v1/me")
            .await
            .expect(StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn revoked_keys_stop_working() {
        let app = TestApp::new().await;
        let team = app.seed(Persona::Team, "k1").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let ws = team.workspaces["main"].id;
        let full = &team.api_keys["full"];
        let key = app.with_key(&full.token);
        key.get(&format!("/api/v1/workspaces/{ws}/projects")).await.expect(StatusCode::OK);
        admin
            .delete(&format!("/api/v1/workspaces/{ws}/api-keys/{}", full.id))
            .await
            .expect(StatusCode::NO_CONTENT);
        key.get(&format!("/api/v1/workspaces/{ws}/projects"))
            .await
            .expect(StatusCode::UNAUTHORIZED);
        let list = admin.get(&format!("/api/v1/workspaces/{ws}/api-keys")).await;
        assert!(list["items"][0]["revoked_at"].is_string(), "listed as revoked");
    }

    #[tokio::test]
    async fn unknown_scopes_are_validation_errors() {
        let app = TestApp::new().await;
        let team = app.seed(Persona::Team, "k1").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let ws = team.workspaces["main"].id;
        let response = admin
            .post(
                &format!("/api/v1/workspaces/{ws}/api-keys"),
                &json!({ "name": "x", "scopes": ["forms:read", "projects:own"] }),
            )
            .await;
        response.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let fields = response.body["error"]["fields"].as_object().unwrap();
        assert!(fields.contains_key("scopes[0]") && fields.contains_key("scopes[1]"), "{fields:?}");
    }

    #[tokio::test]
    async fn keys_with_bad_tokens_are_anonymous() {
        let app = TestApp::new().await;
        let team = app.seed(Persona::Team, "k1").await;
        let ws = team.workspaces["main"].id;
        app.with_key("bk_nope")
            .get(&format!("/api/v1/workspaces/{ws}/projects"))
            .await
            .expect(StatusCode::UNAUTHORIZED);
    }
}
