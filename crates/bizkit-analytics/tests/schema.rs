//! The schema lookups through the API: event names, property and trait
//! keys and their values, with the scan bounds and tenant checks.

#[cfg(test)]
mod schema {
    use std::fmt::Write as _;

    use bizkit_analytics::Analytics;
    use bizkit_analytics::testing::project_pool;
    use bizkit_testkit::{Manifest, Persona, SpecClient, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    /// The fake clock's day.
    const TODAY_MS: i64 = 1_767_225_600_000;
    const DAY_MS: i64 = 86_400_000;
    /// A key that would break SQL or a JSON path if it reached either.
    const NASTY: &str = "a'b\"c; DROP TABLE analytics_events;--.$[0]%_\\";

    struct Setup {
        app: TestApp,
        team: Manifest,
        member: SpecClient,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Analytics::manual_flush()).build().await;
        let team = app.seed(Persona::Team, "schema").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let member = app.with_session(&team.accounts["member"].session_token);
        let s = Setup { app, team, member };
        admin
            .post(&s.path("site"), &json!({"project_id": s.project()}))
            .await
            .expect(StatusCode::CREATED);
        let events = json!([
            {"name": "$identify", "user_id": "ada", "traits": {"plan": "pro", "seats": 3, "beta": true}},
            {"name": "$identify", "user_id": "bob", "traits": {"plan": "free", "seats": "2"}},
            {"name": "pageview", "user_id": "ada", "url": "https://x.test/", "props": {"ref": "ad"}},
            {"name": "signup", "user_id": "ada", "props": {"plan": "pro", "seats": 3}},
            {"name": "signup", "user_id": "bob", "props": {"plan": "free", "seats": "2"}},
            {"name": "signup", "anon_id": "anon-1", "props": {"plan": "free", "beta": false}},
            {"name": "export_done", "user_id": "bob", "props": {(NASTY): "x'y"}},
        ]);
        admin
            .post(&s.path("events"), &json!({"project_id": s.project(), "events": events}))
            .await
            .expect(StatusCode::OK);
        s.app.flush().await;
        s
    }

    fn encode(text: &str) -> String {
        let mut out = String::new();
        for byte in text.bytes() {
            if byte.is_ascii_alphanumeric() {
                out.push(char::from(byte));
            } else {
                write!(out, "%{byte:02X}").unwrap();
            }
        }
        out
    }

    impl Setup {
        fn project(&self) -> String {
            self.team.projects["site"].id.to_string()
        }

        fn path(&self, rest: &str) -> String {
            format!("/api/v1/workspaces/{}/analytics/{rest}", self.team.workspaces["main"].id)
        }

        fn url(&self, what: &str, params: &[(&str, &str)]) -> String {
            let mut url =
                format!("{}?project_id={}", self.path(&format!("schema/{what}")), self.project());
            for (name, value) in params {
                write!(url, "&{name}={}", encode(value)).unwrap();
            }
            url
        }

        async fn get(&self, what: &str, params: &[(&str, &str)]) -> Value {
            self.member.get(&self.url(what, params)).await.expect(StatusCode::OK).json()
        }

