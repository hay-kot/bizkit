//! Workspaces, members, invitations and projects.

#[cfg(test)]
mod tenancy {
    use std::time::Duration;

    use bizkit_testkit::{Persona, SpecClient, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    async fn workspace(client: &SpecClient, slug: &str) -> String {
        let response =
            client.post("/api/v1/workspaces", &json!({ "name": "Acme", "slug": slug })).await;
        response.expect(StatusCode::CREATED);
        response["id"].as_str().unwrap().to_owned()
    }

    async fn user_id(client: &SpecClient) -> String {
        client.get("/api/v1/me").await["id"].as_str().unwrap().to_owned()
    }

    fn invite_id(app: &TestApp, email: &str) -> String {
        let text = app.mail.last_to(email).expect("an invite mail").text;
        let link =
            text.split_whitespace().find(|w| w.contains("/invite/")).expect("an invite link");
        link.rsplit('/').next().unwrap().to_owned()
    }

    #[tokio::test]
    async fn creating_a_workspace_makes_you_its_admin() {
        let app = TestApp::new().await;
        let a = app.sign_in("a@example.com").await;
        let ws = workspace(&a, "acme").await;
        let list = a.get("/api/v1/workspaces").await;
        list.expect(StatusCode::OK);
        assert_eq!(list["items"][0]["workspace"]["id"], ws.as_str(), "listed");
        assert_eq!(list["items"][0]["role"], "admin", "creator is admin");
    }

    #[tokio::test]
    async fn taken_slugs_conflict() {
        let app = TestApp::new().await;
        let a = app.sign_in("a@example.com").await;
        workspace(&a, "acme").await;
        let b = app.sign_in("b@example.com").await;
        let response =
            b.post("/api/v1/workspaces", &json!({ "name": "Other", "slug": "acme" })).await;
        response.expect(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn bad_slugs_are_validation_errors() {
        let app = TestApp::new().await;
        let a = app.sign_in("a@example.com").await;
        let response =
            a.post("/api/v1/workspaces", &json!({ "name": "", "slug": "Bad Slug" })).await;
        response.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let fields = response.body["error"]["fields"].as_object().unwrap();
        assert!(
            fields.contains_key("name") && fields.contains_key("slug"),
            "both fields: {fields:?}"
        );
    }

    #[tokio::test]
    async fn an_outside_email_signs_up_through_an_invitation() {
        let app = TestApp::new().await;
        let admin = app.sign_in("a@example.com").await;
        let ws = workspace(&admin, "acme").await;
        admin
            .post(
                &format!("/api/v1/workspaces/{ws}/invitations"),
                &json!({ "email": "b@outside.test", "role": "admin" }),
            )
            .await
            .expect(StatusCode::CREATED);
        app.drain_jobs().await;
        let id = invite_id(&app, "b@outside.test");

        let b = app.sign_in("b@outside.test").await;
        let mine = b.get("/api/v1/me/invitations").await;
        mine.expect(StatusCode::OK);
        assert_eq!(mine["items"][0]["id"], id.as_str(), "b sees the invite");
        assert_eq!(mine["items"][0]["invited_by"], "a@example.com", "from a");

        let accepted = b.post(&format!("/api/v1/me/invitations/{id}/accept"), &json!({})).await;
        accepted.expect(StatusCode::OK);
        assert_eq!(accepted["role"], "admin", "the invited role");
        let members = admin.get(&format!("/api/v1/workspaces/{ws}/members")).await;
        let emails: Vec<&str> = members["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["email"].as_str().unwrap())
            .collect();
        assert_eq!(emails, ["a@example.com", "b@outside.test"], "b joined");
        assert!(
            b.get("/api/v1/me/invitations").await["items"].as_array().unwrap().is_empty(),
            "no longer open"
        );
    }

    #[tokio::test]
    async fn only_the_invited_email_can_accept() {
        let app = TestApp::new().await;
        let admin = app.sign_in("a@example.com").await;
        let ws = workspace(&admin, "acme").await;
        admin
            .post(
                &format!("/api/v1/workspaces/{ws}/invitations"),
                &json!({ "email": "b@example.com", "role": "member" }),
            )
            .await
            .expect(StatusCode::CREATED);
        app.drain_jobs().await;
        let id = invite_id(&app, "b@example.com");
        let c = app.sign_in("c@example.com").await;
        c.post(&format!("/api/v1/me/invitations/{id}/accept"), &json!({}))
            .await
            .expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn revoked_and_expired_invitations_cant_be_accepted() {
        let app = TestApp::new().await;
        let admin = app.sign_in("a@example.com").await;
        let ws = workspace(&admin, "acme").await;
        for email in ["b@example.com", "c@example.com"] {
            admin
                .post(
                    &format!("/api/v1/workspaces/{ws}/invitations"),
                    &json!({ "email": email, "role": "member" }),
                )
                .await
                .expect(StatusCode::CREATED);
        }
        app.drain_jobs().await;
        let revoked = invite_id(&app, "b@example.com");
        let expiring = invite_id(&app, "c@example.com");
        admin
            .delete(&format!("/api/v1/workspaces/{ws}/invitations/{revoked}"))
            .await
            .expect(StatusCode::NO_CONTENT);
        let b = app.sign_in("b@example.com").await;
        b.post(&format!("/api/v1/me/invitations/{revoked}/accept"), &json!({}))
            .await
            .expect(StatusCode::NOT_FOUND);

        app.clock.advance(Duration::from_hours(7 * 24 + 1));
        let c = app.sign_in("c@example.com").await;
        c.post(&format!("/api/v1/me/invitations/{expiring}/accept"), &json!({}))
            .await
            .expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_expired_invite_can_be_sent_again() {
        let app = TestApp::new().await;
        let admin = app.sign_in("a@example.com").await;
        let ws = workspace(&admin, "acme").await;
        let body = json!({ "email": "b@outside.test", "role": "member" });
        let path = format!("/api/v1/workspaces/{ws}/invitations");
        admin.post(&path, &body).await.expect(StatusCode::CREATED);
        admin.post(&path, &body).await.expect(StatusCode::CONFLICT);
        app.clock.advance(Duration::from_hours(7 * 24 + 1));
        admin.post(&path, &body).await.expect(StatusCode::CREATED);
    }

    #[tokio::test]
    async fn inviting_a_member_conflicts() {
        let app = TestApp::new().await;
        let admin = app.sign_in("a@example.com").await;
        let ws = workspace(&admin, "acme").await;
        let response = admin
            .post(
                &format!("/api/v1/workspaces/{ws}/invitations"),
                &json!({ "email": "A@example.com", "role": "member" }),
            )
            .await;
        response.expect(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_revoked_invite_no_longer_admits_its_email() {
        let app = TestApp::new().await;
        let admin = app.sign_in("a@example.com").await;
        let ws = workspace(&admin, "acme").await;
        admin
            .post(
                &format!("/api/v1/workspaces/{ws}/invitations"),
                &json!({ "email": "b@outside.test", "role": "member" }),
            )
            .await
            .expect(StatusCode::CREATED);
        app.drain_jobs().await;
        let id = invite_id(&app, "b@outside.test");
        let token = app.request_link("b@outside.test").await.expect("the invitation admits b");
        admin
            .delete(&format!("/api/v1/workspaces/{ws}/invitations/{id}"))
            .await
            .expect(StatusCode::NO_CONTENT);
        assert_eq!(
            app.verify(&token).await.unwrap_err(),
            StatusCode::UNAUTHORIZED,
            "verify re-checks admission"
        );
    }

    #[tokio::test]
    async fn members_of_one_workspace_get_404_in_another() {
        let app = TestApp::new().await;
        let a = app.sign_in("a@example.com").await;
        let b = app.sign_in("b@example.com").await;
        let theirs = workspace(&b, "theirs").await;
        workspace(&a, "mine").await;
        for path in [
            format!("/api/v1/workspaces/{theirs}"),
            format!("/api/v1/workspaces/{theirs}/members"),
            format!("/api/v1/workspaces/{theirs}/projects"),
            format!("/api/v1/workspaces/{theirs}/invitations"),
            format!("/api/v1/workspaces/{theirs}/api-keys"),
        ] {
            a.get(&path).await.expect(StatusCode::NOT_FOUND);
        }
        a.post(
            &format!("/api/v1/workspaces/{theirs}/projects"),
            &json!({ "name": "X", "slug": "x" }),
        )
        .await
        .expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn members_cant_do_admin_things() {
        let app = TestApp::new().await;
        let seeded = app.seed(Persona::Team, "t1").await;
        let member = app.with_session(&seeded.accounts["member"].session_token);
        let ws = seeded.workspaces["main"].id;
        member.get(&format!("/api/v1/workspaces/{ws}/projects")).await.expect(StatusCode::OK);
        member
            .get(&format!("/api/v1/workspaces/{ws}/invitations"))
            .await
            .expect(StatusCode::FORBIDDEN);
        member
            .patch(&format!("/api/v1/workspaces/{ws}"), &json!({ "name": "Mine" }))
            .await
            .expect(StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn the_last_admin_cant_leave_or_be_demoted() {
        let app = TestApp::new().await;
        let admin = app.sign_in("a@example.com").await;
        let ws = workspace(&admin, "acme").await;
        let me = user_id(&admin).await;
        let path = format!("/api/v1/workspaces/{ws}/members/{me}");
        admin.delete(&path).await.expect(StatusCode::CONFLICT);
        admin.patch(&path, &json!({ "role": "member" })).await.expect(StatusCode::CONFLICT);
        admin.patch(&path, &json!({ "role": "admin" })).await.expect(StatusCode::OK);
    }

    #[tokio::test]
    async fn projects_round_trip() {
        let app = TestApp::new().await;
        let admin = app.sign_in("a@example.com").await;
        let ws = workspace(&admin, "acme").await;
        let created = admin
            .post(
                &format!("/api/v1/workspaces/{ws}/projects"),
                &json!({ "name": "Site", "slug": "site" }),
            )
            .await;
        created.expect(StatusCode::CREATED);
        let id = created["id"].as_str().unwrap();
        admin
            .post(
                &format!("/api/v1/workspaces/{ws}/projects"),
                &json!({ "name": "Site", "slug": "site" }),
            )
            .await
            .expect(StatusCode::CONFLICT);
        let path = format!("/api/v1/workspaces/{ws}/projects/{id}");
        let renamed = admin.patch(&path, &json!({ "name": "Website" })).await;
        assert_eq!(renamed["name"], "Website", "renamed");
        admin.delete(&path).await.expect(StatusCode::NO_CONTENT);
        admin.get(&path).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn query_params_are_coerced_by_type() {
        let app = TestApp::new().await;
        let seeded = app.seed(Persona::Team, "t1").await;
        let admin = app.with_session(&seeded.accounts["admin"].session_token);
        let ws = seeded.workspaces["main"].id;
        let one = admin.get(&format!("/api/v1/workspaces/{ws}/projects?limit=1")).await;
        one.expect(StatusCode::OK);
        assert_eq!(one["items"].as_array().unwrap().len(), 1, "limit applied");

        let bad = admin.get(&format!("/api/v1/workspaces/{ws}/projects?limit=x")).await;
        bad.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(bad.body["error"]["fields"]["limit"][0], "must be an integer", "field path");

        let out_of_range = admin.get(&format!("/api/v1/workspaces/{ws}/projects?limit=0")).await;
        out_of_range.expect(StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn malformed_path_ids_are_not_found_or_invalid() {
        let app = TestApp::new().await;
        let admin = app.sign_in("a@example.com").await;
        let ws = workspace(&admin, "acme").await;
        admin.get("/api/v1/workspaces/not-a-uuid").await.expect(StatusCode::NOT_FOUND);
        let bad = admin.get(&format!("/api/v1/workspaces/{ws}/projects/not-a-uuid")).await;
        bad.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            bad.body["error"]["fields"].get("project_id").is_some(),
            "field error on project_id: {}",
            bad.text
        );
    }

    #[tokio::test]
    async fn platform_meta_is_public() {
        let app = TestApp::new().await;
        let meta = app.client().get("/api/v1/meta").await;
        meta.expect(StatusCode::OK);
        assert_eq!(meta["version"], env!("CARGO_PKG_VERSION"), "version");
        assert_eq!(meta["modules"], Value::Array(Vec::new()), "no modules");
    }
}
