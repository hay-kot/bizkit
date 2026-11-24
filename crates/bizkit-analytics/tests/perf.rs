//! `mise run analytics:perf`: report timings over 1M events in 90 days.
//! Ignored by `cargo test`; run it in release.

#[cfg(test)]
mod perf {
    use std::time::{Duration, Instant};

    use bizkit_analytics::Analytics;
    use bizkit_analytics::testing::project_pool;
    use bizkit_testkit::{Manifest, Persona, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    const EVENTS: i64 = 1_000_000;
    const DAYS: i64 = 90;
    const BUDGET: Duration = Duration::from_millis(500);

    #[tokio::test]
    #[ignore = "slow; run with mise run analytics:perf"]
    async fn reports_stay_fast_over_a_million_events() {
        let app = TestApp::builder().module(Analytics::manual_flush()).build().await;
        let team = app.seed(Persona::Team, "perf").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let (ws, project) = (team.workspaces["main"].id, team.projects["site"].id.to_string());
        let base = format!("/api/v1/workspaces/{ws}/analytics");
        admin
            .post(&format!("{base}/site"), &json!({"project_id": project}))
            .await
            .expect(StatusCode::CREATED);

        let now = app.app().platform().now().0;
        let start = now - DAYS * 86_400_000;
        let loading = Instant::now();
        load_events(&app, &team, start).await;
        println!("loaded {EVENTS} events in {:?}", loading.elapsed());

        let from = time_of(start);
        let to = time_of(now - 86_400_000);
        let summary =
            format!("{base}/stats/summary?project_id={project}&from={from}&to={to}&compare=true");
        let breakdown = format!(
            "{base}/stats/breakdown?project_id={project}&from={from}&to={to}&dimension=page"
        );

        // Before the rollup every day is raw. Reports normally read raw only
        // for today, so this is a worst case with no budget.
        let raw = timed("summary, all raw (no budget)", None, || admin.get(&summary)).await;

        let client = app.client();
        let rolling = Instant::now();
        let key: Value = admin.get(&format!("{base}/site?project_id={project}")).await.json();
        let beacon = json!({"s": key["key"], "n": "pageview", "u": "https://example.com/"});
        let headers = [(
            http::header::USER_AGENT,
            "Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0",
        )];
        let body = bizkit_testkit::RawBody {
            content_type: "text/plain",
            body: serde_json::to_vec(&beacon).unwrap().into(),
            headers: &headers,
        };
        client
            .send_body(http::Method::POST, "/m/analytics/public/e", body)
            .await
            .expect(StatusCode::ACCEPTED);
        // The beacon waits in the writer's queue; writing it queues the rollup.
        app.flush().await;
        app.clock.advance(Duration::from_mins(11));
        app.drain_jobs().await;
        println!("rolled up {DAYS} days in {:?}", rolling.elapsed());

        let rolled = timed("summary", Some(BUDGET), || admin.get(&summary)).await;
        assert_eq!(raw["totals"], rolled["totals"], "rollups give the raw numbers");
        timed("breakdown", Some(BUDGET), || admin.get(&breakdown)).await;

        // The demo dashboard's four insights over the last 30 days, where
        // every day is raw; a dashboard loads them side by side.
        let month = time_of(now - 30 * 86_400_000);
        let insights = [
            json!({"kind": "people", "filters": [{"key": "api_tokens", "op": "gt", "value": 0}],
                   "breakdown": {"source": "trait", "key": "plan"}}),
            json!({"kind": "events", "event": "project_opened", "measure": "share"}),
            json!({"kind": "events", "event": "project_opened", "measure": "per_user"}),
            json!({"kind": "events", "event": "signup",
                   "breakdown": {"source": "property", "key": "plan"}}),
        ];
        let started = Instant::now();
        for query in insights {
            let subject = query["event"].as_str().unwrap_or("people");
            let label =
                format!("insight {subject} {}", query["measure"].as_str().unwrap_or("count"));
            let run = json!({"project_id": project, "from": month, "to": to, "query": query});
            let path = format!("{base}/insights/query");
            timed(&label, Some(BUDGET), || admin.post(&path, &run)).await;
        }
        let dashboard = started.elapsed();
        println!("dashboard, four insights one after another: {dashboard:?}");
        assert!(dashboard < Duration::from_secs(1), "the dashboard took {dashboard:?}");
    }

    /// Inserts [`EVENTS`] events over [`DAYS`] days from `start` straight
    /// into the project's file, and marks the site as having events.
    async fn load_events(app: &TestApp, team: &Manifest, start: i64) {
        let ws = team.workspaces["main"].id;
        let project = team.projects["site"].id.to_string();
        let platform = app.app().platform();
        let file = project_pool(app.data_dir(), team.projects["site"].id).await.unwrap();
        let mut tx = file.begin().await.unwrap();
        sqlx::query(
            "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < ? - 1)
             INSERT INTO analytics_events (workspace_id, project_id, ts, day, name, path, referrer_host,
                                           country, device, browser, os, visitor, person, props, source)
             SELECT ?, ?, ? + (i % ?) * 86400000 + (i % 86400000),
                    date((? + (i % ?) * 86400000) / 1000, 'unixepoch'),
                    CASE i % 10 WHEN 0 THEN 'signup' WHEN 1 THEN 'project_opened' ELSE 'pageview' END,
                    '/p' || (i % 300),
                    CASE i % 4 WHEN 0 THEN 'google.com' WHEN 1 THEN 'news.ycombinator.com' ELSE NULL END,
                    CASE i % 3 WHEN 0 THEN 'US' WHEN 1 THEN 'DE' ELSE 'GB' END,
                    'desktop', 'Chrome', 'macOS',
                    (i * 7919) % 5000,
                    CASE WHEN (i * 7919) % 2 = 0 THEN 'p:' ELSE 'v:' END || ((i * 7919) % 5000),
                    CASE WHEN i % 10 < 2
                         THEN json_object('plan', CASE i % 3 WHEN 0 THEN 'pro' ELSE 'free' END, 'n', i % 7)
                    END,
                    'web'
             FROM n",
        )
        .bind(EVENTS)
        .bind(ws.to_string())
        .bind(&project)
        .bind(start)
        .bind(DAYS)
        .bind(start)
        .bind(DAYS)
        .execute(&mut *tx)
        .await
        .unwrap();
        // Every other visitor is an identified person with traits, so share,
        // per-user and people insights have rows to read.
        sqlx::query(
            "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 2 FROM n WHERE i < 4998)
             INSERT INTO analytics_people (id, workspace_id, project_id, user_id, traits, first_seen, last_seen)
             SELECT CAST(i AS TEXT), ?, ?, 'u' || i,
                    json_object('plan', CASE i % 3 WHEN 0 THEN 'pro' ELSE 'free' END, 'api_tokens', i % 3),
                    ?, ?
             FROM n",
        )
        .bind(ws.to_string())
        .bind(&project)
        .bind(start)
        .bind(start)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let mut app_tx = platform.db().write().await.unwrap();
        sqlx::query("UPDATE analytics_sites SET last_event_day = date(? / 1000, 'unixepoch')")
            .bind(start)
            .execute(&mut *app_tx)
            .await
            .unwrap();
        app_tx.commit().await.unwrap();
    }

    fn time_of(millis: i64) -> String {
        let at = time::OffsetDateTime::from_unix_timestamp(millis / 1000).unwrap();
        format!("{:04}-{:02}-{:02}", at.year(), u8::from(at.month()), at.day())
    }

    async fn timed<F, Fut>(label: &str, budget: Option<Duration>, call: F) -> Value
    where
        F: Fn() -> Fut,
        Fut: Future<Output = bizkit_testkit::ApiResponse>,
    {
        let started = Instant::now();
        let response = call().await;
        let took = started.elapsed();
        println!("{label}: {took:?}");
        response.expect(StatusCode::OK);
        assert!(budget.is_none_or(|budget| took < budget), "{label} took {took:?}");
        response.json()
    }
}
