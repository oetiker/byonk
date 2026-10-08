# Dev UI Work Area — Server (Plan 1 of 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the dev UI's three-mode work area every server capability it needs: a complete draft wire format, panel- and device-keyed cascade overrides with a TTL and a listing, whole-rung panel Save, a session-auth device Save, thicker list payloads, and a server-side "which devices use this panel" answer.

**Architecture:** All new routes live in `src/api/dev.rs`'s `dev_router` behind `require_dev`. The three writer-less/one-string override maps in `DevOverrides` (`src/server.rs`) are replaced by two `TimedMap<CascadeValues>` maps (panel-keyed, device-keyed) with a lazy 30-minute TTL; `/api/display` and the dev render read both, device over panel, as the **override** rung. Device Save reuses `apply_device_patch` unchanged. A remembered Board/Model per configured device (in memory) lets the server answer device→panel questions the client cannot.

**Tech Stack:** Rust (axum, serde, serde_yaml 0.9.34, tokio RwLock), mdBook.

**Spec:** `docs/superpowers/specs/2026-10-04-dev-ui-work-area-design.md` §8 items 2–7 (item 1 is already done server-side), plus `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md` §2, §4.4–§4.7. Read the controller brief `.superpowers/sdd/dev-ui-planning-brief.md` (rulings R-A..R-I) first. Plan 2 (`docs/superpowers/plans/2026-10-08-dev-ui-work-area.md`) consumes this plan's routes and payloads exactly as written in the Interfaces blocks below.

## Rulings this plan makes (read before Task 1)

Controller rulings (binding, from the brief) — where they differ from the spec, the ruling wins:

- **R-A** Two plans, one PR. This plan runs first; Plan 2 cites its routes verbatim.
- **R-B (overrides spec §8.2 / cascade §4.6 "the draft feeds the override rung")**: a draft rung **replaces** the configured rung it edits; `override` stays the override rung. This plan adds `draft.final` and lifts the `draft.base.colors_actual` refusal. `override.colors` and `override.refresh` stay refused.
- **R-C** Panel-keyed `CascadeValues` override map replaces `panel_colors`; a device-keyed one replaces the writer-less `dither`/`tuning` maps; both get the tuning-session TTL (30 min, touched by re-PUT) and a GET listing.
- **R-D** Panel Save writes the whole panel rung in one request, through `config_writer` + `config_persist::persist` under `write_lock`.
- **R-E** Device Save is a dev-router route calling `apply_device_patch` — no new write logic.
- **R-F** `/api/display` remembers each configured device's last Board header (here: and Model header, see ruling P1-5); a panel-usage endpoint answers per panel which device keys resolve to it, plus the devices never seen since start.
- **R-G** `PanelInfo` gains the `dither` block; `DeviceInfo` gains base + final rungs, `name`, `params`, `temperature_profile`, `maximum_compatibility`, `min_png_bytes`.
- **R-I** No CHANGES.md entry. `docs/src/guide/dev-mode.md` is updated where it describes the render body, overrides, and the endpoints.

Rulings this plan makes where brief and spec are silent:

- **P1-1 Route names.** Overrides: `GET /dev/api/overrides`, `PUT|DELETE /dev/api/overrides/panels/{panel}`, `PUT|DELETE /dev/api/overrides/devices/{device}`. Saves: `POST /dev/api/panels/{panel}/save`, `PATCH /dev/api/devices/{key}`. Usage: `GET /dev/api/panel-usage`. The old `/dev/api/panel-colors/...` routes are **removed** (Tasks 2 and 4). Between this plan and Plan 2's Task 4 the shipped Tune panel's Push/Save answer 404 at runtime; the webui suite still passes because it mocks `api.js`. Same PR, so nothing ships broken.
- **Ruling (controller) C1 — overrides may carry `dither`.** Spec §6/§8.3 push "the whole draft rung … for every panel-rung field", and the dev render refuses only `override.colors` and `override.refresh`. A panel override and a device override both accept `colors_actual`, `dither`, `max_error`, `noise_scale`, `chroma_clamp`, `strength`, `gamut`; both refuse `colors` and `refresh` (400). `/api/display` and the dev render already read `dither` from the override rung (pre-script and post-script), so only the route validation changes.
- **P1-3 Device beats panel on the override rung.** When both a device and its panel carry a live override, the override rung is the device's values field-by-field over the panel's. The device override is the narrower statement.
- **P1-4 The dev render reads live overrides.** Its override rung is `request.override` over the live device override over the live panel override. Otherwise the ladder (spec §5) shows the config winner while a Push is live — the exact lie the override rung exists to prevent. Reading is not writing; `dev_render_is_read_only_test.rs` stays green.
- **P1-5 Remember Model too.** `resolve_panel` falls back to the `Model` header (`display.rs:362-371`), so a Board-only memory would miscount the E1004-style devices. The sighting stores both.
- **P1-6 The dev render uses the sighting.** With a device and no explicit `panel`, the dev render resolves the panel the way `/api/display` would (explicit → remembered Board → remembered Model). Without it, mode c previews a Board-resolved device with no panel at all.
- **P1-7 `draft.params`.** `RenderDraft` gains `params`, replacing the device's configured params (simulated params still merge on top). Spec §4 mode c ("a draft change re-renders the preview at once") needs a way to preview a removed param; merging alone cannot.
- **Ruling (controller) C2 — panel Save never rebuilds the panel block.** `config.yaml` is the operator's hand-written document. Save writes `colors_actual` and the `dither` block **in place, key by key**: it creates `dither:` (and `gamut:` / an algorithm sub-map) under the panel when missing, sets each changed flat key, and removes each key the rung no longer has. Untouched lines — comments inside the panel, sibling panels, the order of `panels:` — are not rewritten. `yamlpatch`'s `Op::Add` mis-indents nested maps (`config_writer.rs:222-226`), so nested mappings are inserted by a new `config_writer` helper that writes the text itself at the child indentation the parent already uses. `upsert_panel` is not added.
- **Ruling (controller) C3 — device Save is one atomic PATCH.** `apply_device_patch` accepts a `DeviceWrite` that both clears and sets: **clears apply to the existing entry first, then the sets merge over the result**, under the one write lock and one `persist`. A key both cleared and set in one request therefore ends up **set** (the set wins): `clear: ["final"]` + `final: {dither}` replaces the whole final rung with `{dither}`; `clear: ["params"]` + `params: {…}` replaces params wholesale. `clear` also accepts single gamut sub-fields: `gamut.knee`, `gamut.amount`, `gamut.max_compression` and `final.gamut.<sub>`. The unit test `a_final_field_cannot_be_set_and_cleared_in_one_request` (`write.rs:1424-1434`) is replaced by `a_final_field_set_and_cleared_in_one_request_ends_up_set`, because the refusal it pins is exactly what C3 removes; the MCP doc comment at `src/mcp/tools_device.rs:90` ("A field cannot be set and cleared in one request") is reworded. Every other admin-PATCH and MCP behaviour is unchanged.
- **P1-8 (follows from C2) Float tidying.** `serde_yaml` turns an `f32` into an `f64` with representation noise (`0.8` → `0.800000011920929`). The panel `dither` block is serialised through `tidy(f32) -> f64` (shortest decimal of the `f32`), so a saved `0.8` reads `0.8` in the file and compares equal on the next Save, which then leaves that line untouched.
- **P1-9 Saving clears the matching override** (panel Save → panel override, device Save → device override), as `handle_save_panel_colors` does today.
- **P1-10 `DeviceInfo.dither` is removed**; it is `base.dither` now.

## Global Constraints

- Every task's requirements include this section and the rulings above.
- **Verification order (never `make check` alone):** `make webui` first (RustEmbed/`include_str!` bake `webui/dist` in at compile time), then `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `(cd docs && mdbook build)`, then `make webui-test`. The Rust suite and `make webui-test` run **one after the other, never in parallel**.
- `test_display_content_hash_is_deterministic` is a load-sensitive flake that aborts its binary; re-run once before believing a red suite.
- **Never `git add -A` or `git add .`.** Never stage `config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`. Check `git diff --cached --stat` before every commit.
- Hex colours in YAML inside Rust raw strings are **single-quoted** (`'#000000'`) — `"#` terminates `r#"…"#`. Use `r##"…"##` when a JSON body contains `"#`.
- `PanelDitherConfig` has a hand-written `Deserialize`: scalar keys → defaults, **any other key → an algorithm name**. Panel `dither:` blocks in fixtures are flat; never nest `defaults:`/`algorithms:`.
- `tests/dev_auth_test.rs` stays byte-identical to `5d6323c`.
- Every URL the browser sees is relative; no nested page route under `/dev`; the webui build emits exactly `index.html`, `app.js`, `app.css`.
- Comments and identifiers in English. No CHANGES.md entry (R-I).
- `src/api/dev.rs` is the single place dev routes are registered (`dev_router`, `dev.rs:870-889` at HEAD).

## Review Focus

1. **A device override that touches the same field as its panel's override.** The device must win that field and the panel must still supply the others; swapping the `or` operands passes every single-map test. [Task 3, `device_override_beats_panel_override_field_by_field`]
2. **An override older than 30 minutes still steering `/api/display`.** Lazy expiry that forgets to remove (or compares the wrong way) is invisible until a panel paints a draft overnight. [Task 2, `TimedMap` unit tests]
3. **A panel Save that creates a `dither:` block under a panel whose block has comments, on a config with sibling panels.** The new nested block must land at the right indentation, every comment must survive and `panels:` must keep its order. [Task 4, `a_dither_save_keeps_comments_and_panel_order`]
4. **A Board-resolved device in the dev render.** Without P1-6 the panel rung is empty and the ladder says `fallback`. [Task 7, `the_dev_render_resolves_a_board_matched_device_after_it_polled`]
5. **Device Save on an embedded (read-only) config.** Must answer 409 like the admin route, not 500. [Task 5, `saving_on_an_embedded_config_is_a_conflict`]

## File Structure

**Modified**
- `src/api/dev.rs` — `RenderDraft` (`final`, `params`), refusals, override routes and listing, panel/device save routes, `PanelInfo`/`DeviceInfo`, panel-usage route, dev render's override + panel resolution.
- `src/server.rs` — `TimedMap<V>`, `OVERRIDE_TTL`, `Sighting`, new `DevOverrides` shape, `DevOverrides::live_override`.
- `src/api/display.rs` — reads `live_override` as the override rung; `render_inputs_for` takes one `&CascadeValues`; records sightings; `resolve_device_panel`.
- `src/models/config.rs` — `CascadeValues::or`, `PanelDitherConfig: Serialize + PartialEq + is_empty`.
- `src/services/config_writer.rs` — `path_range` (generalised `block_range`), `remove_path`, `set_path`, `sync_mapping` (in-place, comment-preserving nested writes, ruling C2).
- `src/mcp/tools_device.rs` — one doc comment (ruling C3).
- `src/api/admin/write.rs` — `merge_final` uses `CascadeValues::or`; `apply_device_patch` clears-then-sets, `resolve_clear` drops the set+clear refusal and accepts gamut sub-fields (ruling C3).
- `docs/src/guide/dev-mode.md` — endpoints table, "Rendering a preview", new "Overrides" subsection.

**Renamed + rewritten tests**
- `tests/dev_panel_colors_test.rs` → `tests/dev_overrides_test.rs`
- `tests/dev_panel_colors_save_test.rs` → `tests/dev_panel_save_test.rs`

**New tests**
- `tests/dev_device_save_test.rs`, `tests/dev_screens_payload_test.rs`, `tests/dev_panel_usage_test.rs`

**Modified tests**
- `tests/dev_render_provenance_test.rs`

---

### Task 1: The draft wire format: `draft.final`, `draft.params`, device `colors_actual`

**Files:**
- Modify: `src/api/dev.rs` — `RenderDraft` (`dev.rs:62-69`), `refused_until_cascade` (`dev.rs:105-122`), `handle_render` (`dev.rs:539-` the rung assembly at `:582-591` and params at `:617-621`)
- Test: `tests/dev_render_provenance_test.rs`

**Interfaces:**
- Consumes: `CascadeValues` (`src/models/config.rs:111`), `DeviceConfig::device_final()`.
- Produces (wire, consumed by Plan 2 Task 5):
  ```json
  "draft": {
    "base":  { CascadeValues },          // replaces devices.<k>.* base rung
    "final": { CascadeValues },          // replaces devices.<k>.final
    "panel": { "colors": "...", "colors_actual": "...", "dither": { flat PanelDitherConfig } },
    "params": { "k": <json> }            // replaces the device's params; simulated.params merge on top
  }
  ```
  Still refused with 400: `override.colors`, `override.refresh`. Unknown keys: 422.

- [ ] **Step 1: Write the failing tests**

In `tests/dev_render_provenance_test.rs`, extend `app()`'s fixture: add a second device and a params-sensitive screen. Replace the `devices:` block of the YAML in `app()` with:

```yaml
devices:
  kitchen:
    screen: local/gradient
    panel: e1004
    dither: floyd-steinberg
    refresh: 600
    params:
      greeting: hi
  porch:
    screen: local/plain
    final:
      strength: 0.4
```

Add a scaffold function and call it from `app()` next to the other two:

