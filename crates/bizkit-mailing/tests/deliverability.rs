//! Deliverability: error classes, provider IDs, the suppression list and
//! the Mailgun event webhook.

#[cfg(test)]
mod deliverability {
    use std::time::Duration;

    use bizkit_mailing::Mailing;
    use bizkit_mailing::send::BREAKER_FAILURES;
    use bizkit_testkit::{ApiResponse, Manifest, Persona, RawBody, SpecClient, TestApp};
    use hmac::{KeyInit as _, Mac as _};
    use http::{Method, StatusCode};
    use serde_json::{Value, json};

    const KEY: &str = "webhook-key";

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        member: SpecClient,
    }

    async fn setup_with(key: Option<&str>) -> Setup {
        let key = key.map(str::to_owned);
        let app = TestApp::builder()
            .config(|c| c.mail.webhook_signing_key = key)
            .module(Mailing)
            .build()
            .await;
        let team = app.seed(Persona::Team, "deliv").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let member = app.with_session(&team.accounts["member"].session_token);
        Setup { app, team, admin, member }
    }

    async fn setup() -> Setup {
        setup_with(Some(KEY)).await
    }

    fn id(value: &Value) -> &str {
        value["id"].as_str().unwrap()
    }

    fn sign(timestamp: &str, token: &str) -> String {
        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(KEY.as_bytes()).unwrap();
        mac.update(timestamp.as_bytes());
        mac.update(token.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    impl Setup {
        fn ws(&self) -> String {
            format!("/api/v1/workspaces/{}/mailing", self.team.workspaces["main"].id)
        }

        fn now_secs(&self) -> i64 {
            self.app.app().platform().now().0 / 1000
        }

        async fn list(&self, name: &str) -> Value {
            let body = json!({
                "name": name,
                "project_id": self.team.projects["site"].id.to_string(),
                "double_opt_in": false,
            });
            self.admin
                .post(&format!("{}/lists", self.ws()), &body)
                .await
                .expect(StatusCode::CREATED)
                .json()
        }

        fn contacts(&self, list: &Value) -> String {
            format!("{}/lists/{}/contacts", self.ws(), id(list))
        }

        async fn subscribed(&self, list: &Value, email: &str) -> Value {
            self.member
                .post(&self.contacts(list), &json!({"email": email}))
                .await
                .expect(StatusCode::CREATED)
                .json()
        }

        async fn contact(&self, list: &Value, contact: &Value) -> Value {
            self.admin
                .get(&format!("{}/{}", self.contacts(list), id(contact)))
                .await
                .expect(StatusCode::OK)
                .json()
        }

        /// Sends a broadcast to `list` and runs the jobs.
        async fn send(&self, list: &Value) -> Value {
            let input = json!({"list_id": id(list), "subject": "Hi", "body_md": "Hello"});
            let broadcast: Value = self
                .member
                .post(&format!("{}/broadcasts", self.ws()), &input)
                .await
                .expect(StatusCode::CREATED)
                .json();
            let path = format!("{}/broadcasts/{}", self.ws(), id(&broadcast));
            self.member.post(&format!("{path}/send"), &json!({})).await.expect(StatusCode::OK);
            self.app.drain_jobs().await;
            broadcast
        }

        async fn deliveries(&self, broadcast: &Value) -> Vec<Value> {
            let page: Value = self
                .admin
                .get(&format!("{}/deliveries?broadcast_id={}", self.ws(), id(broadcast)))
                .await
                .expect(StatusCode::OK)
                .json();
            page["items"].as_array().unwrap().clone()
        }

        async fn suppressions(&self, query: &str) -> Value {
            self.admin
                .get(&format!("{}/suppressions?{query}", self.ws()))
                .await
                .expect(StatusCode::OK)
                .json()
        }

        async fn webhook_raw(&self, body: String) -> ApiResponse {
            self.app
                .client()
                .send_body(
                    Method::POST,
                    "/m/mailing/public/events/mailgun",
                    RawBody::new("application/json", body),
                )
                .await
        }

        async fn webhook(&self, timestamp: i64, token: &str, event_data: Value) -> ApiResponse {
            let timestamp = timestamp.to_string();
            let body = json!({
                "signature": {
                    "timestamp": timestamp,
                    "token": token,
                    "signature": sign(&timestamp, token),
                },
                "event-data": event_data,
            });
            self.webhook_raw(body.to_string()).await
        }
    }

    fn failed(delivery: &Value) -> Value {
        json!({
            "event": "failed",
            "severity": "permanent",
            "recipient": delivery["to_email"],
            "user-variables": {"bizkit_id": delivery["id"]},
        })
    }

    #[tokio::test]
    async fn sends_record_the_provider_id_and_permanent_errors_fail_at_once() {
        let s = setup().await;
        let list = s.list("News").await;
        s.subscribed(&list, "ada@example.com").await;
        let broadcast = s.send(&list).await;
        let sent = s.deliveries(&broadcast).await.remove(0);
        let mail = s.app.mail.take().pop().unwrap();
        assert_eq!(mail.id.as_deref(), sent["id"].as_str(), "Message-ID is the delivery ID");
        assert_eq!(sent["provider_id"], format!("{}@fake", id(&sent)), "provider id stored");

        s.app.mail.fail_next_permanent(1);
        let second = s.send(&list).await;
        let failed = s.deliveries(&second).await.remove(0);
        assert_eq!(
            (failed["status"].clone(), failed["attempts"].clone()),
            (json!("failed"), json!(1)),
            "{failed}"
        );
        assert!(failed["error"].as_str().unwrap().contains("for good"), "{failed}");
        s.app.clock.advance(Duration::from_hours(2));
        assert_eq!(s.app.drain_jobs().await.retried, 0, "no retry");
    }

    #[tokio::test]
    async fn suppressed_addresses_get_no_list_mail() {
        let s = setup().await;
        let list = s.list("News").await;
        s.subscribed(&list, "ada@example.com").await;
        s.subscribed(&list, "bo@example.com").await;
        let created: Value = s
            .admin
            .post(
                &format!("{}/suppressions", s.ws()),
                &json!({"email": "Ada@Example.com", "note": "asked by phone"}),
            )
            .await
            .expect(StatusCode::CREATED)
            .json();
        assert_eq!(
            (created["email"].clone(), created["reason"].clone()),
            (json!("ada@example.com"), json!("manual")),
            "{created}"
        );

        let broadcast = s.send(&list).await;
        let mut log = s.deliveries(&broadcast).await;
        log.sort_by_key(|d| d["to_email"].as_str().unwrap().to_owned());
        assert_eq!(log[0]["status"], "skipped", "{log:?}");
        assert_eq!(log[0]["error"], "suppressed: manual", "{log:?}");
        assert_eq!(log[1]["status"], "sent", "{log:?}");
        let to: Vec<String> = s.app.mail.take().into_iter().map(|m| m.to).collect();
        assert_eq!(to, ["bo@example.com"], "only the other address");

        let opt_in = s
            .admin
            .post(
                &format!("{}/lists", s.ws()),
                &json!({"name": "Opt in", "project_id": s.team.projects["site"].id.to_string()}),
            )
            .await
            .expect(StatusCode::CREATED)
            .json::<Value>();
        s.member
            .post(&s.contacts(&opt_in), &json!({"email": "ada@example.com"}))
            .await
            .expect(StatusCode::CREATED);
        s.app.drain_jobs().await;
        assert!(s.app.mail.take().is_empty(), "no confirm mail either");
    }

    #[tokio::test]
    async fn suppression_ops_list_refuse_duplicates_and_keep_complaints() {
        let s = setup().await;
        let path = format!("{}/suppressions", s.ws());
        for email in ["cy@example.com", "ada@example.com", "bo@example.com"] {
            s.admin.post(&path, &json!({"email": email})).await.expect(StatusCode::CREATED);
        }
        s.admin.post(&path, &json!({"email": "bo@example.com"})).await.expect(StatusCode::CONFLICT);
        s.member
            .post(&path, &json!({"email": "x@example.com"}))
            .await
            .expect(StatusCode::FORBIDDEN);

        let first = s.suppressions("limit=2").await;
        let emails: Vec<&str> = first["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["email"].as_str().unwrap())
            .collect();
        assert_eq!(emails, ["ada@example.com", "bo@example.com"], "by address");
        let next =
            s.suppressions(&format!("after={}", first["next_after"].as_str().unwrap())).await;
        assert_eq!(next["items"][0]["email"], "cy@example.com", "next page");
        assert!(next["next_after"].is_null(), "last page");
        let found = s.suppressions("q=CY").await;
        assert_eq!(found["items"].as_array().unwrap().len(), 1, "search");

        s.admin.delete(&format!("{path}/bo@example.com")).await.expect(StatusCode::NO_CONTENT);
        s.admin.delete(&format!("{path}/bo@example.com")).await.expect(StatusCode::NOT_FOUND);

        let list = s.list("News").await;
        s.subscribed(&list, "dee@example.com").await;
        let broadcast = s.send(&list).await;
        let delivery = s.deliveries(&broadcast).await.remove(0);
        let now = s.now_secs();
        let complaint =
            json!({"event": "complained", "user-variables": {"bizkit_id": delivery["id"]}});
        s.webhook(now, "t1", complaint).await.expect(StatusCode::OK);
        let complaints = s.suppressions("reason=complaint").await;
        assert_eq!(complaints["items"][0]["email"], "dee@example.com", "{complaints}");
        s.admin.delete(&format!("{path}/dee@example.com")).await.expect(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn bounces_mark_the_delivery_and_suppress_the_address() {
        let s = setup().await;
        let list = s.list("News").await;
        let ada = s.subscribed(&list, "ada@example.com").await;
        let broadcast = s.send(&list).await;
        let delivery = s.deliveries(&broadcast).await.remove(0);

        s.webhook(s.now_secs(), "tok-1", failed(&delivery)).await.expect(StatusCode::OK);
        let marked = s.deliveries(&broadcast).await.remove(0);
        assert_eq!(
            (marked["status"].clone(), marked["event"].clone()),
            (json!("sent"), json!("bounced")),
            "{marked}"
        );
        assert!(marked["event_at"].is_string(), "{marked}");
        let suppressed = s.suppressions("").await;
        assert_eq!(
            (
                suppressed["items"][0]["reason"].clone(),
                suppressed["items"][0]["delivery_id"].clone()
            ),
            (json!("bounce"), delivery["id"].clone()),
            "{suppressed}"
        );
        assert_eq!(s.contact(&list, &ada).await["status"], "subscribed", "a bounce keeps consent");

        let got: Value = s
            .admin
            .get(&format!("{}/broadcasts/{}", s.ws(), id(&broadcast)))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(got["deliveries"]["bounced"], 1, "{got}");
    }

    #[tokio::test]
    async fn complaints_unsubscribe_the_address_on_every_list() {
        let s = setup().await;
        let news = s.list("News").await;
        let other = s.list("Other").await;
        let on_news = s.subscribed(&news, "ada@example.com").await;
        let on_other = s.subscribed(&other, "ada@example.com").await;
        let broadcast = s.send(&news).await;
        let delivery = s.deliveries(&broadcast).await.remove(0);
        // Found by the Message-ID when the provider drops the variables.
        let event = json!({
            "event": "complained",
            "message": {"headers": {"message-id": format!("<{}>", delivery["provider_id"].as_str().unwrap())}},
        });
        s.webhook(s.now_secs(), "tok-c", event).await.expect(StatusCode::OK);
        assert_eq!(s.contact(&news, &on_news).await["status"], "unsubscribed", "news");
        assert_eq!(s.contact(&other, &on_other).await["status"], "unsubscribed", "other list");
        assert_eq!(s.deliveries(&broadcast).await[0]["event"], "complained", "event");
    }

    #[tokio::test]
    async fn the_webhook_refuses_bad_signatures_replays_and_stale_events() {
        let s = setup().await;
        let now = s.now_secs();
        let ignored = json!({"event": "delivered"});

        s.webhook_raw("{}".to_owned()).await.expect(StatusCode::BAD_REQUEST);
        let forged = json!({
            "signature": {"timestamp": now.to_string(), "token": "x", "signature": "00ff"},
            "event-data": ignored,
        });
        s.webhook_raw(forged.to_string()).await.expect(StatusCode::UNAUTHORIZED);

        s.webhook(now, "once", ignored.clone()).await.expect(StatusCode::OK);
        s.webhook(now, "once", ignored.clone()).await.expect(StatusCode::NOT_ACCEPTABLE);
        s.webhook(now - 25 * 3600, "old", ignored.clone()).await.expect(StatusCode::NOT_ACCEPTABLE);

        let unknown = json!({
            "event": "failed",
            "severity": "permanent",
            "user-variables": {"bizkit_id": "0190a0a0-0000-7000-8000-000000000000"},
        });
        s.webhook(now, "unknown", unknown).await.expect(StatusCode::OK);
        assert!(s.suppressions("").await["items"].as_array().unwrap().is_empty(), "nothing");

        s.app.clock.advance(Duration::from_hours(24 * 8));
        let later = s.now_secs();
        s.webhook(later, "once", ignored).await.expect(StatusCode::OK);
    }

    #[tokio::test]
    async fn the_webhook_needs_a_signing_key() {
        let s = setup_with(None).await;
        s.webhook(s.now_secs(), "t", json!({"event": "delivered"}))
            .await
            .expect(StatusCode::UNAUTHORIZED);
    }

    fn at(s: &Setup, ahead: Duration) -> Value {
        serde_json::to_value(s.app.app().platform().now().plus(ahead)).unwrap()
    }

    #[tokio::test]
    async fn the_breaker_pauses_a_failing_broadcast_and_resume_sends_the_rest() {
        let s = setup().await;
        let list = s.list("News").await;
        let limit = usize::try_from(BREAKER_FAILURES).unwrap();
        for i in 0..=limit {
            s.subscribed(&list, &format!("c{i:02}@example.com")).await;
        }
        s.app.mail.fail_next(10_000);
        let broadcast = s.send(&list).await;
        let path = format!("{}/broadcasts/{}", s.ws(), id(&broadcast));
        let paused: Value = s.admin.get(&path).await.expect(StatusCode::OK).json();
        assert_eq!(paused["status"], "paused", "{paused}");
        assert!(
            paused["pause_reason"].as_str().unwrap().contains("20 deliveries failed in a row"),
            "{paused}"
        );
        s.member.post(&format!("{path}/resend"), &json!({})).await.expect(StatusCode::CONFLICT);

        // The failed rows wait to retry; the rest found the broadcast paused
        // and parked.
        let log = s.deliveries(&broadcast).await;
        let failed = log.iter().filter(|d| d["attempts"] == 1).count();
        assert_eq!(failed, limit, "{log:?}");
        assert!(log.iter().all(|d| d["status"] == "queued"), "{log:?}");

        s.app.mail.fail_next(0);
        let resumed: Value = s
            .member
            .post(&format!("{path}/resume"), &json!({}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(resumed["status"], "sent", "{resumed}");
        assert!(resumed["pause_reason"].is_null(), "{resumed}");
        s.member.post(&format!("{path}/resume"), &json!({})).await.expect(StatusCode::CONFLICT);
        s.app.drain_jobs().await;
        assert_eq!(s.app.mail.take().len(), 1, "the parked row went out");
        s.app.clock.advance(Duration::from_hours(2));
        s.app.drain_jobs().await;

        let mut to: Vec<String> = s.app.mail.take().into_iter().map(|m| m.to).collect();
        to.sort();
        to.dedup();
        assert_eq!(to.len(), limit, "the retries went out, once each");
        let final_log = s.deliveries(&broadcast).await;
        assert!(final_log.iter().all(|d| d["status"] == "sent"), "{final_log:?}");
    }

    #[tokio::test]
    async fn successes_reset_the_breaker() {
        let s = setup().await;
        let list = s.list("News").await;
        for i in 0..BREAKER_FAILURES + 5 {
            s.subscribed(&list, &format!("c{i:02}@example.com")).await;
        }
        s.app.mail.fail_next_permanent(u32::try_from(BREAKER_FAILURES - 1).unwrap());
        let broadcast = s.send(&list).await;
        let got: Value = s
            .admin
            .get(&format!("{}/broadcasts/{}", s.ws(), id(&broadcast)))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(got["status"], "sent", "{got}");
        assert_eq!(got["deliveries"]["failed"], BREAKER_FAILURES - 1, "{got}");
    }

    #[tokio::test]
    async fn scheduled_broadcasts_start_at_send_at_and_cancel_back_to_draft() {
        let s = setup().await;
        let list = s.list("News").await;
        s.subscribed(&list, "ada@example.com").await;
        let input = json!({"list_id": id(&list), "subject": "Hi", "body_md": "Hello"});
        let draft: Value = s
            .member
            .post(&format!("{}/broadcasts", s.ws()), &input)
            .await
            .expect(StatusCode::CREATED)
            .json();
        let path = format!("{}/broadcasts/{}", s.ws(), id(&draft));
        let send = format!("{path}/send");

        s.member
            .post(&send, &json!({"send_at": at(&s, Duration::ZERO)}))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
        s.member
            .post(&send, &json!({"send_at": at(&s, Duration::from_hours(24 * 400))}))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);

        let scheduled: Value = s
            .member
            .post(&send, &json!({"send_at": at(&s, Duration::from_hours(1))}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(scheduled["status"], "scheduled", "{scheduled}");
        s.member.patch(&path, &json!({"subject": "x"})).await.expect(StatusCode::CONFLICT);
        s.member.delete(&path).await.expect(StatusCode::CONFLICT);
        s.member.post(&send, &json!({})).await.expect(StatusCode::CONFLICT);
        s.app.drain_jobs().await;
        assert!(s.app.mail.take().is_empty(), "not before send_at");

        let cancelled: Value = s
            .member
            .post(&format!("{path}/cancel"), &json!({}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(cancelled["status"], "draft", "{cancelled}");
        s.member.post(&format!("{path}/cancel"), &json!({})).await.expect(StatusCode::CONFLICT);

        s.member
            .post(&send, &json!({"send_at": at(&s, Duration::from_hours(2))}))
            .await
            .expect(StatusCode::OK);
        s.app.clock.advance(Duration::from_hours(1) + Duration::from_mins(1));
        s.app.drain_jobs().await;
        assert!(s.app.mail.take().is_empty(), "the cancelled schedule's job does nothing");
        s.app.clock.advance(Duration::from_hours(1));
        s.app.drain_jobs().await;
        let started: Value = s.admin.get(&path).await.expect(StatusCode::OK).json();
        assert_eq!(started["status"], "sent", "{started}");
        assert_eq!(s.app.mail.take().len(), 1, "sent once");
    }
}
