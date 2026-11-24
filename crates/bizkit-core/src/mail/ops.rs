//! `mail.settings.*`: a workspace's own mail account.

use axum::http::StatusCode;
use lettre::Address;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::settings::{self, MailRow, SealedSecrets};
use super::{AccountProvider, SmtpSecurity, check_smtp_host, is_loopback_host};
use crate::auth::is_plausible_email;
use crate::config::MailgunRegion;
use crate::deps::{Mail, MailError};
use crate::error::ApiError;
use crate::ids::WorkspaceId;
use crate::module::ModuleContext;
use crate::ops::{Access, Level, Method, NoContent, OpCtx, Operation, Scope, Success};
use crate::time::UnixMillis;
use crate::validate::{FieldErrors, Validate};

const WORKSPACE_ADMIN: Access = Access::Workspace(Scope::new("workspace", Level::Admin));
const PATH: &str = "/workspaces/{workspace_id}/mail/settings";
const MAX_SECRET_CHARS: usize = 1024;
const MAX_ERROR_CHARS: usize = 500;

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(MailSettingsGet)?;
    cx.op(MailSettingsPut)?;
    cx.op(MailSettingsDelete)?;
    cx.op(MailSettingsTest)?;
    Ok(())
}

// ---------------------------------------------------------------- types

/// A workspace's mail settings and whether it can have them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkspaceMail {
    /// The workspace's own account, or null when the instance mailer sends
    /// its mail.
    pub settings: Option<MailSettings>,
    /// Whether the instance has a secrets key. Without one, saving fails.
    pub can_save: bool,
    /// The `From` of the instance mailer, which sends without settings.
    pub instance_from: String,
}

/// A workspace's own mail account. Secrets never come back: only whether
/// each is set.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MailSettings {
    /// Which service sends.
    pub provider: AccountProvider,
    /// The sender's display name.
    pub from_name: String,
    /// The sender's address.
    pub from_address: String,
    /// Where replies go, when not to the sender.
    pub reply_to: Option<String>,
    /// The SMTP server, when `provider` is `smtp`.
    pub smtp: Option<SmtpSettings>,
    /// The Mailgun account, when `provider` is `mailgun`.
    pub mailgun: Option<MailgunSettings>,
    /// When a test mail last went out with these settings. Any change
    /// clears it.
    pub verified_at: Option<UnixMillis>,
    /// When the settings last changed.
    pub updated_at: UnixMillis,
}

/// An SMTP server, without its password.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SmtpSettings {
    /// The host name.
    pub host: String,
    /// The port.
    pub port: u16,
    /// How the connection is encrypted.
    pub security: SmtpSecurity,
    /// The user name.
    pub username: Option<String>,
    /// Whether a password is saved.
    pub has_password: bool,
}

/// A Mailgun account, without its keys.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MailgunSettings {
    /// The sending domain.
    pub domain: String,
    /// The region.
    pub region: MailgunRegion,
    /// Whether an API key is saved.
    pub has_api_key: bool,
    /// Whether a webhook signing key is saved.
    pub has_webhook_signing_key: bool,
    /// Where to point the account's event webhook, so bounces and
    /// complaints reach bizkit. Null without the mailing module.
    pub webhook_url: Option<String>,
}

/// The result of a test send.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MailTestResult {
    /// Where the test went: the caller's address.
    pub to: String,
    /// Whether the account accepted the mail.
    pub sent: bool,
    /// The provider's or the connection's error, when it didn't.
    pub error: Option<String>,
    /// When the settings were verified; set by a test that sent.
    pub verified_at: Option<UnixMillis>,
}

/// The workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InWorkspace {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
}
impl Validate for InWorkspace {}

/// New mail settings, replacing any saved ones. A secret left out keeps the
/// saved one when the provider stays the same; an empty string removes it.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PutMailSettings {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// Which service sends.
    pub(crate) provider: AccountProvider,
    /// The sender's display name, 1-120 characters.
    pub(crate) from_name: String,
    /// The sender's address. The account must be allowed to send from it.
    pub(crate) from_address: String,
    /// Where replies go, when not to the sender.
    #[serde(default)]
    pub(crate) reply_to: Option<String>,
    /// Required with `provider: smtp`.
    #[serde(default)]
    pub(crate) smtp: Option<SmtpInput>,
    /// Required with `provider: mailgun`.
    #[serde(default)]
    pub(crate) mailgun: Option<MailgunInput>,
}