```rust
/// `local/needs`: fails unless `params.greeting == "hi"`, naming what it got,
/// so a test can see exactly which params reached the script.
fn scaffold_needs_screen(dir: &std::path::Path) {
    let screen_dir = dir.join("local").join("needs");
    std::fs::create_dir_all(&screen_dir).unwrap();
    std::fs::write(
        screen_dir.join("meta.yaml"),
        format!(
            "title: Needs\ndescription: Params probe, for tests.\nbyonk: \"{}\"\nrefresh: 900\n",
            byonk::models::compat::engine_compat_req()
        ),
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("script.lua"),
        "if params.greeting ~= \"hi\" then error(\"greeting was \" .. tostring(params.greeting)) end\nreturn { data = { width = layout.width, height = layout.height } }\n",
    )
    .unwrap();
    std::fs::write(
        screen_dir.join("screen.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {{ data.width }} {{ data.height }}" width="{{ data.width }}" height="{{ data.height }}">
  <rect x="0" y="0" width="{{ data.width }}" height="{{ data.height }}" fill="#FFFFFF"/>
</svg>
"##,
    )
    .unwrap();
}
```

Replace `fields_the_config_cannot_hold_yet_are_refused` with:

```rust
#[tokio::test]
async fn fields_the_cascade_cannot_carry_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    for body in [
        r##"{"screen":"local/plain","override":{"colors":"#000000,#FFFFFF"}}"##,
        r#"{"screen":"local/plain","override":{"refresh":60}}"#,
        r#"{"screen":"local/plain","dither":"atkinson"}"#,
        r#"{"screen":"local/plain","draft":{"nope":{}}}"#,
    ] {
        let r = app.post_json("/dev/api/render", &[AUTH], body).await;
        assert!(r.status.is_client_error(), "{body} → {}", r.status);
        assert_ne!(
            r.status,
            axum::http::StatusCode::METHOD_NOT_ALLOWED,
            "{body}"
        );
    }
}

#[tokio::test]
async fn a_draft_final_replaces_the_configured_final_rung() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    // porch has final.strength 0.4. A draft final without strength removes it,
    // and its own dither vetoes the screen's atkinson.
    let r = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"device":"porch","draft":{"final":{"dither":"stucki"}}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res = resolution(&r);
    assert_eq!(res["dither"]["effective"], "stucki");
    assert_eq!(res["dither"]["from"], "device_final");
    assert!(
        res["strength"]["rungs"].get("device_final").is_none(),
        "the draft final rung replaced the configured one, strength included"
    );
}

#[tokio::test]
async fn without_a_draft_final_the_configured_final_rung_stays() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"porch"}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert_eq!(
        resolution(&r)["strength"]["rungs"]["device_final"]["value"],
        0.4
    );
}

#[tokio::test]
async fn a_draft_base_may_carry_colors_actual() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r##"{"device":"kitchen","draft":{"base":{"colors_actual":"#111111,#EEEEEE,#AA2222,#CCCC33,#2222AA,#22AA22"}}}"##,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res = resolution(&r);
    assert_eq!(res["colors_actual"]["from"], "device_base");
    assert_eq!(
        res["colors_actual"]["rungs"]["panel"]["status"],
        "outranked"
    );
}

#[tokio::test]
async fn draft_params_replace_the_device_params_and_simulated_ones_merge_on_top() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    // Configured params (greeting: hi) satisfy the probe.
    let ok = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"screen":"local/needs","device":"kitchen"}"#,
        )
        .await;
    assert!(ok.status.is_success(), "{} {}", ok.status, ok.text());
    // An empty draft removes them: the script sees nil.
    let gone = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"screen":"local/needs","device":"kitchen","draft":{"params":{}}}"#,
        )
        .await;
    assert_eq!(gone.status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    assert!(gone.text().contains("greeting was nil"), "{}", gone.text());
    // Simulated params still land on top of the draft.
    let back = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"screen":"local/needs","device":"kitchen","draft":{"params":{}},"simulated":{"params":{"greeting":"hi"}}}"#,
        )
        .await;
    assert!(back.status.is_success(), "{} {}", back.status, back.text());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test dev_render_provenance_test`
Expected: `a_draft_final_replaces_the_configured_final_rung`, `a_draft_base_may_carry_colors_actual` and `draft_params_replace_…` FAIL (422 / 400 / success where 500 is expected).

- [ ] **Step 3: Implement**

In `src/api/dev.rs` replace `RenderDraft`:

```rust
/// An uncommitted edit. Each present rung REPLACES the configured one, so a
/// preview can show the effect of removing a value (controller ruling R-B).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderDraft {
    pub base: Option<CascadeValues>,
    /// The device's `final:` rung, drafted.
    #[serde(rename = "final")]
    pub final_rung: Option<CascadeValues>,
    pub panel: Option<PanelDraft>,
    /// Replaces the device's configured params; `simulated.params` still
    /// merge on top (plan ruling P1-7).
    pub params: Option<HashMap<String, serde_json::Value>>,
}
```

Replace `refused_until_cascade` with:

```rust
/// Override fields the cascade cannot carry on this path (controller ruling
/// R-B): the script would be shown one palette and rendered with another,
/// and nothing here consumes a refresh.
fn refused_fields(req: &RenderRequest) -> Option<&'static str> {
    if req.over.colors.is_some() {
        return Some("override.colors is not supported: the script could not see it");
    }
    if req.over.refresh.is_some() {
        return Some("override.refresh is not supported");
    }
    None
}
```

and change the call at the top of `handle_render` to `if let Some(refusal) = refused_fields(&req) {`.

In `handle_render` replace the `device_final_rung` binding (`dev.rs:585-588`) with:

```rust
    // The device's `final:` block, unless the draft replaces it.
    let device_final_rung: CascadeValues = req
        .draft
        .final_rung
        .clone()
        .unwrap_or_else(|| device.map(|d| d.device_final()).unwrap_or_default());
```

and the params block (`dev.rs:617-621`) with:

```rust
    // The device's params (or the draft's, which replace them), with the
    // simulated ones on top.
    let mut custom_params: HashMap<String, serde_json::Value> = match &req.draft.params {
        Some(p) => p.clone(),
        None => device
            .map(|d| yaml_params_to_json(&d.params))
            .unwrap_or_default(),
    };
    custom_params.extend(req.simulated.params.clone());
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --test dev_render_provenance_test`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/api/dev.rs tests/dev_render_provenance_test.rs
git commit -m "feat(dev): draft.final, draft.params and a device colors_actual in the dev render"
```

---

### Task 2: Timed panel overrides, read as the override rung everywhere

**Files:**
- Modify: `src/server.rs` — `DevOverrides` (`:32-53`), new `TimedMap`, `OVERRIDE_TTL`, `live_override`; tests module at `:615-651`
- Modify: `src/models/config.rs` — `impl CascadeValues` (new, after the struct at `:111-122`)
- Modify: `src/api/admin/write.rs` — `merge_final` (`:597-617`)
- Modify: `src/api/display.rs` — `render_inputs_for` (`:75-118`), the override reads in `handle_display` (`:722-773`), the closure captures (`:851-858`), the call (`:877-889`), the unit tests calling `render_inputs_for` (`:1524-1675`)
- Modify: `src/api/dev.rs` — routes, handlers, `handle_render` override rung
- Rename + modify: `tests/dev_panel_colors_test.rs` → `tests/dev_overrides_test.rs`

**Interfaces:**
- Produces:
  - `pub const OVERRIDE_TTL: Duration` (= `TUNING_SESSION_TTL`, 30 min).
  - `pub struct TimedMap<V>` with `insert(key, v)`, `remove(&key) -> bool`, `get(&key) -> Option<V>`, `live() -> Vec<(String, V, Duration)>` (sorted by key; `Duration` = time left).
  - `DevOverrides { panels: TimedMap<CascadeValues>, devices: TimedMap<CascadeValues>, tuning_sessions: … }`.
  - `DevOverrides::live_override(&self, device_key: Option<&str>, panel_key: Option<&str>) -> CascadeValues` (never carries `colors`/`refresh`).
  - `CascadeValues::or(&self, other: &CascadeValues) -> CascadeValues` (self wins, field by field).
  - Routes: `PUT /dev/api/overrides/panels/{panel}` body `CascadeValues` → 200 empty; 404 unknown panel; 400 with `colors` or `refresh` (ruling C1: `dither` is accepted).
  - `fn refused_override_field(v: &CascadeValues) -> Option<&'static str>` — shared by the panel and device PUT routes (Task 3). `DELETE /dev/api/overrides/panels/{panel}` → 200 empty.

- [ ] **Step 1: Rename the test file and point it at the new route**

```bash
git mv tests/dev_panel_colors_test.rs tests/dev_overrides_test.rs
```

In `tests/dev_overrides_test.rs` replace every `/dev/api/panel-colors/` with `/dev/api/overrides/panels/` (bodies stay: `{"colors_actual": …}` is a valid `CascadeValues`). Update the module doc to: `//! Dev overrides: panel-keyed and device-keyed cascade values that /api/display reads as the override rung.` Then append:

```rust
#[tokio::test]
async fn a_panel_override_carries_tuning_numbers_to_every_device_on_the_panel() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());
    let kitchen_key = app.register_device("kitchen").await;
    let garage_key = app.register_device("garage").await;
    let kitchen = display_headers("kitchen", &kitchen_key, "og", None);
    let garage = display_headers("garage", &garage_key, "og", None);
    let kitchen_before = fetch_png(&app, &kitchen).await;
    let garage_before = fetch_png(&app, &garage).await;

    let r = app
        .put_json(
            "/dev/api/overrides/panels/e1004",
            &[AUTH],
            r#"{"strength":0.0}"#,
        )
        .await;
    assert!(r.status.is_success(), "PUT failed: {} {}", r.status, r.text());

    assert_ne!(
        kitchen_before,
        fetch_png(&app, &kitchen).await,
        "a strength override on the panel must reach a device on it"
    );
    assert_eq!(
        garage_before,
        fetch_png(&app, &garage).await,
        "and must not reach a device on another panel"
    );
}

#[tokio::test]
async fn a_panel_override_refuses_colors_and_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());
    for body in [
        r##"{"colors":"#000000,#FFFFFF"}"##,
        r#"{"refresh":60}"#,
    ] {
        let r = app
            .put_json("/dev/api/overrides/panels/e1004", &[AUTH], body)
            .await;
        assert_eq!(r.status, axum::http::StatusCode::BAD_REQUEST, "{body}");
    }
}

/// Ruling C1: a pushed panel `dither` is the override rung, so it beats a
/// screen that sets its own algorithm -- on the glass and in the report.
#[tokio::test]
async fn a_pushed_panel_dither_beats_the_screens_dither() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());
    let porch_key = app.register_device("porch").await;
    let porch = display_headers("porch", &porch_key, "og", None);
    let before = fetch_png(&app, &porch).await;

    let r = app
        .put_json(
            "/dev/api/overrides/panels/e1004",
            &[AUTH],
            r#"{"dither":"floyd-steinberg"}"#,
        )
        .await;
    assert!(r.status.is_success(), "PUT failed: {} {}", r.status, r.text());

    assert_ne!(
        before,
        fetch_png(&app, &porch).await,
        "the pushed algorithm must replace the screen's atkinson on /api/display"
    );
    let r = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"porch"}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res: serde_json::Value =
        serde_json::from_str(r.headers["x-byonk-resolution"].to_str().unwrap()).unwrap();
    assert_eq!(res["dither"]["effective"], "floyd-steinberg");
    assert_eq!(res["dither"]["from"], "override");
    assert_eq!(res["dither"]["rungs"]["screen"]["value"], "atkinson");
    assert_eq!(res["dither"]["rungs"]["screen"]["status"], "outranked");
}

#[tokio::test]
async fn the_dev_render_shows_a_live_panel_override_on_the_override_rung() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());
    app.put_json(
        "/dev/api/overrides/panels/e1004",
        &[AUTH],
        r#"{"strength":0.25}"#,
    )
    .await;
    let r = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"kitchen"}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res: serde_json::Value = serde_json::from_str(
        r.headers["x-byonk-resolution"].to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(res["strength"]["from"], "override");
    assert_eq!(res["strength"]["rungs"]["override"]["value"], 0.25);
}
```

The dither test needs a screen that sets its own algorithm and a device that shows it. In `scaffold_swatch_screen`, after writing `local/swatch`, also write `local/dithered`: the same `meta.yaml` (title `Dithered`) and `screen.svg`, with `script.lua`

```lua
return { data = { width = layout.width, height = layout.height }, dither = "atkinson", refresh_rate = 3600 }
```

and add to `app()`'s `devices:`

```yaml
  porch:
    screen: local/dithered
    panel: e1004
```

Add unit tests to `src/server.rs`, a new module beside `tuning_session_tests`:

```rust
#[cfg(test)]
mod timed_map_tests {
    use super::*;

    #[tokio::test]
    async fn a_fresh_entry_is_returned_with_its_remaining_time() {
        let m: TimedMap<u32> = TimedMap::default();
        m.insert("a".into(), 7).await;
        assert_eq!(m.get("a").await, Some(7));
        let live = m.live().await;
        assert_eq!(live.len(), 1);
        assert!(live[0].2 > OVERRIDE_TTL - Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_stale_entry_is_dropped_on_read_not_merely_hidden() {
        let m: TimedMap<u32> = TimedMap::default();
        let stale = Instant::now()
            .checked_sub(OVERRIDE_TTL + Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        m.insert_at("a".into(), 7, stale).await;
        assert_eq!(m.get("a").await, None);
        assert!(m.live().await.is_empty());
        assert!(
            m.inner.read().await.is_empty(),
            "an expired override must be removed, or the map grows for the life of the process"
        );
    }

    #[tokio::test]
    async fn a_re_put_touches_the_entry() {
        let m: TimedMap<u32> = TimedMap::default();
        let old = Instant::now()
            .checked_sub(OVERRIDE_TTL - Duration::from_secs(2))
            .unwrap_or_else(Instant::now);
        m.insert_at("a".into(), 1, old).await;
        m.insert("a".into(), 2).await;
        let live = m.live().await;
        assert_eq!(live[0].1, 2);
        assert!(live[0].2 > OVERRIDE_TTL - Duration::from_secs(5));
    }

    #[tokio::test]
    async fn the_device_override_wins_field_by_field_over_the_panel_one() {
        let o = DevOverrides::default();
        o.panels
            .insert(
                "p".into(),
                CascadeValues {
                    strength: Some(0.1),
                    max_error: Some(0.5),
                    ..Default::default()
                },
            )
            .await;
        o.devices
            .insert(
                "d".into(),
                CascadeValues {
                    strength: Some(0.9),
                    ..Default::default()
                },
            )
            .await;
        let v = o.live_override(Some("d"), Some("p")).await;
        assert_eq!(v.strength, Some(0.9));
        assert_eq!(v.max_error, Some(0.5));
        assert_eq!(o.live_override(None, Some("p")).await.strength, Some(0.1));
        assert_eq!(
            o.live_override(None, None).await,
            CascadeValues::default()
        );
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test dev_overrides_test` and `cargo test --lib timed_map_tests`
Expected: compile error (`TimedMap` missing) / 405 on the new route.

