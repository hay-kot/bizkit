//! The raw sign-in routes. They set cookies, which operations can't, and
//! mean nothing over MCP.

use std::time::Duration;

use anyhow::Context as _;
use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Router, body::Bytes};
use serde::Deserialize;
use utoipa::{OpenApi, ToSchema};

use super::{
    hash_token, is_plausible_email, magic_links, normalize_email, random_token, sessions, users,
};
use crate::app::Platform;
use crate::db::WriteTx;
use crate::error::{ApiError, ErrorBody};
use crate::jobs::{Enqueue, SendMail};
use crate::ops::http::{MAX_BODY_BYTES, request_id};
use crate::tenancy::{invitations, members, workspaces};
use crate::usage::{self, UsageKind, UsageReported};
use crate::validate::{FieldErrors, from_json};

const LINK_TTL: Duration = Duration::from_mins(15);
const LINK_COOLDOWN: Duration = Duration::from_mins(1);

/// Routes under `/api/auth`.
pub(crate) fn router() -> Router {
    Router::new()
        .route("/api/auth/sign-in", post(sign_in))
        .route("/api/auth/verify", post(verify))
        .route("/api/auth/sign-out", post(sign_out))
}

/// The docs for the routes above.
#[derive(OpenApi)]
#[openapi(paths(sign_in, verify, sign_out), components(schemas(SignInBody, VerifyBody, ErrorBody)))]
pub(crate) struct AuthDocs;

/// Request a sign-in link.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignInBody {
    /// The email to send the link to.
    email: String,
    /// A path in the app to return to after signing in, such as `/invite/...`.
    #[serde(default)]
    next: Option<String>,
}

/// Exchange a sign-in link's token for a session.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifyBody {
    /// The `token` query parameter of the link.
    token: String,
}

fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ApiError::BadRequest("body too large".to_owned()));
    }
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|err| {
        let mut errors = FieldErrors::default();
        errors.add("", format!("body is not valid JSON: {err}"));
        ApiError::Validation(errors)
    })?;
    Ok(from_json(value)?)
}

/// Only same-app paths, so the link can't bounce the user to another site.
fn safe_next(next: Option<&str>) -> Option<&str> {
    next.filter(|path| path.starts_with('/') && !path.starts_with("//") && !path.contains('\\'))
}

/// Sends a sign-in link if the email may sign in. Always 202, so the response
/// never says whether an account exists.
#[utoipa::path(
    post,
    path = "/api/auth/sign-in",
    operation_id = "auth.sign_in",
    tags = ["auth"],
    request_body = SignInBody,
    responses(
        (status = 202, description = "A link was sent if the email may sign in"),
        (status = 403, description = "Forbidden", body = ErrorBody),
        (status = 422, description = "Unprocessable Entity", body = ErrorBody),
        (status = 500, description = "Internal Server Error", body = ErrorBody),
    )
)]
pub(crate) async fn sign_in(
    Extension(platform): Extension<Platform>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = request_id(&headers);
    respond(
        &request_id,
        sign_in_inner(&platform, &request_id, &body).await.map(|()| StatusCode::ACCEPTED),
    )
}

async fn sign_in_inner(platform: &Platform, request_id: &str, body: &[u8]) -> Result<(), ApiError> {
    let body: SignInBody = parse(body)?;
    let email = normalize_email(&body.email);
    if !is_plausible_email(&email) {
        let mut errors = FieldErrors::default();
        errors.add("email", "must be an email address");
        return Err(errors.into());
    }
    let now = platform.now();
    let mut tx = platform.db().write().await?.with_request_id(request_id);
    if !admitted(platform, &mut tx, &email).await? {
        tracing::info!(request_id, "sign-in requested for an email no policy or invitation admits");
        return Ok(());
    }
    if let Some(sent) = magic_links::latest_sent_at(&mut *tx, &email).await?
        && sent > now.minus(LINK_COOLDOWN)
    {
        tracing::info!(request_id, "sign-in link cooldown: not sending another yet");
        return Ok(());
    }
    let token = random_token();
    magic_links::insert(&mut *tx, &hash_token(&token), &email, now, now.plus(LINK_TTL)).await?;
    let mut link =
        platform.base_url().join("/sign-in/verify").context("building the sign-in link")?;
    link.query_pairs_mut().append_pair("token", &token);
    if let Some(next) = safe_next(body.next.as_deref()) {
        link.query_pairs_mut().append_pair("next", next);
    }
    let mail = crate::deps::Mail {
        to: email,
        subject: "Your bizkit sign-in link".to_owned(),
        text: format!(
            "Sign in to bizkit with this link:\n\n{link}\n\nIt works once and expires in 15 minutes. \
             If you didn't ask for it, ignore this mail."
        ),
        html: None,
        unsubscribe: None,
        id: None,
        workspace_id: None,
        reply_to: None,
    };
    platform.bus().enqueue(&mut tx, &SendMail(mail), Enqueue::default()).await?;
    tx.commit().await?;
    Ok(())
}

/// An existing user, an open invitation or a `sign_up` policy admits an email.
async fn admitted(platform: &Platform, tx: &mut WriteTx, email: &str) -> Result<bool, ApiError> {
    if users::find_by_email(&mut **tx, email).await?.is_some() {
        return Ok(true);
    }
    if invitations::has_open(&mut **tx, email, platform.now()).await? {
        return Ok(true);
    }
    Ok(platform.policies().match_email(email).is_some_and(|policy| policy.sign_up))
}

