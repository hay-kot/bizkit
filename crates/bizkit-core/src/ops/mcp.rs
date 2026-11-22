//! The MCP adapter: every workspace operation as a tool, over stateless
//! Streamable HTTP, for API keys only.

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use super::registry::Registry;
use super::{Access, KeyPrincipal, Method, Principal};
use crate::app::Platform;
use crate::auth::middleware::Authn;
use crate::error::ApiError;

/// A tool and the operation behind it.
#[derive(Debug, Clone)]
pub(crate) struct McpTool {
    pub(crate) op_id: &'static str,
    pub(crate) tool: Tool,
}

/// The tools for every workspace operation, keyed by tool name.
pub(crate) fn tools(registry: &Registry) -> BTreeMap<String, McpTool> {
    registry
        .registered()
        .filter_map(|op| {
            let Access::Workspace(_) = &op.meta.access else { return None };
            let name = tool_name(op.meta.id);
            let schema = input_schema(&op.input_schema, registry.components());
            let mut tool = Tool::new(name.clone(), op.meta.summary, Arc::new(schema));
            tool.annotations = Some(
                ToolAnnotations::new()
                    .read_only(op.meta.method == Method::Get)
                    .destructive(op.meta.method == Method::Delete),
            );
            Some((name, McpTool { op_id: op.meta.id, tool }))
        })
        .collect()
}

/// `projects.list` → `projects_list`.
pub(crate) fn tool_name(op_id: &str) -> String {
    op_id.replace('.', "_")
}

/// The tool schema: the input schema without `workspace_id` (the key fills
/// it), with component refs rewritten to `#/$defs/...` and the components
/// they reach inlined under `$defs`.
pub(crate) fn input_schema(
    input: &Map<String, Value>,
    components: &BTreeMap<String, Value>,
) -> Map<String, Value> {
    let mut schema = input.clone();
    if let Some(Value::Object(props)) = schema.get_mut("properties") {
        props.remove("workspace_id");
    }
    if let Some(Value::Array(required)) = schema.get_mut("required") {
        required.retain(|name| name != "workspace_id");
        if required.is_empty() {
            schema.remove("required");
        }
    }
    schema.insert("type".to_owned(), Value::from("object"));

    let mut defs = BTreeMap::new();
    let mut pending: Vec<String> = Vec::new();
    collect_refs(&Value::Object(schema.clone()), &mut pending);
    while let Some(name) = pending.pop() {
        if defs.contains_key(&name) {
            continue;
        }
        if let Some(component) = components.get(&name) {
            collect_refs(component, &mut pending);
            defs.insert(name, rewrite_refs(component.clone()));
        }
    }
    let Value::Object(mut schema) = rewrite_refs(Value::Object(schema)) else { return Map::new() };
    if !defs.is_empty() {
        schema.insert("$defs".to_owned(), Value::Object(defs.into_iter().collect()));
    }
    schema
}

const COMPONENT_PREFIX: &str = "#/components/schemas/";

fn collect_refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(name) = map
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix(COMPONENT_PREFIX))
            {
                out.push(name.to_owned());
            }
            map.values().for_each(|v| collect_refs(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| collect_refs(v, out)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn rewrite_refs(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, v)| match (key.as_str(), v) {
                    ("$ref", Value::String(target)) => {
                        let rewritten = target
                            .strip_prefix(COMPONENT_PREFIX)
                            .map_or_else(|| target.clone(), |name| format!("#/$defs/{name}"));
                        (key, Value::String(rewritten))
                    }
                    (_, v) => (key, rewrite_refs(v)),
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(rewrite_refs).collect()),
        scalar @ (Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)) => scalar,
    }
}

/// The rmcp handler.
#[derive(Debug, Clone)]
pub(crate) struct McpHandler {
    registry: Arc<Registry>,
    tools: Arc<BTreeMap<String, McpTool>>,
    platform: Platform,
}

impl McpHandler {
    fn key(ctx: &RequestContext<RoleServer>) -> Result<(KeyPrincipal, String), ErrorData> {
        let parts = ctx.extensions.get::<http::request::Parts>();
        let request_id =
            parts.map_or_else(|| "-".to_owned(), |parts| super::http::request_id(&parts.headers));
        match parts
            .and_then(|parts| parts.extensions.get::<Authn>())
            .and_then(|authn| authn.principal.clone())
        {
            Some(Principal::ApiKey(key)) => Ok((key, request_id)),
            _ => Err(ErrorData::invalid_request("an API key is required", None)),
        }
    }

