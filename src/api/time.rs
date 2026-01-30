use axum::response::{IntoResponse, Json};
use serde::Serialize;

/// Response from the /api/time endpoint
#[derive(Debug, Serialize)]
pub struct TimeResponse {
    /// Current server time in milliseconds since Unix epoch
    pub timestamp_ms: u64,
}

/// Return current server time for Ed25519 signature generation
pub async fn handle_time() -> impl IntoResponse {
    let timestamp_ms = chrono::Utc::now().timestamp_millis() as u64;
    Json(TimeResponse { timestamp_ms })
}
