//! Saved insights (a name, a query and a default range) and dashboards (a
//! name and an ordered list of tiles, each an insight and a width). Both
//! are app-database config.

use std::collections::HashSet;

use anyhow::Context as _;
use bizkit_core::ApiError;
use bizkit_core::ModuleContext;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Access, Method, NoContent, OpCtx, Operation, Success};
use bizkit_core::time::UnixMillis;
use bizkit_core::validate::{FieldErrors, Validate, check_name};
use serde::{Deserialize, Serialize};
use sqlx::SqliteExecutor;
use utoipa::ToSchema;

use super::query::InsightQuery;
use super::{READ, TAGS, WRITE};
use crate::api::{DashboardId, InsightId};
use crate::stats::site_scope;

/// Insights per dashboard.
pub(crate) const MAX_TILES: usize = 20;

/// Tile widths in columns of the dashboard's 12-column grid: a third, a
/// half, two thirds and the full row. The table's CHECK holds the same set.
const TILE_WIDTHS: [u8; 4] = [4, 6, 8, 12];

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(InsightsList)?;
    cx.op(InsightsCreate)?;
    cx.op(InsightsGet)?;
    cx.op(InsightsUpdate)?;
    cx.op(InsightsDelete)?;
    cx.op(DashboardsList)?;
    cx.op(DashboardsCreate)?;
    cx.op(DashboardsGet)?;
    cx.op(DashboardsUpdate)?;
    cx.op(DashboardsDelete)
}

/// The range an insight opens with, ending today. A dashboard's range
/// replaces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub(crate) enum InsightRange {
    /// Today, by hour.
    #[serde(rename = "today")]
    Today,
    /// The last 7 days.
    #[serde(rename = "7d")]
    Last7,
    /// The last 30 days.
    #[default]
    #[serde(rename = "30d")]
    Last30,
    /// The last 90 days.
    #[serde(rename = "90d")]
    Last90,
    /// This calendar month.
    #[serde(rename = "month")]
    Month,
    /// Last calendar month.
    #[serde(rename = "last-month")]
    LastMonth,
}

impl InsightRange {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Last7 => "7d",
            Self::Last30 => "30d",
            Self::Last90 => "90d",
            Self::Month => "month",
            Self::LastMonth => "last-month",
        }
    }

    fn from_stored(text: &str) -> Self {
        match text {
            "today" => Self::Today,
            "7d" => Self::Last7,
            "90d" => Self::Last90,
            "month" => Self::Month,
            "last-month" => Self::LastMonth,
            _ => Self::Last30,
        }
    }
}

/// How a saved insight draws its result. The web draws a choice the
/// result can't show (a line for a breakdown) as `auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InsightDisplay {
    /// By the result's view: a line for a series, ranked bars for a
    /// breakdown or distribution, the number for a count.
    #[default]
    Auto,
    /// A line over time. Series only.
    Line,
    /// Bars: over time for a series, ranked for a breakdown or distribution.
    Bar,
    /// The headline number with its change from the previous period.
    Number,
}

impl InsightDisplay {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Line => "line",
            Self::Bar => "bar",
            Self::Number => "number",
        }
    }

    fn from_stored(text: &str) -> Self {
        match text {
            "line" => Self::Line,
            "bar" => Self::Bar,
            "number" => Self::Number,
            _ => Self::Auto,
        }
    }
}

/// A dashboard an insight is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct DashboardRef {
    /// The dashboard ID.
    pub(crate) id: DashboardId,
    /// Its name.
    pub(crate) name: String,
}

/// A saved insight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct Insight {
    /// The insight ID.
    pub(crate) id: InsightId,
    /// The project.
    pub(crate) project_id: ProjectId,
    /// The display name.
    pub(crate) name: String,
    /// What it counts.
    pub(crate) query: InsightQuery,
    /// The range it opens with.
    pub(crate) range: InsightRange,
    /// How it draws.
    pub(crate) display: InsightDisplay,
    /// The dashboards it is on, by name.
    pub(crate) dashboards: Vec<DashboardRef>,
    /// When it was made.
    pub(crate) created_at: UnixMillis,
    /// When it last changed.
    pub(crate) updated_at: UnixMillis,
}

