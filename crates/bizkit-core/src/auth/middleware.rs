//! Authenticates every request from its session cookie or bearer API key.

use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};

use super::{API_KEY_PREFIX, SESSION_COOKIE, api_keys, hash_token, sessions};
use crate::app::Platform;
use crate::error::ApiError;
use crate::ops::{KeyPrincipal, Principal, Scope, UserPrincipal};

/// Session and key lookups write `last_seen_at`/`last_used_at` at most this often.
const TOUCH_EVERY: Duration = Duration::from_hours(1);

/// The caller, as the authenticate layer found it. Every request has one.
#[derive(Debug, Clone, Default)]
pub struct Authn {
    /// The caller, or `None` when anonymous or the credential is invalid.
    pub principal: Option<Principal>,
}

/// Resolves the caller and enforces the CSRF rule for cookie requests.
pub(crate) async fn authenticate(
    State(platform): State<Platform>,
    mut req: Request,
    next: Next,
) -> Response {
    let headers = req.headers();
    let result = match (bearer(headers), session_cookie(headers)) {
        (Some(token), _) => {
            key_principal(&platform, &token).await.map(|p| (p.map(Principal::ApiKey), false))
        }
        (None, Some(token)) => session_principal(&platform, &token).await.map(|p| {
            let via_cookie = p.is_some();
            (p.map(Principal::User), via_cookie)
        }),
        (None, None) => Ok((None, false)),
    };
    let (principal, via_cookie) = match result {
        Ok(found) => found,
        Err(err) => return ApiError::Internal(err).into_response(),
    };
    if via_cookie && !is_safe(req.method()) && !same_origin(req.headers(), &platform) {
        return ApiError::Forbidden("cross-site request refused".to_owned()).into_response();
    }
    req.extensions_mut().insert(Authn { principal });
    next.run(req).await
}

/// Rejects requests that didn't authenticate with an API key. Guards `/mcp`.
pub(crate) async fn require_api_key(req: Request, next: Next) -> Response {
    let has_key = req
        .extensions()
        .get::<Authn>()
        .is_some_and(|authn| matches!(authn.principal, Some(Principal::ApiKey(_))));
    if has_key { next.run(req).await } else { ApiError::Unauthenticated.into_response() }
}

const fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// A cookie-authenticated unsafe request must come from the app's own origin.
fn same_origin(headers: &HeaderMap, platform: &Platform) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    platform.base_url().origin().ascii_serialization() == origin
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))?;
    Some(token.trim().to_owned())
}

/// The session token from the `Cookie` header, if any.
pub(crate) fn session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_owned())
}

async fn session_principal(
    platform: &Platform,
    token: &str,
) -> anyhow::Result<Option<UserPrincipal>> {
    let now = platform.now();
    let hash = hash_token(token);
    let Some(session) = sessions::find(platform.db().read(), &hash, now).await? else {
        return Ok(None);
    };
    if session.last_seen_at < now.minus(TOUCH_EVERY) {
        let mut tx = platform.db().write().await?;
        sessions::touch(&mut *tx, &hash, now, now.plus(platform.session_ttl())).await?;
        tx.commit().await?;
    }
    Ok(Some(UserPrincipal { user_id: session.user_id, email: session.email }))
}

async fn key_principal(platform: &Platform, token: &str) -> anyhow::Result<Option<KeyPrincipal>> {
    if !token.starts_with(API_KEY_PREFIX) {
        return Ok(None);
    }
    let now = platform.now();
    let Some(key) = api_keys::find_active(platform.db().read(), &hash_token(token), now).await?
    else {
        return Ok(None);
    };
    if key.last_used_at.is_none_or(|at| at < now.minus(TOUCH_EVERY)) {
        let mut tx = platform.db().write().await?;
        api_keys::touch(&mut *tx, key.id, now).await?;
        tx.commit().await?;
    }
    let stored: Vec<String> = serde_json::from_str(&key.scopes).unwrap_or_default();
    let scopes = stored
        .iter()
        .filter_map(|text| match Scope::parse(text, platform.scope_areas()) {
            Ok(scope) => Some(scope),
            Err(err) => {
                // A module that declared this area is no longer compiled in.
                tracing::warn!(key_id = %key.id, error = %err, "ignoring an API key scope");
                None
            }
        })
        .collect();
    Ok(Some(KeyPrincipal { key_id: key.id, workspace_id: key.workspace_id, scopes }))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn reads_the_session_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("theme=dark; bizkit_session=abc; other=1"),
        );
        assert_eq!(session_cookie(&headers).as_deref(), Some("abc"), "cookie value");
    }

    #[test]
    fn reads_bearer_tokens() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer bk_abc"));
        assert_eq!(bearer(&headers).as_deref(), Some("bk_abc"), "bearer");
    }
}
