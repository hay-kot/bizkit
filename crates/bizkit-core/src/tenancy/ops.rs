//! Core operations: platform meta, the signed-in user, workspaces, members,
//! invitations and projects.

use std::time::Duration;

use anyhow::Context as _;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::workspaces::WorkspaceRow;
use super::{invitations, members, projects, workspaces};
use crate::auth::{is_plausible_email, normalize_email, users};
use crate::error::ApiError;
use crate::ids::{InvitationId, ProjectId, UserId, WorkspaceId};
use crate::jobs::{Enqueue, Event, SendMail};
use crate::module::ModuleContext;
use crate::ops::{
    Access, Level, Method, NoContent, NoInput, OpCtx, Operation, Role, Scope, Success,
};
use crate::time::UnixMillis;
use crate::validate::{FieldErrors, Validate, check_name, check_slug};

const WORKSPACE_READ: Access = Access::Workspace(Scope::new("workspace", Level::Read));
const WORKSPACE_ADMIN: Access = Access::Workspace(Scope::new("workspace", Level::Admin));
const PROJECTS_READ: Access = Access::Workspace(Scope::new("projects", Level::Read));
const PROJECTS_WRITE: Access = Access::Workspace(Scope::new("projects", Level::Write));
const INVITE_TTL: Duration = Duration::from_hours(7 * 24);

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.scope_area("workspace", "The workspace: settings, members, invitations and API keys")?;
    cx.scope_area("projects", "Projects in the workspace")?;
    cx.op(PlatformMeta)?;
    cx.op(MeGet)?;
    cx.op(WorkspacesList)?;
    cx.op(WorkspacesCreate)?;
    cx.op(WorkspacesGet)?;
    cx.op(WorkspacesUpdate)?;
    cx.op(MembersList)?;
    cx.op(MembersUpdate)?;
    cx.op(MembersRemove)?;
    cx.op(InvitationsList)?;
    cx.op(InvitationsCreate)?;
    cx.op(InvitationsRevoke)?;
    cx.op(InvitationsMine)?;
    cx.op(InvitationsAccept)?;
    cx.op(InvitationsDecline)?;
    cx.op(ProjectsList)?;
    cx.op(ProjectsCreate)?;
    cx.op(ProjectsGet)?;
    cx.op(ProjectsUpdate)?;
    cx.op(ProjectsDelete)?;
    Ok(())
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    err.as_database_error().is_some_and(sqlx::error::DatabaseError::is_unique_violation)
}

const fn internal(err: anyhow::Error) -> ApiError {
    ApiError::Internal(err)
}

// ---------------------------------------------------------------- types

/// A workspace, the unit of tenancy.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Workspace {
    /// The workspace ID.
    pub id: WorkspaceId,
    /// The URL-safe name, unique on the instance and immutable.
    pub slug: String,
    /// The display name.
    pub name: String,
    /// When it was created.
    pub created_at: UnixMillis,
}

impl From<WorkspaceRow> for Workspace {
    fn from(row: WorkspaceRow) -> Self {
        Self { id: row.id, slug: row.slug, name: row.name, created_at: row.created_at }
    }
}

/// A workspace and the caller's role in it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Membership {
    /// The workspace.
    pub workspace: Workspace,
    /// The caller's role there.
    pub role: Role,
}

/// The workspaces the caller belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MembershipList {
    /// Sorted by workspace name.
    pub items: Vec<Membership>,
}

/// The instance.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Meta {
    /// The bizkit version.
    pub version: String,
    /// The compiled-in feature modules.
    pub modules: Vec<String>,
}

/// The signed-in user.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Me {
    /// The user ID.
    pub id: UserId,
    /// The normalized email.
    pub email: String,
    /// The display name, if set.
    pub name: Option<String>,
    /// Every workspace the user belongs to.
    pub memberships: Vec<Membership>,
}

/// A workspace member.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Member {
    /// The user ID.
    pub user_id: UserId,
    /// Their email.
    pub email: String,
    /// Their display name, if set.
    pub name: Option<String>,
    /// Their role.
    pub role: Role,
    /// When they joined.
    pub joined_at: UnixMillis,
}

