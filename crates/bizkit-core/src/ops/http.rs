//! The HTTP adapter: one JSON route per operation under `/api/v1`.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{FromRequestParts as _, RawPathParams, Request};
use axum::http::HeaderMap;
use axum::response::{IntoResponse as _, Response};
use axum::routing::{MethodFilter, MethodRouter};
use serde_json::{Map, Value};

use super::Success;
use super::registry::{FieldKind, RegisteredOp, Registry};
use super::{Method, Principal};
use crate::app::Platform;
use crate::auth::middleware::Authn;
use crate::error::ApiError;
use crate::validate::FieldErrors;

/// The largest JSON body an operation accepts.
pub(crate) const MAX_BODY_BYTES: usize = 1024 * 1024;

const fn filter(method: Method) -> MethodFilter {
    match method {
        Method::Get => MethodFilter::GET,
        Method::Post => MethodFilter::POST,
        Method::Put => MethodFilter::PUT,
        Method::Patch => MethodFilter::PATCH,
        Method::Delete => MethodFilter::DELETE,
    }
}

/// Routes for every operation in `registry`.
pub(crate) fn router(registry: &Arc<Registry>, platform: &Platform) -> Router {
    let mut by_path: BTreeMap<&'static str, MethodRouter> = BTreeMap::new();
    for op in registry.registered() {
        let id = op.meta.id;
        let registry = Arc::clone(registry);
        let platform = platform.clone();
        let handler =
            move |req: Request| async move { handle(&registry, id, &platform, req).await };
        let entry = by_path.remove(op.meta.path).unwrap_or_default();
        by_path.insert(op.meta.path, entry.on(filter(op.meta.method), handler));
    }
    by_path.into_iter().fold(Router::new(), |router, (path, methods)| {
        router.route(&format!("/api/v1{path}"), methods)
    })
}

/// The request id the request-id layer set, or `-` outside it.
pub(crate) fn request_id(headers: &HeaderMap) -> String {
    headers.get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or("-").to_owned()
}

async fn handle(
    registry: &Registry,
    id: &'static str,
    platform: &Platform,
    req: Request,
) -> Response {
    let (mut parts, body) = req.into_parts();
    let request_id = request_id(&parts.headers);
    let result = async {
        let op = registry.get(id).ok_or(ApiError::NotFound)?;
        let principal: Option<Principal> =
            parts.extensions.get::<Authn>().and_then(|authn| authn.principal.clone());
        let params: Vec<(String, String)> = RawPathParams::from_request_parts(&mut parts, &())
            .await
            .map_or_default(|raw| raw.iter().map(|(k, v)| (k.to_owned(), v.to_owned())).collect());
        let workspace_id =
            params.iter().find(|(k, _)| k == "workspace_id").map(|(_, v)| v.as_str());
        let cx =
            Registry::authorize(op, platform, principal, workspace_id, request_id.clone()).await?;
        let body = axum::body::to_bytes(Body::new(body), MAX_BODY_BYTES).await.map_err(|_| {
            ApiError::BadRequest(format!("body must be at most {MAX_BODY_BYTES} bytes"))
        })?;
        let input = merge_input(op, &params, parts.uri.query(), &body)?;
        let output = Registry::invoke(op, cx, input).await?;
        Ok::<_, ApiError>((op.meta.success, output))
    }
    .await;
    match result {
        Ok((Success::NoContent, _)) => Success::NoContent.status().into_response(),
        Ok((success, output)) => (success.status(), axum::Json(output)).into_response(),
        Err(err) => {
            err.log(&request_id);
            err.into_response()
        }
    }
}

/// Merges the JSON body, query params and path params into one input object.
/// Path params win over query params, which win over body fields.
pub(crate) fn merge_input(
    op: &RegisteredOp,
    path_params: &[(String, String)],
    query: Option<&str>,
    body: &[u8],
) -> Result<Value, FieldErrors> {
    let mut errors = FieldErrors::default();
    let mut input = Map::new();
    if !op.meta.method.uses_query() && !body.iter().all(u8::is_ascii_whitespace) {
        match serde_json::from_slice::<Value>(body) {
            Ok(Value::Object(map)) => input = map,
            Ok(_) => errors.add("", "body must be a JSON object"),
            Err(err) => errors.add("", format!("body is not valid JSON: {err}")),
        }
    }
    if op.meta.method.uses_query()
        && let Some(query) = query
    {
        merge_query(&op.field_kinds, query, &op.meta.path_params, &mut input, &mut errors);
    }
    for (name, raw) in path_params {
        let kind = op.field_kinds.get(name).unwrap_or(&FieldKind::String);
        match coerce(kind, raw) {
            Ok(value) => {
                input.insert(name.clone(), value);
            }
            Err(message) => errors.add(name.clone(), message),
        }
    }
    errors.into_result().map(|()| Value::Object(input))
}

