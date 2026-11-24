//! Reading files from the multipart bodies that upload routes take.

use axum::extract::multipart::{Field, MultipartError};
use axum::http::StatusCode;
use bytes::{Bytes, BytesMut};

use crate::error::ApiError;
use crate::module::MAX_UPLOAD_BYTES;

/// Reads one multipart file part, at most [`MAX_UPLOAD_BYTES`] long.
///
/// The body limit of an upload route leaves room for framing and other
/// parts, so a handler reads each file through this to hold the file itself
/// to the limit.
///
/// # Errors
///
/// - [`ApiError::PayloadTooLarge`] once the part passes
///   [`MAX_UPLOAD_BYTES`], or when the whole body passes the route's limit.
/// - [`ApiError::BadRequest`] when the multipart stream is malformed or
///   breaks off.
pub async fn read_file(mut field: Field<'_>) -> Result<Bytes, ApiError> {
    let mut file = BytesMut::new();
    while let Some(chunk) = field.chunk().await.map_err(|err| multipart_error(&err))? {
        if file.len() + chunk.len() > MAX_UPLOAD_BYTES {
            return Err(ApiError::PayloadTooLarge(MAX_UPLOAD_BYTES));
        }
        file.extend_from_slice(&chunk);
    }
    Ok(file.freeze())
}

fn multipart_error(err: &MultipartError) -> ApiError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::PayloadTooLarge(MAX_UPLOAD_BYTES)
    } else {
        ApiError::BadRequest(err.body_text())
    }
}
