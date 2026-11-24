//! The sequence operations: sequences with their steps, and enrollments.

use std::collections::BTreeSet;

use anyhow::Context as _;
use bizkit_core::ids::{ProjectId, WorkspaceId};
use bizkit_core::ops::{Method, NoContent, OpCtx, Operation, Success};
use bizkit_core::validate::{FieldErrors, Validate, check_name};
use bizkit_core::{ApiError, ModuleContext};
use http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::api::{
    ContactId, ContactStatus, Enrollment, EnrollmentCounts, EnrollmentId, EnrollmentPage,
    EnrollmentStatus, ListId, Sequence, SequenceId, SequenceList, SequenceStep, SequenceTrigger,
    StepId,
};
use crate::ops::{MAX_BODY, MAX_SUBJECT, READ, TAGS, WRITE, check_limit, count, field_error, page};
use crate::sequences;
use crate::store::{self, EnrollmentFilter, EnrollmentView, SequenceRow, StepRow};

/// The most steps a sequence has.
pub const MAX_STEPS: usize = 20;
/// The longest wait before a step: a year.
pub const MAX_DELAY_HOURS: u32 = 8760;

pub(crate) fn register(cx: &mut ModuleContext<'_>) -> anyhow::Result<()> {
    cx.op(SequencesList)?;
    cx.op(SequencesCreate)?;
    cx.op(SequencesGet)?;
    cx.op(SequencesUpdate)?;
    cx.op(SequencesDelete)?;
    cx.op(EnrollmentsList)?;
    cx.op(EnrollmentsCreate)?;
    cx.op(EnrollmentsExit)?;
    Ok(())
}

// ---------------------------------------------------------------- conversions

fn step_to_api(row: StepRow) -> SequenceStep {
    SequenceStep {
        id: row.id,
        position: u32::try_from(row.position).unwrap_or(0),
        delay_hours: u32::try_from(row.delay_hours).unwrap_or(0),
        subject: row.subject,
        body_md: row.body_md,
    }
}

