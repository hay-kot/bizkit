//! The webhooks operations: endpoint CRUD, the request inspector and replay.

use anyhow::Context as _;
use base64::Engine as _;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Level, Method, NoContent, OpCtx, Operation, Scope, Success};
use bizkit_core::validate::{FieldErrors, Validate, check_name, present};
use bizkit_core::{ApiError, ModuleContext, Platform};
use http::StatusCode;
use serde::Deserialize;
use url::Url;
use utoipa::ToSchema;

use crate::api::{
    BodyEncoding, CapturedRequest, Delivery, Endpoint, EndpointId, EndpointList, Header, RequestId,
    RequestList, RequestSummary,
};
use crate::deliver;
use crate::store::{self, EndpointRow, RequestRow, SummaryRow};

const READ: Access = Access::Workspace(Scope::new("webhooks", Level::Read));
const WRITE: Access = Access::Workspace(Scope::new("webhooks", Level::Write));
const TAGS: &[&str] = &["webhooks"];
const NO_TARGET: &str = "give a url, or set the endpoint's forward_url";

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.scope_area("webhooks", "Webhook capture endpoints and their requests")?;
    cx.op(EndpointsList)?;
    cx.op(EndpointsCreate)?;
    cx.op(EndpointsGet)?;
    cx.op(EndpointsUpdate)?;
    cx.op(EndpointsDelete)?;
    cx.op(RequestsList)?;
    cx.op(RequestsGet)?;
    cx.op(RequestsReplay)?;
    Ok(())
}

// ---------------------------------------------------------------- conversions

/// The public URL senders post to.
pub(crate) fn capture_url(platform: &Platform, id: EndpointId) -> anyhow::Result<Url> {
    platform
        .base_url()
        .join(&format!("/m/webhooks/public/{id}"))
        .context("building the endpoint URL")
}

fn to_api(platform: &Platform, row: store::EndpointWithStats) -> anyhow::Result<Endpoint> {
    let endpoint = row.endpoint;
    Ok(Endpoint {
        url: capture_url(platform, endpoint.id)?,
        id: endpoint.id,
        project_id: endpoint.project_id,
        name: endpoint.name,
        enabled: endpoint.enabled,
        forward_url: endpoint.forward_url.as_deref().and_then(|url| Url::parse(url).ok()),
        request_count: u64::try_from(row.request_count).unwrap_or(0),
        last_received_at: row.last_received_at,
        created_at: endpoint.created_at,
        updated_at: endpoint.updated_at,
    })
}

fn decode_delivery(text: Option<&str>) -> anyhow::Result<Option<Delivery>> {
    text.map(|text| serde_json::from_str(text).context("decoding a delivery")).transpose()
}

fn summary_to_api(row: SummaryRow) -> anyhow::Result<RequestSummary> {
    Ok(RequestSummary {
        delivery: decode_delivery(row.delivery.as_deref())?,
        id: row.id,
        method: row.method,
        query: row.query,
        content_type: row.content_type,
        size: u64::try_from(row.size).unwrap_or(0),
        received_at: row.received_at,
    })
}

pub(crate) fn request_to_api(row: RequestRow) -> anyhow::Result<CapturedRequest> {
    let headers: Vec<(String, String)> = serde_json::from_str(&row.headers)
        .with_context(|| format!("decoding the headers of request {}", row.id))?;
    let (body, body_encoding) = match String::from_utf8(row.body) {
        Ok(text) => (text, BodyEncoding::Utf8),
        Err(err) => (
            base64::engine::general_purpose::STANDARD.encode(err.into_bytes()),
            BodyEncoding::Base64,
        ),
    };
    Ok(CapturedRequest {
        summary: RequestSummary {
            delivery: decode_delivery(row.delivery.as_deref())?,
            id: row.id,
            method: row.method,
            query: row.query,
            content_type: row.content_type,
            size: u64::try_from(body_len(&body, body_encoding)).unwrap_or(0),
            received_at: row.received_at,
        },
        headers: headers.into_iter().map(|(name, value)| Header { name, value }).collect(),
        body,
        body_encoding,
    })
}

fn body_len(body: &str, encoding: BodyEncoding) -> usize {
    match encoding {
        BodyEncoding::Utf8 => body.len(),
        BodyEncoding::Base64 => {
            base64::engine::general_purpose::STANDARD.decode(body).map_or(0, |bytes| bytes.len())
        }
    }
}

async fn endpoint_in(
    cx: &OpCtx,
    workspace_id: WorkspaceId,
    endpoint_id: EndpointId,
) -> Result<EndpointRow, ApiError> {
    store::find_endpoint(cx.platform().db().read(), workspace_id, endpoint_id)
        .await?
        .ok_or(ApiError::NotFound)
}

