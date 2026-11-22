//! The MCP endpoint.

#[cfg(test)]
mod mcp {
    use bizkit_testkit::{Persona, TestApp};
    use http::StatusCode;
    use serde_json::{Value, json};

    fn tool_names(result: &Value) -> Vec<String> {
        result["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }

    async fn key_with(app: &TestApp, scopes: &[&str]) -> (String, String) {
        let team = app.seed(Persona::Team, "m1").await;
        let admin = app.with_session(&team.accounts["admin"].session_token);
        let ws = team.workspaces["main"].id.to_string();
        let created = admin
            .post(
                &format!("/api/v1/workspaces/{ws}/api-keys"),
                &json!({ "name": "agent", "scopes": scopes }),
            )
            .await;
        created.expect(StatusCode::CREATED);
        (created["token"].as_str().unwrap().to_owned(), ws)
    }

    #[tokio::test]
    async fn tools_list_shows_only_the_keys_tools() {
        let app = TestApp::new().await;
        let (token, _) = key_with(&app, &["projects:read"]).await;
        let tools = app.mcp(&token).list_tools().await;
        assert_eq!(tool_names(&tools), ["projects_get", "projects_list"], "read tools only");
        for tool in tools["tools"].as_array().unwrap() {
            assert!(
                tool["inputSchema"]["properties"].get("workspace_id").is_none(),
                "no workspace_id in {tool}"
            );
            assert_eq!(tool["annotations"]["readOnlyHint"], true, "GET tools are read-only");
        }
    }

    #[tokio::test]
    async fn projects_create_returns_structured_content() {
        let app = TestApp::new().await;
        let (token, ws) = key_with(&app, &["projects:write"]).await;
        let result = app
            .mcp(&token)
            .call_tool("projects_create", json!({ "name": "Docs", "slug": "docs" }))
            .await;
        assert_eq!(result["isError"], false, "success: {result}");
        assert_eq!(result["structuredContent"]["slug"], "docs", "the project");
        assert_eq!(
            result["structuredContent"]["workspace_id"],
            ws.as_str(),
            "in the key's workspace"
        );
        let text = result["content"][0]["text"].as_str().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap(),
            result["structuredContent"],
            "text mirrors the JSON"
        );
    }

    #[tokio::test]
    async fn a_missing_scope_is_a_tool_error() {
        let app = TestApp::new().await;
        let (token, _) = key_with(&app, &["projects:read"]).await;
        let result = app
            .mcp(&token)
            .call_tool("projects_create", json!({ "name": "Docs", "slug": "docs" }))
            .await;
        assert_eq!(result["isError"], true, "tool error: {result}");
        assert_eq!(result["structuredContent"]["error"]["code"], "forbidden", "the error body");
    }

    #[tokio::test]
    async fn validation_errors_are_tool_errors() {
        let app = TestApp::new().await;
        let (token, _) = key_with(&app, &["projects:write"]).await;
        let result =
            app.mcp(&token).call_tool("projects_create", json!({ "name": "", "slug": "x" })).await;
        assert_eq!(result["isError"], true, "tool error: {result}");
        assert_eq!(result["structuredContent"]["error"]["code"], "validation", "validation");
    }

    #[tokio::test]
    async fn requests_without_a_key_get_401() {
        let app = TestApp::new().await;
        let (status, _) = app
            .mcp_anonymous()
            .post(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }))
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "no key");
    }

    #[tokio::test]
    async fn sessions_cant_use_mcp() {
        let app = TestApp::new().await;
        let team = app.seed(Persona::Team, "m1").await;
        let response = app
            .with_session(&team.accounts["admin"].session_token)
            .send_unchecked(
                http::Method::POST,
                "/mcp",
                Some(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })),
            )
            .await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "cookies don't work on /mcp");
    }

    #[tokio::test]
    async fn initialize_works() {
        let app = TestApp::new().await;
        let (token, _) = key_with(&app, &["projects:read"]).await;
        let result = app
            .mcp(&token)
            .request(
                "initialize",
                json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
            )
            .await;
        assert_eq!(result["serverInfo"]["name"], "bizkit", "server info: {result}");
    }
}
