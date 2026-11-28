//! Insights through the API: every view of `insights.query` over a small
//! known data set, the filter operators, and saved insights and dashboards.

#[cfg(test)]
mod insights {
    use bizkit_analytics::Analytics;
    use bizkit_analytics::testing::project_pool;
    use bizkit_testkit::{Manifest, Persona, SpecClient, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    const DAY1: &str = "2026-01-01";
    const DAY1_MS: i64 = 1_767_225_600_000;

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        member: SpecClient,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Analytics::manual_flush()).build().await;
        let team = app.seed(Persona::Team, "insights").await;
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

        /// Ada (pro, 2 tokens) creates 6 workspaces, Bob (free, 0 tokens)
        /// 2, an anonymous browser 1 and a cookieless visitor 1. Cy (free,
        /// tokens sent as text) only views a page; Dee (1 token) has no
        /// events.
        async fn people(&self) {
            let mut events = vec![
                json!({"name": "$identify", "user_id": "ada", "traits": {"plan": "pro", "api_tokens": 2}}),
                json!({"name": "$identify", "user_id": "bob", "traits": {"plan": "free", "api_tokens": 0}}),
                json!({"name": "$identify", "user_id": "cy", "traits": {"plan": "free", "api_tokens": "3"}}),
                json!({"name": "$identify", "user_id": "dee", "traits": {"api_tokens": 1}}),
                json!({"name": "pageview", "user_id": "cy", "url": "https://x.test/"}),
                json!({"name": "workspace_created", "anon_id": "anon-1"}),
            ];
            for n in 1..=6 {
                events.push(json!({"name": "workspace_created", "user_id": "ada",
                                   "props": {"plan": "pro", "n": n}}));
            }
            for _ in 0..2 {
                events.push(json!({"name": "workspace_created", "user_id": "bob",
                                   "props": {"plan": "free", "n": "9"}}));
            }
            self.ingest(Value::Array(events)).await;
            let pool =
                project_pool(self.app.data_dir(), self.team.projects["site"].id).await.unwrap();
            let workspace_id = self.team.workspaces["main"].id.to_string();
            let project_id = self.project();
            sqlx::query(
                "INSERT INTO analytics_events (workspace_id, project_id, ts, day, name, visitor, person, props, source)
                 VALUES (?, ?, ?, ?, 'workspace_created', 7, ?, '{\"plan\":null}', 'web')",
            )
            .bind(&workspace_id)
            .bind(&project_id)
            .bind(DAY1_MS)
            .bind(DAY1)
            .bind(format!("v:{DAY1}:7"))
            .execute(&pool)
            .await
            .unwrap();
        }

        async fn query(&self, query: Value) -> Value {
            self.query_range(DAY1, DAY1, query).await
        }

        async fn query_range(&self, from: &str, to: &str, query: Value) -> Value {
            let body =
                json!({"project_id": self.project(), "from": from, "to": to, "query": query});
            self.member
                .post(&self.path("insights/query"), &body)
                .await
                .expect(StatusCode::OK)
                .json()
        }

