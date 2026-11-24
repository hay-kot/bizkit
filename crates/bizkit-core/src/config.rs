//! The `server` section of the config file.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use serde::Deserialize;
use url::{Host, Url};

use crate::ops::Role;

/// Server settings. Command-line flags fill `listen`, `base_url` and `data_dir`
/// before [`crate::App::build`] sees them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The address to listen on. Default `127.0.0.1:8484`.
    pub listen: Option<SocketAddr>,
    /// The public URL of the server, used in links and the CSRF check.
    /// Default `http://<bound addr>`.
    pub base_url: Option<Url>,
    /// Where the database and blobs live. Required by `App::build`.
    pub data_dir: Option<PathBuf>,
    /// Outgoing mail.
    pub mail: MailConfig,
    /// Sign-in and sessions.
    pub auth: AuthConfig,
    /// How many reverse proxies in front of bizkit append to
    /// `X-Forwarded-For`. With 0 the header is ignored; with n the client
    /// address is the nth entry from the right (1 behind one proxy, 2 behind
    /// a CDN and a load balancer). Set more than you have, and clients can
    /// claim any address.
    pub trusted_proxies: u8,
    /// Each module's own section, keyed by module name.
    pub modules: BTreeMap<String, serde_json::Value>,
    /// The name of the environment variable that holds the instance secrets
    /// key: 32 random bytes in standard base64 (`openssl rand -base64 32`).
    /// It seals credentials at rest, such as workspace mail passwords.
    /// Without it, workspaces can't save their own mail settings.
    pub secrets_key_env: Option<String>,
    /// The secrets key itself, read from `secrets_key_env` by `serve`.
    /// Never read from the file.
    #[serde(skip)]
    pub secrets_key: Option<String>,
}

/// Outgoing mail settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MailConfig {
    /// The `From` header, such as `bizkit <noreply@example.com>`.
    pub from: String,
    /// `smtp://` or `smtps://` URL. Without it, mail goes to the log.
    pub smtp_url: Option<Url>,
    /// The name of the environment variable that holds the SMTP password.
    pub smtp_password_env: Option<String>,
    /// Which service sends mail. Default `smtp`.
    pub provider: MailProvider,
    /// The most mails sent per second across the whole process, 1 to 1000.
    /// Unset means no cap.
    pub rate_per_second: Option<u32>,
    /// Mailgun settings, required with `provider: mailgun`.
    pub mailgun: Option<MailgunConfig>,
    /// The name of the environment variable that holds the Mailgun webhook
    /// signing key. Set it with any provider, so SMTP relayed through
    /// Mailgun gets bounce and complaint events too.
    pub webhook_signing_key_env: Option<String>,
    /// The webhook signing key itself, read from `webhook_signing_key_env`
    /// by `serve`. Never read from the file.
    #[serde(skip)]
    pub webhook_signing_key: Option<String>,
}

impl Default for MailConfig {
    fn default() -> Self {
        Self {
            from: "bizkit <noreply@localhost>".to_owned(),
            smtp_url: None,
            smtp_password_env: None,
            provider: MailProvider::default(),
            rate_per_second: None,
            mailgun: None,
            webhook_signing_key_env: None,
            webhook_signing_key: None,
        }
    }
}

/// The service that sends mail.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailProvider {
    /// SMTP through `smtp_url`, or the log without it.
    #[default]
    Smtp,
    /// Mailgun's HTTP API, with the `mailgun` section.
    Mailgun,
}

/// Mailgun's HTTP API settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailgunConfig {
    /// The sending domain, such as `mg.example.com`.
    pub domain: String,
    /// The region the domain lives in. Default `us`.
    #[serde(default)]
    pub region: MailgunRegion,
    /// The name of the environment variable that holds the API key.
    pub api_key_env: String,
}

/// The Mailgun region, which picks the API host.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, serde::Serialize, utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum MailgunRegion {
    /// `api.mailgun.net`.
    #[default]
    Us,
    /// `api.eu.mailgun.net`.
    Eu,
}

/// Sign-in settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthConfig {
    /// How long a session lasts without use.
    pub session_ttl_days: u32,
    /// Who may have an account. First match wins. With an empty list, only
    /// invited emails can sign up.
    pub policies: Vec<PolicyConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self { session_ttl_days: 30, policies: Vec::new() }
    }
}

/// One email policy.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    /// Required and unique, for log messages.
    pub name: String,
    /// Case-insensitive globs on the normalized email.
    pub matches: Vec<String>,
    /// Whether a matching new email may create an account.
    #[serde(default = "default_true")]
    pub sign_up: bool,
    /// Workspaces a new account joins once, when it is created. Only
    /// workspaces that already exist; joins never create one.
    #[serde(default)]
    pub join: Vec<JoinConfig>,
}

