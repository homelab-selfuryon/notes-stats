use std::sync::atomic::Ordering;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

use crate::{AppState, notes_api::CallError};

pub fn router(state: AppState) -> Router {
    let timeout = state.config.request_timeout;
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/v1/stats", get(stats))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            timeout,
        ))
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

/// Readiness: fails while shutting down so the endpoint leaves the Service. It does not depend on
/// notes-api: this app is up even when its dependency is not.
async fn readyz(State(state): State<AppState>) -> (StatusCode, &'static str) {
    if state.ready.load(Ordering::Relaxed) {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "shutting down")
    }
}

/// Calls notes-api with the projected token and reports what it learned.
async fn stats(State(state): State<AppState>) -> Result<Json<Value>, StatsError> {
    let count = state.notes_api.count().await?;
    Ok(Json(json!({
        "source": "notes-api",
        "notes": { "count": count },
        "env": state.config.app_env,
    })))
}

struct StatsError(CallError);

impl From<CallError> for StatsError {
    fn from(e: CallError) -> Self {
        Self(e)
    }
}

impl IntoResponse for StatsError {
    fn into_response(self) -> Response {
        tracing::error!(error = %self.0, "call to notes-api failed");
        let (status, message) = match self.0 {
            CallError::Token(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "no credentials for notes-api",
            ),
            CallError::Unreachable(_) => (StatusCode::BAD_GATEWAY, "notes-api is unreachable"),
            CallError::Status(401 | 403) => (StatusCode::BAD_GATEWAY, "notes-api refused the call"),
            CallError::Status(_) => (StatusCode::BAD_GATEWAY, "notes-api failed"),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
