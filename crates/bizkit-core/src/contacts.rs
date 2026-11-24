//! Email addresses that modules collect, announced so other modules can act
//! on them without depending on each other.

use serde::{Deserialize, Serialize};

use crate::ids::{ProjectId, WorkspaceId};
use crate::jobs::Event;

/// The longest name a [`ContactCaptured`] carries.
pub const MAX_NAME: usize = 200;

/// An email address collected through someone's own action, such as a form
/// submission. Published in the transaction that stores that action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactCaptured {
    /// The workspace.
    pub workspace_id: WorkspaceId,
    /// The project the source belongs to.
    pub project_id: ProjectId,
    /// The address, checked with [`crate::auth::is_plausible_email`].
    pub email: String,
    /// A display name, at most [`MAX_NAME`] characters.
    pub name: Option<String>,
    /// What collected the address.
    pub source: CaptureSource,
}

impl Event for ContactCaptured {
    const NAME: &'static str = "contacts.captured";
}

/// The entity that collected an address, named like a hook target kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureSource {
    /// The entity kind, such as `forms.form`.
    pub kind: String,
    /// The entity's ID.
    pub id: String,
}
