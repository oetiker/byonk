# Dev Mode Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `/dev` a permanent, login-protected part of `byonk serve`, with a three-column screen editor (screens, CodeMirror editor with live syntax checks, preview with pop-out) and Home Assistant Ingress support.

**Architecture:** The auth layer is one axum middleware (`require_dev`) with three admission paths (Ingress peer, admin bearer token, HMAC session cookie). All screen edits go through the existing `ScreenStore`; new `/dev/api/*` handlers are thin JSON wrappers over it. The page stays plain HTML/CSS/JS; only the editor is a library (CodeMirror 6) built once into a committed bundle and embedded with `include_str!`.

**Tech Stack:** Rust (axum 0.8, hmac, sha2, rand, mlua, tera), CodeMirror 6 via pnpm + Rollup, vanilla JS, HA add-on Ingress.

**Spec:** `docs/superpowers/specs/2026-09-07-dev-mode-editor-design.md`

## Global Constraints

- All work happens on branch `feat/dev-editor` (direct pushes to `main` are blocked by a ruleset). Create it from `main` before Task 1; Task 13 opens the PR.
- Never `git add -A` or `git add .`. Four files are permanently modified in the owner's tree and must never be staged: `config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`. Add by explicit path; run `git diff --cached --name-only` before every commit.
- Sync `main` with `git merge --ff-only origin/main`, never `git pull`.
- Verification is four direct commands, not `make check` alone: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cd docs && mdbook build`.
- Every new test is broken once on purpose (revert a line of the implementation, watch it fail, restore) before it is committed.
- Node package manager is `pnpm`, never npm or yarn.
- The dev page uses **relative URLs only** (`./dev.js`, `./api/render`). No handler reads `X-Ingress-Path`.
- Supervisor peer address for Ingress trust: `172.30.32.2`. Cookie name: `byonk_dev`. Session lifetime: 30 days. Failed-login delay: 1 second.
- Env vars: `BYONK_DEV_USERNAME`, `BYONK_DEV_PASSWORD`, `BYONK_INGRESS_ADDR`. Add-on options: `dev_username`, `dev_password`.
- CHANGES.md entries are user-facing only.
- Comments, identifiers and technical docs in English.

---

## File map

| File | Responsibility |
|---|---|
| `src/api/dev_auth.rs` (new) | `DevAuth`, credential resolution, cookie sign/verify, `require_dev` middleware, `DevPrincipal`, `ViaIngress`, login/logout handlers |
| `src/api/dev.rs` (modify) | `dev_router()`, page/asset handlers, existing simulator handlers moved to `AppState` and `/dev/api` |
| `src/api/dev_files.rs` (new) | `/dev/api/repos`, `/files`, `/file`, `/screen*` handlers and JSON DTOs |
| `src/error.rs` (modify) | four new `ApiError` variants |
| `src/server.rs` (modify) | `AppState` gains `dev_auth`, `file_watcher`; `build_router` nests `/dev` |
| `src/addon_options.rs` (modify) | `dev_username`, `dev_password` options |
| `src/main.rs` (modify) | remove `Dev` command, start watcher and ingress listener in `run_server` |
| `src/services/screen_store.rs` (modify) | `list_files`, `check_text`, `Issue.line`, shared validator dispatch |
| `static/dev/index.html`, `dev.css`, `dev.js` (rewrite) | Layout A |
| `static/dev/login.html`, `preview.html` (new) | login form, pop-out |
| `static/dev/editor/` (new) | pnpm project producing `static/dev/editor.bundle.js` |
| `homeassistant/byonk/config.yaml`, `translations/en.yaml`, `DOCS.md` | Ingress + options |
| `docs/src/guide/dev-mode.md`, `docs/src/guide/configuration.md`, `docs/src/api/lua-api.md`, `README.md`, `CHANGES.md`, `.gitignore` | docs |
| `tests/dev_auth_test.rs`, `tests/dev_files_test.rs` (new) | route tests |

---

### Task 1: Credentials and `DevAuth`

**Files:**
- Create: `src/api/dev_auth.rs`
- Modify: `src/api/mod.rs:2` (add `pub mod dev_auth;`)
- Modify: `src/addon_options.rs:34-45` (struct), `:121-130` (apply)
- Modify: `src/server.rs:70-95` (`AppState`), `:98-160` (constructors)
- Test: `src/api/dev_auth.rs` unit tests, `tests/addon_options_test.rs`

**Interfaces:**
- Produces: `DevAuth { credentials: Option<DevCredentials>, session_key: [u8; 32] }`, `DevCredentials { username: String, password: String }`, `DevAuth::resolve(config_user: Option<&str>, config_pass: Option<&str>) -> DevAuth`, `DevAuth::with_credentials(username, password) -> DevAuth`, `DevAuth::disabled() -> DevAuth`, `AppState.dev_auth: Arc<DevAuth>`, `AppConfig.admin.dev_username / dev_password: Option<String>`.

- [ ] **Step 1: Write failing unit tests in `src/api/dev_auth.rs`**

```rust
//! Dev UI authentication: credentials, session cookies, and the
//! `require_dev` middleware with its three admission paths.

use std::sync::Arc;

/// One username + password pair for the form login.
#[derive(Debug, Clone)]
pub struct DevCredentials {
    pub username: String,
    pub password: String,
}

/// Everything `require_dev` needs. Lives on `AppState` as `Arc<DevAuth>`.
#[derive(Debug)]
pub struct DevAuth {
    pub credentials: Option<DevCredentials>,
    /// Random per process: a restart logs everyone out.
    pub session_key: [u8; 32],
}

