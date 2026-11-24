//! Who may call an operation: scopes, roles and principals.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::app::Platform;
use crate::error::ApiError;
use crate::ids::{ApiKeyId, UserId, WorkspaceId};
use crate::tenancy::members;

/// How much a scope allows. `Admin` implies `Write`, which implies `Read`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Look, don't touch.
    Read,
    /// Create and change.
    Write,
    /// Manage the area itself: members, keys, settings.
    Admin,
}

impl Level {
    /// The level as it appears in scope strings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

/// A permission on one area, written `area:level` (`projects:write`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Scope {
    area: Cow<'static, str>,
    level: Level,
}

/// Why a scope string didn't parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScopeError {
    /// Not of the form `area:level`.
    #[error("scope {0:?} must look like area:read, area:write or area:admin")]
    Malformed(String),
    /// The area isn't declared by core or any module.
    #[error("scope {0:?} names an unknown area")]
    UnknownArea(String),
}

impl Scope {
    /// A scope on a static area, usable in consts.
    #[must_use]
    pub const fn new(area: &'static str, level: Level) -> Self {
        Self { area: Cow::Borrowed(area), level }
    }

    /// Parses a stored or requested scope. Unknown areas are an error.
    ///
    /// # Errors
    ///
    /// When `text` isn't `area:level` or the area isn't declared.
    pub fn parse(text: &str, areas: &ScopeAreas) -> Result<Self, ScopeError> {
        let (area, level) =
            text.split_once(':').ok_or_else(|| ScopeError::Malformed(text.to_owned()))?;
        let level = Level::parse(level).ok_or_else(|| ScopeError::Malformed(text.to_owned()))?;
        let area = areas.get(area).ok_or_else(|| ScopeError::UnknownArea(text.to_owned()))?;
        Ok(Self { area: Cow::Borrowed(area), level })
    }

    /// The area, such as `projects`.
    #[must_use]
    pub fn area(&self) -> &str {
        &self.area
    }

    /// The level.
    #[must_use]
    pub const fn level(&self) -> Level {
        self.level
    }

    /// True when holding `self` allows what `needed` asks for: same area,
    /// same or higher level.
    #[must_use]
    pub fn covers(&self, needed: &Self) -> bool {
        self.area == needed.area && self.level >= needed.level
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.area, self.level.as_str())
    }
}

/// Every declared scope area and its description.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeAreas(BTreeMap<&'static str, &'static str>);

impl ScopeAreas {
    pub(crate) fn insert(&mut self, area: &'static str, description: &'static str) -> bool {
        self.0.insert(area, description).is_none()
    }

    /// The canonical `'static` name for `area`, if declared.
    #[must_use]
    pub fn get(&self, area: &str) -> Option<&'static str> {
        self.0.get_key_value(area).map(|(&name, _)| name)
    }

    /// True when `area` is declared.
    #[must_use]
    pub fn contains(&self, area: &str) -> bool {
        self.0.contains_key(area)
    }

    /// Areas and descriptions, sorted by area.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
        self.0.iter().map(|(&a, &d)| (a, d))
    }

    /// Every scope on every area, sorted.
    #[must_use]
    pub fn all_scopes(&self) -> Vec<Scope> {
        self.0
            .keys()
            .flat_map(|&area| {
                [Level::Read, Level::Write, Level::Admin].map(|level| Scope::new(area, level))
            })
            .collect()
    }
}

/// Who may call an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    /// Anyone, signed in or not.
    Public,
    /// Any signed-in user, outside any workspace. API keys can't call these.
    User,
    /// Callers with this scope in the `{workspace_id}` the path names.
    Workspace(Scope),
}

/// A member's role in a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Every level on every area.
    Admin,
    /// Read and write on every area.
    Member,
}

impl Role {
    /// True when the role allows `scope`.
    #[must_use]
    pub fn grants(self, scope: &Scope) -> bool {
        match self {
            Self::Admin => true,
            Self::Member => scope.level <= Level::Write,
        }
    }

    /// The role as stored in the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    /// Parses the stored form.
    ///
    /// # Errors
    ///
    /// When `text` isn't `admin` or `member`.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        match text {
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            other => anyhow::bail!("unknown role {other:?}"),
        }
    }
}

/// The authenticated caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// A signed-in user, through a session cookie.
    User(UserPrincipal),
    /// An API key, bound to one workspace.
    ApiKey(KeyPrincipal),
}

/// A signed-in user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPrincipal {
    /// The user.
    pub user_id: UserId,
    /// Their normalized email.
    pub email: String,
}

/// An API key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPrincipal {
    /// The key.
    pub key_id: ApiKeyId,
    /// The one workspace the key works in.
    pub workspace_id: WorkspaceId,
    /// What the key may do there.
    pub scopes: Vec<Scope>,
}

