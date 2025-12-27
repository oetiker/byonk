use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
    Json as JsonExtractor,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Request body for log submission
#[derive(Debug, Deserialize, ToSchema)]
pub struct LogRequest {
    /// Array of log entries from the device
    #[serde(default)]
    pub logs: Vec<Value>,
}

/// Response from log submission
#[derive(Debug, Serialize, ToSchema)]
pub struct LogResponse {
    /// Status code (200 = success)
    pub status: u16,
    /// Status message
    pub message: String,
}

/// Submit device logs
///
/// Devices send diagnostic logs when they encounter errors or issues.
#[utoipa::path(
    post,
    path = "/api/log",
    request_body = LogRequest,
    responses(
        (status = 200, description = "Logs received successfully", body = LogResponse),
    ),
    params(
        ("ID" = String, Header, description = "Device MAC address"),
        ("Access-Token" = String, Header, description = "API key from /api/setup"),
    ),
    tag = "Logging"
)]
pub async fn handle_log(
    JsonExtractor(log_request): JsonExtractor<LogRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // For POC, just log to tracing
    for (i, entry) in log_request.logs.iter().enumerate() {
        tracing::debug!(
            log_index = i,
            entry = %serde_json::to_string(entry).unwrap_or_default(),
            "Device log entry"
        );
    }

    tracing::info!(log_count = log_request.logs.len(), "Device logs received");

    Ok(Json(LogResponse {
        status: 200,
        message: "Logs received".to_string(),
    }))
}
