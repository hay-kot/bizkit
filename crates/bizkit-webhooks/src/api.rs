//! The webhooks module's API types.

use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Serialize};
use url::Url;
use utoipa::ToSchema;

bizkit_core::id_type!(EndpointId, "The ID of a webhook capture endpoint.");
bizkit_core::id_type!(RequestId, "The ID of a captured webhook request.");

/// A public URL that stores every request it gets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Endpoint {
    /// The endpoint ID, also its public address.
    pub id: EndpointId,
    /// The project it belongs to.
    pub project_id: ProjectId,
    /// The display name.
    pub name: String,
    /// Disabled endpoints answer 404 and store nothing.
    pub enabled: bool,
    /// The URL senders post to.
    #[schema(value_type = String)]
    pub url: Url,
    /// Where each captured request is sent on, if anywhere.
    #[schema(value_type = Option<String>)]
    pub forward_url: Option<Url>,
    /// Stored requests. The newest 500 are kept.
    pub request_count: u64,
    /// When the newest stored request arrived.
    pub last_received_at: Option<UnixMillis>,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When its settings last changed.
    pub updated_at: UnixMillis,
}

/// Endpoints of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EndpointList {
    /// Sorted by name.
    pub items: Vec<Endpoint>,
}

/// How a request body is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BodyEncoding {
    /// The body is UTF-8 text, given as is.
    Utf8,
    /// The body is binary, given as standard base64.
    Base64,
}

/// The latest time a request went to a URL, by forward or replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Delivery {
    /// Where it went.
    pub url: String,
    /// The answer's status, when one came.
    pub status: Option<u16>,
    /// Why it failed to get an answer, or why the answer counts as a failure.
    pub error: Option<String>,
    /// How long it took.
    pub took_ms: u64,
    /// When it was sent.
    pub at: UnixMillis,
}

/// A captured request without its headers and body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RequestSummary {
    /// The request ID.
    pub id: RequestId,
    /// The HTTP method.
    pub method: String,
    /// The query string without the `?`, if any.
    pub query: Option<String>,
    /// The `Content-Type` header, if any.
    pub content_type: Option<String>,
    /// The body size in bytes.
    pub size: u64,
    /// The latest forward or replay.
    pub delivery: Option<Delivery>,
    /// When it arrived.
    pub received_at: UnixMillis,
}

/// One request header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Header {
    /// The lowercase name.
    pub name: String,
    /// The value. Values over 4096 bytes are not stored.
    pub value: String,
}

/// A captured request in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CapturedRequest {
    /// The summary fields.
    #[serde(flatten)]
    pub summary: RequestSummary,
    /// The headers in arrival order, without `Authorization`, `Cookie`,
    /// `Proxy-Authorization`, hop-by-hop headers and `X-Request-Id`.
    pub headers: Vec<Header>,
    /// The body, encoded as `body_encoding` says.
    pub body: String,
    /// How `body` is encoded.
    pub body_encoding: BodyEncoding,
}

/// A page of requests, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RequestList {
    /// The requests.
    pub items: Vec<RequestSummary>,
    /// Pass as `before` for the next page; absent on the last page.
    pub next_before: Option<RequestId>,
}
