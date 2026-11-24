//! Helpers for tests that read or write a project's analytics file
//! directly.

use std::path::Path;

use bizkit_core::ids::ProjectId;
use sqlx::SqlitePool;

use crate::files::{ProjectDb, project_db_path};

/// A read-write pool on the project's file in `data_dir`, created and
/// migrated if it doesn't exist yet. Ingested events show here after
/// `TestApp::flush`.
///
/// # Errors
///
/// When the file can't be opened or migrated.
pub async fn project_pool(data_dir: &Path, project_id: ProjectId) -> anyhow::Result<SqlitePool> {
    let db = ProjectDb::open(&project_db_path(data_dir, project_id)).await?;
    Ok(db.write_pool())
}
