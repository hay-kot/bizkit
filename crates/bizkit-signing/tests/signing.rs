//! The signer's side: the signing page, signatures, the seal and the
//! completion mails.

#[cfg(test)]
mod signing {
    use std::time::Duration;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use bizkit_signing::Signing;
    use bizkit_signing::testing::sample_pdf;
    use bizkit_testkit::{ApiResponse, Manifest, Persona, RawBody, SpecClient, TestApp};
    use http::{Method, StatusCode, header};
    use serde_json::{Value, json};

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Signing).build().await;
        let team = app.seed(Persona::Team, "pub").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        Setup { app, team, admin }
    }

    /// The `/m/signing/public/...` path and query of the first link to `page` in `text`.
    fn link(text: &str, page: &str) -> String {
        let marker = format!("/m/signing/public/{page}?t=");
        let word = text.split_whitespace().find(|word| word.contains(&marker)).unwrap();
        let (_, rest) = word.split_once(&marker).unwrap();
        format!("{marker}{rest}")
    }

    impl Setup {
        fn path(&self, rest: &str) -> String {
            format!("/api/v1/workspaces/{}/signing/{rest}", self.team.workspaces["main"].id)
        }

        /// Creates a request to `signers`, gives each a signature field and
        /// sends it. Returns its ID.
        async fn send(&self, signers: &Value) -> String {
            let body = json!({
                "project_id": self.team.projects["site"].id,
                "title": "Services agreement",
                "file_name": "agreement.pdf",
                "pdf_base64": STANDARD.encode(sample_pdf(&["Terms"])),
                "signers": signers,
            });
            let created = self.admin.post(&self.path("requests"), &body).await;
            created.expect(StatusCode::CREATED);
            let id = created["id"].as_str().unwrap().to_owned();
            let fields: Vec<Value> = created["signers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|signer| {
                    json!({ "signer_id": signer["id"], "kind": "signature", "page": 1,
                            "x": 0.1, "y": 0.7, "width": 0.3, "height": 0.08 })
                })
                .collect();
            self.admin
                .send(
                    Method::PUT,
                    &self.path(&format!("requests/{id}/fields")),
                    Some(&json!({ "fields": fields })),
                )
                .await
                .expect(StatusCode::OK);
            self.admin
                .post(&self.path(&format!("requests/{id}/send")), &json!({}))
                .await
                .expect(StatusCode::OK);
            self.app.drain_jobs().await;
            id
        }

        /// A sent request to Ada and Grace; their sign links, in that order.
        async fn sent(&self) -> (String, Vec<String>) {
            let id = self
                .send(&json!([
                    { "name": "Ada Lovelace", "email": "ada@example.test" },
                    { "name": "Grace Hopper", "email": "grace@example.test" },
                ]))
                .await;
            let mails = self.app.mail.sent();
            let link_for =
                |to: &str| link(&mails.iter().find(|m| m.to == to).unwrap().text, "sign");
            (id, vec![link_for("ada@example.test"), link_for("grace@example.test")])
        }

        async fn post_form(&self, path: &str, form: &[(&str, &str)]) -> ApiResponse {
            let body =
                url::form_urlencoded::Serializer::new(String::new()).extend_pairs(form).finish();
            let raw = RawBody::new("application/x-www-form-urlencoded", body);
            self.app.client().send_body(Method::POST, path, raw).await
        }

        async fn sign(&self, sign_link: &str, form: &[(&str, &str)]) -> ApiResponse {
            let token = sign_link.split("t=").nth(1).unwrap();
            let mut fields = vec![("t", token)];
            fields.extend_from_slice(form);
            self.post_form("/m/signing/public/sign", &fields).await
        }

        async fn detail(&self, id: &str) -> Value {
            self.admin
                .get(&self.path(&format!("requests/{id}")))
                .await
                .expect(StatusCode::OK)
                .json()
        }
    }

    #[tokio::test]
    async fn the_page_shows_the_document_and_changes_nothing() {
        let s = setup().await;
        let (id, links) = s.sent().await;
        let page = s.app.client().get(&links[0]).await;
        page.expect(StatusCode::OK);
        assert!(page.text.contains("Services agreement"), "{}", page.text);
        assert!(page.text.contains("Hi Ada Lovelace"), "{}", page.text);
        assert!(page.text.contains("<iframe src=\"document?t="), "{}", page.text);
        let csp = page.headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(csp.contains("script-src 'nonce-"), "{csp}");
        assert!(csp.contains("frame-src 'self'"), "{csp}");
        let kinds: Vec<Value> = s.detail(&id).await["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].clone())
            .collect();
        assert!(!kinds.contains(&json!("viewed")), "a GET records nothing: {kinds:?}");

        let doc = s.app.client().get(&links[0].replace("/sign?", "/document?")).await;
        doc.expect(StatusCode::OK);
        assert_eq!(doc.headers[header::CONTENT_TYPE], "application/pdf");
        s.app.client().get("/m/signing/public/sign?t=nope").await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_view_is_recorded_once() {
        let s = setup().await;
        let (id, links) = s.sent().await;
        let token = links[0].split("t=").nth(1).unwrap();
        for _ in 0..3 {
            s.post_form("/m/signing/public/viewed", &[("t", token)])
                .await
                .expect(StatusCode::NO_CONTENT);
        }
        let views = s.detail(&id).await["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "viewed")
            .count();
        assert_eq!(views, 1);
    }

    #[tokio::test]
    async fn signing_needs_consent_and_a_signature() {
        let s = setup().await;
        let (_, links) = s.sent().await;
        let no_consent = s.sign(&links[0], &[("kind", "typed"), ("typed_name", "Ada")]).await;
        no_consent.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert!(no_consent.text.contains("Tick the box"), "{}", no_consent.text);
        let empty =
            s.sign(&links[0], &[("consent", "yes"), ("kind", "drawn"), ("strokes", "")]).await;
        empty.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert!(empty.text.contains("Draw your signature"), "{}", empty.text);
    }

    #[tokio::test]
    async fn a_voided_request_takes_no_more_signatures() {
        let s = setup().await;
        let (id, links) = s.sent().await;
        let ada = s
            .sign(
                &links[0],
                &[("consent", "yes"), ("kind", "typed"), ("typed_name", "Ada Lovelace")],
            )
            .await;
        ada.expect(StatusCode::OK);
        assert!(ada.text.contains("Thank you"), "{}", ada.text);
        s.admin
            .post(&s.path(&format!("requests/{id}/void")), &json!({}))
            .await
            .expect(StatusCode::OK);
        // A voided request takes no more signatures.
        s.sign(&links[1], &[("consent", "yes"), ("kind", "typed"), ("typed_name", "Grace")])
            .await
            .expect(StatusCode::NOT_FOUND);
        // Nor does it promise Ada a signed copy.
        s.app.client().get(&links[0]).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn everyone_signs_and_gets_the_sealed_pdf() {
        let s = setup().await;
        let (id, links) = s.sent().await;
        s.sign(&links[0], &[("consent", "yes"), ("kind", "typed"), ("typed_name", "Ada Lovelace")])
            .await
            .expect(StatusCode::OK);
        let strokes = "[[[10,10],[200,150],[400,40]],[[500,200],[900,250]]]";
        s.sign(&links[1], &[("consent", "yes"), ("kind", "drawn"), ("strokes", strokes)])
            .await
            .expect(StatusCode::OK);
        s.admin
            .post(&s.path(&format!("requests/{id}/void")), &json!({}))
            .await
            .expect(StatusCode::CONFLICT);
        drop(s.app.mail.take());
        s.app.drain_jobs().await;

        let detail = s.detail(&id).await;
        assert_eq!(detail["request"]["status"], "completed", "{detail}");
        let kinds: Vec<&str> = detail["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["created", "sent", "sent", "signed", "signed", "completed"]);
        let signed = detail["events"][3]["detail"].as_str().unwrap();
        assert!(signed.contains("I agree to sign this document electronically"), "{signed}");

        let doc = s.admin.get(&s.path(&format!("requests/{id}/document?version=signed"))).await;
        doc.expect(StatusCode::OK);
        assert_eq!(doc["file_name"], "agreement-signed.pdf");
        assert_eq!(doc["sha256"], detail["request"]["signed_sha256"]);
        let pdf = STANDARD.decode(doc["pdf_base64"].as_str().unwrap()).unwrap();
        assert!(pdf.windows(19).any(|w| w == b"ETSI.CAdES.detached"), "a PAdES signature");

        let mails = s.app.mail.sent();
        let to: Vec<&str> = mails.iter().map(|m| m.to.as_str()).collect();
        assert_eq!(
            to,
            ["ada@example.test", "grace@example.test", "admin+pub@e2e.test"],
            "signers, then the sender"
        );
        assert!(mails[0].text.contains(detail["request"]["signed_sha256"].as_str().unwrap()));
        let download = link(&mails[0].text, "signed.pdf");
        let file = s.app.client().get(&download).await;
        file.expect(StatusCode::OK);
        assert_eq!(
            file.headers[header::CONTENT_DISPOSITION],
            "attachment; filename=\"agreement-signed.pdf\""
        );
        // The sign link still works and leads to the download.
        let page = s.app.client().get(&links[0]).await;
        page.expect(StatusCode::OK);
        assert!(page.text.contains("Download the signed PDF"), "{}", page.text);
        s.app
            .client()
            .get(&links[0].replace("/sign?", "/signed.pdf?"))
            .await
            .expect(StatusCode::OK);
    }

    #[tokio::test]
    async fn a_broken_original_fails_after_three_tries_and_can_be_retried() {
        let s = setup().await;
        let (id, links) = s.sent().await;
        let mut tx = s.app.app().platform().db().write().await.unwrap();
        sqlx::query("UPDATE signing_files SET bytes = x'00' WHERE kind = 'original'")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        for link in &links {
            s.sign(link, &[("consent", "yes"), ("kind", "typed"), ("typed_name", "Someone")])
                .await
                .expect(StatusCode::OK);
        }
        for _ in 0..4 {
            s.app.drain_jobs().await;
            s.app.clock.advance(Duration::from_mins(10));
        }
        let detail = s.detail(&id).await;
        assert_eq!(detail["request"]["status"], "failed", "{detail}");
        assert!(
            detail["request"]["failure"].as_str().unwrap().contains("original PDF"),
            "{detail}"
        );
        let retried = s.admin.post(&s.path(&format!("requests/{id}/retry")), &json!({})).await;
        retried.expect(StatusCode::OK);
        assert_eq!(retried["status"], "sent");
        s.admin
            .post(&s.path(&format!("requests/{id}/retry")), &json!({}))
            .await
            .expect(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_workspace_mails_at_most_100_signers_an_hour() {
        let s = setup().await;
        let signers: Vec<Value> = (0..10)
            .map(
                |n| json!({ "name": format!("Signer {n}"), "email": format!("s{n}@example.test") }),
            )
            .collect();
        let signers = Value::Array(signers);
        for _ in 0..10 {
            s.send(&signers).await;
        }
        let body = json!({
            "project_id": s.team.projects["site"].id,
            "title": "Bulk",
            "file_name": "bulk.pdf",
            "pdf_base64": STANDARD.encode(sample_pdf(&["x"])),
            "signers": signers,
        });
        let created = s.admin.post(&s.path("requests"), &body).await;
        created.expect(StatusCode::CREATED);
        let id = created["id"].as_str().unwrap();
        let fields: Vec<Value> = created["signers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|signer| {
                json!({ "signer_id": signer["id"], "kind": "signature", "page": 1,
                        "x": 0.1, "y": 0.1, "width": 0.1, "height": 0.1 })
            })
            .collect();
        s.admin
            .send(
                Method::PUT,
                &s.path(&format!("requests/{id}/fields")),
                Some(&json!({ "fields": fields })),
            )
            .await
            .expect(StatusCode::OK);
        s.admin
            .post(&s.path(&format!("requests/{id}/send")), &json!({}))
            .await
            .expect(StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn later_groups_count_toward_the_cap_at_the_first_send() {
        let s = setup().await;
        // 10 requests of 10 signers in two groups mail only 50 at first, but
        // count 100.
        let signers: Vec<Value> = (0..10)
            .map(|n| {
                json!({ "name": format!("Signer {n}"), "email": format!("s{n}@example.test"),
                        "signing_order": 1 + n / 5 })
            })
            .collect();
        let signers = Value::Array(signers);
        for _ in 0..10 {
            s.send(&signers).await;
        }
        assert_eq!(s.app.mail.sent().len(), 50, "only the first groups");
        let created = s
            .admin
            .post(
                &s.path("requests"),
                &json!({
                    "project_id": s.team.projects["site"].id,
                    "title": "One too many",
                    "file_name": "x.pdf",
                    "pdf_base64": STANDARD.encode(sample_pdf(&["x"])),
                    "signers": signers,
                }),
            )
            .await;
        let id = created["id"].as_str().unwrap();
        let fields: Vec<Value> = created["signers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|signer| {
                json!({ "signer_id": signer["id"], "kind": "signature", "page": 1,
                        "x": 0.1, "y": 0.1, "width": 0.1, "height": 0.1 })
            })
            .collect();
        s.admin
            .send(
                Method::PUT,
                &s.path(&format!("requests/{id}/fields")),
                Some(&json!({ "fields": fields })),
            )
            .await
            .expect(StatusCode::OK);
        s.admin
            .post(&s.path(&format!("requests/{id}/send")), &json!({}))
            .await
            .expect(StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn signing_order_groups_get_their_links_in_turn() {
        let s = setup().await;
        let id = s
            .send(&json!([
                { "name": "Grace Hopper", "email": "grace@example.test", "signing_order": 5 },
                { "name": "Ada Lovelace", "email": "ada@example.test", "signing_order": 2 },
                { "name": "Alan Turing", "email": "alan@example.test", "signing_order": 5 },
            ]))
            .await;
        let first = s.app.mail.take();
        let to: Vec<&str> = first.iter().map(|m| m.to.as_str()).collect();
        assert_eq!(to, ["ada@example.test"], "only the lowest group");

        let ada = link(&first[0].text, "sign");
        s.sign(&ada, &[("consent", "yes"), ("kind", "typed"), ("typed_name", "Ada Lovelace")])
            .await
            .expect(StatusCode::OK);
        s.app.drain_jobs().await;
        let second = s.app.mail.take();
        let mut together: Vec<&str> = second.iter().map(|m| m.to.as_str()).collect();
        together.sort_unstable();
        assert_eq!(together, ["alan@example.test", "grace@example.test"], "equal orders together");
        let detail = s.detail(&id).await;
        assert_eq!(detail["request"]["status"], "sent");
        let kinds: Vec<&str> = detail["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["created", "sent", "signed", "sent", "sent"]);

        let grace =
            link(&second.iter().find(|m| m.to == "grace@example.test").unwrap().text, "sign");
        s.sign(&grace, &[("consent", "yes"), ("kind", "typed"), ("typed_name", "Grace Hopper")])
            .await
            .expect(StatusCode::OK);
        s.app.drain_jobs().await;
        assert!(s.app.mail.take().is_empty(), "Alan's group is still signing");
        let alan = link(&second.iter().find(|m| m.to == "alan@example.test").unwrap().text, "sign");
        s.sign(&alan, &[("consent", "yes"), ("kind", "typed"), ("typed_name", "Alan Turing")])
            .await
            .expect(StatusCode::OK);
        s.app.drain_jobs().await;
        assert_eq!(s.detail(&id).await["request"]["status"], "completed");
    }
}
