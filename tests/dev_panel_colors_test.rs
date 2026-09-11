//! The dev colour override is keyed by PANEL, so every device using that
//! panel profile sees it — which is the point: a calibration belongs to the
//! hardware, not to one unit of it.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

/// Scaffold a `local/swatch` screen: a single solid-grey rect covering the
/// whole frame, wrapped in `data-byonk-tone="continuous"`.
///
/// Why a bespoke screen rather than a builtin calibration one: `/api/display`
/// always writes the OFFICIAL palette into PLTE (`src/api/display.rs`,
/// `render_png_from_svg(.., false, ..)` — "production always uses official
/// colors"), so a measured-colour difference can only show up through
/// *dithering* — which palette index a pixel gets, not what RGB that index
/// draws. And dithering only consults measured colours for content marked
/// `data-byonk-tone="continuous"` (`eink-dither`'s `RegionMap::model`,
/// ruling 22): unmarked content is matched against the OFFICIAL colours
/// regardless of what's measured, by design (an author who never opts in to
/// tone-aware rendering gets nominal colours). The builtin
/// `calibration/grey` screen this file's tests originally reached for is
/// entirely unmarked structure — swatches, a step wedge, a resolution
/// pattern, all drawn as flat authored colours — so a dev colour override
/// has provably zero effect on it; an earlier version of this test used it
/// and the override assertions failed even with the code correct, which is
/// how this was found. A screen needs an actual `data-byonk-tone="continuous"`
/// region for a measured-colour override to be observable through
/// `/api/display` at all.
fn scaffold_swatch_screen(dir: &std::path::Path) {
    let repo_dir = dir.join("local");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("byonk-screens.yaml"),
        "name: local\ndescription: Test fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();

    let screen_dir = repo_dir.join("swatch");
    std::fs::create_dir_all(&screen_dir).unwrap();
    std::fs::write(
        screen_dir.join("meta.yaml"),
        format!(
            "title: Swatch\ndescription: Solid continuous-tone swatch, for tests.\nbyonk: \"{}\"\nrefresh: 3600\n",
            byonk::models::compat::engine_compat_req()
        ),
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("script.lua"),
        "return { data = { width = layout.width, height = layout.height }, refresh_rate = 3600 }\n",
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("screen.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {{ data.width }} {{ data.height }}" width="{{ data.width }}" height="{{ data.height }}">
  <g data-byonk-tone="continuous">
    <rect x="0" y="0" width="{{ data.width }}" height="{{ data.height }}" fill="#808080"/>
  </g>
</svg>
"##,
    )
    .unwrap();
}

/// One panel (`e1004`) resolved by all three routes `resolve_panel` supports
/// — `kitchen`/`hallway` by device config, `office` by the `Board` header
/// (via `e1004`'s `match:`), `attic` by the `Model` header fallback — plus a
/// second panel (`other`) and a device on it (`garage`), to prove an
/// override never leaks across panels. Every device renders the
/// continuous-tone `local/swatch` screen (see `scaffold_swatch_screen`) so a
/// measured-colour override is actually observable through `/api/display`.
fn app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    scaffold_swatch_screen(dir);
    let yaml = format!(
        "\
admin:
  token: secret
screen_repos:
  local:
    path: {}
panels:
  e1004:
    name: reTerminal E1004
    match: e1004
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
    colors_actual: '#101010,#E8E8E8,#B03030,#D0C040,#3040A0,#00994D'
  other:
    name: Some Other Panel
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
    colors_actual: '#0A0A0A,#F0F0F0,#902020,#A0A020,#202090,#106020'
devices:
  kitchen:
    screen: local/swatch
    panel: e1004
  hallway:
    screen: local/swatch
    panel: e1004
  office:
    screen: local/swatch
  attic:
    screen: local/swatch
  garage:
    screen: local/swatch
    panel: other
",
        dir.join("local").display()
    );
    TestApp::new_with_config_yaml(&yaml, dir)
}

#[tokio::test]
async fn put_and_delete_an_override_by_panel_id() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let r = app
        .put_json(
            "/dev/api/panel-colors/e1004",
            &[AUTH],
            r##"{"colors_actual":"#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00994D"}"##,
        )
        .await;
    assert!(r.status.is_success(), "PUT failed: {}", r.status);

    let r = app.delete("/dev/api/panel-colors/e1004", &[AUTH]).await;
    assert!(r.status.is_success(), "DELETE failed: {}", r.status);
}

#[tokio::test]
async fn an_unknown_panel_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let r = app
        .put_json(
            "/dev/api/panel-colors/nosuchpanel",
            &[AUTH],
            r##"{"colors_actual":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert_eq!(
        r.status,
        axum::http::StatusCode::NOT_FOUND,
        "an override for a panel that does not exist can never be read back, \
         so it must be refused rather than stored where nothing will find it"
    );
}

/// Build `/api/display` headers for one device. `model` and `board` let a
/// test drive the two header-based `resolve_panel` fallbacks a device with
/// no `panel:` in its config relies on; `board: None` means no `Board`
/// header is sent at all (not merely an empty one).
fn display_headers(
    device_id: &str,
    api_key: &str,
    model: &str,
    board: Option<&str>,
) -> Vec<(String, String)> {
    let mut headers = vec![
        ("ID".to_string(), device_id.to_string()),
        ("Access-Token".to_string(), api_key.to_string()),
        ("Width".to_string(), "800".to_string()),
        ("Height".to_string(), "480".to_string()),
        ("Battery-Voltage".to_string(), "4.12".to_string()),
        ("RSSI".to_string(), "-67".to_string()),
        ("FW-Version".to_string(), "1.7.1".to_string()),
        ("Model".to_string(), model.to_string()),
        ("Host".to_string(), "localhost:3000".to_string()),
    ];
    if let Some(board) = board {
        headers.push(("Board".to_string(), board.to_string()));
    }
    headers
}

