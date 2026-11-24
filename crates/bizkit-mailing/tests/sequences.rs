//! Sequences: enrolling, timed steps, step edits, leaving, and the
//! step-condition hook.

#[cfg(test)]
mod sequences {
    use std::time::Duration;

    use bizkit_core::hooks::{RunStatus, SyncOutcome};
    use bizkit_mailing::{MAX_DELAY_HOURS, Mailing};
    use bizkit_testkit::{FakeHooks, Manifest, Persona, SpecClient, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    struct Setup {
        app: TestApp,
        team: Manifest,
        member: SpecClient,
        hooks: FakeHooks,
    }

    async fn setup() -> Setup {
        let hooks = FakeHooks::default();
        let app = TestApp::builder().module(Mailing).hook_runner(hooks.clone()).build().await;
        let team = app.seed(Persona::Team, "seq").await;
        let member = app.with_session(&team.accounts["member"].session_token);
        Setup { app, team, member, hooks }
    }

    fn id(value: &Value) -> &str {
        value["id"].as_str().unwrap()
    }

    fn welcome_steps() -> Value {
        json!([
            {"delay_hours": 0, "subject": "Welcome", "body_md": "Hi {{ name }}"},
            {"delay_hours": 24, "subject": "Day one", "body_md": "Tips"},
            {"delay_hours": 48, "subject": "Day three", "body_md": "More tips"},
        ])
    }

    impl Setup {
        fn ws(&self) -> String {
            format!("/api/v1/workspaces/{}/mailing", self.team.workspaces["main"].id)
        }

        /// A list without double opt-in, so API adds subscribe at once.
        async fn list(&self) -> Value {
            let admin = self.app.with_session(&self.team.accounts["admin"].session_token);
            let body = json!({
                "name": "Newsletter",
                "project_id": self.team.projects["site"].id,
                "double_opt_in": false,
            });
            admin
                .post(&format!("{}/lists", self.ws()), &body)
                .await
                .expect(StatusCode::CREATED)
                .json()
        }

        async fn sequence(&self, list: &Value, trigger: &str, steps: Value) -> Value {
            let body =
                json!({"list_id": id(list), "name": "Welcome", "trigger": trigger, "steps": steps});
            self.member
                .post(&format!("{}/sequences", self.ws()), &body)
                .await
                .expect(StatusCode::CREATED)
                .json()
        }

        async fn get(&self, sequence: &Value) -> Value {
            self.member
                .get(&format!("{}/sequences/{}", self.ws(), id(sequence)))
                .await
                .expect(StatusCode::OK)
                .json()
        }

        async fn subscribe(&self, list: &Value, email: &str) -> Value {
            let path = format!("{}/lists/{}/contacts", self.ws(), id(list));
            self.member
                .post(&path, &json!({"email": email, "name": "Ada"}))
                .await
                .expect(StatusCode::CREATED)
                .json()
        }

        async fn enrollments(&self, sequence: &Value) -> Vec<Value> {
            let page: Value = self
                .member
                .get(&format!("{}/sequences/{}/enrollments", self.ws(), id(sequence)))
                .await
                .expect(StatusCode::OK)
                .json();
            page["items"].as_array().unwrap().clone()
        }

        async fn log(&self, sequence: &Value) -> Vec<Value> {
            let page: Value = self
                .member
                .get(&format!("{}/deliveries?sequence_id={}", self.ws(), id(sequence)))
                .await
                .expect(StatusCode::OK)
                .json();
            page["items"].as_array().unwrap().clone()
        }

        /// Subjects of the mail sent since the last call.
        fn subjects(&self) -> Vec<String> {
            self.app.mail.take().into_iter().map(|mail| mail.subject).collect()
        }

        async fn after_hours(&self, hours: u64) {
            self.app.clock.advance(Duration::from_hours(hours));
            self.app.drain_jobs().await;
        }
    }

    #[tokio::test]
    async fn on_join_sends_each_step_after_its_delay_then_completes() {
        let s = setup().await;
        let list = s.list().await;
        let sequence = s.sequence(&list, "on_join", welcome_steps()).await;
        s.subscribe(&list, "ada@example.com").await;
        s.app.drain_jobs().await;
        assert_eq!(s.subjects(), ["Welcome"], "first step at once");

        s.after_hours(23).await;
        assert!(s.subjects().is_empty(), "day one is not due yet");
        s.after_hours(1).await;
        assert_eq!(s.subjects(), ["Day one"]);
        s.after_hours(48).await;
        assert_eq!(s.subjects(), ["Day three"]);

        let enrollments = s.enrollments(&sequence).await;
        assert_eq!(
            (enrollments[0]["status"].clone(), enrollments[0]["steps_done"].clone()),
            (json!("completed"), json!(3))
        );
        let log = s.log(&sequence).await;
        assert_eq!(log.len(), 3, "one step delivery each");
        assert!(log.iter().all(|d| d["kind"] == "step" && d["status"] == "sent"), "{log:?}");
        let counts = &s.get(&sequence).await["enrollments"];
        assert_eq!(counts["completed"], 1, "{counts}");
    }

    #[tokio::test]
    async fn the_first_step_mail_has_merge_fields_and_an_unsubscribe_link() {
        let s = setup().await;
        let list = s.list().await;
        s.sequence(&list, "on_join", welcome_steps()).await;
        s.subscribe(&list, "ada@example.com").await;
        s.app.drain_jobs().await;
        let mail = s.app.mail.take().pop().unwrap();
        assert!(mail.text.contains("Hi Ada"), "{}", mail.text);
        assert!(mail.unsubscribe.is_some(), "List-Unsubscribe header");
    }

    #[tokio::test]
    async fn double_opt_in_enrolls_on_confirm_and_manual_sequences_wait() {
        let s = setup().await;
        let admin = s.app.with_session(&s.team.accounts["admin"].session_token);
        let body = json!({"name": "Opt in", "project_id": s.team.projects["site"].id});
        let list: Value = admin
            .post(&format!("{}/lists", s.ws()), &body)
            .await
            .expect(StatusCode::CREATED)
            .json();
        let on_join = s.sequence(&list, "on_join", welcome_steps()).await;
        let manual = s.sequence(&list, "manual", welcome_steps()).await;
        s.subscribe(&list, "ada@example.com").await;
        s.app.drain_jobs().await;
        assert!(s.enrollments(&on_join).await.is_empty(), "pending contacts don't enroll");

        let confirm = s.app.mail.take().pop().unwrap();
        let word = confirm.text.split_whitespace().find(|w| w.contains("/confirm?")).unwrap();
        let url = url::Url::parse(word).unwrap();
        let path = format!("{}?{}", url.path(), url.query().unwrap());
        let raw = bizkit_testkit::RawBody::new("application/x-www-form-urlencoded", "");
        let res = s.app.client().send_body(http::Method::POST, &path, raw).await;
        assert_eq!(res.status, StatusCode::OK);
        s.app.drain_jobs().await;

        assert_eq!(s.enrollments(&on_join).await.len(), 1, "enrolled on confirm");
        assert!(s.enrollments(&manual).await.is_empty(), "manual waits for the API");
        assert_eq!(s.subjects(), ["Welcome"]);
    }

    #[tokio::test]
    async fn disabled_sequences_and_earlier_subscribers_get_no_enrollments() {
        let s = setup().await;
        let list = s.list().await;
        s.subscribe(&list, "early@example.com").await;
        let sequence = s.sequence(&list, "on_join", welcome_steps()).await;
        let path = format!("{}/sequences/{}", s.ws(), id(&sequence));
        s.member.patch(&path, &json!({"enabled": false})).await.expect(StatusCode::OK);
        s.subscribe(&list, "late@example.com").await;
        s.app.drain_jobs().await;
        assert!(s.enrollments(&sequence).await.is_empty());
        assert!(s.subjects().is_empty());
    }

    #[tokio::test]
    async fn step_edits_keep_ids_and_never_resend_a_reached_step() {
        let s = setup().await;
        let list = s.list().await;
        let sequence = s.sequence(&list, "on_join", welcome_steps()).await;
        s.subscribe(&list, "ada@example.com").await;
        s.app.drain_jobs().await;
        assert_eq!(s.subjects(), ["Welcome"]);

        // Reword the sent step, drop "Day three", add a new last step.
        let steps = s.get(&sequence).await["steps"].as_array().unwrap().clone();
        let edit = json!({"steps": [
            {"id": steps[0]["id"], "delay_hours": 0, "subject": "Welcome again", "body_md": "x"},
            {"id": steps[1]["id"], "delay_hours": 24, "subject": "Day one", "body_md": "Tips"},
            {"delay_hours": 1, "subject": "Bonus", "body_md": "y"},
        ]});
        let path = format!("{}/sequences/{}", s.ws(), id(&sequence));
        let updated: Value = s.member.patch(&path, &edit).await.expect(StatusCode::OK).json();
        assert_eq!(updated["steps"][0]["id"], steps[0]["id"], "ids kept");
        assert_eq!(updated["steps"].as_array().unwrap().len(), 3);

        s.after_hours(24).await;
        assert_eq!(s.subjects(), ["Day one"], "the reworded first step is not sent again");
        s.after_hours(1).await;
        assert_eq!(s.subjects(), ["Bonus"]);
        assert_eq!(s.enrollments(&sequence).await[0]["status"], "completed");

        let unknown = json!({"steps": [
            {"id": "0190a5f0-0000-7000-8000-000000000009", "delay_hours": 0, "subject": "x"},
        ]});
        s.member.patch(&path, &unknown).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let too_late = json!({"steps": [{"delay_hours": MAX_DELAY_HOURS + 1, "subject": "x"}]});
        s.member.patch(&path, &too_late).await.expect(StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn unsubscribing_ends_the_run_and_skips_a_queued_step() {
        let s = setup().await;
        let list = s.list().await;
        let sequence = s.sequence(&list, "on_join", welcome_steps()).await;
        let contact = s.subscribe(&list, "ada@example.com").await;
        s.app.mail.fail_next(1);
        s.app.drain_jobs().await;
        assert_eq!(s.log(&sequence).await[0]["status"], "queued", "retrying");

        let path = format!("{}/lists/{}/contacts/{}/unsubscribe", s.ws(), id(&list), id(&contact));
        s.member.post(&path, &json!({})).await.expect(StatusCode::OK);
        s.after_hours(100).await;
        assert!(s.subjects().is_empty(), "nothing after unsubscribing");
        assert_eq!(s.enrollments(&sequence).await[0]["status"], "unsubscribed");
        assert_eq!(s.log(&sequence).await[0]["status"], "skipped");
    }

    #[tokio::test]
    async fn manual_enrollment_and_exit() {
        let s = setup().await;
        let list = s.list().await;
        let sequence = s.sequence(&list, "manual", welcome_steps()).await;
        let contact = s.subscribe(&list, "ada@example.com").await;
        let path = format!("{}/sequences/{}/enrollments", s.ws(), id(&sequence));
        let body = json!({"contact_id": id(&contact)});
        let enrollment: Value =
            s.member.post(&path, &body).await.expect(StatusCode::CREATED).json();
        assert_eq!(enrollment["status"], "active");
        s.member.post(&path, &body).await.expect(StatusCode::CONFLICT);
        s.app.drain_jobs().await;
        assert_eq!(s.subjects(), ["Welcome"]);

        let exit = format!("{path}/{}/exit", id(&enrollment));
        let exited: Value = s.member.post(&exit, &json!({})).await.expect(StatusCode::OK).json();
        assert_eq!(
            (exited["status"].clone(), exited["steps_done"].clone()),
            (json!("exited"), json!(1))
        );
        let again: Value = s.member.post(&exit, &json!({})).await.expect(StatusCode::OK).json();
        assert_eq!(again["status"], "exited", "exit is idempotent");
        s.after_hours(100).await;
        assert!(s.subjects().is_empty(), "the stale advance job does nothing");

        let other = s.list().await;
        let stranger = s.subscribe(&other, "bo@example.com").await;
        s.member
            .post(&path, &json!({"contact_id": id(&stranger)}))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn a_script_can_skip_a_step_and_the_run_goes_on() {
        let s = setup().await;
        s.hooks.answer(
            "mailing.step_condition",
            SyncOutcome::Returned(json!({"action": "skip", "reason": "not a trial"})),
        );
        let list = s.list().await;
        let sequence = s.sequence(&list, "on_join", welcome_steps()).await;
        s.subscribe(&list, "ada@example.com").await;
        s.app.drain_jobs().await;
        assert!(s.subjects().is_empty(), "skipped");
        let log = s.log(&sequence).await;
        assert_eq!(
            (log[0]["status"].clone(), log[0]["error"].clone()),
            (json!("skipped"), json!("not a trial"))
        );

        let call = s.hooks.calls().pop().unwrap();
        assert_eq!(call.target.entity.as_deref(), Some(id(&sequence)), "bound to the sequence");
        assert_eq!(call.input["step"]["subject"], "Welcome");
        assert_eq!(call.input["contact"]["email"], "ada@example.com");

        s.hooks.answer("mailing.step_condition", SyncOutcome::NoScript);
        s.after_hours(24).await;
        assert_eq!(s.subjects(), ["Day one"], "the next step still runs");
    }

    #[tokio::test]
    async fn failing_scripts_send_the_step() {
        for status in
            [RunStatus::Error, RunStatus::Timeout, RunStatus::Overloaded, RunStatus::InvalidOutput]
        {
            let s = setup().await;
            s.hooks.answer(
                "mailing.step_condition",
                SyncOutcome::Failed { status, message: "boom".to_owned() },
            );
            let list = s.list().await;
            s.sequence(&list, "on_join", welcome_steps()).await;
            s.subscribe(&list, "ada@example.com").await;
            s.app.drain_jobs().await;
            assert_eq!(s.subjects(), ["Welcome"], "{status:?} fails open");
        }
        let s = setup().await;
        s.hooks.answer("mailing.step_condition", SyncOutcome::Returned(json!({"action": "stop"})));
        let list = s.list().await;
        s.sequence(&list, "on_join", welcome_steps()).await;
        s.subscribe(&list, "ada@example.com").await;
        s.app.drain_jobs().await;
        assert_eq!(s.subjects(), ["Welcome"], "invalid output fails open");
    }

    #[tokio::test]
    async fn deleting_a_sequence_keeps_the_sent_log_and_stops_queued_steps() {
        let s = setup().await;
        let list = s.list().await;
        let sequence = s.sequence(&list, "on_join", welcome_steps()).await;
        let contact = s.subscribe(&list, "ada@example.com").await;
        s.app.mail.fail_next(1);
        s.app.drain_jobs().await;
        let path = format!("{}/sequences/{}", s.ws(), id(&sequence));
        s.member.delete(&path).await.expect(StatusCode::NO_CONTENT);
        s.after_hours(100).await;
        assert!(s.subjects().is_empty());
        let page: Value = s
            .member
            .get(&format!("{}/deliveries?contact_id={}", s.ws(), id(&contact)))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(page["items"][0]["status"], "skipped", "{page}");
        s.member.get(&path).await.expect(StatusCode::NOT_FOUND);
    }
}
