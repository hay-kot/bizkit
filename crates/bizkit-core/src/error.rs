//! The API error type and the JSON error body every non-2xx response carries.

use std::collections::BTreeMap;

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::validate::FieldErrors;

/// An error an operation or route returns to its caller.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The request is malformed in a way field errors can't describe.
    #[error("{0}")]
    BadRequest(String),
    /// No valid session or API key.
    #[error("not signed in")]
    Unauthenticated,
    /// Signed in, but missing the scope the operation needs.
    #[error("{0}")]
    Forbidden(String),
    /// The resource doesn't exist, or the caller may not know it exists.
    #[error("not found")]
    NotFound,
    /// The request conflicts with current state, such as a taken slug.
    #[error("{0}")]
    Conflict(String),
    /// The input failed deserialization or validation.
    #[error("invalid input")]
    Validation(FieldErrors),
    /// Too many requests; retry later.
    #[error("rate limited")]
    RateLimited {
        /// Seconds until a retry may succeed.
        retry_after_secs: u32,
    },
    /// An unexpected failure. The cause goes to the log, never to the caller.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(err.into())
    }
}

impl From<FieldErrors> for ApiError {
    fn from(errors: FieldErrors) -> Self {
        Self::Validation(errors)
    }
}

/// The machine-readable error kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 400
    BadRequest,
    /// 401
    Unauthenticated,
    /// 403
    Forbidden,
    /// 404
    NotFound,
    /// 409
    Conflict,
    /// 422
    Validation,
    /// 429
    RateLimited,
    /// 500
    Internal,
}

impl ErrorCode {
    /// The HTTP status for this code.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Validation => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// The body of every non-2xx response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    /// What went wrong.
    pub error: ErrorDetail,
}

/// The details of an [`ErrorBody`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ErrorDetail {
    /// The error kind.
    pub code: ErrorCode,
    /// A human-readable message.
    pub message: String,
    /// Problems per input field, keyed by path such as `items[2].name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<BTreeMap<String, Vec<String>>>,
}

impl ApiError {
    /// The error kind.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::BadRequest(_) => ErrorCode::BadRequest,
            Self::Unauthenticated => ErrorCode::Unauthenticated,
            Self::Forbidden(_) => ErrorCode::Forbidden,
            Self::NotFound => ErrorCode::NotFound,
            Self::Conflict(_) => ErrorCode::Conflict,
            Self::Validation(_) => ErrorCode::Validation,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::Internal(_) => ErrorCode::Internal,
        }
    }

    /// The body sent to the caller. `Internal` errors say only "internal error".
    #[must_use]
    pub fn body(&self) -> ErrorBody {
        let message = if let Self::Internal(_) = self {
            "internal error".to_owned()
        } else {
            self.to_string()
        };
        let fields =
            if let Self::Validation(errors) = self { Some(errors.as_map().clone()) } else { None };
        ErrorBody { error: ErrorDetail { code: self.code(), message, fields } }
    }

    /// Logs the cause of an `Internal` error with the request id.
    pub fn log(&self, request_id: &str) {
        if let Self::Internal(err) = self {
            tracing::error!(request_id, error = format!("{err:#}"), "internal error");
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(err) = &self {
            // Handlers that know the request id log through `ApiError::log`
            // first; this catches errors raised outside them.
            tracing::error!(error = format!("{err:#}"), "internal error");
        }
        let status = self.code().status();
        let mut response = (status, Json(self.body())).into_response();
        if let Self::RateLimited { retry_after_secs } = self {
            response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(retry_after_secs));
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_hide_the_cause() {
        let err = ApiError::Internal(anyhow::anyhow!("db password is hunter2"));
        let body = serde_json::to_value(err.body()).unwrap();
        insta::assert_json_snapshot!(body, @r#"
        {
          "error": {
            "code": "internal",
            "message": "internal error"
          }
        }
        "#);
    }

    #[test]
    fn validation_errors_carry_fields() {
        let mut fields = FieldErrors::default();
        fields.add("name", "must not be empty");
        let body = serde_json::to_value(ApiError::Validation(fields).body()).unwrap();
        insta::assert_json_snapshot!(body, @r#"
        {
          "error": {
            "code": "validation",
            "fields": {
              "name": [
                "must not be empty"
              ]
            },
            "message": "invalid input"
          }
        }
        "#);
    }
}
