//! The `workspace_mail` table and its sealed secrets.

use std::fmt;

use anyhow::Context as _;
use lettre::message::Mailbox;
use serde::{Deserialize, Serialize};
use sqlx::SqliteExecutor;

use super::{AccountProvider, AccountTransport, MailAccount, SmtpSecurity};
use crate::config::MailgunRegion;
use crate::ids::WorkspaceId;
use crate::secrets::Secrets;
use crate::time::UnixMillis;

/// A `workspace_mail` row.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MailRow {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) provider: String,
    pub(crate) from_name: String,
    pub(crate) from_address: String,
    pub(crate) reply_to: Option<String>,
    pub(crate) smtp_host: Option<String>,
    pub(crate) smtp_port: Option<i64>,
    pub(crate) smtp_security: Option<String>,
    pub(crate) smtp_username: Option<String>,
    pub(crate) mailgun_domain: Option<String>,
    pub(crate) mailgun_region: Option<String>,
    pub(crate) secrets: Vec<u8>,
    pub(crate) verified_at: Option<UnixMillis>,
    pub(crate) created_at: UnixMillis,
    pub(crate) updated_at: UnixMillis,
}

impl fmt::Debug for MailRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailRow")
            .field("workspace_id", &self.workspace_id)
            .field("provider", &self.provider)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

/// What `MailRow::secrets` holds once opened.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SealedSecrets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) smtp_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mailgun_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mailgun_webhook_signing_key: Option<String>,
}

impl fmt::Debug for SealedSecrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SealedSecrets(..)")
    }
}

fn aad(workspace_id: WorkspaceId) -> String {
    format!("workspace_mail:{workspace_id}")
}

pub(crate) fn seal(
    secrets: &Secrets,
    workspace_id: WorkspaceId,
    value: &SealedSecrets,
) -> anyhow::Result<Vec<u8>> {
    let json = serde_json::to_vec(value).context("encoding the mail secrets")?;
    secrets.seal(&json, aad(workspace_id).as_bytes())
}

pub(crate) fn open(
    secrets: &Secrets,
    workspace_id: WorkspaceId,
    sealed: &[u8],
) -> anyhow::Result<SealedSecrets> {
    let json = secrets
        .open(sealed, aad(workspace_id).as_bytes())
        .context("opening the workspace mail secrets")?;
    serde_json::from_slice(&json).context("reading the workspace mail secrets")
}

impl MailRow {
    pub(crate) fn provider(&self) -> anyhow::Result<AccountProvider> {
        match self.provider.as_str() {
            "smtp" => Ok(AccountProvider::Smtp),
            "mailgun" => Ok(AccountProvider::Mailgun),
            other => anyhow::bail!("unknown mail provider {other:?}"),
        }
    }

    pub(crate) fn smtp_security(&self) -> anyhow::Result<SmtpSecurity> {
        match self.smtp_security.as_deref() {
            Some("tls") => Ok(SmtpSecurity::Tls),
            Some("starttls") => Ok(SmtpSecurity::Starttls),
            Some("none") => Ok(SmtpSecurity::None),
            other => anyhow::bail!("unknown SMTP security {other:?}"),
        }
    }

    pub(crate) fn mailgun_region(&self) -> anyhow::Result<MailgunRegion> {
        match self.mailgun_region.as_deref() {
            Some("us") => Ok(MailgunRegion::Us),
            Some("eu") => Ok(MailgunRegion::Eu),
            other => anyhow::bail!("unknown Mailgun region {other:?}"),
        }
    }

    /// The `From` mailbox.
    pub(crate) fn sender(&self) -> anyhow::Result<Mailbox> {
        let name = self.from_name.trim();
        let address = self.from_address.parse().context("parsing the from address")?;
        Ok(Mailbox::new((!name.is_empty()).then(|| name.to_owned()), address))
    }

