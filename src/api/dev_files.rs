//! `/dev/api` screen + file routes: thin JSON wrappers over `ScreenStore`.

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::server::AppState;
use crate::services::screen_store::{Issue, StarterKind, StoreError, ValidationReport};

fn map_store(e: StoreError) -> ApiError {
    match e {
        StoreError::ReadOnly { copy_hint } => ApiError::Conflict(copy_hint),
        StoreError::NotFound => ApiError::NotFound,
        StoreError::Conflict => {
            ApiError::PreconditionFailed("file changed on disk or target exists".into())
        }
        StoreError::BinaryOverwrite => ApiError::Unprocessable("target is a binary asset".into()),
        StoreError::Traversal => ApiError::BadRequest("path escapes the screen directory".into()),
        StoreError::TooLarge => ApiError::PayloadTooLarge,
        StoreError::Io(m) => ApiError::Internal(m),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

#[derive(Serialize)]
struct RepoOut {
    handle: String,
    writable: bool,
}

#[derive(Deserialize)]
struct RefQuery {
    r#ref: String,
}

#[derive(Deserialize)]
struct FileQuery {
    r#ref: String,
    name: String,
}

#[derive(Serialize)]
struct FileOut {
    name: String,
    size: Option<u64>,
    text: bool,
}

#[derive(Deserialize)]
struct CreateIn {
    handle: String,
    name: String,
    #[serde(default = "minimal")]
    starter: String,
}
fn minimal() -> String {
    "minimal".into()
}

#[derive(Deserialize)]
struct CopyIn {
    r#ref: String,
    to_handle: String,
    to_name: String,
}

#[derive(Deserialize)]
struct RenameIn {
    r#ref: String,
    name: String,
}

#[derive(Deserialize)]
struct CheckIn {
    r#ref: String,
    file: String,
    text: String,
}

#[derive(Serialize)]
struct IssueOut {
    severity: &'static str,
    location: String,
    message: String,
    line: Option<u32>,
}

#[derive(Serialize)]
struct ReportOut {
    ok: bool,
    issues: Vec<IssueOut>,
}

impl From<ValidationReport> for ReportOut {
    fn from(r: ValidationReport) -> Self {
        let issues = r
            .issues
            .into_iter()
            .map(|i: Issue| IssueOut {
                severity: match i.severity {
                    crate::services::screen_store::Severity::Error => "error",
                    crate::services::screen_store::Severity::Warning => "warning",
                },
                location: i.location,
                message: i.message,
                line: i.line,
            })
            .collect();
        ReportOut { ok: r.ok, issues }
    }
}

async fn repos(State(s): State<AppState>) -> Result<Json<Vec<RepoOut>>, ApiError> {
    let store = s.screen_store.clone();
    let loader = s.screen_repo_manager.loader();
    let mut handles: Vec<String> = s.config.load().screen_repos.keys().cloned().collect();
    for e in blocking(move || store.list_screens()).await? {
        if !handles.contains(&e.handle) {
            handles.push(e.handle);
        }
    }
    let mut out: Vec<RepoOut> = handles
        .into_iter()
        .map(|h| RepoOut {
            writable: loader
                .source_for(&h)
                .map(|src| src.writable_root().is_some())
                .unwrap_or(false),
            handle: h,
        })
        .collect();
    out.sort_by(|a, b| b.writable.cmp(&a.writable).then(a.handle.cmp(&b.handle)));
    Ok(Json(out))
}

async fn files(
    State(s): State<AppState>,
    Query(q): Query<RefQuery>,
) -> Result<Json<Vec<FileOut>>, ApiError> {
    let store = s.screen_store.clone();
    let list = blocking(move || store.list_files(&q.r#ref))
        .await?
        .map_err(map_store)?;
    Ok(Json(
        list.into_iter()
            .map(|f| FileOut {
                name: f.name,
                size: f.size,
                text: f.text,
            })
            .collect(),
    ))
}

async fn read_file(
    State(s): State<AppState>,
    Query(q): Query<FileQuery>,
) -> Result<Response, ApiError> {
    let store = s.screen_store.clone();
    let f = blocking(move || store.read_file(&q.r#ref, &q.name))
        .await?
        .map_err(map_store)?;
    if f.binary {
        return Err(ApiError::UnsupportedMedia("binary file".into()));
    }
    Ok((
        [
            (header::ETAG, f.etag),
            (header::CONTENT_TYPE, "text/plain; charset=utf-8".into()),
        ],
        f.bytes,
    )
        .into_response())
}

/// Write one file. `If-Match` is required (missing -> 428, distinguishing
/// "you forgot the header" from "your header did not match" — an addition
/// over the spec, not present in RFC 9110). `If-Match: *` deliberately means
/// "create; fail with 412 if it already exists" — the opposite of RFC 9110's
/// meaning for that header on other methods — per the binding design spec
/// (`docs/superpowers/specs/2026-09-07-dev-mode-editor-design.md:198`).
///
/// The "*" case can't be expressed as a single `ScreenStore::write_file`
/// call: that method's own `if_match` either skips the check entirely
/// (`None`, unconditional overwrite-or-create) or requires an exact etag
/// match (`Some`, which fails "create" against a file that doesn't exist
/// yet). So this probes with `read_file` first to decide `None` (file
/// absent: create) vs. an if_match value that can never match a real etag
/// (file present: force `Conflict` — but only once `write_file`'s own
/// read-only check has already run, so a read-only repo still reports
/// `ReadOnly` regardless of whether the target happens to exist there).
/// The probe and the write are two separate `ScreenStore` calls (each
/// individually locked), so a concurrent write landing between them is a
/// theoretical race; acceptable for a single-operator dev editor.
async fn write_file(
    State(s): State<AppState>,
    Query(q): Query<FileQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let Some(if_match) = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
    else {
        return Ok((
            StatusCode::PRECONDITION_REQUIRED,
            "If-Match header required",
        )
            .into_response());
    };
    let store = s.screen_store.clone();
    let etag = blocking(move || {
        if if_match == "*" {
            let exists = match store.read_file(&q.r#ref, &q.name) {
                Ok(_) => true,
                Err(StoreError::NotFound) => false,
                Err(e) => return Err(e),
            };
            if exists {
                // Not a valid etag (`etag()` always emits a 64-char blake3
                // hex digest), so this can never accidentally match — its
                // only purpose is to force `Conflict` for an existing file,
                // after `write_file` has already run its own read-only
                // check ahead of the etag comparison.
                store.write_file(&q.r#ref, &q.name, &body, Some(""))
            } else {
                store.write_file(&q.r#ref, &q.name, &body, None)
            }
        } else {
            store.write_file(&q.r#ref, &q.name, &body, Some(&if_match))
        }
    })
    .await?
    .map_err(map_store)?;
    Ok((StatusCode::NO_CONTENT, [(header::ETAG, etag)]).into_response())
}

const CORE_FILES: [&str; 3] = ["meta.yaml", "script.lua", "screen.svg"];

async fn delete_file(
    State(s): State<AppState>,
    Query(q): Query<FileQuery>,
) -> Result<StatusCode, ApiError> {
    if CORE_FILES.contains(&q.name.as_str()) {
        return Err(ApiError::Unprocessable(format!(
            "{} cannot be deleted",
            q.name
        )));
    }
    let store = s.screen_store.clone();
    blocking(move || store.delete_file(&q.r#ref, &q.name))
        .await?
        .map_err(map_store)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create(
    State(s): State<AppState>,
    Json(b): Json<CreateIn>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if b.starter != "minimal" {
        return Err(ApiError::BadRequest(format!(
            "unknown starter '{}'",
            b.starter
        )));
    }
    let store = s.screen_store.clone();
    let r = blocking(move || store.create_screen(&b.handle, &b.name, StarterKind::Minimal))
        .await?
        .map_err(map_store)?;
    Ok(Json(serde_json::json!({"ref": r})))
}

async fn copy(
    State(s): State<AppState>,
    Json(b): Json<CopyIn>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = s.screen_store.clone();
    let r = blocking(move || store.copy_screen(&b.r#ref, &b.to_handle, &b.to_name))
        .await?
        .map_err(map_store)?;
    Ok(Json(serde_json::json!({"ref": r})))
}

async fn rename(
    State(s): State<AppState>,
    Json(b): Json<RenameIn>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = s.screen_store.clone();
    let r = blocking(move || store.rename_screen(&b.r#ref, &b.name))
        .await?
        .map_err(map_store)?;
    Ok(Json(serde_json::json!({"ref": r})))
}

async fn delete_screen(
    State(s): State<AppState>,
    Query(q): Query<RefQuery>,
) -> Result<StatusCode, ApiError> {
    let store = s.screen_store.clone();
    blocking(move || store.delete_screen(&q.r#ref))
        .await?
        .map_err(map_store)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn validate(
    State(s): State<AppState>,
    Json(b): Json<RefQuery>,
) -> Result<Json<ReportOut>, ApiError> {
    let store = s.screen_store.clone();
    Ok(Json(
        blocking(move || store.validate(&b.r#ref)).await?.into(),
    ))
}

async fn check(
    State(s): State<AppState>,
    Json(b): Json<CheckIn>,
) -> Result<Json<ReportOut>, ApiError> {
    let store = s.screen_store.clone();
    Ok(Json(
        blocking(move || store.check_text(&b.r#ref, &b.file, &b.text))
            .await?
            .into(),
    ))
}

/// Screen-file routes, mounted under `/dev/api` by `dev::dev_router`
/// (already carrying `require_dev` protection).
pub fn files_router() -> Router<AppState> {
    Router::new()
        .route("/repos", get(repos))
        .route("/files", get(files))
        .route("/file", get(read_file).put(write_file).delete(delete_file))
        .route("/screen", post(create).delete(delete_screen))
        .route("/screen/copy", post(copy))
        .route("/screen/rename", post(rename))
        .route("/screen/validate", post(validate))
        .route("/screen/check", post(check))
}
