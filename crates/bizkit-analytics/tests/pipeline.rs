//! The buffered writer seen from outside: when queued events land, what a
//! full queue answers, the app-side work a flush owes, and what deletion and
//! shutdown do to a project's file.

#[cfg(test)]
mod pipeline {
    use std::path::Path;
    use std::time::Duration;

    use bizkit_analytics::Analytics;
    use bizkit_analytics::testing::project_pool;
    use bizkit_core::ids::ProjectId;
    use bizkit_testkit::{Manifest, Persona, RawBody, SpecClient, TestApp};
    use http::{HeaderName, Method, StatusCode, header};
    use serde_json::{Value, json};

    const CHROME: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    const COLLECT: &str = "/m/analytics/public/e";

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        writer: SpecClient,
        keys: Vec<(String, String)>,
    }

    async fn setup(analytics: Analytics, config: Value) -> Setup {
        let app = TestApp::builder()
            .module(analytics)
            .config(move |c| {
                c.trusted_proxies = 1;
                c.modules.insert("analytics".to_owned(), config);
            })
            .build()
            .await;
        let team = app.seed(Persona::Team, "pipeline").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let token = app.insert_api_key(team.workspaces["main"].id, &["analytics:write"]).await;
        let writer = app.with_key(&token);
        let mut s = Setup { app, team, admin, writer, keys: Vec::new() };
        for project in ["site", "app"] {
            let key = s.add_site(project).await;
            s.keys.push((project.to_owned(), key));
        }
        s
    }

    impl Setup {
        fn project(&self, name: &str) -> ProjectId {
            self.team.projects[name].id
        }

        fn path(&self, rest: &str) -> String {
            format!("/api/v1/workspaces/{}/analytics/{rest}", self.team.workspaces["main"].id)
        }

        fn key(&self, project: &str) -> &str {
            &self.keys.iter().find(|(name, _)| name == project).unwrap().1
        }

        async fn add_site(&self, project: &str) -> String {
            let project_id = self.project(project).to_string();
            let site: Value = self
                .admin
                .post(&self.path("site"), &json!({"project_id": project_id}))
                .await
                .expect(StatusCode::CREATED)
                .json();
            self.admin
                .patch(
                    &self.path("site"),
                    &json!({"project_id": project_id, "identity_mode": "after_identify"}),
                )
                .await
                .expect(StatusCode::OK);
            site["key"].as_str().unwrap().to_owned()
        }

        /// Posts `n` server events named `name` to the "site" project.
        async fn server(&self, name: &str, n: usize) -> StatusCode {
            let events: Vec<Value> =
                (0..n).map(|i| json!({"name": name, "user_id": format!("u{i}")})).collect();
            let body = json!({"project_id": self.project("site").to_string(), "events": events});
            self.writer.post(&self.path("events"), &body).await.status
        }

        async fn beacon(&self, project: &str, beacon: Value) {
            let mut beacon = beacon;
            let fields = beacon.as_object_mut().unwrap();
            fields.insert("s".to_owned(), json!(self.key(project)));
            fields.entry("u").or_insert_with(|| json!("https://example.com/"));
            let headers = [
                (header::USER_AGENT, CHROME),
                (HeaderName::from_static("x-forwarded-for"), "203.0.113.1"),
            ];
            let raw = RawBody {
                content_type: "text/plain;charset=UTF-8",
                body: serde_json::to_vec(&beacon).unwrap().into(),
                headers: &headers,
            };
            self.app
                .client()
                .send_body(Method::POST, COLLECT, raw)
                .await
                .expect(StatusCode::ACCEPTED);
        }

        /// Events in the project's file, without flushing first.
        async fn stored(&self, project: &str) -> i64 {
            count_events(self.app.data_dir(), self.project(project)).await
        }

        async fn rollup_jobs(&self) -> i64 {
            sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'analytics.rollup'")
                .fetch_one(self.app.app().platform().db().read())
                .await
                .unwrap()
        }

        fn file_exists(&self, project: &str) -> bool {
            bizkit_analytics::project_db_path(self.app.data_dir(), self.project(project)).exists()
        }
    }

    async fn count_events(data_dir: &Path, project_id: ProjectId) -> i64 {
        let pool = project_pool(data_dir, project_id).await.unwrap();
        let count = sqlx::query_scalar("SELECT COUNT(*) FROM analytics_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;
        count
    }

    /// Polls until the "site" project holds `want` events, for up to 2 s.
    async fn eventually(s: &Setup, want: i64) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let got = s.stored("site").await;
            if got == want {
                return;
            }
            assert!(tokio::time::Instant::now() < deadline, "{got} events stored, want {want}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn queued_events_land_on_flush_or_at_a_thousand() {
        let s = setup(Analytics::manual_flush(), json!({})).await;
        for _ in 0..9 {
            assert_eq!(s.server("tick", 100).await, StatusCode::OK);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(s.stored("site").await, 0, "nothing lands before a flush");

        assert_eq!(s.server("tick", 100).await, StatusCode::OK);
        eventually(&s, 1_000).await;

        assert_eq!(s.server("tick", 1).await, StatusCode::OK);
        s.app.flush().await;
        assert_eq!(s.stored("site").await, 1_001);
    }

    #[tokio::test]
    async fn the_flush_window_writes_without_a_manual_flush() {
        let s = setup(Analytics::default(), json!({"flush_ms": 10})).await;
        assert_eq!(s.server("tick", 1).await, StatusCode::OK);
        eventually(&s, 1).await;
    }

    #[tokio::test]
    async fn a_flush_window_out_of_range_fails_the_build() {
        for flush_ms in [9, 251] {
            let build = tokio::spawn(setup(Analytics::default(), json!({"flush_ms": flush_ms})));
            assert!(build.await.is_err_and(|err| err.is_panic()), "flush_ms {flush_ms} builds");
        }
    }

    #[tokio::test]
    async fn a_full_queue_refuses_server_events_and_drops_beacons() {
        let s = setup(Analytics::manual_flush_with_queue(5), json!({})).await;
        assert_eq!(s.server("tick", 5).await, StatusCode::OK);
        assert_eq!(s.server("tick", 1).await, StatusCode::TOO_MANY_REQUESTS);
        s.beacon("site", json!({"n": "pageview"})).await;

        s.app.flush().await;
        assert_eq!(s.stored("site").await, 5, "the dropped beacon and the refused event are gone");
        assert_eq!(s.server("tick", 5).await, StatusCode::OK, "a flush frees the queue");
        s.app.flush().await;
        assert_eq!(s.stored("site").await, 10);
    }

    #[tokio::test]
    async fn the_first_flush_of_a_day_enqueues_one_rollup() {
        let s = setup(Analytics::manual_flush(), json!({})).await;
        assert_eq!(s.server("tick", 1).await, StatusCode::OK);
        s.app.flush().await;
        assert_eq!(s.rollup_jobs().await, 1);

        assert_eq!(s.server("tick", 1).await, StatusCode::OK);
        s.app.flush().await;
        assert_eq!(s.rollup_jobs().await, 1, "the same day adds none");

        s.app.clock.advance(Duration::from_hours(24));
        assert_eq!(s.server("tick", 1).await, StatusCode::OK);
        s.app.flush().await;
        assert_eq!(s.rollup_jobs().await, 2, "a new day adds one");
    }

    #[tokio::test]
    async fn an_event_keeps_the_day_it_arrived_on() {
        let s = setup(Analytics::manual_flush(), json!({})).await;
        s.app.clock.advance(Duration::from_hours(24) - Duration::from_millis(50));
        assert_eq!(s.server("tick", 1).await, StatusCode::OK);
        s.app.clock.advance(Duration::from_millis(100));
        s.app.flush().await;
        let pool = project_pool(s.app.data_dir(), s.project("site")).await.unwrap();
        let day: String =
            sqlx::query_scalar("SELECT day FROM analytics_events").fetch_one(&pool).await.unwrap();
        assert_eq!(day, "2026-01-01");
    }

    #[tokio::test]
    async fn one_bad_request_fails_alone() {
        let s = setup(Analytics::manual_flush(), json!({})).await;
        let pool = project_pool(s.app.data_dir(), s.project("site")).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER poison BEFORE INSERT ON analytics_events WHEN NEW.name = 'poison'
             BEGIN SELECT RAISE(ABORT, 'poison'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        assert_eq!(s.server("good", 2).await, StatusCode::OK);
        assert_eq!(s.server("poison", 3).await, StatusCode::OK);
        assert_eq!(s.server("good", 4).await, StatusCode::OK);
        let flushed = s.app.app().platform().flush_modules().await;
        assert!(flushed.is_err(), "the flush reports the bad request");
        assert_eq!(s.stored("site").await, 6, "the good requests land");
    }

    #[tokio::test]
    async fn identify_in_one_project_leaves_another_alone() {
        let s = setup(Analytics::manual_flush(), json!({})).await;
        for project in ["site", "app"] {
            let body = json!({"project_id": s.project(project).to_string(), "name": "Signup", "kind": "event", "match": "signup"});
            s.admin.post(&s.path("goals"), &body).await.expect(StatusCode::CREATED);
            s.beacon(project, json!({"n": "signup", "a": "anon-1"})).await;
        }
        let identify = json!({"project_id": s.project("site").to_string(), "events": [
            {"name": "$identify", "user_id": "u1", "anon_id": "anon-1"},
        ]});
        s.writer.post(&s.path("events"), &identify).await.expect(StatusCode::OK);
        s.app.flush().await;

        let people = |project: &'static str| {
            let s = &s;
            async move {
                let pool = project_pool(s.app.data_dir(), s.project(project)).await.unwrap();
                let events: Vec<String> = sqlx::query_scalar("SELECT person FROM analytics_events")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
                let conversions: Vec<String> =
                    sqlx::query_scalar("SELECT person FROM analytics_conversions")
                        .fetch_all(&pool)
                        .await
                        .unwrap();
                (events, conversions)
            }
        };
        let (events, conversions) = people("site").await;
        assert!(events[0].starts_with("p:") && conversions[0].starts_with("p:"), "{events:?}");
        let other = (vec!["a:anon-1".to_owned()], vec!["a:anon-1".to_owned()]);
        assert_eq!(people("app").await, other);
    }

    #[tokio::test]
    async fn deleting_a_goal_removes_conversions_still_queued() {
        let s = setup(Analytics::manual_flush(), json!({})).await;
        let body = json!({"project_id": s.project("site").to_string(), "name": "Signup", "kind": "event", "match": "signup"});
        let goal: Value =
            s.admin.post(&s.path("goals"), &body).await.expect(StatusCode::CREATED).json();
        s.beacon("site", json!({"n": "signup"})).await;
        let goal_path = s.path(&format!("goals/{}", goal["id"].as_str().unwrap()));
        s.admin.delete(&goal_path).await.expect(StatusCode::NO_CONTENT);

        s.app.flush().await;
        let pool = project_pool(s.app.data_dir(), s.project("site")).await.unwrap();
        let conversions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM analytics_conversions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(conversions, 0);
        assert_eq!(s.stored("site").await, 1, "the event stays");
    }

    #[tokio::test]
    async fn deleting_a_project_removes_its_file_for_good() {
        let s = setup(Analytics::manual_flush(), json!({})).await;
        s.beacon("site", json!({"n": "pageview"})).await;
        s.beacon("app", json!({"n": "pageview"})).await;
        s.app.flush().await;
        assert!(s.file_exists("site"));

        let project = format!(
            "/api/v1/workspaces/{}/projects/{}",
            s.team.workspaces["main"].id,
            s.project("site")
        );
        s.admin.delete(&project).await.expect(StatusCode::NO_CONTENT);
        s.beacon("site", json!({"n": "pageview"})).await;
        s.app.drain_jobs().await;
        assert!(!s.file_exists("site"), "the subscriber removes the file");

        s.beacon("site", json!({"n": "pageview"})).await;
        s.app.flush().await;
        assert!(!s.file_exists("site"), "a later beacon doesn't bring it back");
        assert!(s.file_exists("app"), "other projects keep theirs");
    }

    #[tokio::test]
    async fn shutting_down_writes_every_queued_event() {
        let s = setup(Analytics::manual_flush(), json!({})).await;
        for _ in 0..3 {
            assert_eq!(s.server("tick", 100).await, StatusCode::OK);
        }
        let project = s.project("site");
        let dir = s.app.shut_down().await;
        assert_eq!(count_events(dir.path(), project).await, 300);
    }
}
