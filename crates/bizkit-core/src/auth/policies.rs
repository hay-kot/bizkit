//! Email policies: which emails may have an account, and which workspaces a
//! new account joins.

use anyhow::Context as _;
use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};

use crate::config::{JoinConfig, PolicyConfig};

/// Compiles one policy glob: case-insensitive, `*` matches any characters.
///
/// # Errors
///
/// When `pattern` isn't a valid glob.
pub fn compile_glob(pattern: &str) -> Result<Glob, globset::Error> {
    GlobBuilder::new(pattern).case_insensitive(true).literal_separator(false).build()
}

/// One compiled policy.
#[derive(Debug, Clone)]
pub struct Policy {
    /// The policy name.
    pub name: String,
    /// Whether a matching new email may create an account.
    pub sign_up: bool,
    /// Workspaces a new account joins.
    pub join: Vec<JoinConfig>,
    globs: GlobSet,
}

/// The policies in config order. First match wins.
#[derive(Debug, Clone, Default)]
pub struct Policies(Vec<Policy>);

impl Policies {
    /// Compiles every policy. `Config::validate` reports bad globs first.
    ///
    /// # Errors
    ///
    /// When a glob doesn't compile.
    pub fn from_config(policies: &[PolicyConfig]) -> anyhow::Result<Self> {
        policies
            .iter()
            .map(|policy| {
                let mut set = GlobSetBuilder::new();
                for pattern in &policy.matches {
                    set.add(
                        compile_glob(pattern)
                            .with_context(|| format!("policy {}: glob {pattern:?}", policy.name))?,
                    );
                }
                Ok(Policy {
                    name: policy.name.clone(),
                    sign_up: policy.sign_up,
                    join: policy.join.clone(),
                    globs: set.build().with_context(|| format!("policy {}", policy.name))?,
                })
            })
            .collect::<anyhow::Result<_>>()
            .map(Self)
    }

    /// The first policy that matches the normalized `email`.
    #[must_use]
    pub fn match_email(&self, email: &str) -> Option<&Policy> {
        self.0.iter().find(|policy| policy.globs.is_match(email))
    }

    /// Every policy, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Policy> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policies(list: &[(&str, &[&str], bool)]) -> Policies {
        let configs: Vec<PolicyConfig> = list
            .iter()
            .map(|(name, matches, sign_up)| PolicyConfig {
                name: (*name).to_owned(),
                matches: matches.iter().map(|&m| m.to_owned()).collect(),
                sign_up: *sign_up,
                join: Vec::new(),
            })
            .collect();
        Policies::from_config(&configs).unwrap()
    }

    #[test]
    fn first_match_wins() {
        let p = policies(&[
            ("founder", &["founder@example.com"], true),
            ("team", &["*@example.com"], false),
        ]);
        assert_eq!(p.match_email("founder@example.com").unwrap().name, "founder", "exact first");
        assert_eq!(p.match_email("a@example.com").unwrap().name, "team", "glob second");
        assert!(p.match_email("a@other.com").is_none(), "no match");
    }

    #[test]
    fn globs_ignore_case() {
        let p = policies(&[("team", &["*@Example.COM"], true)]);
        assert!(p.match_email("a@example.com").is_some(), "case-insensitive");
    }

    #[test]
    fn star_spans_dots_and_plus() {
        let p = policies(&[("e2e", &["*@signup.e2e.test"], true)]);
        assert!(p.match_email("a.b+c@signup.e2e.test").is_some(), "dots and plus");
        assert!(p.match_email("a@e2e.test").is_none(), "other domain");
    }
}