fn sequence_to_api(row: SequenceRow, steps: Vec<StepRow>) -> anyhow::Result<Sequence> {
    Ok(Sequence {
        trigger: SequenceTrigger::parse(&row.trigger)
            .with_context(|| format!("unknown sequence trigger {:?}", row.trigger))?,
        id: row.id,
        list_id: row.list_id,
        name: row.name,
        enabled: row.enabled,
        steps: steps.into_iter().map(step_to_api).collect(),
        enrollments: None,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// One sequence with its steps and enrollment counts.
async fn full_sequence(
    exec: impl sqlx::SqliteExecutor<'_> + Copy,
    row: SequenceRow,
) -> anyhow::Result<Sequence> {
    let steps = store::list_steps(exec, row.workspace_id, row.id).await?;
    let counts = store::enrollment_counts(exec, row.workspace_id, row.id).await?;
    let mut sequence = sequence_to_api(row, steps)?;
    sequence.enrollments = Some(EnrollmentCounts {
        active: count(counts.active),
        completed: count(counts.completed),
        exited: count(counts.exited),
        unsubscribed: count(counts.unsubscribed),
    });
    Ok(sequence)
}

fn enrollment_to_api(row: EnrollmentView) -> anyhow::Result<Enrollment> {
    Ok(Enrollment {
        status: EnrollmentStatus::parse(&row.status)
            .with_context(|| format!("unknown enrollment status {:?}", row.status))?,
        id: row.id,
        sequence_id: row.sequence_id,
        contact_id: row.contact_id,
        email: row.email,
        steps_done: u32::try_from(row.steps_done).unwrap_or(0),
        next_run_at: row.next_run_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

async fn sequence_in(
    exec: impl sqlx::SqliteExecutor<'_>,
    workspace_id: WorkspaceId,
    sequence_id: SequenceId,
) -> Result<SequenceRow, ApiError> {
    store::find_sequence(exec, workspace_id, sequence_id).await?.ok_or(ApiError::NotFound)
}

/// Step rows for `inputs` in order. IDs given must be steps of `existing`;
/// steps without one get a new ID.
fn step_rows(
    workspace_id: WorkspaceId,
    sequence_id: SequenceId,
    existing: &[StepRow],
    inputs: &[StepInput],
) -> Result<Vec<StepRow>, ApiError> {
    let known: BTreeSet<StepId> = existing.iter().map(|step| step.id).collect();
    if inputs.iter().filter_map(|step| step.id).any(|id| !known.contains(&id)) {
        return Err(field_error("steps", "a step id is not a step of this sequence"));
    }
    Ok(inputs
        .iter()
        .zip(0_i64..)
        .map(|(input, position)| StepRow {
            id: input.id.unwrap_or_else(StepId::new),
            workspace_id,
            sequence_id,
            position,
            delay_hours: i64::from(input.delay_hours),
            subject: input.subject.trim().to_owned(),
            body_md: input.body_md.clone(),
        })
        .collect())
}

// ---------------------------------------------------------------- inputs

/// A step of a sequence being saved.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StepInput {
    /// The step's ID, to keep an existing step; absent adds a new one.
    #[serde(default)]
    id: Option<StepId>,
    /// Hours to wait after enrolling (first step) or after the step before,
    /// 0 to 8760.
    delay_hours: u32,
    /// The subject line.
    subject: String,
    /// The body in Markdown.
    #[serde(default)]
    body_md: String,
}

fn check_steps(errors: &mut FieldErrors, steps: &[StepInput]) {
    if steps.len() > MAX_STEPS {
        errors.add("steps", format!("at most {MAX_STEPS} steps"));
    }
    let mut seen = BTreeSet::new();
    if steps.iter().filter_map(|step| step.id).any(|id| !seen.insert(id)) {
        errors.add("steps", "a step id appears twice");
    }
    for (i, step) in steps.iter().enumerate() {
        if step.delay_hours > MAX_DELAY_HOURS {
            errors.add(
                format!("steps[{i}].delay_hours"),
                format!("must be at most {MAX_DELAY_HOURS}"),
            );
        }
        if step.subject.trim().is_empty() {
            errors.add(format!("steps[{i}].subject"), "must not be empty");
        } else if step.subject.chars().count() > MAX_SUBJECT {
            errors.add(
                format!("steps[{i}].subject"),
                format!("must be at most {MAX_SUBJECT} characters"),
            );
        }
        if step.body_md.len() > MAX_BODY {
            errors.add(format!("steps[{i}].body_md"), format!("must be at most {MAX_BODY} bytes"));
        }
    }
}

/// Which sequences to list.
#[expect(clippy::struct_field_names, reason = "the IDs are named for what they identify")]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListSequences {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// Only sequences of this project's lists.
    #[serde(default)]
    project_id: Option<ProjectId>,
    /// Only sequences of this list.
    #[serde(default)]
    list_id: Option<ListId>,
}
impl Validate for ListSequences {}

/// A new sequence.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateSequence {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The list whose contacts it mails.
    list_id: ListId,
    /// The name, for the team.
    name: String,
    /// What enrolls contacts.
    trigger: SequenceTrigger,
    /// Whether new contacts enroll. Default true.
    #[serde(default)]
    enabled: Option<bool>,
    /// The steps in order.
    #[serde(default)]
    steps: Vec<StepInput>,
}
impl Validate for CreateSequence {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_name(&mut errors, "name", &self.name);
        if self.steps.iter().any(|step| step.id.is_some()) {
            errors.add("steps", "new sequences take steps without ids");
        }
        check_steps(&mut errors, &self.steps);
        errors.into_result()
    }
}

/// Changes to a sequence. Absent fields stay as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateSequence {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The sequence.
    sequence_id: SequenceId,
    /// The name.
    #[serde(default)]
    name: Option<String>,
    /// What enrolls contacts from now on.
    #[serde(default)]
    trigger: Option<SequenceTrigger>,
    /// Whether new contacts enroll. Running enrollments go on either way.
    #[serde(default)]
    enabled: Option<bool>,
    /// Replaces the steps. Steps sent the ID they have keep it; steps left
    /// out are deleted; steps without an ID are added.
    #[serde(default)]
    steps: Option<Vec<StepInput>>,
}
impl Validate for UpdateSequence {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        if let Some(name) = &self.name {
            check_name(&mut errors, "name", name);
        }
        if let Some(steps) = &self.steps {
            check_steps(&mut errors, steps);
        }
        errors.into_result()
    }
}