fn fresh_key() -> [u8; 32] {
    use rand::RngCore;
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn non_blank(v: Option<&str>) -> Option<String> {
    v.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

impl DevAuth {
    /// No form login. Ingress and bearer paths still work.
    pub fn disabled() -> Self {
        Self { credentials: None, session_key: fresh_key() }
    }

    pub fn with_credentials(username: &str, password: &str) -> Self {
        Self {
            credentials: Some(DevCredentials {
                username: username.to_string(),
                password: password.to_string(),
            }),
            session_key: fresh_key(),
        }
    }

    /// Env wins over config (which the add-on options already wrote into);
    /// blank counts as unset; both halves are required.
    pub fn resolve(config_user: Option<&str>, config_pass: Option<&str>) -> Self {
        let user = non_blank(std::env::var("BYONK_DEV_USERNAME").ok().as_deref())
            .or_else(|| non_blank(config_user));
        let pass = non_blank(std::env::var("BYONK_DEV_PASSWORD").ok().as_deref())
            .or_else(|| non_blank(config_pass));
        match (user, pass) {
            (Some(u), Some(p)) => Self::with_credentials(&u, &p),
            _ => Self::disabled(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_requires_both_halves() {
        assert!(DevAuth::resolve(Some("u"), None).credentials.is_none());
        assert!(DevAuth::resolve(None, Some("p")).credentials.is_none());
        assert!(DevAuth::resolve(Some("u"), Some("p")).credentials.is_some());
    }

    #[test]
    fn resolve_treats_blank_as_unset() {
        assert!(DevAuth::resolve(Some("  "), Some("p")).credentials.is_none());
    }

    #[test]
    fn two_instances_have_different_session_keys() {
        assert_ne!(DevAuth::disabled().session_key, DevAuth::disabled().session_key);
    }
}
```

Env-var precedence is tested in `tests/addon_options_test.rs` style (process-wide env, so keep it to one test):

```rust
#[test]
fn dev_env_wins_over_config() {
    std::env::set_var("BYONK_DEV_USERNAME", "envu");
    std::env::set_var("BYONK_DEV_PASSWORD", "envp");
    let auth = byonk::api::dev_auth::DevAuth::resolve(Some("cfgu"), Some("cfgp"));
    std::env::remove_var("BYONK_DEV_USERNAME");
    std::env::remove_var("BYONK_DEV_PASSWORD");
    let c = auth.credentials.unwrap();
    assert_eq!((c.username.as_str(), c.password.as_str()), ("envu", "envp"));
}
```

- [ ] **Step 2: Run, expect compile failure (`dev_auth` module missing)**

Run: `cargo test --lib dev_auth`
Expected: FAIL, unresolved module.

- [ ] **Step 3: Wire the module, config fields, add-on options and `AppState`**

`src/api/mod.rs`: add `pub mod dev_auth;` after `pub mod dev;`.

`src/models/config.rs:441` `AdminConfig` gains:

```rust
    /// Dev UI form login. Both required; set by env or add-on options.
    #[serde(default)]
    pub dev_username: Option<String>,
    #[serde(default)]
    pub dev_password: Option<String>,
```

`src/addon_options.rs` `AddonOptions` gains:

```rust
    #[serde(default)]
    pub dev_username: Option<String>,
    #[serde(default)]
    pub dev_password: Option<String>,
```

and `apply_to_config` sets them the same way as `admin_token` (authoritative, blank clears):

```rust
        config.admin.dev_username = opts.dev_username.as_deref().map(str::trim)
            .filter(|t| !t.is_empty()).map(str::to_string);
        config.admin.dev_password = opts.dev_password.as_deref().map(str::trim)
            .filter(|t| !t.is_empty()).map(str::to_string);
```

Every literal `AddonOptions { ... }` in that file's tests gains `dev_username: None, dev_password: None,`.

`src/server.rs` `AppState` gains:

```rust
    /// Dev UI auth (credentials + session key). See `api::dev_auth`.
    pub dev_auth: Arc<crate::api::dev_auth::DevAuth>,
```

`create_app_state_with_overrides` builds it right after `admin_token`:

```rust
    let dev_auth = Arc::new(crate::api::dev_auth::DevAuth::resolve(
        config.admin.dev_username.as_deref(),
        config.admin.dev_password.as_deref(),
    ));
```

and adds `dev_auth,` to the struct literal.

- [ ] **Step 4: Run tests, expect pass**

Run: `cargo test --lib dev_auth && cargo test --test addon_options_test`
Expected: PASS.

- [ ] **Step 5: Break, confirm, restore**

Change `non_blank` to return `v.map(str::to_string)` (no trim/filter). `resolve_treats_blank_as_unset` must fail. Restore.

- [ ] **Step 6: Commit**

```bash
git add src/api/dev_auth.rs src/api/mod.rs src/addon_options.rs src/models/config.rs src/server.rs tests/addon_options_test.rs
git diff --cached --name-only
git commit -m "feat(dev): resolve dev UI credentials from env and add-on options"
```

---

### Task 2: Session cookie sign and verify

**Files:**
- Modify: `src/api/dev_auth.rs`
- Test: unit tests in the same file

**Interfaces:**
- Produces: `pub const COOKIE_NAME: &str = "byonk_dev"`, `pub const SESSION_MAX_AGE_SECS: i64 = 30 * 24 * 3600`, `DevAuth::issue_cookie_value(&self, now: i64) -> String`, `DevAuth::verify_cookie_value(&self, value: &str, now: i64) -> bool`.

- [ ] **Step 1: Write failing tests**

```rust
    #[test]
    fn cookie_round_trips() {
        let a = DevAuth::with_credentials("u", "p");
        let v = a.issue_cookie_value(1_000);
        assert!(a.verify_cookie_value(&v, 1_000 + 60));
    }

    #[test]
    fn cookie_from_another_key_is_rejected() {
        let a = DevAuth::with_credentials("u", "p");
        let b = DevAuth::with_credentials("u", "p");
        assert!(!b.verify_cookie_value(&a.issue_cookie_value(1_000), 1_060));
    }

    #[test]
    fn tampered_timestamp_is_rejected() {
        let a = DevAuth::with_credentials("u", "p");
        let v = a.issue_cookie_value(1_000);
        let (_, mac) = v.split_once('.').unwrap();
        assert!(!a.verify_cookie_value(&format!("2000.{mac}"), 2_060));
    }

    #[test]
    fn expired_cookie_is_rejected() {
        let a = DevAuth::with_credentials("u", "p");
        let v = a.issue_cookie_value(1_000);
        assert!(!a.verify_cookie_value(&v, 1_000 + SESSION_MAX_AGE_SECS + 1));
        assert!(a.verify_cookie_value(&v, 1_000 + SESSION_MAX_AGE_SECS - 1));
    }

    #[test]
    fn garbage_is_rejected() {
        let a = DevAuth::with_credentials("u", "p");
        assert!(!a.verify_cookie_value("", 0));
        assert!(!a.verify_cookie_value("abc", 0));
        assert!(!a.verify_cookie_value("12.zz", 0));
    }
```

- [ ] **Step 2: Run, expect failure (methods missing)**

Run: `cargo test --lib dev_auth`

- [ ] **Step 3: Implement**

```rust
pub const COOKIE_NAME: &str = "byonk_dev";
pub const SESSION_MAX_AGE_SECS: i64 = 30 * 24 * 3600;

fn mac_hex(key: &[u8; 32], issued: i64) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(issued.to_string().as_bytes());
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time byte compare, same shape as `api::admin::constant_time_eq`.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

impl DevAuth {
    /// `<issued_unix>.<hex hmac>`; `now` is injected for tests.
    pub fn issue_cookie_value(&self, now: i64) -> String {
        format!("{now}.{}", mac_hex(&self.session_key, now))
    }

    pub fn verify_cookie_value(&self, value: &str, now: i64) -> bool {
        let Some((issued, mac)) = value.split_once('.') else { return false };
        let Ok(issued) = issued.parse::<i64>() else { return false };
        if now - issued > SESSION_MAX_AGE_SECS || issued > now + 60 {
            return false;
        }
        ct_eq(mac.as_bytes(), mac_hex(&self.session_key, issued).as_bytes())
    }
}
```

Make `constant_time_eq` in `src/api/admin/mod.rs:34` `pub(crate)` and use it instead of a second copy if clippy is happy with the visibility; otherwise keep `ct_eq` and note why.

- [ ] **Step 4: Run tests, expect pass**

- [ ] **Step 5: Break, confirm, restore**

Replace `ct_eq(...)` with `true` in `verify_cookie_value`. `cookie_from_another_key_is_rejected` and `tampered_timestamp_is_rejected` must fail. Restore.

- [ ] **Step 6: Commit**

```bash
git add src/api/dev_auth.rs src/api/admin/mod.rs
git commit -m "feat(dev): HMAC session cookie for the dev UI"
```

---

### Task 3: `require_dev` middleware, login and logout

**Files:**
- Modify: `src/api/dev_auth.rs`
- Create: `static/dev/login.html`
- Modify: `src/error.rs:10-37`, `:72-80` (variants used later too)
- Test: `tests/dev_auth_test.rs` (new), helper additions in `tests/common/app.rs`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy)] pub struct ViaIngress;` request extension marking the ingress listener
  - `pub const SUPERVISOR_ADDR: std::net::IpAddr` = `172.30.32.2`
  - `#[derive(Clone)] pub enum DevPrincipal { Ingress(String), Token, Session(String) }`, inserted into request extensions on success
  - `pub async fn require_dev(State(AppState), Request, Next) -> Response`
  - `pub async fn login_form(State) -> Response`, `pub async fn login_submit(State, Form<LoginForm>) -> Response`, `pub async fn logout() -> Response`
  - `pub fn dev_auth_router(state: &AppState) -> Router<AppState>` with `/login` and `/logout`
  - `ApiError::PreconditionFailed(String)` 412, `ApiError::Unprocessable(String)` 422, `ApiError::PayloadTooLarge` 413, `ApiError::UnsupportedMedia(String)` 415
- Consumes: Task 2 cookie functions.

- [ ] **Step 1: Add the `ApiError` variants**

In `src/error.rs` enum:

```rust
    #[error("Precondition failed: {0}")]
    PreconditionFailed(String),
    #[error("Unprocessable: {0}")]
    Unprocessable(String),
    #[error("Payload too large")]
    PayloadTooLarge,
    #[error("Unsupported media type: {0}")]
    UnsupportedMedia(String),
```

and in the status match:

```rust
            ApiError::PreconditionFailed(m) => (StatusCode::PRECONDITION_FAILED, m.clone()),
            ApiError::Unprocessable(m) => (StatusCode::UNPROCESSABLE_ENTITY, m.clone()),
            ApiError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, self.to_string()),
            ApiError::UnsupportedMedia(m) => (StatusCode::UNSUPPORTED_MEDIA_TYPE, m.clone()),
```

Add one assertion per variant to the existing status test in `src/error.rs` tests (pattern at lines 99-111).

- [ ] **Step 2: Write failing route tests in `tests/dev_auth_test.rs`**

The middleware does not exist yet; these tests need the `/dev` nest from Task 4 to route. Write them now, run them after Task 4 Step 4. Add to `tests/common/app.rs`:

```rust
    /// Like `new_admin` but with dev credentials set in config.yaml.
    pub fn new_with_dev_login(token: Option<&str>, user: &str, pass: &str) -> Self {
        let mut yaml = String::from("admin:\n");
        if let Some(t) = token { yaml.push_str(&format!("  token: {t}\n")); }
        yaml.push_str(&format!("  dev_username: {user}\n  dev_password: {pass}\n"));
        yaml.push_str("devices:\n  DEFAULT:\n    screen: byonk-builtin/default\n");
        let dir = tempfile::tempdir().unwrap();
        let (app, _) = Self::new_with_config_yaml(&yaml, dir.path());
        std::mem::forget(dir); // keep the temp dir alive for the test's duration
        app
    }

    /// Send a request that pretends to come from `peer` on the ingress listener.
    pub async fn get_via_ingress(&self, path: &str, peer: &str, headers: &[(&str, &str)]) -> TestResponse {
        use axum::extract::ConnectInfo;
        let mut builder = Request::get(path);
        for (n, v) in headers { builder = builder.header(*n, *v); }
        let mut req = builder.body(Body::empty()).unwrap();
        req.extensions_mut().insert(byonk::api::dev_auth::ViaIngress);
        req.extensions_mut().insert(ConnectInfo(peer.parse::<std::net::SocketAddr>().unwrap()));
        self.request(req).await
    }

    pub async fn post_form(&self, path: &str, body: &str) -> TestResponse {
        let req = Request::post(path)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(Body::from(body.to_string())).unwrap();
        self.request(req).await
    }
```

Tests:

```rust
mod common;
use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn no_credentials_page_is_404_and_api_is_401() {
    let app = TestApp::new();
    assert_eq!(app.get("/dev/").await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/dev/login").await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/dev/api/whoami").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn with_credentials_page_redirects_to_login() {
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let r = app.get("/dev/").await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.headers["location"], "login");
    assert_eq!(app.get("/dev/login").await.status, StatusCode::OK);
}

#[tokio::test]
async fn wrong_password_is_delayed_and_rejected() {
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let t = std::time::Instant::now();
    let r = app.post_form("/dev/login", "username=u&password=x").await;
    assert!(t.elapsed() >= std::time::Duration::from_millis(900));
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert!(r.headers.get("set-cookie").is_none());
}

#[tokio::test]
async fn good_login_sets_cookie_that_passes() {
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let r = app.post_form("/dev/login", "username=u&password=p").await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let cookie = r.headers["set-cookie"].to_str().unwrap().to_string();
    assert!(cookie.starts_with("byonk_dev="));
    assert!(cookie.contains("HttpOnly") && cookie.contains("Path=/dev"));
    let pair = cookie.split(';').next().unwrap();
    let who = app.get_with_headers("/dev/api/whoami", &[("Cookie", pair)]).await;
    assert_eq!(who.status, StatusCode::OK);
    let j: serde_json::Value = who.json();
    assert_eq!(j["user"], "u");
    assert_eq!(j["via"], "cookie");
    assert_eq!(j["can_logout"], true);
}

#[tokio::test]
async fn tampered_cookie_fails() {
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let r = app.get_with_headers("/dev/api/whoami", &[("Cookie", "byonk_dev=1.deadbeef")]).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_admin_token_passes_without_cookie() {
    let app = TestApp::new_admin("secret");
    let r = app.get_with_headers("/dev/api/whoami", &[("Authorization", "Bearer secret")]).await;
    assert_eq!(r.status, StatusCode::OK);
    let j: serde_json::Value = r.json();
    assert_eq!(j["via"], "token");
    assert_eq!(j["can_logout"], false);
    let bad = app.get_with_headers("/dev/api/whoami", &[("Authorization", "Bearer nope")]).await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ingress_trusts_supervisor_peer_only() {
    let app = TestApp::new();
    let ok = app.get_via_ingress("/dev/api/whoami", "172.30.32.2:1234", &[("X-Remote-User-Name", "tobi")]).await;
    assert_eq!(ok.status, StatusCode::OK);
    let j: serde_json::Value = ok.json();
    assert_eq!(j["user"], "tobi");
    assert_eq!(j["via"], "ingress");
    let bad = app.get_via_ingress("/dev/api/whoami", "10.0.0.5:1234", &[("X-Remote-User-Name", "tobi")]).await;
    assert_eq!(bad.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn remote_user_header_on_public_listener_is_ignored() {
    let app = TestApp::new();
    let r = app.get_with_headers("/dev/api/whoami", &[("X-Remote-User-Name", "tobi")]).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_clears_cookie() {
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let r = app.post_form("/dev/logout", "").await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert!(r.headers["set-cookie"].to_str().unwrap().contains("Max-Age=0"));
}
```

- [ ] **Step 3: Implement middleware and handlers in `src/api/dev_auth.rs`**

```rust
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use crate::server::AppState;

#[derive(Debug, Clone, Copy)]
pub struct ViaIngress;

pub const SUPERVISOR_ADDR: std::net::IpAddr =
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(172, 30, 32, 2));

#[derive(Debug, Clone)]
pub enum DevPrincipal {
    Ingress(String),
    Token,
    Session(String),
}

impl DevPrincipal {
    pub fn user(&self) -> &str {
        match self {
            DevPrincipal::Ingress(u) | DevPrincipal::Session(u) => u,
            DevPrincipal::Token => "token",
        }
    }
    pub fn via(&self) -> &'static str {
        match self {
            DevPrincipal::Ingress(_) => "ingress",
            DevPrincipal::Token => "token",
            DevPrincipal::Session(_) => "cookie",
        }
    }
    pub fn can_logout(&self) -> bool {
        matches!(self, DevPrincipal::Session(_))
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|line| line.split(';'))
        .map(str::trim)
        .find_map(|kv| kv.strip_prefix(&format!("{COOKIE_NAME}=")).map(str::to_string))
}

/// Decide who is asking. `Err(status)` is the refusal to send when nobody
/// matched: 403 only on the ingress listener from a foreign peer.
fn admit(state: &AppState, request: &Request) -> Result<DevPrincipal, StatusCode> {
    let headers = request.headers();
    if request.extensions().get::<ViaIngress>().is_some() {
        let peer = request.extensions().get::<ConnectInfo<std::net::SocketAddr>>().map(|c| c.0.ip());
        if peer != Some(SUPERVISOR_ADDR) {
            return Err(StatusCode::FORBIDDEN);
        }
        let user = headers
            .get("x-remote-user-name")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
            .unwrap_or("ha-user")
            .to_string();
        return Ok(DevPrincipal::Ingress(user));
    }
    if crate::api::admin::require_admin(state, headers).is_ok() {
        return Ok(DevPrincipal::Token);
    }
    if let (Some(creds), Some(v)) = (&state.dev_auth.credentials, cookie_value(headers)) {
        if state.dev_auth.verify_cookie_value(&v, now_unix()) {
            return Ok(DevPrincipal::Session(creds.username.clone()));
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

/// Gate for everything under `/dev` except `/login` and `/logout`.
pub async fn require_dev(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    match admit(&state, &request) {
        Ok(p) => {
            request.extensions_mut().insert(p);
            next.run(request).await
        }
        Err(StatusCode::FORBIDDEN) => StatusCode::FORBIDDEN.into_response(),
        Err(_) => {
            let is_api = request.uri().path().contains("/api/");
            if is_api {
                (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "login required"}))).into_response()
            } else if state.dev_auth.credentials.is_some() {
                Redirect::to("login").into_response()
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

const LOGIN_HTML: &str = include_str!("../../static/dev/login.html");

fn login_page(message: &str) -> Html<String> {
    Html(LOGIN_HTML.replace("{{message}}", message))
}

pub async fn login_form(State(state): State<AppState>) -> Response {
    if state.dev_auth.credentials.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    login_page("").into_response()
}

pub async fn login_submit(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let Some(creds) = &state.dev_auth.credentials else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ok = ct_eq(form.username.as_bytes(), creds.username.as_bytes())
        & ct_eq(form.password.as_bytes(), creds.password.as_bytes());
    if !ok {
        tracing::warn!(username = %form.username, "dev UI login failed");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        return (StatusCode::UNAUTHORIZED, login_page("Wrong username or password.")).into_response();
    }
    let value = state.dev_auth.issue_cookie_value(now_unix());
    let cookie = format!(
        "{COOKIE_NAME}={value}; HttpOnly; SameSite=Lax; Path=/dev; Max-Age={SESSION_MAX_AGE_SECS}"
    );
    ([(header::SET_COOKIE, cookie)], Redirect::to("./")).into_response()
}

pub async fn logout() -> Response {
    let cookie = format!("{COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/dev; Max-Age=0");
    ([(header::SET_COOKIE, cookie)], Redirect::to("login")).into_response()
}

pub async fn whoami(axum::Extension(p): axum::Extension<DevPrincipal>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"user": p.user(), "via": p.via(), "can_logout": p.can_logout()}))
}

/// The two unprotected routes.
pub fn dev_auth_router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_form).post(login_submit))
        .route("/logout", post(logout))
}
```

`Redirect::to` produces 303 See Other; the tests assert `SEE_OTHER`. `ct_eq` is `&`-combined, not `&&`, so both compares always run.

`static/dev/login.html`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Byonk Login</title>
<link rel="stylesheet" href="dev.css">
</head>
<body class="login-body">
<form class="login-card" method="post" action="login">
  <h1>Byonk</h1>
  <p class="login-message">{{message}}</p>
  <label>Username <input name="username" autocomplete="username" autofocus required></label>
  <label>Password <input name="password" type="password" autocomplete="current-password" required></label>
  <button type="submit">Log in</button>
</form>
</body>
</html>
```

`dev.css` must be reachable without login: Task 4 places `/dev.css` outside the protected group.

- [ ] **Step 4: Compile**

Run: `cargo build`
Expected: builds. The route tests run after Task 4.

- [ ] **Step 5: Commit**

```bash
git add src/api/dev_auth.rs src/error.rs static/dev/login.html tests/dev_auth_test.rs tests/common/app.rs
git commit -m "feat(dev): require_dev middleware with ingress, token and cookie admission"
```

---

### Task 4: One server mode: `/dev` in `build_router`, `byonk dev` removed, ingress listener

**Files:**
- Modify: `src/api/dev.rs` (all handlers take `State<AppState>`; add `dev_router`)
- Modify: `src/server.rs:70-95`, `:98-160`, `:250-282`
- Modify: `src/main.rs:22-30` (enum), `:168`, `:794`, `:843-966` (`run_server`), delete `run_dev_server` (`:967-1075`)
- Modify: `src/services/file_watcher.rs` (no API change; `FileWatcher::new(None)` already yields inactive)
- Test: `tests/dev_auth_test.rs` (from Task 3), `src/server.rs` brace-syntax test

**Interfaces:**
- Produces: `AppState.file_watcher: Arc<FileWatcher>` (inactive when no `SCREENS_DIR`), `pub fn dev_router(state: &AppState) -> Router<AppState>` in `src/api/dev.rs`, routes `/dev/`, `/dev/dev.css`, `/dev/dev.js`, `/dev/editor.bundle.js`, `/dev/preview.html`, `/dev/api/{screens,events,render,resolve-mac,panel-colors,panel-colors/{panel},whoami}`, `/dev/login`, `/dev/logout`.
- Consumes: `require_dev`, `dev_auth_router`, `whoami` from Task 3.

- [ ] **Step 1: Add the watcher to `AppState`**

`src/server.rs`: field

```rust
    /// Live-reload source for `/dev/api/events`. Inactive (never fires) when
    /// there is no external `SCREENS_DIR`.
    pub file_watcher: Arc<crate::services::FileWatcher>,
```

`create_app_state_with_overrides` sets `file_watcher: Arc::new(crate::services::FileWatcher::new(asset_loader.screens_dir().map(|p| p.to_path_buf())))`. If `AssetLoader` has no `screens_dir()` accessor, add `pub fn screens_dir(&self) -> Option<&Path>` returning the path it was constructed with.

- [ ] **Step 2: Rewrite `src/api/dev.rs` state and router**

Delete `DevState`. Every handler's `State(state): State<DevState>` becomes `State(state): State<AppState>`. Field access changes: `state.config` was `Arc<AppConfig>`, now `SharedConfig`, so use `let config = state.config.load();` at the top of `handle_screens`, `handle_resolve_mac` and `handle_render` and read `config.devices`, `config.panels`, `config.get_device_config(..)` from it. `state.file_watcher.subscribe()` unchanged.

Add at the bottom:

```rust
/// Page and assets; the `.css` is public so the login page can style itself.
pub async fn handle_dev_page(uri: axum::http::Uri) -> Response {
    if !uri.path().ends_with('/') {
        return Redirect::permanent(&format!("{}/", uri.path())).into_response();
    }
    Html(include_str!("../../static/dev/index.html")).into_response()
}
pub async fn handle_editor_bundle() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], include_str!("../../static/dev/editor.bundle.js"))
}
pub async fn handle_preview_page() -> Html<&'static str> {
    Html(include_str!("../../static/dev/preview.html"))
}

/// Everything under `/dev`. Protected routes get `require_dev` as a
/// route layer; `/login`, `/logout` and the stylesheet stay open.
pub fn dev_router(state: &AppState) -> Router<AppState> {
    use axum::routing::{delete, get, post};
    let api = Router::new()
        .route("/screens", get(handle_screens))
        .route("/events", get(handle_events))
        .route("/render", get(handle_render))
        .route("/resolve-mac", get(handle_resolve_mac))
        .route("/panel-colors", post(handle_set_panel_colors))
        .route("/panel-colors/{panel}", delete(handle_delete_panel_colors))
        .route("/whoami", get(crate::api::dev_auth::whoami))
        .merge(crate::api::dev_files::files_router());
    let protected = Router::new()
        .route("/", get(handle_dev_page))
        .route("/dev.js", get(handle_dev_js))
        .route("/editor.bundle.js", get(handle_editor_bundle))
        .route("/preview.html", get(handle_preview_page))
        .nest("/api", api)
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::api::dev_auth::require_dev,
        ));
    Router::new()
        .merge(protected)
        .route("/dev.css", get(handle_dev_css))
        .merge(crate::api::dev_auth::dev_auth_router())
}
```

Until Task 6 exists, `dev_files::files_router()` is a stub returning `Router::new()`; create `src/api/dev_files.rs` with that stub and `pub mod dev_files;` in `src/api/mod.rs`. Until Task 7 exists, `static/dev/editor.bundle.js` is a one-line placeholder file `// built by static/dev/editor (pnpm run build)`, and `preview.html` an empty `<!DOCTYPE html><title>Byonk Preview</title>` so `include_str!` resolves.

