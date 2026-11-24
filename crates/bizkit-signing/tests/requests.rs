//! Signature requests through the API: create, list, read, send, void and
//! delete, and their PDFs.

#[cfg(test)]
mod requests {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use bizkit_signing::Signing;
    use bizkit_signing::testing::sample_pdf;
    use bizkit_testkit::{Manifest, Persona, SpecClient, TestApp};
    use http::StatusCode;
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
        let id = s.create().await["id"].as_str().unwrap().to_owned();
        let sent = s.member.post(&s.path(&format!("requests/{id}/send")), &json!({})).await;
        sent.expect(StatusCode::OK);
        assert_eq!(sent["status"], "sent");
        s.app.drain_jobs().await;
        let mails = s.app.mail.sent();
        assert_eq!(mails.len(), 2);
        let ada = mails.iter().find(|mail| mail.to == "ada@example.test").unwrap();
        assert_eq!(ada.subject, "Please sign: Services agreement");
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
        let id = s.create().await["id"].as_str().unwrap().to_owned();
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
}
