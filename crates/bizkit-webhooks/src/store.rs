//! The webhooks tables. Every query filters on `workspace_id`, except
//! [`find_public`]: public capture knows only the endpoint ID, and later
//! queries use the workspace of the row it returns.

use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::time::UnixMillis;
use sqlx::SqliteExecutor;

use crate::api::{EndpointId, RequestId};

/// A `webhook_endpoints` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndpointRow {
    pub(crate) id: EndpointId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) forward_url: Option<String>,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

/// An endpoint row with its request stats, for list and get.
#[derive(Debug, Clone)]
pub(crate) struct EndpointWithStats {
    pub(crate) endpoint: EndpointRow,
    pub(crate) request_count: i64,
    pub(crate) last_received_at: Option<UnixMillis>,
}

/// A `webhook_requests` row. `headers` and `delivery` stay JSON text here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestRow {
    pub(crate) id: RequestId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) endpoint_id: EndpointId,
    pub(crate) method: String,
    pub(crate) query: Option<String>,
    pub(crate) headers: String,
    pub(crate) content_type: Option<String>,
    pub(crate) body: Vec<u8>,
    pub(crate) delivery: Option<String>,
    pub(crate) received_at: UnixMillis,
}

/// A request row without headers and body, for the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SummaryRow {
    pub(crate) id: RequestId,
    pub(crate) method: String,
    pub(crate) query: Option<String>,
    pub(crate) content_type: Option<String>,
    pub(crate) size: i64,
    pub(crate) delivery: Option<String>,
    pub(crate) received_at: UnixMillis,
}