impl TryFrom<members::MemberRow> for Member {
    type Error = anyhow::Error;

    fn try_from(row: members::MemberRow) -> anyhow::Result<Self> {
        Ok(Self {
            user_id: row.user_id,
            email: row.email,
            name: row.name,
            role: Role::parse(&row.role)?,
            joined_at: row.created_at,
        })
    }
}

/// The members of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MemberList {
    /// Sorted by email.
    pub items: Vec<Member>,
}

/// An invitation to a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Invitation {
    /// The invitation ID; the invite link is `/invite/{id}`.
    pub id: InvitationId,
    /// The workspace.
    pub workspace_id: WorkspaceId,
    /// Who is invited.
    pub email: String,
    /// The role they get on accepting.
    pub role: Role,
    /// When it was sent.
    pub created_at: UnixMillis,
    /// When it stops working.
    pub expires_at: UnixMillis,
}

impl TryFrom<invitations::InvitationRow> for Invitation {
    type Error = anyhow::Error;

    fn try_from(row: invitations::InvitationRow) -> anyhow::Result<Self> {
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            email: row.email,
            role: Role::parse(&row.role)?,
            created_at: row.created_at,
            expires_at: row.expires_at,
        })
    }
}

/// Pending invitations of a workspace, expired ones included.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvitationList {
    /// Newest first.
    pub items: Vec<Invitation>,
}

/// An open invitation, as the invitee sees it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MyInvitation {
    /// The invitation ID.
    pub id: InvitationId,
    /// The workspace it is for.
    pub workspace: WorkspaceRef,
    /// The role on accepting.
    pub role: Role,
    /// Who sent it, if they still have an account.
    pub invited_by: Option<String>,
    /// When it stops working.
    pub expires_at: UnixMillis,
}

/// A workspace's ID, slug and name.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkspaceRef {
    /// The workspace ID.
    pub id: WorkspaceId,
    /// The slug.
    pub slug: String,
    /// The display name.
    pub name: String,
}

/// The caller's open invitations.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MyInvitationList {
    /// Newest first.
    pub items: Vec<MyInvitation>,
}

/// A project.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Project {
    /// The project ID.
    pub id: ProjectId,
    /// The workspace it belongs to.
    pub workspace_id: WorkspaceId,
    /// The URL-safe name, unique in the workspace.
    pub slug: String,
    /// The display name.
    pub name: String,
    /// When it was created.
    pub created_at: UnixMillis,
}

impl From<projects::ProjectRow> for Project {
    fn from(row: projects::ProjectRow) -> Self {
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            slug: row.slug,
            name: row.name,
            created_at: row.created_at,
        }
    }
}

/// Projects of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProjectList {
    /// Sorted by name.
    pub items: Vec<Project>,
}

/// Published when a project is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCreated {
    /// The new project.
    pub project: Project,
}

impl Event for ProjectCreated {
    const NAME: &'static str = "projects.created";
}

// ---------------------------------------------------------------- inputs

/// Just the workspace from the path.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InWorkspace {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
}
impl Validate for InWorkspace {}

/// A new workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateWorkspace {
    /// The display name.
    pub(crate) name: String,
    /// 2-48 of `a-z`, `0-9` and `-`, starting with a letter. Can't change later.
    pub(crate) slug: String,
}
impl Validate for CreateWorkspace {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        check_slug(&mut errors, "slug", &self.slug);
        errors.into_result()
    }
}

/// A workspace's new name.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateWorkspace {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// The new display name.
    pub(crate) name: String,
}
impl Validate for UpdateWorkspace {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        errors.into_result()
    }
}

/// A member's new role.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateMember {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// The member.
    pub(crate) user_id: UserId,
    /// The new role.
    pub(crate) role: Role,
}
impl Validate for UpdateMember {}

/// A member of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InMember {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// The member.
    pub(crate) user_id: UserId,
}
impl Validate for InMember {}

/// Who to invite.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateInvitation {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// The email to invite.
    pub(crate) email: String,
    /// The role they get on accepting.
    pub(crate) role: Role,
}
impl Validate for CreateInvitation {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if !is_plausible_email(&normalize_email(&self.email)) {
            errors.add("email", "must be an email address");
        }
        errors.into_result()
    }
}