In `src/server.rs::build_router`, after the admin nest:

```rust
        .nest("/dev", crate::api::dev::dev_router(&state));
```

`build_router` receives `state` by value already; borrow before the `.with_state(state)` call.

- [ ] **Step 3: Collapse `main.rs`**

- Remove `Dev` from `Commands` (lines 26-27), the `Some(Commands::Dev) => run_dev_server().await,` arm (168), the help line (794) and the whole `run_dev_server` function.
- In `run_server`, after `seed_and_migrate`, keep `create_app_state_with_config` and add a log line: `if state.file_watcher.is_active() { tracing::info!("File watcher active for live reload") }`.
- Replace the tail:

```rust
    let app = server::build_router(state)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .nest_service("/static", ServeDir::new("./static"));

    if let Ok(ingress_addr) = std::env::var("BYONK_INGRESS_ADDR") {
        let ingress_app = app
            .clone()
            .layer(axum::Extension(byonk::api::dev_auth::ViaIngress));
        let l = tokio::net::TcpListener::bind(&ingress_addr).await?;
        tracing::info!(addr = %ingress_addr, "Byonk ingress listener");
        tokio::spawn(async move {
            if let Err(e) = axum::serve(
                l,
                ingress_app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            {
                tracing::error!(error = %e, "ingress listener stopped");
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "Byonk server listening");
    tracing::info!("Dev UI at http://{}/dev/", bind_addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
```