const fn default_true() -> bool {
    true
}

/// A workspace a policy joins new accounts to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinConfig {
    /// The workspace slug.
    pub workspace: String,
    /// The role the account gets.
    pub role: Role,
}

/// Every problem `Config::validate` found.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct ConfigErrors(pub Vec<String>);

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid config: {}", self.0.join("; "))
    }
}

impl Config {
    /// Checks the whole config, returning warnings when it is usable and
    /// every error at once when it is not.
    ///
    /// # Errors
    ///
    /// Every problem found, at once.
    pub fn validate(&self) -> Result<Vec<String>, ConfigErrors> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if let Some(base_url) = &self.base_url {
            check_base_url(base_url, &mut errors);
        }
        if let Some(smtp_url) = &self.mail.smtp_url {
            check_smtp_url(smtp_url, &mut errors);
        }
        check_mail_provider(&self.mail, &mut errors);
        if self.auth.session_ttl_days == 0 {
            errors.push("auth.session_ttl_days must be at least 1".to_owned());
        }
        check_policies(&self.auth.policies, &mut errors, &mut warnings);

        if errors.is_empty() { Ok(warnings) } else { Err(ConfigErrors(errors)) }
    }
}

fn check_base_url(base_url: &Url, errors: &mut Vec<String>) {
    match base_url.scheme() {
        "https" => {}
        "http" if is_loopback(base_url) => {}
        "http" => {
            errors.push(format!("base_url {base_url} must use https unless the host is loopback"));
        }
        other => errors.push(format!("base_url scheme {other} must be http or https")),
    }
}

fn check_smtp_url(smtp_url: &Url, errors: &mut Vec<String>) {
    match smtp_url.scheme() {
        "smtps" => {}
        "smtp" => {
            let tls_required = smtp_url.query_pairs().any(|(k, v)| k == "tls" && v == "required");
            if !tls_required && !is_loopback(smtp_url) {
                errors.push(
                    "mail.smtp_url: plain smtp:// off loopback needs ?tls=required, or use smtps://"
                        .to_owned(),
                );
            }
        }
        other => errors.push(format!("mail.smtp_url scheme {other} must be smtp or smtps")),
    }
}

fn check_mail_provider(mail: &MailConfig, errors: &mut Vec<String>) {
    if let Some(rate) = mail.rate_per_second
        && !(1..=1000).contains(&rate)
    {
        errors.push(format!("mail.rate_per_second {rate} must be 1 to 1000"));
    }
    match (mail.provider, &mail.mailgun) {
        (MailProvider::Mailgun, None) => {
            errors.push("mail.provider is mailgun but mail.mailgun is unset".to_owned());
        }
        (MailProvider::Mailgun, Some(mailgun)) => {
            if mail.smtp_url.is_some() {
                errors.push("mail.provider is mailgun, so mail.smtp_url must be unset".to_owned());
            }
            let domain_ok = !mailgun.domain.is_empty()
                && mailgun
                    .domain
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
            if !domain_ok {
                errors.push(format!("mail.mailgun.domain {:?} is not a domain", mailgun.domain));
            }
        }
        (MailProvider::Smtp, Some(_)) => {
            errors.push("mail.mailgun is set but mail.provider is not mailgun".to_owned());
        }
        (MailProvider::Smtp, None) => {}
    }
}

