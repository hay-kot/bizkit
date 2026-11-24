//! The beacon endpoint and snippet: what gets stored, what gets dropped, and
//! how identity links events to people.

#[cfg(test)]
mod ingest {
    use std::time::Duration;

    use bizkit_analytics::Analytics;
    use bizkit_analytics::hooks::GOAL_CONVERTED;
    use bizkit_analytics::testing::project_pool;
    use bizkit_core::jobs::JobCtx;
    use bizkit_testkit::{FakeHooks, Manifest, Persona, RawBody, SpecClient, TestApp};
    use http::{HeaderName, Method, StatusCode, header};
    use serde_json::{Value, json};

    const CHROME: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    const FIREFOX: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0";
    const COLLECT: &str = "/m/analytics/public/e";

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        anon: SpecClient,
        key: String,
        hooks: FakeHooks,
    }

    #[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
    struct Event {
        name: String,
        path: Option<String>,
        person: String,
        referrer_host: Option<String>,
        utm_source: Option<String>,
        browser: Option<String>,
        os: Option<String>,
        device: Option<String>,
        anon_id: Option<String>,
        props: Option<String>,
    }

    async fn setup(mode: &str) -> Setup {
        setup_with(Analytics::manual_flush(), mode).await
    }

    async fn setup_with(analytics: Analytics, mode: &str) -> Setup {
        let hooks = FakeHooks::default();
        let app = TestApp::builder()
            .module(analytics)
            .hook_runner(hooks.clone())
            .config(|config| config.trusted_proxies = 1)
            .build()
            .await;
        let team = app.seed(Persona::Team, "analytics").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let anon = app.client();
        let mut s = Setup { app, team, admin, anon, key: String::new(), hooks };
        let site: Value = s
            .admin
            .post(&s.path("site"), &s.body(&json!({})))
            .await
            .expect(StatusCode::CREATED)
            .json();
        s.key = site["key"].as_str().unwrap().to_owned();
        s.admin
            .patch(&s.path("site"), &s.body(&json!({"identity_mode": mode})))
            .await
            .expect(StatusCode::OK);
        s
    }

    impl Setup {
        fn path(&self, rest: &str) -> String {
            format!("/api/v1/workspaces/{}/analytics/{rest}", self.team.workspaces["main"].id)
        }

        fn body(&self, extra: &Value) -> Value {
            let mut body = json!({"project_id": self.team.projects["site"].id.to_string()});
            body.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            body
        }

        async fn raw(&self, body: Vec<u8>, ua: &str, ip: &str) {
            let headers =
                [(header::USER_AGENT, ua), (HeaderName::from_static("x-forwarded-for"), ip)];
            let raw = RawBody {
                content_type: "text/plain;charset=UTF-8",
                body: body.into(),
                headers: &headers,
            };
            let response = self.anon.send_body(Method::POST, COLLECT, raw).await;
            response.expect(StatusCode::ACCEPTED);
            assert!(response.text.is_empty(), "no body");
        }

        async fn send(&self, mut beacon: Value, ip: &str) {
            let fields = beacon.as_object_mut().unwrap();
            fields.entry("s").or_insert_with(|| json!(self.key));
            fields.entry("u").or_insert_with(|| json!("https://example.com/"));
            self.raw(serde_json::to_vec(&beacon).unwrap(), CHROME, ip).await;
        }

        /// Flushes queued events and opens the project's file.
        async fn file(&self) -> sqlx::SqlitePool {
            self.app.flush().await;
            let project = self.team.projects["site"].id;
            project_pool(self.app.data_dir(), project).await.unwrap()
        }

        async fn events(&self) -> Vec<Event> {
            sqlx::query_as(
                "SELECT name, path, person, referrer_host, utm_source, browser, os, device, anon_id, props
                 FROM analytics_events ORDER BY id",
            )
            .fetch_all(&self.file().await)
            .await
            .unwrap()
        }

        async fn count(&self, sql: &'static str) -> i64 {
            sqlx::query_scalar(sql).fetch_one(&self.file().await).await.unwrap()
        }
    }

    #[tokio::test]
    async fn the_snippet_is_per_site_and_carries_the_mode() {
        let s = setup("cookieless").await;
        let site: Value = s
            .admin
            .get(&format!("{}?project_id={}", s.path("site"), s.team.projects["site"].id))
            .await
            .expect(StatusCode::OK)
            .json();
        let src = format!("/m/analytics/public/s/{}.js", s.key);
        assert!(site["snippet"].as_str().unwrap().contains(&src), "{}", site["snippet"]);

        let script = s.anon.get(&src).await;
        script.expect(StatusCode::OK);
        assert_eq!(script.headers[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
        assert!(
            script.text.contains(&format!(r#"var K = "{}", M = "cookieless""#, s.key)),
            "{}",
            script.text
        );
        assert!(script.text.contains("/m/analytics/public/e\""), "endpoint");

        s.admin
            .patch(&s.path("site"), &s.body(&json!({"identity_mode": "always"})))
            .await
            .expect(StatusCode::OK);
        assert!(
            s.anon.get(&src).await.text.contains(r#"M = "always""#),
            "a mode change drops the cache"
        );
        s.anon.get("/m/analytics/public/s/nope.js").await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn pageviews_are_enriched() {
        let s = setup("cookieless").await;
        s.send(
            json!({"n": "pageview", "u": "https://www.example.com/pricing?utm_source=hn", "r": "https://news.ycombinator.com/x", "w": 1440, "p": {"plan": "pro"}}),
            "203.0.113.1",
        )
        .await;
        let events = s.events().await;
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.path.as_deref(), Some("/pricing"));
        assert_eq!(event.referrer_host.as_deref(), Some("news.ycombinator.com"));
        assert_eq!(event.utm_source.as_deref(), Some("hn"));
        assert_eq!(
            (event.browser.as_deref(), event.os.as_deref(), event.device.as_deref()),
            (Some("Chrome"), Some("macOS"), Some("desktop"))
        );
        assert_eq!(event.props.as_deref(), Some(r#"{"plan":"pro"}"#));
        assert!(event.person.starts_with("v:2026-01-01:"), "{}", event.person);

        let site: Value = s
            .admin
            .get(&format!("{}?project_id={}", s.path("site"), s.team.projects["site"].id))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(site["has_events"], true);
    }

    #[tokio::test]
    async fn visitors_are_daily_hashes_of_address_and_agent() {
        let s = setup("cookieless").await;
        s.send(json!({"n": "pageview"}), "203.0.113.1").await;
        s.send(json!({"n": "pageview", "u": "https://example.com/b"}), "203.0.113.1").await;
        s.send(json!({"n": "pageview"}), "203.0.113.2").await;
        s.raw(
            serde_json::to_vec(&json!({"s": s.key, "n": "pageview", "u": "https://example.com/"}))
                .unwrap(),
            FIREFOX,
            "203.0.113.1",
        )
        .await;
        s.app.clock.advance(Duration::from_hours(24));
        s.send(json!({"n": "pageview"}), "203.0.113.1").await;

        let people: Vec<String> = s.events().await.into_iter().map(|e| e.person).collect();
        assert_eq!(people[0], people[1], "same address and agent, same day");
        assert_ne!(people[0], people[2], "other address");
        assert_ne!(people[0], people[3], "other agent");
        assert_ne!(people[0], people[4], "next day");
        let salts: i64 = sqlx::query_scalar("SELECT count(*) FROM analytics_salts")
            .fetch_one(s.app.app().platform().db().read())
            .await
            .unwrap();
        assert_eq!(salts, 1, "old salts are deleted");
    }

    #[tokio::test]
    async fn dropped_beacons_still_answer_202() {
        let s = setup("cookieless").await;
        s.admin
            .patch(
                &s.path("site"),
                &s.body(&json!({"hostnames": ["example.com", "*.example.org"]})),
            )
            .await
            .expect(StatusCode::OK);
        let good = json!({"s": s.key, "n": "pageview", "u": "https://example.com/"});
        let body = |value: &Value| serde_json::to_vec(value).unwrap();

        s.raw(body(&good), "Googlebot/2.1 (+http://www.google.com/bot.html)", "203.0.113.1").await;
        s.raw(body(&good), "curl/8.4.0", "203.0.113.1").await;
        s.raw(
            body(&json!({"s": "unknown", "n": "pageview", "u": "https://example.com/"})),
            CHROME,
            "203.0.113.1",
        )
        .await;
        s.raw(
            body(&json!({"s": s.key, "n": "pageview", "u": "https://evil.test/"})),
            CHROME,
            "203.0.113.1",
        )
        .await;
        s.raw(
            body(&json!({"s": s.key, "n": "pageview", "u": "https://example.org/"})),
            CHROME,
            "203.0.113.1",
        )
        .await;
        s.raw(b"{not json".to_vec(), CHROME, "203.0.113.1").await;
        s.raw(vec![b' '; 9 * 1024], CHROME, "203.0.113.1").await;
        assert_eq!(s.events().await.len(), 0, "nothing stored");

        s.raw(
            body(&json!({"s": s.key, "n": "pageview", "u": "https://blog.example.org/"})),
            CHROME,
            "203.0.113.1",
        )
        .await;
        assert_eq!(s.events().await.len(), 1, "a subdomain pattern matches");

        for _ in 0..70 {
            s.raw(body(&good), CHROME, "203.0.113.9").await;
        }
        assert_eq!(s.events().await.len(), 61, "60 per client per minute");
        s.app.clock.advance(Duration::from_secs(60));
        s.raw(body(&good), CHROME, "203.0.113.9").await;
        assert_eq!(s.events().await.len(), 62, "the next minute is a new window");
    }

    #[tokio::test]
    async fn cookieless_sites_ignore_identity() {
        let s = setup("cookieless").await;
        s.send(json!({"n": "pageview", "a": "anon-1", "i": "user_1"}), "203.0.113.1").await;
        s.send(
            json!({"n": "$identify", "a": "anon-1", "i": "user_1", "t": {"email": "a@x.test"}}),
            "203.0.113.1",
        )
        .await;
        let events = s.events().await;
        assert_eq!(events.len(), 1);
        assert!(events[0].person.starts_with("v:"), "{}", events[0].person);
        assert_eq!(events[0].anon_id, None);
        assert_eq!(s.count("SELECT count(*) FROM analytics_people").await, 0);
    }

    #[tokio::test]
    async fn identify_links_this_browser_and_no_other_user() {
        let s = setup("always").await;
        let ip = "203.0.113.1";
        s.send(json!({"n": "pageview", "u": "https://example.com/landing"}), ip).await;
        s.send(json!({"n": "pageview", "u": "https://example.com/pricing", "a": "anon-1"}), ip)
            .await;
        s.send(json!({"n": "pageview"}), "203.0.113.2").await;
        s.send(
            json!({"n": "$identify", "a": "anon-1", "i": "user_1", "t": {"email": "a@x.test"}}),
            ip,
        )
        .await;
        s.send(json!({"n": "signup", "a": "anon-1"}), ip).await;

        let events = s.events().await;
        assert_eq!(events.len(), 4, "identify is not an event");
        let person = &events[1].person;
        assert!(person.starts_with("p:"), "{person}");
        assert_eq!(&events[0].person, person, "same-day visitor rows backfill");
        assert!(events[2].person.starts_with("v:"), "another visitor stays anonymous");
        assert_eq!(&events[3].person, person, "the alias links later events");
        let traits: String =
            sqlx::query_scalar("SELECT traits FROM analytics_people WHERE user_id = 'user_1'")
                .fetch_one(&s.file().await)
                .await
                .unwrap();
        assert_eq!(traits, r#"{"email":"a@x.test"}"#);

        s.send(json!({"n": "$identify", "a": "anon-1", "i": "user_2"}), ip).await;
        s.send(json!({"n": "pageview", "a": "anon-1"}), ip).await;
        let after = s.events().await;
        assert!(
            after[..4].iter().filter(|e| e.person.starts_with("p:")).all(|e| &e.person == person),
            "user_1 keeps its history"
        );
        assert_ne!(&after[4].person, person, "the browser now belongs to user_2");
    }

    #[tokio::test]
    async fn goals_convert_once_per_day_and_person() {
        let s = setup("cookieless").await;
        for (kind, matches) in [("page", "/signup/done"), ("event", "signup")] {
            s.admin
                .post(
                    &s.path("goals"),
                    &s.body(&json!({"name": matches, "kind": kind, "match": matches})),
                )
                .await
                .expect(StatusCode::CREATED);
        }
        let done = json!({"n": "pageview", "u": "https://example.com/signup/done"});
        s.send(done.clone(), "203.0.113.1").await;
        s.send(done.clone(), "203.0.113.1").await;
        s.send(json!({"n": "signup", "u": "https://example.com/signup/done"}), "203.0.113.1").await;
        s.send(done.clone(), "203.0.113.2").await;
        s.send(json!({"n": "pageview", "u": "https://example.com/signup"}), "203.0.113.3").await;
        assert_eq!(s.count("SELECT count(*) FROM analytics_conversions").await, 3);

        s.app.clock.advance(Duration::from_hours(24));
        s.send(done, "203.0.113.1").await;
        assert_eq!(s.count("SELECT count(*) FROM analytics_conversions").await, 4, "a new day");
    }

    #[tokio::test]
    async fn identify_takes_only_recent_visitor_rows() {
        let s = setup("after_identify").await;
        let ip = "203.0.113.1";
        s.send(json!({"n": "pageview", "u": "https://example.com/coworker"}), ip).await;
        s.app.clock.advance(Duration::from_mins(31));
        s.send(json!({"n": "pageview", "u": "https://example.com/pricing"}), ip).await;
        s.send(json!({"n": "$identify", "a": "anon-1", "i": "user_1"}), ip).await;

        let events = s.events().await;
        assert!(events[0].person.starts_with("v:"), "older rows on the hash stay anonymous");
        assert!(events[1].person.starts_with("p:"), "{}", events[1].person);
    }

    #[tokio::test]
    async fn a_conversion_before_identify_counts_for_the_person() {
        let s = setup("after_identify").await;
        s.admin
            .post(
                &s.path("goals"),
                &s.body(&json!({"name": "Signup", "kind": "event", "match": "signup"})),
            )
            .await
            .expect(StatusCode::CREATED);
        let ip = "203.0.113.1";
        s.send(json!({"n": "signup"}), ip).await;
        s.send(json!({"n": "$identify", "a": "anon-1", "i": "user_1"}), ip).await;
        s.send(json!({"n": "signup", "a": "anon-1", "i": "user_1"}), ip).await;
        s.app.drain_jobs().await;
        assert_eq!(s.hooks.dispatches().len(), 1, "{:?}", s.hooks.dispatches());
    }

    #[tokio::test]
    async fn conversions_dispatch_the_goal_hook_once_per_day_and_person() {
        let s = setup("after_identify").await;
        let goal: Value = s
            .admin
            .post(
                &s.path("goals"),
                &s.body(&json!({"name": "Signup", "kind": "event", "match": "signup"})),
            )
            .await
            .expect(StatusCode::CREATED)
            .json();
        s.send(
            json!({"n": "$identify", "a": "anon-1", "i": "user_1", "t": {"email": "ada@x.test"}}),
            "203.0.113.1",
        )
        .await;
        let signup = json!({"n": "signup", "a": "anon-1", "i": "user_1", "p": {"plan": "pro"}});
        s.send(signup.clone(), "203.0.113.1").await;
        s.send(signup.clone(), "203.0.113.1").await;
        s.send(json!({"n": "signup"}), "203.0.113.2").await;
        s.app.drain_jobs().await;

        let dispatches = s.hooks.dispatches();
        assert_eq!(dispatches.len(), 2, "one per person and day: {dispatches:?}");
        let dispatch = &dispatches[0];
        assert_eq!(dispatch.point, GOAL_CONVERTED.id);
        assert_eq!(
            dispatch.target.entity.as_deref(),
            goal["id"].as_str(),
            "the goal is the entity"
        );
        assert_eq!(dispatch.target.project_id, s.team.projects["site"].id);
        let keys: Vec<&String> = dispatch.event.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["event_id", "goal_id", "project_id", "workspace_id"], "IDs only");

        let platform = s.app.app().platform();
        let cx = JobCtx { platform: platform.clone(), job_id: 0, attempt: 1, request_id: None };
        let input = platform.hook_input(&cx, GOAL_CONVERTED.id, &dispatch.event).await.unwrap();
        let input = input.unwrap();
        assert_eq!(input["goal"], json!({"id": goal["id"], "name": "Signup"}));
        assert_eq!(input["event"]["name"], "signup");
        assert_eq!(input["event"]["props"], json!({"plan": "pro"}));
        assert_eq!(
            input["person"],
            json!({"user_id": "user_1", "traits": {"email": "ada@x.test"}})
        );
        let anonymous =
            platform.hook_input(&cx, GOAL_CONVERTED.id, &dispatches[1].event).await.unwrap();
        assert_eq!(anonymous.unwrap()["person"], Value::Null, "no person for visitors");

        s.app.clock.advance(Duration::from_hours(24));
        s.send(signup, "203.0.113.1").await;
        s.app.drain_jobs().await;
        assert_eq!(s.hooks.dispatches().len(), 3, "a new day converts again");

        let goal_path = format!("goals/{}", goal["id"].as_str().unwrap());
        s.admin.delete(&s.path(&goal_path)).await.expect(StatusCode::NO_CONTENT);
        let gone = platform.hook_input(&cx, GOAL_CONVERTED.id, &dispatch.event).await.unwrap();
        assert_eq!(gone, None, "a deleted goal skips the run");
        assert_eq!(s.events().await.len(), 4, "events stay");
    }

    impl Setup {
        async fn key(&self, scope: &str) -> SpecClient {
            let path = format!("/api/v1/workspaces/{}/api-keys", self.team.workspaces["main"].id);
            let made: Value = self
                .admin
                .post(&path, &json!({"name": scope, "scopes": [scope]}))
                .await
                .expect(StatusCode::CREATED)
                .json();
            self.app.with_key(made["token"].as_str().unwrap())
        }
    }

    #[tokio::test]
    async fn servers_ingest_with_a_write_key() {
        let s = setup("cookieless").await;
        let writer = s.key("analytics:write").await;
        let reader = s.key("analytics:read").await;
        let batch = s.body(&json!({"events": [
            {"name": "$identify", "user_id": "user_1", "traits": {"plan": "pro"}},
            {"name": "invoice_paid", "user_id": "user_1", "url": "https://app.example.com/billing?utm_source=mail", "props": {"amount": 49}},
        ]}));
        reader.post(&s.path("events"), &batch).await.expect(StatusCode::FORBIDDEN);
        let done: Value =
            writer.post(&s.path("events"), &batch).await.expect(StatusCode::OK).json();
        assert_eq!(done["accepted"], 2);

        let events = s.events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "invoice_paid");
        assert!(events[0].person.starts_with("p:"), "server events identify in every mode");
        assert_eq!(events[0].path.as_deref(), Some("/billing"));
        assert_eq!(events[0].utm_source.as_deref(), Some("mail"));
        assert_eq!(events[0].browser, None);
        let source: String = sqlx::query_scalar("SELECT source FROM analytics_events")
            .fetch_one(&s.file().await)
            .await
            .unwrap();
        assert_eq!(source, "server");
    }

    #[tokio::test]
    async fn server_identify_links_browser_events() {
        let s = setup("after_identify").await;
        s.send(json!({"n": "pageview", "a": "anon-9"}), "203.0.113.1").await;
        let writer = s.key("analytics:write").await;
        writer
            .post(&s.path("events"), &s.body(&json!({"events": [{"name": "$identify", "user_id": "u9", "anon_id": "anon-9"}]})))
            .await
            .expect(StatusCode::OK);
        assert!(s.events().await[0].person.starts_with("p:"));
    }

    #[tokio::test]
    async fn server_ingest_rejects_bad_batches() {
        let s = setup("cookieless").await;
        let writer = s.key("analytics:write").await;
        let post = |events: Value| {
            let body = s.body(&json!({"events": events}));
            let path = s.path("events");
            let writer = &writer;
            async move { writer.post(&path, &body).await }
        };
        post(json!([])).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let response = post(json!([
            {"name": "$identify"},
            {"name": "bad name", "user_id": "u"},
            {"name": "x", "user_id": "u", "traits": {"a": 1}},
            {"name": "x", "anon_id": "has space", "url": "ftp://x"},
        ]))
        .await;
        response.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let fields = response.body["error"]["fields"].as_object().unwrap();
        for field in [
            "events[0].user_id",
            "events[1].name",
            "events[2].traits",
            "events[3].anon_id",
            "events[3].url",
        ] {
            assert!(fields.contains_key(field), "{field}: {fields:?}");
        }

        let other = s.team.projects["app"].id.to_string();
        writer
            .post(
                &s.path("events"),
                &json!({"project_id": other, "events": [{"name": "x", "user_id": "u"}]}),
            )
            .await
            .expect(StatusCode::NOT_FOUND);
        assert_eq!(s.events().await.len(), 0);
    }

    /// Waits up to two seconds for the file to hold `events` events.
    async fn holds(file: &sqlx::SqlitePool, events: i64) -> bool {
        for _ in 0..100 {
            let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM analytics_events")
                .fetch_one(file)
                .await
                .unwrap();
            if stored >= events {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    /// Builds with the flush timer, unlike the other tests: what it checks
    /// is that timer flushes go on while the app database is busy.
    #[tokio::test]
    async fn a_busy_app_database_does_not_hold_up_ingest() {
        let s = setup_with(Analytics::default(), "cookieless").await;
        s.admin
            .post(
                &s.path("goals"),
                &s.body(&json!({"name": "done", "kind": "page", "match": "/done"})),
            )
            .await
            .expect(StatusCode::CREATED);
        let file = project_pool(s.app.data_dir(), s.team.projects["site"].id).await.unwrap();
        s.send(json!({"n": "pageview", "u": "https://example.com/"}), "203.0.113.1").await;
        assert!(holds(&file, 1).await, "the first event lands");

        let busy = s.app.app().platform().db().write().await.unwrap();
        // A conversion owes the app database a hook event, which waits on `busy`.
        s.send(json!({"n": "pageview", "u": "https://example.com/done"}), "203.0.113.1").await;
        assert!(holds(&file, 2).await, "the converting event lands");
        s.send(json!({"n": "pageview", "u": "https://example.com/after"}), "203.0.113.1").await;
        assert!(holds(&file, 3).await, "later events land while the app database is busy");

        drop(busy);
        s.app.flush().await;
        s.app.drain_jobs().await;
        let points: Vec<_> = s.hooks.dispatches().iter().map(|dispatch| dispatch.point).collect();
        assert_eq!(points, [GOAL_CONVERTED.id], "the owed conversion lands once the app is free");
        file.close().await;
    }
}
