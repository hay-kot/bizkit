//! Signature requests through the API: create, list, read, edit, send, void
//! and delete, and their PDFs.

#[cfg(test)]
mod requests {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use bizkit_signing::Signing;
    use bizkit_signing::testing::{sample_pdf, sample_pdf_padded};
    use bizkit_testkit::{ApiResponse, Manifest, Persona, RawBody, SpecClient, TestApp};
    use http::{Method, StatusCode, header};
    use serde_json::{Value, json};

    struct Setup {
        app: TestApp,
        team: Manifest,
        admin: SpecClient,
        member: SpecClient,
    }

    async fn setup() -> Setup {
        let app = TestApp::builder().module(Signing).build().await;
        let team = app.seed(Persona::Team, "sign").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let member = app.with_session(&team.accounts["member"].session_token);
        Setup { app, team, admin, member }
    }

    impl Setup {
        fn path(&self, rest: &str) -> String {
            format!("/api/v1/workspaces/{}/signing/{rest}", self.team.workspaces["main"].id)
        }

        fn body(&self, pdf: &[u8]) -> Value {
            json!({
                "project_id": self.team.projects["site"].id,
                "title": "Services agreement",
                "message": "Please sign by Friday.",
                "file_name": "agreement.pdf",
                "pdf_base64": STANDARD.encode(pdf),
                "signers": [
                    { "name": "Ada Lovelace", "email": "Ada@Example.test" },
                    { "name": "Grace Hopper", "email": "grace@example.test" },
                ],
            })
        }

        async fn create(&self) -> Value {
            let created =
                self.member.post(&self.path("requests"), &self.body(&sample_pdf(&["Terms"]))).await;
            created.expect(StatusCode::CREATED);
            created.json()
        }

        async fn put(&self, id: &str, what: &str, body: &Value) -> ApiResponse {
            let path = self.path(&format!("requests/{id}/{what}"));
            self.member.send(Method::PUT, &path, Some(body)).await
        }

        /// Gives every signer of `request` a signature field on page 1.
        async fn place(&self, request: &Value) -> ApiResponse {
            let fields: Vec<Value> = request["signers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|signer| signature_field(&signer["id"], 1))
                .collect();
            self.put(request["id"].as_str().unwrap(), "fields", &json!({ "fields": fields })).await
        }

        async fn upload(&self, client: &SpecClient, meta: &Value, pdf: &[u8]) -> ApiResponse {
            let path = format!(
                "/m/signing/upload/workspaces/{}/requests",
                self.team.workspaces["main"].id
            );
            client.send_body(Method::POST, &path, multipart(meta, pdf)).await
        }

        fn file_path(&self, id: &str, version: &str) -> String {
            format!(
                "/m/signing/workspaces/{}/requests/{id}/{version}.pdf",
                self.team.workspaces["main"].id
            )
        }
    }

    fn signature_field(signer_id: &Value, page: u32) -> Value {
        json!({
            "signer_id": signer_id, "kind": "signature", "page": page,
            "x": 0.1, "y": 0.7, "width": 0.3, "height": 0.08,
        })
    }

    const BOUNDARY: &str = "signing-boundary";

    fn multipart(meta: &Value, pdf: &[u8]) -> RawBody<'static> {
        let mut body = format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"meta\"\r\nContent-Type: application/json\r\n\r\n{meta}\r\n\
             --{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"scan.pdf\"\r\nContent-Type: application/pdf\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(pdf);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
        RawBody::new("multipart/form-data; boundary=signing-boundary", body)
    }