        /// Inserts `count` `old_event` events `days_ago` days back, straight
        /// into the file.
        async fn insert_old(&self, days_ago: i64, count: i64) {
            let pool =
                project_pool(self.app.data_dir(), self.team.projects["site"].id).await.unwrap();
            let ts = TODAY_MS - days_ago * DAY_MS;
            sqlx::query(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?)
                 INSERT INTO analytics_events (workspace_id, project_id, ts, day, name, visitor, person, props, source)
                 SELECT ?, ?, ?, date(? / 1000, 'unixepoch'), 'old_event', i, 'a:' || i, '{\"old\":1}', 'server' FROM n",
            )
            .bind(count)
            .bind(self.team.workspaces["main"].id.to_string())
            .bind(self.project())
            .bind(ts)
            .bind(ts)
            .execute(&pool)
            .await
            .unwrap();
        }
    }

    fn names(list: &Value, field: &str) -> Vec<String> {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item[field].as_str().unwrap().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn events_lists_names_with_pageview() {
        let s = setup().await;
        let list = s.get("events", &[]).await;
        assert_eq!(list["sampled"], false);
        assert_eq!(names(&list, "name"), ["signup", "export_done", "pageview"]);
        let signup = &list["items"][0];
        assert_eq!(signup["events"], 3);
        assert_eq!(signup["users"], 3);
        assert_eq!(signup["last_seen"], "2026-01-01T00:00:00Z");

        let found = s.get("events", &[("q", "PAGE")]).await;
        assert_eq!(names(&found, "name"), ["pageview"], "q ignores case");
        let one = s.get("events", &[("limit", "1")]).await;
        assert_eq!(names(&one, "name"), ["signup"]);
        let none = s.get("events", &[("q", "%")]).await;
        assert_eq!(names(&none, "name"), Vec::<String>::new(), "% is literal");
    }

    #[tokio::test]
    async fn property_keys_and_values() {
        let s = setup().await;
        let keys = s.get("keys", &[("source", "property")]).await;
        assert_eq!(keys["sampled"], false);
        let items = keys["items"].as_array().unwrap();
        assert_eq!(items[0], json!({"key": "plan", "types": ["text"], "count": 3}));
        assert_eq!(items[1], json!({"key": "seats", "types": ["text", "number"], "count": 2}));
        let all = names(&keys, "key");
        assert!(all.contains(&"ref".to_owned()), "pageview props count: {all:?}");
        assert!(all.contains(&"beta".to_owned()));

        let signup = s.get("keys", &[("source", "property"), ("event", "signup")]).await;
        assert_eq!(names(&signup, "key"), ["plan", "seats", "beta"]);
        let page = s.get("keys", &[("source", "property"), ("event", "pageview")]).await;
        assert_eq!(names(&page, "key"), ["ref"]);
        let by_q = s.get("keys", &[("source", "property"), ("q", "EAT")]).await;
        assert_eq!(names(&by_q, "key"), ["seats"]);

        let values = s.get("values", &[("source", "property"), ("key", "plan")]).await;
        assert_eq!(
            values,
            json!({"items": [{"value": "free", "count": 2}, {"value": "pro", "count": 1}], "sampled": false})
        );
        let seats = s.get("values", &[("source", "property"), ("key", "seats")]).await;
        assert_eq!(names(&seats, "value"), ["2", "3"]);
        let beta = s.get("values", &[("source", "property"), ("key", "beta")]).await;
        assert_eq!(names(&beta, "value"), ["false"]);
        let any_r = s.get("values", &[("source", "property"), ("key", "plan"), ("q", "R")]).await;
        assert_eq!(names(&any_r, "value"), ["free", "pro"]);
        let starts_fr =
            s.get("values", &[("source", "property"), ("key", "plan"), ("q", "fr")]).await;
        assert_eq!(names(&starts_fr, "value"), ["free"]);
    }

    #[tokio::test]
    async fn a_key_with_sql_metacharacters_is_data() {
        let s = setup().await;
        let keys = s.get("keys", &[("source", "property"), ("q", "DROP TABLE")]).await;
        assert_eq!(names(&keys, "key"), [NASTY]);
        let values = s.get("values", &[("source", "property"), ("key", NASTY)]).await;
        assert_eq!(names(&values, "value"), ["x'y"]);
        let q = s.get("values", &[("source", "property"), ("key", NASTY), ("q", "'")]).await;
        assert_eq!(names(&q, "value"), ["x'y"]);
        let path = s.get("values", &[("source", "property"), ("key", "$.plan")]).await;
        assert_eq!(names(&path, "value"), Vec::<String>::new(), "keys are never JSON paths");
        assert_eq!(s.get("events", &[]).await["items"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn traits_ignore_the_range() {
        let s = setup().await;
        let old = [("source", "trait"), ("from", "2020-01-01"), ("to", "2020-01-02")];
        let keys = s.get("keys", &old).await;
        assert_eq!(names(&keys, "key"), ["plan", "seats", "beta"]);
        assert_eq!(keys["items"][1]["types"], json!(["text", "number"]));
        let mut params = old.to_vec();
        params.push(("key", "plan"));
        let values = s.get("values", &params).await;
        assert_eq!(names(&values, "value"), ["free", "pro"]);
        let props = s
            .get("keys", &[("source", "property"), ("from", "2020-01-01"), ("to", "2020-01-02")])
            .await;
        assert_eq!(props["items"], json!([]), "properties keep to the range");
    }

    #[tokio::test]
    async fn the_default_range_is_30_days() {
        let s = setup().await;
        s.insert_old(29, 1).await;
        s.insert_old(30, 1).await;
        let list = s.get("events", &[]).await;
        let old = list["items"].as_array().unwrap().iter().find(|item| item["name"] == "old_event");
        assert_eq!(old.unwrap()["events"], 1, "29 days back is in, 30 is out");
        let wide = s.get("events", &[("from", "2025-11-01"), ("to", "2026-01-01")]).await;
        let wide_old =
            wide["items"].as_array().unwrap().iter().find(|item| item["name"] == "old_event");
        assert_eq!(wide_old.unwrap()["events"], 2);
    }

    #[tokio::test]
    async fn scans_stop_at_the_cap() {
        let s = setup().await;
        s.insert_old(2, 1).await;
        s.insert_old(1, 50_000).await;
        let list = s.get("events", &[]).await;
        assert_eq!(list["sampled"], true);
        let old = list["items"].as_array().unwrap().iter().find(|item| item["name"] == "old_event");
        assert_eq!(old.unwrap()["events"], 50_000 - 5, "the 5 newer events fill the rest");
        let by_q = s.get("events", &[("q", "old_event")]).await;
        assert_eq!(by_q["sampled"], true, "q filters after the capped read");
        assert_eq!(by_q["items"][0]["events"], 50_000 - 5, "the oldest one is past the cap");
        let keys = s.get("keys", &[("source", "property")]).await;
        assert_eq!(keys["sampled"], true);
        let values = s.get("values", &[("source", "property"), ("key", "old")]).await;
        assert_eq!(values["sampled"], true);
        assert_eq!(values["items"], json!([{"value": "1", "count": 50_000 - 5}]));
        let plan = s.get("values", &[("source", "property"), ("key", "plan")]).await;
        assert_eq!(names(&plan, "value"), ["free", "pro"], "the newest events are in the read");
        let traits = s.get("keys", &[("source", "trait")]).await;
        assert_eq!(traits["sampled"], false);
    }

    #[tokio::test]
    async fn bad_input_is_refused() {
        let s = setup().await;
        for (what, params) in [
            ("events", vec![("limit", "0")]),
            ("events", vec![("limit", "201")]),
            ("events", vec![("from", "2026-01-01")]),
            ("events", vec![("from", "2026-01-02"), ("to", "2026-01-01")]),
            ("keys", vec![("source", "trait"), ("event", "signup")]),
            ("keys", vec![("source", "property"), ("event", "bad name!")]),
            ("values", vec![("source", "property"), ("key", "")]),
        ] {
            s.member.get(&s.url(what, &params)).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let long = "q".repeat(129);
        s.member
            .get(&s.url("events", &[("q", &long)]))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn another_workspaces_project_is_not_found() {
        let s = setup().await;
        let other = s.app.seed(Persona::Team, "other").await;
        let other_admin = s.app.with_session(&other.accounts["admin"].session_token);
        let other_project = other.projects["site"].id.to_string();
        other_admin
            .post(
                &format!("/api/v1/workspaces/{}/analytics/site", other.workspaces["main"].id),
                &json!({"project_id": other_project}),
            )
            .await
            .expect(StatusCode::CREATED);
        for what in ["events", "keys?source=trait", "values?source=trait&key=plan"] {
            let (what, extra) = what.split_once('?').unwrap_or((what, ""));
            let url = format!(
                "{}?project_id={other_project}{}{extra}",
                s.path(&format!("schema/{what}")),
                if extra.is_empty() { "" } else { "&" },
            );
            s.member.get(&url).await.expect(StatusCode::NOT_FOUND);
        }
        let no_site =
            format!("{}?project_id={}", s.path("schema/events"), s.team.projects["app"].id);
        s.member.get(&no_site).await.expect(StatusCode::NOT_FOUND);
    }
}