/// An invitation of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InInvitation {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// The invitation.
    pub(crate) invitation_id: InvitationId,
}
impl Validate for InInvitation {}

/// One of the caller's invitations.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InMyInvitation {
    /// The invitation.
    pub(crate) invitation_id: InvitationId,
}
impl Validate for InMyInvitation {}

/// Which projects to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListProjects {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// At most this many, 1-500. Default 100.
    #[serde(default)]
    pub(crate) limit: Option<u32>,
}
impl Validate for ListProjects {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if self.limit.is_some_and(|limit| !(1..=500).contains(&limit)) {
            errors.add("limit", "must be between 1 and 500");
        }
        errors.into_result()
    }
}

/// A new project.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateProject {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// The display name.
    pub(crate) name: String,
    /// 2-48 of `a-z`, `0-9` and `-`, unique in the workspace.
    pub(crate) slug: String,
}
impl Validate for CreateProject {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        check_slug(&mut errors, "slug", &self.slug);
        errors.into_result()
    }
}

/// A project of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InProject {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// The project.
    pub(crate) project_id: ProjectId,
}
impl Validate for InProject {}

/// A project's new name.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateProject {
    /// The workspace.
    pub(crate) workspace_id: WorkspaceId,
    /// The project.
    pub(crate) project_id: ProjectId,
    /// The new display name.
    pub(crate) name: String,
}
impl Validate for UpdateProject {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        errors.into_result()
    }
}

// ---------------------------------------------------------------- platform and me

struct PlatformMeta;
impl Operation for PlatformMeta {
    type Input = NoInput;
    type Output = Meta;
    const ID: &'static str = "platform.meta";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/meta";
    const ACCESS: Access = Access::Public;
    const SUMMARY: &'static str = "Show the bizkit version and enabled modules";
    const TAGS: &'static [&'static str] = &["platform"];

    fn call(
        &self,
        cx: OpCtx,
        _input: NoInput,
    ) -> impl Future<Output = Result<Meta, ApiError>> + Send {
        std::future::ready(Ok(Meta {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            modules: cx.platform().modules().iter().map(|&m| m.to_owned()).collect(),
        }))
    }
}

async fn memberships(cx: &OpCtx, user_id: UserId) -> Result<Vec<Membership>, ApiError> {
    workspaces::list_for_user(cx.platform().db().read(), user_id)
        .await?
        .into_iter()
        .map(|row| {
            let (workspace, role) = row.split()?;
            Ok(Membership { workspace: workspace.into(), role })
        })
        .collect::<anyhow::Result<_>>()
        .map_err(internal)
}

struct MeGet;
impl Operation for MeGet {
    type Input = NoInput;
    type Output = Me;
    const ID: &'static str = "me.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/me";
    const ACCESS: Access = Access::User;
    const SUMMARY: &'static str = "Show the signed-in user and their workspaces";
    const TAGS: &'static [&'static str] = &["me"];

    async fn call(&self, cx: OpCtx, _input: NoInput) -> Result<Me, ApiError> {
        let user_id = cx.user_id()?;
        let user = users::find(cx.platform().db().read(), user_id)
            .await?
            .ok_or(ApiError::Unauthenticated)?;
        Ok(Me {
            id: user.id,
            email: user.email,
            name: user.name,
            memberships: memberships(&cx, user_id).await?,
        })
    }
}

// ---------------------------------------------------------------- workspaces

struct WorkspacesList;
impl Operation for WorkspacesList {
    type Input = NoInput;
    type Output = MembershipList;
    const ID: &'static str = "workspaces.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces";
    const ACCESS: Access = Access::User;
    const SUMMARY: &'static str = "List the workspaces you belong to";
    const TAGS: &'static [&'static str] = &["workspaces"];

    async fn call(&self, cx: OpCtx, _input: NoInput) -> Result<MembershipList, ApiError> {
        let user_id = cx.user_id()?;
        Ok(MembershipList { items: memberships(&cx, user_id).await? })
    }
}

