//! Project secrets at rest. New values are sealed with the instance key;
//! rows from before that stay plaintext until [`seal_plaintext`] runs at
//! startup with a key.

use std::collections::BTreeMap;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::secrets::Secrets;

use crate::store::{self, StoredSecret};

/// Binds a sealed value to its row, so a value copied to another project or
/// name doesn't open.
fn aad(workspace_id: WorkspaceId, project_id: ProjectId, name: &str) -> String {
    format!("extension_secrets:{workspace_id}:{project_id}:{name}")
}

pub(crate) fn seal(
    secrets: &Secrets,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    name: &str,
    value: &str,
) -> anyhow::Result<Vec<u8>> {
    secrets.seal(value.as_bytes(), aad(workspace_id, project_id, name).as_bytes())
}

/// The secret's value, or `None` when it is sealed and doesn't open: the
/// instance has no key, or another key sealed it.
pub(crate) fn open(secrets: Option<&Secrets>, row: &StoredSecret) -> Option<String> {
    if let Some(value) = &row.value {
        return Some(value.clone());
    }
    let sealed = row.sealed.as_deref()?;
    let bytes = secrets?.open(sealed, aad(row.workspace_id, row.project_id, &row.name).as_bytes());
    String::from_utf8(bytes.ok()?).ok()
}

/// A project's secrets for a run.
#[derive(Debug, Default)]
pub(crate) struct Loaded {
    /// The values by name.
    pub(crate) values: BTreeMap<String, String>,
    /// Names whose sealed value doesn't open: no key, or another key.
    /// `ctx.secrets` throws when a script reads one, rather than handing it
    /// `undefined`, which could skip a check the secret guards.
    pub(crate) unreadable: Vec<String>,
}

pub(crate) async fn load(
    platform: &Platform,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> sqlx::Result<Loaded> {
    let rows = store::project_secrets(platform.db().read(), workspace_id, project_id).await?;
    let mut loaded = Loaded::default();
    for row in rows {
        match open(platform.secrets(), &row) {
            Some(value) => {
                loaded.values.insert(row.name, value);
            }
            None => loaded.unreadable.push(row.name),
        }
    }
    Ok(loaded)
}

/// Seals every plaintext secret when the instance has a key. Without one it
/// only warns, and the plaintext rows keep working.
///
/// Sealing drops the plaintext, so the key the instance starts with must be
/// the one it keeps: values sealed with a wrong key can't be recovered.
pub(crate) async fn seal_plaintext(platform: &Platform) -> anyhow::Result<()> {
    let Some(secrets) = platform.secrets() else {
        let count = store::count_plaintext_secrets(platform.db().read())
            .await
            .context("counting plaintext extension secrets")?;
        if count > 0 {
            tracing::warn!(
                count,
                "extension secrets are stored in plaintext; set secrets_key_env to seal them"
            );
        }
        return Ok(());
    };
    let mut tx = platform.db().write().await.context("opening a write for sealing secrets")?;
    let rows =
        store::plaintext_secrets(&mut *tx).await.context("reading plaintext extension secrets")?;
    for row in &rows {
        let Some(value) = &row.value else { continue };
        let sealed = seal(secrets, row.workspace_id, row.project_id, &row.name, value)
            .with_context(|| format!("sealing extension secret {}", row.name))?;
        store::seal_secret(&mut *tx, row, &sealed)
            .await
            .with_context(|| format!("storing sealed extension secret {}", row.name))?;
    }
    tx.commit().await.context("committing sealed extension secrets")?;
    if !rows.is_empty() {
        tracing::info!(count = rows.len(), "sealed plaintext extension secrets");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bizkit_core::secrets::SecretKey;
    use bizkit_core::time::UnixMillis;

    use super::*;

    fn row(secrets: &Secrets, name: &str, value: &str) -> StoredSecret {
        let (workspace_id, project_id) = (WorkspaceId::new(), ProjectId::new());
        StoredSecret {
            workspace_id,
            project_id,
            name: name.to_owned(),
            value: None,
            sealed: Some(seal(secrets, workspace_id, project_id, name, value).unwrap()),
            updated_at: UnixMillis(0),
        }
    }

    #[test]
    fn opens_sealed_and_plaintext_values() {
        let secrets = Secrets::new(&SecretKey::generate());
        let sealed = row(&secrets, "TOKEN", "hunter2");
        assert_eq!(open(Some(&secrets), &sealed).as_deref(), Some("hunter2"), "sealed");
        let plain = StoredSecret { value: Some("old".to_owned()), sealed: None, ..sealed.clone() };
        assert_eq!(open(None, &plain).as_deref(), Some("old"), "plaintext needs no key");
        assert_eq!(open(None, &sealed), None, "no key");
        let other = Secrets::new(&SecretKey::generate());
        assert_eq!(open(Some(&other), &sealed), None, "another key");
    }

    #[test]
    fn a_sealed_value_is_bound_to_its_row() {
        let secrets = Secrets::new(&SecretKey::generate());
        let sealed = row(&secrets, "TOKEN", "hunter2");
        let renamed = StoredSecret { name: "OTHER".to_owned(), ..sealed.clone() };
        assert_eq!(open(Some(&secrets), &renamed), None, "another name");
        let moved = StoredSecret { project_id: ProjectId::new(), ..sealed };
        assert_eq!(open(Some(&secrets), &moved), None, "another project");
    }
}
