//! The webhooks hook point: `webhooks.request_received` runs after each
//! captured request is stored. Scripts bind to endpoints.

use anyhow::Context as _;
use async_trait::async_trait;
use bizkit_core::Platform;
use bizkit_core::hooks::{
    HookKind, HookPoint, HookSource, HookTarget, TargetEntity, TargetKind, TargetResolver,
};
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::jobs::{Event, JobCtx};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::{EndpointId, RequestId};
use crate::store::{self, EndpointRow, RequestRow};

/// The most body text a script gets. Longer or binary bodies come as null.
/// Async hook input is capped at 256 KiB, and JSON escaping can double the
/// text.
pub const MAX_HOOK_BODY: usize = 32 * 1024;
/// The most header bytes (names and values) a script gets; later headers
/// are left out, so the whole input stays under the async input cap.
pub const MAX_HOOK_HEADERS: usize = 32 * 1024;

/// Header names that look like they carry a credential. Scripts never get
/// them; the inspector still shows them.
fn is_secret_header(name: &str) -> bool {
    ["token", "secret", "key", "password", "auth", "session"].iter().any(|word| name.contains(word))
}

/// Published in the insert transaction for each stored request.
#[expect(clippy::struct_field_names, reason = "the IDs are named for what they identify")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestReceived {
    /// The workspace.
    pub workspace_id: WorkspaceId,
    /// The endpoint.
    pub endpoint_id: EndpointId,
    /// The stored request.
    pub request_id: RequestId,
}

impl Event for RequestReceived {
    const NAME: &'static str = "webhooks.request.received";
}

/// Scripts on the webhooks point can bind to one endpoint.
pub static ENDPOINT: TargetKind =
    TargetKind { id: "webhooks.endpoint", noun: "endpoint", plural: "endpoints" };

/// Runs as a job after each captured request is stored.
pub const REQUEST_RECEIVED: HookPoint = HookPoint {
    id: "webhooks.request_received",
    summary: "After each captured request is stored, such as to post it to a chat channel or call another API.",
    sample: request_received_sample,
    kind: HookKind::Async { event: RequestReceived::NAME },
    target: Some(&ENDPOINT),
};

fn endpoint_json(endpoint: &EndpointRow) -> Value {
    json!({ "id": endpoint.id, "name": endpoint.name, "project_id": endpoint.project_id })
}

/// The `webhooks.request_received` input. Headers come as an object; a
/// repeated header keeps its last value. Credential-like headers and those
/// past [`MAX_HOOK_HEADERS`] are left out.
pub(crate) fn request_received_input(endpoint: &EndpointRow, request: &RequestRow) -> Value {
    let pairs: Vec<(String, String)> = serde_json::from_str(&request.headers).unwrap_or_default();
    let mut budget = MAX_HOOK_HEADERS;
    let mut headers = Map::new();
    for (name, value) in pairs.into_iter().filter(|(name, _)| !is_secret_header(name)) {
        let Some(left) = budget.checked_sub(name.len() + value.len()) else { break };
        budget = left;
        headers.insert(name, Value::String(value));
    }
    let body = (request.body.len() <= MAX_HOOK_BODY)
        .then(|| std::str::from_utf8(&request.body).ok())
        .flatten();
    let parsed = body.and_then(|text| serde_json::from_str::<Value>(text).ok());
    json!({
        "endpoint": endpoint_json(endpoint),
        "request": {
            "id": request.id,
            "method": request.method,
            "query": request.query,
            "headers": headers,
            "content_type": request.content_type,
            "body": body,
            "json": parsed,
            "received_at": request.received_at,
        },
    })
}

fn request_received_sample() -> Value {
    json!({
        "endpoint": {
            "id": "0190a5f0-0000-7000-8000-000000000001",
            "name": "Stripe",
            "project_id": "0190a5f0-0000-7000-8000-000000000002",
        },
        "request": {
            "id": "0190a5f0-0000-7000-8000-000000000003",
            "method": "POST",
            "query": null,
            "headers": { "content-type": "application/json", "stripe-signature": "t=1767225600,v1=..." },
            "content_type": "application/json",
            "body": "{\"type\":\"payment_intent.succeeded\"}",
            "json": { "type": "payment_intent.succeeded" },
            "received_at": "2026-01-01T00:00:00Z",
        },
    })
}

fn entity(endpoint: EndpointRow) -> TargetEntity {
    TargetEntity {
        id: endpoint.id.to_string(),
        project_id: endpoint.project_id,
        label: endpoint.name,
    }
}

/// Resolves [`ENDPOINT`] targets: endpoint IDs, labelled by name.
#[derive(Debug, Clone, Copy, Default)]
pub struct EndpointTargets;

