//! Web reports and the daily rollup: numbers must not change when a day
//! moves from raw events to rollups.

#[cfg(test)]
mod reports {
    use std::time::Duration;

    use bizkit_analytics::Analytics;
    use bizkit_testkit::{Manifest, Persona, RawBody, SpecClient, TestApp};
    use http::{HeaderName, Method, StatusCode, header};
    use serde_json::{Value, json};

    const CHROME: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    const SAFARI: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1";
    const DAY1: &str = "2026-01-01";
    const DAY2: &str = "2026-01-02";

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        key: String,
    }

    async fn setup(config: Value) -> Setup {
        let app = TestApp::builder()
            .module(Analytics)
            .config(move |c| {
                c.trust_proxy_headers = true;
                c.modules.insert("analytics".to_owned(), config);
            })
            .build()
            .await;
        let team = app.seed(Persona::Team, "reports").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let mut s = Setup { app, team, admin, key: String::new() };
        let site: Value = s
            .admin
            .post(&s.path("site"), &json!({"project_id": s.project()}))
            .await
            .expect(StatusCode::CREATED)
            .json();
        s.key = site["key"].as_str().unwrap().to_owned();
        s.admin
            .post(
                &s.path("goals"),
                &json!({"project_id": s.project(), "name": "Signup", "kind": "event", "match": "signup"}),
            )
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

        async fn send(
            &self,
            name: &str,
            url: &str,
            referrer: Option<&str>,
            (ua, ip): (&str, &str),
        ) {
            let mut beacon = json!({"s": self.key, "n": name, "u": url});
            if let Some(referrer) = referrer {
                beacon["r"] = json!(referrer);
            }
            let headers =
                [(header::USER_AGENT, ua), (HeaderName::from_static("x-forwarded-for"), ip)];
            let raw = RawBody {
                content_type: "text/plain",
                body: serde_json::to_vec(&beacon).unwrap().into(),
                headers: &headers,
            };
            self.app
                .client()
                .send_body(Method::POST, "/m/analytics/public/e", raw)
                .await
                .expect(StatusCode::ACCEPTED);
        }

        /// Day one: two visitors, four pageviews, one signup.
        async fn day_one(&self) {
            let hn = Some("https://news.ycombinator.com/");
            self.send("pageview", "https://example.com/", hn, (CHROME, "203.0.113.1")).await;
            self.send("pageview", "https://example.com/pricing", None, (CHROME, "203.0.113.1"))
                .await;
            self.send("signup", "https://example.com/pricing", None, (CHROME, "203.0.113.1")).await;
            self.send("pageview", "https://example.com/", None, (SAFARI, "203.0.113.2")).await;
            self.send(
                "pageview",
                "https://example.com/?utm_source=mail",
                None,
                (SAFARI, "203.0.113.2"),
            )
            .await;
        }

        async fn summary(&self, from: &str, to: &str, compare: bool) -> Value {
            let path = format!(
                "{}?project_id={}&from={from}&to={to}&compare={compare}",
                self.path("stats/summary"),
                self.project()
            );
            self.admin.get(&path).await.expect(StatusCode::OK).json()
        }

        async fn breakdown(&self, dimension: &str, from: &str, to: &str) -> Vec<(String, i64)> {
            let path = format!(
                "{}?project_id={}&from={from}&to={to}&dimension={dimension}",
                self.path("stats/breakdown"),
                self.project()
            );
            let body: Value = self.admin.get(&path).await.expect(StatusCode::OK).json();
            body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    (row["value"].as_str().unwrap().to_owned(), row["visitors"].as_i64().unwrap())
                })
                .collect()
        }

        async fn daily_rows(&self) -> i64 {
            sqlx::query_scalar("SELECT count(*) FROM analytics_daily")
                .fetch_one(self.app.app().platform().db().read())
                .await
                .unwrap()
        }

        async fn roll_into_day_two(&self) {
            self.app.clock.advance(Duration::from_hours(24));
            self.send("pageview", "https://example.com/", None, (CHROME, "203.0.113.3")).await;
            self.app.clock.advance(Duration::from_mins(11));
            self.app.drain_jobs().await;
        }
    }

    #[tokio::test]
    async fn reports_read_raw_events_for_today() {
        let s = setup(Value::Null).await;
        s.day_one().await;
        let summary = s.summary(DAY1, DAY1, false).await;
        assert_eq!(
            summary["totals"],
            json!({"visitors": 2, "pageviews": 4, "events": 1, "conversions": 1})
        );
        assert_eq!(summary["interval"], "hour");
        assert_eq!(summary["series"].as_array().unwrap().len(), 24);
        assert_eq!(summary["series"][0]["visitors"], 2);
        assert_eq!(summary["previous"], Value::Null);

        assert_eq!(
            s.breakdown("page", DAY1, DAY1).await,
            [("/".to_owned(), 2), ("/pricing".to_owned(), 1)]
        );
        assert_eq!(
            s.breakdown("referrer", DAY1, DAY1).await,
            [(String::new(), 2), ("news.ycombinator.com".to_owned(), 1)],
            "direct traffic is the empty value"
        );
        assert_eq!(
            s.breakdown("browser", DAY1, DAY1).await,
            [("Chrome".to_owned(), 1), ("Safari".to_owned(), 1)]
        );
        assert_eq!(s.breakdown("country", DAY1, DAY1).await, [(String::new(), 2)], "no GeoIP file");
        assert_eq!(s.breakdown("event", DAY1, DAY1).await, [("signup".to_owned(), 1)]);
        assert_eq!(
            s.breakdown("utm_source", DAY1, DAY1).await,
            [(String::new(), 2), ("mail".to_owned(), 1)]
        );
    }

    #[tokio::test]
    async fn the_rollup_keeps_the_numbers() {
        let s = setup(Value::Null).await;
        s.day_one().await;
        let before = s.summary(DAY1, DAY1, false).await["totals"].clone();
        let dimensions =
            ["page", "referrer", "utm_source", "device", "browser", "os", "event", "goal"];
        let mut breakdowns = Vec::new();
        for dimension in dimensions {
            breakdowns.push(s.breakdown(dimension, DAY1, DAY1).await);
        }

        s.roll_into_day_two().await;
        assert!(s.daily_rows().await > 0, "day one is rolled up");
        let rolled: Option<String> =
            sqlx::query_scalar("SELECT rolled_through FROM analytics_sites")
                .fetch_one(s.app.app().platform().db().read())
                .await
                .unwrap();
        assert_eq!(rolled.as_deref(), Some(DAY1));

        let two_days = s.summary(DAY1, DAY2, false).await;
        assert_eq!(two_days["interval"], "day");
        assert_eq!(two_days["totals"]["visitors"], 3, "summed per day");
        assert_eq!(
            two_days["series"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["visitors"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            [2, 1]
        );
        let mut tx = s.app.app().platform().db().write().await.unwrap();
        sqlx::query("DELETE FROM analytics_events WHERE day = ?")
            .bind(DAY1)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            s.summary(DAY1, DAY1, false).await["totals"],
            before,
            "day one now comes from rollups"
        );
        for (dimension, want) in dimensions.iter().zip(&breakdowns) {
            assert_eq!(&s.breakdown(dimension, DAY1, DAY1).await, want, "{dimension}");
        }

        let rows = s.daily_rows().await;
        s.app.clock.advance(Duration::from_hours(1));
        let platform = s.app.app().platform();
        let mut rerun = platform.db().write().await.unwrap();
        sqlx::query("UPDATE analytics_sites SET rolled_through = NULL")
            .execute(&mut *rerun)
            .await
            .unwrap();
        let job = json!({"workspace_id": s.team.workspaces["main"].id, "project_id": s.project()})
            .to_string();
        sqlx::query("INSERT INTO jobs (kind, payload, status, attempts, max_attempts, run_at, created_at, updated_at) VALUES ('analytics.rollup', ?, 'queued', 0, 5, 0, 0, 0)")
            .bind(job)
            .execute(&mut *rerun)
            .await
            .unwrap();
        rerun.commit().await.unwrap();
        s.app.drain_jobs().await;
        assert_eq!(s.daily_rows().await, rows, "a rerun changes nothing");
    }

    #[tokio::test]
    async fn goals_rate_over_visitors_and_compare_periods() {
        let s = setup(Value::Null).await;
        s.day_one().await;
        s.roll_into_day_two().await;
        let goal = format!(
            "{}?project_id={}&from={DAY1}&to={DAY1}&dimension=goal",
            s.path("stats/breakdown"),
            s.project()
        );
        let body: Value = s.admin.get(&goal).await.expect(StatusCode::OK).json();
        assert_eq!(body["items"][0]["value"], "Signup");
        assert_eq!(body["items"][0]["visitors"], 1);
        assert_eq!(body["items"][0]["conversion_rate"], 0.5);

        let compared = s.summary(DAY2, DAY2, true).await;
        assert_eq!(compared["totals"]["visitors"], 1);
        assert_eq!(compared["previous"]["visitors"], 2, "the day before");
        assert_eq!(compared["previous_series"].as_array().unwrap().len(), 24);
    }

    #[tokio::test]
    async fn raw_events_past_retention_are_pruned() {
        let s = setup(json!({"raw_retention_days": 7})).await;
        s.day_one().await;
        s.app.clock.advance(Duration::from_hours(24 * 9));
        s.send("pageview", "https://example.com/", None, (CHROME, "203.0.113.3")).await;
        s.app.clock.advance(Duration::from_mins(11));
        s.app.drain_jobs().await;
        let raw: i64 = sqlx::query_scalar("SELECT count(*) FROM analytics_events")
            .fetch_one(s.app.app().platform().db().read())
            .await
            .unwrap();
        assert_eq!(raw, 1, "only today's event is left");
        assert_eq!(
            s.summary(DAY1, DAY1, false).await["totals"]["pageviews"],
            4,
            "rollups keep day one"
        );
    }

    #[tokio::test]
    async fn bad_ranges_and_unknown_sites() {
        let s = setup(Value::Null).await;
        let path = |from: &str, to: &str| {
            format!("{}?project_id={}&from={from}&to={to}", s.path("stats/summary"), s.project())
        };
        s.admin.get(&path(DAY2, DAY1)).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
        s.admin
            .get(&path("2025-01-01", "2026-02-05"))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
        s.admin.get(&path("2026-13-01", DAY1)).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let other = format!(
            "{}?project_id={}&from={DAY1}&to={DAY1}",
            s.path("stats/summary"),
            s.team.projects["app"].id
        );
        s.admin.get(&other).await.expect(StatusCode::NOT_FOUND);
    }
}