- [ ] **Step 3: Implement `CascadeValues::or`**

In `src/models/config.rs`, after the `CascadeValues` struct:

```rust
impl CascadeValues {
    /// Merge: `self` takes priority, `other` fills the gaps, field by field.
    pub fn or(&self, other: &CascadeValues) -> CascadeValues {
        CascadeValues {
            colors: self.colors.clone().or_else(|| other.colors.clone()),
            colors_actual: self
                .colors_actual
                .clone()
                .or_else(|| other.colors_actual.clone()),
            dither: self.dither.clone().or_else(|| other.dither.clone()),
            max_error: self.max_error.or(other.max_error),
            noise_scale: self.noise_scale.or(other.noise_scale),
            chroma_clamp: self.chroma_clamp.or(other.chroma_clamp),
            strength: self.strength.or(other.strength),
            refresh: self.refresh.or(other.refresh),
            gamut: self.gamut.or(&other.gamut),
        }
    }
}
```

In `src/api/admin/write.rs` `merge_final`, replace the `(Some(b), Some(e)) => Some(CascadeValues { … })` arm with `(Some(b), Some(e)) => Some(b.or(e)),`.

- [ ] **Step 4: Implement `TimedMap` and the new `DevOverrides`**

In `src/server.rs` replace the `DevOverrides` struct and its doc comment with:

```rust
/// How long a dev override survives without being re-PUT. The same backstop
/// as a tuning session: a closed tab must not leave a physical panel showing
/// an uncommitted draft until byonk restarts (cascade spec §4.7).
pub const OVERRIDE_TTL: Duration = TUNING_SESSION_TTL;

/// A string-keyed map whose entries expire [`OVERRIDE_TTL`] after their last
/// insert. Expiry is lazy — every read drops what it finds stale — so there
/// is no background task to schedule, leak or shut down.
#[derive(Clone)]
pub struct TimedMap<V> {
    inner: Arc<RwLock<HashMap<String, (V, Instant)>>>,
}

impl<V> Default for TimedMap<V> {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl<V: Clone> TimedMap<V> {
    /// Insert or replace, and (re)start the clock.
    pub async fn insert(&self, key: String, value: V) {
        self.inner
            .write()
            .await
            .insert(key, (value, Instant::now()));
    }

    #[cfg(test)]
    async fn insert_at(&self, key: String, value: V, touched: Instant) {
        self.inner.write().await.insert(key, (value, touched));
    }

    pub async fn remove(&self, key: &str) -> bool {
        self.inner.write().await.remove(key).is_some()
    }

    pub async fn get(&self, key: &str) -> Option<V> {
        let mut map = self.inner.write().await;
        match map.get(key) {
            Some((v, t)) if t.elapsed() < OVERRIDE_TTL => Some(v.clone()),
            Some(_) => {
                map.remove(key);
                None
            }
            None => None,
        }
    }

    /// Every live entry with its remaining lifetime, sorted by key.
    pub async fn live(&self) -> Vec<(String, V, Duration)> {
        let mut map = self.inner.write().await;
        map.retain(|_, (_, t)| t.elapsed() < OVERRIDE_TTL);
        let mut out: Vec<_> = map
            .iter()
            .map(|(k, (v, t))| (k.clone(), v.clone(), OVERRIDE_TTL.saturating_sub(t.elapsed())))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

/// Runtime overrides set by the dev UI and consumed by production handlers.
///
/// `panels` is keyed by **panel config key**: a calibration belongs to the
/// panel profile, so every device sharing it picks it up. `devices` is keyed
/// by **device config key**. Both feed the cascade's `override` rung, the
/// device's values over the panel's (plan ruling P1-3).
#[derive(Clone, Default)]
pub struct DevOverrides {
    pub panels: TimedMap<CascadeValues>,
    pub devices: TimedMap<CascadeValues>,
    /// device_config_key → when the dev UI last touched this tuning session.
    ///
    /// Present and fresh ⇒ `/api/display` answers `TUNING_REFRESH_SECS` for
    /// that device. Expiry is lazy (see `tuning_session_active`).
    pub tuning_sessions: Arc<RwLock<HashMap<String, Instant>>>,
}

impl DevOverrides {
    /// The override rung for one device on one panel: the device's live
    /// override field by field over its panel's. Never carries `colors` or
    /// `refresh`; the PUT routes refuse both, and this guarantees it.
    pub async fn live_override(
        &self,
        device_key: Option<&str>,
        panel_key: Option<&str>,
    ) -> CascadeValues {
        let device = match device_key {
            Some(k) => self.devices.get(k).await,
            None => None,
        }
        .unwrap_or_default();
        let panel = match panel_key {
            Some(k) => self.panels.get(k).await,
            None => None,
        }
        .unwrap_or_default();
        let mut merged = device.or(&panel);
        merged.colors = None;
        merged.refresh = None;
        merged
    }
}
```

Change the import line `use crate::models::{config::ScreenRepoRef, AppConfig, DitherTuningValues};` to `use crate::models::{config::ScreenRepoRef, AppConfig, CascadeValues};` (`DitherTuningValues` is no longer used here; let clippy confirm).

- [ ] **Step 5: `/api/display` reads the override rung from `live_override`**

In `src/api/display.rs`:

Change `render_inputs_for`'s signature and its `over` field. Replace the three parameters `override_colors_actual: Option<&[(u8, u8, u8)]>, dev_dither: Option<&str>, dev_tuning: Option<&DitherTuningValues>,` with `over: &CascadeValues,`, and the body's `over: RungSettings { … },` with:

```rust
        over: RungSettings::from_cascade(over),
```

Replace the block from `let override_colors = if let Some(ref key) = panel_key {` through `let override_colors_parsed … ;` (`:726-733`) with:

```rust
    // The override rung: live dev overrides, the device's over its panel's
    // (plan ruling P1-3), each expiring 30 minutes after its last PUT.
    let live_override: CascadeValues = dev_overrides
        .live_override(device_entry_key.as_deref(), panel_key.as_deref())
        .await;
```

Delete `dev_dither_override` (`:761-765`) and `dev_tuning_override` (`:768-773`) with their comments. In `resolve_pre_script`'s `over:` use:

```rust
        over: RungSettings {
            dither: live_override.dither.clone(),
            colors_actual: live_override
                .colors_actual
                .as_deref()
                .map(parse_colors_header),
            ..Default::default()
        },
```

Replace the closure captures `let dev_dither = dev_dither_override;` and `let dev_tuning = dev_tuning_override;` with `let live_over = live_override;`, the debug field `dev_dither = ?dev_dither,` with `live_override = ?live_over,`, and the three arguments `override_colors_parsed.as_deref(), dev_dither.as_deref(), dev_tuning.as_ref(),` in the `render_inputs_for(` call with `&live_over,`.

In the unit tests at the bottom of `display.rs`: in `render_inputs_for_assigns_each_binding_to_its_own_rung` replace `let dev_tuning = DitherTuningValues { max_error: Some(0.91), ..Default::default() };` and `let over_actual = …` usage with:

```rust
        let over = CascadeValues {
            colors_actual: Some("#020202,#FDFDFD".into()),
            dither: Some("stucki".into()),
            max_error: Some(0.91),
            ..Default::default()
        };
        let over_actual = parse_colors_header("#020202,#FDFDFD");
```

and the call's three arguments `Some(&over_actual), Some("stucki"), Some(&dev_tuning),` with `&over,`. In the other two tests replace `None, None, None,` (the first three arguments after `&result`) with `&CascadeValues::default(),`.

- [ ] **Step 6: The dev render reads live overrides; the panel routes**

In `src/api/dev.rs` `handle_render`, after the `device` lookup, add:

```rust
    // The config key the device is stored under: override maps are keyed by it.
    let device_key: Option<String> = req
        .device
        .as_deref()
        .and_then(|k| config.resolve_device_key(k, Some(k)));
```

Replace the `panel_cfg` binding with:

```rust
    let panel_key: Option<String> = req
        .panel
        .clone()
        .or_else(|| device.and_then(|d| d.panel.clone()))
        .filter(|k| config.get_panel(k).is_some());
    let panel_cfg = panel_key.as_deref().and_then(|name| config.get_panel(name));
```

After `panel_cfg`, add:

```rust
    // The override rung: the request's own ad-hoc values over whatever a
    // Push made live (plan ruling P1-4). Reading only — this handler writes
    // no server state.
    let live = state
        .dev_overrides
        .live_override(device_key.as_deref(), panel_key.as_deref())
        .await;
    let over: CascadeValues = req.over.or(&live);
```

Then use `over` instead of `req.over` in `resolve_pre_script` (`dither: over.dither.clone()`, `colors_actual: over.colors_actual.as_deref().map(display::parse_colors_header)`) and in `resolve_render` (`over: RungSettings::from_cascade(&over)`).

Replace `PanelColorsRequest`, `handle_set_panel_colors` and `handle_delete_panel_colors` with:

```rust
/// The first field no dev override may carry (ruling C1): `colors` would
/// show the script one palette and render another, and nothing on the
/// override rung consumes `refresh`. Everything else -- `dither` included --
/// is a legitimate draft value for both the panel and the device rung.
fn refused_override_field(v: &CascadeValues) -> Option<&'static str> {
    if v.colors.is_some() {
        Some("colors")
    } else if v.refresh.is_some() {
        Some("refresh")
    } else {
        None
    }
}

/// `PUT /dev/api/overrides/panels/{panel}`: push a panel draft to every
/// device on that panel, as the cascade's override rung. In memory, expiring
/// 30 minutes after the last PUT. Does NOT touch a tuning session: the client
/// re-PUTs the watched device's session alongside each push.
pub async fn handle_put_panel_override(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
    Json(body): Json<CascadeValues>,
) -> Result<StatusCode, ApiError> {
    require_known_panel(&state, &panel)?;
    if let Some(field) = refused_override_field(&body) {
        return Err(ApiError::BadRequest(format!(
            "`{field}` cannot be overridden: an override carries colors_actual, dither, \
             max_error, noise_scale, chroma_clamp, strength and gamut"
        )));
    }
    state.dev_overrides.panels.insert(panel, body).await;
    Ok(StatusCode::OK)
}

/// `DELETE /dev/api/overrides/panels/{panel}`. Idempotent.
pub async fn handle_delete_panel_override(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
) -> StatusCode {
    state.dev_overrides.panels.remove(&panel).await;
    StatusCode::OK
}
```

Keep `PanelColorsRequest` (the save route still uses it until Task 4) — move it next to `handle_save_panel_colors`. In `handle_save_panel_colors` replace `state.dev_overrides.panel_colors.write().await.remove(&panel);` with `state.dev_overrides.panels.remove(&panel).await;`.

In `dev_router` replace the `"/panel-colors/{panel}"` PUT/DELETE route with:

```rust
        .route(
            "/overrides/panels/{panel}",
            put(handle_put_panel_override).delete(handle_delete_panel_override),
        )
```

In `tests/dev_panel_colors_save_test.rs`, `saving_clears_the_temporary_override` uses the old PUT/DELETE: change those two URLs to `/dev/api/overrides/panels/e1004` (Task 4 rewrites this file).

- [ ] **Step 7: Run the tests**

Run: `cargo test --test dev_overrides_test --test dev_panel_colors_save_test --test dev_render_is_read_only_test --test dev_render_provenance_test --test dev_tuning_session_test && cargo test --lib`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/server.rs src/models/config.rs src/api/admin/write.rs src/api/display.rs src/api/dev.rs tests/dev_overrides_test.rs tests/dev_panel_colors_save_test.rs
git commit -m "feat(dev): timed panel-keyed cascade overrides, read as the override rung"
```

---

### Task 3: Device overrides and the live-override listing

**Files:**
- Modify: `src/api/dev.rs` — new handlers, response types, routes
- Test: `tests/dev_overrides_test.rs`

**Interfaces:**
- Consumes: `TimedMap`, `DevOverrides::live_override`, `AppConfig::resolve_device_key` (`config.rs:714`).
- Produces:
  - `PUT /dev/api/overrides/devices/{device}` body `CascadeValues` → 200 empty; 404 unknown device; 400 with `colors` or `refresh` (via `refused_override_field`, Task 2). Stored under the **config key** (`{device}` may be a MAC in any case or a registration code).
  - `DELETE /dev/api/overrides/devices/{device}` → 200 empty (resolves the key the same way; unknown → 200, idempotent).
  - `GET /dev/api/overrides` →
    ```json
    {
      "panels":  [{"key":"e1004","values":{CascadeValues},"expires_in_secs":1799}],
      "devices": [{"key":"kitchen","values":{CascadeValues},"expires_in_secs":1799}],
      "tuning_sessions": [{"key":"kitchen","expires_in_secs":1799}]
    }
    ```
    each sorted by key, expired entries dropped.

- [ ] **Step 1: Write the failing tests**

Append to `tests/dev_overrides_test.rs`:

```rust
fn resolution_of(r: &common::app::TestResponse) -> serde_json::Value {
    serde_json::from_str(r.headers["x-byonk-resolution"].to_str().unwrap()).unwrap()
}

