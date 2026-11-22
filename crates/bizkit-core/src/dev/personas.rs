//! Seeded test data. Each persona writes rows straight through the store
//! functions under a namespace, so parallel tests never share rows.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::auth::{API_KEY_PREFIX, api_keys, hash_token, random_token, sessions, users};
use crate::db::{Db, WriteTx};
use crate::ids::{ApiKeyId, InvitationId, ProjectId, UserId, WorkspaceId};
use crate::ops::{Role, ScopeAreas};
use crate::tenancy::{invitations, members, projects, workspaces};
use crate::time::UnixMillis;

/// A named set of seed data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persona {
    /// One admin and one workspace.
    Basic,
    /// An admin and a member, two projects, a full-scope API key and an open
    /// invitation to `invitee+<ns>@e2e.test`.
    Team,
}

impl Persona {
    /// Every persona.
    pub const ALL: [Self; 2] = [Self::Basic, Self::Team];

    /// The persona's name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Team => "team",
        }
    }
}

impl fmt::Display for Persona {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Persona {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.as_str() == s)
            .with_context(|| format!("unknown persona {s:?}"))
    }
}

/// What a seed wrote, for tests to find their rows by key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The persona.
    pub persona: String,
    /// The namespace in every email and slug.
    pub namespace: String,
    /// Accounts by key (`admin`, `member`).
    pub accounts: BTreeMap<String, Account>,
    /// Workspaces by key (`main`).
    pub workspaces: BTreeMap<String, SeededWorkspace>,
    /// Projects by key.
    pub projects: BTreeMap<String, SeededProject>,
    /// API keys by key.
    pub api_keys: BTreeMap<String, SeededKey>,
    /// Open invitations by key.
    pub invitations: BTreeMap<String, SeededInvitation>,
}

/// A seeded account with a live session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// The user ID.
    pub id: UserId,
    /// The email.
    pub email: String,
    /// A session token for the `bizkit_session` cookie.
    pub session_token: String,
}

/// A seeded workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeededWorkspace {
    /// The workspace ID.
    pub id: WorkspaceId,
    /// The slug.
    pub slug: String,
}

/// A seeded project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeededProject {
    /// The project ID.
    pub id: ProjectId,
    /// The slug.
    pub slug: String,
}

/// A seeded API key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeededKey {
    /// The key ID.
    pub id: ApiKeyId,
    /// The bearer token.
    pub token: String,
}

/// A seeded invitation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeededInvitation {
    /// The invitation ID.
    pub id: InvitationId,
    /// Who is invited.
    pub email: String,
}

/// What a seed needs from the platform.
#[derive(Debug, Clone, Copy)]
pub struct SeedOptions<'a> {
    /// Decides what the full-scope key holds.
    pub areas: &'a ScopeAreas,
    /// The time rows are created at.
    pub now: UnixMillis,
    /// How long seeded sessions last.
    pub session_ttl: Duration,
}

/// Seeds `persona` under `namespace`.
///
/// # Errors
///
/// When a write fails, for example because the namespace was already seeded.
pub async fn seed(
    db: &Db,
    options: &SeedOptions<'_>,
    persona: Persona,
    namespace: &str,
) -> anyhow::Result<Manifest> {
    let SeedOptions { areas, now, session_ttl } = *options;
    let mut seeder = Seeder {
        tx: db.write().await?,
        now,
        session_ttl,
        manifest: Manifest {
            persona: persona.to_string(),
            namespace: namespace.to_owned(),
            accounts: BTreeMap::new(),
            workspaces: BTreeMap::new(),
            projects: BTreeMap::new(),
            api_keys: BTreeMap::new(),
            invitations: BTreeMap::new(),
        },
    };
    match persona {
        Persona::Basic => {
            let admin = seeder.account("admin", &format!("admin+{namespace}@e2e.test")).await?;
            let workspace =
                seeder.workspace("main", &format!("basic-{namespace}"), "Basic").await?;
            seeder.member(workspace, admin, Role::Admin).await?;
        }
        Persona::Team => {
            let admin = seeder.account("admin", &format!("admin+{namespace}@e2e.test")).await?;
            let member = seeder.account("member", &format!("member+{namespace}@e2e.test")).await?;
            let workspace = seeder.workspace("main", &format!("team-{namespace}"), "Team").await?;
            seeder.member(workspace, admin, Role::Admin).await?;
            seeder.member(workspace, member, Role::Member).await?;
            seeder.project(workspace, "site", "Website").await?;
            seeder.project(workspace, "app", "App").await?;
            let all: Vec<String> = areas.iter().map(|(area, _)| format!("{area}:admin")).collect();
            seeder.api_key(workspace, admin, "full", &all).await?;
            seeder
                .invitation(workspace, admin, "invitee", &format!("invitee+{namespace}@e2e.test"))
                .await?;
        }
    }
    let Seeder { tx, manifest, .. } = seeder;
    tx.commit().await?;
    Ok(manifest)
}

