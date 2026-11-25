//! The signer's side: the session, signatures and field fills, the seal
//! and the completion mails.

#[cfg(test)]
mod signing {
    use std::time::Duration;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use bizkit_core::time::UnixMillis;
    use bizkit_signing::Signing;
    use bizkit_signing::testing::sample_pdf;
    use bizkit_testkit::{ApiResponse, BASE_URL, Manifest, Persona, SpecClient, TestApp};
    use http::{Method, StatusCode, header};
    use lopdf::content::Content;
    use lopdf::{Document, Object};
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

    /// The token of the first sign link in `text`.
    fn token(text: &str) -> String {
        let prefix = format!("{BASE_URL}/sign?t=");
        let word = text.split_whitespace().find(|word| word.starts_with(&prefix)).unwrap();
        let url = url::Url::parse(word).unwrap();
        url.query_pairs().find(|(k, _)| k == "t").unwrap().1.into_owned()
    }

    /// The `/m/signing/public/...` path and query of the first link to `page` in `text`.
    fn link(text: &str, page: &str) -> String {
        let marker = format!("/m/signing/public/{page}?t=");
        let word = text.split_whitespace().find(|word| word.contains(&marker)).unwrap();
        let (_, rest) = word.split_once(&marker).unwrap();
        format!("{marker}{rest}")
    }

    fn signature_field(signer: &Value) -> Value {
        json!({ "signer_id": signer["id"], "kind": "signature", "page": 1,
                "x": 0.1, "y": 0.7, "width": 0.3, "height": 0.08 })
    }

    fn typed(name: &str) -> Value {
        json!({ "kind": "typed", "name": name })
    }

    fn field_errors(response: &ApiResponse) -> Vec<String> {
        let mut keys: Vec<String> =
            response.body["error"]["fields"].as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    }

    /// The pages of a PDF, and the overlay `XObject`'s decoded operators on
    /// `page`, if it has one.
    fn overlay_ops(doc: &Document, page: u32) -> Option<Vec<lopdf::content::Operation>> {
        let page = doc.get_dictionary(doc.get_pages()[&page]).unwrap();
        let resources = page.get(b"Resources").ok()?.as_dict().ok()?;
        let xobjects = resources.get(b"XObject").ok()?.as_dict().ok()?;
        let form =
            doc.get_object(xobjects.get(b"BizkitFields").ok()?.as_reference().unwrap()).unwrap();
        let content = form.as_stream().unwrap().get_plain_content().unwrap();
        Some(Content::decode(&content).unwrap().operations)
    }

    impl Setup {
        fn path(&self, rest: &str) -> String {
            format!("/api/v1/workspaces/{}/signing/{rest}", self.team.workspaces["main"].id)
        }

        /// Creates a request to `signers`, places the fields `fields` makes
        /// from the created signers, and sends it. Returns the request.
        async fn send_with(
            &self,
            signers: &Value,
            fields: impl Fn(&[Value]) -> Vec<Value>,
        ) -> Value {
            let body = json!({
                "project_id": self.team.projects["site"].id,
                "title": "Services agreement",
                "file_name": "agreement.pdf",
                "pdf_base64": STANDARD.encode(sample_pdf(&["Terms", "Schedule"])),
                "signers": signers,
            });
            let created = self.admin.post(&self.path("requests"), &body).await;
            created.expect(StatusCode::CREATED);
            let id = created["id"].as_str().unwrap().to_owned();
            let fields = fields(created["signers"].as_array().unwrap());
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
            self.detail(&id).await
        }

        /// [`send_with`] a signature field for each signer. Returns its ID.
        async fn send(&self, signers: &Value) -> String {
            let detail = self.send_with(signers, |s| s.iter().map(signature_field).collect()).await;
            detail["request"]["id"].as_str().unwrap().to_owned()
        }

        /// A sent request to Ada and Grace; their link tokens, in that order.
        async fn sent(&self) -> (String, Vec<String>) {
            let id = self
                .send(&json!([
                    { "name": "Ada Lovelace", "email": "ada@example.test" },
                    { "name": "Grace Hopper", "email": "grace@example.test" },
                ]))
                .await;
            (id, vec![self.token_for("ada@example.test"), self.token_for("grace@example.test")])
        }

