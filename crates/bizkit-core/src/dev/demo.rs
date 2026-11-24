//! The demo data the dev stack opens with: a user, a workspace and two
//! projects, then each module's sample rows. Every step finds its row by
//! its natural key first, so seeding again changes nothing.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context as _;

use crate::auth::users;
use crate::db::Db;
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
    let user = match users::find_by_email(&mut *tx, DEMO_EMAIL).await? {
        Some(user) => user,
        None => users::insert(&mut *tx, DEMO_EMAIL, now).await.context("seeding the demo user")?,
    };
    let workspace = match workspaces::find_by_slug(&mut *tx, DEMO_WORKSPACE).await? {
        Some(workspace) => workspace,
        None => workspaces::insert(&mut *tx, DEMO_WORKSPACE, "Demo Co", now)
            .await
            .context("seeding the demo workspace")?,
    };
    members::insert(&mut *tx, workspace.id, user.id, Role::Admin, now).await?;
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
