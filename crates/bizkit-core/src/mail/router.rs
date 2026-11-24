use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::Context as _;

use super::settings;
use crate::Platform;
use crate::deps::{Mail, MailError, Mailer, ThrottledMailer};
use crate::ids::WorkspaceId;
use crate::time::UnixMillis;

/// How long a built workspace mailer is reused. SMTP mailers pin the
/// address the host resolved to, so this is also how long a DNS change
/// takes to apply.
const CACHE_LIFE: Duration = Duration::from_mins(15);

/// Picks the mailer for each mail.
///
/// That is the workspace's own account when `Mail::workspace_id` names a
/// workspace that has one, else the instance mailer. Workspace mailers are built on demand and cached by workspace
/// and the settings' `updated_at`, so a save takes effect on the next send.
/// Each mailer has its own `mail.rate_per_second` budget.
#[derive(Debug, Default)]
pub struct MailRouter {
    cache: Mutex<HashMap<WorkspaceId, Cached>>,
    /// Held while a mailer is built, so concurrent cache misses build one
    /// mailer (and one rate budget) instead of one each.
    building: tokio::sync::Mutex<()>,
}

#[derive(Debug)]
struct Cached {
    updated_at: UnixMillis,
    built_at: UnixMillis,
    mailer: Arc<dyn Mailer>,
}

fn transient(what: &str) -> impl FnOnce(anyhow::Error) -> MailError + '_ {
    move |err| MailError::Transient(format!("{what}: {err:#}"))
}

impl MailRouter {
    /// Sends `mail` through the mailer [`MailRouter`] picks.
    ///
    /// # Errors
    ///
    /// The mailer's error, or [`MailError::Transient`] when the workspace
    /// has settings that can't be used right now (no secrets key, a host
    /// that doesn't resolve). Such mail never falls back to the instance
    /// mailer, which would send it under the wrong identity.
    pub async fn send(
        &self,
        platform: &Platform,
        mail: &Mail,
    ) -> Result<Option<String>, MailError> {
        let workspace = match mail.workspace_id {
            Some(workspace_id) => self.workspace_mailer(platform, workspace_id).await?,
            None => None,
        };
        let mailer = workspace.unwrap_or_else(|| Arc::clone(&platform.deps().mailer));
        mailer.send(mail).await
    }

    /// The workspace's own mailer, or `None` when it has no settings.
    ///
    /// # Errors
    ///
    /// [`MailError::Transient`] when the settings can't be read or used.
    pub async fn workspace_mailer(
        &self,
        platform: &Platform,
        workspace_id: WorkspaceId,
    ) -> Result<Option<Arc<dyn Mailer>>, MailError> {
        let db = platform.db().read();
        let updated_at = settings::updated_at(db, workspace_id)
            .await
            .map_err(|err| transient("reading the workspace mail settings")(err.into()))?;
        let Some(updated_at) = updated_at else {
            self.lock().remove(&workspace_id);
            return Ok(None);
        };
        let now = platform.now();
        if let Some(mailer) = self.cached(workspace_id, updated_at, now) {
            return Ok(Some(mailer));
        }
        let _building = self.building.lock().await;
        if let Some(mailer) = self.cached(workspace_id, updated_at, now) {
            return Ok(Some(mailer));
        }
        let Some(row) = settings::find(db, workspace_id)
            .await
            .map_err(|err| transient("reading the workspace mail settings")(err.into()))?
        else {
            return Ok(None);
        };
        let secrets = platform.secrets().ok_or_else(|| {
            MailError::Transient(
                "the workspace has its own mail settings, but this instance has no secrets key \
                 (secrets_key_env)"
                    .to_owned(),
            )
        })?;
        let account =
            row.account(secrets).map_err(transient("the workspace mail settings can't be used"))?;
        let built = platform
            .deps()
            .mail_transports
            .build(&account)
            .await
            .map_err(transient("connecting to the workspace mail account"))?;
        let mailer: Arc<dyn Mailer> = match platform.config().mail.rate_per_second {
            Some(rate) => Arc::new(ThrottledMailer::new(built, rate)),
            None => built,
        };
        self.lock().insert(
            workspace_id,
            Cached { updated_at: row.updated_at, built_at: now, mailer: Arc::clone(&mailer) },
        );
        Ok(Some(mailer))
    }

    /// The Mailgun webhook signing key the workspace saved, or `None`
    /// (also when the instance has no secrets key to open it).
    ///
    /// # Errors
    ///
    /// When the settings can't be read or opened.
    pub async fn webhook_signing_key(
        &self,
        platform: &Platform,
        workspace_id: WorkspaceId,
    ) -> anyhow::Result<Option<String>> {
        let Some(row) = settings::find(platform.db().read(), workspace_id)
            .await
            .with_context(|| format!("reading the mail settings of workspace {workspace_id}"))?
        else {
            return Ok(None);
        };
        let Some(secrets) = platform.secrets() else {
            tracing::warn!(
                %workspace_id,
                "a Mailgun event arrived for a workspace account, but this instance has no secrets key (secrets_key_env)"
            );
            return Ok(None);
        };
        let opened = settings::open(secrets, workspace_id, &row.secrets)?;
        Ok(opened.mailgun_webhook_signing_key.filter(|key| !key.trim().is_empty()))
    }

    fn cached(
        &self,
        workspace_id: WorkspaceId,
        updated_at: UnixMillis,
        now: UnixMillis,
    ) -> Option<Arc<dyn Mailer>> {
        self.lock()
            .get(&workspace_id)
            .filter(|cached| {
                cached.updated_at == updated_at && cached.built_at > now.minus(CACHE_LIFE)
            })
            .map(|cached| Arc::clone(&cached.mailer))
    }

    /// Drops the cached mailer, after the settings changed.
    pub(crate) fn forget(&self, workspace_id: WorkspaceId) {
        self.lock().remove(&workspace_id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<WorkspaceId, Cached>> {
        // The map holds only rebuildable mailers; a poisoned one is still usable.
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
