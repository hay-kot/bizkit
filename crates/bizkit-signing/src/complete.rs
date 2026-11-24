//! The `signing.complete` job: after the last signature, build the final
//! PDF (the original, a signatures page and the audit trail), seal it, and
//! mail everyone the result.

use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::time::Duration;

use anyhow::Context as _;
use bizkit_core::Platform;
use bizkit_core::auth::{hash_token, random_token};
use bizkit_core::ids::WorkspaceId;
use bizkit_core::jobs::{Enqueue, Job, JobCtx, JobHandler, Lane, SendMail};
use futures::FutureExt as _;
use serde::{Deserialize, Serialize};

use crate::api::{DocumentVersion, EventKind, RequestStatus, SignerId, SigningRequestId};
use crate::ops::sha256_hex;
use crate::pdf::{self, AuditLine, Record, SignedBy};
use crate::seal::{self, SealKey};
use crate::signature::Signature;
use crate::store::{self, NewEvent, RequestRow, SignerRow};

/// Seals run one at a time in their own lane, so they never hold up mail.
const SIGNING: Lane = Lane { name: "signing", concurrency: 1 };

/// Seal a request whose signers have all signed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Complete {
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) request_id: SigningRequestId,
}

impl Job for Complete {
    const KIND: &'static str = "signing.complete";
    const MAX_ATTEMPTS: u32 = 3;
    const LANE: Lane = SIGNING;
    const TIMEOUT: Duration = Duration::from_secs(120);
}

/// How long one attempt may take before it counts as failed. Shorter than
/// [`Complete::TIMEOUT`], so the last attempt still has time to mark the
/// request failed before the worker gives up on it.
const BUDGET: Duration = Duration::from_secs(90);

pub(crate) struct CompleteHandler;

impl JobHandler<Complete> for CompleteHandler {
    async fn handle(&self, cx: JobCtx, job: Complete) -> anyhow::Result<()> {
        let run = AssertUnwindSafe(complete(&cx.platform, &job)).catch_unwind();
        let outcome = match tokio::time::timeout(BUDGET, run).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(anyhow::anyhow!("sealing panicked")),
            Err(_) => Err(anyhow::anyhow!("sealing took longer than {} s", BUDGET.as_secs())),
        };
        match outcome {
            // The worker retries any Err; a seal that fails every time
            // marks the request instead, for `signing.requests.retry`.
            Err(err) if cx.attempt >= Complete::MAX_ATTEMPTS => {
                mark_failed(&cx.platform, &job, &err).await
            }
            other => other,
        }
    }
}

async fn complete(platform: &Platform, job: &Complete) -> anyhow::Result<()> {
    let db = platform.db().read();
    let Some(row) = store::find_request(db, job.workspace_id, job.request_id)
        .await
        .context("reading the request")?
    else {
        return Ok(());
    };
    // Voided, failed or already sealed: nothing to do.
    let pending = store::pending_signers(db, &row).await.context("counting pending signers")?;
    if row.status != RequestStatus::Sent.as_str() || pending > 0 {
        return Ok(());
    }
    let sealed = build(platform, &row).await?;
    finish(platform, &row, &sealed).await
}

async fn mark_failed(
    platform: &Platform,
    job: &Complete,
    err: &anyhow::Error,
) -> anyhow::Result<()> {
    let failure = format!("{err:#}");
    tracing::error!(request = %job.request_id, error = failure, "sealing a signed PDF failed");
    let now = platform.now();
    let mut tx = platform.db().write().await?;
    let Some(row) = store::find_request(&mut *tx, job.workspace_id, job.request_id)
        .await
        .context("reading the request")?
    else {
        return Ok(());
    };
    if store::set_failed(&mut *tx, &row, &failure, now)
        .await
        .context("marking the request failed")?
    {
        let event =
            NewEvent { detail: Some(&failure), ..NewEvent::of(&row, EventKind::Failed, now) };
        store::insert_event(&mut *tx, event).await.context("recording the failure")?;
    }
    tx.commit().await.context("committing the failure")
}

/// The instance's seal, made on first use.
async fn seal_key(platform: &Platform) -> anyhow::Result<SealKey> {
    let db = platform.db();
    if let Some((cert, key)) = store::seal(db.read()).await.context("reading the seal")? {
        return SealKey::from_parts(&cert, &key);
    }
    let host = platform.base_url().host_str().unwrap_or("bizkit").to_owned();
    let key = SealKey::generate(&host, platform.now()).context("making the seal")?;
    let (cert, scalar) = key.to_parts().context("encoding the seal")?;
    let mut tx = db.write().await?;
    store::insert_seal(&mut *tx, &cert, &scalar, platform.now())
        .await
        .context("storing the seal")?;
    tx.commit().await.context("committing the seal")?;
    // Another worker may have stored its seal first; use the stored one.
    let (stored_cert, stored_key) = store::seal(db.read())
        .await
        .context("reading the seal")?
        .context("the seal row is missing")?;
    SealKey::from_parts(&stored_cert, &stored_key)
}

