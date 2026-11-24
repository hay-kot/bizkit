//! The public ingest endpoint: encodings, answers, bot checks, mail, the
//! hook and the submission event.

#[cfg(test)]
mod ingest {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use base64::Engine as _;
    use bizkit_core::hooks::{RunStatus, SyncOutcome};
    use bizkit_core::jobs::{JobCtx, Subscriber};
    use bizkit_core::{Module, ModuleContext};
    use bizkit_forms::Forms;
    use bizkit_forms::hooks::{PRE_VALIDATE, SUBMISSION_CREATED, SubmissionCreated};
    use bizkit_testkit::{FakeHooks, Manifest, Persona, RawBody, SpecClient, TestApp};
    use http::{HeaderName, Method, StatusCode, header};
    use serde_json::{Value, json};
    use sha2::Digest as _;

    const FORM: &str = "application/x-www-form-urlencoded";

    /// Counts `forms.submission.created` deliveries.
    #[derive(Debug, Clone, Default)]
    struct Counter(Arc<AtomicU32>);

    impl Subscriber<SubmissionCreated> for Counter {
        const NAME: &'static str = "probe";
        fn handle(
            &self,
            _cx: JobCtx,
            _event: SubmissionCreated,
        ) -> impl Future<Output = anyhow::Result<()>> + Send {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::future::ready(Ok(()))
        }
    }

    #[derive(Debug)]
    struct Probe(Counter);

