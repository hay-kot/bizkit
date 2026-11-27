//! Sample endpoints for the dev stack's demo workspace.

use std::time::Duration;

use bizkit_core::DemoSeed;
use bizkit_core::db::Db;
use serde_json::json;

use crate::api::{Delivery, EndpointId, RequestId};
use crate::hooks::ENDPOINT;
use crate::store::{self, EndpointRow, RequestRow};

/// A demo request: method, query, extra headers, JSON body, and whether it
/// carries a sample successful delivery.
type DemoRequest =
    (&'static str, Option<&'static str>, Vec<(&'static str, String)>, serde_json::Value, bool);

fn stripe_event(id: &str, kind: &str, amount: u32) -> serde_json::Value {
    json!({
        "id": id,
        "object": "event",
        "type": kind,
        "data": { "object": { "id": "pi_3QdemoA1b2C3d4", "object": "payment_intent",
                              "amount": amount, "currency": "usd", "status": kind.rsplit('.').next() } },
    })
}

fn demo_endpoints() -> [(&'static str, Vec<DemoRequest>); 2] {
    let stripe = |kind: &str| {
        vec![
            (
                "stripe-signature",
                format!(
                    "t=1767225600,v1=5257a869e7ecebeda32affa62cdca3fa51cad7e77a0e56ff536d0ce8e108d8bd,kind={kind}"
                ),
            ),
            ("user-agent", "Stripe/1.0 (+https://stripe.com/docs/webhooks)".to_owned()),
        ]
    };
    let github = |event: &str| {
        vec![
            ("x-github-event", event.to_owned()),
            ("x-github-delivery", "72d3162e-cc78-11e3-81ab-4c9367dc0958".to_owned()),
            (
                "x-hub-signature-256",
                "sha256=d57c68ca6f92289e6987922ff26938930f6e66a2d161ef06abdf1859230aa23c"
                    .to_owned(),
            ),
            ("user-agent", "GitHub-Hookshot/044aadd".to_owned()),
        ]
    };
    [
        (
            "Stripe",
            vec![
                (
                    "POST",
                    None,
                    stripe("created"),
                    stripe_event("evt_1", "payment_intent.created", 4900),
                    false,
                ),
                (
                    "POST",
                    None,
                    stripe("succeeded"),
                    stripe_event("evt_2", "payment_intent.succeeded", 4900),
                    true,
                ),
                (
                    "POST",
                    None,
                    stripe("failed"),
                    stripe_event("evt_3", "payment_intent.payment_failed", 12_000),
                    false,
                ),
            ],
        ),
        (
            "GitHub",
            vec![
                (
                    "POST",
                    None,
                    github("push"),
                    json!({ "ref": "refs/heads/main", "repository": { "full_name": "demo/website" },
                            "pusher": { "name": "ada" }, "commits": [{ "id": "a1b2c3d", "message": "Fix the signup page on mobile" }] }),
                    false,
                ),
                (
                    "POST",
                    Some("source=docs"),
                    github("push"),
                    json!({ "ref": "refs/heads/docs", "repository": { "full_name": "demo/website" },
                            "pusher": { "name": "grace" }, "commits": [{ "id": "e4f5a6b", "message": "Document annual billing" }] }),
                    false,
                ),
            ],
        ),
    ]
}

/// Seeds "Stripe" and "GitHub" in the `website` project, with sample
/// requests, and records them in `seed.entities`. An endpoint that already
/// exists by name keeps its settings; it gets the sample requests only
/// while it has none, which is how the dev stack's OpenTofu endpoints get
/// them. No endpoint forwards,
/// so the dev stack sends nothing out.
pub(crate) async fn seed(db: &Db, seed: &mut DemoSeed) -> anyhow::Result<()> {
    let Some(&project_id) = seed.project_ids.get("website") else { return Ok(()) };
    let existing = store::list_endpoints(db.read(), seed.workspace_id, Some(project_id)).await?;
    let mut tx = db.write().await?;
    for (name, requests) in demo_endpoints() {
        let key = (ENDPOINT.id.to_owned(), name.to_owned());
        let endpoint_id = match existing.iter().find(|e| e.endpoint.name == name) {
            Some(found) if found.request_count > 0 => {
                seed.entities.insert(key, found.endpoint.id.to_string());
                continue;
            }
            // Made by the dev stack's OpenTofu config, still without requests.
            Some(found) => found.endpoint.id,
            None => {
                let endpoint = EndpointRow {
                    id: EndpointId::new(),
                    workspace_id: seed.workspace_id,
                    project_id,
                    name: name.to_owned(),
                    enabled: true,
                    forward_url: None,
                    created_at: seed.now,
                    updated_at: seed.now,
                };
                store::insert_endpoint(&mut *tx, &endpoint).await?;
                endpoint.id
            }
        };
        seed.entities.insert(key, endpoint_id.to_string());
        let count = u64::try_from(requests.len()).unwrap_or(0);
        for (i, (method, query, extra, body, delivered)) in requests.into_iter().enumerate() {
            let age = Duration::from_mins(20 * count.saturating_sub(u64::try_from(i).unwrap_or(0)));
            let received_at = seed.now.minus(age);
            let mut headers = vec![("content-type".to_owned(), "application/json".to_owned())];
            headers.extend(extra.into_iter().map(|(header, value)| (header.to_owned(), value)));
            let delivery = delivered.then(|| Delivery {
                url: "https://example.com/hooks/stripe".to_owned(),
                status: Some(200),
                error: None,
                took_ms: 142,
                at: received_at.plus(Duration::from_secs(1)),
            });
            store::insert_request(
                &mut *tx,
                &RequestRow {
                    id: RequestId::new(),
                    workspace_id: seed.workspace_id,
                    endpoint_id,
                    method: method.to_owned(),
                    query: query.map(str::to_owned),
                    headers: serde_json::to_string(&headers)?,
                    content_type: Some("application/json".to_owned()),
                    body: serde_json::to_vec(&body)?,
                    delivery: delivery.map(|d| serde_json::to_string(&d)).transpose()?,
                    received_at,
                },
            )
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
