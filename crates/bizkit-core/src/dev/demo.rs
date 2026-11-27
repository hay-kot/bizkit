//! The demo data the dev stack opens with: a user, a workspace and its
//! projects, then each module's sample rows. Every step finds its row by
//! its natural key first, so seeding again changes nothing.
//!
//! The dev stack makes the demo's configuration with `tofu` first
//! (`dev/tofu`), through the API key [`api_key`] keeps; [`seed`] then finds
//! those rows by name and adds the sample data around them.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context as _;

use crate::auth::{API_KEY_PREFIX, api_keys, hash_token, random_token, users};
use crate::db::{Db, WriteTx};
use crate::ids::WorkspaceId;
use crate::module::{DemoSeed, Module};
use crate::ops::Role;
use crate::tenancy::{members, projects, workspaces};
use crate::time::UnixMillis;

/// The demo account. `dev/bizkit.yaml` admits every `@bizkit.test` email.
pub const DEMO_EMAIL: &str = "demo@bizkit.test";
/// The demo workspace slug.
pub const DEMO_WORKSPACE: &str = "demo";
/// `bizkit` gets the dev stack's own usage (`self_usage` in `dev/bizkit.yaml`).
const PROJECTS: [(&str, &str); 3] =
    [("website", "Website"), ("docs", "Docs"), ("bizkit", "bizkit usage")];

/// Seeds core's demo rows, then calls [`Module::seed_demo`] on each module.
///
/// # Errors
///
/// When a database write or a module's seed fails.
pub async fn seed(
    db: &Db,
    modules: &[Box<dyn Module>],
    now: UnixMillis,
    data_dir: &Path,
) -> anyhow::Result<DemoSeed> {
    let mut tx = db.write().await?;
    let (user, workspace) = demo_account(&mut tx, now).await?;
    let existing = projects::list(&mut *tx, workspace.id, 500).await?;
    let mut project_ids = BTreeMap::new();
    for (slug, name) in PROJECTS {
        let id = match existing.iter().find(|p| p.slug == slug) {
            Some(project) => project.id,
            None => projects::insert(&mut *tx, workspace.id, slug, name, now).await?.id,
        };
        project_ids.insert(slug.to_owned(), id);
    }
    tx.commit().await?;
    let mut seed = DemoSeed {
        workspace_id: workspace.id,
        project_ids,
        user_id: user.id,
        now,
        data_dir: data_dir.to_owned(),
        entities: BTreeMap::new(),
    };
    for module in modules {
        module
            .seed_demo(db, &mut seed)
            .await
            .with_context(|| format!("seeding demo data for module {}", module.name()))?;
    }
    Ok(seed)
}

/// The demo user and workspace, with the user as admin, made when missing.
async fn demo_account(
    tx: &mut WriteTx,
    now: UnixMillis,
) -> anyhow::Result<(users::User, workspaces::WorkspaceRow)> {
    let user = match users::find_by_email(&mut **tx, DEMO_EMAIL).await? {
        Some(user) => user,
        None => users::insert(&mut **tx, DEMO_EMAIL, now).await.context("seeding the demo user")?,
    };
    let workspace = match workspaces::find_by_slug(&mut **tx, DEMO_WORKSPACE).await? {
        Some(workspace) => workspace,
        None => workspaces::insert(&mut **tx, DEMO_WORKSPACE, "Demo Co", now)
            .await
            .context("seeding the demo workspace")?,
    };
    members::insert(&mut **tx, workspace.id, user.id, Role::Admin, now).await?;
    Ok((user, workspace))
}

/// The name of the demo workspace's key for `tofu`.
pub const DEMO_KEY_NAME: &str = "dev seed (tofu)";

/// The demo workspace's API key for `tofu`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoKey {
    /// The demo workspace.
    pub workspace_id: WorkspaceId,
    /// The bearer token.
    pub token: String,
    /// Whether this call minted the token.
    pub minted: bool,
}

/// Makes the demo account when missing and returns its live `tofu` key.
///
/// The key is named [`DEMO_KEY_NAME`] and holds `<area>:admin` for every
/// area. `known` is the token an earlier call returned; it is kept while it still
/// works in the demo workspace. Otherwise keys with that name are revoked
/// and a new one is minted, because only the hash of a token is stored.
///
/// # Errors
///
/// When a database write fails.
pub async fn api_key(
    db: &Db,
    areas: &crate::ops::ScopeAreas,
    known: Option<&str>,
    now: UnixMillis,
) -> anyhow::Result<DemoKey> {
    let mut tx = db.write().await?;
    let (user, workspace) = demo_account(&mut tx, now).await?;
    if let Some(token) = known {
        let found = api_keys::find_active(&mut *tx, &hash_token(token), now).await?;
        if found.is_some_and(|key| key.workspace_id == workspace.id && key.name == DEMO_KEY_NAME) {
            tx.commit().await?;
            return Ok(DemoKey {
                workspace_id: workspace.id,
                token: token.to_owned(),
                minted: false,
            });
        }
    }
    for key in api_keys::list(&mut *tx, workspace.id).await? {
        if key.name == DEMO_KEY_NAME && key.revoked_at.is_none() {
            api_keys::revoke(&mut *tx, workspace.id, key.id, now).await?;
        }
    }
    let token = format!("{API_KEY_PREFIX}{}", random_token());
    let prefix: String = token.chars().take(10).collect();
    let scopes: Vec<String> = areas.iter().map(|(area, _)| format!("{area}:admin")).collect();
    let scopes = serde_json::to_string(&scopes).context("serializing the key's scopes")?;
    api_keys::insert(
        &mut *tx,
        &api_keys::NewApiKey {
            workspace_id: workspace.id,
            name: DEMO_KEY_NAME,
            prefix: &prefix,
            secret_hash: &hash_token(&token),
            scopes: &scopes,
            created_by: Some(user.id),
            expires_at: None,
        },
        now,
    )
    .await
    .context("minting the demo key")?;
    tx.commit().await?;
    Ok(DemoKey { workspace_id: workspace.id, token, minted: true })
}
