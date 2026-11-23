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
        let run: Value = s.admin.post(&test, &json!({})).await.expect(StatusCode::OK).json();
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
        let runs = s.runs(&script).await;
        assert_eq!(
            (runs[0]["cause"].clone(), runs[0]["status"].clone()),
            (json!("sync"), json!("timeout")),
            "{}",
            runs[0]
        );
    }
}