/// Fetch `/api/display`, follow its `image_url`, and return the rendered
/// PNG bytes.
async fn fetch_png(app: &TestApp, headers: &[(String, String)]) -> Vec<u8> {
    let pairs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let display_response = app.get_with_headers("/api/display", &pairs).await;
    let image_url = common::assert_valid_display_response(&display_response);
    let path = image_url
        .split("localhost:3000")
        .nth(1)
        .expect("display response should carry a path after the host");
    let image_response = app.get(path).await;
    common::assert_png(&image_response);
    image_response.body
}

/// Spec §10: "a device inherits its panel's override — set for panel P,
/// request `/api/display` as a device that resolves to P by device config,
/// by `Board` header, and by `Model` header; a device that resolves to a
/// different panel is unaffected."
///
/// This is the test that actually pins the re-keying: `put_and_delete_an_override_by_panel_id`
/// only proves the PUT/DELETE round-trip accepts and clears a string under
/// the path's panel id — it never proves that string is what `/api/display`
/// reads back, so it can't tell a panel-keyed override from a device-keyed
/// one. This test can: reverting `src/api/display.rs`'s override lookup
/// from `panel_key` back to `device_entry_key` makes it fail, because none
/// of `kitchen`/`hallway`/`office`/`attic` is a device literally named
/// `e1004` (confirmed by mutation testing — see the fix report).
#[tokio::test]
async fn override_propagates_through_the_panel_and_only_the_panel() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let kitchen_key = app.register_device("kitchen").await;
    let hallway_key = app.register_device("hallway").await;
    let office_key = app.register_device("office").await;
    let attic_key = app.register_device("attic").await;
    let garage_key = app.register_device("garage").await;

    // kitchen/hallway: resolved via `panel: e1004` in device config.
    let kitchen_headers = display_headers("kitchen", &kitchen_key, "og", None);
    let hallway_headers = display_headers("hallway", &hallway_key, "og", None);
    // office: no `panel:` in config; resolved via the `Board` header
    // matching e1004's `match: e1004`.
    let office_headers = display_headers("office", &office_key, "og", Some("e1004"));
    // attic: no `panel:` in config, no `Board` header sent; resolved via the
    // `Model` header fallback matching the same `match: e1004`.
    let attic_headers = display_headers("attic", &attic_key, "e1004", None);
    // garage: resolved via `panel: other` — a different panel entirely.
    let garage_headers = display_headers("garage", &garage_key, "og", None);

    let kitchen_before = fetch_png(&app, &kitchen_headers).await;
    let hallway_before = fetch_png(&app, &hallway_headers).await;
    let office_before = fetch_png(&app, &office_headers).await;
    let attic_before = fetch_png(&app, &attic_headers).await;
    let garage_before = fetch_png(&app, &garage_headers).await;

    let r = app
        .put_json(
            "/dev/api/panel-colors/e1004",
            &[AUTH],
            r##"{"colors_actual":"#050505,#FAFAFA,#E01010,#101010,#1010E0,#10E010"}"##,
        )
        .await;
    assert!(r.status.is_success(), "PUT failed: {}", r.status);

    let kitchen_after = fetch_png(&app, &kitchen_headers).await;
    let hallway_after = fetch_png(&app, &hallway_headers).await;
    let office_after = fetch_png(&app, &office_headers).await;
    let attic_after = fetch_png(&app, &attic_headers).await;
    let garage_after = fetch_png(&app, &garage_headers).await;

    assert_ne!(
        kitchen_before, kitchen_after,
        "device-config route: setting the panel override had no visible effect"
    );
    assert_ne!(
        hallway_before, hallway_after,
        "a second device sharing the panel via device config did not see the override"
    );
    assert_ne!(
        office_before, office_after,
        "Board-header route: setting the panel override had no visible effect"
    );
    assert_ne!(
        attic_before, attic_after,
        "Model-header route: setting the panel override had no visible effect"
    );

    // Same override, same panel, same screen and dimensions -> every
    // affected device renders byte-identically to every other.
    assert_eq!(
        kitchen_after, hallway_after,
        "two devices sharing a panel via device config should render identically once overridden"
    );
    assert_eq!(
        kitchen_after, office_after,
        "the Board-header device should render identically to the device-config devices"
    );
    assert_eq!(
        kitchen_after, attic_after,
        "the Model-header device should render identically to the device-config devices"
    );

    assert_eq!(
        garage_before, garage_after,
        "a device resolving to a DIFFERENT panel must not see e1004's override"
    );

    let r = app.delete("/dev/api/panel-colors/e1004", &[AUTH]).await;
    assert!(r.status.is_success(), "DELETE failed: {}", r.status);

    let kitchen_restored = fetch_png(&app, &kitchen_headers).await;
    assert_eq!(
        kitchen_before, kitchen_restored,
        "deleting the override should restore the panel's own colors_actual"
    );
}
