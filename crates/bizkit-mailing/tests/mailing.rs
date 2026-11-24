//! Lists, contacts and consent, the public pages, form capture, broadcasts
//! and the send log.

#[cfg(test)]
mod mailing {
    use std::time::Duration;

    use bizkit_forms::Forms;
    use bizkit_mailing::contacts::CONFIRMS_PER_HOUR;
    use bizkit_mailing::{Mailing, TESTS_PER_HOUR};
    use bizkit_testkit::{Manifest, Persona, RawBody, SpecClient, TestApp};
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
        let team = app.seed(Persona::Team, "mail").await;
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

    impl Setup {
        fn ws(&self) -> String {
            format!("/api/v1/workspaces/{}/mailing", self.team.workspaces["main"].id)
        }

        fn project(&self) -> String {
            self.team.projects["site"].id.to_string()
        }

        async fn list(&self, extra: Value) -> Value {
            let mut body = json!({"name": "Newsletter", "project_id": self.project()});
            if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
                body.extend(extra.clone());
            }
            self.admin
                .post(&format!("{}/lists", self.ws()), &body)
                .await
                .expect(StatusCode::CREATED)
                .json()
        }

        async fn get_list(&self, list: &Value) -> Value {
            self.admin
                .get(&format!("{}/lists/{}", self.ws(), id(list)))
                .await
                .expect(StatusCode::OK)
                .json()
        }

        fn contacts(&self, list: &Value) -> String {
            format!("{}/lists/{}/contacts", self.ws(), id(list))
        }

        async fn add(&self, list: &Value, email: &str) -> bizkit_testkit::ApiResponse {
            self.member.post(&self.contacts(list), &json!({"email": email})).await
        }

        async fn contact(&self, list: &Value, email: &str) -> Value {
            let page: Value = self
                .admin
                .get(&format!("{}?q={email}", self.contacts(list)))
                .await
                .expect(StatusCode::OK)
                .json();
            page["items"].as_array().unwrap().iter().find(|c| c["email"] == email).cloned().unwrap()
        }

        async fn form(&self, fields: Value) -> String {
            let body = json!({"project_id": self.project(), "name": "Signup", "fields": fields});
            let path = format!("/api/v1/workspaces/{}/forms", self.team.workspaces["main"].id);
            let form: Value =
                self.admin.post(&path, &body).await.expect(StatusCode::CREATED).json();
            form["id"].as_str().unwrap().to_owned()
        }

        async fn submit(&self, form: &str, body: Value) {
            let path = format!("/m/forms/public/{form}");
            let raw = RawBody::new("application/json", body.to_string());
            let res = self.app.client().send_body(Method::POST, &path, raw).await;
            assert!(res.status.is_success(), "submit: {}", res.status);
        }

        async fn public(
            &self,
            method: Method,
            page: &str,
            token: &str,
        ) -> bizkit_testkit::ApiResponse {
            let path = format!("/m/mailing/public/{page}?t={token}");
            match method {
                Method::POST => {
                    self.app
                        .client()
                        .send_body(
                            method,
                            &path,
                            RawBody::new("application/x-www-form-urlencoded", ""),
                        )
                        .await
                }
                _ => self.app.client().send(method, &path, None).await,
            }
        }

        async fn deliveries(&self, query: &str) -> Vec<Value> {
            let page: Value = self
                .admin
                .get(&format!("{}/deliveries?{query}", self.ws()))
                .await
                .expect(StatusCode::OK)
                .json();
            page["items"].as_array().unwrap().clone()
        }

        async fn broadcast(&self, list: &Value, body: &str) -> Value {
            let input =
                json!({"list_id": id(list), "subject": "Hello {{ name }}", "body_md": body});
            self.member
                .post(&format!("{}/broadcasts", self.ws()), &input)
                .await
                .expect(StatusCode::CREATED)
                .json()
        }

        /// Adds a subscribed contact through a list without double opt-in.
        async fn audience(&self, list: &Value, segment: Value) -> u64 {
            let path = format!("{}/lists/{}/audience", self.ws(), id(list));
            let res: Value = self
                .member
                .post(&path, &json!({"segment": segment}))
                .await
                .expect(StatusCode::OK)
                .json();
            res["subscribers"].as_u64().unwrap()
        }

