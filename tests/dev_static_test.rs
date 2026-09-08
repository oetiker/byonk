//! The dev UI's static assets: that they are served behind `require_dev`,
//! and that every URL they construct is relative.

mod common;
use axum::http::StatusCode;
use common::TestApp;

const INDEX_HTML: &str = include_str!("../static/dev/index.html");
const LOGIN_HTML: &str = include_str!("../static/dev/login.html");
const PREVIEW_HTML: &str = include_str!("../static/dev/preview.html");
const DEV_JS: &str = include_str!("../static/dev/dev.js");

#[tokio::test]
async fn static_assets_are_served_to_an_authenticated_caller() {
    let app = TestApp::new_admin("secret");
    let auth = [("Authorization", "Bearer secret")];

    let lens = app.get_with_headers("/dev/lens.js", &auth).await;
    assert_eq!(lens.status, StatusCode::OK);
    assert_eq!(lens.headers["content-type"], "application/javascript");
    assert!(lens.text().contains("function setupLens("));

    let preview = app.get_with_headers("/dev/preview.html", &auth).await;
    assert_eq!(preview.status, StatusCode::OK);
    assert!(preview.text().contains("byonk-preview"));
}

#[tokio::test]
async fn static_assets_are_behind_require_dev() {
    // No dev credentials configured, so an unauthenticated request must not
    // even reveal that the dev tree exists.
    let app = TestApp::new_admin("secret");
    assert_eq!(app.get("/dev/lens.js").await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.get("/dev/preview.html").await.status,
        StatusCode::NOT_FOUND
    );

    // With credentials configured the same requests go to the login page, and
    // the redirect target has to stay relative for the same reason the URLs
    // inside these files do.
    let app = TestApp::new_with_dev_login(None, "u", "p");
    let r = app.get("/dev/preview.html").await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.headers["location"], "login");
}

/// Every URL the dev UI builds must be relative. Under Home Assistant Ingress
/// the Supervisor strips the path prefix before byonk sees the request, so the
/// server cannot know it and cannot write it into a page; an origin-absolute
/// URL would send the user out of the ingress iframe onto Home Assistant's own
/// frontend.
#[test]
fn dev_pages_and_scripts_use_relative_urls() {
    for (name, body) in [
        ("index.html", INDEX_HTML),
        ("login.html", LOGIN_HTML),
        ("preview.html", PREVIEW_HTML),
    ] {
        for attr in ["href=\"", "src=\"", "action=\""] {
            for (at, _) in body.match_indices(attr) {
                let rest = &body[at + attr.len()..];
                let end = rest.find('"').expect("unterminated attribute value");
                let value = &rest[..end];
                assert!(
                    !value.starts_with('/') && !value.contains("://"),
                    "{name}: {attr}{value}\" must be a relative URL"
                );
            }
        }
    }

    // The same rule for the URLs JavaScript builds, checked at the few call
    // shapes that can carry one.
    for opener in [
        "fetch(\"/",
        "fetch(`/",
        "EventSource(\"/",
        "window.open(\"/",
        "location.href = \"/",
    ] {
        assert!(
            !DEV_JS.contains(opener),
            "dev.js: `{opener}` builds an origin-absolute URL"
        );
        assert!(
            !PREVIEW_HTML.contains(opener),
            "preview.html: `{opener}` builds an origin-absolute URL"
        );
    }
}