pub(crate) async fn insert_endpoint(
    exec: impl SqliteExecutor<'_>,
    row: &EndpointRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO webhook_endpoints (id, workspace_id, project_id, name, enabled, forward_url,
                                        created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.project_id,
        row.name,
        row.enabled,
        row.forward_url,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Writes every settings column of `row`, matched by workspace and ID.
pub(crate) async fn update_endpoint(
    exec: impl SqliteExecutor<'_>,
    row: &EndpointRow,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE webhook_endpoints SET name = ?, enabled = ?, forward_url = ?, updated_at = ?
         WHERE workspace_id = ? AND id = ?",
        row.name,
        row.enabled,
        row.forward_url,
        row.updated_at,
        row.workspace_id,
        row.id,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn list_endpoints(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: Option<ProjectId>,
) -> sqlx::Result<Vec<EndpointWithStats>> {
    let rows = sqlx::query!(
        r#"SELECT e.id AS "id: EndpointId", e.workspace_id AS "workspace_id: WorkspaceId",
                  e.project_id AS "project_id: ProjectId", e.name, e.enabled AS "enabled: bool",
                  e.forward_url, e.created_at AS "created_at: UnixMillis",
                  e.updated_at AS "updated_at: UnixMillis",
                  (SELECT COUNT(*) FROM webhook_requests r
                   WHERE r.workspace_id = e.workspace_id AND r.endpoint_id = e.id)
                    AS "request_count!: i64",
                  (SELECT MAX(r.received_at) FROM webhook_requests r
                   WHERE r.workspace_id = e.workspace_id AND r.endpoint_id = e.id)
                    AS "last_received_at: UnixMillis"
           FROM webhook_endpoints e
           WHERE e.workspace_id = ? AND (?2 IS NULL OR e.project_id = ?2)
           ORDER BY e.name, e.id"#,
        workspace_id,
        project_id,
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| EndpointWithStats {
            endpoint: EndpointRow {
                id: r.id,
                workspace_id: r.workspace_id,
                project_id: r.project_id,
                name: r.name,
                enabled: r.enabled,
                forward_url: r.forward_url,
                created_at: r.created_at,
                updated_at: r.updated_at,
            },
            request_count: r.request_count,
            last_received_at: r.last_received_at,
        })
        .collect())
}

pub(crate) async fn find_endpoint(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: EndpointId,
) -> sqlx::Result<Option<EndpointRow>> {
    sqlx::query_as!(
        EndpointRow,
        r#"SELECT id AS "id: EndpointId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, enabled AS "enabled: bool",
                  forward_url, created_at AS "created_at: UnixMillis",
                  updated_at AS "updated_at: UnixMillis"
           FROM webhook_endpoints WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        id,
    )
    .fetch_optional(exec)
    .await
}

/// Endpoints of the workspace among `ids`.
pub(crate) async fn find_endpoints(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    ids: &[EndpointId],
) -> sqlx::Result<Vec<EndpointRow>> {
    let ids = serde_json::to_string(ids).map_err(|err| sqlx::Error::Encode(Box::new(err)))?;
    sqlx::query_as!(
        EndpointRow,
        r#"SELECT id AS "id: EndpointId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, enabled AS "enabled: bool",
                  forward_url, created_at AS "created_at: UnixMillis",
                  updated_at AS "updated_at: UnixMillis"
           FROM webhook_endpoints
           WHERE workspace_id = ? AND id IN (SELECT value FROM json_each(?))
           ORDER BY name, id"#,
        workspace_id,
        ids,
    )
    .fetch_all(exec)
    .await
}

/// An enabled endpoint by ID alone, for public capture.
pub(crate) async fn find_public(
    exec: impl SqliteExecutor<'_>,
    id: EndpointId,
) -> sqlx::Result<Option<EndpointRow>> {
    sqlx::query_as!(
        EndpointRow,
        r#"SELECT id AS "id: EndpointId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, enabled AS "enabled: bool",
                  forward_url, created_at AS "created_at: UnixMillis",
                  updated_at AS "updated_at: UnixMillis"
           FROM webhook_endpoints WHERE id = ? AND enabled = 1"#,
        id,
    )
    .fetch_optional(exec)
    .await
}

pub(crate) async fn delete_endpoint(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: EndpointId,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM webhook_endpoints WHERE workspace_id = ? AND id = ?",
        workspace_id,
        id
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn insert_request(
    exec: impl SqliteExecutor<'_>,
    row: &RequestRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO webhook_requests (id, workspace_id, endpoint_id, method, query, headers,
                                       content_type, body, delivery, received_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.endpoint_id,
        row.method,
        row.query,
        row.headers,
        row.content_type,
        row.body,
        row.delivery,
        row.received_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Deletes the endpoint's requests past the newest `keep`.
pub(crate) async fn prune(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    endpoint_id: EndpointId,
    keep: i64,
) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        "DELETE FROM webhook_requests
         WHERE workspace_id = ?1 AND endpoint_id = ?2 AND id < (
           SELECT id FROM webhook_requests WHERE workspace_id = ?1 AND endpoint_id = ?2
           ORDER BY id DESC LIMIT 1 OFFSET ?3 - 1)",
        workspace_id,
        endpoint_id,
        keep,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected())
}

/// Requests the endpoint stored since `since`.
pub(crate) async fn recent(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    endpoint_id: EndpointId,
    since: UnixMillis,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM webhook_requests
           WHERE workspace_id = ? AND endpoint_id = ? AND received_at >= ?"#,
        workspace_id,
        endpoint_id,
        since,
    )
    .fetch_one(exec)
    .await
}

/// Requests newest first, older than `before` when given.
pub(crate) async fn list_requests(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    endpoint_id: EndpointId,
    before: Option<RequestId>,
    limit: i64,
) -> sqlx::Result<Vec<SummaryRow>> {
    sqlx::query_as!(
        SummaryRow,
        r#"SELECT id AS "id: RequestId", method, query, content_type,
                  LENGTH(body) AS "size!: i64", delivery, received_at AS "received_at: UnixMillis"
           FROM webhook_requests
           WHERE workspace_id = ?1 AND endpoint_id = ?2 AND (?3 IS NULL OR id < ?3)
           ORDER BY id DESC LIMIT ?4"#,
        workspace_id,
        endpoint_id,
        before,
        limit,
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn find_request(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    endpoint_id: EndpointId,
    id: RequestId,
) -> sqlx::Result<Option<RequestRow>> {
    sqlx::query_as!(
        RequestRow,
        r#"SELECT id AS "id: RequestId", workspace_id AS "workspace_id: WorkspaceId",
                  endpoint_id AS "endpoint_id: EndpointId", method, query, headers, content_type,
                  body, delivery, received_at AS "received_at: UnixMillis"
           FROM webhook_requests WHERE workspace_id = ? AND endpoint_id = ? AND id = ?"#,
        workspace_id,
        endpoint_id,
        id,
    )
    .fetch_optional(exec)
    .await
}

/// Records the latest delivery of a request. False when the request is gone.
pub(crate) async fn set_delivery(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    id: RequestId,
    delivery: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE webhook_requests SET delivery = ? WHERE workspace_id = ? AND id = ?",
        delivery,
        workspace_id,
        id,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}