        async fn subscribed(&self, list: &Value, email: &str, extra: Value) -> Value {
            let mut body = json!({"email": email});
            if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
                body.extend(extra.clone());
            }
            let contact: Value = self
                .member
                .post(&self.contacts(list), &body)
                .await
                .expect(StatusCode::CREATED)
                .json();
            assert_eq!(contact["status"], "subscribed", "{contact}");
            contact
        }
    }

    #[tokio::test]
    async fn lists_link_forms_of_their_project() {
        let s = setup().await;
        let form = s.form(json!([])).await;
        let list = s.list(json!({"form_ids": [form], "footer": "1 Main St"})).await;
        assert_eq!(list["form_ids"], json!([form]), "linked");
        assert_eq!(list["double_opt_in"], true, "default");
        assert_eq!(list["counts"]["subscribed"], 0, "counts");

        let path = format!("{}/lists/{}", s.ws(), id(&list));
        let bad = s.admin.patch(&path, &json!({"form_ids": ["nope"]})).await;
        bad.expect(StatusCode::UNPROCESSABLE_ENTITY);

        let other = s.team.projects["app"].id.to_string();
        let body = json!({"project_id": other, "name": "Docs form"});
        let path_forms = format!("/api/v1/workspaces/{}/forms", s.team.workspaces["main"].id);
        let foreign: Value =
            s.admin.post(&path_forms, &body).await.expect(StatusCode::CREATED).json();
        let foreign_res = s.admin.patch(&path, &json!({"form_ids": [id(&foreign)]})).await;
        foreign_res.expect(StatusCode::UNPROCESSABLE_ENTITY);

        let cleared: Value = s
            .admin
            .patch(&path, &json!({"form_ids": [], "footer": null}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(cleared["form_ids"], json!([]), "cleared");
        assert!(cleared["footer"].is_null(), "footer removed");

        let member_res = s
            .member
            .post(&format!("{}/lists", s.ws()), &json!({"name": "X", "project_id": s.project()}))
            .await;
        member_res.expect(StatusCode::FORBIDDEN);

        let all: Value = s
            .admin
            .get(&format!("{}/lists?project_id={}", s.ws(), s.project()))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(all["items"].as_array().unwrap().len(), 1, "listed");
        s.admin.delete(&path).await.expect(StatusCode::NO_CONTENT);
        s.admin.get(&path).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn double_opt_in_confirms_through_the_page() {
        let s = setup().await;
        let list = s.list(json!({})).await;
        let contact: Value =
            s.add(&list, "Ada@Example.com").await.expect(StatusCode::CREATED).json();
        assert_eq!(
            (contact["status"].clone(), contact["email"].clone()),
            (json!("pending"), json!("ada@example.com"))
        );
        assert_eq!(contact["source"], "admin", "source");
        s.add(&list, "ada@example.com").await.expect(StatusCode::CONFLICT);

        s.app.drain_jobs().await;
        let mail = s.app.mail.last_to("ada@example.com").unwrap();
        assert_eq!(mail.subject, "Confirm your subscription to Newsletter", "subject");
        assert!(mail.unsubscribe.is_none(), "no list header on the confirm mail");
        let token = link_token(&mail.text, "confirm");

        let page = s.public(Method::GET, "confirm", &token).await;
        assert_eq!(page.status, StatusCode::OK, "page");
        assert_eq!(
            s.contact(&list, "ada@example.com").await["status"],
            "pending",
            "GET changes nothing"
        );

        let done = s.public(Method::POST, "confirm", &token).await;
        assert_eq!(done.status, StatusCode::OK, "confirmed");
        let confirmed = s.contact(&list, "ada@example.com").await;
        assert_eq!(confirmed["status"], "subscribed", "subscribed");
        assert!(confirmed["subscribed_at"].is_string(), "subscribed_at");
        assert_eq!(s.public(Method::POST, "confirm", &token).await.status, StatusCode::OK, "again");

        let log = s.deliveries(&format!("contact_id={}", id(&confirmed))).await;
        assert_eq!(
            (log.len(), log[0]["kind"].clone(), log[0]["status"].clone()),
            (1, json!("confirm"), json!("sent"))
        );

        assert_eq!(
            s.public(Method::GET, "confirm", "nope").await.status,
            StatusCode::NOT_FOUND,
            "unknown"
        );
        assert_eq!(
            s.public(Method::POST, "confirm", "nope").await.status,
            StatusCode::NOT_FOUND,
            "unknown post"
        );
    }

    #[tokio::test]
    async fn confirm_links_expire_and_pending_contacts_get_one_resend_an_hour() {
        let s = setup().await;
        let form = s.form(json!([])).await;
        let list = s.list(json!({"form_ids": [form]})).await;
        s.submit(&form, json!({"email": "bo@example.com"})).await;
        s.app.drain_jobs().await;
        s.submit(&form, json!({"email": "bo@example.com"})).await;
        s.app.drain_jobs().await;
        assert_eq!(s.app.mail.take().len(), 1, "no resend within the hour");

        s.app.clock.advance(Duration::from_hours(24 * 8));
        let old = s.contact(&list, "bo@example.com").await;
        let token_mail = s.deliveries(&format!("contact_id={}", id(&old))).await;
        assert_eq!(token_mail.len(), 1, "one confirm so far");
        s.submit(&form, json!({"email": "bo@example.com"})).await;
        s.app.drain_jobs().await;
        let mail = s.app.mail.take().pop().unwrap();
        let token = link_token(&mail.text, "confirm");

        s.app.clock.advance(Duration::from_hours(24 * 8));
        assert_eq!(
            s.public(Method::GET, "confirm", &token).await.status,
            StatusCode::NOT_FOUND,
            "expired page"
        );
        assert_eq!(
            s.public(Method::POST, "confirm", &token).await.status,
            StatusCode::NOT_FOUND,
            "expired"
        );
        assert_eq!(s.contact(&list, "bo@example.com").await["status"], "pending", "still pending");
    }

    #[tokio::test]
    async fn lists_without_double_opt_in_subscribe_api_adds_at_once() {
        let s = setup().await;
        let list = s.list(json!({"double_opt_in": false})).await;
        let contact = s
            .subscribed(
                &list,
                "cy@example.com",
                json!({"tags": ["vip", " vip ", "beta"], "name": "Cy"}),
            )
            .await;
        assert_eq!(contact["tags"], json!(["vip", "beta"]), "tags cleaned");
        s.app.drain_jobs().await;
        assert!(s.app.mail.sent().is_empty(), "no mail");

        let path = format!("{}/{}", s.contacts(&list), id(&contact));
        let updated: Value = s
            .member
            .patch(&path, &json!({"name": null, "tags": ["x"]}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert!(updated["name"].is_null() && updated["tags"] == json!(["x"]), "{updated}");

        let filtered: Value = s
            .admin
            .get(&format!("{}?tag=x&status=subscribed", s.contacts(&list)))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(filtered["items"].as_array().unwrap().len(), 1, "tag filter");

        let gone: Value = s
            .member
            .post(&format!("{path}/unsubscribe"), &json!({}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(gone["status"], "unsubscribed", "unsubscribed");
        s.member.delete(&path).await.expect(StatusCode::CONFLICT);
        s.add(&list, "cy@example.com").await.expect(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn form_captures_always_confirm_and_never_reach_unsubscribed_addresses() {
        let s = setup().await;
        let fields = json!([{"name": "your_email", "label": "Email", "kind": "email", "required": true}, {"name": "name", "label": "Name", "kind": "text"}]);
        let typed = s.form(fields).await;
        let raw = s.form(json!([])).await;
        let list = s.list(json!({"double_opt_in": false, "form_ids": [typed, raw]})).await;

        s.submit(&typed, json!({"your_email": "Dee@Example.com", "name": "Dee"})).await;
        s.submit(&raw, json!({"email": "eve@example.com"})).await;
        s.app.drain_jobs().await;
        let dee = s.contact(&list, "dee@example.com").await;
        assert_eq!(
            (dee["status"].clone(), dee["name"].clone()),
            (json!("pending"), json!("Dee")),
            "{dee}"
        );
        assert_eq!(dee["source"], format!("form:{typed}"), "source");
        assert_eq!(
            s.contact(&list, "eve@example.com").await["status"],
            "pending",
            "raw email field"
        );
        assert_eq!(s.app.mail.take().len(), 2, "two confirm mails");

        let gone = s.subscribed(&list, "fay@example.com", json!({})).await;
        let path = format!("{}/{}/unsubscribe", s.contacts(&list), id(&gone));
        s.member.post(&path, &json!({})).await.expect(StatusCode::OK);
        s.submit(&raw, json!({"email": "fay@example.com"})).await;
        s.app.drain_jobs().await;
        assert!(s.app.mail.take().is_empty(), "nothing to an unsubscribed address");
        assert_eq!(s.contact(&list, "fay@example.com").await["status"], "unsubscribed", "stays");
    }

    #[tokio::test]
    async fn confirm_mails_are_capped_per_list_and_hour() {
        let s = setup().await;
        let list = s.list(json!({})).await;
        for i in 0..=CONFIRMS_PER_HOUR {
            s.add(&list, &format!("p{i}@example.com")).await.expect(StatusCode::CREATED);
        }
        s.app.drain_jobs().await;
        let sent = s.app.mail.take().len();
        assert_eq!(i64::try_from(sent).unwrap(), CONFIRMS_PER_HOUR, "capped");
        assert_eq!(
            s.get_list(&list).await["counts"]["pending"],
            CONFIRMS_PER_HOUR + 1,
            "all pending"
        );
    }

    #[tokio::test]
    async fn unsubscribe_pages_and_one_click() {
        let s = setup().await;
        let list = s.list(json!({"double_opt_in": false})).await;
        s.subscribed(&list, "gus@example.com", json!({})).await;
        s.subscribed(&list, "hal@example.com", json!({})).await;
        s.subscribed(&list, "ivy@example.com", json!({})).await;
        let broadcast = s.broadcast(&list, "Hi").await;
        s.member
            .post(&format!("{}/broadcasts/{}/send", s.ws(), id(&broadcast)), &json!({}))
            .await
            .expect(StatusCode::OK);
        s.app.drain_jobs().await;

        let gus = s.app.mail.last_to("gus@example.com").unwrap();
        let header = gus.unsubscribe.clone().unwrap();
        assert!(gus.text.contains(header.as_str()), "the text links the same URL");
        let token = link_token(&gus.text, "unsubscribe");
        assert_eq!(
            s.public(Method::GET, "unsubscribe", &token).await.status,
            StatusCode::OK,
            "page"
        );
        assert_eq!(
            s.contact(&list, "gus@example.com").await["status"],
            "subscribed",
            "GET changes nothing"
        );
        assert_eq!(
            s.public(Method::POST, "unsubscribe", &token).await.status,
            StatusCode::OK,
            "button"
        );
        assert_eq!(
            s.contact(&list, "gus@example.com").await["status"],
            "unsubscribed",
            "unsubscribed"
        );

        let hal = s.app.mail.last_to("hal@example.com").unwrap().unsubscribe.unwrap();
        let path = format!("{}?{}", hal.path(), hal.query().unwrap());
        let res = s
            .app
            .client()
            .send_body(
                Method::POST,
                &path,
                RawBody::new("application/x-www-form-urlencoded", "List-Unsubscribe=One-Click"),
            )
            .await;
        assert_eq!(res.status, StatusCode::OK, "one click");
        assert_eq!(
            s.contact(&list, "hal@example.com").await["status"],
            "unsubscribed",
            "one click"
        );

        // RFC 8058's own example posts multipart/form-data.
        let ivy = s.app.mail.last_to("ivy@example.com").unwrap().unsubscribe.unwrap();
        let ivy_path = format!("{}?{}", ivy.path(), ivy.query().unwrap());
        let multipart = "--b\r\nContent-Disposition: form-data; name=\"List-Unsubscribe\"\r\n\r\nOne-Click\r\n--b--\r\n";
        let ivy_res = s
            .app
            .client()
            .send_body(
                Method::POST,
                &ivy_path,
                RawBody::new("multipart/form-data; boundary=b", multipart),
            )
            .await;
        assert_eq!(ivy_res.status, StatusCode::OK, "multipart one click");
        assert_eq!(
            s.contact(&list, "ivy@example.com").await["status"],
            "unsubscribed",
            "multipart one click"
        );
        assert_eq!(
            s.public(Method::POST, "unsubscribe", "nope").await.status,
            StatusCode::NOT_FOUND,
            "unknown"
        );
    }

    #[tokio::test]
    async fn broadcasts_go_to_subscribers_and_log_each_delivery() {
        let s = setup().await;
        let list = s.list(json!({"double_opt_in": false, "footer": "1 Main St"})).await;
        s.subscribed(&list, "ida@example.com", json!({"name": "Ida", "tags": ["vip"]})).await;
        s.subscribed(&list, "jo@example.com", json!({})).await;
        s.add(&s.list(json!({"name": "Other"})).await, "kim@example.com")
            .await
            .expect(StatusCode::CREATED);
        s.app.drain_jobs().await;
        let _confirms = s.app.mail.take();

        let preview: Value = s
            .member
            .post(&format!("{}/preview", s.ws()), &json!({"list_id": id(&list), "subject": "Hi {{ name }}", "body_md": "<b>x</b> *y*"}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(preview["subject"], "Hi Ada Lovelace", "merge");
        assert!(
            preview["html"].as_str().unwrap().contains("&lt;b&gt;x&lt;/b&gt; <em>y</em>"),
            "{preview}"
        );
        assert!(preview["html"].as_str().unwrap().contains("1 Main St"), "footer");

        let broadcast = s.broadcast(&list, "Hi {{ name }}").await;
        assert_eq!(broadcast["status"], "draft", "draft");
        let path = format!("{}/broadcasts/{}", s.ws(), id(&broadcast));
        let test_path = format!("{path}/test");
        s.member
            .post(&test_path, &json!({"to": "stranger@example.com"}))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
        let member_email = s.team.accounts["member"].email.clone();
        let test: Value = s
            .member
            .post(&test_path, &json!({"to": member_email}))
            .await
            .expect(StatusCode::CREATED)
            .json();
        assert_eq!(test["kind"], "test", "test delivery");
        s.app.drain_jobs().await;
        let mail = s.app.mail.take().pop().unwrap();
        assert_eq!(
            (mail.to.as_str(), mail.subject.as_str()),
            (member_email.as_str(), "Hello"),
            "test mail"
        );

        let sent: Value =
            s.member.post(&format!("{path}/send"), &json!({})).await.expect(StatusCode::OK).json();
        assert_eq!(sent["status"], "sending", "sending");
        s.member.patch(&path, &json!({"subject": "x"})).await.expect(StatusCode::CONFLICT);
        s.member.post(&format!("{path}/send"), &json!({})).await.expect(StatusCode::CONFLICT);
        s.member.delete(&path).await.expect(StatusCode::CONFLICT);
        s.app.drain_jobs().await;

        let done: Value = s.admin.get(&path).await.expect(StatusCode::OK).json();
        assert_eq!(
            (done["status"].clone(), done["recipients"].clone()),
            (json!("sent"), json!(2)),
            "{done}"
        );
        let mut to: Vec<String> = s.app.mail.take().into_iter().map(|m| m.to).collect();
        to.sort();
        assert_eq!(to, ["ida@example.com", "jo@example.com"], "subscribers only");
        let log = s.deliveries(&format!("broadcast_id={}", id(&broadcast))).await;
        assert_eq!(log.len(), 2, "test sends stay out of the log");
        assert!(log.iter().all(|d| d["status"] == "sent" && d["attempts"] == 1), "{log:?}");

        let tagged = s.broadcast(&list, "VIP only").await;
        let tagged_path = format!("{}/broadcasts/{}", s.ws(), id(&tagged));
        s.member
            .patch(&tagged_path, &json!({"segment": {"tags": ["vip"]}}))
            .await
            .expect(StatusCode::OK);
        s.member.post(&format!("{tagged_path}/send"), &json!({})).await.expect(StatusCode::OK);
        s.app.drain_jobs().await;
        let tagged_to: Vec<String> = s.app.mail.take().into_iter().map(|m| m.to).collect();
        assert_eq!(tagged_to, ["ida@example.com"], "tag");
    }

    #[tokio::test]
    async fn contacts_keep_typed_attributes_and_mail_fills_them() {
        let s = setup().await;
        let list = s.list(json!({"double_opt_in": false})).await;
        let ida = s
            .subscribed(
                &list,
                "ida@example.com",
                json!({"name": "Ida", "attributes": {"plan": "pro", "seats": 12, "trial": false}}),
            )
            .await;
        assert_eq!(ida["attributes"], json!({"plan": "pro", "seats": 12, "trial": false}), "{ida}");

        let path = format!("{}/{}", s.contacts(&list), id(&ida));
        let updated: Value = s
            .member
            .patch(&path, &json!({"attributes": {"plan": "team"}}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(updated["attributes"], json!({"plan": "team"}), "update replaces the object");
        let kept: Value =
            s.member.patch(&path, &json!({"name": "Ida B"})).await.expect(StatusCode::OK).json();
        assert_eq!(kept["attributes"], json!({"plan": "team"}), "absent keeps them");

        for bad in [json!({"Plan": "x"}), json!({"1st": "x"}), json!({"plan": "x".repeat(501)})] {
            s.member
                .patch(&path, &json!({"attributes": bad}))
                .await
                .expect(StatusCode::UNPROCESSABLE_ENTITY);
        }
        s.member
            .patch(&path, &json!({"attributes": {"nested": {"a": 1}}}))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);

        let broadcast = s.broadcast(&list, "You are on {{ attr.plan }} [{{ attr.city }}]").await;
        let send = format!("{}/broadcasts/{}/send", s.ws(), id(&broadcast));
        s.member.post(&send, &json!({})).await.expect(StatusCode::OK);
        s.app.drain_jobs().await;
        let mail = s.app.mail.take().pop().unwrap();
        assert!(mail.text.contains("You are on team []"), "{}", mail.text);
    }

    #[tokio::test]
    async fn segments_pick_subscribers_by_tags_attributes_and_join_date() {
        let s = setup().await;
        let list = s.list(json!({"double_opt_in": false})).await;
        s.subscribed(
            &list,
            "ann@example.com",
            json!({"tags": ["vip", "beta"], "attributes": {"plan": "pro", "seats": 5}}),
        )
        .await;
        s.subscribed(
            &list,
            "bob@example.com",
            json!({"tags": ["vip"], "attributes": {"plan": "free", "seats": 5.0}}),
        )
        .await;
        s.app.clock.advance(Duration::from_hours(24));
        let cy = s
            .subscribed(
                &list,
                "cy@example.com",
                json!({"attributes": {"plan": "pro", "seats": "5"}}),
            )
            .await;

        assert_eq!(s.audience(&list, json!({})).await, 3, "every subscriber");
        assert_eq!(s.audience(&list, json!({"tags": ["vip"]})).await, 2, "tag");
        assert_eq!(s.audience(&list, json!({"tags": ["vip", "beta"]})).await, 1, "every tag");
        let pro = json!({"attributes": [{"key": "plan", "equals": "pro"}]});
        assert_eq!(s.audience(&list, pro.clone()).await, 2, "attribute");
        assert_eq!(
            s.audience(&list, json!({"attributes": [{"key": "seats", "equals": 5}]})).await,
            2,
            "numbers match numbers, integer or real; text \"5\" doesn't"
        );
        assert_eq!(
            s.audience(&list, json!({"attributes": [{"key": "seats", "equals": "5"}]})).await,
            1,
            "text matches text"
        );
        let later = cy["subscribed_at"].clone();
        assert_eq!(s.audience(&list, json!({"joined_after": later})).await, 1, "joined after");
        assert_eq!(s.audience(&list, json!({"joined_before": later})).await, 2, "joined before");
        s.member
            .post(
                &format!("{}/lists/{}/audience", s.ws(), id(&list)),
                &json!({"segment": {"joined_after": later, "joined_before": later}}),
            )
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);

        let broadcast = s.broadcast(&list, "Hi").await;
        let path = format!("{}/broadcasts/{}", s.ws(), id(&broadcast));
        let segment = json!({"tags": ["vip"], "attributes": [{"key": "plan", "equals": "pro"}]});
        let draft: Value =
            s.member.patch(&path, &json!({"segment": segment})).await.expect(StatusCode::OK).json();
        assert_eq!(draft["segment"]["tags"], json!(["vip"]), "{draft}");
        s.member.post(&format!("{path}/send"), &json!({})).await.expect(StatusCode::OK);
        s.app.drain_jobs().await;
        let to: Vec<String> = s.app.mail.take().into_iter().map(|m| m.to).collect();
        assert_eq!(to, ["ann@example.com"], "the fan-out uses the segment");
    }

    #[tokio::test]
    async fn test_sends_are_capped_per_workspace() {
        let s = setup().await;
        let list = s.list(json!({})).await;
        let broadcast = s.broadcast(&list, "Hi").await;
        let path = format!("{}/broadcasts/{}/test", s.ws(), id(&broadcast));
        let to = json!({"to": s.team.accounts["admin"].email});
        for _ in 0..TESTS_PER_HOUR {
            s.member.post(&path, &to).await.expect(StatusCode::CREATED);
        }
        s.member.post(&path, &to).await.expect(StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn sends_skip_contacts_that_left_and_fail_after_the_last_attempt() {
        let s = setup().await;
        let list = s.list(json!({"double_opt_in": false})).await;
        let lea = s.subscribed(&list, "lea@example.com", json!({})).await;
        let broadcast = s.broadcast(&list, "Hi").await;
        let path = format!("{}/broadcasts/{}", s.ws(), id(&broadcast));
        s.app.mail.fail_next(1);
        s.member.post(&format!("{path}/send"), &json!({})).await.expect(StatusCode::OK);
        s.app.drain_jobs().await;
        let log = s.deliveries(&format!("broadcast_id={}", id(&broadcast))).await;
        assert_eq!(
            (log[0]["status"].clone(), log[0]["attempts"].clone()),
            (json!("queued"), json!(1)),
            "retrying"
        );
        assert!(log[0]["error"].as_str().unwrap().contains("fake mailer"), "error kept");

        let unsubscribe = format!("{}/{}/unsubscribe", s.contacts(&list), id(&lea));
        s.member.post(&unsubscribe, &json!({})).await.expect(StatusCode::OK);
        s.app.clock.advance(Duration::from_hours(2));
        s.app.drain_jobs().await;
        let after_skip = s.deliveries(&format!("broadcast_id={}", id(&broadcast))).await;
        assert_eq!(after_skip[0]["status"], "skipped", "skipped after unsubscribe");
        assert!(s.app.mail.take().is_empty(), "nothing sent");

        let mo = s.subscribed(&list, "mo@example.com", json!({})).await;
        let second = s.broadcast(&list, "Again").await;
        s.app.mail.fail_next(100);
        s.member
            .post(&format!("{}/broadcasts/{}/send", s.ws(), id(&second)), &json!({}))
            .await
            .expect(StatusCode::OK);
        for _ in 0..10 {
            s.app.drain_jobs().await;
            s.app.clock.advance(Duration::from_hours(2));
        }
        let failed_log = s.deliveries(&format!("contact_id={}", id(&mo))).await;
        assert_eq!(
            (failed_log[0]["status"].clone(), failed_log[0]["attempts"].clone()),
            (json!("failed"), json!(8)),
            "{failed_log:?}"
        );
        s.app.mail.fail_next(0);
    }

    #[tokio::test]
    async fn resend_queues_only_failed_deliveries_once() {
        let s = setup().await;
        let list = s.list(json!({"double_opt_in": false})).await;
        s.subscribed(&list, "ana@example.com", json!({})).await;
        s.subscribed(&list, "ben@example.com", json!({})).await;
        let cal = s.subscribed(&list, "cal@example.com", json!({})).await;
        let broadcast = s.broadcast(&list, "Hi").await;
        let path = format!("{}/broadcasts/{}", s.ws(), id(&broadcast));
        s.member.post(&format!("{path}/resend"), &json!({})).await.expect(StatusCode::CONFLICT);

        s.app.mail.fail_next(100);
        s.member.post(&format!("{path}/send"), &json!({})).await.expect(StatusCode::OK);
        s.app.drain_jobs().await;
        let unsubscribe = format!("{}/{}/unsubscribe", s.contacts(&list), id(&cal));
        s.member.post(&unsubscribe, &json!({})).await.expect(StatusCode::OK);
        for _ in 0..10 {
            s.app.clock.advance(Duration::from_hours(2));
            s.app.drain_jobs().await;
        }
        let got: Value = s.member.get(&path).await.expect(StatusCode::OK).json();
        assert_eq!(
            got["deliveries"],
            json!({"queued": 0, "sent": 0, "failed": 2, "skipped": 1, "bounced": 0, "complained": 0}),
            "two failed, one left the list"
        );

        s.app.mail.fail_next(0);
        let first: Value = s
            .member
            .post(&format!("{path}/resend"), &json!({}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(first["requeued"], 2, "only the failed ones");
        let again: Value = s
            .member
            .post(&format!("{path}/resend"), &json!({}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(again["requeued"], 0, "already queued, nothing to add");
        s.app.drain_jobs().await;

        let mut sent: Vec<String> = s.app.mail.take().into_iter().map(|mail| mail.to).collect();
        sent.sort();
        assert_eq!(sent, ["ana@example.com", "ben@example.com"], "one mail each, none to cal");
        let log = s.deliveries(&format!("broadcast_id={}", id(&broadcast))).await;
        let mut rows: Vec<(String, String, u64)> = log
            .iter()
            .map(|row| {
                (
                    row["to_email"].as_str().unwrap().to_owned(),
                    row["status"].as_str().unwrap().to_owned(),
                    row["resends"].as_u64().unwrap(),
                )
            })
            .collect();
        rows.sort();
        assert_eq!(
            rows,
            [
                ("ana@example.com".to_owned(), "sent".to_owned(), 1),
                ("ben@example.com".to_owned(), "sent".to_owned(), 1),
                ("cal@example.com".to_owned(), "skipped".to_owned(), 0),
            ],
            "the log shows the retry"
        );

        let done: Value = s
            .member
            .post(&format!("{path}/resend"), &json!({}))
            .await
            .expect(StatusCode::OK)
            .json();
        assert_eq!(done["requeued"], 0, "sent mail is never sent twice");
        s.app.drain_jobs().await;
        assert!(s.app.mail.take().is_empty(), "no mail");
    }

    #[tokio::test]
    async fn forms_list_shows_the_projects_forms_for_the_picker() {
        let s = setup().await;
        let form = s.form(json!([])).await;
        let path = format!("{}/forms?project_id={}", s.ws(), s.project());
        let forms: Value = s.member.get(&path).await.expect(StatusCode::OK).json();
        assert_eq!(forms["items"], json!([{"id": form, "name": "Signup"}]));
    }

    #[tokio::test]
    async fn forms_list_is_empty_without_the_forms_module() {
        let app = TestApp::builder().module(Mailing).build().await;
        let team = app.seed(Persona::Team, "mail").await;
        let member = app.with_session(&team.accounts["member"].session_token);
        let path = format!(
            "/api/v1/workspaces/{}/mailing/forms?project_id={}",
            team.workspaces["main"].id, team.projects["site"].id
        );
        let forms: Value = member.get(&path).await.expect(StatusCode::OK).json();
        assert_eq!(forms["items"], json!([]));
    }
}