        async fn value(&self, query: Value) -> f64 {
            self.query(query).await["value"].as_f64().unwrap()
        }
    }

    fn created(extra: &Value) -> Value {
        let mut query = json!({"kind": "events", "event": "workspace_created"});
        query.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        query
    }

    fn rows(result: &Value) -> Vec<(Value, f64)> {
        result["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["value"].clone(), row["result"].as_f64().unwrap()))
            .collect()
    }

    #[tokio::test]
    async fn series_count_events_and_people() {
        let s = setup().await;
        s.people().await;
        let total = s.query(created(&json!({}))).await;
        assert_eq!(total["view"], "series");
        assert_eq!(total["interval"], "hour");
        assert_eq!(total["value"], 10.0);
        let points = total["points"].as_array().unwrap();
        assert_eq!(points.len(), 24);
        let summed: f64 = points.iter().map(|p| p["value"].as_f64().unwrap()).sum();
        assert_eq!(summed, 10.0);
        assert_eq!(total["raw_since"], Value::Null);

        assert_eq!(s.value(created(&json!({"measure": "users"}))).await, 4.0, "cookieless counts");

        let share = s.query(created(&json!({"measure": "share"}))).await;
        assert_eq!(share["active"], 4, "ada, bob, the anonymous browser and cy; not cookieless");
        assert_eq!(share["value"], 0.75);
    }

    #[tokio::test]
    async fn per_user_buckets_identified_people() {
        let s = setup().await;
        s.people().await;
        let result = s.query(created(&json!({"measure": "per_user"}))).await;
        assert_eq!(result["view"], "distribution");
        assert_eq!(result["users"], 3);
        assert_eq!(
            result["buckets"],
            json!([
                {"times": 1, "or_more": false, "users": 1},
                {"times": 2, "or_more": false, "users": 1},
                {"times": 3, "or_more": false, "users": 0},
                {"times": 4, "or_more": false, "users": 0},
                {"times": 5, "or_more": true, "users": 1},
            ])
        );
    }

    #[tokio::test]
    async fn filter_operators() {
        let s = setup().await;
        s.people().await;
        let with = |filter: Value| created(&json!({"filters": [filter]}));
        assert_eq!(s.value(with(json!({"key": "plan", "op": "is", "value": "pro"}))).await, 6.0);
        assert_eq!(
            s.value(with(json!({"key": "plan", "op": "is_not", "value": "pro"}))).await,
            4.0,
            "bob's 2, and the 2 without a plan"
        );
        assert_eq!(
            s.value(with(json!({"key": "n", "op": "gt", "value": 4}))).await,
            2.0,
            "bob's text \"9\" never compares"
        );
        assert_eq!(s.value(with(json!({"key": "n", "op": "lt", "value": "2.5"}))).await, 2.0);
        assert_eq!(
            s.value(with(json!({"key": "plan", "op": "is_set"}))).await,
            8.0,
            "a null plan is not set"
        );
        let both = created(&json!({"filters": [
            {"key": "plan", "op": "is", "value": "pro"},
            {"key": "n", "op": "is", "value": 3},
        ]}));
        assert_eq!(s.value(both).await, 1.0, "filters all match");
        let as_text = created(&json!({"filters": [{"key": "n", "op": "is", "value": "3.0"}]}));
        assert_eq!(s.value(as_text).await, 1.0, "numbers compare as numbers");
        let nine = created(&json!({"filters": [{"key": "n", "op": "is", "value": 9}]}));
        assert_eq!(s.value(nine).await, 2.0, "bob's text \"9\" matches as text");
    }

    #[tokio::test]
    async fn breakdowns_by_property_and_trait() {
        let s = setup().await;
        s.people().await;
        let by_prop =
            s.query(created(&json!({"breakdown": {"source": "property", "key": "plan"}}))).await;
        assert_eq!(by_prop["view"], "breakdown");
        assert_eq!(by_prop["value"], 10.0);
        assert_eq!(rows(&by_prop), [(json!("pro"), 6.0), (json!("free"), 2.0), (Value::Null, 2.0)]);
        assert_eq!(by_prop["other"], Value::Null);

        let by_trait = s
            .query(created(
                &json!({"measure": "users", "breakdown": {"source": "trait", "key": "plan"}}),
            ))
            .await;
        assert_eq!(
            rows(&by_trait),
            [(Value::Null, 2.0), (json!("free"), 1.0), (json!("pro"), 1.0)],
            "the anonymous and cookieless people have no traits"
        );

        let share = s
            .query(created(
                &json!({"measure": "share", "breakdown": {"source": "trait", "key": "plan"}}),
            ))
            .await;
        assert_eq!(share["active"], 4);
        assert_eq!(
            rows(&share),
            [(json!("free"), 0.25), (json!("pro"), 0.25), (Value::Null, 0.25)],
            "cookieless left out"
        );
    }

    #[tokio::test]
    async fn the_rest_of_a_breakdown_is_one_row() {
        let s = setup().await;
        let mut events: Vec<Value> = (0..12)
            .map(|n| json!({"name": "pick", "user_id": "ada", "props": {"k": format!("v{n:02}")}}))
            .collect();
        events.push(json!({"name": "pick", "user_id": "bob", "props": {"k": "v11"}}));
        s.ingest(Value::Array(events)).await;
        let by = json!({"source": "property", "key": "k"});
        let total = s.query(json!({"kind": "events", "event": "pick", "breakdown": by})).await;
        let top = rows(&total);
        assert_eq!(top.len(), 10);
        assert_eq!(top[0], (json!("v11"), 2.0));
        assert_eq!(total["other"], json!({"value": null, "result": 2.0}), "v09 and v10");

        let users = s
            .query(json!({"kind": "events", "event": "pick", "measure": "users", "breakdown": by}))
            .await;
        assert_eq!(users["value"], 2.0);
        assert_eq!(users["other"]["result"], 1.0, "ada once, not once per value");
    }

    #[tokio::test]
    async fn people_count_by_trait() {
        let s = setup().await;
        s.people().await;
        let tokens = json!([{"key": "api_tokens", "op": "gt", "value": 0}]);
        let count = s.query(json!({"kind": "people", "filters": tokens})).await;
        assert_eq!(count, json!({"view": "count", "people": 2, "known": 4}), "cy's \"3\" is text");

        let by_plan = s
            .query(json!({"kind": "people", "filters": tokens,
                          "breakdown": {"source": "trait", "key": "plan"}}))
            .await;
        assert_eq!(by_plan["value"], 2.0);
        assert_eq!(by_plan["active"], 4);
        assert_eq!(
            rows(&by_plan),
            [(json!("pro"), 1.0), (Value::Null, 1.0)],
            "not set goes last on a tie"
        );
    }

    #[tokio::test]
    async fn ranges_before_raw_retention_say_so() {
        let s = setup().await;
        s.people().await;
        let result = s.query_range("2025-09-01", DAY1, created(&json!({}))).await;
        assert_eq!(result["interval"], "day");
        assert_eq!(result["raw_since"], "2025-10-03", "90 days before 2026-01-01");
        assert_eq!(result["value"], 10.0);
    }

    #[tokio::test]
    async fn bad_queries_are_refused() {
        let s = setup().await;
        let invalid = json!({"project_id": s.project(), "from": DAY1, "to": DAY1,
                          "query": {"kind": "people", "event": "x"}});
        s.member
            .post(&s.path("insights/query"), &invalid)
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
        let no_site = json!({"project_id": s.team.projects["app"].id, "from": DAY1, "to": DAY1,
                          "query": {"kind": "people"}});
        s.member.post(&s.path("insights/query"), &no_site).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_insight_keeps_its_display() {
        let s = setup().await;
        let query = json!({"kind": "events", "event": "signup"});
        let plain: Value = s
            .member
            .post(
                &s.path("insights"),
                &json!({"project_id": s.project(), "name": "Plain", "query": query}),
            )
            .await
            .expect(StatusCode::CREATED)
            .json();
        assert_eq!(plain["display"], "auto");

        let body =
            json!({"project_id": s.project(), "name": "Big", "query": query, "display": "number"});
        let big: Value =
            s.member.post(&s.path("insights"), &body).await.expect(StatusCode::CREATED).json();
        assert_eq!(big["display"], "number");
        let path = s.path(&format!("insights/{}", big["id"].as_str().unwrap()));

        let bars: Value =
            s.member.patch(&path, &json!({"display": "bar"})).await.expect(StatusCode::OK).json();
        assert_eq!(bars["display"], "bar");
        let renamed: Value =
            s.member.patch(&path, &json!({"name": "Bars"})).await.expect(StatusCode::OK).json();
        assert_eq!(renamed["display"], "bar", "a change without display keeps it");
        let got: Value = s.member.get(&path).await.expect(StatusCode::OK).json();
        assert_eq!(got["display"], "bar");

        s.member
            .patch(&path, &json!({"display": "pie"}))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn insights_and_dashboards_are_saved() {
        let s = setup().await;
        let save = |name: &str| {
            json!({"project_id": s.project(), "name": name,
                   "query": {"kind": "events", "event": "signup", "measure": "users"}})
        };
        let a: Value = s
            .member
            .post(&s.path("insights"), &save("Signups"))
            .await
            .expect(StatusCode::CREATED)
            .json();
        assert_eq!(a["range"], "30d");
        assert_eq!(a["dashboards"], json!([]));
        assert_eq!(a["query"], json!({"kind": "events", "event": "signup", "measure": "users"}));
        let a_id = a["id"].as_str().unwrap().to_owned();
        let b: Value = s
            .member
            .post(&s.path("insights"), &save("Activations"))
            .await
            .expect(StatusCode::CREATED)
            .json();
        let b_id = b["id"].as_str().unwrap().to_owned();

        let changed: Value = s
            .member
            .patch(
                &s.path(&format!("insights/{a_id}")),
                &json!({"name": "All signups", "range": "7d"}),
            )
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(
            (changed["name"].as_str(), changed["range"].as_str()),
            (Some("All signups"), Some("7d"))
        );

        let body = json!({"project_id": s.project(), "name": "Growth",
                          "tiles": [{"insight_id": a_id, "width": 6}]});
        let board: Value =
            s.member.post(&s.path("dashboards"), &body).await.expect(StatusCode::CREATED).json();
        let board_id = board["id"].as_str().unwrap().to_owned();
        assert_eq!(board["tiles"], json!([{"insight_id": a_id, "width": 6}]));

        let got: Value =
            s.member.get(&s.path(&format!("insights/{a_id}"))).await.expect(StatusCode::OK).json();
        assert_eq!(got["dashboards"], json!([{"id": board_id, "name": "Growth"}]));

        let board_path = s.path(&format!("dashboards/{board_id}"));
        let laid_out = json!([{"insight_id": b_id, "width": 12}, {"insight_id": a_id, "width": 4}]);
        let reordered: Value = s
            .member
            .patch(&board_path, &json!({"tiles": laid_out}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(reordered["tiles"], laid_out);
        s.member.delete(&s.path(&format!("insights/{a_id}"))).await.expect(StatusCode::NO_CONTENT);
        let after: Value = s.member.get(&board_path).await.expect(StatusCode::OK).json();
        assert_eq!(
            after["tiles"],
            json!([{"insight_id": b_id, "width": 12}]),
            "a deleted insight leaves its dashboards"
        );
        let list: Value = s
            .member
            .get(&s.path(&format!("dashboards?project_id={}", s.project())))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(list["items"][0]["name"], "Growth");
        let insights: Value = s
            .member
            .get(&s.path(&format!("insights?project_id={}", s.project())))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(insights["items"].as_array().unwrap().len(), 1);
        assert_eq!(insights["items"][0]["dashboards"][0]["name"], "Growth");

        s.member.delete(&board_path).await.expect(StatusCode::NO_CONTENT);
        s.member.get(&board_path).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn dashboard_tiles_are_unique_known_widths_up_to_the_cap() {
        let s = setup().await;
        let insight: Value = s
            .member
            .post(
                &s.path("insights"),
                &json!({"project_id": s.project(), "name": "People", "query": {"kind": "people"}}),
            )
            .await
            .expect(StatusCode::CREATED)
            .json();
        let id = insight["id"].as_str().unwrap().to_owned();
        for width in [0, 3, 5, 7, 13] {
            let body = json!({"project_id": s.project(), "name": "Odd",
                              "tiles": [{"insight_id": id, "width": width}]});
            let refused: Value = s
                .member
                .post(&s.path("dashboards"), &body)
                .await
                .expect(StatusCode::UNPROCESSABLE_ENTITY)
                .json();
            assert!(refused.to_string().contains("tiles[0].width"), "width {width}: {refused}");
        }
        let repeated = json!({"project_id": s.project(), "name": "Twice",
                              "tiles": [{"insight_id": id, "width": 6}, {"insight_id": id, "width": 6}]});
        s.member
            .post(&s.path("dashboards"), &repeated)
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
        for width in [4, 6, 8, 12] {
            let tiles = json!([{"insight_id": id, "width": width}]);
            let body =
                json!({"project_id": s.project(), "name": format!("W{width}"), "tiles": tiles});
            let board: Value = s
                .member
                .post(&s.path("dashboards"), &body)
                .await
                .expect(StatusCode::CREATED)
                .json();
            let board_path = s.path(&format!("dashboards/{}", board["id"].as_str().unwrap()));
            let renamed: Value = s
                .member
                .patch(&board_path, &json!({"name": "Renamed"}))
                .await
                .expect(StatusCode::OK)
                .json();
            assert_eq!(renamed["tiles"], tiles, "a rename keeps the tiles and their widths");
        }
        let mut ids = Vec::new();
        for n in 0..=20 {
            let made: Value = s
                .member
                .post(
                    &s.path("insights"),
                    &json!({"project_id": s.project(), "name": format!("I{n}"),
                            "query": {"kind": "people"}}),
                )
                .await
                .expect(StatusCode::CREATED)
                .json();
            ids.push(made["id"].as_str().unwrap().to_owned());
        }
        let tiles = |count: usize| -> Vec<Value> {
            ids[..count].iter().map(|each| json!({"insight_id": each, "width": 4})).collect()
        };
        let full = json!({"project_id": s.project(), "name": "Full", "tiles": tiles(20)});
        s.member.post(&s.path("dashboards"), &full).await.expect(StatusCode::CREATED);
        let over = json!({"project_id": s.project(), "name": "Over", "tiles": tiles(21)});
        s.member.post(&s.path("dashboards"), &over).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn patching_tiles_refuses_a_repeated_insight() {
        let s = setup().await;
        let insight: Value = s
            .member
            .post(
                &s.path("insights"),
                &json!({"project_id": s.project(), "name": "People", "query": {"kind": "people"}}),
            )
            .await
            .expect(StatusCode::CREATED)
            .json();
        let tiles = json!([{"insight_id": insight["id"], "width": 8}]);
        let body = json!({"project_id": s.project(), "name": "Once", "tiles": tiles});
        let board: Value =
            s.member.post(&s.path("dashboards"), &body).await.expect(StatusCode::CREATED).json();
        let board_path = s.path(&format!("dashboards/{}", board["id"].as_str().unwrap()));
        let repeated = json!({"tiles": [{"insight_id": insight["id"], "width": 6},
                                        {"insight_id": insight["id"], "width": 12}]});
        let refused: Value = s
            .member
            .patch(&board_path, &repeated)
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY)
            .json();
        assert!(refused.to_string().contains("must not repeat an insight"), "{refused}");
        let kept: Value = s.member.get(&board_path).await.expect(StatusCode::OK).json();
        assert_eq!(kept["tiles"], tiles, "a refused update changes nothing");
    }

    #[tokio::test]
    async fn dashboards_only_hold_their_projects_insights() {
        let s = setup().await;
        let app_project = s.team.projects["app"].id.to_string();
        s.admin
            .post(&s.path("site"), &json!({"project_id": app_project}))
            .await
            .expect(StatusCode::CREATED);
        let other: Value = s
            .member
            .post(
                &s.path("insights"),
                &json!({"project_id": app_project, "name": "Elsewhere", "query": {"kind": "people"}}),
            )
            .await
            .expect(StatusCode::CREATED)
            .json();
        let body = json!({"project_id": s.project(), "name": "Mixed",
                          "tiles": [{"insight_id": other["id"], "width": 6}]});
        s.member.post(&s.path("dashboards"), &body).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let none: Value = s
            .member
            .get(&s.path(&format!("dashboards?project_id={}", s.project())))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(none["items"], json!([]), "the refused dashboard was not kept");
    }

    #[tokio::test]
    async fn saving_needs_write_access() {
        let s = setup().await;
        let key = s.app.insert_api_key(s.team.workspaces["main"].id, &["analytics:read"]).await;
        let reader = s.app.with_key(&key);
        let body = json!({"project_id": s.project(), "name": "Mine", "query": {"kind": "people"}});
        reader.post(&s.path("insights"), &body).await.expect(StatusCode::FORBIDDEN);
        reader
            .get(&s.path(&format!("insights?project_id={}", s.project())))
            .await
            .expect(StatusCode::OK);
        let run = json!({"project_id": s.project(), "from": DAY1, "to": DAY1, "query": {"kind": "people"}});
        reader.post(&s.path("insights/query"), &run).await.expect(StatusCode::OK);
    }
}
