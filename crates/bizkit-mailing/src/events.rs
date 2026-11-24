//! Mailgun's event webhook: permanent failures and complaints mark the
//! delivery and suppress the address in its workspace.
//!
//! Mailgun signs each request with the webhook signing key
//! (`mail.webhook_signing_key_env`). A token is accepted once, and a
//! timestamp older than a day is refused. Both refusals answer 406, which
//! Mailgun treats as final, so it stops retrying a request that can never
//! pass. The token is stored in the same transaction as the event, so a
//! request that fails half way can be retried.

use std::time::Duration;

use anyhow::Context as _;
use axum::Extension;
use axum::body::Bytes;
use axum::extract::Path;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use bizkit_core::Platform;
use bizkit_core::deps::verify_mailgun_signature;
use bizkit_core::ids::WorkspaceId;
use bizkit_core::time::UnixMillis;
use serde::Deserialize;
use serde_json::Value;

use crate::api::{DeliveryEvent, DeliveryId, SuppressionReason};
use crate::store;
use crate::suppress::{self, Suppress};

/// How old a signed timestamp may be. Mailgun retries for 8 hours; a day
/// leaves room in case a retry keeps the first signature.
const MAX_AGE: Duration = Duration::from_hours(24);
/// How long tokens are kept to catch replays. Longer than `MAX_AGE`, so a
/// replay is refused for its token before its age.
const TOKEN_TTL: Duration = Duration::from_hours(24 * 7);

#[derive(Debug, Deserialize)]
struct Payload {
    signature: Signature,
    #[serde(rename = "event-data")]
    event_data: Value,
}

#[derive(Debug, Deserialize)]
#[expect(clippy::struct_field_names, reason = "Mailgun names the fields")]
struct Signature {
    timestamp: String,
    token: String,
    signature: String,
}

#[utoipa::path(
    post,
    path = "/events/mailgun",
    operation_id = "mailing.public.mailgun_events",
    tag = "mailing",
    summary = "Receive Mailgun delivery events",
    description = "Mailgun's webhook, signed with the webhook signing key. Permanent failures and complaints mark the delivery and suppress the address in its workspace. Other events, and events for unknown mail, are accepted and ignored.",
    request_body(content = Object, content_type = "application/json", description = "`{signature: {timestamp, token, signature}, \"event-data\": {...}}`"),
    responses(
        (status = 200, description = "Accepted", content_type = "text/plain", body = String),
        (status = 400, description = "Not a Mailgun event", content_type = "text/plain", body = String),
        (status = 401, description = "The signature is wrong, or no signing key is set", content_type = "text/plain", body = String),
        (status = 406, description = "A replayed token or a stale timestamp; Mailgun stops retrying", content_type = "text/plain", body = String),
        (status = 500, description = "Bizkit failed; Mailgun retries", content_type = "text/plain", body = String),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
pub(crate) const fn mailgun_docs() {}

fn plain(status: StatusCode, text: &'static str) -> Response {
    let mut response = (status, text).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    response
}

/// What an event means for mailing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MailEvent {
    Bounced,
    Complained,
    Ignored,
}

fn classify(event_data: &Value) -> MailEvent {
    let field = |name: &str| event_data.get(name).and_then(Value::as_str);
    match (field("event"), field("reason")) {
        (Some("complained"), _) => MailEvent::Complained,
        // Mailgun refused to try an address on its own suppression lists. A
        // complaint or unsubscribe there withdrew consent, so it counts as a
        // complaint here: final, and it unsubscribes the contact rows.
        (Some("failed"), Some("suppress-complaint" | "suppress-unsubscribe")) => {
            MailEvent::Complained
        }
        (Some("failed"), Some("suppress-bounce")) => MailEvent::Bounced,
        (Some("failed"), _) if field("severity") == Some("permanent") => MailEvent::Bounced,
        _ => MailEvent::Ignored,
    }
}

