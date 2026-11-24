//! The public archive: which broadcasts it shows, escaping and the feed.

#[cfg(test)]
mod archive {
    use bizkit_mailing::Mailing;
    use bizkit_testkit::{ApiResponse, Manifest, Persona, SpecClient, TestApp};
    use http::{Method, StatusCode};
    use serde_json::{Value, json};

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Mailing).build().await;
        let team = app.seed(Persona::Team, "archive").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        Setup { app, team, admin }
    }

    fn id(value: &Value) -> &str {
        value["id"].as_str().unwrap()
    }

    impl Setup {
        fn ws(&self) -> String {
            format!("/api/v1/workspaces/{}/mailing", self.team.workspaces["main"].id)
        }

        /// A list without double opt-in that has one subscriber.
        async fn list(&self, name: &str) -> Value {
            let body = json!({
                "name": name,
                "project_id": self.team.projects["site"].id.to_string(),
                "double_opt_in": false,
            });
            let list: Value = self
                .admin
                .post(&format!("{}/lists", self.ws()), &body)
                .await
                .expect(StatusCode::CREATED)
                .json();
            let contacts = format!("{}/lists/{}/contacts", self.ws(), id(&list));
            let contact =
                json!({"email": "ann@example.com", "name": "Ann", "attributes": {"plan": "pro"}});
            self.admin.post(&contacts, &contact).await.expect(StatusCode::CREATED);
            list
        }

        async fn set_archive(&self, list: &Value, on: bool) -> Value {
            self.admin
                .patch(&format!("{}/lists/{}", self.ws(), id(list)), &json!({"archive": on}))
                .await
                .expect(StatusCode::OK)
                .json()
        }

        async fn broadcast(
            &self,
            list: &Value,
            subject: &str,
            archived: bool,
            send: bool,
        ) -> Value {
            let input = json!({
                "list_id": id(list),
                "subject": subject,
                "body_md": "Hi {{ name }}, you are on {{ attr.plan }}.",
                "archived": archived,
            });
            let broadcast: Value = self
                .admin
                .post(&format!("{}/broadcasts", self.ws()), &input)
                .await
                .expect(StatusCode::CREATED)
                .json();
            if send {
                let path = format!("{}/broadcasts/{}/send", self.ws(), id(&broadcast));
                self.admin.post(&path, &json!({})).await.expect(StatusCode::OK);
                self.app.drain_jobs().await;
            }
            broadcast
        }

        async fn patch_broadcast(&self, broadcast: &Value, body: Value) -> ApiResponse {
            self.admin.patch(&format!("{}/broadcasts/{}", self.ws(), id(broadcast)), &body).await
        }

        async fn get(&self, path: &str) -> ApiResponse {
            self.app.client().send(Method::GET, path, None).await
        }
    }

    /// The path of the list's archive page.
    fn archive_path(list: &Value) -> String {
        url::Url::parse(list["archive_url"].as_str().unwrap()).unwrap().path().to_owned()
    }

    #[tokio::test]
    async fn the_archive_shows_only_sent_broadcasts_marked_archived() {
        let s = setup().await;
        let list = s.list("Newsletter").await;
        assert_eq!(list["archive"], false, "off by default");
        let archive = archive_path(&list);
        assert!(archive.starts_with("/m/mailing/public/archive/"), "{archive}");

        let shown = s.broadcast(&list, "Shown <b>news</b> for {{ name }}", true, true).await;
        let unmarked = s.broadcast(&list, "Unmarked issue", false, true).await;
        let draft = s.broadcast(&list, "Draft issue", true, false).await;

        s.get(&archive).await.expect(StatusCode::NOT_FOUND);
        let list = s.set_archive(&list, true).await;
        assert_eq!(list["archive"], true, "turned on");

        let page = s.get(&archive).await;
        page.expect(StatusCode::OK);
        assert!(
            page.text.contains("Shown &lt;b&gt;news&lt;/b&gt; for"),
            "escaped subject: {}",
            page.text
        );
        assert!(!page.text.contains("<b>news"), "no raw markup");
        assert!(!page.text.contains("Ann"), "no reader's name");
        assert!(!page.text.contains("Unmarked issue"), "unmarked stays out");
        assert!(!page.text.contains("Draft issue"), "unsent stays out");
        assert!(
            page.headers[http::header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("default-src 'none'")
        );

        let issue = s.get(&format!("{archive}/{}", id(&shown))).await;
        issue.expect(StatusCode::OK);
        assert!(issue.text.contains("you are on ."), "merge fields render empty: {}", issue.text);
        assert!(!issue.text.contains("Unsubscribe"), "no reader links");
        s.get(&format!("{archive}/{}", id(&unmarked))).await.expect(StatusCode::NOT_FOUND);
        s.get(&format!("{archive}/{}", id(&draft))).await.expect(StatusCode::NOT_FOUND);
        s.get(&format!("{archive}/not-an-id")).await.expect(StatusCode::NOT_FOUND);

        let feed = s.get(&format!("{archive}/feed.xml")).await;
        feed.expect(StatusCode::OK);
        assert!(
            feed.headers[http::header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("application/rss+xml")
        );
        assert!(feed.text.starts_with("<?xml"), "{}", feed.text);
        assert_eq!(feed.text.matches("<item>").count(), 1, "{}", feed.text);
        assert!(
            feed.text.contains("<title>Shown &lt;b&gt;news&lt;/b&gt; for</title>"),
            "{}",
            feed.text
        );
        assert!(feed.text.contains("<pubDate>"), "{}", feed.text);

        s.set_archive(&list, false).await;
        s.get(&archive).await.expect(StatusCode::NOT_FOUND);
        s.get(&format!("{archive}/feed.xml")).await.expect(StatusCode::NOT_FOUND);
        s.get(&format!("{archive}/{}", id(&shown))).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn archived_changes_after_a_send_and_nothing_else_does() {
        let s = setup().await;
        let list = s.set_archive(&s.list("Newsletter").await, true).await;
        let archive = archive_path(&list);
        let sent = s.broadcast(&list, "Later issue", false, true).await;
        assert!(!s.get(&archive).await.text.contains("Later issue"), "not yet");

        let res = s.patch_broadcast(&sent, json!({"archived": true})).await;
        res.expect(StatusCode::OK);
        assert_eq!(res.body["archived"], true, "{}", res.text);
        assert_eq!(res.body["status"], "sent", "still sent");
        assert!(s.get(&archive).await.text.contains("Later issue"), "now shown");

        s.patch_broadcast(&sent, json!({"subject": "Changed"})).await.expect(StatusCode::CONFLICT);
        s.patch_broadcast(&sent, json!({"archived": false, "subject": "Changed"}))
            .await
            .expect(StatusCode::CONFLICT);
        let page = s.get(&archive).await;
        assert!(page.text.contains("Later issue"), "a refused patch changes nothing");
    }

    #[tokio::test]
    async fn a_key_shows_only_its_own_list() {
        let s = setup().await;
        let first = s.set_archive(&s.list("First").await, true).await;
        let second = s.set_archive(&s.list("Second").await, true).await;
        let other = s.broadcast(&second, "Second's issue", true, true).await;
        assert_ne!(first["archive_url"], second["archive_url"], "keys differ");

        s.get(&format!("{}/{}", archive_path(&first), id(&other)))
            .await
            .expect(StatusCode::NOT_FOUND);
        assert!(!s.get(&archive_path(&first)).await.text.contains("Second's issue"));
        s.get("/m/mailing/public/archive/unknown-key").await.expect(StatusCode::NOT_FOUND);
    }
}