/// An SMTP server.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SmtpInput {
    /// The host name. It must resolve to public addresses.
    pub(crate) host: String,
    /// The port, such as 465 for `tls` or 587 for `starttls`.
    pub(crate) port: u16,
    /// How the connection is encrypted. `none` works only for local hosts
    /// in development builds.
    pub(crate) security: SmtpSecurity,
    /// The user name, when the server wants sign-in.
    #[serde(default)]
    pub(crate) username: Option<String>,
    /// The password. Write-only.
    #[serde(default)]
    pub(crate) password: Option<String>,
}

/// A Mailgun account.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MailgunInput {
    /// The sending domain, such as `mg.example.com`.
    pub(crate) domain: String,
    /// The region. Default `us`.
    #[serde(default)]
    pub(crate) region: MailgunRegion,
    /// The API key. Write-only.
    #[serde(default)]
    pub(crate) api_key: Option<String>,
    /// The webhook signing key, to accept the account's bounce and
    /// complaint events. Write-only.
    #[serde(default)]
    pub(crate) webhook_signing_key: Option<String>,
}

fn check_address(errors: &mut FieldErrors, field: &str, address: &str) {
    if !is_plausible_email(address.trim()) || address.trim().parse::<Address>().is_err() {
        errors.add(field, "must be an email address");
    }
}

fn check_secret(errors: &mut FieldErrors, field: &str, secret: Option<&String>) {
    if secret
        .is_some_and(|s| s.chars().count() > MAX_SECRET_CHARS || s.chars().any(char::is_control))
    {
        errors.add(field, format!("must be at most {MAX_SECRET_CHARS} characters, on one line"));
    }
}

fn is_domain(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 253
        && text.contains('.')
        && text.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

impl Validate for PutMailSettings {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        let name = self.from_name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            errors.add("from_name", "must be 1 to 120 characters");
        } else if name.chars().any(char::is_control) {
            errors.add("from_name", "must be on one line");
        }
        check_address(&mut errors, "from_address", &self.from_address);
        if let Some(reply_to) = &self.reply_to {
            check_address(&mut errors, "reply_to", reply_to);
        }
        match self.provider {
            AccountProvider::Smtp => {
                if self.mailgun.is_some() {
                    errors.add("mailgun", "must be unset with provider smtp");
                }
                match &self.smtp {
                    None => errors.add("smtp", "is required with provider smtp"),
                    Some(smtp) => {
                        if let Err(err) = check_smtp_host(&smtp.host) {
                            errors.add("smtp.host", err.to_string());
                        }
                        if smtp.port == 0 {
                            errors.add("smtp.port", "must be 1 to 65535");
                        }
                        if smtp.security == SmtpSecurity::None && !is_loopback_host(&smtp.host) {
                            errors.add("smtp.security", "must be tls or starttls off loopback");
                        }
                        if smtp.username.as_ref().is_some_and(|u| u.chars().count() > 254) {
                            errors.add("smtp.username", "must be at most 254 characters");
                        }
                        check_secret(&mut errors, "smtp.password", smtp.password.as_ref());
                    }
                }
            }
            AccountProvider::Mailgun => {
                if self.smtp.is_some() {
                    errors.add("smtp", "must be unset with provider mailgun");
                }
                match &self.mailgun {
                    None => errors.add("mailgun", "is required with provider mailgun"),
                    Some(mailgun) => {
                        if !is_domain(mailgun.domain.trim()) {
                            errors.add("mailgun.domain", "must be a domain such as mg.example.com");
                        }
                        check_secret(&mut errors, "mailgun.api_key", mailgun.api_key.as_ref());
                        check_secret(
                            &mut errors,
                            "mailgun.webhook_signing_key",
                            mailgun.webhook_signing_key.as_ref(),
                        );
                    }
                }
            }
        }
        errors.into_result()
    }
}

/// A test send.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestMailSettings {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
}
impl Validate for TestMailSettings {}

// ---------------------------------------------------------------- helpers