    /// The account, with its secrets opened.
    pub(crate) fn account(&self, secrets: &Secrets) -> anyhow::Result<MailAccount> {
        let opened = open(secrets, self.workspace_id, &self.secrets)?;
        let transport = match self.provider()? {
            AccountProvider::Smtp => AccountTransport::Smtp {
                host: self.smtp_host.clone().context("the SMTP host is missing")?,
                port: self
                    .smtp_port
                    .and_then(|port| u16::try_from(port).ok())
                    .context("the SMTP port is missing")?,
                security: self.smtp_security()?,
                username: self.smtp_username.clone(),
                password: opened.smtp_password,
            },
            AccountProvider::Mailgun => AccountTransport::Mailgun {
                domain: self.mailgun_domain.clone().context("the Mailgun domain is missing")?,
                region: self.mailgun_region()?,
                api_key: opened.mailgun_api_key.context("the Mailgun API key is missing")?,
            },
        };
        let reply_to = self
            .reply_to
            .as_deref()
            .map(str::parse)
            .transpose()
            .context("parsing the reply-to address")?;
        Ok(MailAccount {
            workspace_id: self.workspace_id,
            from: self.sender()?,
            reply_to,
            transport,
        })
    }
}

pub(crate) async fn find(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
) -> sqlx::Result<Option<MailRow>> {
    sqlx::query_as!(
        MailRow,
        r#"SELECT workspace_id AS "workspace_id: WorkspaceId", provider, from_name, from_address,
                  reply_to, smtp_host, smtp_port, smtp_security, smtp_username, mailgun_domain,
                  mailgun_region, secrets, verified_at AS "verified_at: UnixMillis",
                  created_at AS "created_at: UnixMillis", updated_at AS "updated_at: UnixMillis"
           FROM workspace_mail WHERE workspace_id = ?"#,
        workspace_id
    )
    .fetch_optional(exec)
    .await
}

/// When the row last changed, or `None` when the workspace has none.
pub(crate) async fn updated_at(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
) -> sqlx::Result<Option<UnixMillis>> {
    sqlx::query_scalar!(
        r#"SELECT updated_at AS "updated_at: UnixMillis" FROM workspace_mail WHERE workspace_id = ?"#,
        workspace_id
    )
    .fetch_optional(exec)
    .await
}

/// Inserts or replaces the row. `verified_at` resets, since what was
/// verified may have changed.
pub(crate) async fn upsert(exec: impl SqliteExecutor<'_>, row: &MailRow) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO workspace_mail (workspace_id, provider, from_name, from_address, reply_to,
             smtp_host, smtp_port, smtp_security, smtp_username, mailgun_domain, mailgun_region,
             secrets, verified_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)
         ON CONFLICT (workspace_id) DO UPDATE SET
             provider = excluded.provider, from_name = excluded.from_name,
             from_address = excluded.from_address, reply_to = excluded.reply_to,
             smtp_host = excluded.smtp_host, smtp_port = excluded.smtp_port,
             smtp_security = excluded.smtp_security, smtp_username = excluded.smtp_username,
             mailgun_domain = excluded.mailgun_domain, mailgun_region = excluded.mailgun_region,
             secrets = excluded.secrets, verified_at = NULL, updated_at = excluded.updated_at",
        row.workspace_id,
        row.provider,
        row.from_name,
        row.from_address,
        row.reply_to,
        row.smtp_host,
        row.smtp_port,
        row.smtp_security,
        row.smtp_username,
        row.mailgun_domain,
        row.mailgun_region,
        row.secrets,
        row.created_at,
        row.updated_at,
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Deletes the row; false when there was none.
pub(crate) async fn delete(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
) -> sqlx::Result<bool> {
    let done = sqlx::query!("DELETE FROM workspace_mail WHERE workspace_id = ?", workspace_id)
        .execute(exec)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Records a passed test, unless the row changed since the test read it.
pub(crate) async fn mark_verified(
    exec: impl SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    tested: UnixMillis,
    now: UnixMillis,
) -> sqlx::Result<bool> {
    let done = sqlx::query!(
        "UPDATE workspace_mail SET verified_at = ? WHERE workspace_id = ? AND updated_at = ?",
        now,
        workspace_id,
        tested,
    )
    .execute(exec)
    .await?;
    Ok(done.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::SecretKey;

    #[test]
    fn sealed_secrets_belong_to_their_workspace() {
        let secrets = Secrets::new(&SecretKey::generate());
        let ws = WorkspaceId::new();
        let value =
            SealedSecrets { smtp_password: Some("hunter2".to_owned()), ..Default::default() };
        let sealed = seal(&secrets, ws, &value).unwrap();
        assert_eq!(open(&secrets, ws, &sealed).unwrap(), value, "opens for its workspace");
        assert!(open(&secrets, WorkspaceId::new(), &sealed).is_err(), "not for another");
        assert_eq!(format!("{value:?}"), "SealedSecrets(..)", "debug hides it");
    }
}