/// A sequence of a workspace.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InSequence {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The sequence.
    sequence_id: SequenceId,
}
impl Validate for InSequence {}

/// Which enrollments to list.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListEnrollments {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The sequence.
    sequence_id: SequenceId,
    /// Only enrollments with this status.
    #[serde(default)]
    status: Option<EnrollmentStatus>,
    /// Only enrollments older than this one, for the next page.
    #[serde(default)]
    before: Option<EnrollmentId>,
    /// At most this many, 1-200. Default 50.
    #[serde(default)]
    limit: Option<u32>,
}
impl Validate for ListEnrollments {
    fn validate(&self) -> Result<(), FieldErrors> {
        let mut errors = FieldErrors::default();
        check_limit(&mut errors, self.limit);
        errors.into_result()
    }
}

/// A contact to enroll.
#[expect(clippy::struct_field_names, reason = "the IDs are named for what they identify")]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateEnrollment {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The sequence.
    sequence_id: SequenceId,
    /// A subscribed contact of the sequence's list.
    contact_id: ContactId,
}
impl Validate for CreateEnrollment {}

/// An enrollment of a sequence.
#[expect(clippy::struct_field_names, reason = "path params are named like the path segments")]
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InEnrollment {
    /// The workspace.
    workspace_id: WorkspaceId,
    /// The sequence.
    sequence_id: SequenceId,
    /// The enrollment.
    enrollment_id: EnrollmentId,
}
impl Validate for InEnrollment {}

// ---------------------------------------------------------------- sequences

struct SequencesList;
impl Operation for SequencesList {
    type Input = ListSequences;
    type Output = SequenceList;
    const ID: &'static str = "mailing.sequences.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/sequences";
    const ACCESS: bizkit_core::ops::Access = READ;
    const SUMMARY: &'static str = "List email sequences with their steps";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListSequences) -> Result<SequenceList, ApiError> {
        let db = cx.platform().db().read();
        let rows =
            store::list_sequences(db, input.workspace_id, input.project_id, input.list_id).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let steps = store::list_steps(db, row.workspace_id, row.id).await?;
            items.push(sequence_to_api(row, steps)?);
        }
        Ok(SequenceList { items })
    }
}

struct SequencesCreate;
impl Operation for SequencesCreate {
    type Input = CreateSequence;
    type Output = Sequence;
    const ID: &'static str = "mailing.sequences.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/sequences";
    const ACCESS: bizkit_core::ops::Access = WRITE;
    const SUMMARY: &'static str = "Create an email sequence for a list";
    const SUCCESS: Success = Success::Created;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateSequence) -> Result<Sequence, ApiError> {
        let mut tx = cx.write().await?;
        let Some(list) = store::find_list(&mut *tx, input.workspace_id, input.list_id).await?
        else {
            return Err(field_error("list_id", "is not a list of this workspace"));
        };
        let now = cx.now();
        let row = SequenceRow {
            id: SequenceId::new(),
            workspace_id: list.workspace_id,
            list_id: list.id,
            name: input.name.trim().to_owned(),
            trigger: input.trigger.as_str().to_owned(),
            enabled: input.enabled.unwrap_or(true),
            created_at: now,
            updated_at: now,
        };
        store::insert_sequence(&mut *tx, &row).await?;
        let steps = step_rows(row.workspace_id, row.id, &[], &input.steps)?;
        store::set_steps(&mut tx, row.workspace_id, row.id, &steps).await?;
        tx.commit().await?;
        Ok(full_sequence(cx.platform().db().read(), row).await?)
    }
}