/// A project's insights.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct InsightList {
    /// Sorted by name.
    pub(crate) items: Vec<Insight>,
}

/// One insight on a dashboard and how wide it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tile {
    /// The insight, of the dashboard's project.
    pub(crate) insight_id: InsightId,
    /// Columns of a 12-column grid: 4, 6, 8 or 12. A row is as tall as its
    /// tallest tile.
    pub(crate) width: u8,
}

/// A dashboard: tiles in order, laid out left to right on a 12-column grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct Dashboard {
    /// The dashboard ID.
    pub(crate) id: DashboardId,
    /// The project.
    pub(crate) project_id: ProjectId,
    /// The display name.
    pub(crate) name: String,
    /// Its tiles, in order.
    pub(crate) tiles: Vec<Tile>,
    /// When it was made.
    pub(crate) created_at: UnixMillis,
    /// When it last changed.
    pub(crate) updated_at: UnixMillis,
}

/// A project's dashboards.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct DashboardList {
    /// Sorted by name.
    pub(crate) items: Vec<Dashboard>,
}

// ---------------------------------------------------------------- inputs

/// A project's insights or dashboards.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InProject {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
}
impl Validate for InProject {}

/// A new insight.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateInsight {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The display name.
    name: String,
    /// What it counts.
    query: InsightQuery,
    /// The range it opens with; default `30d`.
    #[serde(default)]
    range: Option<InsightRange>,
    /// How it draws; default `auto`.
    #[serde(default)]
    display: Option<InsightDisplay>,
}

impl Validate for CreateInsight {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        self.query.check(&mut errors, "query");
        errors.into_result()
    }
}

/// An insight of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InInsight {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The insight.
    insight_id: InsightId,
}
impl Validate for InInsight {}

/// Changes to an insight. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateInsight {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The insight.
    insight_id: InsightId,
    /// The display name.
    #[serde(default)]
    name: Option<String>,
    /// What it counts.
    #[serde(default)]
    query: Option<InsightQuery>,
    /// The range it opens with.
    #[serde(default)]
    range: Option<InsightRange>,
    /// How it draws.
    #[serde(default)]
    display: Option<InsightDisplay>,
}

impl Validate for UpdateInsight {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(name) = &self.name {
            check_name(&mut errors, "name", name);
        }
        if let Some(query) = &self.query {
            query.check(&mut errors, "query");
        }
        errors.into_result()
    }
}

fn check_tiles(errors: &mut FieldErrors, tiles: &[Tile]) {
    if tiles.len() > MAX_TILES {
        errors.add("tiles", format!("must have at most {MAX_TILES} insights"));
    }
    let unique: HashSet<InsightId> = tiles.iter().map(|tile| tile.insight_id).collect();
    if unique.len() != tiles.len() {
        errors.add("tiles", "must not repeat an insight");
    }
    for (at, tile) in tiles.iter().enumerate() {
        if !TILE_WIDTHS.contains(&tile.width) {
            errors.add(format!("tiles[{at}].width"), "must be 4, 6, 8 or 12");
        }
    }
}

/// A new dashboard.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateDashboard {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The project.
    project_id: ProjectId,
    /// The display name.
    name: String,
    /// Tiles of the project's insights, in order; at most 20.
    #[serde(default)]
    tiles: Vec<Tile>,
}

impl Validate for CreateDashboard {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        check_tiles(&mut errors, &self.tiles);
        errors.into_result()
    }
}

/// A dashboard of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InDashboard {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The dashboard.
    dashboard_id: DashboardId,
}
impl Validate for InDashboard {}

/// Changes to a dashboard. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateDashboard {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The dashboard.
    dashboard_id: DashboardId,
    /// The display name.
    #[serde(default)]
    name: Option<String>,
    /// The whole new list of tiles, in order; at most 20. Add, remove,
    /// reorder and resize by sending the list.
    #[serde(default)]
    tiles: Option<Vec<Tile>>,
}

