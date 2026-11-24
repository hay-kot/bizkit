//! API key operations.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{API_KEY_PREFIX, api_keys, hash_token, random_token};
use crate::error::ApiError;
use crate::ids::{ApiKeyId, WorkspaceId};
use crate::module::ModuleContext;
use crate::ops::{Access, Level, Method, NoContent, OpCtx, Operation, Principal, Scope, Success};
use crate::time::UnixMillis;
use crate::usage::{self, Report, UsageKind};
use crate::validate::{FieldErrors, Validate, check_name};

const WORKSPACE_ADMIN: Access = Access::Workspace(Scope::new("workspace", Level::Admin));

/// How many characters of a token the key keeps for display.
const PREFIX_LEN: usize = 10;

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(ApiKeysList)?;
    cx.op(ApiKeysCreate)?;
    cx.op(ApiKeysRevoke)?;
    Ok(())
}

/// An API key. The secret is shown once, at creation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiKey {
    /// The key ID.
    pub id: ApiKeyId,
    /// The workspace the key works in.
    pub workspace_id: WorkspaceId,
    /// A label.
    pub name: String,
    /// The first characters of the token, to tell keys apart.
    pub prefix: String,
    /// What the key may do, such as `projects:write`.
    pub scopes: Vec<String>,
    /// When it was created.
    pub created_at: UnixMillis,
    /// When it stops working, if ever.
    pub expires_at: Option<UnixMillis>,
    /// When it was last used, to the hour.
    pub last_used_at: Option<UnixMillis>,
    /// When it was revoked.
    pub revoked_at: Option<UnixMillis>,
}

impl ApiKey {
    fn from_row(row: api_keys::ApiKeyRow) -> Result<Self, ApiError> {
        let scopes: Vec<String> =
            serde_json::from_str(&row.scopes).map_err(|err| ApiError::Internal(err.into()))?;
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            name: row.name,
            prefix: row.prefix,
            scopes,
            created_at: row.created_at,
            expires_at: row.expires_at,
            last_used_at: row.last_used_at,
            revoked_at: row.revoked_at,
        })
    }
}

/// The keys of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiKeyList {
    /// Newest first, revoked keys included.
    pub items: Vec<ApiKey>,
}

/// A new key and its token.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatedApiKey {
    /// The key.
    pub key: ApiKey,
    /// The bearer token, `bk_...`. Shown only now.
    pub token: String,
}

/// Just the workspace from the path.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListKeys {
    /// The workspace.
    workspace_id: WorkspaceId,
}
impl Validate for ListKeys {}

/// A new key.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateKey {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// A label.
    name: String,
    /// Scopes such as `projects:read`. You can only grant scopes you hold.
    scopes: Vec<String>,
    /// Days until the key expires; never when absent.
    #[serde(default)]
    expires_in_days: Option<u32>,
}
impl Validate for CreateKey {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        if self.scopes.is_empty() {
            errors.add("scopes", "must name at least one scope");
        }
        if self.expires_in_days == Some(0) {
            errors.add("expires_in_days", "must be at least 1");
        }
        errors.into_result()
    }
}

/// A key of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InKey {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The key.
    api_key_id: ApiKeyId,
}
impl Validate for InKey {}

struct ApiKeysList;
impl Operation for ApiKeysList {
    type Input = ListKeys;
    type Output = ApiKeyList;
    const ID: &'static str = "api_keys.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/api-keys";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "List the API keys of a workspace";
    const TAGS: &'static [&'static str] = &["api_keys"];

    async fn call(&self, cx: OpCtx, input: ListKeys) -> Result<ApiKeyList, ApiError> {
        let rows = api_keys::list(cx.platform().db().read(), input.workspace_id).await?;
        let items = rows.into_iter().map(ApiKey::from_row).collect::<Result<_, _>>()?;
        Ok(ApiKeyList { items })
    }
}

struct ApiKeysCreate;
impl Operation for ApiKeysCreate {
    type Input = CreateKey;
    type Output = CreatedApiKey;
    const ID: &'static str = "api_keys.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/api-keys";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "Create an API key; the token is shown once";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = &["api_keys"];

    async fn call(&self, cx: OpCtx, input: CreateKey) -> Result<CreatedApiKey, ApiError> {
        let mut errors = FieldErrors::default();
        let mut scopes = Vec::new();
        for (i, text) in input.scopes.iter().enumerate() {
            match Scope::parse(text, cx.platform().scope_areas()) {
                Ok(scope) => scopes.push(scope),
                Err(err) => errors.add(format!("scopes[{i}]"), err.to_string()),
            }
        }
        errors.into_result()?;
        if let Some(missing) = scopes.iter().find(|scope| !cx.has_scope(scope)) {
            return Err(ApiError::Forbidden(format!(
                "you can't grant {missing}, which you don't hold"
            )));
        }
        scopes.sort();
        scopes.dedup();
        let stored: Vec<String> = scopes.iter().map(ToString::to_string).collect();
        let stored =
            serde_json::to_string(&stored).map_err(|err| ApiError::Internal(err.into()))?;
        let token = format!("{API_KEY_PREFIX}{}", random_token());
        let prefix: String = token.chars().take(PREFIX_LEN).collect();
        let now = cx.now();
        let expires_at =
            input.expires_in_days.map(|days| now.plus(Duration::from_hours(u64::from(days) * 24)));
        let created_by = match cx.principal() {
            Some(Principal::User(user)) => Some(user.user_id),
            _ => None,
        };
        let secret_hash = hash_token(&token);
        let key = api_keys::NewApiKey {
            workspace_id: input.workspace_id,
            name: input.name.trim(),
            prefix: &prefix,
            secret_hash: &secret_hash,
            scopes: &stored,
            created_by,
            expires_at,
        };
        let mut tx = cx.write().await?;
        let row = api_keys::insert(&mut *tx, &key, now).await?;
        usage::report(&cx, &mut tx, Report::new(UsageKind::ApiKeyCreated, input.workspace_id))
            .await?;
        tx.commit().await?;
        Ok(CreatedApiKey { key: ApiKey::from_row(row)?, token })
    }
}

struct ApiKeysRevoke;
impl Operation for ApiKeysRevoke {
    type Input = InKey;
    type Output = NoContent;
    const ID: &'static str = "api_keys.revoke";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/api-keys/{api_key_id}";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "Revoke an API key";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = &["api_keys"];

    async fn call(&self, cx: OpCtx, input: InKey) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let creator = api_keys::revoke(&mut *tx, input.workspace_id, input.api_key_id, cx.now())
            .await?
            .ok_or(ApiError::NotFound)?;
        let report = Report::new(UsageKind::ApiKeyRevoked, input.workspace_id).affecting(creator);
        usage::report(&cx, &mut tx, report).await?;
        tx.commit().await?;
        Ok(NoContent)
    }
}
