//! Features: the compiled-in modules, which an admin can turn off per
//! workspace. Off refuses the module's operations, MCP tools and routes in
//! that workspace; its data stays.

use serde::{Deserialize, Serialize};
use sqlx::SqliteExecutor;
use utoipa::ToSchema;

use crate::app::Platform;
use crate::error::ApiError;
use crate::ids::WorkspaceId;
use crate::module::ModuleContext;
use crate::ops::{Access, Level, Method, OpCtx, Operation, Scope};
use crate::validate::{FieldErrors, Validate};

impl Platform {
    /// True unless an admin turned `feature` off in the workspace. Public
    /// routes that find their workspace from a row check it themselves;
    /// operations and [`crate::ops::authorize_workspace`] check it for you.
    ///
    /// # Errors
    ///
    /// When the lookup fails.
    pub async fn feature_enabled(
        &self,
        workspace_id: WorkspaceId,
        feature: &str,
    ) -> anyhow::Result<bool> {
        let off = sqlx::query_scalar!(
            r#"SELECT 1 AS "off!: i64" FROM workspace_disabled_features
               WHERE workspace_id = ? AND feature = ?"#,
            workspace_id,
            feature
        )
        .fetch_optional(self.db().read())
        .await?;
        Ok(off.is_none())
    }

    /// The features an admin turned off in the workspace, sorted.
    ///
    /// # Errors
    ///
    /// When the lookup fails.
    pub async fn disabled_features(
        &self,
        workspace_id: WorkspaceId,
    ) -> anyhow::Result<Vec<String>> {
        Ok(disabled(self.db().read(), workspace_id).await?)
    }
}

async fn disabled(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar!(
        "SELECT feature FROM workspace_disabled_features WHERE workspace_id = ? ORDER BY feature",
        workspace_id
    )
    .fetch_all(exec)
    .await
}

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(FeaturesGet)?;
    cx.op(FeaturesPut)
}

/// A feature and whether it is on in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Feature {
    /// The module name, such as `forms`.
    pub name: String,
    /// What the feature covers.
    pub description: String,
    /// False when an admin turned it off.
    pub enabled: bool,
}

/// Every feature of this build, sorted by name.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeatureList {
    /// The features.
    pub items: Vec<Feature>,
}

/// The workspace whose features to show.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InWorkspaceFeatures {
    /// The workspace.
    workspace_id: WorkspaceId,
}
impl Validate for InWorkspaceFeatures {}

/// The features to turn off. Every other feature is on.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PutFeatures {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// Names of the features to turn off, such as `["mailing"]`. Empty
    /// turns every feature on. Data of a feature that is off stays, and
    /// comes back when it is on again.
    disabled: Vec<String>,
}
impl Validate for PutFeatures {}

async fn list(platform: &Platform, workspace_id: WorkspaceId) -> Result<FeatureList, ApiError> {
    let off = disabled(platform.db().read(), workspace_id).await?;
    let mut names = platform.modules().to_vec();
    names.sort_unstable();
    let items = names
        .into_iter()
        .map(|name| Feature {
            name: name.to_owned(),
            description: platform
                .scope_areas()
                .iter()
                .find_map(|(area, description)| (area == name).then_some(description))
                .unwrap_or_default()
                .to_owned(),
            enabled: !off.iter().any(|feature| feature == name),
        })
        .collect();
    Ok(FeatureList { items })
}

struct FeaturesGet;
impl Operation for FeaturesGet {
    type Input = InWorkspaceFeatures;
    type Output = FeatureList;
    const ID: &'static str = "workspaces.features.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/features";
    const ACCESS: Access = Access::Workspace(Scope::new("workspace", Level::Read));
    const SUMMARY: &'static str = "List the workspace's features and whether each is on";
    const TAGS: &'static [&'static str] = &["workspaces"];

    async fn call(&self, cx: OpCtx, input: InWorkspaceFeatures) -> Result<FeatureList, ApiError> {
        list(cx.platform(), input.workspace_id).await
    }
}

struct FeaturesPut;
impl Operation for FeaturesPut {
    type Input = PutFeatures;
    type Output = FeatureList;
    const ID: &'static str = "workspaces.features.put";
    const METHOD: Method = Method::Put;
    const PATH: &'static str = "/workspaces/{workspace_id}/features";
    const ACCESS: Access = Access::Workspace(Scope::new("workspace", Level::Admin));
    const SUMMARY: &'static str = "Turn features off in the workspace; the rest are on";
    const TAGS: &'static [&'static str] = &["workspaces"];

    async fn call(&self, cx: OpCtx, input: PutFeatures) -> Result<FeatureList, ApiError> {
        let known = cx.platform().modules();
        let mut errors = FieldErrors::default();
        for (i, name) in input.disabled.iter().enumerate() {
            if !known.contains(&name.as_str()) {
                errors.add(format!("disabled[{i}]"), format!("{name:?} is not a feature"));
            }
        }
        errors.into_result()?;
        let mut tx = cx.write().await?;
        sqlx::query!(
            "DELETE FROM workspace_disabled_features WHERE workspace_id = ?",
            input.workspace_id
        )
        .execute(&mut *tx)
        .await?;
        for name in &input.disabled {
            sqlx::query!(
                "INSERT OR IGNORE INTO workspace_disabled_features (workspace_id, feature)
                 VALUES (?, ?)",
                input.workspace_id,
                name
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        list(cx.platform(), input.workspace_id).await
    }
}