impl KeyPrincipal {
    /// True when one of the key's scopes covers `scope`.
    #[must_use]
    pub fn grants(&self, scope: &Scope) -> bool {
        self.scopes.iter().any(|held| held.covers(scope))
    }
}

/// What the caller holds in the workspace an operation runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grant {
    /// A member, through their role.
    Role(Role),
    /// An API key, through its scopes.
    Scopes(Vec<Scope>),
}

impl Grant {
    /// True when the grant allows `scope`.
    #[must_use]
    pub fn allows(&self, scope: &Scope) -> bool {
        match self {
            Self::Role(role) => role.grants(scope),
            Self::Scopes(scopes) => scopes.iter().any(|held| held.covers(scope)),
        }
    }
}

/// The workspace an operation runs in, and the caller's grant there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAccess {
    /// The workspace from the path.
    pub workspace_id: WorkspaceId,
    /// What the caller holds there.
    pub grant: Grant,
}

/// The workspace check of an `Access::Workspace` operation, for raw routes
/// that take `{workspace_id}` in their path. Operations and raw routes share
/// it, so they answer the same way.
///
/// # Errors
///
/// - [`ApiError::Unauthenticated`] without a `principal`.
/// - [`ApiError::NotFound`] when `workspace_id` isn't a workspace ID, the
///   user isn't a member, or the API key belongs to another workspace.
/// - [`ApiError::Forbidden`] when the role or key doesn't allow `scope`.
/// - [`ApiError::Internal`] when the membership lookup fails.
pub async fn authorize_workspace(
    platform: &Platform,
    principal: Option<&Principal>,
    workspace_id: &str,
    scope: &Scope,
) -> Result<WorkspaceAccess, ApiError> {
    let Some(principal) = principal else { return Err(ApiError::Unauthenticated) };
    let workspace_id: WorkspaceId = workspace_id.parse().map_err(|_| ApiError::NotFound)?;
    let grant = match principal {
        Principal::User(user) => {
            let role = members::role_of(platform.db().read(), workspace_id, user.user_id).await?;
            Grant::Role(role.ok_or(ApiError::NotFound)?)
        }
        Principal::ApiKey(key) if key.workspace_id == workspace_id => {
            Grant::Scopes(key.scopes.clone())
        }
        Principal::ApiKey(_) => return Err(ApiError::NotFound),
    };
    if !grant.allows(scope) {
        return Err(ApiError::Forbidden(format!("missing scope {scope}")));
    }
    Ok(WorkspaceAccess { workspace_id, grant })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn areas() -> ScopeAreas {
        let mut areas = ScopeAreas::default();
        areas.insert("projects", "Projects");
        areas.insert("workspace", "The workspace");
        areas
    }

    #[test]
    fn parses_scopes() {
        let scope = Scope::parse("projects:write", &areas()).unwrap();
        assert_eq!(scope, Scope::new("projects", Level::Write), "parsed");
        assert_eq!(scope.to_string(), "projects:write", "display");
    }

    #[test]
    fn rejects_bad_scopes() {
        assert!(
            matches!(Scope::parse("projects", &areas()), Err(ScopeError::Malformed(_))),
            "no level"
        );
        assert!(
            matches!(Scope::parse("projects:own", &areas()), Err(ScopeError::Malformed(_))),
            "level"
        );
        assert!(
            matches!(Scope::parse("forms:read", &areas()), Err(ScopeError::UnknownArea(_))),
            "area"
        );
    }

    #[test]
    fn higher_levels_cover_lower_ones() {
        let write = Scope::new("projects", Level::Write);
        assert!(write.covers(&Scope::new("projects", Level::Read)), "write covers read");
        assert!(!write.covers(&Scope::new("projects", Level::Admin)), "write doesn't cover admin");
        assert!(!write.covers(&Scope::new("workspace", Level::Read)), "areas don't mix");
    }

    #[test]
    fn roles_grant_levels() {
        assert!(Role::Member.grants(&Scope::new("projects", Level::Write)), "member writes");
        assert!(!Role::Member.grants(&Scope::new("workspace", Level::Admin)), "member isn't admin");
        assert!(Role::Admin.grants(&Scope::new("workspace", Level::Admin)), "admin is admin");
    }

    proptest! {
        #[test]
        fn display_and_parse_round_trip(area in prop::sample::select(vec!["projects", "workspace"]),
                                        level in prop::sample::select(vec![Level::Read, Level::Write, Level::Admin])) {
            let scope = Scope::new(area, level);
            prop_assert_eq!(Scope::parse(&scope.to_string(), &areas()).unwrap(), scope);
        }

        #[test]
        fn parse_never_panics(text in ".*") {
            let _result = Scope::parse(&text, &areas());
        }
    }
}
