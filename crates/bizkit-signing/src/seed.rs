//! Demo requests for the dev stack's website project: a draft, and a sent
//! request one of two signers has signed. A seed can't run the seal job, so
//! nothing is completed.

use anyhow::Context as _;
use bizkit_core::DemoSeed;
use bizkit_core::auth::{hash_token, random_token};
use bizkit_core::db::Db;

use crate::api::{DocumentVersion, EventKind, RequestStatus, SignerId, SigningRequestId};
use crate::ops::sha256_hex;
use crate::signature::Signature;
use crate::store::{self, NewEvent, RequestRow, SignerRow};
use crate::testing::sample_pdf;

struct Demo {
    title: &'static str,
    file_name: &'static str,
    status: RequestStatus,
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

pub(crate) async fn seed(db: &Db, seed: &DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get("website") else { return Ok(()) };
    if !store::list_requests(db.read(), seed.workspace_id, project_id).await?.is_empty() {
        return Ok(());
    }
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
                name: name.to_owned(),
                email: email.to_owned(),
                status: "pending".to_owned(),
                signature: None,
                signed_at: None,
            };
            store::insert_signer(&mut *tx, &signer).await?;
            if !sent {
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
                store::mark_signed(&mut *tx, &signer, &signature, seed.now).await?;
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