Remove the now-unused `FileWatcher`, `DevState` and dev handler imports.

- [ ] **Step 4: Run the Task 3 tests and the full suite**

Run: `cargo test --test dev_auth_test && cargo test --workspace`
Expected: all PASS, including `build_router_accepts_brace_path_syntax`.

- [ ] **Step 5: Break, confirm, restore**

In `admit`, replace `peer != Some(SUPERVISOR_ADDR)` with `false`. `ingress_trusts_supervisor_peer_only` must fail. Restore. Then swap the `require_admin(...).is_ok()` check to `true`; `remote_user_header_on_public_listener_is_ignored` and `bearer_admin_token_passes_without_cookie` (bad token half) must fail. Restore.

- [ ] **Step 6: Commit**

```bash
git add src/api/dev.rs src/api/dev_files.rs src/api/mod.rs src/server.rs src/main.rs src/assets static/dev/editor.bundle.js static/dev/preview.html
git diff --cached --name-only
git commit -m "feat!: mount /dev in byonk serve behind login; remove byonk dev"
```

---

### Task 5: Store additions: `list_files`, `check_text`, `Issue.line`

**Files:**
- Modify: `src/services/screen_store.rs:85-103` (Issue), `:932-1030` (validate)
- Modify: `src/mcp/tools_read.rs` (wherever `Issue` is mapped to JSON: add `line`)
- Test: `tests/screen_store_check_test.rs` (new), using `tests/common/store.rs::build_store`

**Interfaces:**
- Produces:
  - `Issue.line: Option<u32>`
  - `pub struct FileEntry { pub name: String, pub size: Option<u64>, pub text: bool }`
  - `ScreenStore::list_files(&self, screen_ref: &str) -> Result<Vec<FileEntry>, StoreError>`
  - `ScreenStore::check_text(&self, screen_ref: &str, file: &str, text: &str) -> ValidationReport`
  - `pub fn error_line(message: &str) -> Option<u32>`

- [ ] **Step 1: Write failing tests**

```rust
mod common;
use common::store::build_store;
use byonk::services::screen_store::{error_line, Severity};

#[test]
fn error_line_parses_lua_and_tera_forms() {
    assert_eq!(error_line(r#"syntax error: [string "script.lua"]:6: '}' expected"#), Some(6));
    assert_eq!(error_line("Failed to parse 'screen.svg'\n --> 3:5\n  |"), Some(3));
    assert_eq!(error_line("no numbers here"), None);
}

#[test]
fn check_text_reports_lua_syntax_error_with_line() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    let r = store.check_text("local/clock", "script.lua", "return {\n  a = 1\nend\n");
    assert!(!r.ok);
    assert_eq!(r.issues[0].location, "script.lua");
    assert_eq!(r.issues[0].severity, Severity::Error);
    assert_eq!(r.issues[0].line, Some(3));
}

#[test]
fn check_text_ok_for_valid_lua_and_unknown_kind() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    assert!(store.check_text("local/clock", "script.lua", "return {}").ok);
    assert!(store.check_text("local/clock", "notes.txt", "anything").ok);
}

#[test]
fn check_text_reports_tera_error_with_line() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    let r = store.check_text("local/clock", "screen.svg", "<svg>\n{% if %}\n</svg>");
    assert!(!r.ok);
    assert_eq!(r.issues[0].location, "screen.svg");
    assert_eq!(r.issues[0].line, Some(2));
}

#[test]
fn check_text_and_validate_agree() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    store.write_file("local/clock", "script.lua", b"return {\nend\n", None).unwrap();
    let v = store.validate("local/clock");
    let c = store.check_text("local/clock", "script.lua", "return {\nend\n");
    assert_eq!(v.issues.len(), 1);
    assert_eq!(v.issues[0].message, c.issues[0].message);
    assert_eq!(v.issues[0].line, c.issues[0].line);
}

#[test]
fn list_files_marks_text_and_binary() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    store.write_file("local/clock", "extra.svg", b"<svg/>", None).unwrap();
    std::fs::write(dir.path().join("local/clock/logo.png"), [0x89, b'P', b'N', b'G', 0xff, 0xfe]).unwrap();
    store.rebuild(); // see Step 3: expose rebuild_loader for tests
    let files = store.list_files("local/clock").unwrap();
    let names: Vec<_> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["meta.yaml", "script.lua", "screen.svg", "extra.svg", "logo.png"]);
    assert!(files.iter().find(|f| f.name == "logo.png").unwrap().text == false);
    assert!(files.iter().find(|f| f.name == "extra.svg").unwrap().text);
    assert_eq!(files.iter().find(|f| f.name == "extra.svg").unwrap().size, Some(6));
}

#[test]
fn list_files_unknown_ref_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let store = build_store(dir.path(), &["clock"]);
    assert!(matches!(store.list_files("local/nope"), Err(byonk::services::screen_store::StoreError::NotFound)));
}
```

- [ ] **Step 2: Run, expect compile failure**

Run: `cargo test --test screen_store_check_test`

- [ ] **Step 3: Implement**

`Issue` gains `pub line: Option<u32>`. `Severity` derives `PartialEq` already.

```rust
/// Best-effort line number from a validator message. Lua: `]:6:`; Tera
/// (pest): ` --> 3:5`. `None` when neither form is present.
pub fn error_line(message: &str) -> Option<u32> {
    if let Some(i) = message.find("]:") {
        let rest = &message[i + 2..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse() {
            return Some(n);
        }
    }
    if let Some(i) = message.find("--> ") {
        let rest = &message[i + 4..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse() {
            return Some(n);
        }
    }
    None
}

pub struct FileEntry {
    pub name: String,
    /// `None` when the file is over `MAX_FILE_BYTES` and was not read.
    pub size: Option<u64>,
    pub text: bool,
}
```

In `impl ScreenStore`:

```rust
    /// Run the validator that applies to `file` over `text`. `None` means
    /// "no check for this kind of file". Shared by `validate` and
    /// `check_text` so the two cannot drift.
    fn check_one(
        &self,
        source: &Arc<dyn ScreenRepoSource>,
        screen_path: &str,
        file: &str,
        text: &str,
    ) -> Option<Result<(), String>> {
        let name = file.rsplit('/').next().unwrap_or(file);
        if name == "meta.yaml" {
            return Some(ScreenMeta::from_yaml(text).map(|_| ()));
        }
        if name.ends_with(".lua") {
            return Some(
                mlua::Lua::new().load(text).into_function().map(|_| ()).map_err(|e| e.to_string()),
            );
        }
        if name.ends_with(".svg") {
            return Some(
                self.pipeline
                    .template_service()
                    .validate_template(text, source, screen_path)
                    .map_err(|e| e.to_string()),
            );
        }
        None
    }

    pub fn check_text(&self, screen_ref: &str, file: &str, text: &str) -> ValidationReport {
        let bad = |message: String| ValidationReport {
            ok: false,
            issues: vec![Issue { severity: Severity::Error, location: file.to_string(), line: None, message }],
        };
        let (handle, screen_path) = match Self::split_ref(screen_ref) {
            Ok(v) => v,
            Err(_) => return bad("invalid screen reference".into()),
        };
        if safe_rel(file).is_err() {
            return bad("invalid file name".into());
        }
        let loader = self.manager.loader();
        let Some(source) = loader.source_for(handle) else {
            return bad(format!("unknown screen repo '{handle}'"));
        };
        match self.check_one(&source, screen_path, file, text) {
            Some(Err(message)) => ValidationReport {
                ok: false,
                issues: vec![Issue {
                    severity: Severity::Error,
                    location: file.to_string(),
                    line: error_line(&message),
                    message,
                }],
            },
            _ => ValidationReport { ok: true, issues: vec![] },
        }
    }

    pub fn list_files(&self, screen_ref: &str) -> Result<Vec<FileEntry>, StoreError> {
        let (handle, screen_path) = Self::split_ref(screen_ref)?;
        let loader = self.manager.loader();
        let source = loader.source_for(handle).ok_or(StoreError::NotFound)?;
        let prefix = format!("{screen_path}/");
        let names: Vec<String> = source
            .screen_files(screen_path)
            .into_iter()
            .filter_map(|f| f.strip_prefix(&prefix).map(str::to_string))
            .collect();
        if names.is_empty() {
            return Err(StoreError::NotFound);
        }
        let core = ["meta.yaml", "script.lua", "screen.svg"];
        let mut out: Vec<FileEntry> = names
            .into_iter()
            .map(|name| {
                let (size, text) = match source.read_limited(&format!("{prefix}{name}"), MAX_FILE_BYTES) {
                    ReadOutcome::Found(b) => (Some(b.len() as u64), std::str::from_utf8(&b).is_ok()),
                    ReadOutcome::TooLarge => (None, false),
                    ReadOutcome::Missing => (Some(0), false),
                };
                FileEntry { name, size, text }
            })
            .collect();
        out.sort_by_key(|f| {
            let rank = core.iter().position(|c| *c == f.name).unwrap_or(core.len());
            (rank, f.name.clone())
        });
        Ok(out)
    }

    /// Test hook: re-read the repo directories after an out-of-band change.
    pub fn rebuild(&self) {
        self.manager.rebuild_loader();
    }
```

Rework `validate` so the three `check_file` closures call `self.check_one` (the read/missing/too-large handling stays), and fill `line: error_line(&message)` on every issue it builds. `check_one` is passed the `source` `validate` already holds. Every other `Issue { .. }` literal in the file gains `line: None`.

`src/mcp/tools_read.rs`: where `validate_screen` serialises issues, add `"line": i.line`.

- [ ] **Step 4: Run tests, expect pass**

Run: `cargo test --test screen_store_check_test && cargo test --workspace`

- [ ] **Step 5: Break, confirm, restore**

Make `check_one` return `None` for `.lua`. Three tests fail. Restore.

- [ ] **Step 6: Commit**

```bash
git add src/services/screen_store.rs src/mcp/tools_read.rs tests/screen_store_check_test.rs
git commit -m "feat(store): check_text for drafts, list_files, line numbers on issues"
```

---

### Task 6: `/dev/api` screen and file routes

**Files:**
- Modify: `src/api/dev_files.rs` (replace stub)
- Test: `tests/dev_files_test.rs` (new)