struct WorkspacesCreate;
impl Operation for WorkspacesCreate {
    type Input = CreateWorkspace;
    type Output = Workspace;
    const ID: &'static str = "workspaces.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces";
    const ACCESS: Access = Access::User;
    const SUMMARY: &'static str = "Create a workspace; you become its first admin";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = &["workspaces"];
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: CreateWorkspace) -> Result<Workspace, ApiError> {
        let user_id = cx.user_id()?;
        let now = cx.now();
        let mut tx = cx.write().await?;
        let workspace =
            match workspaces::insert(&mut *tx, &input.slug, input.name.trim(), now).await {
                Err(err) if is_unique_violation(&err) => {
                    return Err(ApiError::Conflict(format!("the slug {} is taken", input.slug)));
                }
                other => other?,
            };
        members::insert(&mut *tx, workspace.id, user_id, Role::Admin, now).await?;
        tx.commit().await?;
        Ok(workspace.into())
    }
}

struct WorkspacesGet;
impl Operation for WorkspacesGet {
    type Input = InWorkspace;
    type Output = Workspace;
    const ID: &'static str = "workspaces.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}";
    const ACCESS: Access = WORKSPACE_READ;
    const SUMMARY: &'static str = "Show a workspace";
    const TAGS: &'static [&'static str] = &["workspaces"];

    async fn call(&self, cx: OpCtx, input: InWorkspace) -> Result<Workspace, ApiError> {
        let workspace = workspaces::find(cx.platform().db().read(), input.workspace_id).await?;
        workspace.map(Into::into).ok_or(ApiError::NotFound)
    }
}

struct WorkspacesUpdate;
impl Operation for WorkspacesUpdate {
    type Input = UpdateWorkspace;
    type Output = Workspace;
    const ID: &'static str = "workspaces.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "Rename a workspace; the slug never changes";
    const TAGS: &'static [&'static str] = &["workspaces"];

    async fn call(&self, cx: OpCtx, input: UpdateWorkspace) -> Result<Workspace, ApiError> {
        let mut tx = cx.write().await?;
        let workspace = workspaces::rename(&mut *tx, input.workspace_id, input.name.trim()).await?;
        tx.commit().await?;
        workspace.map(Into::into).ok_or(ApiError::NotFound)
    }
}

// ---------------------------------------------------------------- members

struct MembersList;
impl Operation for MembersList {
    type Input = InWorkspace;
    type Output = MemberList;
    const ID: &'static str = "members.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/members";
    const ACCESS: Access = WORKSPACE_READ;
    const SUMMARY: &'static str = "List the members of a workspace";
    const TAGS: &'static [&'static str] = &["members"];

    async fn call(&self, cx: OpCtx, input: InWorkspace) -> Result<MemberList, ApiError> {
        let rows = members::list(cx.platform().db().read(), input.workspace_id).await?;
        let items = rows
            .into_iter()
            .map(Member::try_from)
            .collect::<anyhow::Result<_>>()
            .map_err(internal)?;
        Ok(MemberList { items })
    }
}

/// Refuses to leave a workspace without an admin.
async fn guard_last_admin(
    tx: &mut crate::db::WriteTx,
    workspace_id: WorkspaceId,
    member: &members::MemberRow,
) -> Result<(), ApiError> {
    if member.role == Role::Admin.as_str()
        && members::admin_count(&mut **tx, workspace_id).await? <= 1
    {
        return Err(ApiError::Conflict("a workspace needs at least one admin".to_owned()));
    }
    Ok(())
}