/// Consumes a sign-in link and starts a session.
#[utoipa::path(
    post,
    path = "/api/auth/verify",
    operation_id = "auth.verify",
    tags = ["auth"],
    request_body = VerifyBody,
    responses(
        (status = 204, description = "Signed in; the session cookie is set"),
        (status = 401, description = "The link is unknown, used or expired", body = ErrorBody),
        (status = 403, description = "Forbidden", body = ErrorBody),
        (status = 422, description = "Unprocessable Entity", body = ErrorBody),
        (status = 500, description = "Internal Server Error", body = ErrorBody),
    )
)]
pub(crate) async fn verify(
    Extension(platform): Extension<Platform>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = request_id(&headers);
    let user_agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok());
    let result = verify_inner(&platform, &body, user_agent).await.and_then(|token| {
        let cookie = session_cookie_header(&platform, &token, platform.session_ttl())?;
        Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]))
    });
    respond(&request_id, result)
}

async fn verify_inner(
    platform: &Platform,
    body: &[u8],
    user_agent: Option<&str>,
) -> Result<String, ApiError> {
    let body: VerifyBody = parse(body)?;
    let now = platform.now();
    let mut tx = platform.db().write().await?;
    let email = magic_links::consume(&mut *tx, &hash_token(&body.token), now)
        .await?
        .ok_or(ApiError::Unauthenticated)?;
    let user = if let Some(user) = users::find_by_email(&mut *tx, &email).await? {
        report_sign_in(platform, &mut tx, user.id, UsageKind::SignedIn).await?;
        user
    } else {
        // Config or invitations may have changed since the link went out.
        if !admitted(platform, &mut tx, &email).await? {
            return Err(ApiError::Unauthenticated);
        }
        let user = users::insert(&mut *tx, &email, now).await?;
        report_sign_in(platform, &mut tx, user.id, UsageKind::SignedUp).await?;
        apply_policy_joins(platform, &mut tx, &email, user.id).await?;
        user
    };
    users::touch_sign_in(&mut *tx, user.id, now).await?;
    let token = random_token();
    let session = sessions::NewSession {
        token_hash: &hash_token(&token),
        user_id: user.id,
        expires_at: now.plus(platform.session_ttl()),
        user_agent,
    };
    sessions::insert(&mut *tx, session, now).await?;
    tx.commit().await?;
    Ok(token)
}

async fn report_sign_in(
    platform: &Platform,
    tx: &mut WriteTx,
    user_id: crate::ids::UserId,
    kind: UsageKind,
) -> anyhow::Result<()> {
    let usage = UsageReported {
        user_id: Some(user_id),
        kind,
        workspace_id: None,
        at: platform.now(),
        affected: Vec::new(),
    };
    usage::publish(platform.bus(), tx, &usage).await
}

/// Joins a new account to the existing workspaces its policy names. Joins run
/// once, at account creation, so a later `members.remove` sticks.
async fn apply_policy_joins(
    platform: &Platform,
    tx: &mut WriteTx,
    email: &str,
    user_id: crate::ids::UserId,
) -> Result<(), ApiError> {
    let Some(policy) = platform.policies().match_email(email) else { return Ok(()) };
    for join in &policy.join {
        if let Some(workspace) = workspaces::find_by_slug(&mut **tx, &join.workspace).await? {
            if members::insert(&mut **tx, workspace.id, user_id, join.role, platform.now()).await? {
                let usage = UsageReported {
                    user_id: Some(user_id),
                    kind: UsageKind::MemberJoined,
                    workspace_id: Some(workspace.id),
                    at: platform.now(),
                    affected: Vec::new(),
                };
                usage::publish(platform.bus(), tx, &usage).await?;
            }
        } else {
            tracing::warn!(policy = %policy.name, workspace = %join.workspace, "policy join names no workspace; skipped");
        }
    }
    Ok(())
}

/// Ends the current session.
#[utoipa::path(
    post,
    path = "/api/auth/sign-out",
    operation_id = "auth.sign_out",
    tags = ["auth"],
    security(("session" = [])),
    responses(
        (status = 204, description = "Signed out; the session cookie is cleared"),
        (status = 403, description = "Forbidden", body = ErrorBody),
        (status = 500, description = "Internal Server Error", body = ErrorBody),
    )
)]
pub(crate) async fn sign_out(
    Extension(platform): Extension<Platform>,
    headers: HeaderMap,
) -> Response {
    let request_id = request_id(&headers);
    let result = async {
        if let Some(token) = super::middleware::session_cookie(&headers) {
            let mut tx = platform.db().write().await?;
            sessions::delete(&mut *tx, &hash_token(&token)).await?;
            tx.commit().await?;
        }
        let cookie = session_cookie_header(&platform, "", Duration::ZERO)?;
        Ok::<_, ApiError>((StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]))
    }
    .await;
    respond(&request_id, result)
}

fn session_cookie_header(
    platform: &Platform,
    token: &str,
    max_age: Duration,
) -> Result<HeaderValue, ApiError> {
    let secure = if platform.base_url().scheme() == "https" { "; Secure" } else { "" };
    let cookie = format!(
        "{}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{secure}",
        super::SESSION_COOKIE,
        max_age.as_secs()
    );
    HeaderValue::from_str(&cookie)
        .context("building the session cookie")
        .map_err(ApiError::Internal)
}

fn respond(request_id: &str, result: Result<impl IntoResponse, ApiError>) -> Response {
    match result {
        Ok(response) => response.into_response(),
        Err(err) => {
            err.log(request_id);
            err.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_must_stay_in_the_app() {
        assert_eq!(safe_next(Some("/invite/abc")), Some("/invite/abc"), "app path");
        assert_eq!(safe_next(Some("//evil.example")), None, "protocol-relative");
        assert_eq!(safe_next(Some("https://evil.example")), None, "absolute");
        assert_eq!(safe_next(Some("/\\evil.example")), None, "backslash");
    }
}