/// The endpoint with its stats, read in one place for get and update.
async fn with_stats(cx: &OpCtx, row: EndpointRow) -> Result<Endpoint, ApiError> {
    let stats =
        store::list_endpoints(cx.platform().db().read(), row.workspace_id, Some(row.project_id))
            .await?
            .into_iter()
            .find(|e| e.endpoint.id == row.id)
            .unwrap_or(store::EndpointWithStats {
                endpoint: row,
                request_count: 0,
                last_received_at: None,
            });
    Ok(to_api(cx.platform(), stats)?)
}

fn check_target(errors: &mut FieldErrors, field: &str, url: &Url) {
    if !matches!(url.scheme(), "http" | "https") {
        errors.add(field, "must be an http or https URL");
    } else if let Err(err) = bizkit_core::net::check_url(url) {
        errors.add(field, err.to_string());
    }
}

// ---------------------------------------------------------------- inputs

/// Which endpoints to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListEndpoints {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// Only this project's endpoints.
    #[serde(default)]
    project_id: Option<ProjectId>,
}
impl Validate for ListEndpoints {}

/// A new endpoint.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateEndpoint {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project it belongs to.
    project_id: ProjectId,
    /// The display name.
    name: String,
    /// Where to send each captured request on, a public http or https URL.
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    forward_url: Option<Url>,
}

impl Validate for CreateEndpoint {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        if let Some(url) = &self.forward_url {
            check_target(&mut errors, "forward_url", url);
        }
        errors.into_result()
    }
}

/// Changes to an endpoint. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateEndpoint {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The endpoint.
    endpoint_id: EndpointId,
    /// The display name.
    #[serde(default)]
    name: Option<String>,
    /// Disabled endpoints answer 404 and store nothing.
    #[serde(default)]
    enabled: Option<bool>,
    /// Where to send each captured request on; `null` stops forwarding.
    #[serde(default, deserialize_with = "present")]
    #[expect(clippy::option_option, reason = "absent keeps the URL, null removes it")]
    #[schema(value_type = Option<String>)]
    forward_url: Option<Option<Url>>,
}

impl Validate for UpdateEndpoint {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(name) = &self.name {
            check_name(&mut errors, "name", name);
        }
        if let Some(Some(url)) = &self.forward_url {
            check_target(&mut errors, "forward_url", url);
        }
        errors.into_result()
    }
}

/// An endpoint of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InEndpoint {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The endpoint.
    endpoint_id: EndpointId,
}
impl Validate for InEndpoint {}

/// Which requests to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListRequests {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The endpoint.
    endpoint_id: EndpointId,
    /// Only requests older than this one, for the next page.
    #[serde(default)]
    before: Option<RequestId>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}
impl Validate for ListRequests {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if self.limit.is_some_and(|limit| !(1..=200).contains(&limit)) {
            errors.add("limit", "must be between 1 and 200");
        }
        errors.into_result()
    }
}

/// A request of an endpoint.
#[expect(clippy::struct_field_names, reason = "path params are named like the path segments")]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InRequest {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The endpoint.
    endpoint_id: EndpointId,
    /// The request.
    request_id: RequestId,
}
impl Validate for InRequest {}

/// Where to replay a request.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplayRequest {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The endpoint.
    endpoint_id: EndpointId,
    /// The request.
    request_id: RequestId,
    /// A public http or https URL. Default the endpoint's forward URL.
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    url: Option<Url>,
}
impl Validate for ReplayRequest {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(url) = &self.url {
            check_target(&mut errors, "url", url);
        }
        errors.into_result()
    }
}

// ---------------------------------------------------------------- endpoints

struct EndpointsList;
impl Operation for EndpointsList {
    type Input = ListEndpoints;
    type Output = EndpointList;
    const ID: &'static str = "webhooks.endpoints.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/webhooks";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List the webhook capture endpoints of a workspace";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListEndpoints) -> Result<EndpointList, ApiError> {
        let rows =
            store::list_endpoints(cx.platform().db().read(), input.workspace_id, input.project_id)
                .await?;
        let items = rows
            .into_iter()
            .map(|row| to_api(cx.platform(), row))
            .collect::<anyhow::Result<_>>()?;
        Ok(EndpointList { items })
    }
}

struct EndpointsCreate;
impl Operation for EndpointsCreate {
    type Input = CreateEndpoint;
    type Output = Endpoint;
    const ID: &'static str = "webhooks.endpoints.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/webhooks";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Create a webhook capture endpoint in a project";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateEndpoint) -> Result<Endpoint, ApiError> {
        let now = cx.now();
        let row = EndpointRow {
            id: EndpointId::new(),
            workspace_id: input.workspace_id,
            project_id: input.project_id,
            name: input.name.trim().to_owned(),
            enabled: true,
            forward_url: input.forward_url.map(String::from),
            created_at: now,
            updated_at: now,
        };
        let mut tx = cx.write().await?;
        if !bizkit_core::projects::exists(&mut *tx, input.workspace_id, input.project_id).await? {
            let mut errors = FieldErrors::default();
            errors.add("project_id", "is not a project of this workspace");
            return Err(errors.into());
        }
        store::insert_endpoint(&mut *tx, &row).await?;
        tx.commit().await?;
        let stats =
            store::EndpointWithStats { endpoint: row, request_count: 0, last_received_at: None };
        Ok(to_api(cx.platform(), stats)?)
    }
}

