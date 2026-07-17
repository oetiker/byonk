# Reserved DEFAULT Device Implementation Plan (Plan B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace byonk's two overlapping global "what does an unconfigured device show" settings — `AppConfig.default_screen` and `RegistrationConfig.screen` — with a single reserved `DEFAULT` device (`devices["DEFAULT"]`), and surface it in the HA integration as a normal device with a live screen-select.

**Architecture:** A reserved device key `"DEFAULT"` lives in `config.devices` like any other device. Screen resolution for every not-yet-configured device (unregistered *and* registered-but-unassigned) becomes `device.screen → DEFAULT.screen → built-in fallback`. Because byonk already threads `device_context.registration_code` into the screen script, the DEFAULT screen template renders the pairing code for unregistered devices and normal content otherwise (the shipped `byonk-builtin/default` screen is already registration-aware). The DEFAULT device is written/read over the normal per-device admin API (live, allowed in add-on mode), so the integration presents it with the standard screen-select. `registration.enabled` is kept. This is a **core model change** — it applies to standalone byonk too, not just the add-on.

**Tech Stack:** Rust (axum, serde_yaml) for byonk; Python (Home Assistant custom component, pytest) for the integration; mdBook for docs.

## Global Constraints

- **`make check`** (fmt + clippy `-D warnings` + tests) must pass after every Rust task. **Any change to `homeassistant/byonk/config.yaml`** additionally requires `make check` (asserted by `tests/addon_manifest_test.rs`) — but note **this plan does not touch the add-on manifest** (the DEFAULT device is not an add-on option; it flows over the per-device admin API).
- **Integration:** `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q` must pass after every Python task (baseline: 69 passing).
- **No `git add -A` / `git add .`** — this repo has pre-existing untracked local files. Always `git add <explicit paths>` and verify `git diff --cached` before committing.
- **Reserved key string is `"DEFAULT"`** everywhere (Rust const `RESERVED_DEFAULT_KEY`, Python const `DEFAULT_DEVICE_KEY`). Never spell it differently.
- **Standalone byonk stays fully writable**; add-on mode keeps per-device writes allowed and global-config writes 409'd (unchanged by this plan).
- Commit at the end of each task with the exact message shown.

---

## Ground-truth reference (verified 2026-07-05, HEAD `2360c3c`)

Line numbers are from this HEAD and may shift as tasks land — match on content, not line number.

