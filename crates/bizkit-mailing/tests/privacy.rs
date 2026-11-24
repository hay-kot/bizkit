//! Privacy: the signed manage-subscriptions page, and export and erase
//! per address.

#[cfg(test)]
mod privacy {
    use std::time::Duration;

    use bizkit_forms::Forms;
    use bizkit_mailing::{MANAGE_TTL, Mailing};
    use bizkit_testkit::{ApiResponse, Manifest, Persona, RawBody, SpecClient, TestApp};
    use http::{Method, StatusCode};
    use serde_json::{Value, json};

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        member: SpecClient,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Forms).module(Mailing).build().await;
        let team = app.seed(Persona::Team, "priv").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let member = app.with_session(&team.accounts["member"].session_token);
        Setup { app, team, admin, member }
    }

    fn id(value: &Value) -> &str {
        value["id"].as_str().unwrap()
    }

    /// The `t` query parameter of the first link to `page` in `text`.
    fn link_token(text: &str, page: &str) -> String {
        let marker = format!("/m/mailing/public/{page}?");
        let word = text.split_whitespace().find(|word| word.contains(&marker)).unwrap();
        let url = url::Url::parse(word).unwrap();
        url.query_pairs().find(|(k, _)| k == "t").map(|(_, v)| v.into_owned()).unwrap()
    }

    /// The `value`s of the inputs named `name` in a page.
    fn inputs(html: &str, name: &str) -> Vec<String> {
        let marker = format!("name=\"{name}\" value=\"");
        html.split(&marker)
            .skip(1)
            .filter_map(|rest| rest.split('"').next().map(str::to_owned))
            .collect()
    }

    impl Setup {
        fn ws_id(&self) -> String {
            self.team.workspaces["main"].id.to_string()
        }

        fn ws(&self) -> String {
            format!("/api/v1/workspaces/{}/mailing", self.ws_id())
        }

        fn site(&self) -> String {
            self.team.projects["site"].id.to_string()
        }

        async fn project(&self, name: &str, slug: &str) -> String {
            let path = format!("/api/v1/workspaces/{}/projects", self.ws_id());
            let body = json!({"name": name, "slug": slug});
            let project: Value =
                self.admin.post(&path, &body).await.expect(StatusCode::CREATED).json();
            id(&project).to_owned()
        }

        async fn list(&self, name: &str, project: &str) -> Value {
            let body = json!({"name": name, "project_id": project, "double_opt_in": false});
            self.admin
                .post(&format!("{}/lists", self.ws()), &body)
                .await
                .expect(StatusCode::CREATED)
                .json()
        }

        fn contacts(&self, list: &Value) -> String {
            format!("{}/lists/{}/contacts", self.ws(), id(list))
        }

        async fn add(&self, list: &Value, email: &str) -> ApiResponse {
            self.member.post(&self.contacts(list), &json!({"email": email})).await
        }

        async fn status(&self, list: &Value, email: &str) -> Value {
            let page: Value = self
                .admin
                .get(&format!("{}?q={email}", self.contacts(list)))
                .await
                .expect(StatusCode::OK)
                .json();
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["email"] == email)
                .map_or(Value::Null, |c| c["status"].clone())
        }

        /// Sends a broadcast to `list` and runs the jobs.
        async fn broadcast(&self, list: &Value) {
            let input = json!({"list_id": id(list), "subject": "News", "body_md": "Hello"});
            let broadcast: Value = self
                .member
                .post(&format!("{}/broadcasts", self.ws()), &input)
                .await
                .expect(StatusCode::CREATED)
                .json();
            let path = format!("{}/broadcasts/{}/send", self.ws(), id(&broadcast));
            self.member.post(&path, &json!({})).await.expect(StatusCode::OK);
            self.app.drain_jobs().await;
        }

        /// The manage token from the last mail to `email`.
        fn manage_token(&self, email: &str) -> String {
            link_token(&self.app.mail.last_to(email).unwrap().text, "manage")
        }

        async fn manage_get(&self, token: &str) -> ApiResponse {
            let path = format!("/m/mailing/public/manage?t={}", urlencode(token));
            self.app.client().send(Method::GET, &path, None).await
        }

        async fn manage_post(&self, token: &str, form: &str) -> ApiResponse {
            let path = format!("/m/mailing/public/manage?t={}", urlencode(token));
            let raw = RawBody::new("application/x-www-form-urlencoded", form.to_owned());
            self.app.client().send_body(Method::POST, &path, raw).await
        }

        async fn export(&self, email: &str) -> Value {
            self.admin
                .post(&format!("{}/addresses/export", self.ws()), &json!({"email": email}))
                .await
                .expect(StatusCode::OK)
                .json()
        }

        async fn erase(&self, email: &str) -> Value {
            self.admin
                .post(&format!("{}/addresses/erase", self.ws()), &json!({"email": email}))
                .await
                .expect(StatusCode::OK)
                .json()
        }

        /// Rows in every mailing table that hold `email` as text.
        async fn plaintext_rows(&self, email: &str) -> i64 {
            let db = self.app.app().platform().db().read();
            let mut total = 0;
            for sql in [
                "SELECT COUNT(*) FROM mailing_contacts WHERE lower(email) = ?",
                "SELECT COUNT(*) FROM mailing_deliveries WHERE lower(to_email) = ?",
                "SELECT COUNT(*) FROM mailing_suppressions WHERE lower(email) = ?",
            ] {
                let n: i64 = sqlx::query_scalar(sql).bind(email).fetch_one(db).await.unwrap();
                total += n;
            }
            total
        }
    }

    fn urlencode(text: &str) -> String {
        url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
    }

    #[tokio::test]
    async fn the_manage_page_shows_the_project_lists_and_changes_nothing() {
        let s = setup().await;
        let news = s.list("News", &s.site()).await;
        let offers = s.list("Offers", &s.site()).await;
        let docs = s.project("Docs", "docs").await;
        let elsewhere = s.list("Elsewhere", &docs).await;
        for list in [&news, &offers, &elsewhere] {
            s.add(list, "ada@example.com").await.expect(StatusCode::CREATED);
        }
        s.broadcast(&news).await;

        let mail = s.app.mail.last_to("ada@example.com").unwrap();
        assert!(mail.text.contains("Manage subscriptions: "), "text footer: {}", mail.text);
        assert!(mail.html.unwrap().contains("Manage subscriptions"), "html footer");

        let res = s.manage_get(&s.manage_token("ada@example.com")).await;
        assert_eq!(res.status, StatusCode::OK, "page");
        let html = res.text.clone();
        assert!(html.contains("News") && html.contains("Offers"), "the project's lists: {html}");
        assert!(!html.contains("Elsewhere"), "another project's list: {html}");
        let mut shown = inputs(&html, "shown");
        shown.sort();
        let mut expected = vec![id(&news).to_owned(), id(&offers).to_owned()];
        expected.sort();
        assert_eq!(shown, expected, "one box per list");
        assert_eq!(s.status(&news, "ada@example.com").await, "subscribed", "GET changes nothing");
    }

    #[tokio::test]
    async fn saving_leaves_only_the_unticked_lists_that_were_shown() {
        let s = setup().await;
        let news = s.list("News", &s.site()).await;
        let offers = s.list("Offers", &s.site()).await;
        for list in [&news, &offers] {
            s.add(list, "bo@example.com").await.expect(StatusCode::CREATED);
        }
        s.broadcast(&news).await;
        let token = s.manage_token("bo@example.com");
        let page = s.manage_get(&token).await.text;

        // A list joined after the GET is not on the form, so Save keeps it.
        let later = s.list("Later", &s.site()).await;
        s.add(&later, "bo@example.com").await.expect(StatusCode::CREATED);

        let empty = s.manage_post(&token, "").await;
        assert_eq!(empty.status, StatusCode::OK, "empty post");
        assert_eq!(s.status(&news, "bo@example.com").await, "subscribed", "empty post");
        assert_eq!(s.status(&offers, "bo@example.com").await, "subscribed", "empty post");

        let form: Vec<String> = inputs(&page, "shown")
            .into_iter()
            .map(|list| format!("shown={list}"))
            .chain([format!("keep={}", id(&news))])
            .collect();
        let saved = s.manage_post(&token, &form.join("&")).await;
        assert_eq!(saved.status, StatusCode::OK, "save");
        assert!(saved.text.contains("Saved."), "notice");
        assert_eq!(s.status(&news, "bo@example.com").await, "subscribed", "kept");
        assert_eq!(s.status(&offers, "bo@example.com").await, "unsubscribed", "unticked");
        assert_eq!(s.status(&later, "bo@example.com").await, "subscribed", "not shown");

        s.manage_post(&token, "all=1").await.expect(StatusCode::OK);
        for list in [&news, &offers, &later] {
            assert_eq!(s.status(list, "bo@example.com").await, "unsubscribed", "all");
        }
    }

    #[tokio::test]
    async fn manage_links_expire_and_cannot_be_forged() {
        let s = setup().await;
        let news = s.list("News", &s.site()).await;
        s.add(&news, "cy@example.com").await.expect(StatusCode::CREATED);
        s.add(&news, "di@example.com").await.expect(StatusCode::CREATED);
        s.broadcast(&news).await;
        let token = s.manage_token("cy@example.com");
        let other = s.manage_token("di@example.com");
        let (cy_id, rest) = token.split_once('.').unwrap();
        let (expires, sig) = rest.split_once('.').unwrap();
        let later: i64 = expires.parse::<i64>().unwrap() + 86_400 * 365;
        let (di_id, _) = other.split_once('.').unwrap();

        for (what, forged) in [
            ("a later expiry", format!("{cy_id}.{later}.{sig}")),
            ("another contact's ID", format!("{di_id}.{expires}.{sig}")),
            ("garbage", "nope".to_owned()),
        ] {
            let res = s.manage_get(&forged).await;
            assert_eq!(res.status, StatusCode::NOT_FOUND, "{what}");
            assert!(res.text.contains("not valid"), "{what} is invalid, not expired");
            s.manage_post(&forged, "all=1").await.expect(StatusCode::NOT_FOUND);
        }
        assert_eq!(
            s.status(&news, "cy@example.com").await,
            "subscribed",
            "forgeries changed nothing"
        );

        s.app.clock.advance(MANAGE_TTL + Duration::from_secs(1));
        let res = s.manage_get(&token).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "expired");
        assert!(res.text.contains("expired"), "expired page");
        s.manage_post(&token, "all=1").await.expect(StatusCode::NOT_FOUND);
        // Sessions expire across the jump too, so read the row directly.
        let db = s.app.app().platform().db().read();
        let status: String = sqlx::query_scalar(
            "SELECT status FROM mailing_contacts WHERE email = 'cy@example.com'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(status, "subscribed", "expired post");
    }

    #[tokio::test]
    async fn export_returns_what_mailing_holds_on_an_address() {
        let s = setup().await;
        let news = s.list("News", &s.site()).await;
        let body = json!({"list_id": id(&news), "name": "Welcome", "trigger": "on_join",
            "steps": [{"delay_hours": 0, "subject": "Welcome", "body_md": "Hi"}]});
        s.admin.post(&format!("{}/sequences", s.ws()), &body).await.expect(StatusCode::CREATED);
        s.add(&news, "Ed@Example.com").await.expect(StatusCode::CREATED);
        s.app.drain_jobs().await;
        s.broadcast(&news).await;
        s.admin
            .post(&format!("{}/suppressions", s.ws()), &json!({"email": "ed@example.com"}))
            .await
            .expect(StatusCode::CREATED);

        let export = s.export("ED@example.com").await;
        assert_eq!(export["email"], "ed@example.com", "normalized");
        assert_eq!(export["erased"], false, "not erased");
        assert_eq!(export["suppression"]["reason"], "manual", "suppression");
        assert_eq!(export["lists"][0]["name"], "News", "lists");
        assert_eq!(export["contacts"].as_array().unwrap().len(), 1, "contacts");
        assert_eq!(export["enrollments"].as_array().unwrap().len(), 1, "enrollments");
        assert_eq!(export["deliveries"].as_array().unwrap().len(), 2, "step and broadcast");

        let nobody = s.export("nobody@example.com").await;
        assert_eq!(nobody["contacts"], json!([]), "an unknown address exports empty");

        s.member
            .post(&format!("{}/addresses/export", s.ws()), &json!({"email": "ed@example.com"}))
            .await
            .expect(StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn erase_removes_the_address_and_keeps_it_from_mail_and_lists() {
        let s = setup().await;
        let news = s.list("News", &s.site()).await;
        let offers = s.list("Offers", &s.site()).await;
        let body = json!({"list_id": id(&news), "name": "Welcome", "trigger": "on_join",
        "steps": [
            {"delay_hours": 0, "subject": "Welcome", "body_md": "Hi"},
            {"delay_hours": 24, "subject": "Day one", "body_md": "Tips"},
        ]});
        s.admin.post(&format!("{}/sequences", s.ws()), &body).await.expect(StatusCode::CREATED);
        for list in [&news, &offers] {
            s.add(list, "fay@example.com").await.expect(StatusCode::CREATED);
            s.add(list, "gil@example.com").await.expect(StatusCode::CREATED);
        }
        s.app.drain_jobs().await;
        s.broadcast(&offers).await;
        s.admin
            .post(&format!("{}/suppressions", s.ws()), &json!({"email": "fay@example.com"}))
            .await
            .expect(StatusCode::CREATED);
        let token = s.manage_token("fay@example.com");
        // The breaker copies provider text, which can quote the recipient.
        let mut write = s.app.app().platform().db().write().await.unwrap();
        sqlx::query(
            "UPDATE mailing_broadcasts SET pause_reason = '550 <Fay@example.com>: unknown'",
        )
        .execute(&mut *write)
        .await
        .unwrap();
        write.commit().await.unwrap();
        let db = s.app.app().platform().db().read();

        let erased = s.erase("Fay@example.com").await;
        assert_eq!(erased["contacts"], 2, "contacts");
        assert!(erased["deliveries"].as_u64().unwrap() >= 2, "deliveries: {erased}");
        assert_eq!(erased["suppression"], true, "suppression");
        assert_eq!(s.plaintext_rows("fay@example.com").await, 0, "no plaintext left");
        let quoted: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM mailing_broadcasts WHERE lower(pause_reason) LIKE '%fay@example.com%'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        assert_eq!(quoted, 0, "pause reasons redacted");
        assert_eq!(s.export("fay@example.com").await["erased"], true, "export says erased");
        s.manage_get(&token).await.expect(StatusCode::NOT_FOUND);

        // The queued day-one step and later broadcasts never reach it.
        let before = s.app.mail.sent().len();
        s.app.clock.advance(Duration::from_hours(25));
        s.app.drain_jobs().await;
        s.broadcast(&news).await;
        assert!(s.app.mail.last_to("gil@example.com").is_some(), "others still get mail");
        assert!(
            s.app.mail.sent()[before..].iter().all(|m| m.to != "fay@example.com"),
            "no mail to the erased address"
        );

        // Adds refuse it and store nothing.
        s.add(&news, "fay@example.com").await.expect(StatusCode::CONFLICT);
        s.admin
            .post(&format!("{}/suppressions", s.ws()), &json!({"email": "fay@example.com"}))
            .await
            .expect(StatusCode::CONFLICT);
        assert_eq!(s.plaintext_rows("fay@example.com").await, 0, "still no plaintext");

        // A member's address can't get a test send after an erase either.
        let member_email = s.team.accounts["member"].email.clone();
        let draft = json!({"list_id": id(&news), "subject": "Draft", "body_md": "Hi"});
        let broadcast: Value = s
            .member
            .post(&format!("{}/broadcasts", s.ws()), &draft)
            .await
            .expect(StatusCode::CREATED)
            .json();
        s.erase(&member_email).await;
        let test_path = format!("{}/broadcasts/{}/test", s.ws(), id(&broadcast));
        s.member.post(&test_path, &json!({"to": member_email})).await.expect(StatusCode::CONFLICT);

        let again = s.erase("fay@example.com").await;
        assert_eq!(
            again,
            json!({"contacts": 0, "deliveries": 0, "suppression": false}),
            "idempotent"
        );
        assert_eq!(s.status(&news, "gil@example.com").await, "subscribed", "others untouched");
    }

    #[tokio::test]
    async fn erase_stays_in_its_workspace() {
        let s = setup().await;
        let other = s.app.seed(Persona::Team, "priv2").await;
        let other_admin = s.app.with_session(&other.accounts["admin"].session_token);
        let other_ws = format!("/api/v1/workspaces/{}/mailing", other.workspaces["main"].id);
        let body = json!({"name": "Theirs", "project_id": other.projects["site"].id.to_string(),
            "double_opt_in": false});
        let theirs: Value = other_admin
            .post(&format!("{other_ws}/lists"), &body)
            .await
            .expect(StatusCode::CREATED)
            .json();
        let theirs_contacts = format!("{other_ws}/lists/{}/contacts", id(&theirs));
        other_admin
            .post(&theirs_contacts, &json!({"email": "hu@example.com"}))
            .await
            .expect(StatusCode::CREATED);
        let ours = s.list("Ours", &s.site()).await;
        s.add(&ours, "hu@example.com").await.expect(StatusCode::CREATED);

        s.erase("hu@example.com").await;
        let page: Value = other_admin.get(&theirs_contacts).await.expect(StatusCode::OK).json();
        assert_eq!(page["items"][0]["status"], "subscribed", "the other workspace keeps it");
        other_admin
            .post(&theirs_contacts, &json!({"email": "hu2@example.com"}))
            .await
            .expect(StatusCode::CREATED);
    }
}