        fn token_for(&self, to: &str) -> String {
            token(&self.app.mail.sent().iter().rev().find(|m| m.to == to).unwrap().text)
        }

        async fn session(&self, token: &str) -> ApiResponse {
            self.app.client().get(&format!("/m/signing/public/session?t={token}")).await
        }

        async fn sign(&self, token: &str, body: &Value) -> ApiResponse {
            let mut body = body.clone();
            body["t"] = json!(token);
            self.app.client().post("/m/signing/public/sign", &body).await
        }

        /// Signs with consent and a typed signature, and nothing else.
        async fn sign_as(&self, token: &str, name: &str) -> ApiResponse {
            self.sign(token, &json!({ "consent": true, "signature": typed(name) })).await
        }

        async fn detail(&self, id: &str) -> Value {
            self.admin
                .get(&self.path(&format!("requests/{id}")))
                .await
                .expect(StatusCode::OK)
                .json()
        }

        async fn signed_pdf(&self, id: &str) -> Document {
            let doc =
                self.admin.get(&self.path(&format!("requests/{id}/document?version=signed"))).await;
            doc.expect(StatusCode::OK);
            let pdf = STANDARD.decode(doc["pdf_base64"].as_str().unwrap()).unwrap();
            assert!(pdf.windows(19).any(|w| w == b"ETSI.CAdES.detached"), "a PAdES signature");
            Document::load_mem(&pdf).unwrap()
        }
    }