fn view(cx: &OpCtx, row: &MailRow) -> Result<MailSettings, ApiError> {
    let internal = |err: anyhow::Error| ApiError::Internal(err);
    let provider = row.provider().map_err(internal)?;
    let opened = cx
        .platform()
        .secrets()
        .and_then(|secrets| settings::open(secrets, row.workspace_id, &row.secrets).ok())
        .unwrap_or_default();
    let smtp = match provider {
        AccountProvider::Smtp => Some(SmtpSettings {
            host: row.smtp_host.clone().unwrap_or_default(),
            port: row.smtp_port.and_then(|p| u16::try_from(p).ok()).unwrap_or_default(),
            security: row.smtp_security().map_err(internal)?,
            username: row.smtp_username.clone(),
            has_password: opened.smtp_password.is_some(),
        }),
        AccountProvider::Mailgun => None,
    };
    let mailgun = match provider {
        AccountProvider::Mailgun => Some(MailgunSettings {
            domain: row.mailgun_domain.clone().unwrap_or_default(),
            region: row.mailgun_region().map_err(internal)?,
            has_api_key: opened.mailgun_api_key.is_some(),
            has_webhook_signing_key: opened.mailgun_webhook_signing_key.is_some(),
            webhook_url: webhook_url(cx, row.workspace_id),
        }),
        AccountProvider::Smtp => None,
    };
    Ok(MailSettings {
        provider,
        from_name: row.from_name.clone(),
        from_address: row.from_address.clone(),
        reply_to: row.reply_to.clone(),
        smtp,
        mailgun,
        verified_at: row.verified_at,
        updated_at: row.updated_at,
    })
}

fn webhook_url(cx: &OpCtx, workspace_id: WorkspaceId) -> Option<String> {
    let platform = cx.platform();
    if !platform.modules().contains(&"mailing") {
        return None;
    }
    let path = format!("/m/mailing/public/events/mailgun/{workspace_id}");
    platform.base_url().join(&path).ok().map(String::from)
}

/// An empty string removes the secret; a missing one keeps `kept`.
fn merge_secret(given: Option<String>, kept: Option<String>) -> Option<String> {
    match given {
        Some(text) if text.is_empty() => None,
        Some(text) => Some(text),
        None => kept,
    }
}

fn capped(text: &str) -> String {
    text.chars().take(MAX_ERROR_CHARS).collect()
}

// ---------------------------------------------------------------- operations

struct MailSettingsGet;
impl Operation for MailSettingsGet {
    type Input = InWorkspace;
    type Output = WorkspaceMail;
    const ID: &'static str = "mail.settings.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = PATH;
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "Show the workspace's mail account and sender, without secrets";
    const TAGS: &'static [&'static str] = &["mail"];

    async fn call(&self, cx: OpCtx, input: InWorkspace) -> Result<WorkspaceMail, ApiError> {
        let row = settings::find(cx.platform().db().read(), input.workspace_id).await?;
        let settings = row.as_ref().map(|row| view(&cx, row)).transpose()?;
        Ok(WorkspaceMail {
            settings,
            can_save: cx.platform().secrets().is_some(),
            instance_from: cx.platform().config().mail.from.clone(),
        })
    }
}

struct MailSettingsPut;
impl Operation for MailSettingsPut {
    type Input = PutMailSettings;
    type Output = MailSettings;
    const ID: &'static str = "mail.settings.put";
    const METHOD: Method = Method::Put;
    const PATH: &'static str = PATH;
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str =
        "Set the workspace's own mail account (SMTP or Mailgun) and sender; secrets are write-only";
    const TAGS: &'static [&'static str] = &["mail"];
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: PutMailSettings) -> Result<MailSettings, ApiError> {
        let secrets = cx.platform().secrets().ok_or_else(|| {
            ApiError::Conflict(
                "this instance has no secrets key (secrets_key_env), so workspaces can't save \
                 mail settings"
                    .to_owned(),
            )
        })?;
        let workspace_id = input.workspace_id;
        let now = cx.now();
        let mut tx = cx.write().await?;
        let existing = settings::find(&mut *tx, workspace_id).await?;
        let kept = existing
            .as_ref()
            .filter(|row| row.provider == input.provider.as_str())
            .and_then(|row| settings::open(secrets, workspace_id, &row.secrets).ok())
            .unwrap_or_default();
        let mut errors = FieldErrors::default();
        let mut sealed = SealedSecrets::default();
        let mut row = MailRow {
            workspace_id,
            provider: input.provider.as_str().to_owned(),
            from_name: input.from_name.trim().to_owned(),
            from_address: input.from_address.trim().to_owned(),
            reply_to: input.reply_to.map(|r| r.trim().to_owned()).filter(|r| !r.is_empty()),
            smtp_host: None,
            smtp_port: None,
            smtp_security: None,
            smtp_username: None,
            mailgun_domain: None,
            mailgun_region: None,
            secrets: Vec::new(),
            verified_at: None,
            created_at: existing.as_ref().map_or(now, |row| row.created_at),
            // Strictly later than the saved row, so the mailer cache and a
            // running test always see the change.
            updated_at: existing
                .as_ref()
                .map_or(now, |row| now.max(UnixMillis(row.updated_at.0.saturating_add(1)))),
        };
        if let Some(smtp) = input.smtp {
            let username = smtp.username.map(|u| u.trim().to_owned()).filter(|u| !u.is_empty());
            sealed.smtp_password = merge_secret(smtp.password, kept.smtp_password);
            if username.is_some() && sealed.smtp_password.is_none() {
                errors.add("smtp.password", "is required with a username");
            }
            row.smtp_host = Some(smtp.host.trim().to_owned());
            row.smtp_port = Some(i64::from(smtp.port));
            row.smtp_security = Some(smtp.security.as_str().to_owned());
            row.smtp_username = username;
        }
        if let Some(mailgun) = input.mailgun {
            sealed.mailgun_api_key = merge_secret(mailgun.api_key, kept.mailgun_api_key);
            sealed.mailgun_webhook_signing_key =
                merge_secret(mailgun.webhook_signing_key, kept.mailgun_webhook_signing_key);
            if sealed.mailgun_api_key.is_none() {
                errors.add("mailgun.api_key", "is required");
            }
            row.mailgun_domain = Some(mailgun.domain.trim().to_ascii_lowercase());
            row.mailgun_region = Some(
                match mailgun.region {
                    MailgunRegion::Us => "us",
                    MailgunRegion::Eu => "eu",
                }
                .to_owned(),
            );
        }
        errors.into_result()?;
        row.secrets = settings::seal(secrets, workspace_id, &sealed)?;
        settings::upsert(&mut *tx, &row).await?;
        tx.commit().await?;
        cx.platform().mail().forget(workspace_id);
        view(&cx, &row)
    }
}

