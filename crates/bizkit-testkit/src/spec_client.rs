//! An HTTP client that fails the test when a response strays from the
//! app's own OpenAPI document.

#![expect(clippy::panic, reason = "a response that strays from the spec fails the test")]

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use axum_test::TestServer;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// Checks responses against the OpenAPI document. Request bodies are not
/// checked, so negative tests can send bad input.
pub struct SpecChecker {
    spec: Value,
    validators: Mutex<HashMap<String, Arc<jsonschema::Validator>>>,
}

impl fmt::Debug for SpecChecker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpecChecker").finish_non_exhaustive()
    }
}

impl SpecChecker {
    /// A checker for `spec`, the app's OpenAPI document as JSON.
    #[must_use]
    pub fn new(spec: Value) -> Self {
        Self { spec, validators: Mutex::new(HashMap::new()) }
    }

    /// The documented template for `path`, preferring static segments.
    fn template(&self, path: &str) -> Option<&str> {
        let paths = self.spec.get("paths")?.as_object()?;
        let segments: Vec<&str> = path.split('/').collect();
        paths
            .keys()
            .filter(|template| {
                let parts: Vec<&str> = template.split('/').collect();
                parts.len() == segments.len()
                    && parts
                        .iter()
                        .zip(&segments)
                        .all(|(t, s)| t == s || (t.starts_with('{') && t.ends_with('}')))
            })
            .min_by_key(|template| template.matches('{').count())
            .map(String::as_str)
    }

    /// Checks one response. Paths outside `/api/` (health, MCP, test hooks)
    /// aren't in the spec and pass unchecked.
    ///
    /// # Panics
    ///
    /// Unless `status` and `body` are documented for `method path`.
    pub fn check(&self, method: &Method, path: &str, status: StatusCode, body: &[u8]) {
        self.check_typed(method, path, status, Typed { content_type: None, body });
    }

    /// [`check`](Self::check) for a response with a content type. A body
    /// that isn't JSON must be documented under its media type and is not
    /// validated further.
    ///
    /// # Panics
    ///
    /// Unless `status` and `body` are documented for `method path`.
    pub fn check_typed(&self, method: &Method, path: &str, status: StatusCode, typed: Typed<'_>) {
        let Typed { content_type, body } = typed;
        let path = path.split('?').next().unwrap_or(path);
        if !path.starts_with("/api/") && !path.starts_with("/m/") {
            return;
        }
        let Some(template) = self.template(path) else {
            panic!("{method} {path}: path is not in the OpenAPI document");
        };
        let method_key = method.as_str().to_lowercase();
        let pointer = format!("/paths/{}/{method_key}", escape(template));
        let Some(operation) = self.spec.pointer(&pointer) else {
            panic!("{method} {path}: method is not documented for {template}");
        };
        let code = status.as_u16().to_string();
        let Some(response) = operation.pointer(&format!("/responses/{code}")) else {
            let body = String::from_utf8_lossy(body);
            panic!(
                "{method} {path}: status {status} is not documented for {template}; body: {body}"
            );
        };
        let media = content_type.and_then(|v| v.split(';').next()).map(str::trim);
        if let Some(media) = media.filter(|m| !body.is_empty() && *m != "application/json") {
            assert!(
                response.pointer(&format!("/content/{}", escape(media))).is_some(),
                "{method} {path}: {status} documents no {media} body"
            );
            return;
        }
        let Some(schema) = response.pointer("/content/application~1json/schema") else {
            assert!(
                body.is_empty(),
                "{method} {path}: {status} documents no body but got {}",
                String::from_utf8_lossy(body)
            );
            return;
        };
        let instance: Value = serde_json::from_slice(body).unwrap_or_else(|err| {
            panic!(
                "{method} {path}: {status} body is not JSON ({err}): {}",
                String::from_utf8_lossy(body)
            )
        });
        let validator = self.validator(&format!("{pointer}/responses/{code}"), schema);
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        assert!(
            errors.is_empty(),
            "{method} {path}: {status} body doesn't match the spec:\n  {}\nbody: {instance:#}",
            errors.join("\n  ")
        );
    }

    fn validator(&self, key: &str, schema: &Value) -> Arc<jsonschema::Validator> {
        let mut cache = self.validators.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(validator) = cache.get(key) {
            return Arc::clone(validator);
        }
        // Refs point at `#/components/schemas/...`, so the components ride
        // along at the root of the schema document.
        let document = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "components": self.spec.get("components").cloned().unwrap_or(Value::Null),
            "allOf": [schema],
        });
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .should_validate_formats(true)
            .build(&document)
            .unwrap_or_else(|err| panic!("compiling the schema at {key}: {err}"));
        let validator = Arc::new(validator);
        cache.insert(key.to_owned(), Arc::clone(&validator));
        validator
    }
}