**Interfaces:**
- Consumes: `ScreenStore::{list_screens, list_files, read_file, write_file, delete_file, create_screen, copy_screen, rename_screen, delete_screen, validate, check_text}`, `StarterKind::Minimal`, Task 3 `ApiError` variants, `require_dev` (applied by the parent router).
- Produces: `pub fn files_router() -> Router<AppState>` with the routes from spec section 2.

- [ ] **Step 1: Write failing tests**

Fixture: a writable `local` repo via `TestApp::new_with_config_yaml` with `admin.token: secret` (bearer auth is the simplest way through `require_dev` in tests) and `screen_repos: local: path: <dir>/local` containing `byonk-screens.yaml` (copy the four lines from `tests/common/store.rs:31-35`). Helper `fn app(dir) -> TestApp` at the top of the test file. `AUTH` const as in `tests/admin_write_test.rs`. Add `put_text(path, headers, body)` to `TestApp` (same shape as `post_json` with `Request::put` and `Content-Type: text/plain`).

```rust
#[tokio::test]
async fn create_list_read_write_delete_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());

    let r = app.post_json("/dev/api/screen", &[AUTH], r#"{"handle":"local","name":"clock","starter":"minimal"}"#).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json::<serde_json::Value>()["ref"], "local/clock");

    let repos = app.get_with_headers("/dev/api/repos", &[AUTH]).await.json::<serde_json::Value>();
    assert!(repos.as_array().unwrap().iter().any(|x| x["handle"] == "local" && x["writable"] == true));

    let files = app.get_with_headers("/dev/api/files?ref=local/clock", &[AUTH]).await.json::<serde_json::Value>();
    assert_eq!(files[0]["name"], "meta.yaml");
    assert_eq!(files[1]["name"], "script.lua");

    let f = app.get_with_headers("/dev/api/file?ref=local/clock&name=script.lua", &[AUTH]).await;
    assert_eq!(f.status, StatusCode::OK);
    let etag = f.headers["etag"].to_str().unwrap().to_string();
    assert!(f.text().contains("return"));

    let w = app.put_text("/dev/api/file?ref=local/clock&name=script.lua", &[AUTH, ("If-Match", &etag)], "return { data = {} }\n").await;
    assert_eq!(w.status, StatusCode::NO_CONTENT);
    let new_etag = w.headers["etag"].to_str().unwrap().to_string();
    assert_ne!(new_etag, etag);

    let stale = app.put_text("/dev/api/file?ref=local/clock&name=script.lua", &[AUTH, ("If-Match", &etag)], "x").await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);

    let missing = app.put_text("/dev/api/file?ref=local/clock&name=script.lua", &[AUTH], "x").await;
    assert_eq!(missing.status, StatusCode::PRECONDITION_REQUIRED);

    let created = app.put_text("/dev/api/file?ref=local/clock&name=part.svg", &[AUTH, ("If-Match", "*")], "<svg/>").await;
    assert_eq!(created.status, StatusCode::NO_CONTENT);
    let dup = app.put_text("/dev/api/file?ref=local/clock&name=part.svg", &[AUTH, ("If-Match", "*")], "<svg/>").await;
    assert_eq!(dup.status, StatusCode::PRECONDITION_FAILED);

    let core = app.delete("/dev/api/file?ref=local/clock&name=script.lua", &[AUTH]).await;
    assert_eq!(core.status, StatusCode::UNPROCESSABLE_ENTITY);
    let extra = app.delete("/dev/api/file?ref=local/clock&name=part.svg", &[AUTH]).await;
    assert_eq!(extra.status, StatusCode::NO_CONTENT);

    let gone = app.delete("/dev/api/screen?ref=local/clock", &[AUTH]).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get_with_headers("/dev/api/files?ref=local/clock", &[AUTH]).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn copy_rename_validate_check() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let c = app.post_json("/dev/api/screen/copy", &[AUTH], r#"{"ref":"byonk-builtin/default","to_handle":"local","to_name":"mine"}"#).await;
    assert_eq!(c.status, StatusCode::OK);
    let rn = app.post_json("/dev/api/screen/rename", &[AUTH], r#"{"ref":"local/mine","name":"yours"}"#).await;
    assert_eq!(rn.json::<serde_json::Value>()["ref"], "local/yours");
    let v = app.post_json("/dev/api/screen/validate", &[AUTH], r#"{"ref":"local/yours"}"#).await;
    assert_eq!(v.json::<serde_json::Value>()["ok"], true);
    let ck = app.post_json("/dev/api/screen/check", &[AUTH], r#"{"ref":"local/yours","file":"script.lua","text":"return {\nend"}"#).await;
    let j: serde_json::Value = ck.json();
    assert_eq!(j["ok"], false);
    assert_eq!(j["issues"][0]["line"], 2);
}

#[tokio::test]
async fn read_only_repo_write_is_409_and_binary_read_is_415() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let w = app.put_text("/dev/api/file?ref=byonk-builtin/default&name=script.lua", &[AUTH, ("If-Match", "*")], "x").await;
    assert_eq!(w.status, StatusCode::CONFLICT);
    app.post_json("/dev/api/screen", &[AUTH], r#"{"handle":"local","name":"b","starter":"minimal"}"#).await;
    std::fs::write(dir.path().join("local/b/logo.png"), [0x89, 0x50, 0xff, 0xfe]).unwrap();
    let r = app.get_with_headers("/dev/api/file?ref=local/b&name=logo.png", &[AUTH]).await;
    assert_eq!(r.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn routes_are_gated() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    assert_eq!(app.get("/dev/api/repos").await.status, StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Run, expect failure (404s from the stub router)**

- [ ] **Step 3: Implement `src/api/dev_files.rs`**

```rust
//! `/dev/api` screen + file routes: thin JSON wrappers over `ScreenStore`.

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
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
        StoreError::Conflict => ApiError::PreconditionFailed("file changed on disk or target exists".into()),
        StoreError::BinaryOverwrite => ApiError::Unprocessable("target is a binary asset".into()),
        StoreError::Traversal => ApiError::BadRequest("path escapes the screen directory".into()),
        StoreError::TooLarge => ApiError::PayloadTooLarge,
        StoreError::Io(m) => ApiError::Internal(m),
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| ApiError::Internal(e.to_string()))
}

#[derive(Serialize)]
struct RepoOut { handle: String, writable: bool }