struct MembersUpdate;
impl Operation for MembersUpdate {
    type Input = UpdateMember;
    type Output = Member;
    const ID: &'static str = "members.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/members/{user_id}";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "Change a member's role";
    const TAGS: &'static [&'static str] = &["members"];
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: UpdateMember) -> Result<Member, ApiError> {
        let mut tx = cx.write().await?;
        let member = members::find(&mut *tx, input.workspace_id, input.user_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if input.role != Role::Admin {
            guard_last_admin(&mut tx, input.workspace_id, &member).await?;
        }
        members::set_role(&mut *tx, input.workspace_id, input.user_id, input.role).await?;
        let updated = members::find(&mut *tx, input.workspace_id, input.user_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        tx.commit().await?;
        Member::try_from(updated).map_err(internal)
    }
}

struct MembersRemove;
impl Operation for MembersRemove {
    type Input = InMember;
    type Output = NoContent;
    const ID: &'static str = "members.remove";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/members/{user_id}";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "Remove a member from a workspace";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = &["members"];
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: InMember) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let member = members::find(&mut *tx, input.workspace_id, input.user_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        guard_last_admin(&mut tx, input.workspace_id, &member).await?;
        members::remove(&mut *tx, input.workspace_id, input.user_id).await?;
        tx.commit().await?;
        Ok(NoContent)
    }
}

// ---------------------------------------------------------------- invitations

struct InvitationsList;
impl Operation for InvitationsList {
    type Input = InWorkspace;
    type Output = InvitationList;
    const ID: &'static str = "invitations.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/invitations";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "List pending invitations of a workspace";
    const TAGS: &'static [&'static str] = &["invitations"];

    async fn call(&self, cx: OpCtx, input: InWorkspace) -> Result<InvitationList, ApiError> {
        let rows = invitations::list_pending(cx.platform().db().read(), input.workspace_id).await?;
        let items = rows
            .into_iter()
            .map(Invitation::try_from)
            .collect::<anyhow::Result<_>>()
            .map_err(internal)?;
        Ok(InvitationList { items })
    }
}

struct InvitationsCreate;
impl Operation for InvitationsCreate {
    type Input = CreateInvitation;
    type Output = Invitation;
    const ID: &'static str = "invitations.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/invitations";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "Invite an email to a workspace; mails them a link";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = &["invitations"];
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: CreateInvitation) -> Result<Invitation, ApiError> {
        let email = normalize_email(&input.email);
        let now = cx.now();
        let inviter = match cx.principal() {
            Some(crate::ops::Principal::User(user)) => Some(user.user_id),
            _ => None,
        };
        let mut tx = cx.write().await?;
        let workspace =
            workspaces::find(&mut *tx, input.workspace_id).await?.ok_or(ApiError::NotFound)?;
        if let Some(user) = users::find_by_email(&mut *tx, &email).await?
            && members::role_of(&mut *tx, input.workspace_id, user.id).await?.is_some()
        {
            return Err(ApiError::Conflict(format!("{email} is already a member")));
        }
        if let Some(pending) =
            invitations::find_pending(&mut *tx, input.workspace_id, &email).await?
        {
            if pending.is_open(now) {
                return Err(ApiError::Conflict(format!("{email} already has an open invitation")));
            }
            // An expired invite still holds the one-pending-per-email index.
            invitations::mark_revoked(&mut *tx, pending.id, now).await?;
        }
        let invitation = invitations::insert(
            &mut *tx,
            invitations::NewInvitation {
                workspace_id: input.workspace_id,
                email: &email,
                role: input.role,
                invited_by: inviter,
                expires_at: now.plus(INVITE_TTL),
            },
            now,
        )
        .await?;
        let link = cx
            .platform()
            .base_url()
            .join(&format!("/invite/{}", invitation.id))
            .context("building the invite link")
            .map_err(internal)?;
        let mail = crate::deps::Mail {
            to: email,
            subject: format!("You're invited to {} on bizkit", workspace.name),
            text: format!(
                "You've been invited to join {} on bizkit.\n\nOpen this link and sign in with this email \
                 address to accept:\n\n{link}\n\nThe invitation expires in 7 days.",
                workspace.name
            ),
            html: None,
        };
        cx.platform().bus().enqueue(&mut tx, &SendMail(mail), Enqueue::default()).await?;
        tx.commit().await?;
        Invitation::try_from(invitation).map_err(internal)
    }
}

struct InvitationsRevoke;
impl Operation for InvitationsRevoke {
    type Input = InInvitation;
    type Output = NoContent;
    const ID: &'static str = "invitations.revoke";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/invitations/{invitation_id}";
    const ACCESS: Access = WORKSPACE_ADMIN;
    const SUMMARY: &'static str = "Revoke a pending invitation";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = &["invitations"];

