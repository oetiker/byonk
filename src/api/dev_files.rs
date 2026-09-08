//! Screen file editing endpoints for the dev UI editor.
//!
//! Stub for Task 4 — the real routes (read/write a screen's source files
//! through `AppState.screen_store`) land in Task 6. Mounted under
//! `/dev/api` by `dev::dev_router`, so it already carries `require_dev`
//! protection once populated.

use axum::Router;

use crate::server::AppState;

/// Screen-file routes. Empty until Task 6.
pub fn files_router() -> Router<AppState> {
    Router::new()
}