#[derive(Deserialize)]
struct RefQuery { r#ref: String }

#[derive(Deserialize)]
struct FileQuery { r#ref: String, name: String }

#[derive(Serialize)]
struct FileOut { name: String, size: Option<u64>, text: bool }

#[derive(Deserialize)]
struct CreateIn { handle: String, name: String, #[serde(default = "minimal")] starter: String }
fn minimal() -> String { "minimal".into() }

#[derive(Deserialize)]
struct CopyIn { r#ref: String, to_handle: String, to_name: String }

#[derive(Deserialize)]
struct RenameIn { r#ref: String, name: String }

#[derive(Deserialize)]
struct CheckIn { r#ref: String, file: String, text: String }

#[derive(Serialize)]
struct IssueOut { severity: &'static str, location: String, message: String, line: Option<u32> }

#[derive(Serialize)]
struct ReportOut { ok: bool, issues: Vec<IssueOut> }

impl From<ValidationReport> for ReportOut {
    fn from(r: ValidationReport) -> Self {
        let issues = r.issues.into_iter().map(|i: Issue| IssueOut {
            severity: match i.severity {
                crate::services::screen_store::Severity::Error => "error",
                crate::services::screen_store::Severity::Warning => "warning",
            },
            location: i.location, message: i.message, line: i.line,
        }).collect();
        ReportOut { ok: r.ok, issues }
    }
}

async fn repos(State(s): State<AppState>) -> Result<Json<Vec<RepoOut>>, ApiError> {
    let store = s.screen_store.clone();
    let loader = s.screen_repo_manager.loader();
    let mut handles: Vec<String> = s.config.load().screen_repos.keys().cloned().collect();
    for e in blocking(move || store.list_screens()).await? {
        if !handles.contains(&e.handle) { handles.push(e.handle); }
    }
    let mut out: Vec<RepoOut> = handles.into_iter().map(|h| RepoOut {
        writable: loader.source_for(&h).map(|src| src.writable_root().is_some()).unwrap_or(false),
        handle: h,
    }).collect();
    out.sort_by(|a, b| b.writable.cmp(&a.writable).then(a.handle.cmp(&b.handle)));
    Ok(Json(out))
}

async fn files(State(s): State<AppState>, Query(q): Query<RefQuery>) -> Result<Json<Vec<FileOut>>, ApiError> {
    let store = s.screen_store.clone();
    let list = blocking(move || store.list_files(&q.r#ref)).await?.map_err(map_store)?;
    Ok(Json(list.into_iter().map(|f| FileOut { name: f.name, size: f.size, text: f.text }).collect()))
}

async fn read_file(State(s): State<AppState>, Query(q): Query<FileQuery>) -> Result<Response, ApiError> {
    let store = s.screen_store.clone();
    let f = blocking(move || store.read_file(&q.r#ref, &q.name)).await?.map_err(map_store)?;
    if f.binary {
        return Err(ApiError::UnsupportedMedia("binary file".into()));
    }
    Ok(([(header::ETAG, f.etag), (header::CONTENT_TYPE, "text/plain; charset=utf-8".into())], f.bytes).into_response())
}

async fn write_file(State(s): State<AppState>, Query(q): Query<FileQuery>, headers: HeaderMap, body: Bytes) -> Result<Response, ApiError> {
    let Some(if_match) = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()).map(str::to_string) else {
        return Ok((StatusCode::PRECONDITION_REQUIRED, "If-Match header required").into_response());
    };
    let store = s.screen_store.clone();
    let etag = blocking(move || {
        if if_match == "*" {
            match store.read_file(&q.r#ref, &q.name) {
                Ok(_) => return Err(StoreError::Conflict),
                Err(StoreError::NotFound) => {}
                Err(e) => return Err(e),
            }
            store.write_file(&q.r#ref, &q.name, &body, None)
        } else {
            store.write_file(&q.r#ref, &q.name, &body, Some(&if_match))
        }
    }).await?.map_err(map_store)?;
    Ok((StatusCode::NO_CONTENT, [(header::ETAG, etag)]).into_response())
}

const CORE_FILES: [&str; 3] = ["meta.yaml", "script.lua", "screen.svg"];

async fn delete_file(State(s): State<AppState>, Query(q): Query<FileQuery>) -> Result<StatusCode, ApiError> {
    if CORE_FILES.contains(&q.name.as_str()) {
        return Err(ApiError::Unprocessable(format!("{} cannot be deleted", q.name)));
    }
    let store = s.screen_store.clone();
    blocking(move || store.delete_file(&q.r#ref, &q.name)).await?.map_err(map_store)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create(State(s): State<AppState>, Json(b): Json<CreateIn>) -> Result<Json<serde_json::Value>, ApiError> {
    if b.starter != "minimal" {
        return Err(ApiError::BadRequest(format!("unknown starter '{}'", b.starter)));
    }
    let store = s.screen_store.clone();
    let r = blocking(move || store.create_screen(&b.handle, &b.name, StarterKind::Minimal)).await?.map_err(map_store)?;
    Ok(Json(serde_json::json!({"ref": r})))
}

async fn copy(State(s): State<AppState>, Json(b): Json<CopyIn>) -> Result<Json<serde_json::Value>, ApiError> {
    let store = s.screen_store.clone();
    let r = blocking(move || store.copy_screen(&b.r#ref, &b.to_handle, &b.to_name)).await?.map_err(map_store)?;
    Ok(Json(serde_json::json!({"ref": r})))
}

async fn rename(State(s): State<AppState>, Json(b): Json<RenameIn>) -> Result<Json<serde_json::Value>, ApiError> {
    let store = s.screen_store.clone();
    let r = blocking(move || store.rename_screen(&b.r#ref, &b.name)).await?.map_err(map_store)?;
    Ok(Json(serde_json::json!({"ref": r})))
}

async fn delete_screen(State(s): State<AppState>, Query(q): Query<RefQuery>) -> Result<StatusCode, ApiError> {
    let store = s.screen_store.clone();
    blocking(move || store.delete_screen(&q.r#ref)).await?.map_err(map_store)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn validate(State(s): State<AppState>, Json(b): Json<RefQuery>) -> Result<Json<ReportOut>, ApiError> {
    let store = s.screen_store.clone();
    Ok(Json(blocking(move || store.validate(&b.r#ref)).await?.into()))
}

async fn check(State(s): State<AppState>, Json(b): Json<CheckIn>) -> Result<Json<ReportOut>, ApiError> {
    let store = s.screen_store.clone();
    Ok(Json(blocking(move || store.check_text(&b.r#ref, &b.file, &b.text)).await?.into()))
}

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
```

Check `copy_screen`'s and `rename_screen`'s real argument order at `src/services/screen_store.rs:702` and `:788` and match it. `PRECONDITION_REQUIRED` is 428; the spec said "If-Match required", this is the status that says so.

- [ ] **Step 4: Run tests, expect pass**

Run: `cargo test --test dev_files_test && cargo test --workspace`

- [ ] **Step 5: Break, confirm, restore**

Remove the `CORE_FILES.contains` guard. `create_list_read_write_delete_round_trip` fails at the 422 assertion. Restore.

- [ ] **Step 6: Commit**

```bash
git add src/api/dev_files.rs tests/dev_files_test.rs tests/common/app.rs
git commit -m "feat(dev): /dev/api screen and file routes over ScreenStore"
```

---

### Task 7: CodeMirror 6 bundle

**Files:**
- Create: `static/dev/editor/package.json`, `rollup.config.mjs`, `src/editor.js`, `hash.sh`, `stamp.mjs`, `.gitignore` (`node_modules/`, `dist/`)
- Create: `static/dev/editor.bundle.js` (built, committed)
- Modify: `Makefile` (`editor`, `editor-check`, `check`)

**Interfaces:**
- Produces: `window.byonkEditor.create(parent, {language, doc, readOnly, onChange, onSave})` returning `{getValue(), setValue(text), setDiagnostics(issues), focus(), destroy()}` where `language` is `"lua" | "xml" | "yaml" | "text"` and `issues` is `[{line, message}]`.

- [ ] **Step 1: Create the project**

```bash
cd static/dev/editor
pnpm init
pnpm add codemirror @codemirror/state @codemirror/view @codemirror/commands @codemirror/language @codemirror/search @codemirror/lint @codemirror/lang-xml @codemirror/lang-yaml @codemirror/legacy-modes
pnpm add -D rollup @rollup/plugin-node-resolve @rollup/plugin-terser
```

`package.json` scripts: `"build": "rollup -c && node stamp.mjs"`. Set `"private": true`, `"type": "module"`.

`rollup.config.mjs`:

```js
import resolve from "@rollup/plugin-node-resolve";
import terser from "@rollup/plugin-terser";
export default {
  input: "src/editor.js",
  output: { file: "dist/editor.bundle.js", format: "iife", name: "byonkEditor" },
  plugins: [resolve(), terser()],
};
```

`src/editor.js`:

```js
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLine } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, StreamLanguage, syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { linter, lintGutter, setDiagnostics as cmSetDiagnostics } from "@codemirror/lint";
import { xml } from "@codemirror/lang-xml";
import { yaml } from "@codemirror/lang-yaml";
import { lua } from "@codemirror/legacy-modes/mode/lua";

function languageFor(name) {
  if (name === "lua") return StreamLanguage.define(lua);
  if (name === "xml") return xml();
  if (name === "yaml") return yaml();
  return [];
}

export function create(parent, opts) {
  const onSave = () => { if (opts.onSave) opts.onSave(); return true; };
  const state = EditorState.create({
    doc: opts.doc || "",
    extensions: [
      lineNumbers(), highlightActiveLine(), history(), bracketMatching(),
      highlightSelectionMatches(), lintGutter(),
      syntaxHighlighting(defaultHighlightStyle),
      languageFor(opts.language),
      EditorState.readOnly.of(!!opts.readOnly),
      keymap.of([{ key: "Mod-s", run: onSave }, indentWithTab, ...defaultKeymap, ...historyKeymap, ...searchKeymap]),
      EditorView.updateListener.of((u) => { if (u.docChanged && opts.onChange) opts.onChange(); }),
    ],
  });
  const view = new EditorView({ state, parent });
  return {
    getValue: () => view.state.doc.toString(),
    setValue: (text) => view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } }),
    setDiagnostics: (issues) => {
      const diags = issues.filter((i) => i.line).map((i) => {
        const ln = view.state.doc.line(Math.min(i.line, view.state.doc.lines));
        return { from: ln.from, to: ln.to, severity: "error", message: i.message };
      });
      view.dispatch(cmSetDiagnostics(view.state, diags));
    },
    focus: () => view.focus(),
    destroy: () => view.destroy(),
  };
}
```

`hash.sh` (used by stamp and by `make editor-check`):

```sh
#!/bin/sh
# SHA-256 over the editor sources, portable between macOS and Linux.
cd "$(dirname "$0")"
find src package.json pnpm-lock.yaml -type f | LC_ALL=C sort | xargs cat | (sha256sum 2>/dev/null || shasum -a 256) | cut -c1-64
```

`stamp.mjs`:

```js
import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
const hash = execSync("sh hash.sh").toString().trim();
const body = readFileSync("dist/editor.bundle.js", "utf8");
writeFileSync("../editor.bundle.js", `// byonk-editor-src-sha256: ${hash}\n${body}`);
```

Makefile additions:

```make
# Rebuild the CodeMirror bundle embedded in the binary
editor:
	cd static/dev/editor && pnpm install --frozen-lockfile && pnpm run build

# Fail when the committed bundle no longer matches its sources
editor-check:
	@want=$$(sh static/dev/editor/hash.sh); \
	have=$$(head -1 static/dev/editor.bundle.js | sed 's/^.*sha256: //'); \
	if [ "$$want" != "$$have" ]; then echo "editor.bundle.js is stale: run make editor"; exit 1; fi

check: fmt lint test editor-check
```

Add `editor` and `editor-check` to `.PHONY` and to `make help`.

- [ ] **Step 2: Build and check**

Run: `make editor && make editor-check && head -c 200 static/dev/editor.bundle.js`
Expected: bundle starts with the stamp line; check passes. Then edit a comment in `src/editor.js`, run `make editor-check`, expect failure; revert.

- [ ] **Step 3: Commit**

```bash
git add static/dev/editor/package.json static/dev/editor/pnpm-lock.yaml static/dev/editor/rollup.config.mjs static/dev/editor/src/editor.js static/dev/editor/hash.sh static/dev/editor/stamp.mjs static/dev/editor/.gitignore static/dev/editor.bundle.js Makefile
git commit -m "build(dev): CodeMirror 6 editor bundle with source hash stamp"
```

---

### Task 8: The page: shell, screens list, editor, save, problems, live reload

**Files:**
- Rewrite: `static/dev/index.html`, `static/dev/dev.css`, `static/dev/dev.js`

**Interfaces:**
- Consumes: `/dev/api/*` from Tasks 4 and 6, `window.byonkEditor` from Task 7.
- Produces: the DOM ids listed below; Task 9 (drawer) and Task 10 (pop-out) plug into them. `dev.js` exposes `window.byonkDev = { state, render, api }` for Task 9 and 10.

- [ ] **Step 1: `index.html` skeleton (layout A)**

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Byonk Dev</title>
<link rel="stylesheet" href="dev.css">
</head>
<body>
<header class="topbar">
  <strong>Byonk</strong>
  <span id="current-ref" class="pill">no screen</span>
  <span class="spacer"></span>
  <span id="watch-status" class="status-badge disconnected">File Watch: Connecting...</span>
  <button id="simulate-toggle" class="ghost">Simulate</button>
  <button id="save-btn" class="primary" disabled>Save</button>
  <span id="whoami" class="muted"></span>
  <form id="logout-form" method="post" action="logout" hidden><button class="ghost">Logout</button></form>
</header>
<main class="columns">
  <aside class="screens">
    <div class="toolbar">
      <button id="new-btn">New</button>
      <button id="copy-btn" disabled>Copy</button>
      <button id="rename-btn" disabled>Rename</button>
      <button id="delete-btn" disabled>Delete</button>
    </div>
    <form id="inline-form" class="inline-form" hidden>
      <span id="inline-title"></span>
      <select id="inline-repo"></select>
      <input id="inline-name" placeholder="name" required>
      <button type="submit">OK</button>
      <button type="button" id="inline-cancel">Cancel</button>
    </form>
    <div id="screen-list"></div>
  </aside>
  <section class="editor">
    <div id="tabs" class="tabs"></div>
    <div id="editor-host"></div>
    <div id="problems" class="problems"></div>
  </section>
  <section class="preview">
    <div class="preview-head">
      <span id="display-size">800 x 480</span>
      <span id="render-time"></span>
      <button id="popout-btn" class="ghost">Pop out</button>
    </div>
    <div id="device-frame" class="device-frame og">
      <div class="device-bezel"><div class="screen-well">
        <img id="display-image" alt="Screen preview">
        <div id="loading-overlay" class="loading-overlay hidden"><div class="spinner"></div></div>
      </div><div id="bezel-label" class="bezel-label">BYONK</div></div>
    </div>
    <div id="console" class="console"><pre id="console-output"></pre></div>
    <div id="drawer" class="drawer" hidden><!-- Task 9 moves the configuration panel here --></div>
  </section>
</main>
<script src="editor.bundle.js"></script>
<script src="dev.js"></script>
</body>
</html>
```

Keep the `#color-popup` block from the old file verbatim after `</main>`; Task 9 needs it.

- [ ] **Step 2: `dev.js` core**

Structure the file as: `api` (fetch helpers), `state`, `screens` (list + toolbar), `tabs` (open files), `problems`, `render`, `sse`, `init`. Existing functions `consoleClear`, `consoleLog`, `consoleError`, `setupLens`, `updateLensImage`, `showLens`, `hideLens`, `moveLens`, `updateDeviceFrame` move over unchanged. Key new code:

```js
const api = {
  async json(path, opts = {}) {
    const r = await fetch(path, opts);
    if (r.status === 401) { location.href = "login"; throw new Error("login required"); }
    if (!r.ok) throw new Error((await r.text()) || r.statusText);
    return r.status === 204 ? null : r.json();
  },
  repos: () => api.json("./api/repos"),
  screens: () => api.json("./api/screens"),
  files: (ref) => api.json(`./api/files?ref=${encodeURIComponent(ref)}`),
  async readFile(ref, name) {
    const r = await fetch(`./api/file?ref=${encodeURIComponent(ref)}&name=${encodeURIComponent(name)}`);
    if (!r.ok) throw new Error(await r.text());
    return { text: await r.text(), etag: r.headers.get("ETag") };
  },
  async writeFile(ref, name, text, etag) {
    const r = await fetch(`./api/file?ref=${encodeURIComponent(ref)}&name=${encodeURIComponent(name)}`,
      { method: "PUT", headers: { "If-Match": etag, "Content-Type": "text/plain" }, body: text });
    if (r.status === 412) return { conflict: true };
    if (!r.ok) throw new Error(await r.text());
    return { etag: r.headers.get("ETag") };
  },
  deleteFile: (ref, name) => api.json(`./api/file?ref=${encodeURIComponent(ref)}&name=${encodeURIComponent(name)}`, { method: "DELETE" }),
  create: (handle, name) => api.json("./api/screen", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ handle, name, starter: "minimal" }) }),
  copy: (ref, to_handle, to_name) => api.json("./api/screen/copy", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ref, to_handle, to_name }) }),
  rename: (ref, name) => api.json("./api/screen/rename", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ref, name }) }),
  deleteScreen: (ref) => api.json(`./api/screen?ref=${encodeURIComponent(ref)}`, { method: "DELETE" }),
  validate: (ref) => api.json("./api/screen/validate", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ref }) }),
  check: (ref, file, text) => api.json("./api/screen/check", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ref, file, text }) }),
  whoami: () => api.json("./api/whoami"),
};

const state = {
  repos: [],          // [{handle, writable}]
  screens: [],        // from /api/screens (also devices, panels for the drawer)
  ref: null,          // selected screen ref
  writable: false,
  files: [],          // [{name, size, text}]
  open: new Map(),    // name -> {editor, etag, dirty, saved}
  active: null,       // file name
};
window.byonkDev = { state, api, render: null };
```

Tabs and editor:

```js
function languageFor(name) {
  if (name.endsWith(".lua")) return "lua";
  if (name.endsWith(".svg") || name.endsWith(".xml")) return "xml";
  if (name.endsWith(".yaml") || name.endsWith(".yml")) return "yaml";
  return "text";
}

async function openFile(name) {
  let entry = state.open.get(name);
  if (!entry) {
    const { text, etag } = await api.readFile(state.ref, name);
    const host = document.createElement("div");
    host.className = "editor-instance";
    document.getElementById("editor-host").appendChild(host);
    entry = { host, etag, dirty: false, saved: text, editor: null };
    entry.editor = byonkEditor.create(host, {
      language: languageFor(name), doc: text, readOnly: !state.writable,
      onChange: () => { markDirty(name, entry.editor.getValue() !== entry.saved); scheduleCheck(name); },
      onSave: () => saveFile(name),
    });
    state.open.set(name, entry);
  }
  state.active = name;
  for (const [n, e] of state.open) e.host.hidden = n !== name;
  renderTabs();
  entry.editor.focus();
}

function markDirty(name, dirty) {
  const e = state.open.get(name);
  if (!e || e.dirty === dirty) return;
  e.dirty = dirty;
  renderTabs();
  document.getElementById("save-btn").disabled = !state.writable || ![...state.open.values()].some((x) => x.dirty);
  window.onbeforeunload = [...state.open.values()].some((x) => x.dirty) ? () => true : null;
}

let checkTimer = null;
function scheduleCheck(name) {
  clearTimeout(checkTimer);
  checkTimer = setTimeout(async () => {
    const e = state.open.get(name);
    if (!e) return;
    const report = await api.check(state.ref, name, e.editor.getValue());
    e.editor.setDiagnostics(report.issues);
    showProblems(report.issues.map((i) => ({ ...i, location: name })));
  }, 400);
}

async function saveFile(name) {
  const e = state.open.get(name);
  if (!e || !state.writable) return;
  const text = e.editor.getValue();
  const r = await api.writeFile(state.ref, name, text, e.etag);
  if (r.conflict) { showConflict(name); return; }
  e.etag = r.etag; e.saved = text; markDirty(name, false);
  const report = await api.validate(state.ref);
  showProblems(report.issues, report.ok);
  render();
}

function showConflict(name) {
  const p = document.getElementById("problems");
  p.innerHTML = `<span class="err">${name} changed on disk.</span> `;
  const reload = Object.assign(document.createElement("button"), { textContent: "Reload" });
  reload.onclick = async () => { const f = await api.readFile(state.ref, name); const e = state.open.get(name); e.editor.setValue(f.text); e.etag = f.etag; e.saved = f.text; markDirty(name, false); showProblems([]); };
  const over = Object.assign(document.createElement("button"), { textContent: "Overwrite" });
  over.onclick = async () => { const f = await api.readFile(state.ref, name); state.open.get(name).etag = f.etag; saveFile(name); };
  p.append(reload, " ", over);
}

function showProblems(issues, ok) {
  const p = document.getElementById("problems");
  if (!issues.length) { p.innerHTML = ok === undefined ? "" : `<span class="ok">✔ all files valid</span>`; return; }
  p.innerHTML = issues.map((i) => `<div class="err">✖ ${i.location}${i.line ? ":" + i.line : ""} ${escapeHtml(i.message)}</div>`).join("");
}
```

`escapeHtml` is a four-line replace helper. Screens list: `loadScreens()` calls `api.repos()` and `api.screens()`, groups screen refs by handle in repo order, renders `.repo` headings and `.scr` rows (class `ro` for read-only), click selects: `selectScreen(ref)` checks dirty buffers with an in-page confirm bar (`#problems` shows "Unsaved changes. Discard / Cancel"), then clears `state.open` (destroying editors), loads `api.files(ref)`, sets `state.writable`, opens `script.lua`, updates `#current-ref`, toolbar button states, `localStorage.byonkDevRef`, and calls `render()`.

Toolbar: `New` shows `#inline-form` with writable repos in `#inline-repo`; submit calls `api.create` then `loadScreens()` and `selectScreen`. `Copy` pre-fills the source name; submit calls `api.copy`. `Rename` hides the repo select; submit calls `api.rename`. `Delete` shows the inline form with title "Delete <ref>?" and only OK/Cancel; submit calls `api.deleteScreen`. Binary files render as `<span class="tab disabled">name (size)</span>`; the `+` tab shows the inline form with title "New file", submit does `api.writeFile(ref, name, "", "*")` then refreshes files and opens it. Tabs have a close `×` that discards a clean buffer or asks first.

`render()` builds the query from `state.ref` plus the drawer values (Task 9 fills `drawerParams()`; until then it returns `{}`), sets `#display-image.src` to `./api/render?...`, updates size/time, and posts to the `BroadcastChannel` (Task 10). Assign `window.byonkDev.render = render`.

SSE (`connectSSE`) uses `new EventSource("./api/events")`; on `file-change` with paths, for each open file whose name is in the list: clean buffer → reload silently; dirty → `showConflict(name)`. Then `render()`.

`init()`: `whoami` fills `#whoami` and unhides `#logout-form` when `can_logout`; then `loadScreens()`, restore `localStorage.byonkDevRef`, `connectSSE()`, `setupLens()`.

- [ ] **Step 3: `dev.css`**

Grid: `.columns { display: grid; grid-template-columns: 220px 1fr 380px; height: calc(100vh - 48px); }`. Keep the existing device-frame, bezel, console, lens, color-popup and status-badge rules from the old file. Add rules for `.topbar`, `.screens`, `.repo`, `.scr`, `.scr.sel`, `.ro`, `.tabs`, `.tab`, `.tab.on`, `.tab.dirty::after { content: " ●" }`, `.editor-instance { height: 100% }`, `.problems`, `.err`, `.ok`, `.drawer` (absolute, right 0, full height of `.preview`, slides with `transform`), `.login-body`, `.login-card`. The editor section is a column flex with `#editor-host { flex: 1; min-height: 0; overflow: auto }`.

- [ ] **Step 4: Run and click through**

Run: `BYONK_DEV_USERNAME=u BYONK_DEV_PASSWORD=p CONFIG_FILE=config.yaml SCREENS_DIR=screens FONTS_DIR=fonts cargo run -- serve`
Open `http://localhost:3000/dev/`, log in, create `local/plan-test`, type `return {` into `script.lua`, see the marker within a second, fix, Cmd+S, see the preview, close the tab, delete the screen, log out. Use the Chrome tools for this and keep a screenshot in the scratchpad.

- [ ] **Step 5: Commit**

```bash
git add static/dev/index.html static/dev/dev.css static/dev/dev.js
git commit -m "feat(dev): three-column screen editor page"
```

---

### Task 9: Simulate drawer and server-fed dither list

**Files:**
- Modify: `static/dev/index.html` (`#drawer` contents), `static/dev/dev.js`
- Modify: `src/api/dev.rs::handle_screens` and `ScreensResponse` (add `dither_algorithms`)

**Interfaces:**
- Produces: `dither_algorithms: Vec<DitherAlgorithmInfo { name: String, defaults: DitherDefaults }>` in `/dev/api/screens`; `drawerParams()` in `dev.js` returning the render query fields.

- [ ] **Step 1: Server side**

In `handle_screens`, add to the response `dither_algorithms: crate::models::config::DITHER_ALGORITHMS.iter().map(|n| DitherAlgorithmInfo { name: n.to_string(), defaults: DitherAlgorithm::from_name(n).defaults() }).collect()`. Check the real names of the defaults accessor in `crates/eink-dither/src/dither/mod.rs` and `src/models/config.rs` (`DITHER_ALGORITHMS`, `normalize_algorithm_name`) and use those. Add a test in `tests/dev_files_test.rs` that `/dev/api/screens` lists nine algorithm names and that `floyd-steinberg` has a numeric `noise_scale` default.

- [ ] **Step 2: Move the configuration panel**

Cut the whole `<div class="panel config-panel">…</div>` from the old `index.html` (kept in git history at `133c528`) into `#drawer`, minus the `Screen` select (the screens column replaces it) and with the hard-coded dither `<option>` list emptied. In `dev.js`, carry over unchanged: `formatDatetimeLocal`, `tickTime`, `setTimeLocked`, `getEffectiveDimensions`, `getEffectiveColors`, `applyDitherDefaults`, `saveDitherTuningOverride`, `updateColorSwatches`, `updateUseActualVisibility`, `applyRestoredColorOverrides`, `saveState`, `loadSavedState`, the colour helpers (`hexToRgb` through `hslToHex`), `openColorPopup`, `closeColorPopup`, `updateColorFromSliders`, `updateColorFromHex`, `applyColorChange`, `copyActualColors`, `copyToClipboard`, `setupColorPopup`, and the device/panel parts of `setupEventListeners` and `loadScreens`. Replace `DITHER_DEFAULTS` with the list from `state.screens.dither_algorithms`; populate `#dither-select` from it. Every `fetch("/dev/…")` becomes `fetch("./api/…")`.

`drawerParams()` returns `{model, width, height, battery_voltage, rssi, timestamp, params, panel, use_actual, dither, colors_actual, max_error, chroma_clamp, noise_scale, strength, mac}` from the drawer inputs, omitting empties. `render()` uses it. `#simulate-toggle` toggles `#drawer.hidden` and an `open` class. Any drawer input change calls `render()` and, when a device is selected, the existing `saveDitherTuningOverride()`.

- [ ] **Step 3: Verify**

Same run command as Task 8. Open the drawer, switch dither to `floyd-steinberg`, change noise scale, confirm the preview re-renders and that selecting the E1004 device still posts to `./api/panel-colors` when a swatch is changed (watch the network tab).

- [ ] **Step 4: Commit**

```bash
git add static/dev/index.html static/dev/dev.js src/api/dev.rs tests/dev_files_test.rs
git commit -m "feat(dev): simulate drawer; dither algorithms come from the server"
```

---

### Task 10: Pop-out preview

**Files:**
- Create: `static/dev/preview.html` (replace placeholder)
- Modify: `static/dev/dev.js` (`render()` broadcast, `#popout-btn`)

- [ ] **Step 1: `preview.html`**

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Byonk Preview</title>
<link rel="stylesheet" href="dev.css">
<style>
  body { margin: 0; background: #222; display: flex; flex-direction: column; height: 100vh; }
  .bar { padding: 4px 8px; color: #ccc; font: 12px system-ui; display: flex; gap: 8px; align-items: center; }
  .stage { flex: 1; overflow: auto; display: flex; align-items: center; justify-content: center; }
  .stage.fit img { max-width: 100%; max-height: 100%; }
  .stage.full img { image-rendering: pixelated; }
</style>
</head>
<body>
<div class="bar">
  <span id="size"></span>
  <label><input type="radio" name="zoom" value="fit" checked> Fit</label>
  <label><input type="radio" name="zoom" value="full"> 100%</label>
</div>
<div id="stage" class="stage fit"><img id="display-image" alt="preview"></div>
<script>
  const ch = new BroadcastChannel("byonk-preview");
  const img = document.getElementById("display-image");
  ch.onmessage = (ev) => {
    if (!ev.data || !ev.data.url) return;
    img.src = ev.data.url;
    document.getElementById("size").textContent = `${ev.data.width} × ${ev.data.height}`;
  };
  document.querySelectorAll("input[name=zoom]").forEach((r) => r.onchange = () => {
    document.getElementById("stage").className = "stage " + r.value;
  });
  ch.postMessage({ hello: true });
</script>
</body>
</html>
```

- [ ] **Step 2: Main page side**

In `dev.js`:

```js
const previewChannel = new BroadcastChannel("byonk-preview");
let lastRender = null;
previewChannel.onmessage = (ev) => { if (ev.data && ev.data.hello && lastRender) previewChannel.postMessage(lastRender); };
document.getElementById("popout-btn").onclick = () => window.open("preview.html", "byonk-preview", "width=900,height=700");
```

At the end of `render()` after the image URL is known: `lastRender = { url, width, height }; previewChannel.postMessage(lastRender);`. The pixel lens moves out of `dev.js` into `static/dev/lens.js` (`setupLens`, `updateLensImage`, `showLens`, `hideLens`, `moveLens`), served by a new `handle_lens_js` handler at `/dev/lens.js` in the protected group of `dev_router`, and loaded by both `index.html` and `preview.html` with `<script src="lens.js">` before their own script.

- [ ] **Step 3: Verify**

Open the pop-out, change the dither in the drawer, confirm the pop-out image updates. Switch to 100% on a 1200×1600 panel and confirm scrollbars.

- [ ] **Step 4: Commit**

```bash
git add static/dev/preview.html static/dev/dev.js static/dev/lens.js src/api/dev.rs
git commit -m "feat(dev): pop-out preview window following the main page"
```

---

### Task 11: Add-on manifest, translations, DOCS

**Files:**
- Modify: `homeassistant/byonk/config.yaml`, `translations/en.yaml`, `DOCS.md`, `CHANGELOG.md`
- Test: `tests/addon_manifest_test.rs`

- [ ] **Step 1: Failing manifest test**

```rust
#[test]
fn addon_config_declares_ingress_and_dev_login() {
    let cfg = load_yaml("homeassistant/byonk/config.yaml");
    assert_eq!(cfg["ingress"], true);
    assert_eq!(cfg["ingress_port"], 8099);
    assert_eq!(cfg["ingress_entry"], "/dev/");
    assert_eq!(cfg["panel_admin"], true);
    assert_eq!(cfg["environment"]["BYONK_INGRESS_ADDR"], "0.0.0.0:8099");
    assert_eq!(cfg["schema"]["dev_username"], "str?");
    assert_eq!(cfg["schema"]["dev_password"], "password?");
    assert!(cfg["ports"].as_mapping().unwrap().get("8099/tcp").is_none(), "ingress port must not be published");
}
```

- [ ] **Step 2: Edit the manifest**

Add the keys from spec section 1, bump `version` to the next patch (e.g. `0.19.1`; the release workflow sets the final one), add `BYONK_INGRESS_ADDR: 0.0.0.0:8099` under `environment`, `dev_username: ""` and `dev_password: ""` under `options`, the two schema rows. `translations/en.yaml`:

```yaml
  dev_username:
    name: Dev UI username
    description: >-
      Only needed to open the screen editor from outside Home Assistant
      (http://<host>:3000/dev/). Inside Home Assistant use the Byonk sidebar entry.
  dev_password:
    name: Dev UI password
    description: Password for the username above. Leave both blank to disable the external login.
```

`DOCS.md`: a "Screen editor" section: sidebar entry, what it does, the optional external login. `CHANGELOG.md` in the add-on dir gets the same user-facing lines as `CHANGES.md`.

- [ ] **Step 3: Run, commit**

Run: `cargo test --test addon_manifest_test`

```bash
git add homeassistant/byonk/config.yaml homeassistant/byonk/translations/en.yaml homeassistant/byonk/DOCS.md homeassistant/byonk/CHANGELOG.md tests/addon_manifest_test.rs
git commit -m "feat(addon): Ingress sidebar entry and optional dev UI login"
```

---

### Task 12: Docs, changelog, gitignore

**Files:**
- Modify: `docs/src/guide/dev-mode.md`, `docs/src/guide/configuration.md`, `docs/src/api/lua-api.md:1332`, `README.md`, `CHANGES.md`, `.gitignore`

- [ ] **Step 1: Rewrite `dev-mode.md`**

Sections: Opening the editor (URL, login, env vars, HA sidebar), The three columns, Editing and saving (tabs, dirty marker, Cmd+S, conflicts), Syntax checking (what `check` catches, what only a render catches), Preview and pop-out, Simulate drawer (move the existing calibration text here unchanged), Live reload, API (the `/dev/api` table from the spec). Replace `images/dev-mode-screenshot.png` with a fresh capture from Task 8 Step 4.

- [ ] **Step 2: Remove `byonk dev` everywhere**

`grep -rn "byonk dev" README.md docs/src` and fix each hit. `lua-api.md:1332` says `/dev/api/render`.

- [ ] **Step 3: `CHANGES.md` Unreleased**

```markdown
### Breaking
- `byonk dev` is gone. The dev UI is part of `byonk serve` at `/dev/` and needs a login: set `BYONK_DEV_USERNAME` and `BYONK_DEV_PASSWORD` (or the add-on options `dev_username` / `dev_password`). The admin bearer token is accepted too. The former `/dev/*` endpoints moved to `/dev/api/*`.

### Added
- Screen editor in the dev UI: create, copy, rename and delete screens in writable repos, edit every file with syntax checking while you type, and save with conflict detection.
- Home Assistant sidebar entry for the editor (add-on Ingress, no extra login).
- Pop-out preview window with fit and 100 % zoom.
- The dither algorithm list in the dev UI now comes from the server.
```

- [ ] **Step 4: `.gitignore`**

Append `.superpowers/`.

- [ ] **Step 5: Build docs and commit**

Run: `cd docs && mdbook build`

```bash
git add docs/src/guide/dev-mode.md docs/src/guide/configuration.md docs/src/api/lua-api.md docs/src/images/dev-mode-screenshot.png README.md CHANGES.md .gitignore
git commit -m "docs: dev mode editor, login and ingress; changelog"
```

---

### Task 13: Full verification

- [ ] **Step 1: The four commands**

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd docs && mdbook build)
make editor-check
```

All must pass. Fix anything that fails in its own commit.

- [ ] **Step 2: Browser run-through**

With the Chrome tools against `cargo run -- serve` as in Task 8 Step 4: no-credentials run shows 404 on `/dev/`; with credentials: login, create, error marker, fix, save, preview, pop-out follows a drawer change, copy `examples/…` into `local`, delete, logout returns to the form. Record a GIF `dev-editor-walkthrough.gif` in the scratchpad.

- [ ] **Step 3: Home Assistant VM**

Follow the `ha-vm-testing` skill: build from source, bump the manifest, `POST /store/reload`, update the add-on. Open the sidebar entry: page loads under `/api/hassio_ingress/…/dev/`, `#whoami` shows the HA user, no Logout button. Edit and save a file. From the Mac host, `http://<vm>:3000/dev/` must ask for the password. Ask the owner before any `ssh`.

- [ ] **Step 4: Push and open a PR**

```bash
git push -u origin feat/dev-editor
gh pr create --title "feat: always-on dev UI with login, screen editor and HA ingress" --body-file <scratchpad>/pr-body.md
```

PR body: summary, the breaking change, the test list, screenshots, and "🤖 Generated with [Claude Code](https://claude.com/claude-code)".
