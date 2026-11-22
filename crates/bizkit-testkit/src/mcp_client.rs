//! A JSON-RPC client for `/mcp`.

#![expect(clippy::panic, reason = "test helpers fail the test on a bad response")]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum_test::TestServer;
use http::{HeaderValue, StatusCode, header};
use serde_json::{Value, json};

/// The MCP protocol version the client asks for.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Posts JSON-RPC to `/mcp` with a bearer API key.
#[derive(Debug, Clone)]
pub struct McpClient {
    server: Arc<TestServer>,
    token: Option<String>,
    next_id: Arc<AtomicU64>,
}

impl McpClient {
    pub(crate) fn new(server: Arc<TestServer>, token: Option<String>) -> Self {
        Self { server, token, next_id: Arc::new(AtomicU64::new(1)) }
    }

    /// Posts `message` and returns the status and the JSON-RPC response,
    /// read from a JSON or an SSE body.
    ///
    /// # Panics
    ///
    /// When the token isn't a valid header value.
    pub async fn post(&self, message: &Value) -> (StatusCode, Value) {
        let mut request = self
            .server
            .post("/mcp")
            .add_header(
                header::ACCEPT,
                HeaderValue::from_static("application/json, text/event-stream"),
            )
            .add_header(
                header::HeaderName::from_static("mcp-protocol-version"),
                HeaderValue::from_static(PROTOCOL_VERSION),
            )
            .json(message);
        if let Some(token) = &self.token {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .unwrap_or_else(|err| panic!("token header: {err}"));
            request = request.add_header(header::AUTHORIZATION, value);
        }
        let response = request.await;
        let status = response.status_code();
        let text = response.text();
        (status, parse_body(&text))
    }

    /// Sends a request and returns its `result`.
    ///
    /// # Panics
    ///
    /// On a non-200 status or a JSON-RPC error.
    pub async fn request(&self, method: &str, params: Value) -> Value {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (status, response) = self
            .post(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        assert_eq!(status, StatusCode::OK, "{method}: HTTP status; body: {response}");
        assert!(response.get("error").is_none(), "{method}: JSON-RPC error: {response}");
        response.get("result").cloned().unwrap_or(Value::Null)
    }

    /// `tools/list`.
    pub async fn list_tools(&self) -> Value {
        self.request("tools/list", json!({})).await
    }

    /// `tools/call`.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({ "name": name, "arguments": arguments })).await
    }
}

/// The JSON-RPC message from a JSON body, or the last `data:` event of an
/// SSE body.
fn parse_body(text: &str) -> Value {
    if let Ok(value) = serde_json::from_str(text) {
        return value;
    }
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str(data.trim()).ok())
        .next_back()
        .unwrap_or_else(|| Value::String(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sse_bodies() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        assert_eq!(parse_body(body)["id"], 1, "the data line");
    }
}
