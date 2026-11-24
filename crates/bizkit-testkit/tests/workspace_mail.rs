//! Per-workspace mail accounts: settings ops, sealed secrets and routing.

#[cfg(test)]
mod workspace_mail {
    use bizkit_core::deps::Mail;
    use bizkit_core::ids::WorkspaceId;
    use bizkit_core::jobs::{Enqueue, SendMail};
    use bizkit_testkit::{ApiResponse, Manifest, Persona, SpecClient, TestApp};
    use http::{Method, StatusCode};
    use serde_json::{Value, json};

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        member: SpecClient,
    }

    async fn setup() -> Setup {
        setup_with(TestApp::new().await).await
    }

    async fn setup_with(app: TestApp) -> Setup {
        let team = app.seed(Persona::Team, "mail").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let member = app.with_session(&team.accounts["member"].session_token);
        Setup { app, team, admin, member }
    }

    fn smtp() -> Value {
        json!({
            "provider": "smtp",
            "from_name": "Tallyho",
            "from_address": "news@tallyho.example",
            "reply_to": "help@tallyho.example",
            "smtp": {
                "host": "smtp.tallyho.example",
                "port": 587,
                "security": "starttls",
                "username": "ada",
                "password": "hunter2",
            },
        })
    }

    fn mailgun() -> Value {
        json!({
            "provider": "mailgun",
            "from_name": "Tallyho",
            "from_address": "news@mg.tallyho.example",
            "mailgun": {
                "domain": "mg.tallyho.example",
                "region": "eu",
                "api_key": "key-hunter2",
                "webhook_signing_key": "whk-hunter2",
            },
        })
    }

    impl Setup {
        fn ws(&self) -> WorkspaceId {
            self.team.workspaces["main"].id
        }

        fn path(&self) -> String {
            format!("/api/v1/workspaces/{}/mail/settings", self.ws())
        }

        async fn put(&self, body: &Value) -> ApiResponse {
            self.admin.send(Method::PUT, &self.path(), Some(body)).await
        }

        async fn queue(&self, workspace_id: Option<WorkspaceId>, to: &str) {
            let platform = self.app.app().platform();
            let mail = Mail { workspace_id, ..Mail::new(to, "Hello", "Hi") };
            let mut tx = platform.db().write().await.unwrap();
            platform.bus().enqueue(&mut tx, &SendMail(mail), Enqueue::default()).await.unwrap();
            tx.commit().await.unwrap();
            self.app.drain_jobs().await;
        }

        async fn stored_secrets(&self) -> Vec<u8> {
            sqlx::query_scalar("SELECT secrets FROM workspace_mail WHERE workspace_id = ?")
                .bind(self.ws())
                .fetch_one(self.app.app().platform().db().read())
                .await
                .unwrap()
        }
    }

    fn fields(response: &ApiResponse) -> Vec<String> {
        response.body["error"]["fields"].as_object().unwrap().keys().cloned().collect()
    }

    #[tokio::test]
    async fn secrets_are_write_only_and_sealed() {
        let s = setup().await;
        let empty = s.admin.get(&s.path()).await;
        empty.expect(StatusCode::OK);
        assert_eq!(empty["settings"], Value::Null, "no settings yet");
        assert_eq!(empty["can_save"], true, "the test app has a key");

        let saved = s.put(&smtp()).await;
        saved.expect(StatusCode::OK);
        assert_eq!(saved["smtp"]["has_password"], true, "password saved");
        assert_eq!(saved["smtp"]["username"], "ada", "username");
        let shown = s.admin.get(&s.path()).await;
        for body in [&saved.body, &shown.body] {
            assert!(!body.to_string().contains("hunter2"), "no secret in {body}");
        }
        let sealed = s.stored_secrets().await;
        assert!(!sealed.windows(7).any(|w| w == b"hunter2"), "sealed at rest");

        let switched = s.put(&mailgun()).await;
        switched.expect(StatusCode::OK);
        assert_eq!(switched["mailgun"]["has_api_key"], true, "api key");
        assert_eq!(switched["mailgun"]["has_webhook_signing_key"], true, "webhook key");
        assert_eq!(switched["smtp"], Value::Null, "smtp gone");
        assert!(!switched.body.to_string().contains("hunter2"), "{}", switched.body);
        assert_eq!(switched["mailgun"]["webhook_url"], Value::Null, "no mailing module, no URL");
    }

    #[tokio::test]
    async fn left_out_secrets_are_kept_until_the_provider_changes() {
        let s = setup().await;
        s.put(&smtp()).await.expect(StatusCode::OK);
        let mut again = smtp();
        again["smtp"].as_object_mut().unwrap().remove("password");
        again["from_name"] = json!("Tallyho News");
        let kept = s.put(&again).await;
        kept.expect(StatusCode::OK);
        assert_eq!(kept["smtp"]["has_password"], true, "kept");
        assert_eq!(kept["from_name"], "Tallyho News", "updated");

        again["smtp"]["password"] = json!("");
        let removed = s.put(&again).await;
        removed.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(fields(&removed), ["smtp.password"], "a username needs a password");

        let mut switched = mailgun();
        switched["mailgun"].as_object_mut().unwrap().remove("api_key");
        let refused = s.put(&switched).await;
        refused.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(fields(&refused), ["mailgun.api_key"], "a new provider needs its key");
    }

    #[tokio::test]
    async fn hosts_and_addresses_are_checked() {
        let s = setup().await;
        let mut private = smtp();
        private["smtp"]["host"] = json!("10.0.0.5");
        private["from_address"] = json!("not an address");
        let private_host = s.put(&private).await;
        private_host.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(fields(&private_host), ["from_address", "smtp.host"], "both reported");

        let mut plain = smtp();
        plain["smtp"]["security"] = json!("none");
        let plain_smtp = s.put(&plain).await;
        plain_smtp.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(fields(&plain_smtp), ["smtp.security"], "no plain SMTP off loopback");

        let mut mixed = smtp();
        mixed["mailgun"] = mailgun()["mailgun"].clone();
        let both = s.put(&mixed).await;
        both.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(fields(&both), ["mailgun"], "one provider at a time");
    }

    #[tokio::test]
    async fn only_admins_manage_mail_settings() {
        let s = setup().await;
        s.member.get(&s.path()).await.expect(StatusCode::FORBIDDEN);
        s.member.send(Method::PUT, &s.path(), Some(&smtp())).await.expect(StatusCode::FORBIDDEN);
        s.member.delete(&s.path()).await.expect(StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn saving_needs_the_instance_key() {
        let app = TestApp::builder().config(|c| c.secrets_key = None).build().await;
        let s = setup_with(app).await;
        assert_eq!(s.admin.get(&s.path()).await["can_save"], false, "can_save");
        s.put(&smtp()).await.expect(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn workspace_mail_goes_through_the_workspace_account() {
        let s = setup().await;
        s.put(&smtp()).await.expect(StatusCode::OK);
        s.queue(Some(s.ws()), "ada@example.com").await;
        s.queue(None, "sys@example.com").await;
        let other = s.app.seed(Persona::Team, "other").await;
        s.queue(Some(other.workspaces["main"].id), "bob@example.com").await;

        let sent = s.app.workspace_mail.sent();
        assert_eq!(sent.len(), 1, "one mail through the account: {sent:?}");
        assert_eq!(sent[0].mail.to, "ada@example.com", "to");
        assert_eq!(sent[0].from, "Tallyho <news@tallyho.example>", "from");
        assert_eq!(sent[0].reply_to.as_deref(), Some("help@tallyho.example"), "reply-to");
        let instance: Vec<String> = s.app.mail.sent().into_iter().map(|m| m.to).collect();
        assert_eq!(instance, ["sys@example.com", "bob@example.com"], "the rest");

        s.admin.delete(&s.path()).await.expect(StatusCode::NO_CONTENT);
        s.admin.delete(&s.path()).await.expect(StatusCode::NOT_FOUND);
        s.queue(Some(s.ws()), "ada@example.com").await;
        assert_eq!(
            s.app.mail.last_to("ada@example.com").map(|m| m.to).as_deref(),
            Some("ada@example.com"),
            "falls back"
        );
    }

    #[tokio::test]
    async fn a_save_rebuilds_the_mailer() {
        let s = setup().await;
        s.put(&smtp()).await.expect(StatusCode::OK);
        s.queue(Some(s.ws()), "ada@example.com").await;
        s.queue(Some(s.ws()), "ada@example.com").await;
        assert_eq!(s.app.workspace_mail.built().len(), 1, "cached between sends");
        let mut renamed = smtp();
        renamed["from_name"] = json!("Tallyho News");
        s.put(&renamed).await.expect(StatusCode::OK);
        s.queue(Some(s.ws()), "ada@example.com").await;
        let sent = s.app.workspace_mail.sent();
        assert_eq!(sent.last().unwrap().from, "Tallyho News <news@tallyho.example>", "new from");
        assert_eq!(s.app.workspace_mail.built().len(), 2, "rebuilt once");
    }

    #[tokio::test]
    async fn broken_settings_never_fall_back_to_the_instance() {
        let s = setup().await;
        s.put(&smtp()).await.expect(StatusCode::OK);
        s.app.workspace_mail.fail_builds(Some("no route to host"));
        s.queue(Some(s.ws()), "ada@example.com").await;
        assert!(s.app.mail.sent().is_empty(), "not sent as the instance");
        assert!(s.app.workspace_mail.sent().is_empty(), "not sent at all");
    }

    #[tokio::test]
    async fn a_test_mail_goes_to_the_caller_and_verifies() {
        let s = setup().await;
        let test = format!("{}/test", s.path());
        s.admin.post(&test, &json!({})).await.expect(StatusCode::NOT_FOUND);
        s.put(&smtp()).await.expect(StatusCode::OK);

        s.app.workspace_mail.fail_next(1);
        let failed = s.admin.post(&test, &json!({})).await;
        failed.expect(StatusCode::OK);
        assert_eq!(failed["sent"], false, "failed");
        assert!(failed["error"].as_str().unwrap().contains("421"), "{}", failed.body);

        let passed = s.admin.post(&test, &json!({})).await;
        passed.expect(StatusCode::OK);
        let admin_email = &s.team.accounts["admin"].email;
        assert_eq!(passed["sent"], true, "sent");
        assert_eq!(passed["to"], admin_email.as_str(), "to the caller");
        assert_eq!(s.app.workspace_mail.sent()[0].mail.to, *admin_email, "through the account");
        let verified = s.admin.get(&s.path()).await;
        assert_eq!(verified["settings"]["verified_at"], passed["verified_at"], "recorded");

        let saved = s.put(&smtp()).await;
        assert_eq!(saved["verified_at"], Value::Null, "a save clears it");

        let key = s.app.insert_api_key(s.ws(), &["workspace:admin"]).await;
        s.app.with_key(&key).post(&test, &json!({})).await.expect(StatusCode::BAD_REQUEST);
    }
}
