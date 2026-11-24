//! Sequences at run time: enrolling contacts and the `mailing.advance` job
//! that sends one step per run and schedules the next.
//!
//! The next step is always "the lowest-position step with no delivery for
//! this enrollment", so editing, reordering or adding steps never sends a
//! step twice and never strands an enrollment.

use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::db::WriteTx;
use bizkit_core::hooks::{HookResult, HookTarget};
use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::{Enqueue, Job, JobCtx, JobHandler, Lane};
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Serialize};

use crate::api::{ContactStatus, DeliveryKind, DeliveryStatus, EnrollmentId, EnrollmentStatus};
use crate::hooks::{self, Condition, STEP_CONDITION};
use crate::send::{self, MAILING};
use crate::store::{self, ContactRow, EnrollmentRow, ListRow, SequenceRow, StepRow};

/// Sends an enrollment's next step if it is due.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Advance {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) enrollment_id: EnrollmentId,
}

impl Job for Advance {
    const KIND: &'static str = "mailing.advance";
    const LANE: Lane = MAILING;
}

fn after(now: UnixMillis, delay_hours: i64) -> UnixMillis {
    now.plus(Duration::from_hours(u64::try_from(delay_hours).unwrap_or(0)))
}

async fn schedule(
    platform: &Platform,
    tx: &mut WriteTx,
    enrollment: &EnrollmentRow,
    run_at: UnixMillis,
) -> anyhow::Result<()> {
    let job = Advance { workspace_id: enrollment.workspace_id, enrollment_id: enrollment.id };
    platform.bus().enqueue(tx, &job, Enqueue { run_at: Some(run_at) }).await?;
    Ok(())
}

/// Enrolls `contact` in `sequence` with its first step due after the step's
/// delay. `None` when the contact was already enrolled, in any status.
pub(crate) async fn enroll(
    platform: &Platform,
    tx: &mut WriteTx,
    sequence: &SequenceRow,
    contact: &ContactRow,
) -> anyhow::Result<Option<EnrollmentRow>> {
    let now = platform.now();
    let steps = store::list_steps(&mut **tx, sequence.workspace_id, sequence.id)
        .await
        .context("loading the steps")?;
    let first_due = after(now, steps.first().map_or(0, |step| step.delay_hours));
    let row = EnrollmentRow {
        id: EnrollmentId::new(),
        workspace_id: sequence.workspace_id,
        sequence_id: sequence.id,
        contact_id: contact.id,
        status: EnrollmentStatus::Active.as_str().to_owned(),
        next_run_at: Some(first_due),
        created_at: now,
        updated_at: now,
    };
    if !store::insert_enrollment(&mut **tx, &row).await.context("inserting the enrollment")? {
        return Ok(None);
    }
    schedule(platform, tx, &row, first_due).await?;
    Ok(Some(row))
}

/// Enrolls a contact that just became subscribed in its list's enabled
/// `on_join` sequences.
pub(crate) async fn enroll_on_join(
    platform: &Platform,
    tx: &mut WriteTx,
    contact: &ContactRow,
) -> anyhow::Result<()> {
    let sequences = store::on_join_sequences(&mut **tx, contact.workspace_id, contact.list_id)
        .await
        .context("finding the list's sequences")?;
    for sequence in &sequences {
        enroll(platform, tx, sequence, contact).await?;
    }
    Ok(())
}

/// Asks the step-condition script, if any, whether to send `step`. Fails
/// open: anything but a valid skip sends.
async fn ask(
    platform: &Platform,
    project_id: bizkit_core::ids::ProjectId,
    sequence: &SequenceRow,
    step: &StepRow,
    contact: &ContactRow,
) -> anyhow::Result<Condition> {
    let input = hooks::step_condition_input(sequence, step, contact)?;
    let target = HookTarget {
        workspace_id: sequence.workspace_id,
        project_id,
        entity: Some(sequence.id.to_string()),
    };
    Ok(match platform.call_sync_hook(&STEP_CONDITION, target, &input).await {
        HookResult::Returned(value) => hooks::condition(&value).unwrap_or(Condition::Send),
        HookResult::Continue => Condition::Send,
    })
}

/// What one advance run read before its write transaction.
struct Run {
    seen: EnrollmentRow,
    sequence: SequenceRow,
    list: ListRow,
    contact: ContactRow,
    subscribed: bool,
    step: Option<StepRow>,
    condition: Condition,
}