fn merge_query(
    kinds: &BTreeMap<String, FieldKind>,
    query: &str,
    path_params: &[&'static str],
    input: &mut Map<String, Value>,
    errors: &mut FieldErrors,
) {
    let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        values.entry(key.into_owned()).or_default().push(value.into_owned());
    }
    for (name, raws) in values {
        // Unknown keys are ignored like any HTTP API's, so cache busters and
        // tracking params don't break requests. Path params can't be overridden.
        let Some(kind) = kinds.get(&name) else { continue };
        if path_params.contains(&name.as_str()) {
            continue;
        }
        let coerced = match kind {
            FieldKind::Array(item) => raws
                .iter()
                .enumerate()
                .map(|(i, raw)| {
                    coerce(item, raw).map_err(|message| (format!("{name}[{i}]"), message))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array),
            scalar @ (FieldKind::String
            | FieldKind::Integer
            | FieldKind::Number
            | FieldKind::Boolean) => raws
                .last()
                .map_or(Ok(Value::Null), |raw| coerce(scalar, raw).map_err(|m| (name.clone(), m))),
        };
        match coerced {
            Ok(value) => {
                input.insert(name, value);
            }
            Err((field, message)) => errors.add(field, message),
        }
    }
}

/// Turns one path or query string into the JSON value its schema expects.
pub(crate) fn coerce(kind: &FieldKind, raw: &str) -> Result<Value, String> {
    match kind {
        FieldKind::String => Ok(Value::String(raw.to_owned())),
        FieldKind::Integer => raw.parse::<i64>().map_or_else(
            |_| raw.parse::<u64>().map(Value::from).map_err(|_| "must be an integer".to_owned()),
            |n| Ok(Value::from(n)),
        ),
        FieldKind::Number => raw
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| "must be a number".to_owned()),
        FieldKind::Boolean => match raw {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err("must be true or false".to_owned()),
        },
        FieldKind::Array(item) => coerce(item, raw).map(|value| Value::Array(vec![value])),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn coerces_scalars() {
        assert_eq!(coerce(&FieldKind::Integer, "5"), Ok(Value::from(5)), "integer");
        assert_eq!(
            coerce(&FieldKind::Integer, "x"),
            Err("must be an integer".to_owned()),
            "bad integer"
        );
        assert_eq!(coerce(&FieldKind::Boolean, "true"), Ok(Value::Bool(true)), "boolean");
        assert_eq!(
            coerce(&FieldKind::Boolean, "1"),
            Err("must be true or false".to_owned()),
            "bad bool"
        );
        assert_eq!(coerce(&FieldKind::Number, "1.5"), Ok(Value::from(1.5)), "number");
        assert_eq!(coerce(&FieldKind::String, "5"), Ok(Value::from("5")), "string stays");
    }

    #[test]
    fn repeated_keys_form_arrays() {
        let kinds =
            BTreeMap::from([("tag".to_owned(), FieldKind::Array(Box::new(FieldKind::Integer)))]);
        let mut input = Map::new();
        let mut errors = FieldErrors::default();
        merge_query(&kinds, "tag=1&tag=2&other=x", &[], &mut input, &mut errors);
        assert!(errors.is_empty(), "no errors");
        assert_eq!(
            Value::Object(input),
            serde_json::json!({"tag": [1, 2]}),
            "array from repeated keys"
        );
    }

    #[test]
    fn bad_array_items_report_their_index() {
        let kinds =
            BTreeMap::from([("tag".to_owned(), FieldKind::Array(Box::new(FieldKind::Integer)))]);
        let mut errors = FieldErrors::default();
        merge_query(&kinds, "tag=1&tag=x", &[], &mut Map::new(), &mut errors);
        assert_eq!(errors.as_map().keys().collect::<Vec<_>>(), ["tag[1]"], "index in path");
    }

    proptest! {
        #[test]
        fn integers_survive_a_string_round_trip(n in any::<i64>()) {
            prop_assert_eq!(coerce(&FieldKind::Integer, &n.to_string()), Ok(Value::from(n)));
        }

        #[test]
        fn unsigned_integers_survive_a_string_round_trip(n in any::<u64>()) {
            prop_assert_eq!(coerce(&FieldKind::Integer, &n.to_string()), Ok(Value::from(n)));
        }

        #[test]
        fn booleans_survive_a_string_round_trip(b in any::<bool>()) {
            prop_assert_eq!(coerce(&FieldKind::Boolean, &b.to_string()), Ok(Value::Bool(b)));
        }

        #[test]
        fn finite_numbers_survive_a_string_round_trip(x in any::<f64>().prop_filter("finite", |x| x.is_finite())) {
            prop_assert_eq!(coerce(&FieldKind::Number, &x.to_string()), Ok(Value::from(x)));
        }

        #[test]
        fn strings_pass_through(s in ".*") {
            prop_assert_eq!(coerce(&FieldKind::String, &s), Ok(Value::String(s)));
        }
    }
}
