//! `reload_config` must re-apply the Home Assistant add-on options overlay.
//!
//! Before this fix it reparsed `config.yaml` and swapped it in bare, so any
//! config write silently discarded what the add-on Options form had set.

mod common;

use common::TestApp;

/// `BYONK_OPTIONS_FILE` is process-wide. Cargo runs the tests in a binary on
/// parallel threads, so serialise every test that sets it — the same idiom
/// `tests/addon_options_test.rs` already uses for the dev-credential vars.
/// This must be a `tokio::sync::Mutex`, not `std::sync::Mutex`: the guard is
/// held across `.await` points below, and `clippy::await_holding_lock` (part
/// of this repo's `-D warnings` gate) rejects a std lock held that way.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn a_config_write_re_applies_the_addon_overlay() {
    let _guard = ENV_LOCK.lock().await;
    let dir = tempfile::tempdir().unwrap();

    // options.json carries a value that config.yaml does NOT.
    let options = dir.path().join("options.json");
    std::fs::write(
        &options,
        r#"{"admin_token":"secret","auth_mode":"ed25519","screen_repos":[]}"#,
    )
    .unwrap();
    std::env::set_var("BYONK_OPTIONS_FILE", &options);

    // The app starts WITHOUT the overlay (TestApp does not apply it), so the
    // live config here says `api_key` — whatever config.yaml says.
    let (app, config_path) = TestApp::new_admin_with_file("secret", dir.path());

    // Any admin write goes through `persist` -> `reload_config`.
    let r = app
        .post_json(
            "/api/admin/devices",
            &[("Authorization", "Bearer secret")],
            r#"{"key":"AA:BB:CC:DD:EE:FF","screen":"byonk-builtin/default"}"#,
        )
        .await;
    assert!(
        r.status.is_success(),
        "device write failed: {} {}",
        r.status,
        r.text()
    );

    // After the reload the overlay must be back on. `GET /api/admin/config`
    // is NOT a fit for this: `redacted_config` (src/api/admin/read.rs)
    // deliberately re-reads and re-parses the raw `config.yaml` bytes on
    // every call — by design, it shows the file that a save would write,
    // never the in-memory, overlay-folded `state.config`. So it would read
    // "api_key" here no matter what `reload_config` does, and can't tell
    // this bug apart from a fix.
    //
    // `/api/setup` reads live state instead: `handle_setup` is wired to
    // `state.config.load_full()` per request (src/server.rs, `handle_setup`
    // wrapper), which is exactly the `ArcSwap` `reload_config` stores into.
    // It is the real-world path this bug affects — TRMNL devices read
    // `auth_mode` from this same response.
    let setup = app
        .get_with_headers(
            "/api/setup",
            &[
                ("ID", "11:22:33:44:55:66"),
                ("FW-Version", "1.7.1"),
                ("Model", "og"),
            ],
        )
        .await;
    let body: serde_json::Value = setup.json();
    assert_eq!(
        body["auth_mode"], "ed25519",
        "reload_config dropped the add-on overlay; /api/setup body was {body}"
    );

    // The file itself is untouched by the overlay — it is an in-memory fold.
    let on_disk = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        !on_disk.contains("ed25519"),
        "the overlay must never be written back to config.yaml"
    );

    std::env::remove_var("BYONK_OPTIONS_FILE");
}
