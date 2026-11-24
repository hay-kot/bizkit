//! The analytics tables. Every query filters on `workspace_id`, except the
//! site lookup by public key: ingest knows only the key, and later queries
//! use the workspace of the row it returns.

use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::time::UnixMillis;
use sqlx::SqliteExecutor;

use crate::api::{GoalId, PersonId};
use crate::report::Scope;

/// An `analytics_sites` row. `hostnames` stays JSON text here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SiteRow {
    pub(crate) project_id: ProjectId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) site_key: String,
    pub(crate) hostnames: String,
    pub(crate) identity_mode: String,
    pub(crate) last_event_day: Option<String>,
    pub(crate) rolled_through: Option<String>,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

/// An `analytics_goals` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GoalRow {
    pub(crate) id: GoalId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) matches: String,
    pub(crate) created_at: UnixMillis,
}

pub(crate) async fn insert_site(exec: impl SqliteExecutor<'_>, row: &SiteRow) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO analytics_sites (project_id, workspace_id, site_key, hostnames, identity_mode,
                                      last_event_day, rolled_through, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.project_id,
        row.workspace_id,
        row.site_key,
        row.hostnames,
        row.identity_mode,
        row.last_event_day,
        row.rolled_through,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn find_site(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Option<SiteRow>> {
    sqlx::query_as!(
        SiteRow,
        r#"SELECT project_id AS "project_id: ProjectId", workspace_id AS "workspace_id: WorkspaceId",
                  site_key, hostnames, identity_mode, last_event_day, rolled_through,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM analytics_sites WHERE workspace_id = ? AND project_id = ?"#,
        workspace_id,
        project_id,
    )
    .fetch_optional(exec)
    .await
}

/// The site of the project with these slugs.
pub(crate) async fn find_site_by_slugs(
    exec: impl SqliteExecutor<'_>,
    workspace_slug: &str,
    project_slug: &str,
) -> sqlx::Result<Option<SiteRow>> {
    sqlx::query_as!(
        SiteRow,
        r#"SELECT s.project_id AS "project_id: ProjectId", s.workspace_id AS "workspace_id: WorkspaceId",
                  s.site_key, s.hostnames, s.identity_mode, s.last_event_day, s.rolled_through,
                  s.created_at AS "created_at: UnixMillis", s.updated_at AS "updated_at: UnixMillis"
           FROM analytics_sites s
           JOIN workspaces w ON w.id = s.workspace_id
           JOIN projects p ON p.id = s.project_id AND p.workspace_id = s.workspace_id
           WHERE w.slug = ? AND p.slug = ?"#,
        workspace_slug,
        project_slug,
    )
    .fetch_optional(exec)
    .await
}

/// Writes the settings columns of `row`, matched by workspace and project.
pub(crate) async fn update_site_settings(
    exec: impl SqliteExecutor<'_>,
    row: &SiteRow,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE analytics_sites SET hostnames = ?, identity_mode = ?, updated_at = ?
         WHERE workspace_id = ? AND project_id = ?",
        row.hostnames,
        row.identity_mode,
        row.updated_at,
        row.workspace_id,
        row.project_id,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn insert_goal(exec: impl SqliteExecutor<'_>, row: &GoalRow) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO analytics_goals (id, workspace_id, project_id, name, kind, match, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.workspace_id,
        row.project_id,
        row.name,
        row.kind,
        row.matches,
        row.created_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn list_goals(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Vec<GoalRow>> {
    sqlx::query_as!(
        GoalRow,
        r#"SELECT id AS "id: GoalId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, kind, match AS matches,
                  created_at AS "created_at: UnixMillis"
           FROM analytics_goals WHERE workspace_id = ? AND project_id = ?
           ORDER BY name, id"#,
        workspace_id,
        project_id,
    )
    .fetch_all(exec)
    .await
}

pub(crate) async fn goal_exists(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    kind: &str,
    matches: &str,
) -> sqlx::Result<bool> {
    let found = sqlx::query_scalar!(
        "SELECT 1 AS found FROM analytics_goals
         WHERE workspace_id = ? AND project_id = ? AND kind = ? AND match = ?",
        workspace_id,
        project_id,
        kind,
        matches,
    )
    .fetch_optional(exec)
    .await?;
    Ok(found.is_some())
}

/// Deletes a goal and returns its project, or `None` when there was none.
pub(crate) async fn delete_goal(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    goal_id: GoalId,
) -> sqlx::Result<Option<ProjectId>> {
    sqlx::query_scalar!(
        r#"DELETE FROM analytics_goals WHERE workspace_id = ? AND id = ?
           RETURNING project_id AS "project_id: ProjectId""#,
        workspace_id,
        goal_id
    )
    .fetch_optional(exec)
    .await
}

/// Goals by ID, for hook targets. Unknown IDs are left out.
pub(crate) async fn find_goals(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    ids: &[GoalId],
) -> sqlx::Result<Vec<GoalRow>> {
    let ids = serde_json::to_string(ids).map_err(|err| sqlx::Error::Encode(Box::new(err)))?;
    sqlx::query_as!(
        GoalRow,
        r#"SELECT id AS "id: GoalId", workspace_id AS "workspace_id: WorkspaceId",
                  project_id AS "project_id: ProjectId", name, kind, match AS matches,
                  created_at AS "created_at: UnixMillis"
           FROM analytics_goals
           WHERE workspace_id = ? AND id IN (SELECT value FROM json_each(?))
           ORDER BY name, id"#,
        workspace_id,
        ids,
    )
    .fetch_all(exec)
    .await
}

/// The parts of a stored event a goal hook shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookEventRow {
    pub(crate) name: String,
    pub(crate) path: Option<String>,
    pub(crate) props: Option<String>,
    pub(crate) ts: UnixMillis,
    pub(crate) user_id: Option<String>,
    pub(crate) traits: Option<String>,
}

/// An event by row ID, with its person's user ID and traits when the
/// event is keyed to an identified person.
pub(crate) async fn find_hook_event(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    event_id: i64,
) -> sqlx::Result<Option<HookEventRow>> {
    sqlx::query_as!(
        HookEventRow,
        r#"SELECT e.name, e.path, e.props, e.ts AS "ts: UnixMillis",
                  p.user_id AS "user_id?", p.traits AS "traits?"
           FROM analytics_events e
           LEFT JOIN analytics_people p
             ON p.project_id = e.project_id AND e.person = 'p:' || p.id
           WHERE e.workspace_id = ? AND e.project_id = ? AND e.id = ?"#,
        workspace_id,
        project_id,
        event_id,
    )
    .fetch_optional(exec)
    .await
}

/// Finds a site by its public key, for ingest.
pub(crate) async fn find_site_by_key(
    exec: impl SqliteExecutor<'_>,
    site_key: &str,
) -> sqlx::Result<Option<SiteRow>> {
    sqlx::query_as!(
        SiteRow,
        r#"SELECT project_id AS "project_id: ProjectId", workspace_id AS "workspace_id: WorkspaceId",
                  site_key, hostnames, identity_mode, last_event_day, rolled_through,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM analytics_sites WHERE site_key = ?"#,
        site_key,
    )
    .fetch_optional(exec)
    .await
}

/// The projects in a workspace that have a site, oldest site first.
pub(crate) async fn list_site_projects(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
) -> sqlx::Result<Vec<ProjectId>> {
    sqlx::query_scalar!(
        r#"SELECT project_id AS "project_id: ProjectId" FROM analytics_sites
           WHERE workspace_id = ? ORDER BY created_at, project_id"#,
        workspace_id,
    )
    .fetch_all(exec)
    .await
}

/// Moves `last_event_day` forward to `day`. True when it moved: the first
/// event of a new day for the site.
pub(crate) async fn advance_event_day(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    day: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE analytics_sites SET last_event_day = ?
         WHERE workspace_id = ? AND project_id = ? AND (last_event_day IS NULL OR last_event_day < ?)",
        day,
        workspace_id,
        project_id,
        day,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Today's salt, made if missing. Older salts are deleted in the same call,
/// so yesterday's visitor hashes can't be recomputed.
pub(crate) async fn day_salt(
    tx: &mut sqlx::SqliteConnection,
    day: &str,
    fresh: &[u8],
) -> sqlx::Result<Vec<u8>> {
    sqlx::query!("INSERT OR IGNORE INTO analytics_salts (day, salt) VALUES (?, ?)", day, fresh)
        .execute(&mut *tx)
        .await?;
    sqlx::query!("DELETE FROM analytics_salts WHERE day < ?", day).execute(&mut *tx).await?;
    sqlx::query_scalar!("SELECT salt FROM analytics_salts WHERE day = ?", day)
        .fetch_one(&mut *tx)
        .await
}

/// An `analytics_events` row to insert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventRow {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) project_id: ProjectId,
    pub(crate) ts: UnixMillis,
    pub(crate) day: String,
    pub(crate) name: String,
    pub(crate) path: Option<String>,
    pub(crate) host: Option<String>,
    pub(crate) referrer_host: Option<String>,
    pub(crate) utm_source: Option<String>,
    pub(crate) utm_medium: Option<String>,
    pub(crate) utm_campaign: Option<String>,
    pub(crate) country: Option<String>,
    pub(crate) device: Option<String>,
    pub(crate) browser: Option<String>,
    pub(crate) os: Option<String>,
    pub(crate) visitor: i64,
    pub(crate) person: String,
    pub(crate) anon_id: Option<String>,
    pub(crate) user_id: Option<String>,
    pub(crate) props: Option<String>,
    pub(crate) source: &'static str,
}

/// Inserts an event and returns its row ID.
pub(crate) async fn insert_event(
    exec: impl SqliteExecutor<'_>,
    row: &EventRow,
) -> sqlx::Result<i64> {
    let result = sqlx::query!(
        "INSERT INTO analytics_events (workspace_id, project_id, ts, day, name, path, host,
                                       referrer_host, utm_source, utm_medium, utm_campaign,
                                       country, device, browser, os, visitor, person, anon_id,
                                       user_id, props, source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.workspace_id,
        row.project_id,
        row.ts,
        row.day,
        row.name,
        row.path,
        row.host,
        row.referrer_host,
        row.utm_source,
        row.utm_medium,
        row.utm_campaign,
        row.country,
        row.device,
        row.browser,
        row.os,
        row.visitor,
        row.person,
        row.anon_id,
        row.user_id,
        row.props,
        row.source,
    )
    .execute(exec)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Records a person as seen, made on first sight, and returns their ID.
pub(crate) async fn upsert_person(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    user_id: &str,
    now: UnixMillis,
) -> sqlx::Result<PersonId> {
    let fresh = PersonId::new();
    sqlx::query_scalar!(
        r#"INSERT INTO analytics_people (id, workspace_id, project_id, user_id, first_seen, last_seen)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT (project_id, user_id) DO UPDATE SET last_seen = max(last_seen, excluded.last_seen)
           RETURNING id AS "id!: PersonId""#,
        fresh,
        workspace_id,
        project_id,
        user_id,
        now,
        now,
    )
    .fetch_one(exec)
    .await
}

pub(crate) async fn person_traits(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    person_id: PersonId,
) -> sqlx::Result<String> {
    sqlx::query_scalar!(
        "SELECT traits FROM analytics_people WHERE id = ? AND workspace_id = ?",
        person_id,
        workspace_id,
    )
    .fetch_one(exec)
    .await
}

pub(crate) async fn set_person_traits(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    person_id: PersonId,
    traits: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE analytics_people SET traits = ? WHERE id = ? AND workspace_id = ?",
        traits,
        person_id,
        workspace_id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// The person an anonymous ID was linked to by `identify`.
pub(crate) async fn aliased_person(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    anon_id: &str,
) -> sqlx::Result<Option<PersonId>> {
    sqlx::query_scalar!(
        r#"SELECT person_id AS "person_id: PersonId" FROM analytics_aliases
           WHERE workspace_id = ? AND project_id = ? AND anon_id = ?"#,
        workspace_id,
        project_id,
        anon_id,
    )
    .fetch_optional(exec)
    .await
}

/// Links an anonymous ID to a person; a later identify relinks it.
pub(crate) async fn set_alias(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    anon_id: &str,
    person_id: PersonId,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO analytics_aliases (workspace_id, project_id, anon_id, person_id) VALUES (?, ?, ?, ?)
         ON CONFLICT (project_id, anon_id) DO UPDATE SET person_id = excluded.person_id",
        workspace_id,
        project_id,
        anon_id,
        person_id,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Moves a project's events from person key `from` to `to`.
pub(crate) async fn rekey_events(
    exec: impl SqliteExecutor<'_>,
    scope: Scope,
    from: &str,
    to: &str,
    since: UnixMillis,
) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        "UPDATE analytics_events SET person = ?
         WHERE project_id = ? AND person = ? AND ts >= ? AND workspace_id = ?",
        to,
        scope.project_id,
        from,
        since,
        scope.workspace_id,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected())
}

/// Moves conversions in the project file to a person key, so the person's
/// later conversions that day don't count as first ones. A conversion the person already has
/// stays under the old key.
pub(crate) async fn rekey_conversions(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    from: &str,
    to: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE OR IGNORE analytics_conversions SET person = ? WHERE workspace_id = ? AND person = ?",
        to,
        workspace_id,
        from,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Records a goal conversion. True when it is the first for the goal, day
/// and person.
pub(crate) async fn insert_conversion(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    goal_id: GoalId,
    day: &str,
    person: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "INSERT OR IGNORE INTO analytics_conversions (workspace_id, goal_id, day, person)
         VALUES (?, ?, ?, ?)",
        workspace_id,
        goal_id,
        day,
        person,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn delete_salts_before(
    exec: impl SqliteExecutor<'_>,
    day: &str,
) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM analytics_salts WHERE day < ?", day).execute(exec).await?;
    Ok(())
}

/// The earliest day with a raw event.
pub(crate) async fn first_event_day(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar!(
        "SELECT min(day) FROM analytics_events WHERE workspace_id = ? AND project_id = ?",
        workspace_id,
        project_id,
    )
    .fetch_one(exec)
    .await
}

pub(crate) async fn delete_daily(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    day: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "DELETE FROM analytics_daily WHERE workspace_id = ? AND project_id = ? AND day = ?",
        workspace_id,
        project_id,
        day,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Aggregates one day's raw events into `analytics_daily` rows for
/// `dimension` (`total`, or a [`crate::api::Dimension`] the rollup stores).
/// The value `CASE` matches the raw aggregate in `report`.
pub(crate) async fn insert_daily(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    dimension: &str,
    day: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO analytics_daily (workspace_id, project_id, dimension, day, value, visitors, pageviews, events)
         SELECT ?, ?, ?, ?, value, COUNT(DISTINCT visitor), SUM(name = 'pageview'), SUM(name <> 'pageview')
         FROM (
           SELECT COALESCE(CASE ? WHEN 'page' THEN path WHEN 'referrer' THEN referrer_host
                                  WHEN 'utm_source' THEN utm_source WHEN 'utm_medium' THEN utm_medium
                                  WHEN 'utm_campaign' THEN utm_campaign WHEN 'country' THEN country
                                  WHEN 'device' THEN device WHEN 'browser' THEN browser
                                  WHEN 'os' THEN os WHEN 'event' THEN name END, '') AS value,
                  visitor, name
           FROM analytics_events
           WHERE workspace_id = ? AND project_id = ? AND day = ?
             AND CASE ? WHEN 'total' THEN 1 WHEN 'event' THEN name <> 'pageview' ELSE name = 'pageview' END)
         GROUP BY value",
        workspace_id,
        project_id,
        dimension,
        day,
        dimension,
        workspace_id,
        project_id,
        day,
        dimension,
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub(crate) async fn set_rolled_through(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    day: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE analytics_sites SET rolled_through = ?
         WHERE workspace_id = ? AND project_id = ? AND (rolled_through IS NULL OR rolled_through < ?)",
        day,
        workspace_id,
        project_id,
        day,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Deletes up to `limit` raw events from days before `before`.
pub(crate) async fn prune_events(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    before: &str,
    limit: i64,
) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        "DELETE FROM analytics_events WHERE id IN (
           SELECT id FROM analytics_events
           WHERE workspace_id = ? AND project_id = ? AND day < ? LIMIT ?)",
        workspace_id,
        project_id,
        before,
        limit,
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected())
}

/// Deletes conversions from days before `before`. A project file holds
/// only its project's goals.
pub(crate) async fn prune_conversions(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    before: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "DELETE FROM analytics_conversions WHERE workspace_id = ? AND day < ?",
        workspace_id,
        before,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Deletes a goal's conversions from its project file; the goal row is in
/// the app database, so no foreign key does this.
pub(crate) async fn delete_goal_conversions(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    goal_id: GoalId,
) -> sqlx::Result<()> {
    sqlx::query!(
        "DELETE FROM analytics_conversions WHERE workspace_id = ? AND goal_id = ?",
        workspace_id,
        goal_id,
    )
    .execute(exec)
    .await?;
    Ok(())
}
