//! Extension seams for later scripts: an event per inbox submission, and a
//! hook that runs before field validation.

use std::fmt;
use std::time::Duration;

use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::Event;
use bizkit_core::validate::FieldErrors;
use serde::{Deserialize, Serialize};

use crate::api::{FormId, SubmissionId};
use crate::ingest::parse::RawFields;

/// How long a [`SubmitHook`] may take. Implementations that run scripts
/// must stop them at this budget.
pub const HOOK_BUDGET: Duration = Duration::from_millis(50);

/// Published in the insert transaction for each inbox submission.
#[expect(clippy::struct_field_names, reason = "the IDs are named for what they identify")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionCreated {
    /// The workspace.
    pub workspace_id: WorkspaceId,
    /// The form.
    pub form_id: FormId,
    /// The new submission.
    pub submission_id: SubmissionId,
}

impl Event for SubmissionCreated {
    const NAME: &'static str = "forms.submission.created";
}

/// What a pre-validate hook decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookVerdict {
    /// Go on to field validation.
    Accept,
    /// Refuse with these field errors (422).
    Reject(FieldErrors),
    /// Store as spam with this reason.
    Spam(String),
}

/// What a hook sees.
#[derive(Debug, Clone, Copy)]
pub struct HookInput<'a> {
    /// The form's workspace.
    pub workspace_id: WorkspaceId,
    /// The form.
    pub form_id: FormId,
    /// The form's display name.
    pub form_name: &'a str,
    /// The parsed fields, control fields included.
    pub fields: &'a RawFields,
}

/// Runs in the submit route after the bot checks pass and before field
/// validation, inside the request. It is synchronous and must return
/// within [`HOOK_BUDGET`].
pub trait SubmitHook: Send + Sync + fmt::Debug + 'static {
    /// Decides what happens to the submission.
    fn pre_validate(&self, input: &HookInput<'_>) -> HookVerdict;
}

/// The default hook: accepts everything.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopHook;

impl SubmitHook for NoopHook {
    fn pre_validate(&self, _input: &HookInput<'_>) -> HookVerdict {
        HookVerdict::Accept
    }
}
