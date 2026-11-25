//! Demo requests for the dev stack's website project: a draft, and a sent
//! request whose first signing-order group has signed. Each signer has a
//! signature and a date field on the last page, in groups 1 and 2. A seed
//! can't run the seal job, so nothing is completed.

use anyhow::Context as _;
use bizkit_core::DemoSeed;
use bizkit_core::auth::{hash_token, random_token};
use bizkit_core::db::Db;

use crate::api::{DocumentVersion, EventKind, RequestStatus, SignerId, SigningRequestId};
use crate::fields::{FieldId, FieldKind, FieldValue};
use crate::ops::sha256_hex;
use crate::signature::Signature;
use crate::store::{self, FieldRow, NewEvent, RequestRow, SignerRow};
use crate::testing::sample_pdf;

struct Demo {
    title: &'static str,
    file_name: &'static str,
    status: RequestStatus,
    /// Name, email and, for a signer who signed, their typed signature.
    signers: [(&'static str, &'static str, Option<&'static str>); 2],
}

const DEMOS: [Demo; 2] = [
    Demo {
        title: "Contractor agreement",
        file_name: "contractor-agreement.pdf",
        status: RequestStatus::Draft,
        signers: [("Lin Ito", "lin@example.com", None), ("Demo Co", "demo@bizkit.test", None)],
    },
    Demo {
        title: "Mutual NDA",
        file_name: "mutual-nda.pdf",
        status: RequestStatus::Sent,
        signers: [
            ("Grace Hopper", "grace@example.com", Some("Grace Hopper")),
            ("Alan Turing", "alan@example.com", None),
        ],
    },
];

const PAGES: i64 = 2;

/// A signature and a date field on the last page, side by side per signer,
/// filled at `filled` (time and date) when the signer signed.
async fn insert_fields(
    tx: &mut bizkit_core::db::WriteTx,
    signer: &SignerRow,
    filled: Option<(bizkit_core::time::UnixMillis, &str)>,
) -> anyhow::Result<()> {
    let column = if signer.position == 0 { 0.08 } else { 0.54 };
    let fields = [
        (FieldKind::Signature, 0.7, 0.08, filled.map(|_| FieldValue::Signed)),
        (
            FieldKind::Date,
            0.8,
            0.04,
            filled.map(|(_, date)| FieldValue::Date { date: date.to_owned() }),
        ),
    ];
    for ((kind, y, height, value), at) in fields.into_iter().zip(0_i64..) {
        let field = FieldRow {
            id: FieldId::new(),
            workspace_id: signer.workspace_id,
            request_id: signer.request_id,
            signer_id: signer.id,
            position: signer.position * 2 + at,
            kind: kind.as_str().to_owned(),
            page: PAGES,
            x: column,
            y,
            width: 0.38,
            height,
            required: true,
            label: String::new(),
            value: value.map(|value| serde_json::to_string(&value)).transpose()?,
            filled_at: filled.map(|(time, _)| time),
        };
        store::insert_field(&mut **tx, &field).await.context("seeding a signing field")?;
    }
    Ok(())
}

pub(crate) async fn seed(db: &Db, seed: &DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get("website") else { return Ok(()) };
    if !store::list_requests(db.read(), seed.workspace_id, project_id).await?.is_empty() {
        return Ok(());
    }
    let today: String = seed.now.to_rfc3339()?.chars().take(10).collect();
    let mut tx = db.write().await?;
    for demo in DEMOS {
        let pdf = sample_pdf(&[demo.title, "Terms and conditions"]);
        let sent = demo.status == RequestStatus::Sent;
        let row = RequestRow {
            id: SigningRequestId::new(),
            workspace_id: seed.workspace_id,
            project_id,
            title: demo.title.to_owned(),
            message: "Please read and sign this week.".to_owned(),
            status: demo.status.as_str().to_owned(),
            file_name: demo.file_name.to_owned(),
            original_sha256: sha256_hex(&pdf),
            signed_sha256: None,
            created_by: Some(seed.user_id),
            sender: crate::DEMO_SENDER.to_owned(),
            failure: None,
            created_at: seed.now,
            updated_at: seed.now,
            sent_at: sent.then_some(seed.now),
            completed_at: None,
            voided_at: None,
            page_count: Some(PAGES),
        };
        store::insert_request(&mut *tx, &row).await.context("seeding a signing request")?;
        store::put_file(&mut *tx, &row, DocumentVersion::Original, &pdf).await?;
        store::insert_event(&mut *tx, NewEvent::of(&row, EventKind::Created, seed.now)).await?;
        for ((name, email, signed), position) in demo.signers.into_iter().zip(0_i64..) {
            let signer = SignerRow {
                id: SignerId::new(),
                workspace_id: row.workspace_id,
                request_id: row.id,
                position,
                signing_order: position + 1,
                name: name.to_owned(),
                email: email.to_owned(),
                status: "pending".to_owned(),
                signature: None,
                initials: None,
                signed_at: None,
            };
            store::insert_signer(&mut *tx, &signer).await?;
            let filled = signed.map(|_| (seed.now, today.as_str()));
            insert_fields(&mut tx, &signer, filled).await?;
            // The second group's link goes out once the first group signed,
            // as record_signature does.
            let mailed = position == 0 || demo.signers[0].2.is_some();
            if !sent || !mailed {
                continue;
            }
            store::set_token(&mut *tx, &signer, &hash_token(&random_token())).await?;
            store::insert_event(
                &mut *tx,
                NewEvent {
                    signer_id: Some(signer.id),
                    ..NewEvent::of(&row, EventKind::Sent, seed.now)
                },
            )
            .await?;
            if let Some(typed) = signed {
                let signature =
                    serde_json::to_string(&Signature::Typed { name: typed.to_owned() })?;
                store::mark_signed(&mut *tx, &signer, &signature, None, seed.now).await?;
                store::insert_event(
                    &mut *tx,
                    NewEvent {
                        signer_id: Some(signer.id),
                        ..NewEvent::of(&row, EventKind::Signed, seed.now)
                    },
                )
                .await?;
            }
        }
    }
    tx.commit().await.context("committing the signing seed")
}