/// The delivery an event is about: by the ID bizkit sent as `v:bizkit_id`,
/// else by the Message-ID.
async fn find_delivery(
    exec: &mut sqlx::SqliteConnection,
    event_data: &Value,
) -> sqlx::Result<Option<(WorkspaceId, DeliveryId, String)>> {
    let bizkit_id = event_data
        .pointer("/user-variables/bizkit_id")
        .and_then(Value::as_str)
        .and_then(|id| id.parse::<DeliveryId>().ok());
    if let Some(id) = bizkit_id
        && let Some(found) = store::find_delivery_for_event(&mut *exec, id).await?
    {
        return Ok(Some(found));
    }
    let message_id = event_data
        .pointer("/message/headers/message-id")
        .and_then(Value::as_str)
        .map(|id| id.trim().trim_matches(['<', '>']))
        .filter(|id| !id.is_empty());
    match message_id {
        Some(id) => store::find_delivery_by_provider_id(exec, id).await,
        None => Ok(None),
    }
}

#[utoipa::path(
    post,
    path = "/events/mailgun/{workspace_id}",
    operation_id = "mailing.public.mailgun_workspace_events",
    tag = "mailing",
    summary = "Receive Mailgun delivery events for a workspace's own account",
    description = "The webhook for a workspace that sends through its own Mailgun account, signed with the webhook signing key in its mail settings. It handles events as the instance webhook does, for that workspace's deliveries only.",
    params(("workspace_id" = String, Path, description = "The workspace")),
    request_body(content = Object, content_type = "application/json", description = "`{signature: {timestamp, token, signature}, \"event-data\": {...}}`"),
    responses(
        (status = 200, description = "Accepted", content_type = "text/plain", body = String),
        (status = 400, description = "Not a Mailgun event, or not a workspace ID", content_type = "text/plain", body = String),
        (status = 401, description = "The signature is wrong, or the workspace saved no signing key", content_type = "text/plain", body = String),
        (status = 406, description = "A replayed token or a stale timestamp; Mailgun stops retrying", content_type = "text/plain", body = String),
        (status = 500, description = "Bizkit failed; Mailgun retries", content_type = "text/plain", body = String),
    )
)]
#[expect(dead_code, reason = "exists for its utoipa docs")]
pub(crate) const fn mailgun_workspace_docs() {}

/// `POST /events/mailgun`: the instance account's events.
pub(crate) async fn mailgun(Extension(platform): Extension<Platform>, body: Bytes) -> Response {
    let key = platform.config().mail.webhook_signing_key.clone();
    if key.is_none() {
        tracing::warn!("a Mailgun webhook arrived, but mail.webhook_signing_key_env is not set");
    }
    receive(&platform, &body, key, None).await
}

/// `POST /events/mailgun/{workspace_id}`: a workspace account's events.
pub(crate) async fn mailgun_workspace(
    Extension(platform): Extension<Platform>,
    Path(workspace_id): Path<String>,
    body: Bytes,
) -> Response {
    let Ok(workspace_id) = workspace_id.parse::<WorkspaceId>() else {
        return plain(StatusCode::BAD_REQUEST, "Expected a workspace ID.\n");
    };
    let key = match platform.mail().webhook_signing_key(&platform, workspace_id).await {
        Ok(key) => key,
        Err(err) => {
            tracing::error!(%workspace_id, error = %format!("{err:#}"), "reading a workspace's Mailgun webhook key failed");
            return plain(StatusCode::INTERNAL_SERVER_ERROR, "Try again later.\n");
        }
    };
    receive(&platform, &body, key, Some(workspace_id)).await
}