#[tokio::test]
async fn a_device_override_reaches_that_device_only() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());
    let kitchen_key = app.register_device("kitchen").await;
    let hallway_key = app.register_device("hallway").await;
    let kitchen = display_headers("kitchen", &kitchen_key, "og", None);
    let hallway = display_headers("hallway", &hallway_key, "og", None);
    let kitchen_before = fetch_png(&app, &kitchen).await;
    let hallway_before = fetch_png(&app, &hallway).await;

    let r = app
        .put_json(
            "/dev/api/overrides/devices/kitchen",
            &[AUTH],
            r#"{"strength":0.0}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());

    assert_ne!(kitchen_before, fetch_png(&app, &kitchen).await);
    assert_eq!(
        hallway_before,
        fetch_png(&app, &hallway).await,
        "a device override must not reach another device on the same panel"
    );

    app.delete("/dev/api/overrides/devices/kitchen", &[AUTH]).await;
    assert_eq!(kitchen_before, fetch_png(&app, &kitchen).await);
}

#[tokio::test]
async fn device_override_beats_panel_override_field_by_field() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());
    app.put_json(
        "/dev/api/overrides/panels/e1004",
        &[AUTH],
        r#"{"strength":0.1,"max_error":0.5}"#,
    )
    .await;
    app.put_json(
        "/dev/api/overrides/devices/kitchen",
        &[AUTH],
        r#"{"strength":0.9}"#,
    )
    .await;

    let k = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"kitchen"}"#)
        .await;
    let k = resolution_of(&k);
    assert_eq!(k["strength"]["rungs"]["override"]["value"], 0.9);
    assert_eq!(k["max_error"]["rungs"]["override"]["value"], 0.5);

    let h = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"hallway"}"#)
        .await;
    assert_eq!(
        resolution_of(&h)["strength"]["rungs"]["override"]["value"],
        0.1
    );
}

#[tokio::test]
async fn a_device_override_refuses_colors_and_refresh_and_unknown_devices() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());
    for body in [r##"{"colors":"#000000,#FFFFFF"}"##, r#"{"refresh":60}"#] {
        let r = app
            .put_json("/dev/api/overrides/devices/kitchen", &[AUTH], body)
            .await;
        assert_eq!(r.status, axum::http::StatusCode::BAD_REQUEST, "{body}");
    }
    let r = app
        .put_json(
            "/dev/api/overrides/devices/nosuchdevice",
            &[AUTH],
            r#"{"strength":1.0}"#,
        )
        .await;
    assert_eq!(r.status, axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_listing_shows_every_live_override_and_session_with_its_time_left() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());
    app.put_json(
        "/dev/api/overrides/panels/e1004",
        &[AUTH],
        r##"{"colors_actual":"#050505,#FAFAFA,#E01010,#101010,#1010E0,#10E010"}"##,
    )
    .await;
    app.put_json(
        "/dev/api/overrides/devices/kitchen",
        &[AUTH],
        r#"{"dither":"stucki"}"#,
    )
    .await;
    app.put_json("/dev/api/tuning-session/kitchen", &[AUTH], "")
        .await;

    let r = app.get_with_headers("/dev/api/overrides", &[AUTH]).await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let v: serde_json::Value = r.json();
    assert_eq!(v["panels"][0]["key"], "e1004");
    assert_eq!(
        v["panels"][0]["values"]["colors_actual"],
        "#050505,#FAFAFA,#E01010,#101010,#1010E0,#10E010"
    );
    assert_eq!(v["devices"][0]["key"], "kitchen");
    assert_eq!(v["devices"][0]["values"]["dither"], "stucki");
    assert_eq!(v["tuning_sessions"][0]["key"], "kitchen");
    let left = v["panels"][0]["expires_in_secs"].as_u64().unwrap();
    assert!(left > 1790 && left <= 1800, "{left}");

    app.delete("/dev/api/overrides/panels/e1004", &[AUTH]).await;
    let v: serde_json::Value = app
        .get_with_headers("/dev/api/overrides", &[AUTH])
        .await
        .json();
    assert_eq!(v["panels"], serde_json::json!([]));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test dev_overrides_test`
Expected: the four new tests FAIL with 405 / 404.

- [ ] **Step 3: Implement**

In `src/api/dev.rs` add:

```rust
/// `PUT /dev/api/overrides/devices/{device}`: push a device draft as the
/// override rung for that one device. Stored under the config key, so a MAC
/// in another case or a registration code addresses the same entry.
pub async fn handle_put_device_override(
    State(state): State<AppState>,
    axum::extract::Path(device): axum::extract::Path<String>,
    Json(body): Json<CascadeValues>,
) -> Result<StatusCode, ApiError> {
    let key = state
        .config
        .load()
        .resolve_device_key(&device, Some(&device))
        .ok_or(ApiError::NotFound)?;
    if let Some(field) = refused_override_field(&body) {
        return Err(ApiError::BadRequest(format!(
            "`{field}` cannot be overridden: an override carries colors_actual, dither, \
             max_error, noise_scale, chroma_clamp, strength and gamut"
        )));
    }
    state.dev_overrides.devices.insert(key, body).await;
    Ok(StatusCode::OK)
}

/// `DELETE /dev/api/overrides/devices/{device}`. Idempotent.
pub async fn handle_delete_device_override(
    State(state): State<AppState>,
    axum::extract::Path(device): axum::extract::Path<String>,
) -> StatusCode {
    let key = state
        .config
        .load()
        .resolve_device_key(&device, Some(&device))
        .unwrap_or(device);
    state.dev_overrides.devices.remove(&key).await;
    StatusCode::OK
}

/// One live override in the listing.
#[derive(Debug, Serialize)]
pub struct LiveOverride {
    pub key: String,
    pub values: CascadeValues,
    pub expires_in_secs: u64,
}

/// One live tuning session in the listing.
#[derive(Debug, Serialize)]
pub struct LiveSession {
    pub key: String,
    pub expires_in_secs: u64,
}

/// `GET /dev/api/overrides`.
#[derive(Debug, Serialize)]
pub struct OverridesResponse {
    pub panels: Vec<LiveOverride>,
    pub devices: Vec<LiveOverride>,
    pub tuning_sessions: Vec<LiveSession>,
}

/// List everything the dev UI has made live, so a reloaded page can show a
/// Push that is still reaching hardware (spec §6).
pub async fn handle_list_overrides(State(state): State<AppState>) -> Json<OverridesResponse> {
    let o = &state.dev_overrides;
    let to_live = |v: Vec<(String, CascadeValues, std::time::Duration)>| {
        v.into_iter()
            .map(|(key, values, left)| LiveOverride {
                key,
                values,
                expires_in_secs: left.as_secs(),
            })
            .collect::<Vec<_>>()
    };
    let panels = to_live(o.panels.live().await);
    let devices = to_live(o.devices.live().await);
    let mut tuning_sessions: Vec<LiveSession> = {
        let mut sessions = o.tuning_sessions.write().await;
        sessions.retain(|_, t| t.elapsed() < crate::server::TUNING_SESSION_TTL);
        sessions
            .iter()
            .map(|(k, t)| LiveSession {
                key: k.clone(),
                expires_in_secs: crate::server::TUNING_SESSION_TTL
                    .saturating_sub(t.elapsed())
                    .as_secs(),
            })
            .collect()
    };
    tuning_sessions.sort_by(|a, b| a.key.cmp(&b.key));
    Json(OverridesResponse {
        panels,
        devices,
        tuning_sessions,
    })
}
```

Routes in `dev_router`:

```rust
        .route("/overrides", get(handle_list_overrides))
        .route(
            "/overrides/devices/{device}",
            put(handle_put_device_override).delete(handle_delete_device_override),
        )
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --test dev_overrides_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/api/dev.rs tests/dev_overrides_test.rs
git commit -m "feat(dev): device-keyed overrides and a listing of everything live"
```

---

### Task 4: Panel Save writes the whole panel rung, in place

**Files:**
- Modify: `src/models/config.rs` — `PanelDitherConfig` (`:149-181`): derive `PartialEq`, add `Serialize` (flat form, tidy floats), `is_empty`; tests in the existing `mod tests`
- Modify: `src/services/config_writer.rs` — generalise `block_range` (`:146-170`) into `path_range`; add `remove_path`, `set_path`, `sync_mapping`; unit tests in its `mod tests` (`:375-`)
- Modify: `src/api/dev.rs` — replace `handle_save_panel_colors`/`PanelColorsRequest` with `handle_save_panel`/`PanelRungWrite`; route
- Rename + modify: `tests/dev_panel_colors_save_test.rs` → `tests/dev_panel_save_test.rs`

**Interfaces:**
- Produces:
  - `impl Serialize for PanelDitherConfig` — the **flat** form the hand-written `Deserialize` reads: `max_error`, `noise_scale`, `chroma_clamp`, `strength` (only when set), `gamut` (only set sub-fields, omitted when empty), then each algorithm name → its own flat map. Every number goes through `tidy(f32) -> f64` (ruling P1-8). Round-trips through `Deserialize`.
  - `PanelDitherConfig::is_empty(&self) -> bool`.
  - `config_writer::remove_path(yaml: &str, path: &[&str]) -> Result<String, ConfigWriteError>` — removes the key and its body lines; an absent path leaves the text unchanged.
  - `config_writer::set_path(yaml: &str, path: &[&str], value: &serde_yaml::Value) -> Result<String, ConfigWriteError>` — scalar over scalar: in place (`replace_scalar`); otherwise the key's own lines are replaced, or (absent key) inserted as the parent's last child, as text written at the parent's child indentation. The parent must exist.
  - `config_writer::sync_mapping(yaml: &str, path: &[&str], target: &serde_yaml::Mapping) -> Result<String, ConfigWriteError>` — makes the mapping at `path` equal `target` key by key, recursing into nested mappings; unchanged keys' lines are not touched.
  - `POST /dev/api/panels/{panel}/save` body (deny_unknown_fields):
    ```json
    { "colors_actual": "#..,#..", "dither": { "max_error": 1.0, "strength": 0.8, "gamut": {"knee": 0.6}, "atkinson": {"noise_scale": 2.0} } }
    ```
    Whole-rung replace: an absent `colors_actual` or `dither` is **removed** from `panels.<id>`. 200 empty; 404 unknown panel; 409 embedded config; 422 unknown field. Clears the panel's live override. Never rewrites a line it does not change (ruling C2).
  - The route `POST /dev/api/panel-colors/{panel}/save` is removed.

- [ ] **Step 1: Write the failing unit tests**

In `src/models/config.rs` `mod tests`:

```rust
#[test]
fn panel_dither_config_serializes_to_the_flat_form_it_parses_from() {
    let yaml = "max_error: 0.5\nstrength: 0.8\ngamut:\n  knee: 0.6\natkinson:\n  noise_scale: 2.0\n";
    let parsed: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
    let json = serde_json::to_value(&parsed).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "max_error": 0.5,
            "strength": 0.8,
            "gamut": {"knee": 0.6},
            "atkinson": {"noise_scale": 2.0}
        })
    );
    let back: PanelDitherConfig = serde_json::from_value(json).unwrap();
    assert_eq!(back, parsed);
    // Ruling P1-8: no f32 representation noise reaches config.yaml.
    let text = serde_yaml::to_string(&parsed).unwrap();
    assert!(text.contains("strength: 0.8\n"), "{text}");
}

#[test]
fn an_empty_panel_dither_config_is_empty() {
    assert!(PanelDitherConfig::default().is_empty());
    let one: PanelDitherConfig = serde_yaml::from_str("stucki:\n  strength: 0.5\n").unwrap();
    assert!(!one.is_empty());
}
```

In `src/services/config_writer.rs` `mod tests`:

```rust
    const PANELS: &str = "\
panels:
    # first panel
    e1004:
        name: E1004   # inline
        colors: '#000000,#FFFFFF'
    og:
        name: OG
        dither:
            max_error: 0.5   # tuned by hand
            strength: 1.0