fn event_label(kind: &str) -> &str {
    match kind {
        "created" => "Created",
        "sent" => "Link sent",
        "viewed" => "Opened the signing page",
        "signed" => "Signed",
        "voided" => "Voided",
        "failed" => "Sealing failed",
        other => other,
    }
}

async fn record(
    platform: &Platform,
    row: &RequestRow,
    signers: &[SignerRow],
) -> anyhow::Result<Record> {
    let names: BTreeMap<SignerId, String> =
        signers.iter().map(|s| (s.id, format!("{} <{}>", s.name, s.email))).collect();
    let events = store::events(platform.db().read(), row.workspace_id, row.id)
        .await
        .context("reading the audit trail")?;
    let audit = events
        .into_iter()
        .map(|event| {
            let who =
                event.signer_id.and_then(|id| names.get(&id)).map(|name| format!(" - {name}"));
            let detail = event.detail.map(|detail| format!(". {detail}"));
            let client = match (event.ip, event.user_agent) {
                (None, None) => None,
                (ip, agent) => Some(format!(
                    "IP {} - {}",
                    ip.unwrap_or_else(|| "unknown".to_owned()),
                    agent.unwrap_or_else(|| "unknown browser".to_owned())
                )),
            };
            AuditLine {
                at: event.at,
                what: format!(
                    "{}{}{}",
                    event_label(&event.kind),
                    who.unwrap_or_default(),
                    detail.unwrap_or_default()
                ),
                client,
            }
        })
        .collect();
    let signed = signers
        .iter()
        .map(|signer| {
            let signature: Signature =
                serde_json::from_str(signer.signature.as_deref().unwrap_or("null"))
                    .with_context(|| format!("reading {}'s signature", signer.email))?;
            Ok(SignedBy {
                name: signer.name.clone(),
                email: signer.email.clone(),
                signed_at: signer.signed_at.unwrap_or(row.created_at),
                signature,
            })
        })
        .collect::<anyhow::Result<_>>()?;
    Ok(Record {
        title: row.title.clone(),
        file_name: row.file_name.clone(),
        request_id: row.id.to_string(),
        sender: row.sender.clone(),
        original_sha256: row.original_sha256.clone(),
        signers: signed,
        audit,
    })
}

/// The sealed PDF.
async fn build(platform: &Platform, row: &RequestRow) -> anyhow::Result<Vec<u8>> {
    let db = platform.db().read();
    let original = store::get_file(db, row, DocumentVersion::Original)
        .await
        .context("reading the original PDF")?
        .context("the original PDF is missing")?;
    let signers =
        store::signers(db, row.workspace_id, row.id).await.context("reading the signers")?;
    let record = record(platform, row, &signers).await?;
    let key = seal_key(platform).await?;
    let now = platform.now();
    tokio::task::spawn_blocking(move || {
        let doc = pdf::assemble(&original, &record)?;
        seal::seal(doc, &key, now)
    })
    .await
    .context("the seal thread panicked")?
}

/// Stores the sealed PDF and mails everyone. Each signer gets a download
/// link with a token of its own, since the database holds only the hash of
/// their sign link's token; the sign link keeps working and leads there too.
async fn finish(platform: &Platform, row: &RequestRow, sealed: &[u8]) -> anyhow::Result<()> {
    let now = platform.now();
    let sha256 = sha256_hex(sealed);
    let mut tx = platform.db().write().await?;
    let completed =
        store::set_status(&mut *tx, row, &[RequestStatus::Sent], RequestStatus::Completed, now)
            .await
            .context("marking the request completed")?;
    if !completed {
        return Ok(());
    }
    store::put_file(&mut *tx, row, DocumentVersion::Signed, sealed)
        .await
        .context("storing the sealed PDF")?;
    store::set_signed_sha256(&mut *tx, row, &sha256).await.context("storing the seal's hash")?;
    let detail = format!("Sealed by bizkit. SHA-256 of the sealed file: {sha256}.");
    let event = NewEvent { detail: Some(&detail), ..NewEvent::of(row, EventKind::Completed, now) };
    store::insert_event(&mut *tx, event).await.context("recording the completion")?;
    let signers =
        store::signers(&mut *tx, row.workspace_id, row.id).await.context("reading the signers")?;
    for signer in signers {
        let token = random_token();
        store::set_download_token(&mut *tx, &signer, &hash_token(&token))
            .await
            .context("storing a download link")?;
        let link = crate::public::signed_link(platform, &token)?;
        let mail = crate::mail::completed(row, &signer.email, &link, &sha256);
        platform
            .bus()
            .enqueue(&mut tx, &SendMail(mail), Enqueue::default())
            .await
            .context("queueing a completion mail")?;
    }
    if row.created_by.is_some() && row.sender.contains('@') {
        let link = platform
            .base_url()
            .join(&format!("/w/{}/p/{}/signing/{}", row.workspace_id, row.project_id, row.id))
            .context("building the request link")?;
        let mail = crate::mail::completed(row, &row.sender, &link, &sha256);
        platform
            .bus()
            .enqueue(&mut tx, &SendMail(mail), Enqueue::default())
            .await
            .context("queueing the sender's completion mail")?;
    }
    tx.commit().await.context("committing the sealed PDF")
}