impl Validate for UpdateDashboard {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(name) = &self.name {
            check_name(&mut errors, "name", name);
        }
        if let Some(tiles) = &self.tiles {
            check_tiles(&mut errors, tiles);
        }
        errors.into_result()
    }
}

// ---------------------------------------------------------------- store

struct InsightRow {
    id: InsightId,
    project_id: ProjectId,
    name: String,
    query: String,
    range: String,
    display: String,
    created_at: UnixMillis,
    updated_at: UnixMillis,
}

struct DashboardRow {
    id: DashboardId,
    project_id: ProjectId,
    name: String,
    created_at: UnixMillis,
    updated_at: UnixMillis,
}

fn insight_to_api(row: InsightRow, dashboards: Vec<DashboardRef>) -> Result<Insight, ApiError> {
    let query = serde_json::from_str(&row.query).map_err(anyhow::Error::from)?;
    Ok(Insight {
        id: row.id,
        project_id: row.project_id,
        name: row.name,
        query,
        range: InsightRange::from_stored(&row.range),
        display: InsightDisplay::from_stored(&row.display),
        dashboards,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn encode_query(query: &InsightQuery) -> Result<String, ApiError> {
    Ok(serde_json::to_string(query).map_err(anyhow::Error::from)?)
}

async fn find_insight(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    insight_id: InsightId,
) -> Result<InsightRow, ApiError> {
    sqlx::query_as!(
        InsightRow,
        r#"SELECT id AS "id: InsightId", project_id AS "project_id: ProjectId", name, query, range, display,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM analytics_insights WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        insight_id,
    )
    .fetch_optional(exec)
    .await?
    .ok_or(ApiError::NotFound)
}

/// Each insight of the project with the dashboards it is on, by dashboard name.
async fn insight_dashboards(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Vec<(InsightId, DashboardRef)>> {
    let rows = sqlx::query!(
        r#"SELECT t.insight_id AS "insight_id: InsightId", d.id AS "id: DashboardId", d.name
           FROM analytics_dashboard_tiles t JOIN analytics_dashboards d ON d.id = t.dashboard_id
           WHERE d.workspace_id = ? AND d.project_id = ? ORDER BY d.name, d.id"#,
        workspace_id,
        project_id,
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.insight_id, DashboardRef { id: row.id, name: row.name }))
        .collect())
}

fn refs_for(all: &[(InsightId, DashboardRef)], insight_id: InsightId) -> Vec<DashboardRef> {
    all.iter().filter(|(id, _)| *id == insight_id).map(|(_, dashboard)| dashboard.clone()).collect()
}

async fn save_insight(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    row: &InsightRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO analytics_insights
           (id, workspace_id, project_id, name, query, range, display, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name, query = excluded.query,
           range = excluded.range, display = excluded.display, updated_at = excluded.updated_at",
        row.id,
        workspace_id,
        row.project_id,
        row.name,
        row.query,
        row.range,
        row.display,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

async fn find_dashboard(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    dashboard_id: DashboardId,
) -> Result<DashboardRow, ApiError> {
    sqlx::query_as!(
        DashboardRow,
        r#"SELECT id AS "id: DashboardId", project_id AS "project_id: ProjectId", name,
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM analytics_dashboards WHERE workspace_id = ? AND id = ?"#,
        workspace_id,
        dashboard_id,
    )
    .fetch_optional(exec)
    .await?
    .ok_or(ApiError::NotFound)
}

/// The tiles of the project's dashboards, in order.
async fn tiles(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> Result<Vec<(DashboardId, Tile)>, ApiError> {
    let rows = sqlx::query!(
        r#"SELECT t.dashboard_id AS "dashboard_id: DashboardId", t.insight_id AS "insight_id: InsightId",
                  t.width
           FROM analytics_dashboard_tiles t JOIN analytics_dashboards d ON d.id = t.dashboard_id
           WHERE d.workspace_id = ? AND d.project_id = ? ORDER BY t.position"#,
        workspace_id,
        project_id,
    )
    .fetch_all(exec)
    .await?;
    rows.into_iter()
        .map(|row| {
            let width = u8::try_from(row.width)
                .with_context(|| format!("tile width {} out of range", row.width))?;
            Ok((row.dashboard_id, Tile { insight_id: row.insight_id, width }))
        })
        .collect()
}

fn dashboard_to_api(row: DashboardRow, tiles: &[(DashboardId, Tile)]) -> Dashboard {
    Dashboard {
        tiles: tiles.iter().filter(|(id, _)| *id == row.id).map(|(_, tile)| *tile).collect(),
        id: row.id,
        project_id: row.project_id,
        name: row.name,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

async fn save_dashboard(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    row: &DashboardRow,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO analytics_dashboards (id, workspace_id, project_id, name, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
        row.id,
        workspace_id,
        row.project_id,
        row.name,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Replaces the dashboard's tiles, after checking that every insight
/// belongs to its project.
async fn set_tiles(
    conn: &mut sqlx::SqliteConnection,
    workspace_id: WorkspaceId,
    row: &DashboardRow,
    tiles: &[Tile],
) -> Result<(), ApiError> {
    let ids: Vec<InsightId> = tiles.iter().map(|tile| tile.insight_id).collect();
    let wanted = serde_json::to_string(&ids).map_err(anyhow::Error::from)?;
    let found = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM analytics_insights
           WHERE workspace_id = ? AND project_id = ? AND id IN (SELECT value FROM json_each(?))"#,
        workspace_id,
        row.project_id,
        wanted,
    )
    .fetch_one(&mut *conn)
    .await?;
    if usize::try_from(found).ok() != Some(tiles.len()) {
        let mut errors = FieldErrors::default();
        errors.add("tiles", "must all be insights of the dashboard's project");
        return Err(errors.into());
    }
    sqlx::query!(
        "DELETE FROM analytics_dashboard_tiles WHERE workspace_id = ? AND dashboard_id = ?",
        workspace_id,
        row.id
    )
    .execute(&mut *conn)
    .await?;
    for (position, tile) in (0_i64..).zip(tiles) {
        sqlx::query!(
            "INSERT INTO analytics_dashboard_tiles (workspace_id, dashboard_id, insight_id, position, width)
             VALUES (?, ?, ?, ?, ?)",
            workspace_id,
            row.id,
            tile.insight_id,
            position,
            tile.width,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------- insight ops

struct InsightsList;
impl Operation for InsightsList {
    type Input = InProject;
    type Output = InsightList;
    const ID: &'static str = "analytics.insights.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/insights";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List a project's saved insights";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InProject) -> Result<InsightList, ApiError> {
        let db = cx.platform().db();
        let rows = sqlx::query_as!(
            InsightRow,
            r#"SELECT id AS "id: InsightId", project_id AS "project_id: ProjectId", name, query, range, display,
                      created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
               FROM analytics_insights WHERE workspace_id = ? AND project_id = ? ORDER BY name, id"#,
            input.workspace_id,
            input.project_id,
        )
        .fetch_all(db.read())
        .await?;
        let refs = insight_dashboards(db.read(), input.workspace_id, input.project_id).await?;
        let items = rows
            .into_iter()
            .map(|row| {
                let dashboards = refs_for(&refs, row.id);
                insight_to_api(row, dashboards)
            })
            .collect::<Result<_, _>>()?;
        Ok(InsightList { items })
    }
}

struct InsightsCreate;
impl Operation for InsightsCreate {
    type Input = CreateInsight;
    type Output = Insight;
    const ID: &'static str = "analytics.insights.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/insights";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str =
        "Save an insight: a name, an insight query and the range it opens with";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateInsight) -> Result<Insight, ApiError> {
        site_scope(cx.platform().db(), input.workspace_id, input.project_id).await?;
        let now = cx.now();
        let row = InsightRow {
            id: InsightId::new(),
            project_id: input.project_id,
            name: input.name.trim().to_owned(),
            query: encode_query(&input.query)?,
            range: input.range.unwrap_or_default().as_str().to_owned(),
            display: input.display.unwrap_or_default().as_str().to_owned(),
            created_at: now,
            updated_at: now,
        };
        let mut tx = cx.write().await?;
        save_insight(&mut *tx, input.workspace_id, &row).await?;
        tx.commit().await?;
        insight_to_api(row, Vec::new())
    }
}

struct InsightsGet;
impl Operation for InsightsGet {
    type Input = InInsight;
    type Output = Insight;
    const ID: &'static str = "analytics.insights.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/insights/{insight_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a saved insight and the dashboards it is on";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InInsight) -> Result<Insight, ApiError> {
        let db = cx.platform().db();
        let row = find_insight(db.read(), input.workspace_id, input.insight_id).await?;
        let refs = insight_dashboards(db.read(), input.workspace_id, row.project_id).await?;
        let dashboards = refs_for(&refs, row.id);
        insight_to_api(row, dashboards)
    }
}

struct InsightsUpdate;
impl Operation for InsightsUpdate {
    type Input = UpdateInsight;
    type Output = Insight;
    const ID: &'static str = "analytics.insights.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/insights/{insight_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Change a saved insight";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateInsight) -> Result<Insight, ApiError> {
        let mut tx = cx.write().await?;
        let mut row = find_insight(&mut *tx, input.workspace_id, input.insight_id).await?;
        if let Some(name) = input.name {
            name.trim().clone_into(&mut row.name);
        }
        if let Some(query) = &input.query {
            row.query = encode_query(query)?;
        }
        if let Some(range) = input.range {
            range.as_str().clone_into(&mut row.range);
        }
        if let Some(display) = input.display {
            display.as_str().clone_into(&mut row.display);
        }
        row.updated_at = cx.now();
        save_insight(&mut *tx, input.workspace_id, &row).await?;
        let refs = insight_dashboards(&mut *tx, input.workspace_id, row.project_id).await?;
        tx.commit().await?;
        let dashboards = refs_for(&refs, row.id);
        insight_to_api(row, dashboards)
    }
}

struct InsightsDelete;
impl Operation for InsightsDelete {
    type Input = InInsight;
    type Output = NoContent;
    const ID: &'static str = "analytics.insights.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/insights/{insight_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Delete a saved insight; it leaves every dashboard it was on";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InInsight) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let result = sqlx::query!(
            "DELETE FROM analytics_insights WHERE workspace_id = ? AND id = ?",
            input.workspace_id,
            input.insight_id,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if result.rows_affected() == 1 { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

// ---------------------------------------------------------------- dashboard ops

struct DashboardsList;
impl Operation for DashboardsList {
    type Input = InProject;
    type Output = DashboardList;
    const ID: &'static str = "analytics.dashboards.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/dashboards";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "List a project's dashboards";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InProject) -> Result<DashboardList, ApiError> {
        let db = cx.platform().db();
        let rows = sqlx::query_as!(
            DashboardRow,
            r#"SELECT id AS "id: DashboardId", project_id AS "project_id: ProjectId", name,
                      created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
               FROM analytics_dashboards WHERE workspace_id = ? AND project_id = ? ORDER BY name, id"#,
            input.workspace_id,
            input.project_id,
        )
        .fetch_all(db.read())
        .await?;
        let tiles = tiles(db.read(), input.workspace_id, input.project_id).await?;
        Ok(DashboardList {
            items: rows.into_iter().map(|row| dashboard_to_api(row, &tiles)).collect(),
        })
    }
}

struct DashboardsCreate;
impl Operation for DashboardsCreate {
    type Input = CreateDashboard;
    type Output = Dashboard;
    const ID: &'static str = "analytics.dashboards.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/dashboards";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str =
        "Make a dashboard: a name and tiles of saved insights, in order, each with a width";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateDashboard) -> Result<Dashboard, ApiError> {
        site_scope(cx.platform().db(), input.workspace_id, input.project_id).await?;
        let now = cx.now();
        let row = DashboardRow {
            id: DashboardId::new(),
            project_id: input.project_id,
            name: input.name.trim().to_owned(),
            created_at: now,
            updated_at: now,
        };
        let mut tx = cx.write().await?;
        save_dashboard(&mut *tx, input.workspace_id, &row).await?;
        set_tiles(&mut tx, input.workspace_id, &row, &input.tiles).await?;
        tx.commit().await?;
        let tiles: Vec<_> = input.tiles.iter().map(|tile| (row.id, *tile)).collect();
        Ok(dashboard_to_api(row, &tiles))
    }
}

struct DashboardsGet;
impl Operation for DashboardsGet {
    type Input = InDashboard;
    type Output = Dashboard;
    const ID: &'static str = "analytics.dashboards.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/dashboards/{dashboard_id}";
    const ACCESS: Access = READ;
    const SUMMARY: &'static str = "Show a dashboard and its tiles in order";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InDashboard) -> Result<Dashboard, ApiError> {
        let db = cx.platform().db();
        let row = find_dashboard(db.read(), input.workspace_id, input.dashboard_id).await?;
        let tiles = tiles(db.read(), input.workspace_id, row.project_id).await?;
        Ok(dashboard_to_api(row, &tiles))
    }
}

struct DashboardsUpdate;
impl Operation for DashboardsUpdate {
    type Input = UpdateDashboard;
    type Output = Dashboard;
    const ID: &'static str = "analytics.dashboards.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/dashboards/{dashboard_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str =
        "Rename a dashboard or set its tiles: the whole list, in order, with widths";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateDashboard) -> Result<Dashboard, ApiError> {
        let mut tx = cx.write().await?;
        let mut row = find_dashboard(&mut *tx, input.workspace_id, input.dashboard_id).await?;
        if let Some(name) = input.name {
            name.trim().clone_into(&mut row.name);
        }
        row.updated_at = cx.now();
        save_dashboard(&mut *tx, input.workspace_id, &row).await?;
        if let Some(new_tiles) = &input.tiles {
            set_tiles(&mut tx, input.workspace_id, &row, new_tiles).await?;
        }
        let tiles = tiles(&mut *tx, input.workspace_id, row.project_id).await?;
        tx.commit().await?;
        Ok(dashboard_to_api(row, &tiles))
    }
}

struct DashboardsDelete;
impl Operation for DashboardsDelete {
    type Input = InDashboard;
    type Output = NoContent;
    const ID: &'static str = "analytics.dashboards.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/analytics/dashboards/{dashboard_id}";
    const ACCESS: Access = WRITE;
    const SUMMARY: &'static str = "Delete a dashboard; its insights stay";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InDashboard) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let result = sqlx::query!(
            "DELETE FROM analytics_dashboards WHERE workspace_id = ? AND id = ?",
            input.workspace_id,
            input.dashboard_id,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if result.rows_affected() == 1 { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

/// Saves `insights` and a dashboard named `name` that shows them at the
/// given widths and displays, unless the project has insights already. Returns whether
/// it saved.
#[cfg(feature = "test-hooks")]
pub(crate) async fn seed_dashboard(
    conn: &mut sqlx::SqliteConnection,
    scope: crate::report::Scope,
    now: UnixMillis,
    name: &str,
    insights: &[(&str, InsightQuery, u8, InsightDisplay)],
) -> anyhow::Result<bool> {
    let has_insights = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM analytics_insights
                         WHERE workspace_id = ? AND project_id = ?) AS "has!: bool""#,
        scope.workspace_id,
        scope.project_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    if has_insights {
        return Ok(false);
    }
    let mut tiles = Vec::new();
    for (insight_name, query, width, display) in insights {
        let row = InsightRow {
            id: InsightId::new(),
            project_id: scope.project_id,
            name: (*insight_name).to_owned(),
            query: serde_json::to_string(query)?,
            range: InsightRange::default().as_str().to_owned(),
            display: display.as_str().to_owned(),
            created_at: now,
            updated_at: now,
        };
        save_insight(&mut *conn, scope.workspace_id, &row).await?;
        tiles.push(Tile { insight_id: row.id, width: *width });
    }
    let board = DashboardRow {
        id: DashboardId::new(),
        project_id: scope.project_id,
        name: name.to_owned(),
        created_at: now,
        updated_at: now,
    };
    save_dashboard(&mut *conn, scope.workspace_id, &board).await?;
    set_tiles(conn, scope.workspace_id, &board, &tiles)
        .await
        .map_err(|err| anyhow::anyhow!("seeding dashboard tiles: {err:?}"))?;
    Ok(true)
}