    async fn call(&self, cx: OpCtx, input: InInvitation) -> Result<NoContent, ApiError> {
        let now = cx.now();
        let mut tx = cx.write().await?;
        let invitation = invitations::find(&mut *tx, input.invitation_id)
            .await?
            .filter(|i| {
                i.workspace_id == input.workspace_id
                    && i.accepted_at.is_none()
                    && i.revoked_at.is_none()
            })
            .ok_or(ApiError::NotFound)?;
        invitations::mark_revoked(&mut *tx, invitation.id, now).await?;
        tx.commit().await?;
        Ok(NoContent)
    }
}

struct InvitationsMine;
impl Operation for InvitationsMine {
    type Input = NoInput;
    type Output = MyInvitationList;
    const ID: &'static str = "invitations.mine";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/me/invitations";
    const ACCESS: Access = Access::User;
    const SUMMARY: &'static str = "List open invitations for your email";
    const TAGS: &'static [&'static str] = &["invitations"];

    async fn call(&self, cx: OpCtx, _input: NoInput) -> Result<MyInvitationList, ApiError> {
        let user = cx.user()?;
        let rows =
            invitations::list_open_for_email(cx.platform().db().read(), &user.email, cx.now())
                .await?;
        let items = rows
            .into_iter()
            .map(|row| {
                Ok(MyInvitation {
                    id: row.id,
                    workspace: WorkspaceRef {
                        id: row.workspace_id,
                        slug: row.workspace_slug,
                        name: row.workspace_name,
                    },
                    role: Role::parse(&row.role)?,
                    invited_by: row.invited_by_email,
                    expires_at: row.expires_at,
                })
            })
            .collect::<anyhow::Result<_>>()
            .map_err(internal)?;
        Ok(MyInvitationList { items })
    }
}

/// The caller's open invitation, or 404. Someone else's invite is a 404 too.
async fn my_open_invitation(
    cx: &OpCtx,
    tx: &mut crate::db::WriteTx,
    id: InvitationId,
) -> Result<invitations::InvitationRow, ApiError> {
    let email = &cx.user()?.email;
    invitations::find(&mut **tx, id)
        .await?
        .filter(|invitation| &invitation.email == email && invitation.is_open(cx.now()))
        .ok_or(ApiError::NotFound)
}

struct InvitationsAccept;
impl Operation for InvitationsAccept {
    type Input = InMyInvitation;
    type Output = Membership;
    const ID: &'static str = "invitations.accept";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/me/invitations/{invitation_id}/accept";
    const ACCESS: Access = Access::User;
    const SUMMARY: &'static str = "Accept an invitation and join its workspace";
    const TAGS: &'static [&'static str] = &["invitations"];
    const ERRORS: &'static [StatusCode] = &[StatusCode::NOT_FOUND];

    async fn call(&self, cx: OpCtx, input: InMyInvitation) -> Result<Membership, ApiError> {
        let user_id = cx.user_id()?;
        let now = cx.now();
        let mut tx = cx.write().await?;
        let invitation = my_open_invitation(&cx, &mut tx, input.invitation_id).await?;
        let invited_role = Role::parse(&invitation.role).map_err(internal)?;
        // Someone already in the workspace (a policy join, say) keeps their role.
        members::insert(&mut *tx, invitation.workspace_id, user_id, invited_role, now).await?;
        invitations::mark_accepted(&mut *tx, invitation.id, now).await?;
        let role = members::role_of(&mut *tx, invitation.workspace_id, user_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        let workspace =
            workspaces::find(&mut *tx, invitation.workspace_id).await?.ok_or(ApiError::NotFound)?;
        tx.commit().await?;
        Ok(Membership { workspace: workspace.into(), role })
    }
}

struct InvitationsDecline;
impl Operation for InvitationsDecline {
    type Input = InMyInvitation;
    type Output = NoContent;
    const ID: &'static str = "invitations.decline";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/me/invitations/{invitation_id}/decline";
    const ACCESS: Access = Access::User;
    const SUMMARY: &'static str = "Decline an invitation";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = &["invitations"];
    const ERRORS: &'static [StatusCode] = &[StatusCode::NOT_FOUND];