";

    #[test]
    fn set_path_inserts_a_nested_mapping_at_the_parents_child_indent() {
        let mut m = serde_yaml::Mapping::new();
        m.insert("strength".into(), 0.8.into());
        let out = set_path(PANELS, &["panels", "e1004", "dither"], &serde_yaml::Value::Mapping(m)).unwrap();
        assert!(out.contains("        dither:\n          strength: 0.8\n"), "{out}");
        assert!(out.contains("# first panel") && out.contains("# inline") && out.contains("# tuned by hand"));
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(doc["panels"]["e1004"]["dither"]["strength"].as_f64(), Some(0.8));
        assert_eq!(doc["panels"]["e1004"]["name"], "E1004");
    }

    #[test]
    fn sync_mapping_touches_only_the_keys_that_change() {
        let mut target = serde_yaml::Mapping::new();
        target.insert("max_error".into(), 0.5.into()); // unchanged: its line and comment stay
        let mut gamut = serde_yaml::Mapping::new();
        gamut.insert("knee".into(), 0.6.into());
        target.insert("gamut".into(), serde_yaml::Value::Mapping(gamut)); // new nested block
        // `strength` absent from the target: removed.
        let out = sync_mapping(PANELS, &["panels", "og", "dither"], &target).unwrap();
        assert!(out.contains("max_error: 0.5   # tuned by hand"), "{out}");
        assert!(!out.contains("strength"), "{out}");
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(doc["panels"]["og"]["dither"]["gamut"]["knee"].as_f64(), Some(0.6));
        assert_eq!(doc["panels"]["og"]["name"], "OG");
    }

    #[test]
    fn set_path_rewrites_an_inline_flow_parent_as_a_block() {
        let yaml = "panels:\n  og:\n    name: OG\n    dither: {}\n";
        let out = set_path(yaml, &["panels", "og", "dither", "strength"], &0.8.into()).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(doc["panels"]["og"]["dither"]["strength"].as_f64(), Some(0.8));
        assert_eq!(doc["panels"]["og"]["name"], "OG");
    }

    #[test]
    fn remove_path_of_an_absent_key_changes_nothing() {
        assert_eq!(remove_path(PANELS, &["panels", "e1004", "dither"]).unwrap(), PANELS);
    }
```

- [ ] **Step 2: Rename the integration test and rewrite it for the new route**

```bash
git mv tests/dev_panel_colors_save_test.rs tests/dev_panel_save_test.rs
```

In `tests/dev_panel_save_test.rs`: change the module doc to `//! Panel Save writes the whole panel rung into config.yaml, in place, keeping every comment and the order of panels.`; replace every `/dev/api/panel-colors/<id>/save` with `/dev/api/panels/<id>/save` and the override PUT/DELETE URLs with `/dev/api/overrides/panels/<id>`. Bodies `{"colors_actual": …}` stay valid. Append:

```rust
fn panel_of(yaml: &str, id: &str) -> serde_yaml::Value {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    doc["panels"][id].clone()
}

fn panel_order(yaml: &str) -> Vec<String> {
    let doc: serde_yaml::Mapping = serde_yaml::from_str::<serde_yaml::Value>(yaml).unwrap()["panels"]
        .as_mapping()
        .unwrap()
        .clone();
    doc.keys().map(|k| k.as_str().unwrap().to_string()).collect()
}

const COMMENTED_PANELS: &str = "\
admin:
  token: secret
panels:
  # The reTerminal E1004. Its greens are hand-tuned; see docs.
  e1004:
    name: reTerminal E1004   # the six-colour one
    # official palette, from the datasheet
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
  og:
    name: TRMNL OG
    colors: '#000000,#FFFFFF'
    colors_actual: '#1A1A1A,#D8D8D8'   # measured 2026-01-01
    dither:
      max_error: 0.5   # hand-tuned
      atkinson:
        noise_scale: 2.0
";

/// Ruling C2 (a), (b), (c): a Save that creates a whole `dither:` block keeps
/// a comment inside the saved panel and one on its sibling, leaves the order
/// of `panels:` alone, and re-parses to exactly what was sent.
#[tokio::test]
async fn a_dither_save_keeps_comments_and_panel_order() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED_PANELS, dir.path());
    let before = std::fs::read_to_string(&config_path).unwrap();
    let r = app
        .post_json(
            "/dev/api/panels/e1004/save",
            &[AUTH],
            &format!(
                r#"{{"colors_actual":"{TUNED}","dither":{{"strength":0.8,"gamut":{{"knee":0.6}}}}}}"#
            ),
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let yaml = std::fs::read_to_string(&config_path).unwrap();

    // (a) comments inside the saved block and on a sibling survive
    for c in [
        "# The reTerminal E1004. Its greens are hand-tuned; see docs.",
        "# the six-colour one",
        "# official palette, from the datasheet",
        "# measured 2026-01-01",
        "# hand-tuned",
    ] {
        assert!(yaml.contains(c), "lost {c:?}:\n{yaml}");
    }
    // (b) panel order unchanged
    assert_eq!(panel_order(&yaml), panel_order(&before));
    // (c) the result re-parses to the expected values
    let p = panel_of(&yaml, "e1004");
    assert_eq!(p["name"], "reTerminal E1004");
    assert_eq!(p["colors"], "#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00");
    assert_eq!(p["colors_actual"], TUNED);
    assert_eq!(p["dither"]["strength"].as_f64(), Some(0.8));
    assert_eq!(p["dither"]["gamut"]["knee"].as_f64(), Some(0.6));
    assert_eq!(panel_of(&yaml, "og"), panel_of(&before, "og"), "the sibling is untouched");
}

#[tokio::test]
async fn saving_changes_one_dither_key_in_place_and_keeps_algorithm_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED_PANELS, dir.path());
    let r = app
        .post_json(
            "/dev/api/panels/og/save",
            &[AUTH],
            r##"{"colors_actual":"#1A1A1A,#D8D8D8","dither":{"max_error":0.5,"strength":0.9,"atkinson":{"noise_scale":2.0}}}"##,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let yaml = std::fs::read_to_string(&config_path).unwrap();
    assert!(yaml.contains("max_error: 0.5   # hand-tuned"), "an unchanged line was rewritten:\n{yaml}");
    assert!(yaml.contains("colors_actual: '#1A1A1A,#D8D8D8'   # measured 2026-01-01"), "{yaml}");
    let p = panel_of(&yaml, "og");
    assert_eq!(p["dither"]["strength"].as_f64(), Some(0.9));
    assert_eq!(p["dither"]["atkinson"]["noise_scale"].as_f64(), Some(2.0));
}

#[tokio::test]
async fn saving_without_colors_actual_or_dither_removes_both() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(COMMENTED_PANELS, dir.path());
    let r = app.post_json("/dev/api/panels/og/save", &[AUTH], "{}").await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let yaml = std::fs::read_to_string(&config_path).unwrap();
    let p = panel_of(&yaml, "og");
    assert!(p.get("colors_actual").is_none(), "{p:?}");
    assert!(p.get("dither").is_none(), "{p:?}");
    assert_eq!(p["name"], "TRMNL OG");
    assert!(yaml.contains("# the six-colour one"), "{yaml}");
}

#[tokio::test]
async fn an_unknown_field_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());
    let r = app
        .post_json(
            "/dev/api/panels/og/save",
            &[AUTH],
            r##"{"colors":"#000000,#FFFFFF"}"##,
        )
        .await;
    assert_eq!(r.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}
```

Note: `saving_without_colors_actual_or_dither_removes_both` deliberately sends `{}`; with Save as a whole-rung replace this is "the panel has no measurement and no tuning".

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test --lib panel_dither_config && cargo test --lib config_writer && cargo test --test dev_panel_save_test`
Expected: compile errors (`Serialize`, `is_empty`, `set_path`, … missing) / 405.

- [ ] **Step 4: Implement `PanelDitherConfig` serialisation**

In `src/models/config.rs` change `#[derive(Debug, Clone, Default)]` on `PanelDitherConfig` to `#[derive(Debug, Clone, Default, PartialEq)]` and add:

```rust
impl PanelDitherConfig {
    /// True when the block configures nothing at all.
    pub fn is_empty(&self) -> bool {
        self.defaults.is_empty() && self.algorithms.is_empty()
    }
}

/// An `f32` as the `f64` with the same shortest decimal, so `0.8` is written
/// as `0.8` and not `0.800000011920929` (plan ruling P1-8).
fn tidy(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(v as f64)
}

/// Write the set fields of one tuning block into a map. Shared by a panel
/// block's flat defaults and each per-algorithm sub-map, so both serialise
/// exactly as `PanelDitherConfig`'s `Deserialize` reads them.
fn write_tuning<M: serde::ser::SerializeMap>(
    m: &mut M,
    t: &DitherTuningValues,
) -> Result<(), M::Error> {
    if let Some(v) = t.max_error {
        m.serialize_entry("max_error", &tidy(v))?;
    }
    if let Some(v) = t.noise_scale {
        m.serialize_entry("noise_scale", &tidy(v))?;
    }
    if let Some(v) = t.chroma_clamp {
        m.serialize_entry("chroma_clamp", &tidy(v))?;
    }
    if let Some(v) = t.strength {
        m.serialize_entry("strength", &tidy(v))?;
    }
    if !t.gamut.is_empty() {
        m.serialize_entry("gamut", &GamutOut(&t.gamut))?;
    }
    Ok(())
}

/// A gamut block without its unset fields (no `knee: null` in config.yaml).
struct GamutOut<'a>(&'a GamutTuningValues);

impl Serialize for GamutOut<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        if let Some(v) = self.0.knee {
            m.serialize_entry("knee", &tidy(v))?;
        }
        if let Some(v) = self.0.amount {
            m.serialize_entry("amount", &tidy(v))?;
        }
        if let Some(v) = self.0.max_compression {
            m.serialize_entry("max_compression", &tidy(v))?;
        }
        m.end()
    }
}

/// One per-algorithm sub-map.
struct TuningOut<'a>(&'a DitherTuningValues);

impl Serialize for TuningOut<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        write_tuning(&mut m, self.0)?;
        m.end()
    }
}

/// The flat form `Deserialize` reads: scalars and `gamut` are the defaults,
/// every other key is an algorithm. `error_clamp` is never written back.
impl Serialize for PanelDitherConfig {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        write_tuning(&mut m, &self.defaults)?;
        let mut names: Vec<&String> = self.algorithms.keys().collect();
        names.sort();
        for name in names {
            m.serialize_entry(name, &TuningOut(&self.algorithms[name]))?;
        }
        m.end()
    }
}
```

- [ ] **Step 5: In-place nested writes in `config_writer`**

In `src/services/config_writer.rs`, replace `block_range` with a path-generic version and keep `block_range` as a thin wrapper (its callers are unchanged):

```rust
/// Byte range of `path`'s key line plus the deeper-indented body lines that
/// belong to it. Scanned by indentation rather than `Op::Remove`, because
/// tree-sitter attaches a following less-indented comment to the last block
/// node and would delete it too.
fn path_range(yaml: &str, path: &[&str]) -> Result<(usize, usize), ConfigWriteError> {
    let doc = document(yaml)?;
    let feature = doc
        .query_pretty(&route_for(path))
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let key_byte = feature.location.byte_span.0;
    let line_start = yaml[..key_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let key_indent = indent_of(&yaml[line_start..]);
    let mut end = next_line_start(yaml, line_start);
    while end < yaml.len() {
        let line_end = next_line_start(yaml, end);
        let line = &yaml[end..line_end];
        if line.trim().is_empty() || indent_of(line) <= key_indent {
            break;
        }
        end = line_end;
    }
    Ok((line_start, end))
}

fn block_range(yaml: &str, section: &str, key: &str) -> Result<(usize, usize), ConfigWriteError> {
    path_range(yaml, &[section, key])
}

fn parse(yaml: &str) -> Result<serde_yaml::Value, ConfigWriteError> {
    serde_yaml::from_str(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))
}

fn lookup<'a>(doc: &'a serde_yaml::Value, path: &[&str]) -> Option<&'a serde_yaml::Value> {
    path.iter().try_fold(doc, |v, k| v.get(*k))
}

fn is_scalar(v: &serde_yaml::Value) -> bool {
    !matches!(v, serde_yaml::Value::Mapping(_) | serde_yaml::Value::Sequence(_))
}

/// Does `path`'s key line carry its value inline (`dither: {}`, `x: null`)?
/// A block mapping's key line ends at the colon (or a comment).
fn inline_value(yaml: &str, path: &[&str]) -> Result<bool, ConfigWriteError> {
    let (start, _) = path_range(yaml, path)?;
    let line = &yaml[start..next_line_start(yaml, start)];
    let code = line.split(" #").next().unwrap_or(line);
    Ok(code
        .split_once(':')
        .map(|(_, rest)| !rest.trim().is_empty())
        .unwrap_or(false))
}

/// `{key: value}` as block YAML, every line indented by `indent` spaces.
/// Written as text because `yamlpatch`'s `Op::Add` mis-indents nested maps.
fn block_text(key: &str, value: &serde_yaml::Value, indent: usize) -> Result<String, ConfigWriteError> {
    let mut m = serde_yaml::Mapping::new();
    m.insert(key.into(), value.clone());
    let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(m))
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let pad = " ".repeat(indent);
    Ok(text.lines().map(|l| format!("{pad}{l}\n")).collect())
}

/// Remove `path` and its body. An absent path leaves the text unchanged:
/// removing what is not there is not an error for a whole-rung write.
pub fn remove_path(yaml: &str, path: &[&str]) -> Result<String, ConfigWriteError> {
    if lookup(&parse(yaml)?, path).is_none() {
        return Ok(yaml.to_string());
    }
    let (start, end) = path_range(yaml, path)?;
    let mut out = yaml.to_string();
    out.replace_range(start..end, "");
    Ok(out)
}

/// Set `path` to `value`, rewriting only the lines that key owns.
///
/// - an equal value: nothing is written;
/// - an existing scalar replaced by a scalar: in place, comment kept;
/// - an existing key otherwise: its lines are replaced by a block at the
///   same place and indentation;
/// - an absent key: appended as the parent's last child, at the indentation
///   the parent's children already use (parent indent + 2 when it has none).
///   An inline parent (`dither: {}`) is first rewritten as a block.
///
/// The parent must exist.
pub fn set_path(yaml: &str, path: &[&str], value: &serde_yaml::Value) -> Result<String, ConfigWriteError> {
    assert!(path.len() >= 2, "set_path: a path needs a parent");
    let doc = parse(yaml)?;
    let (parent, key) = path.split_at(path.len() - 1);
    let key = key[0];

    if let Some(existing) = lookup(&doc, path) {
        if existing == value {
            // Nothing to write: leave the line, its quoting and its comment alone.
            return Ok(yaml.to_string());
        }
        if is_scalar(existing) && is_scalar(value) {
            return replace_scalar(yaml, path, value.clone());
        }
        let (start, end) = path_range(yaml, path)?;
        let text = block_text(key, value, indent_of(&yaml[start..]))?;
        let mut out = yaml.to_string();
        out.replace_range(start..end, &text);
        return Ok(out);
    }

    let parent_value = lookup(&doc, parent)
        .ok_or_else(|| ConfigWriteError::NotFound(parent.join(".")))?;
    if parent.len() >= 2 && inline_value(yaml, parent)? {
        let mut merged = parent_value.as_mapping().cloned().unwrap_or_default();
        merged.insert(key.into(), value.clone());
        return set_path(yaml, parent, &serde_yaml::Value::Mapping(merged));
    }

    let (start, end) = path_range(yaml, parent)?;
    let parent_indent = indent_of(&yaml[start..]);
    let child_indent = yaml[next_line_start(yaml, start)..end]
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(indent_of)
        .unwrap_or(parent_indent + 2);
    let mut text = block_text(key, value, child_indent)?;
    if end > 0 && !yaml[..end].ends_with('\n') {
        text.insert(0, '\n');
    }
    let mut out = yaml.to_string();
    out.insert_str(end, &text);
    Ok(out)
}

/// Make the mapping at `path` equal `target`, key by key: keys the target
/// lacks are removed, changed values are set, nested mappings are synced
/// recursively, and equal values are left exactly as written. An absent or
/// inline mapping is written whole by [`set_path`].
pub fn sync_mapping(
    yaml: &str,
    path: &[&str],
    target: &serde_yaml::Mapping,
) -> Result<String, ConfigWriteError> {
    let doc = parse(yaml)?;
    let existing = match lookup(&doc, path) {
        Some(serde_yaml::Value::Mapping(m)) if !inline_value(yaml, path)? => m.clone(),
        _ => return set_path(yaml, path, &serde_yaml::Value::Mapping(target.clone())),
    };
    let key_of = |k: &serde_yaml::Value| -> Result<String, ConfigWriteError> {
        k.as_str()
            .map(str::to_string)
            .ok_or_else(|| ConfigWriteError::Patch(format!("non-string key {k:?}")))
    };
    let mut out = yaml.to_string();
    // Removals first, so an insert never lands next to a line about to go.
    for k in existing.keys() {
        if !target.contains_key(k) {
            let k = key_of(k)?;
            let child: Vec<&str> = path.iter().copied().chain([k.as_str()]).collect();
            out = remove_path(&out, &child)?;
        }
    }
    for (k, v) in target {
        let ks = key_of(k)?;
        let child: Vec<&str> = path.iter().copied().chain([ks.as_str()]).collect();
        match (existing.get(k), v) {
            (Some(old), new) if old == new => {}
            (Some(serde_yaml::Value::Mapping(_)), serde_yaml::Value::Mapping(m)) => {
                out = sync_mapping(&out, &child, m)?;
            }
            (_, new) => out = set_path(&out, &child, new)?,
        }
    }
    Ok(out)
}
```

