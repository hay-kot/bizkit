//! Workspace mail accounts.
//!
//! Each workspace may send its mail through its own SMTP or Mailgun account
//! and sender identity. [`MailRouter`] picks
//! the mailer for each [`Mail`]; the instance mailer (`Deps.mailer`)
//! sends system mail and mail for workspaces without an account.

pub(crate) mod ops;
mod router;
pub(crate) mod settings;

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub use self::router::MailRouter;
use crate::config::MailgunRegion;
#[cfg(doc)]
use crate::deps::Mail;
use crate::deps::{HttpClient, Mailer, MailgunMailer, SmtpMailer};
use crate::ids::WorkspaceId;
use crate::net::{EgressError, check_host, is_public_ip};

/// How long a workspace SMTP connection may wait on the server, so a dead
/// host fails a send (and the settings test) quickly.
const SMTP_TIMEOUT: Duration = Duration::from_secs(15);

/// Which service sends a workspace's mail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AccountProvider {
    /// An SMTP server.
    Smtp,
    /// Mailgun's HTTP API.
    Mailgun,
}

impl AccountProvider {
    /// The stored form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Smtp => "smtp",
            Self::Mailgun => "mailgun",
        }
    }
}

/// How an SMTP connection is encrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SmtpSecurity {
    /// TLS from the first byte (usually port 465).
    Tls,
    /// STARTTLS, required: the send fails when the server doesn't offer it
    /// (usually port 587).
    Starttls,
    /// No encryption. Only for loopback hosts, which only development
    /// builds accept.
    None,
}

impl SmtpSecurity {
    /// The stored form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tls => "tls",
            Self::Starttls => "starttls",
            Self::None => "none",
        }
    }
}

/// A workspace's mail account with its secrets opened. `Debug` leaves the
/// secrets out.
#[derive(Clone)]
pub struct MailAccount {
    /// The workspace.
    pub workspace_id: WorkspaceId,
    /// The `From` mailbox, such as `Tallyho <news@tallyho.example>`.
    pub from: Mailbox,
    /// The default `Reply-To`.
    pub reply_to: Option<Mailbox>,
    /// How mail leaves.
    pub transport: AccountTransport,
}

impl fmt::Debug for MailAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailAccount")
            .field("workspace_id", &self.workspace_id)
            .field("from", &self.from.to_string())
            .field("reply_to", &self.reply_to.as_ref().map(ToString::to_string))
            .field("transport", &self.transport)
            .finish()
    }
}

/// The provider half of a [`MailAccount`].
#[derive(Clone)]
pub enum AccountTransport {
    /// An SMTP server.
    Smtp {
        /// The host name or IP address.
        host: String,
        /// The port.
        port: u16,
        /// How the connection is encrypted.
        security: SmtpSecurity,
        /// The user name, when the server wants one.
        username: Option<String>,
        /// The password.
        password: Option<String>,
    },
    /// Mailgun's HTTP API.
    Mailgun {
        /// The sending domain.
        domain: String,
        /// The region, which picks the API host.
        region: MailgunRegion,
        /// The API key.
        api_key: String,
    },
}

impl fmt::Debug for AccountTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Smtp { host, port, security, username, password } => f
                .debug_struct("Smtp")
                .field("host", host)
                .field("port", port)
                .field("security", security)
                .field("username", username)
                .field("password", &password.as_ref().map(|_| "[redacted]"))
                .finish(),
            Self::Mailgun { domain, region, .. } => f
                .debug_struct("Mailgun")
                .field("domain", domain)
                .field("region", region)
                .field("api_key", &"[redacted]")
                .finish(),
        }
    }
}

/// Builds the mailer for a workspace account. Production uses
/// [`ProviderTransports`]; tests swap in a fake.
#[async_trait]
pub trait MailTransports: Send + Sync + fmt::Debug {
    /// A mailer that sends through `account`.
    ///
    /// # Errors
    ///
    /// When the account can't be used: a host that resolves to no allowed
    /// address, or settings the provider client refuses.
    async fn build(&self, account: &MailAccount) -> anyhow::Result<Arc<dyn Mailer>>;
}

/// Real SMTP and Mailgun mailers.
///
/// SMTP connects to an address resolved
/// here and checked to be public, and verifies TLS against the host name,
/// so a name that resolves differently later can't reach an internal
/// address.
#[derive(Debug, Clone)]
pub struct ProviderTransports {
    http: Arc<dyn HttpClient>,
}

impl ProviderTransports {
    /// Mailgun requests go through `http`.
    #[must_use]
    pub const fn new(http: Arc<dyn HttpClient>) -> Self {
        Self { http }
    }
}

