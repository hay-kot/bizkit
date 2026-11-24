//! bizkit's own usage, sent to the project `modules.analytics.self_usage`
//! names.

#[cfg(test)]
mod self_usage {
    use std::time::Duration;

    use bizkit_analytics::Analytics;
    use bizkit_analytics::testing::project_pool;
    use bizkit_core::deps::Clock as _;
    use bizkit_testkit::{Manifest, Persona, SpecClient, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    const NS: &str = "usage";

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
    }

    async fn setup(config: Option<Value>, with_site: bool) -> Setup {
        let app = TestApp::builder()
            .module(Analytics::manual_flush())
            .config(|c| {
                if let Some(section) = config {
                    c.modules.insert("analytics".to_owned(), json!({ "self_usage": section }));
                }
            })
            .build()
            .await;
        let team = app.seed(Persona::Team, NS).await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let s = Setup { app, team, admin };
        if with_site {
            s.admin
                .post(
                    &s.path("analytics/site"),
                    &json!({ "project_id": s.team.projects["site"].id }),
                )
                .await
                .expect(StatusCode::CREATED);
        }
        s
    }

    fn here() -> Value {
        json!({ "workspace": format!("team-{NS}"), "project": "site" })
    }

    impl Setup {
        fn path(&self, rest: &str) -> String {
            format!("/api/v1/workspaces/{}/{rest}", self.team.workspaces["main"].id)
        }

        async fn create_key(&self) -> String {
            let key = self
                .admin
                .post(&self.path("api-keys"), &json!({ "name": "ci", "scopes": ["projects:read"] }))
                .await;
            key.expect(StatusCode::CREATED);
            key["key"]["id"].as_str().unwrap().to_owned()
        }

        async fn settle(&self) {
            self.app.drain_jobs().await;
            self.app.flush().await;
        }

        /// The file's events as `(name, user_id, props)`, oldest first.
        async fn events(&self) -> Vec<(String, Option<String>, Option<String>)> {
            let file =
                project_pool(self.app.data_dir(), self.team.projects["site"].id).await.unwrap();
            sqlx::query_as("SELECT name, user_id, props FROM analytics_events ORDER BY ts, rowid")
                .fetch_all(&file)
                .await
                .unwrap()
        }

        async fn times(&self) -> Vec<i64> {
            let file =
                project_pool(self.app.data_dir(), self.team.projects["site"].id).await.unwrap();
            sqlx::query_scalar("SELECT ts FROM analytics_events ORDER BY ts, rowid")
                .fetch_all(&file)
                .await
                .unwrap()
        }

        async fn traits(&self, user_id: &str) -> Value {
            let file =
                project_pool(self.app.data_dir(), self.team.projects["site"].id).await.unwrap();
            let traits: String =
                sqlx::query_scalar("SELECT traits FROM analytics_people WHERE user_id = ?")
                    .bind(user_id)
                    .fetch_one(&file)
                    .await
                    .unwrap();
            serde_json::from_str(&traits).unwrap()
        }

        async fn dead_jobs(&self) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status != 'done'")
                .fetch_one(self.app.app().platform().db().read())
                .await
                .unwrap()
        }
    }

    #[tokio::test]
    async fn actions_arrive_as_events_with_traits() {
        let s = setup(Some(here()), true).await;
        let admin_id = s.team.accounts["admin"].id.to_string();
        let ws = s.team.workspaces["main"].id.to_string();
        s.create_key().await;
        s.settle().await;

        let events = s.events().await;
        let props = json!({ "workspace_id": ws }).to_string();
        assert_eq!(
            events,
            [("api_key_created".to_owned(), Some(admin_id.clone()), Some(props))],
            "$identify stores traits, not an event"
        );
        // The admin also created the persona's `full` key.
        assert_eq!(s.traits(&admin_id).await, json!({ "workspaces": 1, "api_keys": 2 }));
    }

    #[tokio::test]
    async fn traits_are_current_when_the_report_runs() {
        let s = setup(Some(here()), true).await;
        let admin_id = s.team.accounts["admin"].id.to_string();
        let first = s.create_key().await;
        s.create_key().await;
        s.admin.delete(&s.path(&format!("api-keys/{first}"))).await.expect(StatusCode::NO_CONTENT);
        s.settle().await;

        let names: Vec<String> = s.events().await.into_iter().map(|(name, ..)| name).collect();
        assert_eq!(names, ["api_key_created", "api_key_created", "api_key_revoked"]);
        // The persona's key and the second one; every report counts at run time.
        assert_eq!(s.traits(&admin_id).await["api_keys"], 2);
    }

    #[tokio::test]
    async fn a_removed_member_gets_fresh_traits() {
        let s = setup(Some(here()), true).await;
        let member_id = s.team.accounts["member"].id;
        s.admin
            .delete(&s.path(&format!("members/{member_id}")))
            .await
            .expect(StatusCode::NO_CONTENT);
        s.settle().await;
        let member_id = member_id.to_string();
        let acted: Vec<_> =
            s.events().await.into_iter().filter(|(name, ..)| name != "$identify").collect();
        assert_eq!(acted.len(), 1, "{acted:?}");
        assert_eq!(acted[0].0, "member_removed");
        assert_ne!(acted[0].1.as_deref(), Some(member_id.as_str()), "the admin acted");
        assert_eq!(
            s.traits(&member_id).await["workspaces"],
            0,
            "the removed member is refreshed too"
        );
    }

    #[tokio::test]
    async fn a_delivery_keeps_its_day_but_never_lands_in_a_past_one() {
        let s = setup(Some(here()), true).await;
        let acted = s.app.clock.now();
        s.create_key().await;
        s.app.clock.advance(Duration::from_mins(5));
        s.settle().await;
        assert_eq!(s.times().await, [acted.0], "same day: the action's time");

        s.create_key().await;
        s.app.clock.advance(Duration::from_hours(48));
        s.settle().await;
        assert_eq!(s.times().await.last(), Some(&s.app.clock.now().0), "a later day: now");
    }

    #[tokio::test]
    async fn unset_config_sends_nothing() {
        let s = setup(None, true).await;
        s.create_key().await;
        s.settle().await;
        assert_eq!(s.events().await, []);
        assert_eq!(s.dead_jobs().await, 0, "the delivery finished");
    }

    #[tokio::test]
    async fn a_project_without_a_site_drops_the_report_without_retries() {
        let s = setup(Some(here()), false).await;
        s.create_key().await;
        s.settle().await;
        assert_eq!(s.dead_jobs().await, 0, "no retry can fix config");
        let missing = setup(Some(json!({ "workspace": "nope", "project": "site" })), true).await;
        missing.create_key().await;
        missing.settle().await;
        assert_eq!(missing.events().await, []);
        assert_eq!(missing.dead_jobs().await, 0);
    }

    #[tokio::test]
    async fn the_config_refuses_unknown_keys() {
        let built = std::panic::AssertUnwindSafe(
            TestApp::builder()
                .module(Analytics::manual_flush())
                .config(|c| {
                    c.modules.insert(
                        "analytics".to_owned(),
                        json!({ "self_usage": { "workspace": "w", "project": "p", "email": true } }),
                    );
                })
                .build(),
        );
        let result = futures::FutureExt::catch_unwind(built).await;
        assert!(result.is_err(), "unknown self_usage keys fail the build");
    }
}
