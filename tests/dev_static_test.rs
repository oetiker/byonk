//! The dev UI's static assets: that they are served behind `require_dev`,
//! and that every URL they construct is relative.

mod common;
use axum::http::StatusCode;
use common::TestApp;

const LOGIN_HTML: &str = include_str!("../static/dev/login.html");
const PREVIEW_HTML: &str = include_str!("../static/dev/preview.html");
// The hand-written `dev.js` is gone (Task 12); this now reads the built
// Svelte bundle it was replaced by, so the relative-URL guard below still
// covers every URL the dev UI's JavaScript constructs.
const DEV_JS: &str = include_str!("../webui/dist/app.js");

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
    for (name, body) in [("login.html", LOGIN_HTML), ("preview.html", PREVIEW_HTML)] {
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
            "app.js: `{opener}` builds an origin-absolute URL"
        );
        assert!(
            !PREVIEW_HTML.contains(opener),
            "preview.html: `{opener}` builds an origin-absolute URL"
        );
    }
}

/// The SPA is served under three fixed names, and the routes the vanilla UI
/// needed are gone. `dev.js`, `dev.css` and `editor.bundle.js` were the
/// hand-written UI; serving them after the rewrite would mean a stale file
/// was left embedded.
#[tokio::test]
async fn serves_the_spa_and_not_the_retired_routes() {
    let app = TestApp::new_admin("secret");
    let auth = [("Authorization", "Bearer secret")];

    for (path, needle) in [
        ("/dev/", "<div id=\"app\">"),
        // The bundle's real mount call, not incidental filler -- proves the
        // built Svelte app (not an empty or error body) came back.
        ("/dev/app.js", "getElementById(`app`)"),
        // Tailwind's preflight reset, present in every build that pulls in
        // Tailwind -- proves a real stylesheet (not an empty 200) came back.
        ("/dev/app.css", "box-sizing:border-box"),
    ] {
        let res = app.get_with_headers(path, &auth).await;
        assert_eq!(res.status, StatusCode::OK, "{path} must be served");
        assert!(
            res.text().contains(needle),
            "{path}: body did not contain {needle:?}"
        );
    }

    for path in ["/dev/dev.js", "/dev/dev.css", "/dev/editor.bundle.js"] {
        assert_eq!(
            app.get_with_headers(path, &auth).await.status,
            StatusCode::NOT_FOUND,
            "{path} belonged to the deleted UI"
        );
    }
}
