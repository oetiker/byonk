//! `/dev/api/screen-repos`: screen-repo management for the dev UI.
//!
//! Thin wrappers over the same cores the token-gated admin routes call
//! (`api::admin::write::*_core`), so the two surfaces cannot drift. What the
//! dev surface adds is the shape the UI needs (`kind`, `writable`, `managed`,
//! `has_token`, top-level `addon_mode`), a URL check that answers 400 before
//! anything is written, and a refusal of the reserved handles, which byonk
//! registers itself (`byonk-builtin`, `local`, `examples`).
//!
//! There is no rename: a repo's handle is the prefix of every screen ref that
//! names it, and the admin PATCH does not rename either.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::api::admin::read::ScreenRepoInfo;
use crate::api::admin::write::{
    add_screen_repo_core, delete_screen_repo_core, patch_screen_repo_core, require_writable_global,
    screen_repo_info_now, update_screen_repo_core, ScreenRepoWrite,
};
use crate::error::ApiError;
use crate::server::AppState;
use crate::services::git_fetch::validate_repo;
use crate::services::screen_repo_loader::{ScreenRepoKind, BUILTIN_HANDLE};
use crate::services::screen_repo_manager::{EXAMPLES_HANDLE, LOCAL_HANDLE, RESERVED_HANDLES};

/// One repo as the UI sees it. The token is never here, only `has_token`.
#[derive(Serialize)]
struct RepoOut {
    handle: String,
    /// `builtin` | `local` | `examples` | `git`.
    kind: &'static str,
    repo: Option<String>,
    pin: Option<String>,
    has_token: bool,
    /// `ready` | `fetching` | `error` | `offline`.
    status: String,
    pin_kind: Option<crate::services::git_fetch::PinKind>,
    resolved_sha: Option<String>,
    last_fetched: Option<String>,
    error: Option<String>,
    screen_count: usize,
    /// Screens in this repo can be edited from the UI.
    writable: bool,
    /// Registered by byonk itself: cannot be edited, updated or deleted here.
    managed: bool,
}

#[derive(Serialize)]
struct ReposOut {
    /// Config writes are refused in add-on mode; the UI shows a read-only note.
    addon_mode: bool,
    repos: Vec<RepoOut>,
}

fn is_managed(handle: &str) -> bool {
    RESERVED_HANDLES.contains(&handle)
}

fn out(state: &AppState, info: ScreenRepoInfo) -> RepoOut {
    let source = state.screen_repo_manager.loader().source_for(&info.handle);
    let writable = source.as_ref().is_some_and(|s| s.writable_root().is_some());
    let kind = match info.handle.as_str() {
        BUILTIN_HANDLE => "builtin",
        EXAMPLES_HANDLE => "examples",
        LOCAL_HANDLE => "local",
        _ => match source.map(|s| s.kind()) {
            Some(ScreenRepoKind::Local) => "local",
            _ => "git",
        },
    };
    RepoOut {
        kind,
        has_token: info.token_set,
        writable,
        managed: is_managed(&info.handle),
        repo: info.repo,
        pin: info.pin,
        status: info.status,
        pin_kind: info.pin_kind,
        resolved_sha: info.resolved_sha,
        last_fetched: info.last_fetched,
        error: info.error,
        screen_count: info.screen_count,
        handle: info.handle,
    }
}

fn refuse_managed(handle: &str) -> Result<(), ApiError> {
    if is_managed(handle) {
        return Err(ApiError::Conflict(format!(
            "`{handle}` is registered by byonk itself and cannot be changed here"
        )));
    }
    Ok(())
}

/// A handle is the prefix of every screen ref (`handle/path`), so it must be a
/// single plain path segment.
fn check_handle(handle: &str) -> Result<(), ApiError> {
    let mut chars = handle.chars();
    let ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(ApiError::BadRequest(format!(
            "invalid handle `{handle}`: use lowercase letters, digits, `-` and `_`, starting with a letter or digit"
        )))
    }
}

fn check_url(repo: &str) -> Result<(), ApiError> {
    validate_repo(repo).map_err(|e| ApiError::BadRequest(e.to_string()))
}

/// `GET /dev/api/screen-repos`.
async fn list(State(state): State<AppState>) -> Json<ReposOut> {
    let loader = state.screen_repo_manager.loader();
    let mut handles: std::collections::BTreeSet<String> = loader.handles().into_iter().collect();
    handles.extend(state.config.load().screen_repos.keys().cloned());
    let repos = handles
        .into_iter()
        .map(|h| out(&state, screen_repo_info_now(&state, h)))
        .collect();
    Json(ReposOut {
        addon_mode: state.addon_mode,
        repos,
    })
}

/// `POST /dev/api/screen-repos` `{handle, repo, pin?, token?}`. Answers the
/// new repo; its first fetch runs in the background, so poll the list.
async fn add(
    State(state): State<AppState>,
    Json(body): Json<ScreenRepoWrite>,
) -> Result<Json<RepoOut>, ApiError> {
    require_writable_global(&state)?;
    let handle = body
        .handle
        .clone()
        .ok_or_else(|| ApiError::BadRequest("`handle` is required".into()))?;
    refuse_managed(&handle)?;
    check_handle(&handle)?;
    let repo = body
        .repo
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("`repo` is required".into()))?;
    check_url(repo)?;
    let info = add_screen_repo_core(&state, body).await?;
    Ok(Json(out(&state, info)))
}

/// `PATCH /dev/api/screen-repos/{handle}` `{repo?, pin?, token?}`: an omitted
/// field keeps its value (an empty string for `pin` or `token` removes it). A `handle` in the body that differs from the path is a
/// 400: repos are not renamed.
async fn patch(
    State(state): State<AppState>,
    Path(handle): Path<String>,
    Json(body): Json<ScreenRepoWrite>,
) -> Result<Json<RepoOut>, ApiError> {
    require_writable_global(&state)?;
    refuse_managed(&handle)?;
    if body.handle.as_deref().is_some_and(|h| h != handle) {
        return Err(ApiError::BadRequest(
            "a screen repo cannot be renamed; delete it and add it again".into(),
        ));
    }
    if let Some(repo) = body.repo.as_deref() {
        check_url(repo)?;
    }
    let info = patch_screen_repo_core(&state, handle, body).await?;
    Ok(Json(out(&state, info)))
}

/// `DELETE /dev/api/screen-repos/{handle}`: `{"deleted": handle}`; 409 while
/// a device names one of its screens (the message lists the devices).
async fn delete(
    State(state): State<AppState>,
    Path(handle): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_writable_global(&state)?;
    refuse_managed(&handle)?;
    delete_screen_repo_core(&state, &handle).await?;
    Ok(Json(serde_json::json!({ "deleted": handle })))
}

/// `POST /dev/api/screen-repos/{handle}/update`: refetch in the background.
/// Allowed in add-on mode (it writes no config).
async fn update(
    State(state): State<AppState>,
    Path(handle): Path<String>,
) -> Result<Json<RepoOut>, ApiError> {
    refuse_managed(&handle)?;
    let info = update_screen_repo_core(&state, handle)?;
    Ok(Json(out(&state, info)))
}

pub fn repos_router() -> Router<AppState> {
    Router::new()
        .route("/screen-repos", get(list).post(add))
        .route(
            "/screen-repos/{handle}",
            axum::routing::patch(patch).delete(delete),
        )
        .route("/screen-repos/{handle}/update", post(update))
}