fn check_policies(policies: &[PolicyConfig], errors: &mut Vec<String>, warnings: &mut Vec<String>) {
    if policies.is_empty() {
        warnings.push("auth.policies is empty: only invited emails can sign up".to_owned());
    }
    let mut names = BTreeSet::new();
    let mut catch_all: Option<&str> = None;
    for (i, policy) in policies.iter().enumerate() {
        let label = if policy.name.trim().is_empty() {
            errors.push(format!("auth.policies[{i}]: name is required"));
            format!("auth.policies[{i}]")
        } else {
            if !names.insert(policy.name.as_str()) {
                errors.push(format!("auth.policies: duplicate name {}", policy.name));
            }
            format!("policy {}", policy.name)
        };
        if policy.matches.is_empty() {
            errors.push(format!("{label}: matches must not be empty"));
        }
        for pattern in &policy.matches {
            if let Err(err) = crate::auth::policies::compile_glob(pattern) {
                errors.push(format!("{label}: bad glob {pattern:?}: {err}"));
            }
        }
        if let Some(earlier) = catch_all {
            warnings
                .push(format!("{label} never matches: policy {earlier} above it matches everyone"));
        } else if policy.matches.iter().any(|m| m == "*" || m == "*@*") {
            catch_all = Some(policy.name.as_str());
        }
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        // Non-special schemes such as smtp:// keep IPs as opaque domains.
        Some(Host::Domain(domain)) => {
            domain == "localhost"
                || domain
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        }
        Some(Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
        Some(Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(name: &str, matches: &[&str]) -> PolicyConfig {
        PolicyConfig {
            name: name.to_owned(),
            matches: matches.iter().map(|&m| m.to_owned()).collect(),
            sign_up: true,
            join: Vec::new(),
        }
    }

    fn config(policies: Vec<PolicyConfig>) -> Config {
        Config { auth: AuthConfig { policies, ..AuthConfig::default() }, ..Config::default() }
    }

    #[test]
    fn reports_every_error_at_once() {
        let mut config = config(vec![policy("", &[]), policy("a", &["[bad"]), policy("a", &["*"])]);
        config.base_url = Some("http://example.com".parse().unwrap());
        let errors = config.validate().unwrap_err();
        insta::assert_debug_snapshot!(errors.0, @r#"
        [
            "base_url http://example.com/ must use https unless the host is loopback",
            "auth.policies[0]: name is required",
            "auth.policies[0]: matches must not be empty",
            "policy a: bad glob \"[bad\": error parsing glob '[bad': unclosed character class; missing ']'",
            "auth.policies: duplicate name a",
        ]
        "#);
    }

    #[test]
    fn warns_about_shadowed_policies() {
        let warnings = config(vec![policy("all", &["*"]), policy("team", &["*@example.com"])])
            .validate()
            .unwrap();
        assert_eq!(
            warnings,
            ["policy team never matches: policy all above it matches everyone"],
            "warn"
        );
    }

    #[test]
    fn warns_about_an_empty_policy_list() {
        let warnings = config(Vec::new()).validate().unwrap();
        assert_eq!(warnings, ["auth.policies is empty: only invited emails can sign up"], "warn");
    }

    #[test]
    fn plain_smtp_off_loopback_needs_tls() {
        let mut config = config(vec![policy("a", &["*"])]);
        config.mail.smtp_url = Some("smtp://mail.example.com:587".parse().unwrap());
        assert!(config.validate().is_err(), "plain smtp off loopback");
        config.mail.smtp_url = Some("smtp://mail.example.com:587?tls=required".parse().unwrap());
        assert!(config.validate().is_ok(), "tls=required");
        config.mail.smtp_url = Some("smtp://127.0.0.1:1025".parse().unwrap());
        assert!(config.validate().is_ok(), "loopback");
    }

    #[test]
    fn http_base_url_on_loopback_is_fine() {
        let mut config = config(vec![policy("a", &["*"])]);
        config.base_url = Some("http://localhost:5173".parse().unwrap());
        assert!(config.validate().is_ok(), "loopback http");
    }

    #[test]
    fn mailgun_needs_its_section_and_no_smtp_url() {
        let mut config = config(vec![policy("a", &["*"])]);
        config.mail.provider = MailProvider::Mailgun;
        config.mail.rate_per_second = Some(0);
        insta::assert_debug_snapshot!(config.validate().unwrap_err().0, @r#"
        [
            "mail.rate_per_second 0 must be 1 to 1000",
            "mail.provider is mailgun but mail.mailgun is unset",
        ]
        "#);
        config.mail.rate_per_second = Some(10);
        config.mail.mailgun = Some(MailgunConfig {
            domain: "mg.example.com".to_owned(),
            region: MailgunRegion::Eu,
            api_key_env: "MAILGUN_API_KEY".to_owned(),
        });
        assert!(config.validate().is_ok(), "complete mailgun config");
        config.mail.smtp_url = Some("smtps://mail.example.com".parse().unwrap());
        assert!(config.validate().is_err(), "mailgun with smtp_url");
    }

    #[test]
    fn mailgun_section_parses_from_yaml_shape() {
        let mail: MailConfig = serde_json::from_value(serde_json::json!({
            "provider": "mailgun",
            "rate_per_second": 5,
            "mailgun": {"domain": "mg.example.com", "api_key_env": "K"},
            "webhook_signing_key_env": "W",
        }))
        .unwrap();
        assert_eq!(mail.provider, MailProvider::Mailgun, "provider");
        assert_eq!(mail.mailgun.map(|m| m.region), Some(MailgunRegion::Us), "region default");
        assert_eq!(mail.webhook_signing_key, None, "the key never comes from the file");
    }
}