struct Seeder {
    tx: WriteTx,
    now: UnixMillis,
    session_ttl: Duration,
    manifest: Manifest,
}

impl Seeder {
    async fn account(&mut self, key: &str, email: &str) -> anyhow::Result<UserId> {
        let user = users::insert(&mut *self.tx, email, self.now)
            .await
            .with_context(|| format!("seeding {email}"))?;
        let token = random_token();
        let expires_at = self.now.plus(self.session_ttl);
        let session = sessions::NewSession {
            token_hash: &hash_token(&token),
            user_id: user.id,
            expires_at,
            user_agent: Some("seed"),
        };
        sessions::insert(&mut *self.tx, session, self.now).await?;
        self.manifest.accounts.insert(
            key.to_owned(),
            Account { id: user.id, email: email.to_owned(), session_token: token },
        );
        Ok(user.id)
    }

    async fn workspace(
        &mut self,
        key: &str,
        slug: &str,
        name: &str,
    ) -> anyhow::Result<WorkspaceId> {
        let workspace = workspaces::insert(&mut *self.tx, slug, name, self.now)
            .await
            .with_context(|| format!("seeding {slug}"))?;
        self.manifest
            .workspaces
            .insert(key.to_owned(), SeededWorkspace { id: workspace.id, slug: slug.to_owned() });
        Ok(workspace.id)
    }

    async fn member(
        &mut self,
        workspace: WorkspaceId,
        user: UserId,
        role: Role,
    ) -> anyhow::Result<()> {
        members::insert(&mut *self.tx, workspace, user, role, self.now).await?;
        Ok(())
    }

    async fn project(
        &mut self,
        workspace: WorkspaceId,
        slug: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        let project = projects::insert(&mut *self.tx, workspace, slug, name, self.now).await?;
        self.manifest
            .projects
            .insert(slug.to_owned(), SeededProject { id: project.id, slug: slug.to_owned() });
        Ok(())
    }

    async fn api_key(
        &mut self,
        workspace: WorkspaceId,
        creator: UserId,
        key: &str,
        scopes: &[String],
    ) -> anyhow::Result<()> {
        let token = format!("{API_KEY_PREFIX}{}", random_token());
        let prefix: String = token.chars().take(10).collect();
        let scopes = serde_json::to_string(scopes)?;
        let secret_hash = hash_token(&token);
        let row = api_keys::insert(
            &mut *self.tx,
            &api_keys::NewApiKey {
                workspace_id: workspace,
                name: key,
                prefix: &prefix,
                secret_hash: &secret_hash,
                scopes: &scopes,
                created_by: Some(creator),
                expires_at: None,
            },
            self.now,
        )
        .await?;
        self.manifest.api_keys.insert(key.to_owned(), SeededKey { id: row.id, token });
        Ok(())
    }

    async fn invitation(
        &mut self,
        workspace: WorkspaceId,
        inviter: UserId,
        key: &str,
        email: &str,
    ) -> anyhow::Result<()> {
        let row = invitations::insert(
            &mut *self.tx,
            invitations::NewInvitation {
                workspace_id: workspace,
                email,
                role: Role::Member,
                invited_by: Some(inviter),
                expires_at: self.now.plus(Duration::from_hours(7 * 24)),
            },
            self.now,
        )
        .await?;
        self.manifest
            .invitations
            .insert(key.to_owned(), SeededInvitation { id: row.id, email: email.to_owned() });
        Ok(())
    }
}