    async fn call(&self, cx: OpCtx, input: InMyInvitation) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let invitation = my_open_invitation(&cx, &mut tx, input.invitation_id).await?;
        invitations::mark_revoked(&mut *tx, invitation.id, cx.now()).await?;
        tx.commit().await?;
        Ok(NoContent)
    }
}

// ---------------------------------------------------------------- projects

struct ProjectsList;
impl Operation for ProjectsList {
    type Input = ListProjects;
    type Output = ProjectList;
    const ID: &'static str = "projects.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/projects";
    const ACCESS: Access = PROJECTS_READ;
    const SUMMARY: &'static str = "List the projects of a workspace";
    const TAGS: &'static [&'static str] = &["projects"];

    async fn call(&self, cx: OpCtx, input: ListProjects) -> Result<ProjectList, ApiError> {
        let limit = i64::from(input.limit.unwrap_or(100));
        let rows = projects::list(cx.platform().db().read(), input.workspace_id, limit).await?;
        Ok(ProjectList { items: rows.into_iter().map(Into::into).collect() })
    }
}

struct ProjectsCreate;
impl Operation for ProjectsCreate {
    type Input = CreateProject;
    type Output = Project;
    const ID: &'static str = "projects.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/projects";
    const ACCESS: Access = PROJECTS_WRITE;
    const SUMMARY: &'static str = "Create a project";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = &["projects"];
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];

    async fn call(&self, cx: OpCtx, input: CreateProject) -> Result<Project, ApiError> {
        let mut tx = cx.write().await?;
        let row = match projects::insert(
            &mut *tx,
            input.workspace_id,
            &input.slug,
            input.name.trim(),
            cx.now(),
        )
        .await
        {
            Err(err) if is_unique_violation(&err) => {
                return Err(ApiError::Conflict(format!(
                    "the slug {} is taken in this workspace",
                    input.slug
                )));
            }
            other => other?,
        };
        let project = Project::from(row);
        cx.platform().bus().publish(&mut tx, &ProjectCreated { project: project.clone() }).await?;
        tx.commit().await?;
        Ok(project)
    }
}

struct ProjectsGet;
impl Operation for ProjectsGet {
    type Input = InProject;
    type Output = Project;
    const ID: &'static str = "projects.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/projects/{project_id}";
    const ACCESS: Access = PROJECTS_READ;
    const SUMMARY: &'static str = "Show a project";
    const TAGS: &'static [&'static str] = &["projects"];

    async fn call(&self, cx: OpCtx, input: InProject) -> Result<Project, ApiError> {
        let row =
            projects::find(cx.platform().db().read(), input.workspace_id, input.project_id).await?;
        row.map(Into::into).ok_or(ApiError::NotFound)
    }
}

struct ProjectsUpdate;
impl Operation for ProjectsUpdate {
    type Input = UpdateProject;
    type Output = Project;
    const ID: &'static str = "projects.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/projects/{project_id}";
    const ACCESS: Access = PROJECTS_WRITE;
    const SUMMARY: &'static str = "Rename a project";
    const TAGS: &'static [&'static str] = &["projects"];

    async fn call(&self, cx: OpCtx, input: UpdateProject) -> Result<Project, ApiError> {
        let mut tx = cx.write().await?;
        let row =
            projects::rename(&mut *tx, input.workspace_id, input.project_id, input.name.trim())
                .await?;
        tx.commit().await?;
        row.map(Into::into).ok_or(ApiError::NotFound)
    }
}

struct ProjectsDelete;
impl Operation for ProjectsDelete {
    type Input = InProject;
    type Output = NoContent;
    const ID: &'static str = "projects.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/projects/{project_id}";
    const ACCESS: Access = PROJECTS_WRITE;
    const SUMMARY: &'static str = "Delete a project";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = &["projects"];

    async fn call(&self, cx: OpCtx, input: InProject) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted = projects::delete(&mut *tx, input.workspace_id, input.project_id).await?;
        tx.commit().await?;
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}