    fn list_tools_now(
        &self,
        ctx: &RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let (key, _) = Self::key(ctx)?;
        let tools = self
            .tools
            .values()
            .filter(|tool| {
                self.registry.get(tool.op_id).is_some_and(|op| match &op.meta.access {
                    Access::Workspace(scope) => key.grants(scope),
                    Access::Public | Access::User => false,
                })
            })
            .map(|tool| tool.tool.clone())
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call(
        &self,
        key: KeyPrincipal,
        request_id: String,
        request: CallToolRequestParams,
    ) -> Result<Value, ApiError> {
        let tool = self.tools.get(request.name.as_ref()).ok_or(ApiError::NotFound)?;
        let op = self.registry.get(tool.op_id).ok_or(ApiError::NotFound)?;
        let workspace_id = key.workspace_id.to_string();
        let mut input = request.arguments.unwrap_or_default();
        input.insert("workspace_id".to_owned(), Value::String(workspace_id.clone()));
        let principal = Some(Principal::ApiKey(key));
        let cx =
            Registry::authorize(op, &self.platform, principal, Some(&workspace_id), request_id)
                .await?;
        Registry::invoke(op, cx, Value::Object(input)).await
    }
}

impl ServerHandler for McpHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("bizkit", env!("CARGO_PKG_VERSION")))
            .with_instructions("Tools act on the workspace the API key belongs to.")
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        ctx: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(self.list_tools_now(&ctx))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let (key, request_id) = Self::key(&ctx)?;
        if !self.tools.contains_key(request.name.as_ref()) {
            return Err(ErrorData::invalid_params(format!("unknown tool {}", request.name), None));
        }
        let result = match self.call(key, request_id.clone(), request).await {
            Ok(output) => CallToolResult::structured(output),
            Err(err) => {
                err.log(&request_id);
                let body = serde_json::to_value(err.body()).unwrap_or(Value::Null);
                let mut result = CallToolResult::error(vec![ContentBlock::text(body.to_string())]);
                result.structured_content = Some(body);
                result
            }
        };
        Ok(result.into())
    }
}

/// The Streamable HTTP service for `/mcp`.
pub(crate) fn service(
    registry: &Arc<Registry>,
    platform: &Platform,
    shutdown: CancellationToken,
) -> StreamableHttpService<McpHandler, LocalSessionManager> {
    let handler = McpHandler {
        registry: Arc::clone(registry),
        tools: Arc::new(tools(registry)),
        platform: platform.clone(),
    };
    // Host checks guard servers that trust ambient credentials against DNS
    // rebinding. /mcp accepts only bearer API keys, which a browser never
    // attaches on its own, and bizkit runs behind arbitrary public hostnames.
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .disable_allowed_hosts()
        .with_cancellation_token(shutdown);
    StreamableHttpService::new(move || Ok(handler.clone()), Arc::default(), config)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rewrites_refs_and_inlines_components() {
        let input = json!({
            "type": "object",
            "required": ["workspace_id", "item"],
            "properties": {
                "workspace_id": { "$ref": "#/components/schemas/WorkspaceId" },
                "item": { "$ref": "#/components/schemas/Item" },
            },
        });
        let components = BTreeMap::from([
            ("WorkspaceId".to_owned(), json!({ "type": "string", "format": "uuid" })),
            (
                "Item".to_owned(),
                json!({ "type": "object", "properties": { "tag": { "$ref": "#/components/schemas/Tag" } } }),
            ),
            ("Tag".to_owned(), json!({ "type": "string" })),
            ("Unused".to_owned(), json!({ "type": "string" })),
        ]);
        let schema = input_schema(input.as_object().unwrap(), &components);
        insta::assert_json_snapshot!(Value::Object(schema), @r##"
        {
          "$defs": {
            "Item": {
              "properties": {
                "tag": {
                  "$ref": "#/$defs/Tag"
                }
              },
              "type": "object"
            },
            "Tag": {
              "type": "string"
            }
          },
          "properties": {
            "item": {
              "$ref": "#/$defs/Item"
            }
          },
          "required": [
            "item"
          ],
          "type": "object"
        }
        "##);
    }
}
