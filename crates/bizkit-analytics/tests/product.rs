//! Product reports through the API: the event explorer, funnels, people and
//! retention, fed by server ingest.

#[cfg(test)]
mod product {
    use std::time::Duration;

    use bizkit_analytics::Analytics;
    use bizkit_testkit::{Manifest, Persona, SpecClient, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    const DAY1: &str = "2026-01-01";

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        member: SpecClient,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Analytics::manual_flush()).build().await;
        let team = app.seed(Persona::Team, "product").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let member = app.with_session(&team.accounts["member"].session_token);
        let s = Setup { app, team, admin, member };
        s.admin
            .post(&s.path("site"), &json!({"project_id": s.project()}))
            .await
            .expect(StatusCode::CREATED);
        s
    }

    impl Setup {
        fn project(&self) -> String {
            self.team.projects["site"].id.to_string()
        }

        fn path(&self, rest: &str) -> String {
            format!("/api/v1/workspaces/{}/analytics/{rest}", self.team.workspaces["main"].id)
        }

        async fn ingest(&self, events: Value) {
            self.admin
                .post(
                    &self.path("events"),
                    &json!({"project_id": self.project(), "events": events}),
                )
                .await
                .expect(StatusCode::OK);
            self.app.flush().await;
        }

        async fn get(&self, rest: &str) -> Value {
            self.member.get(&self.path(rest)).await.expect(StatusCode::OK).json()
        }

        /// Ada does the whole signup, Bob stops after starting, Cy only views pricing.
        async fn signups(&self) {
            self.ingest(json!([
                {"name": "$identify", "user_id": "ada", "traits": {"email": "ada@x.test"}},
                {"name": "pageview", "user_id": "ada", "url": "https://x.test/pricing"},
                {"name": "signup_started", "user_id": "ada", "props": {"plan": "pro"}},
                {"name": "signup", "user_id": "ada", "props": {"plan": "pro", "annual": true}},
                {"name": "pageview", "user_id": "bob", "url": "https://x.test/pricing"},
                {"name": "signup_started", "user_id": "bob", "props": {"plan": "free"}},
                {"name": "pageview", "user_id": "cy", "url": "https://x.test/pricing/teams"},
            ]))
            .await;
        }
    }

    #[tokio::test]
    async fn the_explorer_counts_and_filters_by_props() {
        let s = setup().await;
        s.signups().await;
        let q = |extra: &str| {
            format!("events/stats?project_id={}&from={DAY1}&to={DAY1}{extra}", s.project())
        };
        let all = s.get(&q("")).await;
        assert_eq!(all["interval"], "hour");
        assert_eq!(all["series"][0]["count"], 3, "custom events only");
        assert_eq!(all["names"][0], json!({"name": "signup_started", "count": 2, "users": 2}));

        let pro = s.get(&q("&props=plan%3Dpro")).await;
        assert_eq!(pro["series"][0]["count"], 2);
        let annual = s.get(&q("&name=signup&props=plan%3Dpro&props=annual%3Dtrue")).await;
        assert_eq!(
            annual["series"][0],
            json!({"start": "2026-01-01T00:00:00Z", "count": 1, "users": 1})
        );

        let page = s
            .get(&format!("events?project_id={}&from={DAY1}&to={DAY1}&limit=4", s.project()))
            .await;
        assert_eq!(page["items"].as_array().unwrap().len(), 4);
        assert_eq!(page["items"][0]["name"], "pageview", "newest first");
        assert!(page["items"][0]["person_id"].is_string());
        let before = page["next_before"].as_i64().unwrap();
        let rest = s
            .get(&format!(
                "events?project_id={}&from={DAY1}&to={DAY1}&before={before}",
                s.project()
            ))
            .await;
        assert_eq!(rest["items"].as_array().unwrap().len(), 2);
        assert_eq!(rest["next_before"], Value::Null);

        s.member
            .get(&s.path(&format!(
                "events/stats?project_id={}&from={DAY1}&to={DAY1}&props=nokey",
                s.project()
            )))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn funnels_count_people_per_step() {
        let s = setup().await;
        s.signups().await;
        let steps = json!([
            {"kind": "page", "match": "/pricing*"},
            {"kind": "event", "match": "signup_started"},
            {"kind": "event", "match": "signup"},
        ]);
        let body = json!({"project_id": s.project(), "name": "Signup", "steps": steps});
        let funnel: Value =
            s.admin.post(&s.path("funnels"), &body).await.expect(StatusCode::CREATED).json();
        assert_eq!(funnel["window_days"], 7);
        let id = funnel["id"].as_str().unwrap();

        let results = s.get(&format!("funnels/{id}/results?from={DAY1}&to={DAY1}")).await;
        let people: Vec<i64> = results["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| step["people"].as_i64().unwrap())
            .collect();
        assert_eq!(people, [3, 2, 1]);
        assert_eq!(results["steps"][2]["from_previous"], 0.5);

        let bad = json!({"project_id": s.project(), "name": "x", "steps": [{"kind": "page", "match": "/a"}]});
        s.admin.post(&s.path("funnels"), &bad).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let renamed: Value = s
            .admin
            .patch(&s.path(&format!("funnels/{id}")), &json!({"name": "Sign up", "window_days": 3}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(
            (renamed["name"].as_str(), renamed["window_days"].as_i64()),
            (Some("Sign up"), Some(3))
        );
        assert_eq!(
            s.get(&format!("funnels?project_id={}", s.project())).await["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        s.admin.delete(&s.path(&format!("funnels/{id}"))).await.expect(StatusCode::NO_CONTENT);
        s.member.get(&s.path(&format!("funnels/{id}"))).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn dropoffs_list_who_stopped_before_a_step() {
        let s = setup().await;
        s.signups().await;
        s.ingest(json!([
            {"name": "$identify", "user_id": "bob", "traits": {"name": "Bob"}},
            {"name": "pageview", "anon_id": "anon-1", "url": "https://x.test/pricing"},
            {"name": "faq_opened", "anon_id": "anon-1"},
        ]))
        .await;
        let steps = json!([
            {"kind": "page", "match": "/pricing*"},
            {"kind": "event", "match": "signup_started"},
            {"kind": "event", "match": "signup"},
        ]);
        let body = json!({"project_id": s.project(), "name": "Signup", "steps": steps});
        let funnel: Value =
            s.admin.post(&s.path("funnels"), &body).await.expect(StatusCode::CREATED).json();
        let id = funnel["id"].as_str().unwrap();
        let range = format!("from={DAY1}&to={DAY1}");
        let results = s.get(&format!("funnels/{id}/results?{range}")).await;
        let people = |step: usize| results["steps"][step]["people"].as_i64().unwrap();

        let second = s.get(&format!("funnels/{id}/dropoffs?{range}&step=2")).await;
        assert_eq!(second["total"].as_i64(), Some(people(0) - people(1)));
        assert_eq!(second["total"], 2, "cy and the anonymous visitor");
        let anonymous = second["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["person_id"].is_null())
            .unwrap();
        let names: Vec<&str> = anonymous["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["pageview", "faq_opened"], "events from entry on");

        let third = s.get(&format!("funnels/{id}/dropoffs?{range}&step=3")).await;
        assert_eq!(third["total"], 1);
        let bob = &third["items"][0];
        assert!(bob["person_id"].is_string());
        assert_eq!((bob["user_id"].as_str(), bob["display"].as_str()), (Some("bob"), Some("Bob")));
        assert!(bob["last_step_at"].as_i64() >= bob["entered_at"].as_i64());

        let one = s.get(&format!("funnels/{id}/dropoffs?{range}&step=2&limit=1")).await;
        assert_eq!((one["items"].as_array().unwrap().len(), one["total"].as_i64()), (1, Some(2)));
        for bad in ["step=1", "step=4", "step=2&limit=0"] {
            s.member
                .get(&s.path(&format!("funnels/{id}/dropoffs?{range}&{bad}")))
                .await
                .expect(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn people_and_their_timelines() {
        let s = setup().await;
        s.signups().await;
        let list = s.get(&format!("people?project_id={}", s.project())).await;
        assert_eq!(list["items"].as_array().unwrap().len(), 3);
        let found = s.get(&format!("people?project_id={}&q=ada%40x", s.project())).await;
        let ada = &found["items"][0];
        assert_eq!(
            (ada["user_id"].as_str(), ada["display"].as_str(), ada["events"].as_i64()),
            (Some("ada"), Some("ada@x.test"), Some(3))
        );

        let detail = s.get(&format!("people/{}", ada["id"].as_str().unwrap())).await;
        assert_eq!(detail["traits"], json!({"email": "ada@x.test"}));
        let names: Vec<&str> = detail["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["signup", "signup_started", "pageview"]);
        let none = format!("people/{}", "0190a000-0000-7000-8000-000000000000");
        s.member.get(&s.path(&none)).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn retention_by_first_week() {
        let s = setup().await;
        s.signups().await;
        s.app.clock.advance(Duration::from_hours(24 * 7));
        s.ingest(
            json!([{"name": "pageview", "user_id": "ada"}, {"name": "signup", "user_id": "cy"}]),
        )
        .await;
        let table =
            s.get(&format!("retention?project_id={}&from={DAY1}&to=2026-01-08", s.project())).await;
        let cohort = &table["cohorts"][0];
        assert_eq!(cohort["week"], "2025-12-29");
        assert_eq!(cohort["people"], 3);
        assert_eq!(cohort["weeks"][0], 1.0);
        let back = cohort["weeks"][1].as_f64().unwrap();
        assert!((back - 2.0 / 3.0).abs() < 1e-9, "{back}");
        assert_eq!(cohort["weeks"][2], Value::Null, "not begun");

        let signups = s
            .get(&format!(
                "retention?project_id={}&from={DAY1}&to=2026-01-08&return_event=signup",
                s.project()
            ))
            .await;
        assert_eq!(signups["cohorts"][0]["weeks"][1].as_f64().unwrap(), 1.0 / 3.0);

        let later = s
            .get(&format!("retention?project_id={}&from=2026-01-08&to=2026-01-08", s.project()))
            .await;
        assert_eq!(later["cohorts"], json!([]), "Ada and Cy were seen in an earlier week");
    }
}
