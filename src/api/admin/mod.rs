//! Admin/management API (`/api/admin/*`), gated by a bearer token.

pub mod read;

use axum::{http::HeaderMap, routing::get, Router};

use crate::error::ApiError;
use crate::server::AppState;

/// Enforce admin auth. Returns 404 when admin is disabled (no token configured),
/// 401 when the token is missing or wrong.
pub fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.admin_token.as_deref() else {
        return Err(ApiError::NotFound); // admin disabled ⇒ invisible
    };
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match provided {
        Some(tok) if constant_time_eq(tok.as_bytes(), expected.as_bytes()) => Ok(()),
        _ => Err(ApiError::Unauthorized),
    }
}

/// Constant-time byte comparison (avoids token-length/timing leaks).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// All admin routes, to be nested under `/api/admin`.
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/devices", get(read::list_devices))
        .route("/pending", get(read::pending))
        .route("/config", get(read::get_config))
}