struct SequencesGet;
impl Operation for SequencesGet {
    type Input = InSequence;
    type Output = Sequence;
    const ID: &'static str = "mailing.sequences.get";
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/sequences/{sequence_id}";
    const ACCESS: bizkit_core::ops::Access = READ;
    const SUMMARY: &'static str = "Show a sequence with its steps and enrollment counts";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InSequence) -> Result<Sequence, ApiError> {
        let db = cx.platform().db().read();
        let row = sequence_in(db, input.workspace_id, input.sequence_id).await?;
        Ok(full_sequence(db, row).await?)
    }
}

struct SequencesUpdate;
impl Operation for SequencesUpdate {
    type Input = UpdateSequence;
    type Output = Sequence;
    const ID: &'static str = "mailing.sequences.update";
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/sequences/{sequence_id}";
    const ACCESS: bizkit_core::ops::Access = WRITE;
    // Steps keep their IDs, so a contact never gets a step twice; a step
    // added before a contact's current point is sent to it next.
    const SUMMARY: &'static str = "Change a sequence and its steps";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: UpdateSequence) -> Result<Sequence, ApiError> {
        let mut tx = cx.write().await?;
        let mut row = sequence_in(&mut *tx, input.workspace_id, input.sequence_id).await?;
        if let Some(name) = input.name {
            name.trim().clone_into(&mut row.name);
        }
        if let Some(trigger) = input.trigger {
            trigger.as_str().clone_into(&mut row.trigger);
        }
        if let Some(enabled) = input.enabled {
            row.enabled = enabled;
        }
        row.updated_at = cx.now();
        store::update_sequence(&mut *tx, &row).await?;
        if let Some(inputs) = &input.steps {
            let existing = store::list_steps(&mut *tx, row.workspace_id, row.id).await?;
            let steps = step_rows(row.workspace_id, row.id, &existing, inputs)?;
            store::set_steps(&mut tx, row.workspace_id, row.id, &steps).await?;
        }
        tx.commit().await?;
        Ok(full_sequence(cx.platform().db().read(), row).await?)
    }
}

struct SequencesDelete;
impl Operation for SequencesDelete {
    type Input = InSequence;
    type Output = NoContent;
    const ID: &'static str = "mailing.sequences.delete";
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/sequences/{sequence_id}";
    const ACCESS: bizkit_core::ops::Access = WRITE;
    const SUMMARY: &'static str =
        "Delete a sequence with its steps and enrollments; sent mail stays in the log";
    const SUCCESS: Success = Success::NoContent;
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InSequence) -> Result<NoContent, ApiError> {
        let mut tx = cx.write().await?;
        let deleted =
            store::delete_sequence(&mut *tx, input.workspace_id, input.sequence_id).await?;
        tx.commit().await?;
        if deleted { Ok(NoContent) } else { Err(ApiError::NotFound) }
    }
}

// ---------------------------------------------------------------- enrollments

struct EnrollmentsList;
impl Operation for EnrollmentsList {
    type Input = ListEnrollments;
    type Output = EnrollmentPage;
    const ID: &'static str = "mailing.enrollments.list";
    const METHOD: Method = Method::Get;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/mailing/sequences/{sequence_id}/enrollments";
    const ACCESS: bizkit_core::ops::Access = READ;
    const SUMMARY: &'static str = "List a sequence's enrollments, newest first";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: ListEnrollments) -> Result<EnrollmentPage, ApiError> {
        let db = cx.platform().db().read();
        sequence_in(db, input.workspace_id, input.sequence_id).await?;
        let limit = input.limit.unwrap_or(50);
        let rows = store::list_enrollments(
            db,
            input.workspace_id,
            input.sequence_id,
            &EnrollmentFilter {
                status: input.status.map(EnrollmentStatus::as_str),
                before: input.before,
            },
            i64::from(limit) + 1,
        )
        .await?;
        let items = rows.into_iter().map(enrollment_to_api).collect::<anyhow::Result<Vec<_>>>()?;
        let (items, more) = page(items, limit);
        let next_before = if more { items.last().map(|e| e.id) } else { None };
        Ok(EnrollmentPage { items, next_before })
    }
}