/// Checks and records one event. With `only`, deliveries of other
/// workspaces are left alone: a workspace's key vouches for its own mail.
async fn receive(
    platform: &Platform,
    body: &[u8],
    key: Option<String>,
    only: Option<WorkspaceId>,
) -> Response {
    let payload: Payload = match serde_json::from_slice(body) {
        Ok(payload) => payload,
        Err(err) => {
            tracing::warn!(error = %err, "a Mailgun webhook body has no signature or event-data");
            return plain(StatusCode::BAD_REQUEST, "Expected a Mailgun event.\n");
        }
    };
    let Some(key) = key.filter(|k| !k.trim().is_empty()) else {
        return plain(StatusCode::UNAUTHORIZED, "No signing key is set.\n");
    };
    let key = key.as_str();
    let sig = &payload.signature;
    if !verify_mailgun_signature(key, &sig.timestamp, &sig.token, &sig.signature) {
        return plain(StatusCode::UNAUTHORIZED, "Bad signature.\n");
    }
    let now = platform.now();
    let fresh = sig.timestamp.parse::<i64>().is_ok_and(|secs| {
        let at = UnixMillis(secs.saturating_mul(1000));
        at > now.minus(MAX_AGE) && at < now.plus(MAX_AGE)
    });
    if !fresh {
        return plain(StatusCode::NOT_ACCEPTABLE, "Stale timestamp.\n");
    }
    match record(platform, &payload, now, only).await {
        Ok(true) => plain(StatusCode::OK, "OK\n"),
        Ok(false) => plain(StatusCode::NOT_ACCEPTABLE, "Token already used.\n"),
        Err(err) => {
            tracing::error!(error = %format!("{err:#}"), "recording a Mailgun event failed");
            plain(StatusCode::INTERNAL_SERVER_ERROR, "Try again later.\n")
        }
    }
}

/// Claims the token and records the event in one transaction. False when
/// the token was used before.
async fn record(
    platform: &Platform,
    payload: &Payload,
    now: UnixMillis,
    only: Option<WorkspaceId>,
) -> anyhow::Result<bool> {
    let mut tx = platform.db().write().await?;
    let claimed =
        store::claim_webhook_token(&mut tx, &payload.signature.token, now, now.minus(TOKEN_TTL))
            .await
            .context("recording the webhook token")?;
    if !claimed {
        return Ok(false);
    }
    let (event, reason) = match classify(&payload.event_data) {
        MailEvent::Bounced => (DeliveryEvent::Bounced, SuppressionReason::Bounce),
        MailEvent::Complained => (DeliveryEvent::Complained, SuppressionReason::Complaint),
        MailEvent::Ignored => {
            tx.commit().await.context("recording the webhook token")?;
            return Ok(true);
        }
    };
    let found = find_delivery(&mut tx, &payload.event_data)
        .await
        .context("finding the delivery")?
        .filter(|(workspace_id, ..)| only.is_none_or(|only| only == *workspace_id));
    if let Some((workspace_id, delivery_id, to_email)) = found {
        store::mark_delivery_event(&mut *tx, workspace_id, delivery_id, event.as_str(), now)
            .await
            .context("recording the event")?;
        let entry = Suppress {
            workspace_id,
            email: &to_email,
            reason,
            delivery_id: Some(delivery_id),
            note: None,
            now,
        };
        suppress::suppress(&mut tx, entry).await?;
    }
    tx.commit().await.context("recording the event")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn classifies_mailgun_events() {
        let cases = [
            (json!({"event": "failed", "severity": "permanent"}), MailEvent::Bounced),
            (json!({"event": "failed", "severity": "temporary"}), MailEvent::Ignored),
            (
                json!({"event": "failed", "severity": "temporary", "reason": "suppress-bounce"}),
                MailEvent::Bounced,
            ),
            (
                json!({"event": "failed", "severity": "permanent", "reason": "suppress-complaint"}),
                MailEvent::Complained,
            ),
            (
                json!({"event": "failed", "severity": "permanent", "reason": "suppress-unsubscribe"}),
                MailEvent::Complained,
            ),
            (json!({"event": "complained"}), MailEvent::Complained),
            (json!({"event": "delivered"}), MailEvent::Ignored),
            (json!({"event": "opened"}), MailEvent::Ignored),
            (json!({}), MailEvent::Ignored),
        ];
        for (data, want) in cases {
            assert_eq!(classify(&data), want, "{data}");
        }
    }
}