    #[tokio::test]
    async fn a_draft_keeps_its_pdf_and_signers() {
        let s = setup().await;
        let pdf = sample_pdf(&["Terms", "More terms"]);
        let created = s.member.post(&s.path("requests"), &s.body(&pdf)).await;
        created.expect(StatusCode::CREATED);
        assert_eq!(created["status"], "draft");
        assert_eq!(created["signers"][0]["email"], "ada@example.test", "emails are normalized");
        assert_eq!(created["signers"][1]["signed"], false);
        let id = created["id"].as_str().unwrap().to_owned();

        let listed = s
            .member
            .get(&format!("{}?project_id={}", s.path("requests"), s.team.projects["site"].id))
            .await;
        listed.expect(StatusCode::OK);
        assert_eq!(listed["items"][0]["id"], id.as_str());

        let detail = s.member.get(&s.path(&format!("requests/{id}"))).await;
        detail.expect(StatusCode::OK);
        assert_eq!(detail["events"][0]["kind"], "created");

        let doc = s.member.get(&s.path(&format!("requests/{id}/document"))).await;
        doc.expect(StatusCode::OK);
        assert_eq!(STANDARD.decode(doc["pdf_base64"].as_str().unwrap()).unwrap(), pdf);
        assert_eq!(doc["sha256"], created["original_sha256"]);
        s.member
            .get(&s.path(&format!("requests/{id}/document?version=signed")))
            .await
            .expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn uploads_must_be_readable_pdfs() {
        let s = setup().await;
        let cases: [(&[u8], &str); 2] =
            [(b"not a pdf", "must be a PDF"), (b"%PDF-1.7 garbage", "must be a PDF that opens")];
        for (bytes, message) in cases {
            let refused = s.member.post(&s.path("requests"), &s.body(bytes)).await;
            refused.expect(StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(refused["error"]["fields"]["pdf_base64"][0], message);
        }
        let mut body = s.body(&sample_pdf(&["x"]));
        body["signers"] = json!([
            { "name": "A", "email": "same@example.test" },
            { "name": "B", "email": "SAME@example.test" },
        ]);
        let duplicate = s.member.post(&s.path("requests"), &body).await;
        duplicate.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(duplicate["error"]["fields"]["signers[1].email"][0], "is already a signer");
    }

    #[tokio::test]
    async fn sending_mails_each_signer_a_private_link_once() {
        let s = setup().await;
        let created = s.create().await;
        let id = created["id"].as_str().unwrap().to_owned();
        s.place(&created).await.expect(StatusCode::OK);
        let sent = s.member.post(&s.path(&format!("requests/{id}/send")), &json!({})).await;
        sent.expect(StatusCode::OK);
        assert_eq!(sent["status"], "sent");
        s.app.drain_jobs().await;
        let mails = s.app.mail.sent();
        assert_eq!(mails.len(), 2);
        let ada = mails.iter().find(|mail| mail.to == "ada@example.test").unwrap();
        assert_eq!(ada.subject, "Please sign: Services agreement");
        assert_eq!(ada.workspace_id, Some(s.team.workspaces["main"].id), "workspace mail");
        assert!(ada.text.contains("Please sign by Friday."), "{}", ada.text);
        let link = ada
            .text
            .split_whitespace()
            .find(|word| word.contains("/m/signing/public/sign?t="))
            .unwrap();
        assert!(link.len() > "/m/signing/public/sign?t=".len() + 40, "a link with a token: {link}");

        s.member
            .post(&s.path(&format!("requests/{id}/send")), &json!({}))
            .await
            .expect(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn void_and_delete_follow_the_status() {
        let s = setup().await;
        let created = s.create().await;
        let id = created["id"].as_str().unwrap().to_owned();
        s.place(&created).await.expect(StatusCode::OK);
        let one = s.path(&format!("requests/{id}"));
        s.admin.post(&format!("{one}/send"), &json!({})).await.expect(StatusCode::OK);
        s.admin.delete(&one).await.expect(StatusCode::CONFLICT);
        let voided = s.member.post(&format!("{one}/void"), &json!({})).await;
        voided.expect(StatusCode::OK);
        assert_eq!(voided["status"], "voided");
        s.member.post(&format!("{one}/void"), &json!({})).await.expect(StatusCode::CONFLICT);
        s.member.delete(&one).await.expect(StatusCode::FORBIDDEN);
        s.admin.delete(&one).await.expect(StatusCode::NO_CONTENT);
        s.admin.get(&one).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn other_workspaces_see_nothing() {
        let s = setup().await;
        let id = s.create().await["id"].as_str().unwrap().to_owned();
        let other = s.app.seed(Persona::Basic, "sign2").await;
        let outsider = s.app.with_session(&other.accounts["admin"].session_token);
        let theirs =
            format!("/api/v1/workspaces/{}/signing/requests/{id}", other.workspaces["main"].id);
        outsider.get(&theirs).await.expect(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn signing_order_defaults_to_one_and_stays_in_range() {
        let s = setup().await;
        let created = s.create().await;
        assert_eq!(created["signers"][0]["signing_order"], 1);
        assert_eq!(created["page_count"], 1);
        let mut body = s.body(&sample_pdf(&["x"]));
        body["signers"][0]["signing_order"] = json!(0);
        body["signers"][1]["signing_order"] = json!(11);
        let refused = s.member.post(&s.path("requests"), &body).await;
        refused.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(refused["error"]["fields"]["signers[0].signing_order"][0], "must be 1 to 10");
        assert_eq!(refused["error"]["fields"]["signers[1].signing_order"][0], "must be 1 to 10");
    }

    #[tokio::test]
    async fn signers_put_keeps_drops_and_reorders_signers() {
        let s = setup().await;
        let created = s.create().await;
        let id = created["id"].as_str().unwrap().to_owned();
        let (ada, grace) =
            (created["signers"][0]["id"].clone(), created["signers"][1]["id"].clone());
        s.place(&created).await.expect(StatusCode::OK);

        let swapped = s
            .put(&id, "signers", &json!({ "signers": [
                { "id": grace, "name": "Grace Hopper", "email": "grace@example.test", "signing_order": 2 },
                { "id": ada, "name": "Ada King", "email": "ada@example.test" },
            ]}))
            .await;
        swapped.expect(StatusCode::OK);
        let signers = &swapped["request"]["signers"];
        assert_eq!(signers[0]["id"], grace, "positions swap");
        assert_eq!(signers[0]["signing_order"], 2);
        assert_eq!(signers[1]["name"], "Ada King");
        assert_eq!(swapped["fields"].as_array().unwrap().len(), 2, "both keep their fields");

        let replaced = s
            .put(
                &id,
                "signers",
                &json!({ "signers": [
                    { "id": grace, "name": "Grace Hopper", "email": "grace@example.test" },
                    { "name": "Alan Turing", "email": "alan@example.test", "signing_order": 3 },
                ]}),
            )
            .await;
        replaced.expect(StatusCode::OK);
        let after = replaced["request"]["signers"].as_array().unwrap();
        assert_eq!(after.len(), 2);
        assert_eq!(after[1]["name"], "Alan Turing");
        let fields = replaced["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1, "Ada's field went with her");
        assert_eq!(fields[0]["signer_id"], grace);

        let unknown = s
            .put(
                &id,
                "signers",
                &json!({ "signers": [
                    { "id": ada, "name": "Ada", "email": "ada@example.test" },
                ]}),
            )
            .await;
        unknown.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            unknown["error"]["fields"]["signers[0].id"][0],
            "is not a signer of this request"
        );
        s.put(&id, "signers", &json!({ "signers": [] }))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn fields_put_checks_signers_pages_and_rects() {
        let s = setup().await;
        let created = s.create().await;
        let id = created["id"].as_str().unwrap().to_owned();
        let ada = created["signers"][0]["id"].clone();
        let other = s.create().await["signers"][0]["id"].clone();
        let refused = s
            .put(&id, "fields", &json!({ "fields": [
                { "signer_id": ada, "kind": "text", "page": 1, "x": 0.9, "y": 0.1, "width": 0.2, "height": 0.05 },
                { "signer_id": ada, "kind": "date", "page": 1, "x": 0.1, "y": 0.1, "width": 0.2,
                  "height": 0.05, "required": false },
            ]}))
            .await;
        refused.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let errors = &refused["error"]["fields"];
        assert_eq!(errors["fields[0].width"][0], "must keep the field on the page", "{errors}");
        assert_eq!(errors["fields[1].required"][0], "must be true for this kind", "{errors}");
        let wrong = s
            .put(
                &id,
                "fields",
                &json!({ "fields": [
                    signature_field(&ada, 2),
                    signature_field(&other, 1),
                ]}),
            )
            .await;
        wrong.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let mismatched = &wrong["error"]["fields"];
        assert_eq!(mismatched["fields[0].page"][0], "must be at most 1", "{mismatched}");
        assert_eq!(
            mismatched["fields[1].signer_id"][0], "is not a signer of this request",
            "{mismatched}"
        );
        let many: Vec<Value> = (0..201).map(|_| signature_field(&ada, 1)).collect();
        s.put(&id, "fields", &json!({ "fields": many }))
            .await
            .expect(StatusCode::UNPROCESSABLE_ENTITY);

        let placed = s
            .put(&id, "fields", &json!({ "fields": [
                signature_field(&ada, 1),
                { "signer_id": ada, "kind": "checkbox", "page": 1, "x": 0.1, "y": 0.5, "width": 0.03,
                  "height": 0.03, "required": false, "label": "Send me updates" },
            ]}))
            .await;
        placed.expect(StatusCode::OK);
        let fields = placed["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[1]["label"], "Send me updates");
        assert_eq!(fields[1]["required"], false);
        assert_eq!(fields[0]["value"], Value::Null);

        let replaced = s.put(&id, "fields", &json!({ "fields": [] })).await;
        replaced.expect(StatusCode::OK);
        assert_eq!(replaced["fields"], json!([]), "the list replaces the old one");
        let detail = s.member.get(&s.path(&format!("requests/{id}"))).await;
        assert_eq!(detail["fields"], json!([]));
    }

    #[tokio::test]
    async fn only_drafts_change_and_send_needs_a_signature_field_per_signer() {
        let s = setup().await;
        let created = s.create().await;
        let id = created["id"].as_str().unwrap().to_owned();
        let send = s.path(&format!("requests/{id}/send"));
        let refused = s.member.post(&send, &json!({})).await;
        refused.expect(StatusCode::CONFLICT);
        assert!(
            refused["error"]["message"].as_str().unwrap().contains("Ada Lovelace, Grace Hopper"),
            "{}",
            refused.text
        );
        let ada = created["signers"][0]["id"].clone();
        let grace = created["signers"][1]["id"].clone();
        let date_only = json!({ "fields": [
            signature_field(&ada, 1),
            { "signer_id": grace, "kind": "date", "page": 1, "x": 0.5, "y": 0.8, "width": 0.2, "height": 0.04 },
        ]});
        s.put(&id, "fields", &date_only).await.expect(StatusCode::OK);
        let grace_only = s.member.post(&send, &json!({})).await;
        grace_only.expect(StatusCode::CONFLICT);
        assert!(grace_only["error"]["message"].as_str().unwrap().ends_with("Grace Hopper"));

        s.place(&created).await.expect(StatusCode::OK);
        s.member.post(&send, &json!({})).await.expect(StatusCode::OK);
        s.put(&id, "fields", &json!({ "fields": [] })).await.expect(StatusCode::CONFLICT);
        s.put(
            &id,
            "signers",
            &json!({ "signers": [
                { "id": ada, "name": "Ada", "email": "ada@example.test" },
            ]}),
        )
        .await
        .expect(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn fields_put_reads_the_page_count_of_older_requests() {
        let s = setup().await;
        let pdf = sample_pdf(&["One", "Two", "Three"]);
        let created = s.member.post(&s.path("requests"), &s.body(&pdf)).await;
        created.expect(StatusCode::CREATED);
        let id = created["id"].as_str().unwrap().to_owned();
        let mut tx = s.app.app().platform().db().write().await.unwrap();
        sqlx::query("UPDATE signing_requests SET page_count = NULL")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let before = s.member.get(&s.path(&format!("requests/{id}"))).await;
        assert_eq!(before["request"]["page_count"], Value::Null);

        let placed = s
            .put(
                &id,
                "fields",
                &json!({ "fields": [signature_field(&created["signers"][0]["id"], 3)] }),
            )
            .await;
        placed.expect(StatusCode::OK);
        assert_eq!(placed["request"]["page_count"], 3);
    }

    #[tokio::test]
    async fn uploads_create_drafts_from_files_over_the_json_limit() {
        let s = setup().await;
        let meta = json!({
            "project_id": s.team.projects["site"].id,
            "title": "Scanned lease",
            "signers": [{ "name": "Ada Lovelace", "email": "ada@example.test", "signing_order": 2 }],
        });
        let pdf = sample_pdf_padded(&["Lease", "Annex"], 1536 * 1024);
        let created = s.upload(&s.member, &meta, &pdf).await;
        created.expect(StatusCode::CREATED);
        assert_eq!(created["status"], "draft");
        assert_eq!(created["file_name"], "scan.pdf");
        assert_eq!(created["page_count"], 2);
        assert_eq!(created["signers"][0]["signing_order"], 2);
        let id = created["id"].as_str().unwrap();

        let file = s.member.get(&s.file_path(id, "original")).await;
        file.expect(StatusCode::OK);
        assert_eq!(file.headers[header::CONTENT_TYPE], "application/pdf");
        assert_eq!(file.headers[header::CONTENT_DISPOSITION], "inline; filename=\"scan.pdf\"");
        assert_eq!(file.headers[header::CONTENT_LENGTH], pdf.len().to_string().as_str());

        let bad = s.upload(&s.member, &meta, b"not a pdf").await;
        bad.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(bad["error"]["fields"]["file"][0], "must be a PDF");
        let bad_meta = s.upload(&s.member, &json!({ "title": "x" }), &pdf).await;
        bad_meta.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let message = bad_meta["error"]["fields"]["meta"][0].as_str();
        assert!(message.unwrap_or_default().starts_with("must be the draft"), "{}", bad_meta.text);

        let reader = s
            .admin
            .post(
                &format!("/api/v1/workspaces/{}/api-keys", s.team.workspaces["main"].id),
                &json!({ "name": "read", "scopes": ["signing:read"] }),
            )
            .await;
        let reader = s.app.with_key(reader["token"].as_str().unwrap());
        s.upload(&reader, &meta, &pdf).await.expect(StatusCode::FORBIDDEN);
        s.upload(&s.app.client(), &meta, &pdf).await.expect(StatusCode::UNAUTHORIZED);
        let full = s.app.with_key(&s.team.api_keys["full"].token);
        s.upload(&full, &meta, &pdf).await.expect(StatusCode::CREATED);
    }

    #[tokio::test]
    async fn uploads_take_one_meta_and_one_file() {
        let s = setup().await;
        let path =
            format!("/m/signing/upload/workspaces/{}/requests", s.team.workspaces["main"].id);
        let part = |head: &str, body: &str| {
            format!("--{BOUNDARY}\r\nContent-Disposition: form-data; {head}\r\n\r\n{body}\r\n")
        };
        let meta = json!({
            "project_id": s.team.projects["site"].id,
            "title": "Lease",
            "signers": [{ "name": "Ada Lovelace", "email": "ada@example.test" }],
        })
        .to_string();
        let body = [
            part("name=\"meta\"", &meta),
            part("name=\"meta\"", &meta),
            part("name=\"extra\"", "x"),
            part("filename=\"a.txt\"", "x"),
            part("name=\"meta2\"", &"x".repeat(10)),
            format!("--{BOUNDARY}--\r\n"),
        ]
        .concat();
        let raw = RawBody::new("multipart/form-data; boundary=signing-boundary", body);
        let refused = s.member.send_body(Method::POST, &path, raw).await;
        refused.expect(StatusCode::UNPROCESSABLE_ENTITY);
        let errors = &refused["error"]["fields"];
        assert_eq!(errors["meta"][0], "must be given once", "{errors}");
        assert_eq!(errors["file"][0], "is required", "{errors}");
        assert_eq!(errors["parts"].as_array().unwrap().len(), 3, "{errors}");

        let big = part("name=\"meta\"", &" ".repeat(64 * 1024 + 1));
        let oversized = RawBody::new(
            "multipart/form-data; boundary=signing-boundary",
            format!("{big}--{BOUNDARY}--\r\n"),
        );
        let too_big = s.member.send_body(Method::POST, &path, oversized).await;
        too_big.expect(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(too_big["error"]["fields"]["meta"][0], "must be at most 64 KiB");
    }

    #[tokio::test]
    async fn an_unreadable_older_file_gives_a_conflict() {
        let s = setup().await;
        let created = s.create().await;
        let id = created["id"].as_str().unwrap().to_owned();
        let mut tx = s.app.app().platform().db().write().await.unwrap();
        sqlx::query("UPDATE signing_requests SET page_count = NULL")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE signing_files SET bytes = x'00'").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let refused = s
            .put(
                &id,
                "fields",
                &json!({ "fields": [signature_field(&created["signers"][0]["id"], 1)] }),
            )
            .await;
        refused.expect(StatusCode::CONFLICT);
        assert!(
            refused["error"]["message"].as_str().unwrap().contains("count its pages"),
            "{}",
            refused.text
        );
    }

    #[tokio::test]
    async fn files_over_8_mib_come_from_the_file_route_only() {
        let s = setup().await;
        let meta = json!({
            "project_id": s.team.projects["site"].id,
            "title": "Big scan",
            "signers": [{ "name": "Ada Lovelace", "email": "ada@example.test" }],
        });
        let created =
            s.upload(&s.member, &meta, &sample_pdf_padded(&["Big"], 9 * 1024 * 1024)).await;
        created.expect(StatusCode::CREATED);
        let id = created["id"].as_str().unwrap();
        let refused = s.member.get(&s.path(&format!("requests/{id}/document"))).await;
        refused.expect(StatusCode::PAYLOAD_TOO_LARGE);
        let message = refused["error"]["message"].as_str().unwrap();
        assert!(message.contains(&s.file_path(id, "original")), "{message}");
        s.member.get(&s.file_path(id, "original")).await.expect(StatusCode::OK);
    }

    #[tokio::test]
    async fn the_file_route_checks_access_and_the_version() {
        let s = setup().await;
        let id = s.create().await["id"].as_str().unwrap().to_owned();
        s.member.get(&s.file_path(&id, "original")).await.expect(StatusCode::OK);
        s.member.get(&s.file_path(&id, "signed")).await.expect(StatusCode::NOT_FOUND);
        s.app.client().get(&s.file_path(&id, "original")).await.expect(StatusCode::UNAUTHORIZED);
        let other = s.app.seed(Persona::Basic, "sign2").await;
        let outsider = s.app.with_session(&other.accounts["admin"].session_token);
        outsider.get(&s.file_path(&id, "original")).await.expect(StatusCode::NOT_FOUND);
        let unknown = s.file_path("not-an-id", "original");
        s.member.get(&unknown).await.expect(StatusCode::NOT_FOUND);
    }
}
