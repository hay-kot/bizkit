//! Test-only routes under `/__test`, for end-to-end tests that drive a real
//! server. Built only with the `test-hooks` feature.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::deps::{Clock, SystemClock};
use crate::error::ApiError;
use crate::jobs::Worker;
use crate::time::UnixMillis;

/// The wall clock plus an offset that tests can move forward.
#[derive(Debug, Default)]
pub struct OffsetClock {
    offset_ms: AtomicI64,
}

impl OffsetClock {
    /// Moves the clock forward by `ms`.
    pub fn advance(&self, ms: i64) {
        self.offset_ms.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for OffsetClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(SystemClock.now().0.saturating_add(self.offset_ms.load(Ordering::SeqCst)))
    }
}

#[derive(Debug, Clone)]
struct HookState {
    clock: Arc<OffsetClock>,
    worker: Worker,
}

#[derive(Debug, Deserialize)]
struct Advance {
    advance_ms: i64,
}

#[derive(Debug, Serialize)]
struct Drained {
    done: u32,
    retried: u32,
    dead: u32,
}

/// `POST /__test/clock {advance_ms}` and `POST /__test/jobs/drain`, which
/// flushes module buffers before it runs due jobs.
pub fn router(clock: Arc<OffsetClock>, worker: Worker) -> Router {
    Router::new()
        .route("/__test/clock", post(advance))
        .route("/__test/jobs/drain", post(drain))
        .with_state(HookState { clock, worker })
}

async fn advance(State(state): State<HookState>, Json(body): Json<Advance>) -> Response {
    state.clock.advance(body.advance_ms);
    Json(serde_json::json!({ "now": state.clock.now() })).into_response()
}

async fn drain(State(state): State<HookState>) -> Response {
    if let Err(err) = state.worker.platform().flush_modules().await {
        return ApiError::Internal(err).into_response();
    }
    match state.worker.run_until_idle().await {
        Ok(run) => {
            Json(Drained { done: run.done, retried: run.retried, dead: run.dead }).into_response()
        }
        Err(err) => ApiError::Internal(err).into_response(),
    }
}
