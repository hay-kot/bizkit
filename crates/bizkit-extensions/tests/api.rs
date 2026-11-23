//! The extensions API end to end: scripts, secrets, test runs, and both
//! forms hook points running real sandboxed scripts.

#[cfg(test)]
mod api {
    use bizkit_extensions::Extensions;
    use bizkit_forms::Forms;
    use bizkit_testkit::{Manifest, Persona, RawBody, SpecClient, TestApp};
    use http::{Method, StatusCode};
    use serde_json::{Value, json};

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Forms).module(Extensions::default()).build().await;
        let team = app.seed(Persona::Team, "ext").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        Setup { app, team, admin }
    }

    impl Setup {
        fn ws(&self) -> String {
            format!("/api/v1/workspaces/{}", self.team.workspaces["main"].id)
        }

        fn project(&self) -> String {
            self.team.projects["site"].id.to_string()
        }

        async fn script(&self, hook: &str, source: &str) -> Value {
            let body = json!({"project_id": self.project(), "hook": hook, "name": "Test", "source": source});
            let res = self.admin.post(&format!("{}/extensions/scripts", self.ws()), &body).await;
            res.expect(StatusCode::CREATED).json()
        }

        async fn runs(&self, script: &Value) -> Vec<Value> {
            let path = format!(
                "{}/extensions/runs?script_id={}",
                self.ws(),
                script["id"].as_str().unwrap()
            );
            let list: Value = self.admin.get(&path).await.expect(StatusCode::OK).json();
            list["items"].as_array().unwrap().clone()
        }

        async fn form(&self) -> String {
            let body = json!({"project_id": self.project(), "name": "Contact"});
            let form: Value = self
                .admin
                .post(&format!("{}/forms", self.ws()), &body)
                .await
                .expect(StatusCode::CREATED)
                .json();
            form["id"].as_str().unwrap().to_owned()
        }

        async fn form_in(&self, project: &str, name: &str) -> String {
            let project_id = self.team.projects[project].id.to_string();
            let body = json!({"project_id": project_id, "name": name});
            let form: Value = self
                .admin
                .post(&format!("{}/forms", self.ws()), &body)
                .await
                .expect(StatusCode::CREATED)
                .json();
            form["id"].as_str().unwrap().to_owned()
        }

        async fn create(
            &self,
            hook: &str,
            name: &str,
            source: &str,
            targets: &[&str],
        ) -> bizkit_testkit::ApiResponse {
            let body = json!({
                "project_id": self.project(), "hook": hook, "name": name, "source": source,
                "target_ids": targets,
            });
            self.admin.post(&format!("{}/extensions/scripts", self.ws()), &body).await
        }

        async fn retarget(&self, script: &Value, targets: &[&str]) -> bizkit_testkit::ApiResponse {
            let path =
                format!("{}/extensions/scripts/{}", self.ws(), script["id"].as_str().unwrap());
            self.admin.patch(&path, &json!({"target_ids": targets})).await
        }

        async fn list(&self, query: &str) -> Vec<Value> {
            let path = format!("{}/extensions/scripts?{query}", self.ws());
            let list: Value = self.admin.get(&path).await.expect(StatusCode::OK).json();
            list["items"].as_array().unwrap().clone()
        }

        /// Sync runs are recorded off the request path, so wait for them.
        async fn runs_at_least(&self, script: &Value, n: usize) -> Vec<Value> {
            for _ in 0..100 {
                let runs = self.runs(script).await;
                if runs.len() >= n {
                    return runs;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            panic!("fewer than {n} runs of {script}");
        }

        async fn submit(&self, form: &str, body: &str) -> StatusCode {
            let path = format!("/m/forms/public/{form}");
            self.app
                .client()
                .send_body(Method::POST, &path, RawBody::new("application/json", body.to_owned()))
                .await
                .status
        }

        async fn submissions(&self, form: &str, status: &str) -> Vec<Value> {
            let path = format!("{}/forms/{form}/submissions?status={status}", self.ws());
            let list: Value = self.admin.get(&path).await.expect(StatusCode::OK).json();
            list["items"].as_array().unwrap().clone()
        }

        async fn put_secret(&self, name: &str, value: &str) {
            let path =
                format!("{}/extensions/projects/{}/secrets/{name}", self.ws(), self.project());
            self.admin
                .send(Method::PUT, &path, Some(&json!({"value": value})))
                .await
                .expect(StatusCode::OK);
        }
    }

    const PRE: &str = "forms.pre_validate";
    const POST: &str = "forms.submission_created";

    #[tokio::test]
    async fn hooks_list_shows_the_forms_points() {
        let s = setup().await;
        let list: Value = s
            .admin
            .get(&format!("{}/extensions/hooks", s.ws()))
            .await
            .expect(StatusCode::OK)
            .json();
        let ids: Vec<&str> = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hook| hook["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, [PRE, POST], "points");
        assert_eq!(list["items"][0]["capabilities"], json!(["log"]), "sync points only log");
        assert_eq!(list["items"][1]["capabilities"], json!(["log", "secrets", "fetch"]), "async");
    }

    #[tokio::test]
    async fn scripts_crud_and_checks() {
        let s = setup().await;
        let scripts = format!("{}/extensions/scripts", s.ws());
        let script = s.script(PRE, "export default () => ({ action: 'accept' })").await;
        assert_eq!(
            (script["sync"].clone(), script["available"].clone()),
            (json!(true), json!(true)),
            "{script}"
        );

        let second = json!({"project_id": s.project(), "hook": PRE, "name": "Two", "source": "export default () => {}"});
        s.admin.post(&scripts, &second).await.expect(StatusCode::CONFLICT);

        for (body, field) in [
            (
                json!({"project_id": s.project(), "hook": "nope.x", "name": "X", "source": "export default () => 1"}),
                "hook",
            ),
            (
                json!({"project_id": s.project(), "hook": POST, "name": "X", "source": "export default ("}),
                "source",
            ),
            (
                json!({"project_id": s.project(), "hook": POST, "name": "X", "source": "export const x = 1"}),
                "source",
            ),
        ] {
            let res = s.admin.post(&scripts, &body).await;
            res.expect(StatusCode::UNPROCESSABLE_ENTITY);
            assert!(res.body["error"]["fields"][field].is_array(), "{field}: {}", res.body);
        }

        let path = format!("{scripts}/{}", script["id"].as_str().unwrap());
        let updated: Value = s
            .admin
            .patch(&path, &json!({"name": "Renamed", "enabled": false}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(
            (updated["name"].clone(), updated["enabled"].clone()),
            (json!("Renamed"), json!(false)),
            "updated"
        );
        s.admin
            .patch(&path, &json!({"source": "nope("}))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
        let list: Value = s
            .admin
            .get(&format!("{scripts}?project_id={}", s.project()))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(list["items"].as_array().unwrap().len(), 1, "listed");
        assert!(list["items"][0].get("source").is_none(), "list leaves the source out");
        s.admin.delete(&path).await.expect(StatusCode::NO_CONTENT);
        s.admin.get(&path).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn members_and_other_workspaces_cant_manage_scripts() {
        let s = setup().await;
        let script = s.script(POST, "export default () => {}").await;
        let member = s.app.with_session(&s.team.accounts["member"].session_token);
        member.get(&format!("{}/extensions/hooks", s.ws())).await.expect(StatusCode::OK);
        let path = format!("{}/extensions/scripts/{}", s.ws(), script["id"].as_str().unwrap());
        member.get(&path).await.expect(StatusCode::FORBIDDEN);
        let secrets = format!("{}/extensions/projects/{}/secrets", s.ws(), s.project());
        member.get(&secrets).await.expect(StatusCode::FORBIDDEN);

        let other = s.app.seed(Persona::Team, "other").await;
        let outsider = s.app.with_session(&other.accounts["admin"].session_token);
        let theirs = format!(
            "/api/v1/workspaces/{}/extensions/scripts/{}",
            other.workspaces["main"].id,
            script["id"].as_str().unwrap()
        );
        outsider.get(&theirs).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_runs_redact_secrets_and_guard_fetch() {
        let s = setup().await;
        s.put_secret("TOKEN", "hunter2-very-secret").await;
        let script = s.script(POST, "export default (input, ctx) => { ctx.log('token', ctx.secrets.TOKEN); return { echo: ctx.secrets.TOKEN, form: input.form.name } }").await;
        let test = format!("{}/extensions/scripts/{}/test", s.ws(), script["id"].as_str().unwrap());
        let report: Value = s.admin.post(&test, &json!({})).await.expect(StatusCode::OK).json();
        assert_eq!(report["items"][0]["name"], "Sample input", "the default case");
        let run = &report["items"][0]["run"];
        assert_eq!(run["status"], "ok", "{run}");
        assert_eq!(run["logs"], json!(["token [secret:TOKEN]"]), "logs redacted");
        assert_eq!(
            run["output"],
            json!({"echo": "[secret:TOKEN]", "form": "Contact"}),
            "output redacted; sample input"
        );
        let runs = s.runs(&script).await;
        assert_eq!(runs.len(), 1, "recorded");
        assert_eq!(runs[0]["cause"], "test", "cause");
        assert!(!runs[0].to_string().contains("hunter2"), "stored redacted");

        let blocked = json!({"source": "export default async (input, ctx) => { await ctx.fetch('http://127.0.0.1:9/') }"});
        let refused: Value = s.admin.post(&test, &blocked).await.expect(StatusCode::OK).json();
        assert_eq!(refused["items"][0]["outcome"], "error", "{refused}");
        let refused = &refused["items"][0]["run"];
        assert_eq!(refused["status"], "error", "{refused}");
        assert!(refused["error"].as_str().unwrap().contains("not a public address"), "{refused}");
        assert!(s.app.http.requests().is_empty(), "never reached the client");

        let secrets: Value = s
            .admin
            .get(&format!("{}/extensions/projects/{}/secrets", s.ws(), s.project()))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(secrets["items"][0]["name"], "TOKEN", "names only");
        assert!(secrets["items"][0].get("value").is_none(), "no values");
    }

    #[tokio::test]
    async fn submissions_run_async_scripts_with_fetch() {
        let s = setup().await;
        s.put_secret("HOOK_URL", "https://hooks.example.com/abc123").await;
        s.app.http.respond(Method::POST, "https://hooks.example.com/", StatusCode::NO_CONTENT, "");
        let script = s.script(POST, "export default async (input, ctx) => { const res = await ctx.fetch(ctx.secrets.HOOK_URL, { method: 'POST', body: JSON.stringify(input.submission.data) }); ctx.log('sent', res.status) }").await;
        let form = s.form().await;
        assert_eq!(s.submit(&form, r#"{"name": "Ada"}"#).await, StatusCode::CREATED, "stored");

        s.app.drain_jobs().await;
        let payloads: Vec<(String,)> =
            sqlx::query_as("SELECT payload FROM jobs WHERE kind = 'extensions.run'")
                .fetch_all(s.app.app().platform().db().read())
                .await
                .unwrap();
        assert_eq!(payloads.len(), 1, "one run job");
        assert!(
            !payloads[0].0.contains("Ada"),
            "the job payload holds no submission data: {}",
            payloads[0].0
        );

        let requests = s.app.http.requests();
        assert_eq!(requests.len(), 1, "one fetch");
        assert_eq!(requests[0].2, r#"{"name":"Ada"}"#, "the submission data");
        let runs = s.runs(&script).await;
        assert_eq!(
            (runs[0]["cause"].clone(), runs[0]["status"].clone()),
            (json!("event"), json!("ok")),
            "{runs:?}"
        );
        assert_eq!(runs[0]["logs"], json!(["sent 204"]), "logs");
    }

    #[tokio::test]
    async fn failing_async_scripts_retry_then_go_dead() {
        let s = setup().await;
        let script = s.script(POST, "export default () => { throw new Error('broken') }").await;
        let form = s.form().await;
        s.submit(&form, r#"{"name": "Ada"}"#).await;
        for _ in 0..3 {
            s.app.drain_jobs().await;
            s.app.clock.advance(std::time::Duration::from_mins(5));
        }
        let (status, tries): (String, i64) =
            sqlx::query_as("SELECT status, attempts FROM jobs WHERE kind = 'extensions.run'")
                .fetch_one(s.app.app().platform().db().read())
                .await
                .unwrap();
        assert_eq!((status.as_str(), tries), ("dead", 3), "dead after three attempts");
        let runs = s.runs(&script).await;
        let attempts: Vec<&Value> = runs.iter().map(|run| &run["attempt"]).collect();
        assert_eq!(attempts, [&json!(3), &json!(2), &json!(1)], "a run per attempt");
        assert!(runs[0]["error"].as_str().unwrap().contains("broken"), "error kept");
    }

    #[tokio::test]
    async fn pre_validate_scripts_decide_and_fail_open() {
        let s = setup().await;
        let script = s.script(PRE, "export default (input) => { if (input.fields.message?.[0]?.includes('casino')) return { action: 'spam', reason: 'casino' } }").await;
        let form = s.form().await;
        assert_eq!(
            s.submit(&form, r#"{"message": "best casino"}"#).await,
            StatusCode::CREATED,
            "spam looks accepted"
        );
        assert_eq!(
            s.submit(&form, r#"{"message": "hello"}"#).await,
            StatusCode::CREATED,
            "accepted"
        );
        let spam = s.submissions(&form, "spam").await;
        assert_eq!(spam.len(), 1, "one spam");
        assert_eq!(spam[0]["spam_reason"], "casino", "the script's reason");

        let path = format!("{}/extensions/scripts/{}", s.ws(), script["id"].as_str().unwrap());
        s.admin
            .patch(&path, &json!({"source": "export default () => { for (;;) {} }"}))
            .await
            .expect(StatusCode::OK);
        assert_eq!(
            s.submit(&form, r#"{"message": "casino"}"#).await,
            StatusCode::CREATED,
            "fails open"
        );
        assert_eq!(s.submissions(&form, "inbox").await.len(), 2, "stored in the inbox");
        let runs = s.runs_at_least(&script, 3).await;
        assert_eq!(
            (runs[0]["cause"].clone(), runs[0]["status"].clone()),
            (json!("sync"), json!("timeout")),
            "{}",
            runs[0]
        );
    }

    const SPAM_IF_CASINO: &str = "export default (input) => input.fields.message?.[0]?.includes('casino') ? { action: 'spam', reason: 'bound' } : undefined";

    fn message(res: &bizkit_testkit::ApiResponse) -> String {
        assert_eq!(res.status, StatusCode::CONFLICT, "a conflict");
        res.json::<Value>()["error"]["message"].as_str().unwrap().to_owned()
    }

    fn names(items: &[Value]) -> Vec<&str> {
        items.iter().map(|item| item["name"].as_str().unwrap()).collect()
    }

    #[tokio::test]
    async fn targets_are_checked_and_canonical() {
        let s = setup().await;
        let form = s.form().await;
        let other_project = s.form_in("app", "Elsewhere").await;
        let source = "export default () => {}";

        let upper = s.create(PRE, "Upper", source, &[&form.to_uppercase(), &form]).await;
        let script: Value = upper.expect(StatusCode::CREATED).json();
        assert_eq!(
            script["targets"],
            json!([{"id": form, "label": "Contact"}]),
            "canonical, labelled and without repeats"
        );

        for (target, why) in [
            ("not-an-id", "malformed"),
            (other_project.as_str(), "another project's form"),
            ("01a20560-0a68-7703-8e0c-5c2bc679782b", "unknown"),
        ] {
            let res = s.create(PRE, "Bad", source, &[&form, target]).await;
            assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{why}");
            assert!(res.json::<Value>()["error"]["fields"]["target_ids"].is_array(), "{why}");
            let updated = s.retarget(&script, &[target]).await;
            assert_eq!(updated.status, StatusCode::UNPROCESSABLE_ENTITY, "{why} on update");
        }
    }

    #[tokio::test]
    async fn sync_slots_are_per_entity_plus_one_for_all() {
        let s = setup().await;
        let contact = s.form().await;
        let other = s.form_in("site", "Other").await;
        let third = s.form_in("site", "Third").await;
        let source = "export default () => {}";
        s.create(PRE, "Wide", source, &[]).await.expect(StatusCode::CREATED);
        s.create(PRE, "Bound", source, &[&contact]).await.expect(StatusCode::CREATED);

        let wide = message(&s.create(PRE, "Wide 2", source, &[]).await);
        assert!(wide.contains("Wide already runs on forms.pre_validate for all forms"), "{wide}");
        let shared = message(&s.create(PRE, "Bound 2", source, &[&other, &contact]).await);
        assert!(
            shared.contains("Bound already runs on forms.pre_validate for Contact"),
            "{shared}"
        );

        let elsewhere: Value = s
            .create(PRE, "Elsewhere", source, &[&other, &third])
            .await
            .expect(StatusCode::CREATED)
            .json();
        let taken = message(&s.retarget(&elsewhere, &[&third, &contact]).await);
        assert!(taken.contains("Bound already runs"), "updates are checked: {taken}");
        message(&s.retarget(&elsewhere, &[]).await);
        s.retarget(&elsewhere, &[&other]).await.expect(StatusCode::OK);

        s.create(POST, "Async 1", source, &[&contact]).await.expect(StatusCode::CREATED);
        s.create(POST, "Async 2", source, &[&contact]).await.expect(StatusCode::CREATED);
    }

    #[tokio::test]
    async fn scripts_run_for_the_forms_they_name() {
        let s = setup().await;
        let contact = s.form().await;
        let other = s.form_in("site", "Other").await;
        let third = s.form_in("site", "Third").await;
        let script: Value = s
            .create(PRE, "Shared", SPAM_IF_CASINO, &[&contact, &third])
            .await
            .expect(StatusCode::CREATED)
            .json();
        let wide = s.script(POST, "export default (input, ctx) => ctx.log(input.form.name)").await;
        let only_contact: Value = s
            .create(
                POST,
                "Contact only",
                "export default (input, ctx) => ctx.log('bound')",
                &[&contact],
            )
            .await
            .expect(StatusCode::CREATED)
            .json();

        for form in [&contact, &other, &third] {
            s.submit(form, r#"{"message": "casino"}"#).await;
        }
        assert_eq!(s.submissions(&contact, "spam").await.len(), 1, "ran for contact");
        assert_eq!(s.submissions(&third, "spam").await.len(), 1, "ran for the third form");
        assert_eq!(s.submissions(&other, "inbox").await.len(), 1, "not for the other form");
        assert_eq!(s.runs_at_least(&script, 2).await.len(), 2, "two sync runs");

        s.app.drain_jobs().await;
        assert_eq!(s.runs(&wide).await.len(), 1, "for-all ran for the other form's inbox post");
        assert_eq!(s.runs(&only_contact).await.len(), 0, "the contact post was spam, so no event");
        s.submit(&contact, r#"{"message": "hi"}"#).await;
        s.app.drain_jobs().await;
        assert_eq!(s.runs(&wide).await.len(), 2, "for-all runs for every form");
        assert_eq!(s.runs(&only_contact).await.len(), 1, "a named form runs its script");

        let filtered = s.list(&format!("target_id={contact}")).await;
        assert_eq!(names(&filtered), ["Contact only", "Shared", "Test"], "what runs for contact");
        let for_other = s.list(&format!("target_id={other}")).await;
        assert_eq!(names(&for_other), ["Test"], "what runs for the other form");
        let elsewhere = s.form_in("app", "Elsewhere").await;
        let app_script = json!({"project_id": s.team.projects["app"].id.to_string(), "hook": POST, "name": "App wide", "source": "export default () => {}"});
        s.admin
            .post(&format!("{}/extensions/scripts", s.ws()), &app_script)
            .await
            .expect(StatusCode::CREATED);
        let unscoped = s.list(&format!("target_id={}", contact.to_uppercase())).await;
        assert_eq!(
            names(&unscoped),
            ["Contact only", "Shared", "Test"],
            "canonical ID, own project only"
        );
        let for_elsewhere = s.list(&format!("target_id={elsewhere}")).await;
        assert_eq!(names(&for_elsewhere), ["App wide"], "the other project's for-all script");
    }

    #[tokio::test]
    async fn the_named_sync_script_wins_until_disabled() {
        let s = setup().await;
        let form = s.form().await;
        s.create(PRE, "Wide", "export default () => ({ action: 'spam', reason: 'wide' })", &[])
            .await
            .expect(StatusCode::CREATED);
        let bound: Value = s
            .create(
                PRE,
                "Bound",
                "export default () => ({ action: 'spam', reason: 'bound' })",
                &[&form],
            )
            .await
            .expect(StatusCode::CREATED)
            .json();
        s.submit(&form, r#"{"message": "a"}"#).await;
        let path = format!("{}/extensions/scripts/{}", s.ws(), bound["id"].as_str().unwrap());
        s.admin.patch(&path, &json!({"enabled": false})).await.expect(StatusCode::OK);
        s.submit(&form, r#"{"message": "b"}"#).await;
        let mut reasons: Vec<String> = s
            .submissions(&form, "spam")
            .await
            .iter()
            .map(|item| item["spam_reason"].as_str().unwrap().to_owned())
            .collect();
        reasons.sort();
        assert_eq!(reasons, ["bound", "wide"], "named first, then the for-all fallback");
    }

    #[tokio::test]
    async fn scripts_of_deleted_forms_lose_their_label() {
        let s = setup().await;
        let form = s.form().await;
        s.create(POST, "Orphan", "export default () => {}", &[&form])
            .await
            .expect(StatusCode::CREATED);
        s.admin.delete(&format!("{}/forms/{form}", s.ws())).await.expect(StatusCode::NO_CONTENT);
        let items = s.list("").await;
        assert_eq!(items[0]["targets"], json!([{"id": form, "label": null}]), "kept, unlabelled");
        let filtered = s.list(&format!("target_id={form}")).await;
        assert_eq!(names(&filtered), ["Orphan"], "the filter still finds it");
        let other = s.form_in("site", "Other").await;
        let orphan = &items[0];
        s.retarget(orphan, &[&form, &other]).await.expect(StatusCode::OK);
        let rename = format!("{}/extensions/scripts/{}", s.ws(), orphan["id"].as_str().unwrap());
        s.admin.patch(&rename, &json!({"name": "Still saves"})).await.expect(StatusCode::OK);
    }

    #[tokio::test]
    async fn targets_list_gives_a_projects_entities() {
        let s = setup().await;
        let contact = s.form().await;
        s.form_in("app", "Elsewhere").await;
        let path =
            format!("{}/extensions/targets?project_id={}&kind=forms.form", s.ws(), s.project());
        let list: Value = s.admin.get(&path).await.expect(StatusCode::OK).json();
        assert_eq!(
            list["items"],
            json!([{"id": contact, "label": "Contact"}]),
            "this project only"
        );
        let unknown = format!("{}/extensions/targets?project_id={}&kind=nope", s.ws(), s.project());
        let res = s.admin.get(&unknown).await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "unknown kind");
    }

    #[tokio::test]
    async fn hooks_name_their_target_kind() {
        let s = setup().await;
        let list: Value = s
            .admin
            .get(&format!("{}/extensions/hooks", s.ws()))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(
            list["items"][0]["target"],
            json!({"id": "forms.form", "noun": "form", "plural": "forms"}),
            "pre_validate targets forms"
        );
    }

    #[tokio::test]
    async fn saved_test_cases_pass_fail_and_error() {
        let s = setup().await;
        let script = s.script(PRE, SPAM_IF_CASINO).await;
        assert_eq!(script["tests"][0]["name"], "Sample input", "a sample case on create");
        let path = format!("{}/extensions/scripts/{}", s.ws(), script["id"].as_str().unwrap());
        let input =
            |message: &str| json!({"form": {"name": "Contact"}, "fields": {"message": [message]}});
        let cases = json!([
            {"name": " Casino ", "input": input("casino"), "expect": {"action": "spam", "reason": "bound"}},
            {"name": "Hello", "input": input("hello")},
            {"name": "Wrong", "input": input("hello"), "expect": {"action": "accept"}},
        ]);
        let saved: Value =
            s.admin.patch(&path, &json!({"tests": cases})).await.expect(StatusCode::OK).json();
        assert_eq!(saved["tests"][0]["name"], "Casino", "names trimmed");
        assert!(saved["tests"][1].get("expect").is_none(), "no expect stays absent");

        let report: Value =
            s.admin.post(&format!("{path}/test"), &json!({})).await.expect(StatusCode::OK).json();
        let outcomes: Vec<(&str, &str)> = report["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| (item["name"].as_str().unwrap(), item["outcome"].as_str().unwrap()))
            .collect();
        assert_eq!(
            outcomes,
            [("Casino", "pass"), ("Hello", "pass"), ("Wrong", "fail")],
            "outcomes"
        );
        assert_eq!(s.runs(&script).await.len(), 3, "a run per case");

        let one = json!({"tests": [{"name": "Only", "input": input("x")}], "source": "export default () => { throw new Error('no') }"});
        let single: Value =
            s.admin.post(&format!("{path}/test"), &one).await.expect(StatusCode::OK).json();
        assert_eq!(single["items"].as_array().unwrap().len(), 1, "a single given case");
        assert_eq!(single["items"][0]["outcome"], "error", "an unsaved source that throws");

        for (bad, why) in [
            (json!([{"name": "A", "input": {}}, {"name": "A", "input": {}}]), "a repeated name"),
            (json!([{"name": " ", "input": {}}]), "an empty name"),
            (json!([{"name": "A"}]), "no input"),
        ] {
            let res = s.admin.patch(&path, &json!({"tests": bad})).await;
            assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{why}");
        }
    }
}
