//! Builds the OpenAPI document from the registry.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use serde_json::{Map, Value, json};

use super::registry::{RegisteredOp, Registry};
use super::{Access, Success};
use crate::error::ErrorBody;

/// The OpenAPI document as JSON: every operation plus `extra` documents (raw
/// routes), with paths and components sorted.
pub(crate) fn document(registry: &Registry, extra: &[Value]) -> anyhow::Result<Value> {
    let mut paths: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for op in registry.registered() {
        let path = format!("/api/v1{}", op.meta.path);
        paths.entry(path).or_default().insert(op.meta.method.as_lower().to_owned(), operation(op));
    }

    let mut schemas: BTreeMap<String, Value> = registry.components().clone();
    let mut error_components = Vec::new();
    <ErrorBody as utoipa::ToSchema>::schemas(&mut error_components);
    error_components.push(("ErrorBody".to_owned(), <ErrorBody as utoipa::PartialSchema>::schema()));
    for (name, schema) in error_components {
        schemas.insert(name, serde_json::to_value(schema)?);
    }

    for doc in extra {
        if let Some(extra_paths) = doc.get("paths").and_then(Value::as_object) {
            for (path, item) in extra_paths {
                let entry = paths.entry(path.clone()).or_default();
                if let Some(item) = item.as_object() {
                    entry.extend(item.clone());
                }
            }
        }
        if let Some(extra_schemas) = doc.pointer("/components/schemas").and_then(Value::as_object) {
            schemas.extend(extra_schemas.clone());
        }
    }

    Ok(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "bizkit",
            "description": "The bizkit API. Every operation is also an MCP tool at /mcp when it runs in a workspace.",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "paths": paths,
        "components": {
            "schemas": schemas,
            "securitySchemes": {
                "session": { "type": "apiKey", "in": "cookie", "name": crate::auth::SESSION_COOKIE },
                "bearer": { "type": "http", "scheme": "bearer", "description": "An API key, bk_..." },
            },
        },
    }))
}

fn operation(op: &RegisteredOp) -> Value {
    let meta = &op.meta;
    let mut operation = Map::new();
    operation.insert("operationId".to_owned(), json!(meta.id));
    operation.insert("summary".to_owned(), json!(meta.summary));
    if !meta.tags.is_empty() {
        operation.insert("tags".to_owned(), json!(meta.tags));
    }
    match &meta.access {
        Access::Public => {}
        Access::User => {
            operation.insert("security".to_owned(), json!([{ "session": [] }]));
        }
        Access::Workspace(scope) => {
            operation.insert("security".to_owned(), json!([{ "session": [] }, { "bearer": [] }]));
            operation.insert("x-bizkit-scope".to_owned(), json!(scope.to_string()));
        }
    }

    let empty = Map::new();
    let properties = op.input_schema.get("properties").and_then(Value::as_object).unwrap_or(&empty);
    let required: Vec<&str> = op
        .input_schema
        .get("required")
        .and_then(Value::as_array)
        .map_or_default(|names| names.iter().filter_map(Value::as_str).collect());

    let mut parameters = Vec::new();
    for name in &meta.path_params {
        if let Some(schema) = properties.get(*name) {
            parameters
                .push(json!({ "name": name, "in": "path", "required": true, "schema": schema }));
        }
    }
    let rest: Map<String, Value> = properties
        .iter()
        .filter(|(name, _)| !meta.path_params.contains(&name.as_str()))
        .map(|(name, schema)| (name.clone(), schema.clone()))
        .collect();
    if meta.method.uses_query() {
        for (name, schema) in &rest {
            parameters.push(json!({
                "name": name,
                "in": "query",
                "required": required.contains(&name.as_str()),
                "schema": schema,
            }));
        }
    } else if !rest.is_empty() {
        let mut body = Map::new();
        body.insert("type".to_owned(), json!("object"));
        let body_required: Vec<&str> =
            required.iter().copied().filter(|name| rest.contains_key(*name)).collect();
        body.insert("properties".to_owned(), Value::Object(rest));
        if !body_required.is_empty() {
            body.insert("required".to_owned(), json!(body_required));
        }
        if let Some(additional) = op.input_schema.get("additionalProperties") {
            body.insert("additionalProperties".to_owned(), additional.clone());
        }
        operation.insert(
            "requestBody".to_owned(),
            json!({ "required": true, "content": { "application/json": { "schema": body } } }),
        );
    }
    if !parameters.is_empty() {
        operation.insert("parameters".to_owned(), Value::Array(parameters));
    }
    operation.insert("responses".to_owned(), Value::Object(responses(op)));
    Value::Object(operation)
}

fn responses(op: &RegisteredOp) -> Map<String, Value> {
    let meta = &op.meta;
    let mut responses = Map::new();
    let success = meta.success.status();
    let success_body = match (&op.output, meta.success) {
        (Some(name), Success::Ok | Success::Created) => json!({
            "description": reason(success),
            "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{name}") } } },
        }),
        _ => json!({ "description": reason(success) }),
    };
    responses.insert(success.as_u16().to_string(), success_body);

    let mut errors: Vec<StatusCode> = vec![StatusCode::INTERNAL_SERVER_ERROR];
    if meta.access != Access::Public {
        // 403 covers the scope check and the cookie CSRF check.
        errors.extend([StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN]);
    }
    if matches!(meta.access, Access::Workspace(_)) {
        errors.push(StatusCode::NOT_FOUND);
    }
    if !meta.no_input || !meta.path_params.is_empty() {
        errors.push(StatusCode::UNPROCESSABLE_ENTITY);
    }
    errors.extend(meta.errors);
    for status in errors {
        responses.insert(status.as_u16().to_string(), error_response(status));
    }
    responses
}

/// The response object for an error status with the shared error body.
pub(crate) fn error_response(status: StatusCode) -> Value {
    json!({
        "description": reason(status),
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorBody" } } },
    })
}

fn reason(status: StatusCode) -> &'static str {
    status.canonical_reason().unwrap_or("Response")
}