/// JSON pointer escaping.
fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// A response the spec check passed.
#[derive(Debug, Clone)]
pub struct ApiResponse {
    /// The status.
    pub status: StatusCode,
    /// The headers.
    pub headers: HeaderMap,
    /// The JSON body, or `Null` when empty or not JSON.
    pub body: Value,
    /// The raw body as text.
    pub text: String,
}

impl ApiResponse {
    /// Asserts the status.
    ///
    /// # Panics
    ///
    /// Unless the status is `status`; the message carries the body.
    #[track_caller]
    #[expect(clippy::must_use_candidate, reason = "callers may assert without using the response")]
    pub fn expect(&self, status: StatusCode) -> &Self {
        assert_eq!(self.status, status, "unexpected status; body: {}", self.text);
        self
    }

    /// The body as `T`.
    ///
    /// # Panics
    ///
    /// When the body doesn't decode as `T`.
    #[track_caller]
    #[must_use]
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_value(self.body.clone())
            .unwrap_or_else(|err| panic!("decoding {}: {err}", self.text))
    }

    /// The error code of an error body.
    #[must_use]
    pub fn error_code(&self) -> Option<&str> {
        self.body.pointer("/error/code").and_then(Value::as_str)
    }
}

impl std::ops::Index<&str> for ApiResponse {
    type Output = Value;

    fn index(&self, key: &str) -> &Value {
        self.body.get(key).unwrap_or(&Value::Null)
    }
}

/// A client for the test app. Every response goes through the spec check.
#[derive(Debug, Clone)]
pub struct SpecClient {
    server: Arc<TestServer>,
    checker: Arc<SpecChecker>,
    session: Option<String>,
    bearer: Option<String>,
    origin: String,
}

impl SpecClient {
    pub(crate) const fn new(
        server: Arc<TestServer>,
        checker: Arc<SpecChecker>,
        session: Option<String>,
        bearer: Option<String>,
        origin: String,
    ) -> Self {
        Self { server, checker, session, bearer, origin }
    }

    /// The same client sending `origin` as its `Origin` header.
    #[must_use]
    pub fn with_origin(&self, origin: &str) -> Self {
        Self { origin: origin.to_owned(), ..self.clone() }
    }

    /// The session token this client sends, if any.
    #[must_use]
    pub fn session_token(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// `GET path`.
    pub async fn get(&self, path: &str) -> ApiResponse {
        self.send(Method::GET, path, None).await
    }

    /// `POST path` with a JSON body.
    pub async fn post(&self, path: &str, body: &Value) -> ApiResponse {
        self.send(Method::POST, path, Some(body)).await
    }

    /// `PATCH path` with a JSON body.
    pub async fn patch(&self, path: &str, body: &Value) -> ApiResponse {
        self.send(Method::PATCH, path, Some(body)).await
    }

    /// `DELETE path`.
    pub async fn delete(&self, path: &str) -> ApiResponse {
        self.send(Method::DELETE, path, None).await
    }

    /// Sends a request and checks the response against the spec.
    ///
    /// # Panics
    ///
    /// When the response strays from the spec.
    pub async fn send(&self, method: Method, path: &str, body: Option<&Value>) -> ApiResponse {
        self.send_inner(method, path, body.map(Body::Json), true).await
    }

    /// Sends a raw body and checks the response against the spec. For raw
    /// routes such as form ingest.
    ///
    /// # Panics
    ///
    /// When the response strays from the spec.
    pub async fn send_body(&self, method: Method, path: &str, raw: RawBody<'_>) -> ApiResponse {
        self.send_inner(method, path, Some(Body::Raw(raw)), true).await
    }

    /// [`send_body`](Self::send_body) without the spec check, for requests
    /// the spec doesn't describe, such as CORS preflights.
    pub async fn send_body_unchecked(
        &self,
        method: Method,
        path: &str,
        raw: RawBody<'_>,
    ) -> ApiResponse {
        self.send_inner(method, path, Some(Body::Raw(raw)), false).await
    }

    /// Sends a request without the spec check, for paths the spec can't
    /// describe, such as the JSON 404 for unknown `/api` paths.
    pub async fn send_unchecked(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> ApiResponse {
        self.send_inner(method, path, body.map(Body::Json), false).await
    }

    async fn send_inner(
        &self,
        method: Method,
        path: &str,
        body: Option<Body<'_>>,
        check: bool,
    ) -> ApiResponse {
        let mut request = self.server.method(method.clone(), path);
        if let Some(token) = &self.session {
            request = request
                .add_header(header::COOKIE, header_value(&format!("bizkit_session={token}")));
        }
        if let Some(token) = &self.bearer {
            request =
                request.add_header(header::AUTHORIZATION, header_value(&format!("Bearer {token}")));
        }
        if !matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
            request = request.add_header(header::ORIGIN, header_value(&self.origin));
        }
        match body {
            Some(Body::Json(value)) => request = request.json(value),
            Some(Body::Raw(raw)) => {
                for (name, value) in raw.headers {
                    request = request.add_header(name.clone(), header_value(value));
                }
                request = request.content_type(raw.content_type).bytes(raw.body);
            }
            None => {}
        }
        let response = request.await;
        let status = response.status_code();
        let bytes = response.as_bytes().clone();
        if check {
            let content_type =
                response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok());
            self.checker.check_typed(&method, path, status, Typed { content_type, body: &bytes });
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        ApiResponse {
            status,
            headers: response.headers().clone(),
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            text,
        }
    }
}

/// A response body and its `Content-Type`, for [`SpecChecker::check_typed`].
#[derive(Debug, Clone, Copy)]
pub struct Typed<'a> {
    /// The `Content-Type` header; `None` is checked as JSON.
    pub content_type: Option<&'a str>,
    /// The body.
    pub body: &'a [u8],
}

