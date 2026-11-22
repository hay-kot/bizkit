//! The forms operations: CRUD, submissions, export and workspace isolation.

#[cfg(test)]
mod forms {
    use bizkit_forms::Forms;
    use bizkit_testkit::{Manifest, Persona, RawBody, SpecClient, TestApp};
    use http::{Method, StatusCode};
    use serde_json::{Value, json};

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        base: String,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Forms::default()).build().await;
        let team = app.seed(Persona::Team, "forms").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let base = format!("/api/v1/workspaces/{}/forms", team.workspaces["main"].id);
        Setup { app, team, admin, base }
    }

    async fn create(s: &Setup, body: Value) -> Value {
        let mut body = body;
        body["project_id"] = json!(s.team.projects["site"].id);
        s.admin.post(&s.base, &body).await.expect(StatusCode::CREATED).json()
    }

    async fn submit(s: &Setup, form: &Value, body: &str) {
        let path = format!("/m/forms/public/{}", form["id"].as_str().unwrap());
        let res = s
            .app
            .client()
            .send_body(
                Method::POST,
                &path,
                RawBody::new("application/x-www-form-urlencoded", body.to_owned()),
            )
            .await;
        assert_eq!(res.status, StatusCode::OK, "submit: {}", res.text);
    }

    #[tokio::test]
    async fn create_update_list_and_delete() {
        let s = setup().await;
        let form = create(
            &s,
            json!({
                "name": "Contact",
                "fields": [{"name": "email", "kind": "email", "required": true}],
                "notify_emails": [" Owner@Example.com ", "b@example.com", "owner@example.com"],
            }),
        )
        .await;
        assert_eq!(form["bot_check"], "honeypot", "default check");
        assert_eq!(
            form["notify_emails"],
            json!(["owner@example.com", "b@example.com"]),
            "normalized, deduped"
        );
        let id = form["id"].as_str().unwrap();
        assert_eq!(
            form["endpoint"].as_str().unwrap(),
            format!("{}/m/forms/public/{id}", bizkit_testkit::BASE_URL.trim_end_matches('/')),
            "endpoint"
        );

        let updated: Value = s
            .admin
            .patch(&format!("{}/{id}", s.base), &json!({"name": "Contact us", "bot_check": "challenge", "redirect_url": "https://example.com/thanks"}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(updated["name"], "Contact us", "renamed");
        assert_eq!(updated["redirect_url"], "https://example.com/thanks", "redirect");

        let cleared: Value = s
            .admin
            .patch(&format!("{}/{id}", s.base), &json!({"redirect_url": null}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(cleared["redirect_url"], Value::Null, "null clears the redirect");
        assert_eq!(cleared["bot_check"], "challenge", "absent fields stay");

        let other_project: Value = s
            .admin
            .get(&format!("{}?project_id={}", s.base, s.team.projects["app"].id))
            .await
            .json();
        assert_eq!(other_project["items"], json!([]), "other project");
        let all: Value = s.admin.get(&s.base).await.json();
        assert_eq!(all["items"].as_array().unwrap().len(), 1, "one form");

        s.admin.delete(&format!("{}/{id}", s.base)).await.expect(StatusCode::NO_CONTENT);
        s.admin.get(&format!("{}/{id}", s.base)).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_settings_are_field_errors() {
        let s = setup().await;
        let res = s
            .admin
            .post(
                &s.base,
                &json!({
                    "project_id": s.team.projects["site"].id,
                    "name": " ",
                    "fields": [{"name": "_x", "kind": "text"}],
                    "redirect_url": "javascript:alert(1)",
                    "notify_emails": ["nope"],
                }),
            )
            .await;
        res.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let fields = res.body["error"]["fields"].as_object().unwrap();
        let keys: Vec<&String> = fields.keys().collect();
        assert_eq!(
            keys,
            ["fields[0].name", "name", "notify_emails[0]", "redirect_url"],
            "{fields:?}"
        );

        let unknown =
            s.admin.post(&s.base, &json!({"project_id": uuid::Uuid::now_v7(), "name": "X"})).await;
        unknown.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert!(unknown.body["error"]["fields"]["project_id"].is_array(), "unknown project");
    }

    #[tokio::test]
    async fn submissions_list_page_mark_and_delete() {
        let s = setup().await;
        let form = create(&s, json!({"name": "Contact"})).await;
        let id = form["id"].as_str().unwrap();
        for n in 0..3 {
            submit(&s, &form, &format!("name=Ada+{n}")).await;
        }
        let subs = format!("{}/{id}/submissions", s.base);
        let page: Value =
            s.admin.get(&format!("{subs}?limit=2")).await.expect(StatusCode::OK).json();
        assert_eq!(page["items"].as_array().unwrap().len(), 2, "page size");
        assert_eq!(page["items"][0]["data"]["name"], "Ada 2", "newest first");
        let before = page["next_before"].as_str().unwrap();
        let rest: Value = s.admin.get(&format!("{subs}?limit=2&before={before}")).await.json();
        assert_eq!(rest["items"][0]["data"]["name"], "Ada 0", "second page");
        assert_eq!(rest["next_before"], Value::Null, "last page");

        let first = page["items"][0]["id"].as_str().unwrap();
        let marked: Value = s
            .admin
            .patch(&format!("{subs}/{first}"), &json!({"status": "spam"}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(marked["status"], "spam", "marked");
        let spam: Value = s.admin.get(&format!("{subs}?status=spam")).await.json();
        assert_eq!(spam["items"].as_array().unwrap().len(), 1, "spam filter");
        let got: Value = s.admin.get(&format!("{s_base}/{id}", s_base = s.base)).await.json();
        assert_eq!(got["inbox_count"], 2, "inbox count");

        s.admin.delete(&format!("{subs}/{first}")).await.expect(StatusCode::NO_CONTENT);
        s.admin.get(&format!("{subs}/{first}")).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn export_is_csv() {
        let s = setup().await;
        let form = create(&s, json!({"name": "Contact Us"})).await;
        submit(&s, &form, "name=Ada&note=%3D1%2B1").await;
        let id = form["id"].as_str().unwrap();
        let export: Value =
            s.admin.get(&format!("{}/{id}/export", s.base)).await.expect(StatusCode::OK).json();
        let csv = export["csv"].as_str().unwrap();
        assert!(csv.starts_with("id,created_at,status,name,note\r\n"), "{csv}");
        assert!(csv.contains(",inbox,Ada,'=1+1\r\n"), "defused formula: {csv}");
        assert!(export["filename"].as_str().unwrap().starts_with("contact-us-inbox-"), "filename");
        assert_eq!(export["truncated"], false, "complete");
    }

    #[tokio::test]
    async fn other_workspaces_get_404() {
        let s = setup().await;
        let form = create(&s, json!({"name": "Contact"})).await;
        submit(&s, &form, "name=Ada").await;
        let id = form["id"].as_str().unwrap();
        let sub: Value = s.admin.get(&format!("{}/{id}/submissions", s.base)).await.json();
        let sub_id = sub["items"][0]["id"].as_str().unwrap();

        let outsider = s.app.seed(Persona::Basic, "outsider").await;
        let client = s.app.with_session(&outsider.accounts["admin"].session_token);
        let their = format!("/api/v1/workspaces/{}/forms/{id}", outsider.workspaces["main"].id);
        client.get(&their).await.expect(StatusCode::NOT_FOUND);
        client.get(&format!("{their}/submissions")).await.expect(StatusCode::NOT_FOUND);
        client.get(&format!("{their}/export")).await.expect(StatusCode::NOT_FOUND);
        client.get(&format!("{their}/submissions/{sub_id}")).await.expect(StatusCode::NOT_FOUND);
        client
            .patch(&format!("{their}/submissions/{sub_id}"), &json!({"status": "spam"}))
            .await
            .expect(StatusCode::NOT_FOUND);
        client.delete(&format!("{their}/submissions/{sub_id}")).await.expect(StatusCode::NOT_FOUND);
        client.delete(&their).await.expect(StatusCode::NOT_FOUND);
        let list: Value = client
            .get(&format!("/api/v1/workspaces/{}/forms", outsider.workspaces["main"].id))
            .await
            .json();
        assert_eq!(list["items"], json!([]), "nothing leaks into the list");
    }

    #[tokio::test]
    async fn deleting_the_project_removes_its_forms() {
        let s = setup().await;
        let form = create(&s, json!({"name": "Contact"})).await;
        let project = format!(
            "/api/v1/workspaces/{}/projects/{}",
            s.team.workspaces["main"].id, s.team.projects["site"].id
        );
        s.admin.delete(&project).await.expect(StatusCode::NO_CONTENT);
        s.admin
            .get(&format!("{}/{}", s.base, form["id"].as_str().unwrap()))
            .await
            .expect(StatusCode::NOT_FOUND);
    }
}