struct EndpointsGet;
impl Operation for EndpointsGet {
    type Input = InEndpoint;
    type Output = Endpoint;
    const ID: &'static str = "webhooks.endpoints.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/webhooks/{endpoint_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a webhook capture endpoint";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InEndpoint) -> Result<Endpoint, ApiError> {
        let row = endpoint_in(&cx, input.workspace_id, input.endpoint_id).await?;
        with_stats(&cx, row).await
    }
}

struct EndpointsUpdate;
impl Operation for EndpointsUpdate {
    type Input = UpdateEndpoint;
    type Output = Endpoint;
    const ID: &'static str = "webhooks.endpoints.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/webhooks/{endpoint_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Change a webhook capture endpoint's settings";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateEndpoint) -> Result<Endpoint, ApiError> {
        let mut tx = cx.write().await?;
        let mut row = store::find_endpoint(&mut *tx, input.workspace_id, input.endpoint_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if let Some(name) = input.name {
            name.trim().clone_into(&mut row.name);
        }
        if let Some(enabled) = input.enabled {
            row.enabled = enabled;
        }
        if let Some(url) = input.forward_url {
            row.forward_url = url.map(String::from);
        }
        row.updated_at = cx.now();
        store::update_endpoint(&mut *tx, &row).await?;
        tx.commit().await?;
        with_stats(&cx, row).await
    }
}

struct EndpointsDelete;
impl Operation for EndpointsDelete {
    type Input = InEndpoint;
    type Output = NoContent;
    const ID: &'static str = "webhooks.endpoints.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/webhooks/{endpoint_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Delete a webhook capture endpoint and its requests";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InEndpoint) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted =
            store::delete_endpoint(&mut *tx, input.workspace_id, input.endpoint_id).await?;
        tx.commit().await?;
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

// ---------------------------------------------------------------- requests

struct RequestsList;
impl Operation for RequestsList {
    type Input = ListRequests;
    type Output = RequestList;
    const ID: &'static str = "webhooks.requests.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/webhooks/{endpoint_id}/requests";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List an endpoint's captured requests, newest first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListRequests) -> Result<RequestList, ApiError> {
        endpoint_in(&cx, input.workspace_id, input.endpoint_id).await?;
        let limit = input.limit.unwrap_or(50);
        let rows = store::list_requests(
            cx.platform().db().read(),
            input.workspace_id,
            input.endpoint_id,
            input.before,
            i64::from(limit) + 1,
        )
        .await?;
        let mut items = rows.into_iter().map(summary_to_api).collect::<anyhow::Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let more = items.len() > limit;
        items.truncate(limit);
        let next_before = if more { items.last().map(|r| r.id) } else { None };
        Ok(RequestList { items, next_before })
    }
}

struct RequestsGet;
impl Operation for RequestsGet {
    type Input = InRequest;
    type Output = CapturedRequest;
    const ID: &'static str = "webhooks.requests.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/webhooks/{endpoint_id}/requests/{request_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a captured request with its headers and body";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InRequest) -> Result<CapturedRequest, ApiError> {
        let row = store::find_request(
            cx.platform().db().read(),
            input.workspace_id,
            input.endpoint_id,
            input.request_id,
        )
        .await?
        .ok_or(ApiError::NotFound)?;
        Ok(request_to_api(row)?)
    }
}

struct RequestsReplay;
impl Operation for RequestsReplay {
    type Input = ReplayRequest;
    type Output = Delivery;
    const ID: &'static str = "webhooks.requests.replay";
    const METHOD: Method = Method::Post;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/webhooks/{endpoint_id}/requests/{request_id}/replay";
    const ACCESS: Access = WRITE;
    // A failed send is still a 200: the returned delivery says why.
    const SUMMARY: &'static str = "Send a captured request again, to a URL or the forward URL";
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ReplayRequest) -> Result<Delivery, ApiError> {
        let db = cx.platform().db().read();
        let endpoint = endpoint_in(&cx, input.workspace_id, input.endpoint_id).await?;
        let request =
            store::find_request(db, input.workspace_id, input.endpoint_id, input.request_id)
                .await?
                .ok_or(ApiError::NotFound)?;
        let url = match input.url {
            Some(url) => url,
            None => endpoint
                .forward_url
                .as_deref()
                .and_then(|url| Url::parse(url).ok())
                .ok_or_else(|| ApiError::Conflict(NO_TARGET.to_owned()))?,
        };
        let platform = cx.platform();
        let delivery =
            deliver::deliver(platform.deps().http.as_ref(), platform.now(), &request, &url).await;
        let mut tx = cx.write().await?;
        deliver::record(&mut tx, input.workspace_id, input.request_id, &delivery).await?;
        tx.commit().await?;
        Ok(delivery)
    }
}