/// A request body that isn't JSON, for [`SpecClient::send_body`].
#[derive(Debug, Clone)]
pub struct RawBody<'a> {
    /// The `Content-Type` header.
    pub content_type: &'a str,
    /// The body.
    pub body: bytes::Bytes,
    /// Extra headers, such as `Accept`.
    pub headers: &'a [(HeaderName, &'a str)],
}

impl<'a> RawBody<'a> {
    /// `body` as `content_type`, with no extra headers.
    pub fn new(content_type: &'a str, body: impl Into<bytes::Bytes>) -> Self {
        Self { content_type, body: body.into(), headers: &[] }
    }
}

enum Body<'a> {
    Json(&'a Value),
    Raw(RawBody<'a>),
}

fn header_value(text: &str) -> HeaderValue {
    HeaderValue::from_str(text).unwrap_or_else(|err| panic!("header value {text:?}: {err}"))
}

/// The value of the `bizkit_session` cookie a response sets, if any.
#[must_use]
pub fn session_from(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(HeaderName::from_static("set-cookie"))
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|cookie| {
            cookie.strip_prefix("bizkit_session=")?.split(';').next().map(ToOwned::to_owned)
        })
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker() -> SpecChecker {
        SpecChecker::new(json!({
            "paths": {
                "/api/v1/things/{id}": { "get": { "responses": {
                    "200": { "description": "ok", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Thing" } } } },
                    "204": { "description": "none" },
                } } },
                "/api/v1/things/mine": { "get": { "responses": { "200": { "description": "ok" } } } },
            },
            "components": { "schemas": { "Thing": {
                "type": "object", "required": ["id"], "properties": { "id": { "type": "string", "format": "uuid" } },
            } } },
        }))
    }

    #[test]
    fn accepts_documented_responses() {
        checker().check(
            &Method::GET,
            "/api/v1/things/x?limit=1",
            StatusCode::OK,
            br#"{"id":"0190a5d0-0000-7000-8000-000000000001"}"#,
        );
        checker().check(&Method::GET, "/api/v1/things/x", StatusCode::NO_CONTENT, b"");
    }

    #[test]
    fn prefers_static_segments() {
        checker().check(&Method::GET, "/api/v1/things/mine", StatusCode::OK, b"");
    }

    #[test]
    #[should_panic(expected = "status 404 Not Found is not documented")]
    fn rejects_undocumented_statuses() {
        checker().check(&Method::GET, "/api/v1/things/x", StatusCode::NOT_FOUND, b"{}");
    }

    #[test]
    #[should_panic(expected = "path is not in the OpenAPI document")]
    fn rejects_undocumented_paths() {
        checker().check(&Method::GET, "/api/v1/other", StatusCode::OK, b"{}");
    }

    #[test]
    #[should_panic(expected = "method is not documented")]
    fn rejects_undocumented_methods() {
        checker().check(&Method::POST, "/api/v1/things/x", StatusCode::OK, b"{}");
    }

    #[test]
    #[should_panic(expected = "doesn't match the spec")]
    fn rejects_bodies_that_break_the_schema() {
        checker().check(
            &Method::GET,
            "/api/v1/things/x",
            StatusCode::OK,
            br#"{"id":"not-a-uuid"}"#,
        );
    }
}