/// Reads what the run needs and asks the step-condition script. `None`
/// when there is nothing to do: the enrollment ended, isn't due (a
/// rescheduled enrollment leaves its older job behind), or a row is gone.
async fn load(platform: &Platform, job: &Advance) -> anyhow::Result<Option<Run>> {
    let db = platform.db().read();
    let Some(seen) = store::find_enrollment(db, job.workspace_id, job.enrollment_id)
        .await
        .context("loading the enrollment")?
    else {
        return Ok(None);
    };
    if seen.status != EnrollmentStatus::Active.as_str()
        || seen.next_run_at.is_some_and(|at| at > platform.now())
    {
        return Ok(None);
    }
    let Some(sequence) = store::find_sequence(db, seen.workspace_id, seen.sequence_id)
        .await
        .context("loading the sequence")?
    else {
        return Ok(None);
    };
    let Some(contact) = store::find_contact(db, seen.workspace_id, seen.contact_id)
        .await
        .context("loading the contact")?
    else {
        return Ok(None);
    };
    let Some(list) = store::find_list(db, sequence.workspace_id, sequence.list_id)
        .await
        .context("loading the list")?
    else {
        return Ok(None);
    };
    let subscribed = contact.status == ContactStatus::Subscribed.as_str();
    let step = store::next_step(db, seen.workspace_id, sequence.id, seen.id)
        .await
        .context("finding the next step")?;
    // The script runs outside any transaction: it can take its whole
    // budget, and a write transaction blocks every other writer.
    let condition = match &step {
        Some(step) if subscribed => {
            ask(platform, list.project_id, &sequence, step, &contact).await?
        }
        _ => Condition::Send,
    };
    Ok(Some(Run { seen, sequence, list, contact, subscribed, step, condition }))
}

/// Records the run: the step's delivery and the next schedule, or the end
/// of the enrollment. Does nothing when the enrollment changed since
/// [`load`].
async fn record(platform: &Platform, run: Run) -> anyhow::Result<()> {
    let Run { seen, sequence, list, contact, subscribed, step, condition } = run;
    let now = platform.now();
    let mut tx = platform.db().write().await?;
    let Some(mut enrollment) = store::find_enrollment(&mut *tx, seen.workspace_id, seen.id)
        .await
        .context("reloading the enrollment")?
    else {
        return Ok(());
    };
    if enrollment.status != EnrollmentStatus::Active.as_str()
        || enrollment.next_run_at != seen.next_run_at
    {
        return Ok(());
    }
    enrollment.updated_at = now;
    if !subscribed {
        EnrollmentStatus::Unsubscribed.as_str().clone_into(&mut enrollment.status);
        enrollment.next_run_at = None;
        store::update_enrollment(&mut *tx, &enrollment).await.context("ending the run")?;
        return tx.commit().await;
    }
    if let Some(step) = &step {
        let mut row = send::delivery(
            platform,
            &list,
            DeliveryKind::Step,
            &contact.email,
            step.subject.clone(),
        );
        row.contact_id = Some(contact.id);
        row.enrollment_id = Some(enrollment.id);
        row.step_id = Some(step.id);
        match condition {
            Condition::Send => {
                send::queue(platform, &mut tx, &row).await?;
            }
            Condition::Skip(reason) => {
                DeliveryStatus::Skipped.as_str().clone_into(&mut row.status);
                row.error = Some(reason);
                store::insert_delivery(&mut *tx, &row).await.context("recording the skip")?;
            }
        }
    }
    let next = store::next_step(&mut *tx, enrollment.workspace_id, sequence.id, enrollment.id)
        .await
        .context("finding the step after")?;
    if let Some(next) = next {
        let due = after(now, next.delay_hours);
        enrollment.next_run_at = Some(due);
        schedule(platform, &mut tx, &enrollment, due).await?;
    } else {
        EnrollmentStatus::Completed.as_str().clone_into(&mut enrollment.status);
        enrollment.next_run_at = None;
    }
    store::update_enrollment(&mut *tx, &enrollment).await.context("saving the run")?;
    tx.commit().await
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AdvanceHandler;

impl JobHandler<Advance> for AdvanceHandler {
    async fn handle(&self, cx: JobCtx, job: Advance) -> anyhow::Result<()> {
        match load(&cx.platform, &job).await? {
            Some(run) => record(&cx.platform, run).await,
            None => Ok(()),
        }
    }
}