    #[tokio::test]
    async fn the_session_shows_the_signers_fields_and_changes_nothing() {
        let s = setup().await;
        let (id, tokens) = s.sent().await;
        let session = s.session(&tokens[0]).await;
        session.expect(StatusCode::OK);
        assert_eq!(session["request"]["title"], "Services agreement");
        assert_eq!(session["request"]["status"], "sent");
        assert_eq!(session["request"]["page_count"], 2);
        assert_eq!(session["signer"]["name"], "Ada Lovelace");
        assert_eq!(session["signer"]["status"], "pending");
        assert!(session["consent"].as_str().unwrap().contains("electronically"));
        let fields = session["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1, "only Ada's own field");
        let ada = s.detail(&id).await["request"]["signers"][0]["id"].clone();
        assert_eq!(fields[0]["signer_id"], ada);
        assert_eq!(session.headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(session.headers[header::REFERRER_POLICY], "no-referrer");

        let kinds: Vec<Value> = s.detail(&id).await["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].clone())
            .collect();
        assert!(!kinds.contains(&json!("viewed")), "a GET records nothing: {kinds:?}");

        let doc = s.app.client().get(&format!("/m/signing/public/document?t={}", tokens[0])).await;
        doc.expect(StatusCode::OK);
        assert_eq!(doc.headers[header::CONTENT_TYPE], "application/pdf");
        let missing = s.session("nope").await;
        missing.expect(StatusCode::NOT_FOUND);
        let no_document = s.app.client().get("/m/signing/public/document?t=nope").await;
        no_document.expect(StatusCode::NOT_FOUND);
        for gone in [&missing, &no_document] {
            assert_eq!(gone.headers[header::CACHE_CONTROL], "no-store");
            assert_eq!(gone.headers[header::REFERRER_POLICY], "no-referrer");
        }
    }

    #[tokio::test]
    async fn old_sign_links_go_to_the_web_app() {
        let s = setup().await;
        let old = s.app.client().get("/m/signing/public/sign?t=abc%2Bdef").await;
        old.expect(StatusCode::SEE_OTHER);
        assert_eq!(old.headers[header::LOCATION], format!("{BASE_URL}/sign?t=abc%2Bdef"));
        assert_eq!(old.headers[header::REFERRER_POLICY], "no-referrer");
    }

    #[tokio::test]
    async fn a_view_is_recorded_once() {
        let s = setup().await;
        let (id, tokens) = s.sent().await;
        for _ in 0..3 {
            s.app
                .client()
                .post("/m/signing/public/viewed", &json!({ "t": tokens[0] }))
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
        s.app
            .client()
            .post("/m/signing/public/viewed", &json!({}))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// A sent request where Ada has one field of each kind plus an optional
    /// text, and Grace a signature: its ID, the field IDs in order, and
    /// Ada's token.
    async fn request_with_every_kind(s: &Setup) -> (String, Vec<String>, String) {
        let detail = s
            .send_with(
                &json!([
                    { "name": "Ada Lovelace", "email": "ada@example.test" },
                    { "name": "Grace Hopper", "email": "grace@example.test" },
                ]),
                |signers| {
                    let field = |signer: &Value, kind: &str, page: u32, y: f64, extra: Value| {
                        let mut field = json!({ "signer_id": signer["id"], "kind": kind, "page": page,
                                                "x": 0.1, "y": y, "width": 0.3, "height": 0.05 });
                        field.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
                        field
                    };
                    let (ada, grace) = (&signers[0], &signers[1]);
                    vec![
                        field(ada, "signature", 1, 0.1, json!({})),
                        field(ada, "initials", 2, 0.1, json!({})),
                        field(ada, "date", 1, 0.2, json!({})),
                        field(ada, "text", 2, 0.3, json!({ "label": "Company" })),
                        field(ada, "checkbox", 2, 0.4, json!({})),
                        field(ada, "text", 2, 0.5, json!({ "required": false, "label": "Notes" })),
                        field(grace, "signature", 1, 0.6, json!({})),
                    ]
                },
            )
            .await;
        let id = detail["request"]["id"].as_str().unwrap().to_owned();
        let ids: Vec<String> = detail["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["id"].as_str().unwrap().to_owned())
            .collect();
        let ada = s.token_for("ada@example.test");
        (id, ids, ada)
    }

    #[tokio::test]
    async fn signing_needs_consent_and_every_required_field() {
        let s = setup().await;
        let (_, ids, ada) = request_with_every_kind(&s).await;

        let empty = s.sign(&ada, &json!({ "consent": false })).await;
        empty.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let mut expected = vec![
            "consent".to_owned(),
            format!("fields.{}", ids[3]),
            format!("fields.{}", ids[4]),
            "initials".to_owned(),
            "signature".to_owned(),
        ];
        expected.sort();
        assert_eq!(field_errors(&empty), expected, "the optional text may stay empty");

        let wrong = s
            .sign(
                &ada,
                &json!({
                    "consent": true,
                    "signature": typed("Ada Lovelace"),
                    "initials": typed("AL"),
                    "values": [
                        { "field_id": ids[6], "text": "Grace's" },
                        { "field_id": ids[3], "text": "Line\nbreak" },
                        { "field_id": ids[4], "checked": false },
                    ],
                }),
            )
            .await;
        wrong.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            field_errors(&wrong),
            [
                format!("fields.{}", ids[4]),
                "values[0].field_id".to_owned(),
                "values[1].text".to_owned()
            ],
            "another signer's field, two lines, and a required box left clear"
        );
    }

    #[tokio::test]
    async fn fills_are_stored_drawn_and_listed_on_the_certificate() {
        let s = setup().await;
        let (id, ids, ada) = request_with_every_kind(&s).await;
        // 23:30 UTC on 2026-01-01, the fake clock's first day: the date
        // field takes the server's UTC day.
        s.app.clock.set(UnixMillis(1_767_310_200_000));
        let drawn =
            json!({ "kind": "drawn", "strokes": [[[10, 10], [200, 150]], [[300, 20], [500, 40]]] });
        let signed = s
            .sign(
                &ada,
                &json!({
                    "consent": true,
                    "signature": typed("Ada Lovelace"),
                    "initials": drawn,
                    "values": [
                        { "field_id": ids[3], "text": "  Acme Ltd " },
                        { "field_id": ids[4], "checked": true },
                    ],
                }),
            )
            .await;
        signed.expect(StatusCode::OK);
        assert_eq!(signed["status"], "sent", "Grace is still to sign");

        let fields = s.detail(&id).await["fields"].clone();
        assert_eq!(fields[0]["value"], json!({ "kind": "signed" }));
        assert_eq!(fields[1]["value"], json!({ "kind": "initialed" }));
        assert_eq!(fields[2]["value"], json!({ "kind": "date", "date": "2026-01-01" }));
        assert_eq!(fields[3]["value"], json!({ "kind": "text", "text": "Acme Ltd" }));
        assert_eq!(fields[4]["value"], json!({ "kind": "checkbox", "checked": true }));
        assert_eq!(fields[5]["value"], Value::Null, "the optional text stays empty");
        assert_eq!(fields[5]["filled_at"], Value::Null);
        assert_eq!(fields[2]["filled_at"], "2026-01-01T23:30:00Z");
        assert_eq!(fields[6]["value"], Value::Null, "Grace's field waits for her");

        let session = s.session(&ada).await;
        assert_eq!(session["signer"]["status"], "signed");
        assert_eq!(session["fields"][2]["value"]["date"], "2026-01-01");
        s.sign_as(&ada, "Ada Lovelace").await.expect(StatusCode::NOT_FOUND);

        s.sign_as(&s.token_for("grace@example.test"), "Grace Hopper").await.expect(StatusCode::OK);
        s.app.drain_jobs().await;
        let doc = s.signed_pdf(&id).await;
        let pages = u32::try_from(doc.get_pages().len()).unwrap();
        assert!(pages >= 3, "two pages and the certificate");
        let schedule = overlay_ops(&doc, 2).unwrap();
        let strings: Vec<&[u8]> = schedule
            .iter()
            .filter(|op| op.operator == "Tj")
            .map(|op| op.operands[0].as_str().unwrap())
            .collect();
        assert_eq!(strings, [&b"Acme Ltd"[..]], "drawn initials and the tick are paths");
        assert!(
            schedule.iter().filter(|op| op.operator == "S").count() >= 3,
            "two strokes and a tick"
        );
        let certificate: Vec<u32> = (3..=pages).collect();
        let text = doc.extract_text(&certificate).unwrap();
        assert!(text.contains("Page 2, Text 'Company': Acme Ltd"), "{text}");
        assert!(text.contains("Page 2, Checkbox: ticked"), "{text}");
        assert!(text.contains("Page 1, Date: 2026-01-01"), "{text}");
        assert!(!text.contains("Notes"), "only filled fields are listed: {text}");
    }

    #[tokio::test]
    async fn a_voided_request_takes_no_more_signatures() {
        let s = setup().await;
        let (id, tokens) = s.sent().await;
        s.sign_as(&tokens[0], "Ada Lovelace").await.expect(StatusCode::OK);
        s.admin
            .post(&s.path(&format!("requests/{id}/void")), &json!({}))
            .await
            .expect(StatusCode::OK);
        s.sign_as(&tokens[1], "Grace").await.expect(StatusCode::NOT_FOUND);
        // Nor does it promise Ada a signed copy.
        s.session(&tokens[0]).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn everyone_signs_and_gets_the_sealed_pdf() {
        let s = setup().await;
        let (id, tokens) = s.sent().await;
        s.sign_as(&tokens[0], "Ada Lovelace").await.expect(StatusCode::OK);
        let strokes = json!([[[10, 10], [200, 150], [400, 40]], [[500, 200], [900, 250]]]);
        s.sign(
            &tokens[1],
            &json!({ "consent": true, "signature": { "kind": "drawn", "strokes": strokes } }),
        )
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

        let doc = s.signed_pdf(&id).await;
        assert_eq!(doc.get_pages().len(), 3, "the two pages and one certificate page");
        let page1 = overlay_ops(&doc, 1).expect("the fields' overlay on page 1");
        let typed: Vec<(&[u8], &[u8])> = page1
            .iter()
            .filter(|op| op.operator == "Tf" || op.operator == "Tj")
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|pair| {
                (pair[0].operands[0].as_name().unwrap(), pair[1].operands[0].as_str().unwrap())
            })
            .collect();
        assert_eq!(typed, [(&b"F3"[..], &b"Ada Lovelace"[..])], "Ada's typed signature, oblique");
        assert!(page1.iter().any(|op| op.operator == "l"), "Grace's strokes");
        assert!(overlay_ops(&doc, 2).is_none(), "no fields on page 2");
        let contents = doc.get_dictionary(doc.get_pages()[&1]).unwrap().get(b"Contents").unwrap();
        assert_eq!(contents.as_array().unwrap().len(), 4, "q, the original, Q, the overlay");
        let text = doc.extract_text(&[3]).unwrap();
        assert!(text.contains("Certificate of completion"), "{text}");
        assert!(text.contains("Ada Lovelace <ada@example.test>"), "{text}");
        assert!(text.contains("Page 1, Signature: signed"), "{text}");
        assert!(!text.contains("Audit trail"), "the certificate replaces the old pages: {text}");

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
        // The sign link still opens the session and leads to the download.
        let session = s.session(&tokens[0]).await;
        session.expect(StatusCode::OK);
        assert_eq!(session["request"]["status"], "completed");
        s.app
            .client()
            .get(&format!("/m/signing/public/signed.pdf?t={}", tokens[0]))
            .await
            .expect(StatusCode::OK);
    }

    #[tokio::test]
    async fn send_refuses_a_draft_whose_field_page_cant_take_fields() {
        let s = setup().await;
        let body = json!({
            "project_id": s.team.projects["site"].id,
            "title": "Old upload",
            "file_name": "old.pdf",
            "pdf_base64": STANDARD.encode(sample_pdf(&["One", "Two", "Three"])),
            "signers": [{ "name": "Ada Lovelace", "email": "ada@example.test" }],
        });
        let created = s.admin.post(&s.path("requests"), &body).await;
        created.expect(StatusCode::CREATED);
        let id = created["id"].as_str().unwrap().to_owned();
        let signer = created["signers"][0].clone();
        let place = |pages: &[u32]| {
            let fields: Vec<Value> = pages
                .iter()
                .map(|page| {
                    let mut field = signature_field(&signer);
                    field["page"] = json!(page);
                    field
                })
                .collect();
            json!({ "fields": fields })
        };
        let fields_path = s.path(&format!("requests/{id}/fields"));
        s.admin.send(Method::PUT, &fields_path, Some(&place(&[1, 3]))).await.expect(StatusCode::OK);

        // A file that an older upload check let through: page 3's
        // resources aren't a dictionary.
        let mut doc = Document::load_mem(&sample_pdf(&["One", "Two", "Three"])).unwrap();
        let third = doc.get_pages()[&3];
        doc.get_dictionary_mut(third).unwrap().set("Resources", 7);
        let mut bad = Vec::new();
        doc.save_to(&mut bad).unwrap();
        let mut tx = s.app.app().platform().db().write().await.unwrap();
        sqlx::query("UPDATE signing_files SET bytes = ? WHERE kind = 'original'")
            .bind(bad)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let send = s.path(&format!("requests/{id}/send"));
        let refused = s.admin.post(&send, &json!({})).await;
        refused.expect(StatusCode::CONFLICT);
        let message = refused.body["error"]["message"].as_str().unwrap();
        assert!(message.contains("page 3"), "{message}");
        assert_eq!(s.app.mail.sent().len(), 0, "nothing mailed");

        s.admin.send(Method::PUT, &fields_path, Some(&place(&[1, 2]))).await.expect(StatusCode::OK);
        s.admin.post(&send, &json!({})).await.expect(StatusCode::OK);
    }

    #[tokio::test]
    async fn a_request_from_before_fields_completes_with_its_signatures() {
        let s = setup().await;
        let (id, tokens) = s.sent().await;
        let mut tx = s.app.app().platform().db().write().await.unwrap();
        sqlx::query("DELETE FROM signing_fields").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE signing_requests SET page_count = NULL")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let session = s.session(&tokens[0]).await;
        assert_eq!(session["fields"], json!([]));
        assert_eq!(session["request"]["page_count"], Value::Null);
        let refused = s
            .sign(
                &tokens[0],
                &json!({ "consent": true, "signature": typed("Ada"), "initials": typed("A") }),
            )
            .await;
        refused.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(field_errors(&refused), ["initials"]);
        for (token, name) in tokens.iter().zip(["Ada Lovelace", "Grace Hopper"]) {
            s.sign_as(token, name).await.expect(StatusCode::OK);
        }
        s.app.drain_jobs().await;
        assert_eq!(s.detail(&id).await["request"]["status"], "completed");
        let doc = s.signed_pdf(&id).await;
        assert_eq!(doc.get_pages().len(), 3);
        assert!(overlay_ops(&doc, 1).is_none(), "nothing drawn on the pages");
        let certificate = doc.get_dictionary(doc.get_pages()[&3]).unwrap();
        let content = doc.get_page_content(doc.get_pages()[&3]).unwrap();
        let ops = Content::decode(&content).unwrap().operations;
        let names: Vec<&[u8]> = ops
            .iter()
            .filter(|op| op.operator == "Tj")
            .filter_map(|op| op.operands[0].as_str().ok())
            .filter(|text| *text == b"Ada Lovelace" || *text == b"Grace Hopper")
            .collect();
        assert_eq!(
            names,
            [&b"Ada Lovelace"[..], b"Grace Hopper"],
            "the signatures print on the certificate"
        );
        assert!(matches!(certificate.get(b"Rotate"), Ok(Object::Integer(0))));
    }

    #[tokio::test]
    async fn a_broken_original_fails_after_three_tries_and_can_be_retried() {
        let s = setup().await;
        let (id, tokens) = s.sent().await;
        let mut tx = s.app.app().platform().db().write().await.unwrap();
        sqlx::query("UPDATE signing_files SET bytes = x'00' WHERE kind = 'original'")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        for token in &tokens {
            s.sign_as(token, "Someone").await.expect(StatusCode::OK);
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

    /// Creates a request to `signers` with a signature field each, and
    /// tries to send it.
    async fn create_and_send(s: &Setup, signers: &Value) -> ApiResponse {
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
        created.expect(StatusCode::CREATED);
        let id = created["id"].as_str().unwrap();
        let fields: Vec<Value> =
            created["signers"].as_array().unwrap().iter().map(signature_field).collect();
        s.admin
            .send(
                Method::PUT,
                &s.path(&format!("requests/{id}/fields")),
                Some(&json!({ "fields": fields })),
            )
            .await
            .expect(StatusCode::OK);
        s.admin.post(&s.path(&format!("requests/{id}/send")), &json!({})).await
    }

    fn ten_signers(group: impl Fn(u8) -> u8) -> Value {
        Value::Array(
            (0..10)
                .map(|n| {
                    json!({ "name": format!("Signer {n}"), "email": format!("s{n}@example.test"),
                            "signing_order": group(n) })
                })
                .collect(),
        )
    }

    #[tokio::test]
    async fn a_workspace_mails_at_most_100_signers_an_hour() {
        let s = setup().await;
        let signers = ten_signers(|_| 1);
        for _ in 0..10 {
            s.send(&signers).await;
        }
        create_and_send(&s, &signers).await.expect(StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn retries_keep_the_first_send_time_for_the_cap() {
        let s = setup().await;
        let signers = ten_signers(|_| 1);
        let mut ids = Vec::new();
        for _ in 0..10 {
            ids.push(s.send(&signers).await);
        }
        s.app.clock.advance(Duration::from_mins(61));
        let mut tx = s.app.app().platform().db().write().await.unwrap();
        sqlx::query("UPDATE signing_requests SET status = 'failed', failure = 'test'")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        for id in &ids {
            s.admin
                .post(&s.path(&format!("requests/{id}/retry")), &json!({}))
                .await
                .expect(StatusCode::OK);
        }
        s.send(&signers).await;
    }

    #[tokio::test]
    async fn later_groups_count_toward_the_cap_at_the_first_send() {
        let s = setup().await;
        // 10 requests of 10 signers in two groups mail only 50 at first, but
        // count 100.
        let signers = ten_signers(|n| 1 + n / 5);
        for _ in 0..10 {
            s.send(&signers).await;
        }
        assert_eq!(s.app.mail.sent().len(), 50, "only the first groups");
        create_and_send(&s, &signers).await.expect(StatusCode::TOO_MANY_REQUESTS);
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

        s.sign_as(&token(&first[0].text), "Ada Lovelace").await.expect(StatusCode::OK);
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

        let token_to =
            |address: &str| token(&second.iter().find(|m| m.to == address).unwrap().text);
        s.sign_as(&token_to("grace@example.test"), "Grace Hopper").await.expect(StatusCode::OK);
        s.app.drain_jobs().await;
        assert!(s.app.mail.take().is_empty(), "Alan's group is still signing");
        s.sign_as(&token_to("alan@example.test"), "Alan Turing").await.expect(StatusCode::OK);
        s.app.drain_jobs().await;
        assert_eq!(s.detail(&id).await["request"]["status"], "completed");
        let doc = s.signed_pdf(&id).await;
        let text = doc.extract_text(&[3]).unwrap();
        assert!(text.contains("Signing order 5"), "{text}");
    }
}