struct EnrollmentsCreate;
impl Operation for EnrollmentsCreate {
    type Input = CreateEnrollment;
    type Output = Enrollment;
    const ID: &'static str = "mailing.enrollments.create";
    const METHOD: Method = Method::Post;
    const PATH: &'static str =
        "/workspaces/{workspace_id}/mailing/sequences/{sequence_id}/enrollments";
    const ACCESS: bizkit_core::ops::Access = WRITE;
    // Works whatever the trigger and even when the sequence is disabled: it
    // is the team's explicit choice.
    const SUMMARY: &'static str = "Enroll a subscribed contact in a sequence";
    const SUCCESS: Success = Success::Created;
    const ERRORS: &'static [StatusCode] = &[StatusCode::CONFLICT];
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: CreateEnrollment) -> Result<Enrollment, ApiError> {
        let mut tx = cx.write().await?;
        let sequence = sequence_in(&mut *tx, input.workspace_id, input.sequence_id).await?;
        let contact = store::find_contact(&mut *tx, input.workspace_id, input.contact_id)
            .await?
            .filter(|contact| contact.list_id == sequence.list_id);
        let Some(contact) = contact else {
            return Err(field_error("contact_id", "is not a contact of the sequence's list"));
        };
        if contact.status != ContactStatus::Subscribed.as_str() {
            return Err(field_error("contact_id", "must be subscribed"));
        }
        let Some(row) = sequences::enroll(cx.platform(), &mut tx, &sequence, &contact).await?
        else {
            return Err(ApiError::Conflict(
                "the contact is already enrolled in this sequence".to_owned(),
            ));
        };
        let view = store::find_enrollment_view(&mut *tx, row.workspace_id, row.id)
            .await?
            .context("reading the new enrollment")?;
        tx.commit().await?;
        Ok(enrollment_to_api(view)?)
    }
}

struct EnrollmentsExit;
impl Operation for EnrollmentsExit {
    type Input = InEnrollment;
    type Output = Enrollment;
    const ID: &'static str = "mailing.enrollments.exit";
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/workspaces/{workspace_id}/mailing/sequences/{sequence_id}/enrollments/{enrollment_id}/exit";
    const ACCESS: bizkit_core::ops::Access = WRITE;
    // Idempotent: an enrollment that already ended stays as it is.
    const SUMMARY: &'static str = "Take a contact out of a sequence; it gets no more steps";
    const TAGS: &'static [&'static str] = TAGS;

    async fn call(&self, cx: OpCtx, input: InEnrollment) -> Result<Enrollment, ApiError> {
        let mut tx = cx.write().await?;
        let mut row = store::find_enrollment(&mut *tx, input.workspace_id, input.enrollment_id)
            .await?
            .filter(|row| row.sequence_id == input.sequence_id)
            .ok_or(ApiError::NotFound)?;
        if row.status == EnrollmentStatus::Active.as_str() {
            EnrollmentStatus::Exited.as_str().clone_into(&mut row.status);
            row.next_run_at = None;
            row.updated_at = cx.now();
            store::update_enrollment(&mut *tx, &row).await?;
        }
        let view = store::find_enrollment_view(&mut *tx, row.workspace_id, row.id)
            .await?
            .ok_or(ApiError::NotFound)?;
        tx.commit().await?;
        Ok(enrollment_to_api(view)?)
    }
}
