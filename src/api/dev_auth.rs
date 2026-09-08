//! Dev UI authentication: credentials, session cookies, and the
//! `require_dev` middleware with its three admission paths.

use axum::{
    extract::{ConnectInfo, OriginalUri, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};

use crate::server::AppState;

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
    v.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub const COOKIE_NAME: &str = "byonk_dev";
pub const SESSION_MAX_AGE_SECS: i64 = 30 * 24 * 3600;

fn mac_hex(key: &[u8; 32], issued: i64) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(issued.to_string().as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl DevAuth {
    /// No form login. Ingress and bearer paths still work.
    pub fn disabled() -> Self {
        Self {
            credentials: None,
            session_key: fresh_key(),
        }
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

    /// `<issued_unix>.<hex hmac>`; `now` is injected for tests.
    pub fn issue_cookie_value(&self, now: i64) -> String {
        format!("{now}.{}", mac_hex(&self.session_key, now))
    }

    pub fn verify_cookie_value(&self, value: &str, now: i64) -> bool {
        let Some((issued, mac)) = value.split_once('.') else {
            return false;
        };
        let Ok(issued) = issued.parse::<i64>() else {
            return false;
        };
        if now.saturating_sub(issued) > SESSION_MAX_AGE_SECS || issued > now.saturating_add(60) {
            return false;
        }
        crate::api::admin::constant_time_eq(
            mac.as_bytes(),
            mac_hex(&self.session_key, issued).as_bytes(),
        )
    }
}

/// Marks a request as having arrived on the ingress listener (as opposed to
/// the public one). Inserted by the listener setup, never by a client.
#[derive(Debug, Clone, Copy)]
pub struct ViaIngress;

/// The only peer address `require_dev` trusts on the ingress listener: the
/// HA Supervisor's fixed address for add-on ingress proxying.
pub const SUPERVISOR_ADDR: std::net::IpAddr =
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(172, 30, 32, 2));

/// Who `require_dev` decided is asking. Inserted into request extensions on
/// success so downstream handlers (e.g. `whoami`) can read it back.
#[derive(Debug, Clone)]
pub enum DevPrincipal {
    /// Home Assistant Ingress, carrying the `X-Remote-User-Name` value (or a
    /// fallback) as the display name.
    Ingress(String),
    /// The admin bearer token.
    Token,
    /// A verified session cookie, carrying the configured dev username.
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

    /// Only a cookie session can be logged out of; ingress and token access
    /// aren't sessions to end.
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
    let prefix = format!("{COOKIE_NAME}=");
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|line| line.split(';'))
        .map(str::trim)
        .find_map(|kv| kv.strip_prefix(&prefix).map(str::to_string))
}

/// Decide who is asking. `Err(status)` is the refusal to send when nobody
/// matched: 403 only on the ingress listener from a foreign peer.
fn admit(state: &AppState, request: &Request) -> Result<DevPrincipal, StatusCode> {
    let headers = request.headers();

    if request.extensions().get::<ViaIngress>().is_some() {
        let peer = request
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|c| c.0.ip());
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
///
/// Before any admission check, normalizes the trailing slash on the nest
/// root: axum's `nest` rewrites `request.uri()` to the path stripped of the
/// mount prefix, so a handler inside the nest can't distinguish `/dev` from
/// `/dev/` — both arrive as `/`. `OriginalUri` still carries the
/// pre-strip path, and using it (rather than a fixed prefix) keeps the
/// redirect correct under Home Assistant Ingress, where the browser's real
/// path is `/api/hassio_ingress/<token>/dev`.
pub async fn require_dev(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(original) = request.extensions().get::<OriginalUri>() {
        let path = original.0.path();
        if !path.ends_with('/') && !path.contains("/api/") {
            return Redirect::permanent(&format!("{path}/")).into_response();
        }
    }

    match admit(&state, &request) {
        Ok(principal) => {
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(StatusCode::FORBIDDEN) => StatusCode::FORBIDDEN.into_response(),
        Err(_) => {
            let is_api = request.uri().path().contains("/api/");
            if is_api {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": "login required"})),
                )
                    .into_response()
            } else if state.dev_auth.credentials.is_some() {
                // Relative target: an absolute `/dev/login` would break under
                // Home Assistant Ingress, which rewrites path prefixes the
                // handler never sees.
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

    // `&`, not `&&`: both compares always run regardless of which fails,
    // so failure timing doesn't leak which field was wrong.
    let ok =
        crate::api::admin::constant_time_eq(form.username.as_bytes(), creds.username.as_bytes())
            & crate::api::admin::constant_time_eq(
                form.password.as_bytes(),
                creds.password.as_bytes(),
            );

    if !ok {
        tracing::warn!(username = %form.username, "dev UI login failed");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        return (
            StatusCode::UNAUTHORIZED,
            login_page("Wrong username or password."),
        )
            .into_response();
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

/// The two unprotected routes: `/login` (GET the form, POST to submit it)
/// and `/logout`. Neither is behind `require_dev`.
pub fn dev_auth_router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_form).post(login_submit))
        .route("/logout", post(logout))
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
        assert!(DevAuth::resolve(Some("  "), Some("p"))
            .credentials
            .is_none());
    }

    #[test]
    fn two_instances_have_different_session_keys() {
        assert_ne!(
            DevAuth::disabled().session_key,
            DevAuth::disabled().session_key
        );
    }

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

    #[test]
    fn adversarial_timestamp_is_rejected_not_panicking() {
        let a = DevAuth::with_credentials("u", "p");
        assert!(!a.verify_cookie_value(&format!("{}.deadbeef", i64::MIN), 0));
        assert!(!a.verify_cookie_value(&format!("{}.deadbeef", i64::MAX), 0));
    }
}
