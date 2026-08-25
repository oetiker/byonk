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
use crate::services::recovery::{spellings_of, RecoverySession, DEFAULT_WIPES};
use crate::services::DeviceRegistry;

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

/// Every name the device addressed by `key` answers to, canonical first.
///
/// A device has two: the MAC that `/api/admin/devices` reports once it has
/// checked in, and the `config.yaml` key it is listed under, which may be a
/// registration code. Only the registry can connect the two spellings, so a
/// device that has never checked in is addressed by the given key alone — no
/// loss, since it has no MAC to be found by yet.
async fn device_names(state: &AppState, key: &str) -> Vec<DeviceId> {
    let normalized = key.to_uppercase().replace('-', "");
    let resolved = match state.registry.list_all().await {
        Ok(devices) => devices.into_iter().find(|d| {
            d.device_id.to_string().eq_ignore_ascii_case(key)
                || d.api_key.registration_code() == normalized
        }),
        Err(_) => None,
    };

    // Unknown device: the key is all there is. Still worth spelling out, since
    // a code typed one way must find a session filed the other way.
    let Some(device) = resolved else {
        return spellings_of(key);
    };

    // The MAC leads, so a run with no session yet is filed under the name the
    // device list reports rather than whichever name the caller happened to use.
    let mut names = vec![DeviceId::new(device.device_id.to_string())];
    for id in spellings_of(&device.api_key.registration_code())
        .into_iter()
        .chain(spellings_of(key))
    {
        if !names.contains(&id) {
            names.push(id);
        }
    }
    names
}

/// The `DeviceId` a recovery session for `key` lives under.
///
/// This route takes the same `{key}` as `PATCH /devices/{key}`, so either of a
/// device's names can arrive here, and `/api/display` already looks under both.
/// Without the same resolution the two ends disagree: a run started under one
/// name reports inactive under the other, and cancelling it silently does
/// nothing while the panel goes on wiping.
async fn recovery_id(state: &AppState, key: &str) -> DeviceId {
    let names = device_names(state, key).await;
    state
        .recovery
        .resolve_key(&names)
        .await
        .unwrap_or_else(|| names[0].clone())
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
    // Resolved so that restarting under the device's other name replaces the
    // run in progress, rather than filing a second one it cannot see.
    let device_id = recovery_id(&state, &key).await;
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
    let device_id = recovery_id(&state, &key).await;
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
    let session = state.recovery.get(&recovery_id(&state, &key).await).await;
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
