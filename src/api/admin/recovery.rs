//! Admin endpoints for panel-recovery sessions.
//!
//! Recovery asks a device to run full-panel wipes instead of showing content,
//! to clear ghosting and burn-in. See [`crate::services::recovery`] for why
//! sessions are in-memory only.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::models::DeviceId;
use crate::server::AppState;
use crate::services::recovery::{RecoverySession, DEFAULT_WIPES};

use super::require_admin;

#[derive(Debug, Deserialize)]
pub struct RecoverStart {
    /// Wipes to run. Clamped to `1..=MAX_WIPES`; defaults to `DEFAULT_WIPES`.
    pub wipes: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct RecoveryStatus {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<Option<RecoverySession>> for RecoveryStatus {
    fn from(session: Option<RecoverySession>) -> Self {
        match session {
            Some(s) => RecoveryStatus {
                active: true,
                total: Some(s.total),
                done: Some(s.done),
                remaining: Some(s.remaining()),
                started_at: Some(s.started_at),
            },
            None => RecoveryStatus {
                active: false,
                total: None,
                done: None,
                remaining: None,
                started_at: None,
            },
        }
    }
}

/// Start a recovery session for a device.
///
/// The device is not contacted here. It picks the work up on its next poll,
/// which is also why starting a session for a device that never checks in is
/// harmless rather than an error.
pub async fn start_recovery(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    body: Option<Json<RecoverStart>>,
) -> Result<Json<RecoveryStatus>, ApiError> {
    require_admin(&state, &headers)?;
    let wipes = body.and_then(|Json(b)| b.wipes).unwrap_or(DEFAULT_WIPES);
    let device_id = DeviceId::new(&key);
    let session = state.recovery.start(&device_id, wipes).await;
    tracing::info!(
        device = %key,
        wipes = session.total,
        "Panel recovery session started"
    );
    Ok(Json(Some(session).into()))
}

/// Cancel a recovery session. Cancelling when none is running is not an error;
/// the reported state is the same either way.
pub async fn cancel_recovery(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RecoveryStatus>, ApiError> {
    require_admin(&state, &headers)?;
    let device_id = DeviceId::new(&key);
    if state.recovery.cancel(&device_id).await {
        tracing::info!(device = %key, "Panel recovery session cancelled");
    }
    Ok(Json(None.into()))
}

/// Report the recovery state of a device.
pub async fn get_recovery(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RecoveryStatus>, ApiError> {
    require_admin(&state, &headers)?;
    let session = state.recovery.get(&DeviceId::new(&key)).await;
    Ok(Json(session.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_of_no_session_is_inactive_and_bare() {
        let status: RecoveryStatus = None.into();
        assert!(!status.active);
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json, serde_json::json!({"active": false}));
    }

    #[test]
    fn status_of_a_session_reports_progress() {
        let session = RecoverySession::for_test(10, 3);
        let status: RecoveryStatus = Some(session).into();
        assert!(status.active);
        assert_eq!(status.total, Some(10));
        assert_eq!(status.done, Some(3));
        assert_eq!(status.remaining, Some(7));
    }

    #[test]
    fn absent_wipes_field_deserializes_to_none() {
        let body: RecoverStart = serde_json::from_str("{}").unwrap();
        assert_eq!(body.wipes, None);
    }
}
