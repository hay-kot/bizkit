//! The feature modules compiled into this binary.

use bizkit_core::Module;

/// Every compiled-in module, in registration order. Cargo features on this
/// crate decide which modules exist.
#[cfg_attr(
    any(
        feature = "forms",
        feature = "webhooks",
        feature = "extensions",
        feature = "analytics",
        feature = "mailing",
        feature = "signing"
    ),
    expect(clippy::vec_init_then_push, reason = "each push sits behind its module's feature")
)]
pub(crate) fn modules() -> Vec<Box<dyn Module>> {
    let mut modules: Vec<Box<dyn Module>> = Vec::new();
    #[cfg(feature = "forms")]
    modules.push(Box::new(bizkit_forms::Forms));
    // Before extensions: its demo seed binds scripts to the seeded endpoints.
    #[cfg(feature = "webhooks")]
    modules.push(Box::new(bizkit_webhooks::Webhooks));
    // Before extensions: its demo seed binds scripts to the seeded goals.
    #[cfg(feature = "analytics")]
    modules.push(Box::new(bizkit_analytics::Analytics::default()));
    // After forms: its demo seed links a list to the seeded Contact form.
    #[cfg(feature = "mailing")]
    modules.push(Box::new(bizkit_mailing::Mailing));
    #[cfg(feature = "signing")]
    modules.push(Box::new(bizkit_signing::Signing));
    #[cfg(feature = "extensions")]
    modules.push(Box::new(bizkit_extensions::Extensions::default()));
    modules
}

/// Checks that hold for every operation of every compiled-in module.
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use bizkit_core::ops::{Access, Method as OpMethod, OpMeta};
    use bizkit_testkit::{Persona, SpecClient, TestApp, TestAppBuilder};
    use http::{Method, StatusCode};
    use serde_json::json;

    use super::*;

    async fn app() -> TestApp {
        modules().into_iter().fold(TestApp::builder(), TestAppBuilder::boxed_module).build().await
    }

    fn http_method(method: OpMethod) -> Method {
        match method {
            OpMethod::Get => Method::GET,
            OpMethod::Post => Method::POST,
            OpMethod::Put => Method::PUT,
            OpMethod::Patch => Method::PATCH,
            OpMethod::Delete => Method::DELETE,
        }
    }

    /// The op's path with `{workspace_id}` filled in and random IDs elsewhere.
    fn path(op: &OpMeta, workspace_id: &str) -> String {
        let filled = op
            .path
            .split('/')
            .map(|seg| match seg {
                "{workspace_id}" => workspace_id.to_owned(),
                param if param.starts_with('{') => uuid::Uuid::now_v7().to_string(),
                other => other.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("/");
        format!("/api/v1{filled}")
    }

    async fn call(client: &SpecClient, op: &OpMeta, workspace_id: &str) -> StatusCode {
        let body = (!op.method.uses_query()).then(|| json!({}));
        client.send(http_method(op.method), &path(op, workspace_id), body.as_ref()).await.status
    }

    #[tokio::test]
    async fn every_operation_enforces_its_access() {
        let app = app().await;
        let team = app.seed(Persona::Team, "sweep").await;
        let outsider = app.seed(Persona::Basic, "outsider").await;
        let ws = team.workspaces["main"].id;
        let anonymous = app.client();
        let full_key = app.with_key(&team.api_keys["full"].token);
        let no_scopes = app.with_key(&app.insert_api_key(ws, &[]).await);
        let other_user = app.with_session(&outsider.accounts["admin"].session_token);
        let ws = ws.to_string();

        for op in app.app().registry().ops() {
            match &op.access {
                Access::Public => {}
                Access::User => {
                    assert_eq!(
                        call(&anonymous, op, &ws).await,
                        StatusCode::UNAUTHORIZED,
                        "{}: anonymous",
                        op.id
                    );
                    assert_eq!(
                        call(&full_key, op, &ws).await,
                        StatusCode::UNAUTHORIZED,
                        "{}: API key",
                        op.id
                    );
                }
                Access::Workspace(_) => {
                    assert_eq!(
                        call(&anonymous, op, &ws).await,
                        StatusCode::UNAUTHORIZED,
                        "{}: anonymous",
                        op.id
                    );
                    assert_eq!(
                        call(&no_scopes, op, &ws).await,
                        StatusCode::FORBIDDEN,
                        "{}: key without scopes",
                        op.id
                    );
                    assert_eq!(
                        call(&other_user, op, &ws).await,
                        StatusCode::NOT_FOUND,
                        "{}: outsider",
                        op.id
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn tools_list_for_a_full_key() {
        let app = app().await;
        let team = app.seed(Persona::Team, "tools").await;
        let tools = app.mcp(&team.api_keys["full"].token).list_tools().await;
        insta::assert_json_snapshot!(tools);
    }

    #[test]
    fn migration_versions_are_unique() {
        let mut seen = BTreeSet::new();
        // Analytics' per-project files have their own set; the sqlx prepare
        // database applies it into the same table as the app's.
        #[cfg_attr(not(feature = "analytics"), expect(unused_mut, reason = "no extra sets"))]
        let mut migrators = bizkit_core::migrators(&modules());
        #[cfg(feature = "analytics")]
        migrators.push(bizkit_analytics::project_migrator());
        for migrator in migrators {
            for migration in migrator.iter() {
                assert!(
                    seen.insert(migration.version),
                    "migration version {} appears twice",
                    migration.version
                );
            }
        }
        assert!(!seen.is_empty(), "core has migrations");
    }
}