#[async_trait]
impl TargetResolver for EndpointTargets {
    async fn find(
        &self,
        platform: &Platform,
        workspace_id: WorkspaceId,
        ids: &[String],
    ) -> anyhow::Result<Vec<TargetEntity>> {
        let ids: Vec<EndpointId> = ids.iter().filter_map(|id| id.parse().ok()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let endpoints = store::find_endpoints(platform.db().read(), workspace_id, &ids)
            .await
            .context("loading endpoints")?;
        Ok(endpoints.into_iter().map(entity).collect())
    }

    async fn list(
        &self,
        platform: &Platform,
        workspace_id: WorkspaceId,
        project_id: ProjectId,
    ) -> anyhow::Result<Vec<TargetEntity>> {
        let endpoints = store::list_endpoints(platform.db().read(), workspace_id, Some(project_id))
            .await
            .context("listing endpoints")?;
        Ok(endpoints.into_iter().map(|row| entity(row.endpoint)).collect())
    }
}

/// Builds the `webhooks.request_received` target and input from the event.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebhooksHookSource;

impl HookSource<RequestReceived> for WebhooksHookSource {
    async fn target(
        &self,
        cx: &JobCtx,
        event: &RequestReceived,
    ) -> anyhow::Result<Option<HookTarget>> {
        let db = cx.platform.db().read();
        let endpoint = store::find_endpoint(db, event.workspace_id, event.endpoint_id)
            .await
            .context("loading the endpoint")?;
        // A request pruned before the run has nothing to run on.
        let request =
            store::find_request(db, event.workspace_id, event.endpoint_id, event.request_id)
                .await
                .context("loading the request")?;
        Ok(endpoint.filter(|_| request.is_some()).map(|endpoint| HookTarget {
            workspace_id: endpoint.workspace_id,
            project_id: endpoint.project_id,
            entity: Some(endpoint.id.to_string()),
        }))
    }

    async fn input(&self, cx: &JobCtx, event: &RequestReceived) -> anyhow::Result<Option<Value>> {
        let db = cx.platform.db().read();
        let Some(endpoint) = store::find_endpoint(db, event.workspace_id, event.endpoint_id)
            .await
            .context("loading the endpoint")?
        else {
            return Ok(None);
        };
        let Some(request) =
            store::find_request(db, event.workspace_id, event.endpoint_id, event.request_id)
                .await
                .context("loading the request")?
        else {
            return Ok(None);
        };
        Ok(Some(request_received_input(&endpoint, &request)))
    }
}

#[cfg(test)]
mod tests {
    use bizkit_core::time::UnixMillis;

    use super::*;

    fn rows(body: &[u8]) -> (EndpointRow, RequestRow) {
        let endpoint = EndpointRow {
            id: EndpointId::new(),
            workspace_id: WorkspaceId::new(),
            project_id: ProjectId::new(),
            name: "Stripe".to_owned(),
            enabled: true,
            forward_url: None,
            created_at: UnixMillis(0),
            updated_at: UnixMillis(0),
        };
        let request = RequestRow {
            id: RequestId::new(),
            workspace_id: endpoint.workspace_id,
            endpoint_id: endpoint.id,
            method: "POST".to_owned(),
            query: Some("a=1".to_owned()),
            headers: r#"[["content-type","application/json"],["x-a","1"],["x-a","2"],["x-api-key","sk"],["x-gitlab-token","t"],["stripe-signature","v1"]]"#.to_owned(),
            content_type: Some("application/json".to_owned()),
            body: body.to_vec(),
            delivery: None,
            received_at: UnixMillis(5),
        };
        (endpoint, request)
    }

    #[test]
    fn json_bodies_are_parsed() {
        let (endpoint, request) = rows(br#"{"type":"charge"}"#);
        let input = request_received_input(&endpoint, &request);
        assert_eq!(input["request"]["json"], json!({"type": "charge"}), "parsed");
        assert_eq!(input["request"]["body"], r#"{"type":"charge"}"#, "text");
        assert_eq!(input["request"]["headers"]["x-a"], "2", "last value wins");
        let headers = input["request"]["headers"].as_object().unwrap();
        assert!(
            !headers.contains_key("x-api-key") && !headers.contains_key("x-gitlab-token"),
            "secrets left out"
        );
        assert!(headers.contains_key("stripe-signature"), "signatures kept");
        assert_eq!(input["endpoint"]["name"], "Stripe", "endpoint");
    }

    #[test]
    fn headers_past_the_budget_are_left_out() {
        let (endpoint, mut request) = rows(b"{}");
        let pairs: Vec<(String, String)> =
            (0..100).map(|i| (format!("x-h{i:03}"), "v".repeat(4000))).collect();
        request.headers = serde_json::to_string(&pairs).unwrap();
        let input = request_received_input(&endpoint, &request);
        let text = serde_json::to_string(&input["request"]["headers"]).unwrap();
        assert!(text.len() < MAX_HOOK_HEADERS + 2048, "{} bytes", text.len());
    }

    #[test]
    fn binary_and_long_bodies_are_null() {
        let (endpoint, request) = rows(&[0xff, 0xfe]);
        let input = request_received_input(&endpoint, &request);
        assert_eq!(input["request"]["body"], Value::Null, "binary");
        let (long_endpoint, long_request) = rows(&vec![b'a'; MAX_HOOK_BODY + 1]);
        let long = request_received_input(&long_endpoint, &long_request);
        assert_eq!(long["request"]["body"], Value::Null, "too long");
        assert_eq!(long["request"]["json"], Value::Null, "no json");
    }
}
