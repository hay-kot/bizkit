//! Operations: each one declares its route, access and types once, and the
//! registry turns it into a JSON route, an OpenAPI entry and an MCP tool.

mod access;
pub(crate) mod http;
pub(crate) mod mcp;
mod openapi;
mod registry;

use std::fmt;
use std::future::Future;

use axum::http::StatusCode;
use serde::de::DeserializeOwned;
use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use utoipa::openapi::schema::{ObjectBuilder, Type};
use utoipa::openapi::{RefOr, Schema};

pub use self::access::{
    Access, Grant, KeyPrincipal, Level, Principal, Role, Scope, ScopeAreas, ScopeError,
    UserPrincipal, WorkspaceAccess, authorize_workspace,
};
pub(crate) use self::openapi::document as openapi_document;
pub use self::registry::{OpMeta, Registry};
use crate::app::Platform;
use crate::db::WriteTx;
use crate::error::ApiError;
use crate::ids::{UserId, WorkspaceId};
use crate::validate::Validate;

/// The HTTP method of an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    /// `GET`: input fields other than path params are query params.
    Get,
    /// `POST`: input fields other than path params form the JSON body.
    Post,
    /// `PUT`
    Put,
    /// `PATCH`
    Patch,
    /// `DELETE`: input fields other than path params are query params.
    Delete,
}

impl Method {
    /// True when non-path input travels in the query string.
    #[must_use]
    pub const fn uses_query(self) -> bool {
        matches!(self, Self::Get | Self::Delete)
    }

    /// The method name in lowercase, as OpenAPI paths key it.
    #[must_use]
    pub const fn as_lower(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Patch => "patch",
            Self::Delete => "delete",
        }
    }
}

/// The status an operation answers with when it succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Success {
    /// 200 with the output as JSON.
    Ok,
    /// 201 with the output as JSON.
    Created,
    /// 204 with no body. The output type must be [`NoContent`].
    NoContent,
}

impl Success {
    /// The HTTP status.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::Ok => StatusCode::OK,
            Self::Created => StatusCode::CREATED,
            Self::NoContent => StatusCode::NO_CONTENT,
        }
    }
}

/// One operation of the API.
///
/// The input is one struct merged from path params, query params and the
/// JSON body, so MCP can pass a single argument object. Path params are the
/// input fields named like the path's `{...}` segments.
pub trait Operation: Send + Sync + 'static {
    /// The merged input.
    type Input: DeserializeOwned + utoipa::ToSchema + Validate + Send + 'static;
    /// The output, serialized as the JSON response.
    type Output: Serialize + utoipa::ToSchema + Send + 'static;
    /// A unique ID such as `projects.create`; also the `operationId`.
    const ID: &'static str;
    /// The HTTP method.
    const METHOD: Method;
    /// The path under `/api/v1`, such as `/workspaces/{workspace_id}/projects`.
    const PATH: &'static str;
    /// Who may call it.
    const ACCESS: Access;
    /// One line for the docs and the MCP tool description.
    const SUMMARY: &'static str;
    /// The success status.
    const SUCCESS: Success = Success::Ok;
    /// OpenAPI tags.
    const TAGS: &'static [&'static str] = &[];
    /// Error statuses beyond the ones access and validation imply, such as 409.
    const ERRORS: &'static [StatusCode] = &[];

    /// Runs the operation. Access is already checked and the input validated.
    fn call(
        &self,
        cx: OpCtx,
        input: Self::Input,
    ) -> impl Future<Output = Result<Self::Output, ApiError>> + Send;
}

/// Input for operations that take none. Accepts any object and ignores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoInput;

impl<'de> Deserialize<'de> for NoInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NoInputVisitor;

        impl<'de> Visitor<'de> for NoInputVisitor {
            type Value = NoInput;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<NoInput, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(NoInput)
            }

            fn visit_unit<E>(self) -> Result<NoInput, E> {
                Ok(NoInput)
            }
        }

        deserializer.deserialize_any(NoInputVisitor)
    }
}

impl Validate for NoInput {}

impl utoipa::PartialSchema for NoInput {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new().schema_type(Type::Object).into()
    }
}

impl utoipa::ToSchema for NoInput {}

/// Output for operations that answer 204. MCP returns `{}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoContent;

impl Serialize for NoContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap as _;
        serializer.serialize_map(Some(0))?.end()
    }
}

impl utoipa::PartialSchema for NoContent {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new().schema_type(Type::Object).into()
    }
}

impl utoipa::ToSchema for NoContent {}

/// What an operation knows about its call.
#[derive(Debug, Clone)]
pub struct OpCtx {
    platform: Platform,
    principal: Option<Principal>,
    workspace: Option<WorkspaceAccess>,
    request_id: String,
}

impl OpCtx {
    pub(crate) const fn new(
        platform: Platform,
        principal: Option<Principal>,
        workspace: Option<WorkspaceAccess>,
        request_id: String,
    ) -> Self {
        Self { platform, principal, workspace, request_id }
    }

    /// The platform: config, database, deps and bus.
    #[must_use]
    pub const fn platform(&self) -> &Platform {
        &self.platform
    }

    /// The caller, if signed in.
    #[must_use]
    pub const fn principal(&self) -> Option<&Principal> {
        self.principal.as_ref()
    }

    /// The signed-in user. `Unauthenticated` for API keys and anonymous calls.
    ///
    /// # Errors
    ///
    /// `Unauthenticated` when the caller is an API key or anonymous.
    pub const fn user(&self) -> Result<&UserPrincipal, ApiError> {
        match &self.principal {
            Some(Principal::User(user)) => Ok(user),
            _ => Err(ApiError::Unauthenticated),
        }
    }

    /// The signed-in user's ID.
    ///
    /// # Errors
    ///
    /// `Unauthenticated` when the caller is an API key or anonymous.
    pub fn user_id(&self) -> Result<UserId, ApiError> {
        self.user().map(|user| user.user_id)
    }

    /// The workspace of a `Workspace`-access operation.
    ///
    /// # Errors
    ///
    /// `Internal` when the operation isn't a workspace operation.
    pub fn workspace_id(&self) -> Result<WorkspaceId, ApiError> {
        self.workspace.as_ref().map(|access| access.workspace_id).ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!("workspace_id outside a workspace operation"))
        })
    }

    /// The caller's grant in the operation's workspace.
    #[must_use]
    pub const fn workspace(&self) -> Option<&WorkspaceAccess> {
        self.workspace.as_ref()
    }

    /// True when the caller holds `scope` in the operation's workspace.
    #[must_use]
    pub fn has_scope(&self, scope: &Scope) -> bool {
        self.workspace.as_ref().is_some_and(|access| access.grant.allows(scope))
    }

    /// The request id, for logs and jobs.
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// The current time from the platform clock.
    #[must_use]
    pub fn now(&self) -> crate::time::UnixMillis {
        self.platform.now()
    }

    /// Starts a write transaction tagged with this request's id.
    ///
    /// # Errors
    ///
    /// `Internal` when no write transaction can start.
    pub async fn write(&self) -> Result<WriteTx, ApiError> {
        Ok(self.platform.db().write().await?.with_request_id(&self.request_id))
    }
}