struct MailSettingsDelete;
impl Operation for MailSettingsDelete {
    type Input = InWorkspace;
    type Output = NoContent;
    const ID: &'static str = "mail.settings.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = PATH;
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str =
        "Remove the workspace's mail account; the instance mailer sends its mail again";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = &["mail"];

    async fn call(&self, cx: OpCtx, input: InWorkspace) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted = settings::delete(&mut *tx, input.workspace_id).await?;
        tx.commit().await?;
        cx.platform().mail().forget(input.workspace_id);
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

struct MailSettingsTest;
impl Operation for MailSettingsTest {
    type Input = TestMailSettings;
    type Output = MailTestResult;
    const ID: &'static str = "mail.settings.test";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mail/settings/test";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str =
        "Send a test mail to yourself through the workspace's mail account";
    const TAGS: &'static [&'static str] = &["mail"];
    const ERRORS: &'static [StatusCode] = &[StatusCode::BAD_REQUEST];

    async fn call(&self, cx: OpCtx, input: TestMailSettings) -> Result<MailTestResult, ApiError> {
        let to = match cx.user() {
            Ok(user) => user.email.clone(),
            Err(_) => {
                return Err(ApiError::BadRequest(
                    "a test mail goes to the signed-in caller; API keys have no address".to_owned(),
                ));
            }
        };
        let platform = cx.platform();
        let workspace_id = input.workspace_id;
        let tested = settings::updated_at(platform.db().read(), workspace_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        let mail = Mail {
            workspace_id: Some(workspace_id),
            ..Mail::new(
                to.clone(),
                "bizkit test mail",
                "This is a test mail from bizkit. Your workspace's mail settings work.\n",
            )
        };
        let sent = match platform.mail().workspace_mailer(platform, workspace_id).await {
            Ok(Some(mailer)) => mailer.send(&mail).await.map(|_| ()),
            Ok(None) => return Err(ApiError::NotFound),
            Err(err) => Err(err),
        };
        if let Err(err) = sent {
            let text = match err {
                MailError::Permanent(text) | MailError::Transient(text) => text,
            };
            return Ok(MailTestResult {
                to,
                sent: false,
                error: Some(capped(&text)),
                verified_at: None,
            });
        }
        let now = cx.now();
        let mut tx = cx.write().await?;
        let marked = settings::mark_verified(&mut *tx, workspace_id, tested, now).await?;
        tx.commit().await?;
        Ok(MailTestResult { to, sent: true, error: None, verified_at: marked.then_some(now) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_merge() {
        let kept = || Some("old".to_owned());
        assert_eq!(merge_secret(None, kept()), kept(), "left out keeps");
        assert_eq!(merge_secret(Some(String::new()), kept()), None, "empty removes");
        assert_eq!(merge_secret(Some("new".to_owned()), kept()).as_deref(), Some("new"), "replace");
    }

    #[test]
    fn domains() {
        assert!(is_domain("mg.example.com"), "domain");
        for bad in ["", "localhost", "a..b", "-a.com", "a_b.com", "a.com/x"] {
            assert!(!is_domain(bad), "{bad}");
        }
    }
}
