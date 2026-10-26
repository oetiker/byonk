//! Route-level tests for the `require_dev` middleware, login and logout.
//!
//! These exercise `/dev/*`, which is only mounted starting with Task 4's
//! `build_router` changes. Until then every test here fails — that is the
//! expected, plan-mandated intermediate state (see task-3 dispatch).

mod common;
use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn no_credentials_page_is_404_and_api_is_401() {
    let app = TestApp::new();
    assert_eq!(app.get("/dev/").await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/dev/login").await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.get("/dev/api/whoami").await.status,
        StatusCode::UNAUTHORIZED
    );
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
    let who = app
        .get_with_headers("/dev/api/whoami", &[("Cookie", pair)])
        .await;
    assert_eq!(who.status, StatusCode::OK);
    let j: serde_json::Value = who.json();
    assert_eq!(j["user"], "u");
    assert_eq!(j["via"], "cookie");
    assert_eq!(j["can_logout"], true);
}

#[tokio::test]
async fn tampered_cookie_fails() {
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let r = app
        .get_with_headers("/dev/api/whoami", &[("Cookie", "byonk_dev=1.deadbeef")])
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_admin_token_passes_without_cookie() {
    let app = TestApp::new_admin("secret");
    let r = app
        .get_with_headers("/dev/api/whoami", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let j: serde_json::Value = r.json();
    assert_eq!(j["via"], "token");
    assert_eq!(j["can_logout"], false);
    let bad = app
        .get_with_headers("/dev/api/whoami", &[("Authorization", "Bearer nope")])
        .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ingress_trusts_supervisor_peer_only() {
    let app = TestApp::new();
    let ok = app
        .get_via_ingress(
            "/dev/api/whoami",
            "172.30.32.2:1234",
            &[("X-Remote-User-Name", "tobi")],
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    let j: serde_json::Value = ok.json();
    assert_eq!(j["user"], "tobi");
    assert_eq!(j["via"], "ingress");
    let bad = app
        .get_via_ingress(
            "/dev/api/whoami",
            "10.0.0.5:1234",
            &[("X-Remote-User-Name", "tobi")],
        )
        .await;
    assert_eq!(bad.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn remote_user_header_on_public_listener_is_ignored() {
    let app = TestApp::new();
    let r = app
        .get_with_headers("/dev/api/whoami", &[("X-Remote-User-Name", "tobi")])
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bare_dev_redirects_to_trailing_slash_relatively() {
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let r = app.get("/dev").await;
    assert_eq!(r.status, StatusCode::TEMPORARY_REDIRECT);
    // Asserted as a literal, relative string: an origin-absolute regression
    // (`/dev/`) would break Home Assistant Ingress, which strips the
    // ingress prefix before byonk ever sees the request (see require_dev's
    // doc comment).
    assert_eq!(r.headers["location"], "./dev/");
}

#[tokio::test]
async fn bare_dev_with_no_credentials_is_404_not_a_redirect() {
    // Pins finding 3: the redirect must not fire when the dev UI would
    // otherwise be invisible (404) — a bare `/dev` answering 307 while
    // `/dev/` answers 404 would let a probe infer the /dev tree exists on
    // a server where it's supposed to be undetectable.
    let app = TestApp::new();
    assert_eq!(app.get("/dev").await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deeper_paths_are_not_redirected() {
    // Only the nest root (`/dev`) gets the trailing-slash treatment.
    // `/dev/api/whoami` must keep its normal 401/200 behaviour, not a
    // redirect — and once Task 4 mounts static assets, the same must hold
    // for e.g. `/dev/dev.js`.
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let r = app.get("/dev/api/whoami").await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_clears_cookie() {
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let r = app.post_form("/dev/logout", "").await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert!(r.headers["set-cookie"]
        .to_str()
        .unwrap()
        .contains("Max-Age=0"));
}
