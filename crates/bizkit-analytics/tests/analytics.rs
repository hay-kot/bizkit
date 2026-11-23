//! Site settings and goals.

#[cfg(test)]
mod analytics {
    use bizkit_analytics::Analytics;
    use bizkit_testkit::{Manifest, Persona, SpecClient, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    struct Setup {
        _app: TestApp,
        team: Manifest,
        admin: SpecClient,
        member: SpecClient,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Analytics).build().await;
        let team = app.seed(Persona::Team, "analytics").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let member = app.with_session(&team.accounts["member"].session_token);
        Setup { _app: app, team, admin, member }
    }

    impl Setup {
        fn base(&self) -> String {
            format!("/api/v1/workspaces/{}/analytics", self.team.workspaces["main"].id)
        }

        fn project(&self) -> String {
            self.team.projects["site"].id.to_string()
        }

        fn site_path(&self) -> String {
            format!("{}/site?project_id={}", self.base(), self.project())
        }
    }

    #[tokio::test]
    async fn turning_analytics_on_gives_a_snippet_once() {
        let s = setup().await;
        s.admin.get(&s.site_path()).await.expect(StatusCode::NOT_FOUND);

        let body = json!({"project_id": s.project()});
        let site: Value = s
            .admin
            .post(&format!("{}/site", s.base()), &body)
            .await
            .expect(StatusCode::CREATED)
            .json();
        let key = site["key"].as_str().unwrap();
        assert_eq!(key.len(), 24);
        let snippet = site["snippet"].as_str().unwrap();
        assert!(snippet.contains("/m/analytics/public/s.js"), "{snippet}");
        assert!(snippet.contains(&format!(r#"data-site="{key}""#)), "{snippet}");
        assert_eq!(site["identity_mode"], "cookieless");
        assert_eq!(site["has_events"], false);

        s.admin.post(&format!("{}/site", s.base()), &body).await.expect(StatusCode::CONFLICT);
        let got: Value = s.member.get(&s.site_path()).await.expect(StatusCode::OK).json();
        assert_eq!(got, site);
    }

    #[tokio::test]
    async fn site_settings_normalize_and_reject_bad_hosts() {
        let s = setup().await;
        let body = json!({"project_id": s.project()});
        s.admin.post(&format!("{}/site", s.base()), &body).await.expect(StatusCode::CREATED);
        let path = format!("{}/site", s.base());

        let changed: Value = s
            .admin
            .patch(
                &path,
                &json!({"project_id": s.project(), "hostnames": [" WWW.Example.com", "*.example.com", "www.example.com"], "identity_mode": "after_identify"}),
            )
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(changed["hostnames"], json!(["*.example.com", "www.example.com"]));
        assert_eq!(changed["identity_mode"], "after_identify");

        s.admin
            .patch(&path, &json!({"project_id": s.project(), "hostnames": ["https://example.com"]}))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
        s.member
            .patch(&path, &json!({"project_id": s.project(), "identity_mode": "always"}))
            .await
            .expect(StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn goals_are_unique_per_match_and_deletable() {
        let s = setup().await;
        let path = format!("{}/goals", s.base());
        let goal = json!({"project_id": s.project(), "name": "Signup", "kind": "event", "match": "signup_completed"});
        let created: Value = s.admin.post(&path, &goal).await.expect(StatusCode::CREATED).json();
        assert_eq!(created["match"], "signup_completed");
        s.admin.post(&path, &goal).await.expect(StatusCode::CONFLICT);

        for (kind, matches) in [("page", "pricing"), ("page", "/blog/*"), ("event", "has space")] {
            let bad =
                json!({"project_id": s.project(), "name": "Bad", "kind": kind, "match": matches});
            s.admin.post(&path, &bad).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let page = json!({"project_id": s.project(), "name": "Pricing", "kind": "page", "match": "/pricing"});
        s.admin.post(&path, &page).await.expect(StatusCode::CREATED);

        let list: Value = s
            .member
            .get(&format!("{path}?project_id={}", s.project()))
            .await
            .expect(StatusCode::OK)
            .json();
        let names: Vec<&str> =
            list["items"].as_array().unwrap().iter().map(|g| g["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["Pricing", "Signup"]);

        let one = format!("{path}/{}", created["id"].as_str().unwrap());
        s.member.delete(&one).await.expect(StatusCode::FORBIDDEN);
        s.admin.delete(&one).await.expect(StatusCode::NO_CONTENT);
        s.admin.delete(&one).await.expect(StatusCode::NOT_FOUND);
    }
}