- `AppConfig` @ `src/models/config.rs`: `default_screen: Option<String>` field (175-176), `default_screen()` fn (222-224), `registration: RegistrationConfig` (179-180); `RegistrationConfig { enabled (283-284), screen (290-291) }`; `AppConfig::Default` (394-407) sets both; `RegistrationConfig::Default` (302-309). `DeviceConfig { screen: String, ... }` (227-264). `get_device_config` / `get_device_config_for_code` (350-373).
- **`content_pipeline::run_script_for_device`** @ `src/services/content_pipeline.rs:160-203` — registered path; fallback at 190-199 uses `config.default_screen.as_deref().unwrap_or("byonk-builtin/default")`.
- **`content_pipeline::render_registration_screen`** @ `src/services/content_pipeline.rs:464` — renders the 2×5 code SVG. Add the generic/unassigned sibling next to it.
- **Server unregistered path** @ `src/api/display.rs:250-342` — builds `device_ctx` with `registration_code`, `screen_to_use = config.registration.screen` (285-293), else built-in `render_registration_screen` (335-342). Logging at 256 references `config.registration.screen`.
- **CLI unregistered path** @ `src/main.rs:257-296` — `screen_to_use = config.registration.screen.or(config.default_screen)` (261-265), else `render_registration_screen`.
- **`SettingsWrite`** @ `src/api/admin/write.rs:271-278` has `default_screen` + `registration_screen`; `require_writable_settings` gate `touches_global` (56-67) lists both; `patch_settings` validates (298-307) and writes (323-331) both; `delete_package` dangling-ref checks (517-537) reference `default_screen` + `registration.screen`; tests (626-667) build `SettingsWrite` literals.
- **`AdminDevice`** @ `src/api/admin/read.rs:91-112`; `list_devices` pushes seen devices (133-152) and every `config.devices` entry with `registered: true` (158-181) — **so a `DEFAULT` device automatically appears in `GET /devices`, `registered: true`**.
- **Dev UI** `src/api/dev.rs:147` (`ScreensResponse.default_screen`) + `236` (assignment from `state.config.default_screen`).
- **Tests that reference the removed fields:** `src/server.rs:298-320` (`test_config_swap_is_visible`), `src/assets.rs:889` (`content.contains("default_screen:")`), `tests/api_display_test.rs:298-342`, `tests/admin_write_test.rs:89,123,387,429,450`, `tests/admin_packages_test.rs:177-216`, `tests/common/app.rs:244`.
- **Embedded YAML** `default-config.yaml`: `registration:\n  enabled: true` (4-5), `default_screen: byonk-builtin/default` (8-9), `devices: {}` (117-118). **Dev YAML** `config.yaml`: `registration:` (8-12), `devices:` (180), `default_screen: default` (287-288).
- **Shipped DEFAULT screen** `screens/default/script.lua` + `screens/default/screen.svg` — **already registration-aware**: `{% if device.registration_code %}` renders `device.registration_code_hyphenated` in a box (screen.svg:40-42). Resolves as ref `byonk-builtin/default`.
- **Integration:** per-device config entries keyed by `unique_id == MAC`, `data[CONF_DEVICE_KEY]`; hub entry `unique_id == DOMAIN`. `coordinator.py`: `registration_screen()` accessor (50-51, **dead** — only usage was Plan-3's reverted Options Flow; not referenced anywhere else), `_async_reconcile` removes HA entries not in `byonk_registered` and **orphan-prunes byonk devices HA has no entry for** (133-159), `_async_sync_discovery` fires discovery flows from `pending` only (161-195). `config_flow.py` `async_step_integration_discovery` → `async_step_configure` (screen picker) → `async_step_dev_params` → `async_create_entry` (130-217). `select.py` `async_setup_entry` adds screen/dither/panel selects for any device entry (15-28); `ByonkScreenSelect` writes `PATCH /devices/:key` (66-74). `tests_ha/test_settings_entities.py:47` asserts `select.byonk_new_device_screen` is gone (unchanged expectation).

---

# PART A — byonk core (Rust)

Each Part-A task compiles green and keeps `make check` passing. Tasks 1-4 are additive (old fields still present but progressively unused); Task 5 removes the old fields atomically; Tasks 6-8 migrate YAML/tests/API surface.

---

### Task 1: Reserved DEFAULT key + `default_device_screen()` accessor

**Files:**
- Modify: `src/models/config.rs` (add const + method; add tests in the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub const RESERVED_DEFAULT_KEY: &str = "DEFAULT";` and `impl AppConfig { pub fn default_device_screen(&self) -> Option<&str> }` returning `devices["DEFAULT"].screen` when present, else `None`. Later tasks call this instead of `config.default_screen`.

- [ ] **Step 1: Write the failing test**

Add to `src/models/config.rs` `mod tests`:

```rust
    #[test]
    fn test_default_device_screen_accessor() {
        let mut config = AppConfig::default();
        assert_eq!(config.default_device_screen(), None);

        config.devices.insert(
            RESERVED_DEFAULT_KEY.to_string(),
            DeviceConfig {
                screen: "byonk-builtin/default".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            config.default_device_screen(),
            Some("byonk-builtin/default")
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib models::config::tests::test_default_device_screen_accessor`
Expected: FAIL — `RESERVED_DEFAULT_KEY` / `default_device_screen` not found (does not compile).

- [ ] **Step 3: Write minimal implementation**

Add near the top of `src/models/config.rs` (after the `use` lines):

```rust
/// Reserved device key whose `screen` is shown by every not-yet-configured
/// device (unregistered, or registered without its own screen). Replaces the
/// former `default_screen` + `registration.screen` settings.
pub const RESERVED_DEFAULT_KEY: &str = "DEFAULT";
```

Add to `impl AppConfig` (next to `get_device_config`):

```rust
    /// Screen ref of the reserved DEFAULT device, if one is configured.
    ///
    /// This is the fallback for any device without its own screen mapping.
    pub fn default_device_screen(&self) -> Option<&str> {
        self.devices
            .get(RESERVED_DEFAULT_KEY)
            .map(|d| d.screen.as_str())
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib models::config::tests::test_default_device_screen_accessor`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/models/config.rs
git commit -m "feat(config): add reserved DEFAULT device key + default_device_screen() accessor"
```

---

### Task 2: Built-in fallback dispatcher — code vs. generic "unassigned" screen

**Files:**
- Modify: `src/services/content_pipeline.rs` (add two methods next to `render_registration_screen`; add tests)

**Interfaces:**
- Produces:
  - `pub fn render_unassigned_screen(&self, width: u32, height: u32) -> String` — a generic "device not assigned" SVG (no code).
  - `pub fn render_builtin_fallback(&self, code: Option<&str>, width: u32, height: u32) -> String` — dispatches to `render_registration_screen(code, ...)` when `code` is `Some`, else `render_unassigned_screen(...)`. Tasks 3-4 call this as the ultimate fallback.

- [ ] **Step 1: Write the failing test**

The test module is `#[cfg(test)] mod pipeline_tests` (at the bottom of `src/services/content_pipeline.rs`). It already has a helper `fn build_pipeline(disk: HashMap<String, PathBuf>, loader: Arc<AssetLoader>) -> ContentPipeline` that builds a pipeline over `AppConfig::default()`. Use it:

```rust
    #[test]
    fn render_builtin_fallback_with_code_shows_code() {
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let pipeline = build_pipeline(HashMap::new(), loader);
        let svg = pipeline.render_builtin_fallback(Some("ABCDEFGHJK"), 800, 480);
        assert!(svg.contains("A B C D E"), "code row should be rendered");
        assert!(svg.contains("DEVICE REGISTRATION"));
    }

    #[test]
    fn render_builtin_fallback_without_code_is_generic() {
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let pipeline = build_pipeline(HashMap::new(), loader);
        let svg = pipeline.render_builtin_fallback(None, 800, 480);
        assert!(svg.contains("<svg"));
        assert!(!svg.contains("DEVICE REGISTRATION"));
        assert!(svg.to_lowercase().contains("not assigned"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib services::content_pipeline::pipeline_tests`
Expected: FAIL — `render_unassigned_screen` / `render_builtin_fallback` not found.

- [ ] **Step 3: Write minimal implementation**

Add to the `impl ContentPipeline` block, immediately after `render_registration_screen`:

```rust
    /// Render a generic "device not assigned" screen for the ultimate fallback
    /// when no registration code is available (a registered device whose DEFAULT
    /// screen is unset/unresolvable). Unregistered devices get the code screen
    /// via `render_builtin_fallback(Some(code), ..)` instead.
    pub fn render_unassigned_screen(&self, width: u32, height: u32) -> String {
        let scale = (width as f32 / 800.0).min(height as f32 / 480.0);
        let title_font_size = (32.0 * scale).round() as u32;
        let subtitle_font_size = (18.0 * scale).round() as u32;
        let center_x = width / 2;
        let title_y = height / 2 - (title_font_size / 2);
        let subtitle_y = title_y + (title_font_size as f32 * 1.6).round() as u32;
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}">
  <defs>
    <style>
      text {{ text-anchor: middle; font-family: Outfit, sans-serif; }}
      .title {{ font-weight: 700; }}
      .subtitle {{ font-weight: 400; }}
    </style>
  </defs>
  <rect width="{width}" height="{height}" fill="#ffffff"/>
  <rect x="10" y="10" width="{border_width}" height="{border_height}" fill="none" stroke="#000000" stroke-width="4" rx="8"/>
  <text x="{center_x}" y="{title_y}" font-size="{title_font_size}" class="title" fill="#000000">DEVICE NOT ASSIGNED</text>
  <text x="{center_x}" y="{subtitle_y}" font-size="{subtitle_font_size}" class="subtitle" fill="#666666">Assign a screen to the DEFAULT device in byonk.</text>
</svg>"##,
            width = width,
            height = height,
            border_width = width.saturating_sub(20),
            border_height = height.saturating_sub(20),
            center_x = center_x,
            title_y = title_y,
            subtitle_y = subtitle_y,
            title_font_size = title_font_size,
            subtitle_font_size = subtitle_font_size,
        )
    }

    /// Ultimate built-in fallback: show the pairing code when we have one
    /// (unregistered device), otherwise a generic "not assigned" screen.
    pub fn render_builtin_fallback(&self, code: Option<&str>, width: u32, height: u32) -> String {
        match code {
            Some(code) => self.render_registration_screen(code, width, height),
            None => self.render_unassigned_screen(width, height),
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib services::content_pipeline::pipeline_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/content_pipeline.rs
git commit -m "feat(pipeline): add render_unassigned_screen + render_builtin_fallback dispatcher"
```

---

### Task 3: Route registered-but-unassigned fallback through the DEFAULT device

**Files:**
- Modify: `src/services/content_pipeline.rs:190-199` (`run_script_for_device` fallback) + add `build_pipeline_with_config` test helper in `mod pipeline_tests`

**Interfaces:**
- Consumes: `AppConfig::default_device_screen()` (Task 1).
- Produces: no new symbols; behavior change only.

- [ ] **Step 1: Write the failing test**

The existing `build_pipeline` helper hardcodes `AppConfig::default()`. Add a sibling helper `build_pipeline_with_config` in `mod pipeline_tests` that takes the config, so the test can inject a DEFAULT device. Copy `build_pipeline`'s body and parameterize the config:

```rust
    fn build_pipeline_with_config(
        config: crate::models::AppConfig,
        disk: HashMap<String, PathBuf>,
        loader: Arc<AssetLoader>,
    ) -> ContentPipeline {
        let shared: crate::server::SharedConfig = Arc::new(arc_swap::ArcSwap::from(Arc::new(config)));
        let renderer = Arc::new(RenderService::new(&loader).unwrap());
        let cache_root = std::env::temp_dir().join(format!(
            "byonk_pipeline_test_cache_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let pm = PackageManager::new(loader.clone(), shared.clone(), PackageCache::new(cache_root), disk);
        pm.rebuild_loader();
        ContentPipeline::new(shared, loader, renderer, pm).unwrap()
    }

    #[test]
    fn run_script_for_device_falls_back_to_default_device_screen() {
        use crate::models::config::{DeviceConfig, RESERVED_DEFAULT_KEY};
        let mut config = crate::models::AppConfig::default();
        config.devices.insert(
            RESERVED_DEFAULT_KEY.to_string(),
            DeviceConfig { screen: "byonk-builtin/default".to_string(), ..Default::default() },
        );
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let pipeline = build_pipeline_with_config(config, HashMap::new(), loader);
        // A device with no mapping of its own resolves through devices["DEFAULT"].
        let result = pipeline
            .run_script_for_device("00:11:22:33:44:55", None)
            .expect("default device screen should run");
        assert!(!result.screen_name.is_empty());
    }
```

Refactor the original `build_pipeline` to delegate: `build_pipeline(disk, loader)` = `build_pipeline_with_config(AppConfig::default(), disk, loader)` (keeps existing tests unchanged).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib services::content_pipeline::pipeline_tests::run_script_for_device_falls_back_to_default_device_screen`
Expected: FAIL — today the fallback reads `config.default_screen` (which `AppConfig::default()` sets to `byonk-builtin/default`), so it does not consult `devices["DEFAULT"]`. Confirm the DEFAULT-device path is not yet wired before the Step-3 change. (If it passes spuriously because both point at the same screen, temporarily set the DEFAULT device screen to `byonk-builtin/example/hello` and assert `result.screen_name` contains `hello` to force a real distinction.)

- [ ] **Step 3: Write minimal implementation**

In `src/services/content_pipeline.rs`, replace the fallback block (currently 190-199):

```rust
        // Fall back to the default screen ref with empty params.
        let default_ref = config
            .default_screen
            .as_deref()
            .unwrap_or("byonk-builtin/default");
```

with:

```rust
        // Fall back to the reserved DEFAULT device's screen with empty params.
        let default_ref = config
            .default_device_screen()
            .unwrap_or("byonk-builtin/default");
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib services::content_pipeline::pipeline_tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/content_pipeline.rs
git commit -m "feat(pipeline): route registered-unassigned fallback through DEFAULT device"
```

---

### Task 4: Route unregistered paths (server + CLI) through the DEFAULT device

**Files:**
- Modify: `src/api/display.rs:256` (logging) and `285-342` (`screen_to_use` + built-in fallback)
- Modify: `src/main.rs:261-294` (`screen_to_use` + built-in fallback)
- Modify: `tests/api_display_test.rs:298-342` (update the "despite default_screen" test to the new behavior)

**Interfaces:**
- Consumes: `AppConfig::default_device_screen()` (Task 1), `render_builtin_fallback` (Task 2).

- [ ] **Step 1: Update the failing test to the new expected behavior**

The current test `test_unregistered_device_shows_registration_screen_despite_default_screen` (tests/api_display_test.rs:298-342) asserts the served screen is the built-in `_registration` even when a default screen is set. Under the new model, an unregistered device is served the **DEFAULT device's screen** (`byonk-builtin/default`), which is registration-aware and **still shows the code**. Rewrite the assertion to reflect this:

- Configure the test app with `devices: { DEFAULT: { screen: byonk-builtin/default } }`, `registration.enabled: true`, and an unregistered device request.
- Assert the response is a successful render (200) and that the served content contains the device's registration code (the DEFAULT template renders `device.registration_code_hyphenated`). Rename the test to `test_unregistered_device_shows_default_device_screen_with_code`.

Match the existing test's request-construction helper in `tests/common/app.rs`; keep the same headers/flow, only change the config and the assertion. Show the code is present by asserting the rendered SVG/PNG pipeline returns 200 and (if the harness exposes the screen name) `screen_name == "byonk-builtin/default"`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test api_display_test test_unregistered_device_shows_default_device_screen_with_code`
Expected: FAIL — today the unregistered branch ignores `default_device_screen` and renders `_registration`.

- [ ] **Step 3: Write minimal implementation**

**`src/api/display.rs`:** at line ~256 remove the now-invalid `custom_screen = config.registration.screen.as_deref(),` field from the `tracing::info!` call (drop that one line). Then replace the `screen_to_use` block (289-293):

```rust
            let screen_to_use = config
                .registration
                .screen
                .as_deref()
                .filter(|s| !s.is_empty());
```

with:

```rust
            // The reserved DEFAULT device's screen (registration-aware via
            // device_context.registration_code). No DEFAULT screen -> built-in.
            let screen_to_use = config.default_device_screen().filter(|s| !s.is_empty());
```

Then replace the two built-in fallbacks in this block that call `content_pipeline.render_registration_screen(code, width, height)` (at ~314-315, ~329, and the final `else` at ~338) with `content_pipeline.render_builtin_fallback(Some(code), width, height)` (keep `code` in scope; the screen name label `"_registration"` stays as-is for the cache key).

**`src/main.rs`:** replace the `screen_to_use` block (261-265):

```rust
        let screen_to_use = config
            .registration
            .screen
            .as_deref()
            .or(config.default_screen.as_deref());
```

with:

```rust
        let screen_to_use = config.default_device_screen();
```

and replace each `content_pipeline.render_registration_screen(code, display_spec.width, display_spec.height)` in this block (276-280, 282-286, 289-293) with `content_pipeline.render_builtin_fallback(Some(code), display_spec.width, display_spec.height)`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test api_display_test && cargo build`
Expected: PASS + clean build (old `default_screen`/`registration.screen` fields still exist but are now unused by these paths).

- [ ] **Step 5: Commit**

```bash
git add src/api/display.rs src/main.rs tests/api_display_test.rs
git commit -m "feat(display): serve unregistered devices the DEFAULT device screen (code-aware)"
```

---

### Task 5: Remove `default_screen` + `registration.screen` fields and all references

**Files:**
- Modify: `src/models/config.rs` (remove field + fn + Default + tests)
- Modify: `src/api/admin/write.rs` (SettingsWrite, gate, patch_settings, delete_package, tests)
- Modify: `src/api/dev.rs:147,236` (ScreensResponse field + assignment)
- Modify: `src/server.rs:298-320` (`test_config_swap_is_visible`)

**Interfaces:**
- Removes: `AppConfig.default_screen`, `AppConfig::default_screen()` fn, `RegistrationConfig.screen`, `SettingsWrite.default_screen`, `SettingsWrite.registration_screen`, `dev::ScreensResponse.default_screen`.
- After this task the tree must compile with **zero** references to the removed fields (except YAML/test-data strings handled in Tasks 6-7). Do the whole removal in one commit so intermediate states never break the build.

- [ ] **Step 1: Remove from `src/models/config.rs`**

  - Delete the `default_screen` field + its `#[serde(default = "default_screen")]` (175-176) and the doc comment (174).
  - Delete the `default_screen()` fn (222-224).
  - In `RegistrationConfig`, delete the `screen` field + its `#[serde(default)]` + doc comment (286-291).
  - In `RegistrationConfig::Default` (302-309), drop `screen: None,`.
  - In `AppConfig::Default` (394-407), drop `default_screen: default_screen(),`.
  - In `mod tests`: delete `test_default_screen_function` (441-445), `test_default_screen_is_builtin` (739-742); in `test_default_config` (423-433) drop the `config.default_screen` assertion; in `test_deserialize_config` (447-468) drop the `default_screen:` YAML line and its assertion (keep the device assertions); in `test_registration_config_default` (470-475) drop the `reg.screen.is_none()` assertion; delete `test_deserialize_config_with_custom_registration_screen` (489-503); in `test_deserialize_config_with_registration` (477-487) drop the `config.registration.screen.is_none()` assertion.

- [ ] **Step 2: Remove from `src/api/admin/write.rs`**

  - `SettingsWrite` (271-278): delete the `default_screen` and `registration_screen` fields.
  - `require_writable_settings` (56-67): change `touches_global` to `let touches_global = body.auth_mode.is_some() || body.package_refresh_interval.is_some();` (drop the two removed clauses).
  - `patch_settings`: delete the validation blocks for `default_screen` (298-302) and `registration_screen` (303-307), and the write blocks for `default_screen` (323-326) and `registration_screen` (327-331).
  - `delete_package` (517-537): delete both dangling-ref checks (the `config.default_screen` check 519-527 and the `config.registration.screen` check 528-537). The existing per-device dangling check (507-516) already covers the DEFAULT device, since DEFAULT is an ordinary entry in `config.devices` — so deleting a package referenced by the DEFAULT screen is still rejected with a device-key message.
  - Tests: in `registration_only_body()` (626-634) drop `default_screen: None,` and `registration_screen: None,`; in `standalone_allows_any_settings_body` (660-666) drop `default_screen: Some(...)` and `registration_screen: None,`.

- [ ] **Step 3: Remove from `src/api/dev.rs`**

  - `ScreensResponse` (147): delete `pub default_screen: Option<String>,`.
  - The `Json(ScreensResponse { ... default_screen: state.config.default_screen.clone(), })` construction (236): delete the `default_screen: ...` line.
  - If `static/dev/index.html` reads `default_screen` from this response, leave the HTML alone (an absent JSON field renders as undefined/blank; the dev page's default-screen display is cosmetic). Note this in the commit body.

- [ ] **Step 4: Fix `src/server.rs` test**

  Rewrite `test_config_swap_is_visible` (298-320) to exercise a DEFAULT-device swap instead of `default_screen`:

```rust
    #[test]
    fn test_config_swap_is_visible() {
        use crate::models::config::{DeviceConfig, RESERVED_DEFAULT_KEY};
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let state = create_app_state(loader).unwrap();
        // Embedded config ships a DEFAULT device pointing at the builtin screen.
        assert_eq!(
            state.config.load().default_device_screen(),
            Some("byonk-builtin/default")
        );

        // Swap in a config whose DEFAULT device screen is a sentinel.
        let mut cfg = (**state.config.load()).clone();
        cfg.devices.insert(
            RESERVED_DEFAULT_KEY.to_string(),
            DeviceConfig { screen: "sentinel".to_string(), ..Default::default() },
        );
        state.config.store(Arc::new(cfg));
        assert_eq!(
            state.config.load().default_device_screen(),
            Some("sentinel")
        );
    }
```

- [ ] **Step 5: Verify compile + run the affected unit tests**

Run: `cargo build && cargo test --lib`
Expected: PASS. If the compiler flags any remaining `default_screen` / `registration.screen` reference, fix it (grep `rg 'default_screen|registration\.screen|registration_screen' src/`). The `tests/` directory is fixed in Tasks 6-7 and is compiled separately.

- [ ] **Step 6: Commit**

```bash
git add src/models/config.rs src/api/admin/write.rs src/api/dev.rs src/server.rs
git commit -m "refactor(config): remove default_screen + registration.screen settings"
```

---

### Task 6: Migrate embedded + dev YAML to ship the DEFAULT device

**Files:**
- Modify: `default-config.yaml` (embedded config, compiled in)
- Modify: `config.yaml` (dev config, on disk)
- Modify: `src/assets.rs:889` (test asserting embedded content)

**Interfaces:** none (data + one test).

- [ ] **Step 1: Migrate `default-config.yaml`**

  - Delete lines 8-9 (`# Screen shown to un-onboarded / unassigned devices.` and `default_screen: byonk-builtin/default`).
  - Update the header comment (line 3) to: `# New (un-onboarded) and unassigned devices show the reserved DEFAULT device's screen.`
  - Replace `devices: {}` (118) with:

```yaml
# Home Assistant owns real devices; only the reserved DEFAULT device ships.
# Its screen is what every un-onboarded / unassigned device shows (the built-in
# screen is registration-aware and renders the pairing code for new devices).
devices:
  DEFAULT:
    screen: byonk-builtin/default
```

- [ ] **Step 2: Migrate `config.yaml` (dev)**

  - Delete lines 287-288 (`# Default screen for devices not in the devices list` and `default_screen: default`).
  - Under `devices:` (180), add the DEFAULT device as the first real entry (after the existing comment block, before/around the commented examples):

```yaml
devices:
  # Reserved: the screen shown by un-onboarded / unassigned devices.
  DEFAULT:
    screen: byonk-builtin/default
  # Example using registration code (read from device screen):
  # "ABCDE-FGHJK":
```

  - In the registration comment block (10-12), delete the `# screen: my_screen ...` lines (the custom registration screen no longer exists; the DEFAULT device replaces it).

- [ ] **Step 3: Fix the `src/assets.rs` embedded-content test**

  At line 889 change:

```rust
        assert!(content.contains("default_screen:"));
```

to:

```rust
        assert!(content.contains("DEFAULT:"));
```

- [ ] **Step 4: Verify**

Run: `cargo test --lib assets && cargo run -- --help >/dev/null 2>&1; echo done`
Then confirm the embedded config parses by running the config load test: `cargo test --lib models::config`
Expected: PASS. (The embedded `default-config.yaml` is parsed by `AppConfig` at startup; a malformed migration would fail these.)

- [ ] **Step 5: Commit**

```bash
git add default-config.yaml config.yaml src/assets.rs
git commit -m "feat(config): ship reserved DEFAULT device in embedded + dev YAML"
```

---

### Task 7: Fix remaining integration-level Rust tests

**Files:**
- Modify: `tests/admin_write_test.rs` (remove default_screen/registration_screen patch tests)
- Modify: `tests/admin_packages_test.rs:177-216` (repoint the dangling-ref test to the DEFAULT device)
- Modify: `tests/common/app.rs:244` (broken-config fixture)

**Interfaces:** none (tests only). This task makes `cargo test` (full suite) green again.

- [ ] **Step 1: `tests/admin_write_test.rs`**

  - Delete `test_patch_settings_default_screen_persists` (the test around line 89 that PATCHes `{"default_screen":...}`) and `test_patch_settings_unknown_default_screen_returns_400` (123).
  - Delete `test_patch_settings_registration_screen_persists` (387), `test_patch_settings_registration_screen_empty_is_builtin_sentinel` (429), `test_patch_settings_unknown_registration_screen_returns_400` (450).
  - Setting the DEFAULT device screen is now tested via the per-device endpoint. Add one test that `PATCH /devices/DEFAULT {"screen":"<known>"}` returns 200 and persists (mirror an existing `patch_device` test in this file, using key `DEFAULT`; the DEFAULT device exists in the embedded config so PATCH updates it):

```rust
#[tokio::test]
async fn test_patch_default_device_screen_persists() {
    let app = TestApp::spawn().await;
    let resp = app
        .patch_json("/api/admin/devices/DEFAULT", r#"{"screen":"byonk-builtin/default"}"#)
        .await;
    assert_eq!(resp.status(), 200);
}
```

Match the existing helpers in this file (`TestApp::spawn`, `patch_json` or the equivalent request helper actually used — copy the pattern from a neighboring `patch_device` test verbatim; do not assume method names).

- [ ] **Step 2: `tests/admin_packages_test.rs`**

  Rewrite `test_delete_package_referenced_by_default_screen_is_conflict` (177-216) to reference the DEFAULT **device** instead of the removed `default_screen` field. Instead of injecting `default_screen: weather/forecast`, inject a device `DEFAULT` whose screen is `weather/forecast`, then assert `DELETE /packages/weather` is a 409 whose message names the referencing device `DEFAULT`:

```rust
    // Point the reserved DEFAULT device at a screen inside the `weather` package.
    cfg.devices.insert(
        "DEFAULT".into(),
        // build the DeviceConfig / YAML mapping the same way the surrounding
        // test injects config — copy the existing injection helper in this file.
    );
    // ...
    assert!(
        del.text().contains("DEFAULT"),
        "conflict message should name the referencing device: {}",
        del.text()
    );
```

Follow the file's actual config-injection mechanism (the current test edits a `serde_yaml::Mapping` and inserts `default_screen`; switch that to inserting a `devices.DEFAULT.screen` mapping). Rename the test to `test_delete_package_referenced_by_default_device_is_conflict`.

- [ ] **Step 3: `tests/common/app.rs`**

  At line 244 the broken-config fixture string embeds `default_screen: broken`. Replace it so the fixture no longer sets `default_screen` (it exercises a broken screen ref). Change:

```rust
            "admin:\n  token: {token}\ndefault_screen: broken\nscreens:\n  broken:\n    script: broken.lua\n    template: broken.svg\n"
```

to:

```rust
            "admin:\n  token: {token}\ndevices:\n  DEFAULT:\n    screen: broken\nscreens:\n  broken:\n    script: broken.lua\n    template: broken.svg\n"
```

(If any test relying on this fixture asserts on `default_screen` specifically, update it to the DEFAULT device; grep `rg 'broken' tests/` to find consumers.)

- [ ] **Step 4: Run the full suite**

Run: `make check`
Expected: PASS (fmt + clippy `-D warnings` + all tests). Fix any remaining reference the compiler/tests surface.

- [ ] **Step 5: Commit**

```bash
git add tests/admin_write_test.rs tests/admin_packages_test.rs tests/common/app.rs
git commit -m "test(admin): migrate default_screen/registration_screen tests to DEFAULT device"
```

---

### Task 8: Expose a `reserved` flag on `GET /devices`

**Files:**
- Modify: `src/api/admin/read.rs` (`AdminDevice` struct + both push sites in `list_devices`)
- Test: `tests/admin_write_test.rs` or `tests/` device-list test (assert the DEFAULT device is flagged)

**Interfaces:**
- Produces: `AdminDevice.reserved: bool` — `true` iff `key == RESERVED_DEFAULT_KEY`. The integration (Part B) uses this to identify and specially present the DEFAULT device.

- [ ] **Step 1: Write the failing test**

Add a test (in the file that already tests `GET /api/admin/devices`; mirror its harness):

```rust
#[tokio::test]
async fn test_list_devices_flags_reserved_default() {
    let app = TestApp::spawn().await;
    let devices = app.get_json("/api/admin/devices").await; // -> serde_json::Value array
    let default = devices
        .as_array().unwrap().iter()
        .find(|d| d["key"] == "DEFAULT").expect("DEFAULT device present");
    assert_eq!(default["reserved"], true);
    // A non-reserved device (if any seen) must be reserved:false — assert the field exists.
    assert!(default.get("reserved").is_some());
}
```

Use the actual request helper the neighboring device-list tests use.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test admin_write_test test_list_devices_flags_reserved_default` (adjust the test-binary name to where you placed it)
Expected: FAIL — `reserved` field absent.

- [ ] **Step 3: Write minimal implementation**

In `src/api/admin/read.rs`, add to `AdminDevice` (after `registered: bool,`):

```rust
    /// `true` for the reserved DEFAULT device (byonk-managed fallback, not a
    /// physical device). The integration presents it specially.
    pub reserved: bool,
```

Add `use crate::models::config::RESERVED_DEFAULT_KEY;` to the imports. In `list_devices`, set the field in **both** `AdminDevice { ... }` constructions:
  - seen-devices push (133-152): `reserved: mac == RESERVED_DEFAULT_KEY,` (a physical device would never key on "DEFAULT", so this is effectively always false here, but set it consistently).
  - configured-not-seen push (163-180): `reserved: key == RESERVED_DEFAULT_KEY,`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test admin_write_test test_list_devices_flags_reserved_default`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/api/admin/read.rs tests/admin_write_test.rs
git commit -m "feat(admin): flag the reserved DEFAULT device in GET /devices"
```

---

**Part A checkpoint:** run `make check` — the full Rust suite is green, byonk resolves screens via the DEFAULT device in standalone and add-on mode, and the DEFAULT screen is settable over `PATCH /devices/DEFAULT`. Standalone byonk is fully functional without Part B. **VM-verify (optional here, required before merge):** set the DEFAULT device screen over the admin API and confirm an unregistered device shows the code and a registered-unassigned device shows the DEFAULT screen.

---

# PART B — HA integration (Python) + docs

Part B presents the DEFAULT device in Home Assistant. It depends on Part A's `GET /devices` (with `reserved`) and the per-device `PATCH /devices/DEFAULT` write. Baseline: `pytest tests_ha -q` = 69 passing.

---

### Task 9: Present the reserved DEFAULT device in the integration

**Files:**
- Modify: `custom_components/byonk/const.py` (add `DEFAULT_DEVICE_KEY`)
- Modify: `custom_components/byonk/coordinator.py` (remove dead accessor; exempt DEFAULT from reconcile/orphan; auto-provision the DEFAULT entry)
- Modify: `custom_components/byonk/config_flow.py` (discovery step handles the DEFAULT key without the screen-picker)
- Modify: `custom_components/byonk/select.py` (present only the screen-select for the DEFAULT entry)
- Modify: `custom_components/byonk/entity.py` (device name for DEFAULT) — only if device naming is derived there
- Test: `tests_ha/` (new test module `test_default_device.py`)

**Interfaces:**
- Consumes: `GET /devices` entries with `reserved == true` and `key == "DEFAULT"` (Part A Task 8); `PATCH /devices/DEFAULT` (per-device write).
- Produces: a config entry with `unique_id == "DEFAULT"`, `data[CONF_DEVICE_KEY] == "DEFAULT"`, exempt from reconcile removal and orphan pruning, exposing a `ByonkScreenSelect`.

- [ ] **Step 1: Add the const**

In `custom_components/byonk/const.py` add:

```python
# Reserved device key whose screen every un-onboarded / unassigned device shows.
# Mirrors byonk's RESERVED_DEFAULT_KEY.
DEFAULT_DEVICE_KEY = "DEFAULT"
```

- [ ] **Step 2: Write the failing tests**

Create `tests_ha/test_default_device.py`. Mirror the harness of the existing device tests (`tests_ha/test_settings_entities.py` / whichever sets up a hub + coordinator with a fake byonk client). Assert:

```python
async def test_default_device_entry_is_provisioned(hass, ...):
    # Given a fake byonk reporting a reserved DEFAULT device in GET /devices,
    # after the hub refreshes, a config entry with unique_id "DEFAULT" exists
    # and exposes select.<...>_screen, and does NOT expose dither/panel selects.
    ...

async def test_default_device_exempt_from_orphan_prune(hass, ...):
    # byonk reports DEFAULT registered but (transiently) no HA entry ->
    # coordinator must NOT call async_delete_device("DEFAULT").
    ...

async def test_default_device_screen_select_writes_patch(hass, ...):
    # Selecting a screen on the DEFAULT device issues PATCH /devices/DEFAULT.
    ...
```

Fill these in using the exact fixtures/fakes the other `tests_ha` modules use (fake client with `async_get_devices` returning a `[{"key":"DEFAULT","reserved":True,"registered":True,"screen":"byonk-builtin/default", ...}]` entry; assert on `hass.config_entries` and on the fake client's recorded calls).

- [ ] **Step 3: Run tests to verify they fail**

Run: `.venv/bin/pytest tests_ha/test_default_device.py -q`
Expected: FAIL — no auto-provision, orphan prune would fire, no special-casing.

- [ ] **Step 4: Implement in `coordinator.py`**

  - **Remove** the dead `registration_screen()` accessor (50-51) — verify no references first: `rg 'registration_screen' custom_components tests_ha` (should be none after Plan A's Options-Flow revert).
  - **Exempt DEFAULT from reconcile** (`_async_reconcile`, 133-159): after computing `byonk_registered`, drop the reserved key from both the removal and orphan candidate sets. Simplest: `byonk_registered.discard(DEFAULT_DEVICE_KEY)` right after it is built **and** `ha_keys` handling: keep the DEFAULT HA entry (it is not a real device, never remove it). Concretely, add near the top of the method:

```python
        byonk_registered = {d["key"] for d in data.devices if d.get("registered")}
        byonk_registered.discard(DEFAULT_DEVICE_KEY)  # reserved: never orphan-prune
        ha_keys = set(device_entries) - {DEFAULT_DEVICE_KEY}  # never auto-remove
```

  (Recompute `ha_keys` before the loops so both `ha_keys - byonk_registered` and `byonk_registered - ha_keys` skip DEFAULT. Keep the strike-clearing loop working for real devices.)

  - **Auto-provision** the DEFAULT entry: add a method `_async_provision_default(self, data)` called from `_async_update_data` right after `_async_sync_discovery(data)`. It checks whether byonk reports a reserved DEFAULT device and whether an HA entry already exists; if the device is present and no entry exists and no DEFAULT flow is in progress, init an integration-discovery flow keyed on DEFAULT:

```python
    def _async_provision_default(self, data: ByonkData) -> None:
        has_default = any(
            d.get("key") == DEFAULT_DEVICE_KEY and d.get("reserved")
            for d in data.devices
        )
        if not has_default:
            return
        configured = {e.unique_id for e in self.hass.config_entries.async_entries(DOMAIN)}
        if DEFAULT_DEVICE_KEY in configured:
            return
        flows = self.hass.config_entries.flow.async_progress_by_handler(
            DOMAIN, include_uninitialized=True
        )
        if any(f["context"].get("unique_id") == DEFAULT_DEVICE_KEY for f in flows):
            return
        self.hass.async_create_task(
            self.hass.config_entries.flow.async_init(
                DOMAIN,
                context={"source": SOURCE_INTEGRATION_DISCOVERY},
                data={"key": DEFAULT_DEVICE_KEY, "code": None, "model": None},
            ),
            eager_start=False,
        )
```

Import `DEFAULT_DEVICE_KEY` from `.const`.

- [ ] **Step 5: Implement in `config_flow.py`**

  In `async_step_integration_discovery` (130-141), short-circuit the DEFAULT key so it does **not** enter the screen-picker (`async_step_configure` expects `_discovery["key"]` to be a pending device being *added* via `async_add_device` — but DEFAULT already exists in byonk). After `self._abort_if_unique_id_configured()`, add:

```python
        if mac == DEFAULT_DEVICE_KEY:
            hub = self._hub_entry()
            if hub is None:
                return self.async_abort(reason="no_hub")
            return self.async_create_entry(
                title="Byonk Default",
                data={CONF_DEVICE_KEY: DEFAULT_DEVICE_KEY, CONF_HUB_ENTRY_ID: hub.entry_id},
            )
```

Import `DEFAULT_DEVICE_KEY`. This creates the entry directly (no picker, no `async_add_device` — the device already exists in byonk); its `ByonkScreenSelect` then edits it live over `PATCH /devices/DEFAULT`.

- [ ] **Step 6: Implement in `select.py`**

  In `async_setup_entry` (15-28), special-case the DEFAULT entry to expose only the screen-select (spec scopes the DEFAULT device to its screen; dither/panel keep their own per-device fallback chains and add no value on a synthetic device):

```python
    if CONF_DEVICE_KEY in entry.data:
        key = entry.data[CONF_DEVICE_KEY]
        if key == DEFAULT_DEVICE_KEY:
            async_add_entities([ByonkScreenSelect(coordinator, key)])
            return
        async_add_entities(
            [
                ByonkScreenSelect(coordinator, key),
                ByonkDitherSelect(coordinator, key),
                ByonkPanelSelect(coordinator, key),
            ]
        )
        setup_param_platform(entry, async_add_entities, {"enum"}, ByonkParamSelect)
```

Import `DEFAULT_DEVICE_KEY` from `.const`. (Also confirm other per-device platforms — `sensor.py`, `switch.py`, `text.py`, `button.py`, `param_entities.py` — behave acceptably for the DEFAULT entry: they read from the device mapping and should no-op or render harmlessly. If any raises on the synthetic device, guard it the same way. Check with the test in Step 2 and by running the full suite in Step 7.)

- [ ] **Step 7: Device naming (`entity.py`)**

  If `ByonkDeviceEntity` derives the HA device name from the key/telemetry, ensure the DEFAULT entry gets a sensible name ("Byonk Default"). If naming comes from `name_sync.py` or the config entry title, the Step-5 title `"Byonk Default"` already covers it — verify and only adjust if the DEFAULT device shows up as `TRMNL DEFAULT` or blank.

- [ ] **Step 8: Run tests**

Run: `.venv/bin/ruff check custom_components/byonk tests_ha && .venv/bin/pytest tests_ha -q`
Expected: PASS — `test_default_device.py` green and the baseline 69 still pass.

- [ ] **Step 9: Commit**

```bash
git add custom_components/byonk/const.py custom_components/byonk/coordinator.py custom_components/byonk/config_flow.py custom_components/byonk/select.py custom_components/byonk/entity.py tests_ha/test_default_device.py
git commit -m "feat(ha): present the reserved DEFAULT device with a live screen-select"
```

(Only add `entity.py` if Step 7 changed it.)

---

### Task 10: Documentation + CHANGES

**Files:**
- Modify: `docs/src/` — the config reference page(s) documenting `default_screen` / registration screen, and the HA integration page.
- Modify: `CHANGES.md` (Unreleased section)

**Interfaces:** none.

- [ ] **Step 1: Find the affected docs**

Run: `rg -l 'default_screen|registration.*screen|registration screen' docs/src/`
Read each hit and identify where the old settings are documented.

- [ ] **Step 2: Update the config reference**

  - Replace descriptions of `default_screen` and the custom `registration.screen` with the reserved DEFAULT device: a `DEFAULT` entry in `devices:` whose `screen` is shown by every un-onboarded / unassigned device; the built-in `byonk-builtin/default` screen is registration-aware (shows the pairing code for new devices). Keep documenting `registration.enabled`.
  - Show the YAML shape:

```yaml
devices:
  DEFAULT:
    screen: byonk-builtin/default
```

- [ ] **Step 3: Update the HA integration docs**

  Document that the default/onboarding screen is now set via the **Byonk Default** device card's screen-select (live, no restart), replacing the former new-device-screen select and the Plan-3 Options-Flow screen field.

- [ ] **Step 4: CHANGES.md**

  Add under Unreleased:

```markdown
### Changed
- Replaced the `default_screen` and custom `registration.screen` settings with a
  single reserved `DEFAULT` device. The screen assigned to `devices.DEFAULT` is
  shown by every un-onboarded or unassigned device; the built-in default screen
  renders the pairing code for new devices. In Home Assistant this is set live via
  the **Byonk Default** device's screen-select.
```

- [ ] **Step 5: Build docs + verify**

Run: `make docs`
Expected: builds without error.

- [ ] **Step 6: Commit**

```bash
git add docs/src CHANGES.md
git commit -m "docs: document the reserved DEFAULT device (replaces default_screen)"
```

---

## Self-review notes (author)

- **Spec coverage:** §4a (reserved DEFAULT device replacing both settings) → Tasks 1,3,4,5,6; code-vs-generic ultimate fallback (§4a, §5.6) → Task 2; remove settings + gate + delete-package refs (§5.6) → Task 5; YAML migration → Task 6; integration live screen-select (§6) → Task 9; docs → Task 10. `registration.enabled` kept (§11) — never touched. Standalone unchanged (§10) — the model change applies uniformly; add-on manifest untouched (DEFAULT is a per-device write, allowed in add-on mode).
- **Type consistency:** `RESERVED_DEFAULT_KEY` (Rust) / `DEFAULT_DEVICE_KEY` (Python) both = `"DEFAULT"`; `default_device_screen()` used in Tasks 3,4,5; `render_builtin_fallback(Option<&str>, u32, u32)` used in Task 4; `AdminDevice.reserved` (Task 8) consumed in Task 9.
- **Open implementation choices deliberately deferred to the implementer** (all with a stated default): exact pipeline test-harness construction (Task 2/3 — copy neighbor tests), and whether `entity.py` needs a naming tweak (Task 9 Step 7 — verify, default no-op). None are placeholders for *what* to build, only *where the existing harness lives*.
- **DEFAULT dither/panel:** deliberately out of scope (screen only) per §4a ("scope to at least the screen") and YAGNI — per-device dither/panel already have independent fallback chains.