- [ ] **Step 6: The route**

In `src/api/dev.rs` replace `PanelColorsRequest` and `handle_save_panel_colors` with:

```rust
/// Body of `POST /dev/api/panels/{panel}/save`: the panel rung, whole. An
/// absent field is removed from the panel, because the draft replaced the
/// rung (controller ruling R-B).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelRungWrite {
    pub colors_actual: Option<String>,
    pub dither: Option<PanelDitherConfig>,
}

/// Write a panel's whole rung into `config.yaml`, durably (controller ruling
/// R-D), in place, key by key: lines it does not change are not rewritten,
/// so comments and the order of `panels:` survive (ruling C2). Then drop the
/// panel's live override (P1-9).
///
/// Deliberately NOT guarded by `require_writable_global`: `panels` is absent
/// from the Home Assistant add-on Options schema, so nothing else owns it.
pub async fn handle_save_panel(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
    Json(body): Json<PanelRungWrite>,
) -> Result<StatusCode, ApiError> {
    use crate::services::config_writer::{remove_path, set_path, sync_mapping};
    require_known_panel(&state, &panel)?;
    let path = crate::api::config_persist::require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let patch = |e: crate::services::config_writer::ConfigWriteError| {
        ApiError::Internal(format!("patch config: {e}"))
    };
    let mut yaml = std::fs::read_to_string(&path)
        .map_err(|e| ApiError::Internal(format!("read config: {e}")))?;

    let actual_path = ["panels", panel.as_str(), "colors_actual"];
    yaml = match &body.colors_actual {
        Some(c) => set_path(&yaml, &actual_path, &serde_yaml::Value::from(c.as_str())),
        None => remove_path(&yaml, &actual_path),
    }
    .map_err(patch)?;

    let dither_path = ["panels", panel.as_str(), "dither"];
    yaml = match body.dither.as_ref().filter(|d| !d.is_empty()) {
        Some(d) => {
            let serde_yaml::Value::Mapping(target) = serde_yaml::to_value(d)
                .map_err(|e| ApiError::Internal(format!("serialize dither: {e}")))?
            else {
                return Err(ApiError::Internal("dither did not serialize to a mapping".into()));
            };
            sync_mapping(&yaml, &dither_path, &target)
        }
        None => remove_path(&yaml, &dither_path),
    }
    .map_err(patch)?;

    crate::api::config_persist::persist(&state, &path, yaml)?;
    state.dev_overrides.panels.remove(&panel).await;
    Ok(StatusCode::OK)
}
```

(If `ConfigWriteError` is not `pub` at `crate::services::config_writer::ConfigWriteError`, use `|e| ApiError::Internal(format!("patch config: {e}"))` inline at each `map_err`; it implements `Display`.)

In `dev_router` replace `.route("/panel-colors/{panel}/save", post(handle_save_panel_colors))` with `.route("/panels/{panel}/save", post(handle_save_panel))`.

- [ ] **Step 7: Run the tests**