#[async_trait]
impl MailTransports for ProviderTransports {
    async fn build(&self, account: &MailAccount) -> anyhow::Result<Arc<dyn Mailer>> {
        match &account.transport {
            AccountTransport::Smtp { host, port, security, username, password } => {
                let addr = resolve_smtp_host(host, *port).await?;
                let tls = match security {
                    SmtpSecurity::None => Tls::None,
                    SmtpSecurity::Tls | SmtpSecurity::Starttls => {
                        let params = TlsParameters::new(host.clone())
                            .with_context(|| format!("setting up TLS for {host}"))?;
                        if *security == SmtpSecurity::Tls {
                            Tls::Wrapper(params)
                        } else {
                            Tls::Required(params)
                        }
                    }
                };
                let mut builder =
                    AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(addr.ip().to_string())
                        .port(addr.port())
                        .tls(tls)
                        .timeout(Some(SMTP_TIMEOUT));
                if let Some(user) = username {
                    builder = builder.credentials(Credentials::new(
                        user.clone(),
                        password.clone().unwrap_or_default(),
                    ));
                }
                Ok(Arc::new(SmtpMailer::with_transport(
                    account.from.clone(),
                    account.reply_to.clone(),
                    builder.build(),
                )))
            }
            AccountTransport::Mailgun { domain, region, api_key } => {
                let mailer = MailgunMailer::with_key(
                    Arc::clone(&self.http),
                    domain,
                    *region,
                    account.from.clone(),
                    api_key,
                )?;
                Ok(Arc::new(mailer.replying_to(account.reply_to.clone())))
            }
        }
    }
}

/// Whether `ip` may serve as a workspace SMTP host: public, or loopback in
/// development builds, where the dev stack and e2e run Mailpit locally.
/// Only SMTP gets the loopback exception; HTTP egress never does.
fn allowed_smtp_ip(ip: std::net::IpAddr) -> bool {
    is_public_ip(ip) || (cfg!(feature = "test-hooks") && ip.is_loopback())
}

/// Checks an SMTP host as typed: [`check_host`], with the loopback
/// exception of [`allowed_smtp_ip`].
pub(crate) fn check_smtp_host(host: &str) -> Result<(), EgressError> {
    match check_host(host) {
        Err(EgressError::Blocked { host: blocked })
            if blocked.parse().is_ok_and(allowed_smtp_ip) =>
        {
            Ok(())
        }
        other => other,
    }
}

/// Whether `host` names the loopback interface (`localhost` or a loopback
/// IP).
pub(crate) fn is_loopback_host(host: &str) -> bool {
    let bare = host.trim().trim_start_matches('[').trim_end_matches(']');
    bare.eq_ignore_ascii_case("localhost")
        || bare.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// The address to connect to for `host`: the first allowed one, IPv4
/// first (Mailpit and many relays listen on IPv4 only).
async fn resolve_smtp_host(host: &str, port: u16) -> anyhow::Result<SocketAddr> {
    check_smtp_host(host)?;
    let bare = host.trim().trim_start_matches('[').trim_end_matches(']');
    let mut addrs: Vec<SocketAddr> = tokio::net::lookup_host((bare, port))
        .await
        .with_context(|| format!("looking up {host}"))?
        .filter(|addr| allowed_smtp_ip(addr.ip()))
        .collect();
    addrs.sort_by_key(|addr| !addr.is_ipv4());
    addrs.first().copied().ok_or_else(|| EgressError::Blocked { host: host.to_owned() }.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_leaves_secrets_out() {
        let smtp = MailAccount {
            workspace_id: WorkspaceId::new(),
            from: "a@example.com".parse().unwrap(),
            reply_to: None,
            transport: AccountTransport::Smtp {
                host: "smtp.example.com".to_owned(),
                port: 587,
                security: SmtpSecurity::Starttls,
                username: Some("ada".to_owned()),
                password: Some("hunter2".to_owned()),
            },
        };
        let mailgun = MailAccount {
            transport: AccountTransport::Mailgun {
                domain: "mg.example.com".to_owned(),
                region: MailgunRegion::Us,
                api_key: "key-hunter2".to_owned(),
            },
            ..smtp.clone()
        };
        for account in [smtp, mailgun] {
            let text = format!("{account:?}");
            assert!(!text.contains("hunter2"), "{text}");
            assert!(text.contains("redacted"), "{text}");
        }
    }

    #[test]
    fn smtp_hosts_get_loopback_only_in_dev_builds() {
        assert_eq!(check_smtp_host("smtp.example.com"), Ok(()), "name");
        assert!(check_smtp_host("10.0.0.5").is_err(), "private");
        assert!(check_smtp_host("169.254.169.254").is_err(), "link-local");
        assert_eq!(check_smtp_host("127.0.0.1").is_ok(), cfg!(feature = "test-hooks"), "loopback");
        assert!(is_loopback_host("localhost") && is_loopback_host("[::1]"), "loopback names");
        assert!(!is_loopback_host("smtp.example.com"), "public name");
    }

    #[tokio::test]
    async fn resolving_refuses_private_addresses() {
        let err = resolve_smtp_host("10.0.0.5", 25).await.unwrap_err();
        assert!(err.to_string().contains("not a public address"), "{err}");
    }
}
