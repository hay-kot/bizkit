//! Turning a feature off in a workspace refuses its operations, MCP tools and
//! raw routes there, and turning it on again brings them back.

#[cfg(test)]
mod features {
    use axum::extract::Path;
    use axum::routing::get;
    use axum::{Extension, Router};
    use bizkit_core::ids::WorkspaceId;
    use bizkit_core::ops::{Access, Level, Method as OpMethod, OpCtx, Operation, Scope};
    use bizkit_core::validate::Validate;
    use bizkit_core::{ApiError, Authn, Module, ModuleContext, Platform};
    use bizkit_testkit::{Persona, TestApp};
    use http::{Method, StatusCode};
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use utoipa::{OpenApi, ToSchema};

    #[derive(Debug, Deserialize, ToSchema)]
    #[serde(deny_unknown_fields)]
    struct PingInput {
        workspace_id: WorkspaceId,
    }
    impl Validate for PingInput {}

    #[derive(Debug, Serialize, ToSchema)]
    struct Pong {
        workspace_id: WorkspaceId,
    }

    struct Ping;
    impl Operation for Ping {
        type Input = PingInput;
        type Output = Pong;
        const ID: &'static str = "probe.ping";
        const METHOD: OpMethod = OpMethod::Get;
        const PATH: &'static str = "/workspaces/{workspace_id}/probe";
        const ACCESS: Access = Access::Workspace(Scope::new("probe", Level::Read));
        const SUMMARY: &'static str = "Ping";
        const TAGS: &'static [&'static str] = &["probe"];

        fn call(
            &self,
            _cx: OpCtx,
            input: PingInput,
        ) -> impl Future<Output = Result<Pong, ApiError>> + Send {
            std::future::ready(Ok(Pong { workspace_id: input.workspace_id }))
        }
    }

    async fn raw(
        Extension(platform): Extension<Platform>,
        Extension(authn): Extension<Authn>,
        Path(workspace_id): Path<String>,
    ) -> Result<&'static str, ApiError> {
        let scope = Scope::new("probe", Level::Read);
        bizkit_core::ops::authorize_workspace(
            &platform,
            authn.principal.as_ref(),
            &workspace_id,
            &scope,
        )
        .await?;
        Ok("pong")
    }

    #[utoipa::path(get, path = "/workspaces/{workspace_id}/ping", params(("workspace_id" = String, Path)), responses((status = 200, body = String, content_type = "text/plain"), (status = 403, description = "Off", body = bizkit_core::error::ErrorBody)))]
    #[expect(dead_code, reason = "exists for its utoipa docs")]
    fn raw_docs() {}

    #[derive(OpenApi)]
    #[openapi(paths(raw_docs))]
    struct RawDocs;

    #[derive(Debug)]
    struct Probe;

    impl Module for Probe {
        fn name(&self) -> &'static str {
            "probe"
        }

        fn register(&self, cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
            cx.scope_area("probe", "A probe feature")?;
            cx.op(Ping)?;
            cx.routes(
                Router::new().route("/workspaces/{workspace_id}/ping", get(raw)),
                RawDocs::openapi(),
            )
        }
    }

    fn enabled(list: &Value, name: &str) -> bool {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["name"] == name)
            .unwrap_or_else(|| panic!("{name} listed in {list}"))["enabled"]
            .as_bool()
            .unwrap()
    }

    fn tool_names(result: &Value) -> Vec<String> {
        result["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn a_feature_that_is_off_refuses_ops_tools_and_routes() {
        let app = TestApp::builder().module(Probe).build().await;
        let team = app.seed(Persona::Team, "feat").await;
        let outsider = app.seed(Persona::Basic, "out").await;
        let ws = team.workspaces["main"].id.to_string();
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let member = app.with_session(&team.accounts["member"].session_token);
        let token = &team.api_keys["full"].token;
        let features = format!("/api/v1/workspaces/{ws}/features");
        let op = format!("/api/v1/workspaces/{ws}/probe");
        let route = format!("/m/probe/workspaces/{ws}/ping");

        let listed = member.get(&features).await;
        listed.expect(StatusCode::OK);
        assert!(enabled(&listed.body, "probe"), "on by default");
        assert_eq!(listed["items"][0]["description"], "A probe feature", "from the area");

        let refused =
            member.send(Method::PUT, &features, Some(&json!({ "disabled": ["probe"] }))).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "members can't change features");

        let off = admin.send(Method::PUT, &features, Some(&json!({ "disabled": ["probe"] }))).await;
        off.expect(StatusCode::OK);
        assert!(!enabled(&off.body, "probe"), "off");

        let call = member.get(&op).await;
        assert_eq!(call.status, StatusCode::FORBIDDEN, "op refused");
        assert!(
            call["error"]["message"].as_str().unwrap().contains("probe feature is off"),
            "{call:?}"
        );
        assert_eq!(admin.get(&route).await.status, StatusCode::FORBIDDEN, "raw route refused");
        let stranger = app.with_session(&outsider.accounts["admin"].session_token);
        assert_eq!(stranger.get(&op).await.status, StatusCode::NOT_FOUND, "outsiders still 404");

        let mcp = app.mcp(token);
        assert!(
            !tool_names(&mcp.list_tools().await).contains(&"probe_ping".to_owned()),
            "tool hidden"
        );
        let result = mcp.call_tool("probe_ping", json!({})).await;
        assert_eq!(result["structuredContent"]["error"]["code"], "forbidden", "tool refused");
        assert!(
            tool_names(&mcp.list_tools().await).contains(&"workspaces_features_get".to_owned()),
            "core tools stay"
        );

        let on = admin.send(Method::PUT, &features, Some(&json!({ "disabled": [] }))).await;
        on.expect(StatusCode::OK);
        assert!(enabled(&on.body, "probe"), "on again");
        member.get(&op).await.expect(StatusCode::OK);
        assert_eq!(admin.get(&route).await.text, "pong", "raw route back");
        assert!(
            tool_names(&mcp.list_tools().await).contains(&"probe_ping".to_owned()),
            "tool back"
        );
    }

    #[tokio::test]
    async fn unknown_features_are_refused() {
        let app = TestApp::builder().module(Probe).build().await;
        let team = app.seed(Persona::Team, "feat").await;
        let ws = team.workspaces["main"].id;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let res = admin
            .send(
                Method::PUT,
                &format!("/api/v1/workspaces/{ws}/features"),
                Some(&json!({ "disabled": ["probe", "projects"] })),
            )
            .await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "core isn't a feature");
        assert!(res["error"]["fields"]["disabled[1]"].is_array(), "{res:?}");
    }

    #[tokio::test]
    async fn platform_reports_features_per_workspace() {
        let app = TestApp::builder().module(Probe).build().await;
        let team = app.seed(Persona::Team, "feat").await;
        let other = app.seed(Persona::Team, "other").await;
        let ws = team.workspaces["main"].id;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        admin
            .send(
                Method::PUT,
                &format!("/api/v1/workspaces/{ws}/features"),
                Some(&json!({ "disabled": ["probe"] })),
            )
            .await
            .expect(StatusCode::OK);
        let platform = app.app().platform();
        assert!(!platform.feature_enabled(ws, "probe").await.unwrap(), "off here");
        assert!(
            platform.feature_enabled(other.workspaces["main"].id, "probe").await.unwrap(),
            "on in another workspace"
        );
        assert_eq!(platform.disabled_features(ws).await.unwrap(), ["probe"], "the off set");
    }
}