    impl Module for Probe {
        fn name(&self) -> &'static str {
            "probe"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.subscribe::<SubmissionCreated, _>(self.0.clone())
        }
    }

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        events: Counter,
        hooks: FakeHooks,
    }

    async fn setup() -> Setup {
        let events = Counter::default();
        let hooks = FakeHooks::default();
        let app = TestApp::builder()
            .module(Forms)
            .module(Probe(events.clone()))
            .hook_runner(hooks.clone())
            .build()
            .await;
        let team = app.seed(Persona::Team, "ingest").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        Setup { app, team, admin, events, hooks }
    }

    impl Setup {
        fn forms(&self) -> String {
            format!("/api/v1/workspaces/{}/forms", self.team.workspaces["main"].id)
        }

        async fn form(&self, body: Value) -> Value {
            let mut body = body;
            body["project_id"] = json!(self.team.projects["site"].id);
            self.admin.post(&self.forms(), &body).await.expect(StatusCode::CREATED).json()
        }

        async fn submissions(&self, form: &Value, status: &str) -> Vec<Value> {
            let path = format!("{}/{}/submissions?status={status}", self.forms(), id(form));
            let list: Value = self.admin.get(&path).await.expect(StatusCode::OK).json();
            list["items"].as_array().unwrap().clone()
        }

        async fn post(&self, form: &Value, raw: RawBody<'_>) -> bizkit_testkit::ApiResponse {
            let path = format!("/m/forms/public/{}", id(form));
            self.app.client().send_body(Method::POST, &path, raw).await
        }
    }

    fn id(form: &Value) -> &str {
        form["id"].as_str().unwrap()
    }

    #[tokio::test]
    async fn json_and_urlencoded_bodies() {
        let s = setup().await;
        let form = s.form(json!({"name": "Contact"})).await;
        let res = s
            .post(&form, RawBody::new("application/json", r#"{"name": "Ada", "tags": ["a", "b"]}"#))
            .await;
        assert_eq!(
            (res.status, res.body.clone()),
            (StatusCode::CREATED, json!({"ok": true})),
            "json"
        );

        let accept = [(header::ACCEPT, "application/json")];
        let raw = RawBody { headers: &accept, ..RawBody::new(FORM, "name=Bea") };
        assert_eq!(
            s.post(&form, raw).await.status,
            StatusCode::CREATED,
            "urlencoded asking for json"
        );

        let page = s.post(&form, RawBody::new(FORM, "name=Cy&_gotcha=")).await;
        assert_eq!(page.status, StatusCode::OK, "thank-you page");
        assert!(page.text.contains("Thanks!"), "{}", page.text);
        assert_eq!(
            page.headers[header::CONTENT_SECURITY_POLICY],
            "default-src 'none'; style-src 'unsafe-inline'",
            "csp"
        );

        let inbox = s.submissions(&form, "inbox").await;
        assert_eq!(inbox.len(), 3, "three stored");
        assert_eq!(inbox[2]["data"], json!({"name": "Ada", "tags": ["a", "b"]}), "json data");
        assert_eq!(s.events.0.load(Ordering::SeqCst), 0, "events wait for the worker");
        s.app.drain_jobs().await;
        assert_eq!(s.events.0.load(Ordering::SeqCst), 3, "one event per inbox submission");
    }

    #[tokio::test]
    async fn redirects_errors_and_unknown_forms() {
        let s = setup().await;
        let form = s
            .form(json!({
                "name": "Contact",
                "fields": [{"name": "email", "kind": "email", "required": true}],
                "redirect_url": "https://example.com/thanks",
            }))
            .await;
        let ok = s.post(&form, RawBody::new(FORM, "email=a%40b.io")).await;
        assert_eq!(ok.status, StatusCode::SEE_OTHER, "redirect");
        assert_eq!(ok.headers[header::LOCATION], "https://example.com/thanks", "location");

        let referer = [(header::REFERER, "https://site.example/contact?x=\"><script>")];
        let raw = RawBody { headers: &referer, ..RawBody::new(FORM, "email=%3Cscript%3E") };
        let bad = s.post(&form, raw).await;
        assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "html field errors");
        assert!(bad.text.contains("must be an email address"), "{}", bad.text);
        assert!(!bad.text.contains("<script>"), "escaped: {}", bad.text);
        assert_eq!(bad.headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff", "nosniff");

        let json_bad = s.post(&form, RawBody::new("application/json", "{}")).await;
        assert_eq!(json_bad.status, StatusCode::UNPROCESSABLE_ENTITY, "json field errors");
        assert_eq!(json_bad.body["error"]["fields"]["email"], json!(["is required"]), "field");

        let multipart =
            s.post(&form, RawBody::new("multipart/form-data; boundary=x", "--x--")).await;
        assert_eq!(multipart.status, StatusCode::BAD_REQUEST, "multipart");

        let client = s.app.client();
        let accept = [(header::ACCEPT, "application/json")];
        for path in
            ["/m/forms/public/not-a-uuid", &format!("/m/forms/public/{}", uuid::Uuid::now_v7())]
        {
            let probe = RawBody { headers: &accept, ..RawBody::new(FORM, "a=b") };
            let res = client.send_body(Method::POST, path, probe).await;
            assert_eq!(res.status, StatusCode::NOT_FOUND, "{path}");
        }

        s.admin
            .patch(&format!("{}/{}", s.forms(), id(&form)), &json!({"enabled": false}))
            .await
            .expect(StatusCode::OK);
        assert_eq!(
            s.post(&form, RawBody::new(FORM, "email=a%40b.io")).await.status,
            StatusCode::NOT_FOUND,
            "disabled"
        );
    }

    #[tokio::test]
    async fn signed_in_cross_site_posts_still_submit() {
        let s = setup().await;
        let form = s.form(json!({"name": "Contact"})).await;
        let cross = s.admin.with_origin("http://localhost:9999");
        let path = format!("/m/forms/public/{}", id(&form));
        let res = cross.send_body(Method::POST, &path, RawBody::new(FORM, "name=Ada")).await;
        assert_eq!(res.status, StatusCode::OK, "{}", res.text);
    }

    #[tokio::test]
    async fn honeypot_and_hook_verdicts() {
        let s = setup().await;
        let form = s.form(json!({"name": "Contact"})).await;
        assert_eq!(
            s.post(&form, RawBody::new(FORM, "name=Ada&_gotcha=buy")).await.status,
            StatusCode::OK,
            "spam looks accepted"
        );
        assert!(s.hooks.calls().is_empty(), "the hook skips submissions the bot checks flagged");

        let verdict = json!({"action": "spam", "reason": "x".repeat(300)});
        s.hooks.answer(PRE_VALIDATE.id, SyncOutcome::Returned(verdict));
        assert_eq!(
            s.post(&form, RawBody::new(FORM, "name=Bea&_gotcha=")).await.status,
            StatusCode::OK,
            "hook spam"
        );
        let call = &s.hooks.calls()[0];
        assert_eq!(call.input["fields"], json!({"name": ["Bea"]}), "control fields left out");
        assert_eq!(call.input["form"]["name"], "Contact", "form in the input");
        assert_eq!(call.target.project_id, s.team.projects["site"].id, "target");
        assert_eq!(call.target.entity.as_deref(), Some(id(&form)), "the form is the entity");

        let reject = json!({"action": "reject", "errors": {"name": ["is not allowed"]}});
        s.hooks.answer(PRE_VALIDATE.id, SyncOutcome::Returned(reject));
        let rejected = s.post(&form, RawBody::new("application/json", r#"{"name": "Cy"}"#)).await;
        assert_eq!(
            rejected.body["error"]["fields"]["name"],
            json!(["is not allowed"]),
            "hook reject"
        );

        let spam = s.submissions(&form, "spam").await;
        let reasons: Vec<usize> =
            spam.iter().map(|sub| sub["spam_reason"].as_str().unwrap().chars().count()).collect();
        assert_eq!(
            reasons,
            [200, "honeypot".len()],
            "reasons, newest first; the script's is capped"
        );
        assert!(s.submissions(&form, "inbox").await.is_empty(), "nothing in the inbox");
        s.app.drain_jobs().await;
        assert_eq!(s.events.0.load(Ordering::SeqCst), 0, "no events for spam");
    }

    #[tokio::test]
    async fn failing_hooks_store_in_the_inbox() {
        let s = setup().await;
        let form = s.form(json!({"name": "Contact"})).await;
        let statuses =
            [RunStatus::Error, RunStatus::Timeout, RunStatus::Overloaded, RunStatus::InvalidOutput];
        for status in statuses {
            let failed = SyncOutcome::Failed { status, message: "boom".to_owned() };
            s.hooks.answer(PRE_VALIDATE.id, failed);
            let res = s.post(&form, RawBody::new("application/json", r#"{"name": "Ada"}"#)).await;
            assert_eq!(res.status, StatusCode::CREATED, "{status} fails open");
        }
        let bad = SyncOutcome::Returned(json!({"action": "reject"}));
        s.hooks.answer(PRE_VALIDATE.id, bad);
        let res = s.post(&form, RawBody::new("application/json", r#"{"name": "Ada"}"#)).await;
        assert_eq!(res.status, StatusCode::CREATED, "a malformed verdict fails open");
        assert_eq!(s.submissions(&form, "inbox").await.len(), statuses.len() + 1, "all stored");
    }

    #[tokio::test]
    async fn inbox_submissions_reach_async_hooks() {
        let s = setup().await;
        let form = s.form(json!({"name": "Contact"})).await;
        s.post(&form, RawBody::new("application/json", r#"{"name": "Ada"}"#)).await;
        s.app.drain_jobs().await;

        let dispatches = s.hooks.dispatches();
        assert_eq!(dispatches.len(), 1, "one dispatch");
        let dispatch = &dispatches[0];
        assert_eq!(dispatch.point, SUBMISSION_CREATED.id, "point");
        assert_eq!(dispatch.target.project_id, s.team.projects["site"].id, "target");
        assert_eq!(dispatch.target.entity.as_deref(), Some(id(&form)), "the form is the entity");
        let keys: Vec<&String> = dispatch.event.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["form_id", "submission_id", "workspace_id"], "the event holds IDs only");

        let platform = s.app.app().platform();
        let cx = JobCtx { platform: platform.clone(), job_id: 0, attempt: 1, request_id: None };
        let input = platform.hook_input(&cx, SUBMISSION_CREATED.id, &dispatch.event).await.unwrap();
        let input = input.unwrap();
        assert_eq!(input["form"]["id"], form["id"], "form");
        assert_eq!(input["submission"]["data"], json!({"name": "Ada"}), "data loaded at run time");

        let submission = s.submissions(&form, "inbox").await[0]["id"].as_str().unwrap().to_owned();
        let path = format!("{}/{}/submissions/{submission}", s.forms(), id(&form));
        s.admin.delete(&path).await.expect(StatusCode::NO_CONTENT);
        let gone = platform.hook_input(&cx, SUBMISSION_CREATED.id, &dispatch.event).await.unwrap();
        assert_eq!(gone, None, "a deleted submission skips the run");
    }

    fn solve(challenge: &Value) -> String {
        let salt = challenge["salt"].as_str().unwrap();
        let target = challenge["challenge"].as_str().unwrap();
        let number = (0..=challenge["maxnumber"].as_u64().unwrap())
            .find(|n| hex::encode(sha2::Sha256::digest(format!("{salt}{n}"))) == target)
            .unwrap();
        let payload = json!({
            "algorithm": challenge["algorithm"],
            "challenge": target,
            "number": number,
            "salt": salt,
            "signature": challenge["signature"],
        });
        base64::engine::general_purpose::STANDARD.encode(payload.to_string())
    }

    #[tokio::test]
    async fn challenge_forms_check_proof_timing_and_replay() {
        let s = setup().await;
        let form = s.form(json!({"name": "Signup", "bot_check": "challenge"})).await;
        let client = s.app.client();
        let challenge: Value = client
            .get(&format!("/m/forms/public/{}/challenge", id(&form)))
            .await
            .expect(StatusCode::OK)
            .json();
        let altcha = solve(&challenge);
        let body = format!(
            "email=a%40b.io&altcha={}",
            url::form_urlencoded::byte_serialize(altcha.as_bytes()).collect::<String>()
        );

        s.post(&form, RawBody::new(FORM, "email=a%40b.io")).await;
        s.post(&form, RawBody::new(FORM, body.clone())).await;
        s.app.clock.advance(Duration::from_secs(3));
        s.post(&form, RawBody::new(FORM, body.clone())).await;
        s.post(&form, RawBody::new(FORM, body)).await;

        let spam = s.submissions(&form, "spam").await;
        let reasons: Vec<&str> =
            spam.iter().map(|sub| sub["spam_reason"].as_str().unwrap()).collect();
        assert_eq!(reasons, ["replayed", "too fast", "missing proof"], "spam reasons");
        let inbox = s.submissions(&form, "inbox").await;
        assert_eq!(inbox.len(), 1, "one good submission");
        assert_eq!(inbox[0]["data"], json!({"email": "a@b.io"}), "altcha is not stored");

        let honeypot_form = s.form(json!({"name": "Plain"})).await;
        client
            .get(&format!("/m/forms/public/{}/challenge", id(&honeypot_form)))
            .await
            .expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn notification_mail_and_its_cap() {
        let s = setup().await;
        let form = s
            .form(json!({"name": "Contact", "notify_emails": ["owner@example.com", "sales@example.com"]}))
            .await;
        s.post(&form, RawBody::new(FORM, "name=Ada&message=Hello")).await;
        s.post(&form, RawBody::new(FORM, "name=Bot&_gotcha=x")).await;
        s.app.drain_jobs().await;
        let sent = s.app.mail.take();
        let recipients: Vec<&str> = sent.iter().map(|m| m.to.as_str()).collect();
        assert_eq!(recipients.len(), 2, "one mail per recipient, none for spam: {recipients:?}");
        assert_eq!(sent[0].subject, "New submission: Contact", "subject");
        assert!(sent[0].text.contains("message: Hello"), "{}", sent[0].text);
        let workspace = Some(s.team.workspaces["main"].id);
        assert!(sent.iter().all(|m| m.workspace_id == workspace), "workspace mail");

        for n in 1..bizkit_forms::ingest::NOTIFY_PER_HOUR {
            s.post(&form, RawBody::new(FORM, format!("name=n{n}"))).await;
        }
        s.post(&form, RawBody::new(FORM, "name=over")).await;
        s.app.drain_jobs().await;
        let capped = s.app.mail.take();
        let expected = usize::try_from(bizkit_forms::ingest::NOTIFY_PER_HOUR - 1).unwrap() * 2;
        assert_eq!(capped.len(), expected, "mail stops at the cap");
        assert!(
            !capped.iter().any(|m| m.text.contains("name: over")),
            "the over-cap one has no mail"
        );
    }

    #[tokio::test]
    async fn preflight_allows_any_origin() {
        let s = setup().await;
        let form = s.form(json!({"name": "Contact"})).await;
        let path = format!("/m/forms/public/{}", id(&form));
        let headers = [
            (header::ORIGIN, "https://site.example"),
            (header::ACCESS_CONTROL_REQUEST_METHOD, "POST"),
            (HeaderName::from_static("access-control-request-headers"), "content-type"),
        ];
        let raw = RawBody { headers: &headers, ..RawBody::new("text/plain", "") };
        let res = s.app.client().send_body_unchecked(Method::OPTIONS, &path, raw).await;
        assert!(res.status.is_success(), "preflight status {}", res.status);
        assert_eq!(res.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*", "any origin");
    }

    #[tokio::test]
    async fn spam_past_the_cap_is_dropped() {
        let s = setup().await;
        let form = s.form(json!({"name": "Contact"})).await;
        for _ in 0..=bizkit_forms::ingest::SPAM_STORED_PER_HOUR {
            let res = s.post(&form, RawBody::new(FORM, "name=Bot&_gotcha=x")).await;
            assert_eq!(res.status, StatusCode::OK, "spam always looks accepted");
        }
        let path = format!("{}/{}/submissions?status=spam&limit=200", s.forms(), id(&form));
        let page: Value = s.admin.get(&path).await.json();
        assert_eq!(page["items"].as_array().unwrap().len(), 200, "a full page");
        assert_eq!(page["next_before"], Value::Null, "nothing past the cap");
    }
}