Run: `cargo test --lib panel_dither && cargo test --lib config_writer && cargo test --test dev_panel_save_test --test dev_overrides_test && cargo test --test admin_devices_test`
Expected: PASS (the last one guards `block_range`'s callers through `upsert_device`/`remove_device`).

- [ ] **Step 8: Commit**

```bash
git add src/models/config.rs src/services/config_writer.rs src/api/dev.rs tests/dev_panel_save_test.rs
git commit -m "feat(dev): panel Save writes the whole panel rung in place, keeping every comment"
```

---

### Task 5: Device Save from the dev UI, as one atomic PATCH

**Files:**
- Modify: `src/api/admin/write.rs` — `resolve_clear` (`:290-346`), `cascade_field_is_set` (`:370-383`, deleted), `clear_cascade_field` (`:385-399`), `apply_clear` (`:409-440`), `apply_device_patch` (`:762-855`); unit tests `a_final_field_cannot_be_set_and_cleared_in_one_request` (`:1422-1434`, replaced) and new ones beside it
- Modify: `src/mcp/tools_device.rs` — the `clear` doc comment (`:88-96`)
- Modify: `src/api/dev.rs` — handler + route
- Create: `tests/dev_device_save_test.rs`

**Interfaces:**
- Consumes: `crate::api::admin::write::{apply_device_patch, DeviceWrite}` (`write.rs:31`, `:762`).
- Produces:
  - `apply_device_patch` semantics (ruling C3): `clear` applies to the existing entry first, then the body's set fields merge over the result; one write lock, one `persist`. A key both cleared and set ends up **set**. `clear` additionally accepts `gamut.knee`, `gamut.amount`, `gamut.max_compression`, `final.gamut.knee`, `final.gamut.amount`, `final.gamut.max_compression`. Same behaviour for `PATCH /api/admin/devices/{key}` and the MCP `configure_device` tool; nothing else about them changes.
  - `PATCH /dev/api/devices/{key}` body = `DeviceWrite` (the admin PATCH body: base fields flat, `final: {…}` merged field by field, `params`, `name`, `temperature_profile`, `maximum_compatibility`, `min_png_bytes`, `clear: [...]`) → 200 `{"key","screen"}`; 404 unknown key; 400 invalid screen/params/clear name; 409 embedded config. Clears the device's live override.
  - Plan 2's device Save sends exactly one such request, e.g. `{"clear":["gamut.knee","params"],"params":{"c":3},"final":{"dither":"atkinson"}}`.

- [ ] **Step 1: Write the failing tests**

Create `tests/dev_device_save_test.rs`:

```rust
//! Device Save from the dev UI: the same write the admin PATCH and the MCP
//! tool make, behind the dev session instead of the bearer token.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

fn config() -> &'static str {
    "\
admin:
  token: secret
devices:
  kitchen:
    screen: byonk-builtin/default
    dither: floyd-steinberg
"
}

#[tokio::test]
async fn saving_writes_the_final_rung_and_plain_fields() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(config(), dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"final":{"dither":"atkinson"},"name":"Kitchen","min_png_bytes":102401}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let d = &doc["devices"]["kitchen"];
    assert_eq!(d["final"]["dither"], "atkinson");
    assert_eq!(d["dither"], "floyd-steinberg", "the base rung is untouched");
    assert_eq!(d["name"], "Kitchen");
    assert_eq!(d["min_png_bytes"], 102401);
}

#[tokio::test]
async fn clear_removes_a_base_field() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(config(), dir.path());
    let r = app
        .patch_json("/dev/api/devices/kitchen", &[AUTH], r#"{"clear":["dither"]}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert!(doc["devices"]["kitchen"].get("dither").is_none());
}

#[tokio::test]
async fn an_unknown_device_is_a_404_and_a_bad_screen_a_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(config(), dir.path());
    let r = app
        .patch_json("/dev/api/devices/nosuch", &[AUTH], r#"{"name":"x"}"#)
        .await;
    assert_eq!(r.status, axum::http::StatusCode::NOT_FOUND);
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"screen":"local/does-not-exist"}"#,
        )
        .await;
    assert!(r.status.is_client_error(), "{}", r.status);
}

fn device_of(path: &std::path::Path, key: &str) -> serde_yaml::Value {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc["devices"][key].clone()
}

fn rich_config() -> &'static str {
    "\
admin:
  token: secret
devices:
  kitchen:
    screen: byonk-builtin/default
    gamut:
      knee: 0.5
      amount: 0.9
    final:
      dither: floyd-steinberg
      strength: 0.4
"
}

/// Ruling C3: clear first, then set -- a key both cleared and set ends up
/// set, so a whole rung can be replaced in one atomic request.
#[tokio::test]
async fn clear_and_set_of_the_same_key_ends_up_set() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(rich_config(), dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"clear":["final"],"final":{"dither":"atkinson"}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let d = device_of(&config_path, "kitchen");
    assert_eq!(d["final"]["dither"], "atkinson");
    assert!(d["final"].get("strength").is_none(), "the cleared rung kept strength: {d:?}");
}

#[tokio::test]
async fn one_gamut_sub_field_can_be_cleared_and_the_others_stay() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(rich_config(), dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"clear":["gamut.knee","final.strength"],"final":{"dither":"stucki"}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let d = device_of(&config_path, "kitchen");
    assert!(d["gamut"].get("knee").is_none(), "{d:?}");
    assert_eq!(d["gamut"]["amount"].as_f64(), Some(0.9));
    assert_eq!(d["final"]["dither"], "stucki");
    assert!(d["final"].get("strength").is_none(), "{d:?}");
}

#[tokio::test]
async fn clearing_and_setting_params_replaces_them_wholesale() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "\
admin:
  token: secret
devices:
  kitchen:
    screen: byonk-builtin/default
    params:
      a: 1
      b: 2
";
    let (app, config_path) = TestApp::new_with_config_yaml(yaml, dir.path());
    let r = app
        .patch_json(
            "/dev/api/devices/kitchen",
            &[AUTH],
            r#"{"clear":["params"],"params":{"c":3}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let d = device_of(&config_path, "kitchen");
    assert_eq!(d["params"]["c"], 3);
    assert!(d["params"].get("a").is_none(), "{d:?}");
}

/// The admin PATCH shares `apply_device_patch`, so it follows the same rule.
#[tokio::test]
async fn the_admin_patch_follows_the_same_clear_then_set_rule() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(rich_config(), dir.path());
    let r = app
        .patch_json(
            "/api/admin/devices/kitchen",
            &[AUTH],
            r#"{"clear":["final.dither"],"final":{"dither":"atkinson"}}"#,
        )
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert_eq!(device_of(&config_path, "kitchen")["final"]["dither"], "atkinson");
}

#[tokio::test]
async fn saving_drops_the_device_override() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(config(), dir.path());
    app.put_json(
        "/dev/api/overrides/devices/kitchen",
        &[AUTH],
        r#"{"strength":0.5}"#,
    )
    .await;
    let r = app
        .patch_json("/dev/api/devices/kitchen", &[AUTH], r#"{"strength":0.5}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let v: serde_json::Value = app
        .get_with_headers("/dev/api/overrides", &[AUTH])
        .await
        .json();
    assert_eq!(v["devices"], serde_json::json!([]));
}

#[tokio::test]
async fn saving_on_an_embedded_config_is_a_conflict() {
    let app = TestApp::new_with_dev_login(Some("secret"), "u", "p");
    let r = app
        .patch_json("/dev/api/devices/kitchen", &[AUTH], r#"{"name":"x"}"#)
        .await;
    assert_eq!(r.status, axum::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn the_route_needs_a_dev_login() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(config(), dir.path());
    let r = app
        .patch_json("/dev/api/devices/kitchen", &[], r#"{"name":"x"}"#)
        .await;
    assert_eq!(r.status, axum::http::StatusCode::UNAUTHORIZED);
}
```

(If `byonk-builtin/default`'s `meta.yaml` declares a params schema that rejects `a`/`b`/`c`, use names it declares; the assertion is about wholesale replacement, not the names.)

In `src/api/admin/write.rs` `mod tests`, replace `a_final_field_cannot_be_set_and_cleared_in_one_request` — it pins exactly the refusal ruling C3 removes — with:

```rust
    /// Ruling C3: setting and clearing one field in a request is no longer a
    /// refusal; `apply_device_patch` clears first and sets second, so the set
    /// wins. `resolve_clear` only validates names now.
    #[test]
    fn a_final_field_set_and_cleared_in_one_request_is_accepted() {
        let body = DeviceWrite {
            final_rung: Some(CascadeValues {
                dither: Some("atkinson".into()),
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        assert_eq!(
            resolve_clear(&with_clear(body, &["final.dither"])).unwrap(),
            vec!["final.dither".to_string()]
        );
    }

    #[test]
    fn gamut_sub_fields_are_clearable_on_both_rungs_and_nothing_else_is() {
        resolve_clear(&with_clear(
            device_write_for_test(),
            &["gamut.knee", "gamut.amount", "final.gamut.max_compression"],
        ))
        .expect("gamut sub-fields are clearable");
        for bad in ["gamut.nope", "final.gamut.nope", "gamut."] {
            resolve_clear(&with_clear(device_write_for_test(), &[bad])).expect_err(bad);
        }
    }

    #[test]
    fn clearing_a_gamut_sub_field_leaves_its_siblings() {
        let mut w = DeviceWrite {
            gamut: Some(GamutTuningValues {
                knee: Some(0.5),
                amount: Some(0.9),
                ..Default::default()
            }),
            ..device_write_for_test()
        };
        apply_clear(&mut w, &["gamut.knee".to_string()]);
        assert_eq!(w.gamut.as_ref().unwrap().knee, None);
        assert_eq!(w.gamut.as_ref().unwrap().amount, Some(0.9));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test dev_device_save_test && cargo test --lib admin::write`
Expected: FAIL — 405 on the dev route; the admin-route and unit tests fail on the set+clear refusal and on `gamut.knee` being an unknown clear name.

- [ ] **Step 3: Clear first, then set, in `apply_device_patch`**

In `src/api/admin/write.rs`:

1. Add, beside `CASCADE_FIELDS`:

```rust
/// Single gamut knobs, clearable on their own (ruling C3) so one can be
/// removed without restating the others.
const GAMUT_SUBFIELDS: [&str; 3] = ["gamut.knee", "gamut.amount", "gamut.max_compression"];

fn clear_gamut_sub(g: &mut GamutTuningValues, name: &str) {
    match name {
        "gamut.knee" => g.knee = None,
        "gamut.amount" => g.amount = None,
        "gamut.max_compression" => g.max_compression = None,
        _ => {}
    }
}
```

2. In `resolve_clear`, make `acceptable`

```rust
        let acceptable = CLEARABLE_DEVICE_KEYS.contains(&name.as_str())
            || GAMUT_SUBFIELDS.contains(&name.as_str())
            || name
                .strip_prefix("final.")
                .is_some_and(|f| is_cascade_field(f) || GAMUT_SUBFIELDS.contains(&f));
```

delete the whole `let also_set = …; if also_set { … }` block, and replace the doc comment's second paragraph with: "A name may also be set in the same request: `apply_device_patch` clears first and sets second, so the set wins (ruling C3)." Delete `cascade_field_is_set`, now unused. In the error message add `gamut.<knob>` to the list: `"cannot clear `{name}`{hint}; clearable: {}, gamut.knee, gamut.amount, gamut.max_compression, or `final.<field>` for one of: {}"`.

3. `clear_cascade_field`: add an arm `sub if GAMUT_SUBFIELDS.contains(&sub) => clear_gamut_sub(&mut v.gamut, sub),` before the catch-all.

4. `apply_clear`: add an arm before `dotted if dotted.starts_with("final.")`:

```rust
            sub if GAMUT_SUBFIELDS.contains(&sub) => {
                if let Some(g) = merged.gamut.as_mut() {
                    clear_gamut_sub(g, sub);
                }
            }
```

5. `apply_device_patch`: after `existing` is loaded, build the existing entry as a write and clear it before merging:

```rust
    // Ruling C3: clear the existing entry first, then merge the body's sets
    // over what is left. A key both cleared and set therefore ends up set,
    // and the whole change is one write under one lock.
    let mut base = DeviceWrite {
        key: Some(key.to_string()),
        screen: Some(existing.screen.clone()),
        panel: existing.panel.clone(),
        dither: existing.dither.clone(),
        colors: existing.colors.clone(),
        colors_actual: existing.colors_actual.clone(),
        gamut: Some(existing.gamut.clone()),
        final_rung: existing.final_rung.clone(),
        params: Some(existing.params.clone()),
        refresh: existing.refresh,
        name: existing.name.clone(),
        max_error: existing.max_error,
        noise_scale: existing.noise_scale,
        chroma_clamp: existing.chroma_clamp,
        strength: existing.strength,
        temperature_profile: existing.temperature_profile.clone(),
        maximum_compatibility: existing.maximum_compatibility,
        min_png_bytes: existing.min_png_bytes,
        clear: None,
    };
    apply_clear(&mut base, &clear);
    let base_params = base.params.clone().unwrap_or_default();
```

Then in the params block and the `merged` struct read every `existing.<field>` from `base` instead (`existing.params` → `base_params`, `existing.panel.clone()` → `base.panel.clone()`, …), with these three spelled out:

```rust
        gamut: Some(
            body.gamut
                .clone()
                .unwrap_or_default()
                .or(&base.gamut.clone().unwrap_or_default()),
        ),
        final_rung: merge_final(body.final_rung.as_ref(), base.final_rung.as_ref()),
        params: Some(params),
```

and **delete** the later `apply_clear(&mut merged, &clear);` line — the clear already happened. `screen` still comes from `body.screen.clone().unwrap_or(existing.screen.clone())` (`screen` is never clearable).

6. `src/mcp/tools_device.rs`: replace the sentence "A field cannot be set and cleared in one request." in the `clear` doc comment with "A field may be cleared and set in one request: the clear applies first, so the new value wins. Single gamut knobs clear as `gamut.knee`, `gamut.amount`, `gamut.max_compression` (and `final.gamut.<knob>`)." — then run `cargo test --test mcp_*` (the tool schema snapshot, if one exists, picks up the doc change; update it only for this sentence).

- [ ] **Step 4: The dev route**

In `src/api/dev.rs`:

```rust
/// `PATCH /dev/api/devices/{key}`: Save a device draft. The same write as the
/// admin PATCH and the MCP tool — `apply_device_patch`, with its screen,
/// params, clear and read-only-config guards (controller ruling R-E) — then
/// drop the device's live override, which the config now carries (P1-9).
pub async fn handle_save_device(
    State(state): State<AppState>,
    axum::extract::Path(key): axum::extract::Path<String>,
    Json(body): Json<crate::api::admin::write::DeviceWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let out = crate::api::admin::write::apply_device_patch(&state, &key, body).await?;
    state.dev_overrides.devices.remove(&key).await;
    Ok(Json(out))
}
```

In `dev_router` change `use axum::routing::{get, post, put};` to `use axum::routing::{get, patch, post, put};` and add `.route("/devices/{key}", patch(handle_save_device))`.

If `crate::api::admin::write` is not reachable from `dev.rs` (check `src/api/admin/mod.rs` for `pub mod write`), make it `pub(crate) mod write`.

- [ ] **Step 5: Run the tests**

Run: `cargo test --test dev_device_save_test && cargo test --lib admin::write && cargo test --test admin_devices_test && cargo test --workspace mcp`
Expected: PASS. Any other existing test that expected a 400 for "both set and cleared" must be rewritten to expect the set to win, with a comment citing ruling C3 — none exists at HEAD besides the unit test replaced above (`grep -rn "both set and cleared" tests src` confirms).

- [ ] **Step 6: Commit**

```bash
git add src/api/admin/write.rs src/mcp/tools_device.rs src/api/dev.rs tests/dev_device_save_test.rs
git commit -m "feat(dev): device Save as one atomic PATCH -- clear first, then set"
```

(Add `src/api/admin/mod.rs` to the `git add` only if Step 3 changed it.)

---

### Task 6: Thicker panel and device payloads

**Files:**
- Modify: `src/api/dev.rs` — `DeviceInfo` (`:161-167`), `PanelInfo` (`:170-178`), `handle_screens` (`:282-305`)
- Create: `tests/dev_screens_payload_test.rs`

**Interfaces:**
- Produces (`GET /dev/api/screens`, consumed by Plan 2's `configDraft`):
  ```json
  "devices": [{
    "id": "kitchen", "screen": "local/hello", "panel": "e1004",
    "name": "Kitchen", "params": {"greeting": "hi"},
    "base": {CascadeValues, every key present, absent = null},
    "final": {CascadeValues} | null,
    "temperature_profile": "a" | null, "maximum_compatibility": true | null, "min_png_bytes": 102401 | null
  }],
  "panels": [{
    "id": "e1004", "name": "…", "width": 800, "height": 480,
    "colors": "…", "colors_actual": "…" | null,
    "dither": { flat PanelDitherConfig } | null
  }]
  ```
  Both lists sorted by `id`. `DeviceInfo.dither` is gone (P1-10).

- [ ] **Step 1: Write the failing test**

Create `tests/dev_screens_payload_test.rs`:

```rust
//! `GET /dev/api/screens` carries every field the dev UI's config editor
//! edits, so a panel or device draft starts from the configured values.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn devices_and_panels_carry_their_rungs_and_plain_fields() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "\
admin:
  token: secret
panels:
  zeta:
    name: Zeta
    colors: '#000000,#FFFFFF'
  e1004:
    name: reTerminal E1004
    colors: '#000000,#FFFFFF'
    colors_actual: '#101010,#E8E8E8'
    dither:
      strength: 0.8
      atkinson:
        noise_scale: 2.0
devices:
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
    name: Kitchen
    dither: floyd-steinberg
    temperature_profile: a
    maximum_compatibility: true
    min_png_bytes: 102401
    params:
      greeting: hi
    final:
      dither: atkinson
  attic:
    screen: byonk-builtin/default
";
    let (app, _) = TestApp::new_with_config_yaml(yaml, dir.path());
    let v: serde_json::Value = app
        .get_with_headers("/dev/api/screens", &[AUTH])
        .await
        .json();

    let ids: Vec<&str> = v["devices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["attic", "kitchen"], "sorted by id");
    let k = &v["devices"][1];
    assert_eq!(k["name"], "Kitchen");
    assert_eq!(k["params"]["greeting"], "hi");
    assert_eq!(k["base"]["dither"], "floyd-steinberg");
    assert_eq!(k["final"]["dither"], "atkinson");
    assert_eq!(k["temperature_profile"], "a");
    assert_eq!(k["maximum_compatibility"], true);
    assert_eq!(k["min_png_bytes"], 102401);
    assert!(k.get("dither").is_none(), "base.dither replaces it");
    assert!(v["devices"][0]["final"].is_null());

    let pids: Vec<&str> = v["panels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(pids, ["e1004", "zeta"]);
    let p = &v["panels"][0];
    assert_eq!(p["dither"]["strength"].as_f64(), Some(0.8_f32 as f64));
    assert_eq!(p["dither"]["atkinson"]["noise_scale"].as_f64(), Some(2.0));
    assert!(v["panels"][1]["dither"].is_null());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --test dev_screens_payload_test`
Expected: FAIL (no `name`, unsorted, …).

- [ ] **Step 3: Implement**

Replace `DeviceInfo` and `PanelInfo` in `src/api/dev.rs`:

```rust
/// A configured device: everything the dev UI's device editor shows.
#[derive(Debug, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub screen: String,
    pub panel: Option<String>,
    pub name: Option<String>,
    pub params: HashMap<String, serde_json::Value>,
    /// The base rung (what is written directly under the device).
    pub base: CascadeValues,
    /// The `final:` rung, the owner's veto.
    #[serde(rename = "final")]
    pub final_rung: Option<CascadeValues>,
    pub temperature_profile: Option<String>,
    pub maximum_compatibility: Option<bool>,
    pub min_png_bytes: Option<u32>,
}

/// A panel profile: everything the dev UI's panel editor shows.
#[derive(Debug, Serialize)]
pub struct PanelInfo {
    pub id: String,
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub colors: String,
    pub colors_actual: Option<String>,
    /// The panel's dither block, in the flat form config.yaml uses.
    pub dither: Option<PanelDitherConfig>,
}
```

In `handle_screens`:

```rust
    let mut devices: Vec<DeviceInfo> = config
        .devices
        .iter()
        .map(|(id, dev)| DeviceInfo {
            id: id.clone(),
            screen: dev.screen.clone(),
            panel: dev.panel.clone(),
            name: dev.name.clone(),
            params: yaml_params_to_json(&dev.params),
            base: dev.base(),
            final_rung: dev.final_rung.clone(),
            temperature_profile: dev.temperature_profile.clone(),
            maximum_compatibility: dev.maximum_compatibility,
            min_png_bytes: dev.min_png_bytes,
        })
        .collect();
    devices.sort_by(|a, b| a.id.cmp(&b.id));

    let mut panels: Vec<PanelInfo> = config
        .panels
        .iter()
        .map(|(id, panel)| PanelInfo {
            id: id.clone(),
            name: panel.name.clone(),
            width: panel.width,
            height: panel.height,
            colors: panel.colors.clone(),
            colors_actual: panel.colors_actual.clone(),
            dither: panel.dither.clone(),
        })
        .collect();
    panels.sort_by(|a, b| a.id.cmp(&b.id));
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --test dev_screens_payload_test --test dev_panel_save_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/api/dev.rs tests/dev_screens_payload_test.rs
git commit -m "feat(dev): panel and device payloads carry every editable field"
```

---

### Task 7: Which devices use a panel

**Files:**
- Modify: `src/server.rs` — `Sighting`, `DevOverrides::sightings`
- Modify: `src/api/display.rs` — record the sighting in `handle_display` (after the `device_entry_key` block, `:678-713`); new `resolve_device_panel` + `PanelResolution` beside `resolve_panel` (`:339`)
- Modify: `src/api/dev.rs` — `handle_panel_usage`, route; `handle_render`'s `panel_key`
- Create: `tests/dev_panel_usage_test.rs`

**Interfaces:**
- Produces:
  - `pub struct Sighting { pub board: Option<String>, pub model: String }`; `DevOverrides.sightings: Arc<RwLock<HashMap<String, Sighting>>>` keyed by device config key. In memory, never persisted.
  - `pub enum PanelResolution { Panel(String), NoPanel, Unseen }`; `pub fn resolve_device_panel(config: &AppConfig, device: &DeviceConfig, sighting: Option<&Sighting>) -> PanelResolution`.
  - `GET /dev/api/panel-usage` →
    ```json
    { "panels": { "e1004": ["attic","hallway","kitchen"], "zeta": [] }, "unseen": ["office"] }
    ```
    every configured panel present (sorted map, sorted lists); `unseen` = configured devices with no explicit `panel:` that have not polled since byonk started.

- [ ] **Step 1: Write the failing tests**

Create `tests/dev_panel_usage_test.rs`:

```rust
//! The server answers "which devices use this panel", because a device
//! without `panel:` resolves through the Board/Model headers it polls with,
//! which the dev UI cannot see (spec §5, controller ruling R-F).

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

fn app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    let yaml = "\
admin:
  token: secret
panels:
  e1004:
    name: reTerminal E1004
    match: e1004
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
  other:
    name: Other
    colors: '#000000,#FFFFFF'
devices:
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
  office:
    screen: byonk-builtin/default
  attic:
    screen: byonk-builtin/default
  garage:
    screen: byonk-builtin/default
    panel: other
";
    TestApp::new_with_config_yaml(yaml, dir)
}

async fn poll(app: &TestApp, id: &str, model: &str, board: Option<&str>) {
    let key = app.register_device(id).await;
    let mut h: Vec<(&str, String)> = common::fixtures::display_headers(id, &key);
    h.retain(|(k, _)| *k != "Model");
    h.push(("Model", model.to_string()));
    if let Some(b) = board {
        h.push(("Board", b.to_string()));
    }
    let r = app
        .get_with_headers("/api/display", &common::fixtures::as_str_pairs(&h))
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
}

async fn usage(app: &TestApp) -> serde_json::Value {
    let r = app.get_with_headers("/dev/api/panel-usage", &[AUTH]).await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    r.json()
}

#[tokio::test]
async fn before_any_poll_only_explicit_panels_count_and_the_rest_are_unseen() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let u = usage(&app).await;
    assert_eq!(u["panels"]["e1004"], serde_json::json!(["kitchen"]));
    assert_eq!(u["panels"]["other"], serde_json::json!(["garage"]));
    assert_eq!(u["unseen"], serde_json::json!(["attic", "office"]));
}

#[tokio::test]
async fn a_poll_resolves_a_device_by_board_or_by_model() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    poll(&app, "office", "og", Some("e1004")).await;
    poll(&app, "attic", "e1004", None).await;
    let u = usage(&app).await;
    assert_eq!(
        u["panels"]["e1004"],
        serde_json::json!(["attic", "kitchen", "office"])
    );
    assert_eq!(u["unseen"], serde_json::json!([]));
}

#[tokio::test]
async fn a_seen_device_matching_no_panel_is_neither_counted_nor_unseen() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    poll(&app, "office", "og", Some("something-else")).await;
    let u = usage(&app).await;
    assert_eq!(u["panels"]["e1004"], serde_json::json!(["kitchen"]));
    assert_eq!(u["unseen"], serde_json::json!(["attic"]));
}

#[tokio::test]
async fn the_dev_render_resolves_a_board_matched_device_after_it_polled() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let header = |r: &common::app::TestResponse| -> serde_json::Value {
        serde_json::from_str(r.headers["x-byonk-resolution"].to_str().unwrap()).unwrap()
    };
    let before = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"office"}"#)
        .await;
    assert_eq!(header(&before)["colors"]["from"], "fallback");
    poll(&app, "office", "og", Some("e1004")).await;
    let after = app
        .post_json("/dev/api/render", &[AUTH], r#"{"device":"office"}"#)
        .await;
    assert_eq!(header(&after)["colors"]["from"], "panel");
}
```

(If `common::fixtures::display_headers`'s element type differs, adapt the `retain`/`push` to it — it is `Vec<(&'static str, String)>` at HEAD.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test dev_panel_usage_test`
Expected: FAIL (route 404/405; render says `fallback` after the poll).

- [ ] **Step 3: Implement the sighting**

In `src/server.rs`:

```rust
/// What a configured device last told byonk about its hardware. Kept so the
/// dev UI can be told which devices resolve to a panel through `match:` —
/// the headers are only known at poll time (controller ruling R-F). In
/// memory only: a restart forgets it, and the UI says so.
#[derive(Debug, Clone, PartialEq)]
pub struct Sighting {
    pub board: Option<String>,
    pub model: String,
}
```

and add to `DevOverrides`:

```rust
    /// device_config_key → the Board/Model headers of its last poll. Not an
    /// override: dev-UI bookkeeping `/api/display` feeds.
    pub sightings: Arc<RwLock<HashMap<String, Sighting>>>,
```

In `src/api/display.rs` `handle_display`, right after the `(device_config, device_entry_key)` block:

```rust
    if let Some(ref key) = device_entry_key {
        dev_overrides.sightings.write().await.insert(
            key.clone(),
            crate::server::Sighting {
                board: board_header.clone(),
                model: model_str.to_string(),
            },
        );
    }
```

Beside `resolve_panel`:

```rust
/// Where a configured device's panel comes from without the device in hand.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelResolution {
    /// The panel config key.
    Panel(String),
    /// Resolvable, and it resolves to no panel.
    NoPanel,
    /// No explicit `panel:` and not seen polling since byonk started.
    Unseen,
}

/// The panel `/api/display` would pick for `device`: its explicit `panel:`,
/// else the Board header and then the Model header of its last poll.
pub fn resolve_device_panel(
    config: &AppConfig,
    device: &crate::models::DeviceConfig,
    sighting: Option<&crate::server::Sighting>,
) -> PanelResolution {
    let (board, model) = match (&device.panel, sighting) {
        (Some(_), _) => (None, ""),
        (None, Some(s)) => (s.board.as_deref(), s.model.as_str()),
        (None, None) => return PanelResolution::Unseen,
    };
    match resolve_panel(config, device.panel.as_deref(), board, model).1 {
        Some(key) => PanelResolution::Panel(key),
        None => PanelResolution::NoPanel,
    }
}
```

(`resolve_panel` returns early on an explicit panel, so the empty model string is never matched.)

- [ ] **Step 4: The endpoint and the dev render**

In `src/api/dev.rs`:

```rust
/// `GET /dev/api/panel-usage`.
#[derive(Debug, Serialize)]
pub struct PanelUsageResponse {
    /// Every configured panel → the device keys resolving to it.
    pub panels: std::collections::BTreeMap<String, Vec<String>>,
    /// Devices without `panel:` that have not polled since byonk started.
    pub unseen: Vec<String>,
}

/// Which devices use which panel, as `/api/display` would resolve them.
pub async fn handle_panel_usage(State(state): State<AppState>) -> Json<PanelUsageResponse> {
    use crate::api::display::{resolve_device_panel, PanelResolution};
    let config = state.config.load();
    let sightings = state.dev_overrides.sightings.read().await;
    let mut panels: std::collections::BTreeMap<String, Vec<String>> = config
        .panels
        .keys()
        .map(|k| (k.clone(), Vec::new()))
        .collect();
    let mut unseen = Vec::new();
    for (key, device) in &config.devices {
        match resolve_device_panel(&config, device, sightings.get(key)) {
            PanelResolution::Panel(p) => panels.entry(p).or_default().push(key.clone()),
            PanelResolution::Unseen => unseen.push(key.clone()),
            PanelResolution::NoPanel => {}
        }
    }
    for list in panels.values_mut() {
        list.sort();
    }
    unseen.sort();
    Json(PanelUsageResponse { panels, unseen })
}
```

Route: `.route("/panel-usage", get(handle_panel_usage))`.

In `handle_render`, replace the `panel_key` binding from Task 2 with (plan ruling P1-6):

```rust
    // Explicit request panel, else the panel `/api/display` would resolve for
    // this device — remembered Board/Model included (plan ruling P1-6).
    let panel_key: Option<String> = match req.panel.clone() {
        Some(p) => Some(p),
        None => match device_key
            .as_deref()
            .and_then(|k| config.devices.get(k).map(|d| (k, d)))
        {
            Some((k, d)) => {
                let sightings = state.dev_overrides.sightings.read().await;
                match display::resolve_device_panel(&config, d, sightings.get(k)) {
                    display::PanelResolution::Panel(p) => Some(p),
                    _ => None,
                }
            }
            None => None,
        },
    }
    .filter(|k| config.get_panel(k).is_some());
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --test dev_panel_usage_test --test dev_render_provenance_test --test dev_overrides_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/server.rs src/api/display.rs src/api/dev.rs tests/dev_panel_usage_test.rs
git commit -m "feat(dev): which devices use a panel, including those matched by Board or Model"
```

---

### Task 8: Document the server contract, and verify the whole plan

**Files:**
- Modify: `docs/src/guide/dev-mode.md` — `## The /dev/api endpoints` table (`:496-522`), `### Rendering a preview` (`:524-` the `draft`/refusal paragraphs)

**Interfaces:**
- Consumes: every route and payload from Tasks 1–7.

- [ ] **Step 1: Endpoints table**

Replace the two `panel-colors` rows with:

```markdown
| `GET /dev/api/overrides` | Every live override and tuning session, with the seconds each has left |
| `PUT /dev/api/overrides/panels/{panel}`, `DELETE …` | Push, and clear, a panel's temporary override: `colors_actual`, `dither`, the tuning numbers and `gamut`, applied to every device on that panel |
| `PUT /dev/api/overrides/devices/{device}`, `DELETE …` | Push, and clear, one device's temporary override: everything but `colors` and `refresh` |
| `POST /dev/api/panels/{panel}/save` | Write a panel's `colors_actual` and `dither` block into `config.yaml`, replacing both — in place, key by key, so comments and the order of panels survive — and clear its override |
| `PATCH /dev/api/devices/{key}` | Save a device: the same body and rules as `PATCH /api/admin/devices/{key}` — `clear` applies first, then the set fields, so one request can replace a value — then clear its override |
| `GET /dev/api/panel-usage` | Which devices use each panel, and which have not been seen since byonk started |
```

- [ ] **Step 2: "Rendering a preview"**

In the example body add `"final": { "dither": "atkinson" }` under `draft` and `"params": { "greeting": "hi" }`. In the field table replace the `draft.base` row and add rows:

```markdown
| `draft.base` | An unsaved device base rung: `colors`, `colors_actual`, `dither`, the tuning numbers, `gamut` and `refresh`. |
| `draft.final` | An unsaved `final:` rung, same fields. |
| `draft.params` | Replaces the device's params; `simulated.params` are still merged on top. |
```

Replace the paragraph starting "Three fields are refused with `400`." with:

```markdown
Two fields are refused with `400`: `override.colors`, because the script would
then have been shown a different palette from the one rendered, and
`override.refresh`, because nothing on this path consumes it. An unknown
field is refused with `422`.

The `override` rung of a preview also carries whatever a Push made live — the
device's override over its panel's, with the request's own `override` on top —
so the resolution report shows a live Push as the winner it is on the glass.
```

Add a subsection after "Rendering a preview":

```markdown
### Overrides

A Push puts a draft on real hardware without writing `config.yaml`: it becomes
the cascade's `override` rung for every device on that panel (a panel
override) or for one device (a device override). When both exist, the
device's values win field by field. Each override expires 30 minutes after it
was last pushed, so a closed browser tab cannot leave a display showing an
uncommitted draft; Discard clears it at once, and a restart clears all of
them. Saving a panel or a device clears its override, since the configuration
now says the same thing.

A device without `panel:` is matched to a panel by the `Board` or `Model`
header it polls with. Byonk remembers the last ones in memory, which is how
`/dev/api/panel-usage` and a preview of that device know its panel; until the
device has polled once since byonk started, it is reported as unseen.
```

- [ ] **Step 3: Full verification, in order**

```bash
make webui
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd docs && mdbook build)
make webui-test
```

Expected: all green (re-run `cargo test --workspace` once if only `test_display_content_hash_is_deterministic` aborted). Confirm `git diff 5d6323c -- tests/dev_auth_test.rs` is empty.

- [ ] **Step 4: Commit**

```bash
git add docs/src/guide/dev-mode.md
git diff --cached --stat
git commit -m "docs(dev): overrides, panel and device Save, panel usage, the draft wire format"
```
