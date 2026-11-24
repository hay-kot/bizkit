//! Core's usage reports: which actions publish `UsageReported`, for whom.

#[cfg(test)]
mod usage {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use bizkit_core::jobs::{JobCtx, Subscriber};
    use bizkit_core::usage::{UsageKind, UsageReported};
    use bizkit_core::{Module, ModuleContext};
    use bizkit_testkit::{Persona, TestApp};
    use http::StatusCode;
    use serde_json::json;

    #[derive(Debug, Clone, Default)]
    struct Seen(Arc<Mutex<Vec<UsageReported>>>);

    impl Seen {
        fn take(&self) -> Vec<UsageReported> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    impl Subscriber<UsageReported> for Seen {
        const NAME: &'static str = "usagetest";

        fn handle(
            &self,
            _cx: JobCtx,
            event: UsageReported,
        ) -> impl Future<Output = anyhow::Result<()>> + Send {
            self.0.lock().unwrap().push(event);
            std::future::ready(Ok(()))
        }
    }

    #[derive(Debug, Default)]
    struct UsageTest(Seen);

    impl Module for UsageTest {
        fn name(&self) -> &'static str {
            "usagetest"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.subscribe::<UsageReported, _>(self.0.clone())
        }
    }

    async fn app(seen: &Seen) -> TestApp {
        TestApp::builder().module(UsageTest(seen.clone())).build().await
    }

    fn kinds(events: &[UsageReported]) -> Vec<UsageKind> {
        events.iter().map(|event| event.kind).collect()
    }

    #[tokio::test]
    async fn sign_up_then_sign_in() {
        let seen = Seen::default();
        let app = app(&seen).await;
        let team = app.seed(Persona::Team, "u1").await;
        let email = &team.invitations["invitee"].email;
        app.sign_in(email).await;
        app.clock.advance(Duration::from_secs(61));
        app.sign_in(email).await;
        app.drain_jobs().await;
        let events = seen.take();
        assert_eq!(kinds(&events), [UsageKind::SignedUp, UsageKind::SignedIn]);
        assert_eq!(events[0].user_id, events[1].user_id, "one person");
        assert_eq!(events[0].workspace_id, None, "sign-in has no workspace");
    }

    #[tokio::test]
    async fn workspace_actions_report_once_each() {
        let seen = Seen::default();
        let app = app(&seen).await;
        let team = app.seed(Persona::Team, "u2").await;
        let admin_id = team.accounts["admin"].id;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let ws = team.workspaces["main"].id;

        let created =
            admin.post("/api/v1/workspaces", &json!({ "name": "Two", "slug": "two-u2" })).await;
        created.expect(StatusCode::CREATED);
        admin
            .post(
                &format!("/api/v1/workspaces/{ws}/projects"),
                &json!({ "name": "Pages", "slug": "pages" }),
            )
            .await
            .expect(StatusCode::CREATED);
        admin
            .post(
                &format!("/api/v1/workspaces/{ws}/invitations"),
                &json!({ "email": "new+u2@e2e.test", "role": "member" }),
            )
            .await
            .expect(StatusCode::CREATED);
        let key = admin
            .post(
                &format!("/api/v1/workspaces/{ws}/api-keys"),
                &json!({ "name": "ci", "scopes": ["projects:read"] }),
            )
            .await;
        key.expect(StatusCode::CREATED);
        let key_id = key["key"]["id"].as_str().unwrap().to_owned();
        let revoke = format!("/api/v1/workspaces/{ws}/api-keys/{key_id}");
        admin.delete(&revoke).await.expect(StatusCode::NO_CONTENT);
        admin.delete(&revoke).await.expect(StatusCode::NOT_FOUND);
        app.drain_jobs().await;

        let events = seen.take();
        assert_eq!(
            kinds(&events),
            [
                UsageKind::WorkspaceCreated,
                UsageKind::ProjectCreated,
                UsageKind::InvitationSent,
                UsageKind::ApiKeyCreated,
                UsageKind::ApiKeyRevoked,
            ],
            "a second revoke of the same key reports nothing"
        );
        assert!(events.iter().all(|event| event.user_id == admin_id), "the admin acted");
        let new_ws = created["id"].as_str().unwrap();
        assert_eq!(events[0].workspace_id.map(|id| id.to_string()).as_deref(), Some(new_ws));
        assert!(events[1..].iter().all(|event| event.workspace_id == Some(ws)), "{events:?}");
    }

    #[tokio::test]
    async fn accepting_an_invitation_reports_a_join() {
        let seen = Seen::default();
        let app = app(&seen).await;
        let team = app.seed(Persona::Team, "u3").await;
        let invitation = &team.invitations["invitee"];
        let invitee = app.sign_in(&invitation.email).await;
        invitee
            .post(&format!("/api/v1/me/invitations/{}/accept", invitation.id), &json!({}))
            .await
            .expect(StatusCode::OK);
        app.drain_jobs().await;
        let events = seen.take();
        assert_eq!(kinds(&events), [UsageKind::SignedUp, UsageKind::MemberJoined]);
        assert_eq!(events[1].workspace_id, Some(team.workspaces["main"].id));
    }

    #[tokio::test]
    async fn api_key_actions_report_nothing() {
        let seen = Seen::default();
        let app = app(&seen).await;
        let team = app.seed(Persona::Team, "u4").await;
        let ws = team.workspaces["main"].id;
        let key = app.with_key(&team.api_keys["full"].token);
        key.post(
            &format!("/api/v1/workspaces/{ws}/projects"),
            &json!({ "name": "Keyed", "slug": "keyed" }),
        )
        .await
        .expect(StatusCode::CREATED);
        app.drain_jobs().await;
        assert_eq!(seen.take(), [], "no person to count");
    }
}
