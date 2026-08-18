# Plan A — Measured Colours End to End

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a panel's measured colours (`colors_actual`) readable from Lua, overridable by a Lua script, and explicitly previewable through the `render_screen` MCP tool.

**Architecture:** `colors_actual` today resolves in one place (`src/api/display.rs`) through an inline `if/else` chain and is handed to the renderer already resolved. This plan extracts that chain into a pure, unit-testable function, adds a script layer on top of it, and threads the resolved value both *into* the Lua `device` table (so a script can read it) and *out of* the script (so a script can replace it). The MCP render tool then gains explicit control over which palette the returned PNG is drawn in.

**Tech Stack:** Rust 2021, `mlua` 0.10 (lua54, vendored), `rmcp` 2.2, `tracing`, `tokio`. Tests are plain `#[test]`/`#[tokio::test]` in `tests/` and inline `mod tests`.

**Spec:** `docs/superpowers/specs/2026-08-06-lua-colors-and-image-ops-design.md`, Parts 1 and 3.

## Global Constraints

- **Never `git add -A` or `git add .`** — this repo has untracked local files that must not be swept in. Stage explicit paths and check `git diff --cached` before every commit.
- **Never use `sed -i` to modify files.** This is macOS: BSD `sed` requires a backup-suffix argument and has silently destroyed work here before. Use the Edit and Write tools.
- **Run everything in the foreground.** Do not background `make check` or any test run.
- **Cap build/test parallelism at 4** (`cargo test -- --test-threads=4`, `CARGO_BUILD_JOBS=4`) — shared machine.
- `make check` = `cargo fmt` + `cargo clippy -- -D warnings` + `cargo test`. Clippy warnings are **errors**.
- Full suite baseline at the start of this plan: **606 passed, 0 failed, 1 ignored**. `--lib` alone is ~362; that is not a discrepancy.
- **Commit after every task**, and incrementally within a task if it runs long. A prior session lost an entire uncommitted fix wave to a crash.
- **CHANGES.md is user-facing only** — describe user-visible behaviour, never internal refactors, tooling, or process.
- Every test you write must **fail against the current code**. Before claiming a test is meaningful, run it against unmodified code (or break the implementation deliberately) and confirm it fails. A test that cannot fail is the most expensive defect this project has found.
- If you disagree with a step in this plan, say so and explain why. Disclosing a weak test or a wrong instruction is valued, not penalised.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src/api/display.rs` | Palette + measured-colour resolution chains, shared by every render path | Modify: new `resolve_measured_colors`, `MeasuredResolution`; `resolve_render_params` gains a parameter |
| `src/services/content_pipeline.rs` | `DeviceContext` (script/template input) and pipeline `ScriptResult` (script output) | Modify: one new field on each |
| `src/services/lua_runtime.rs` | Lua globals setup and script-return parsing | Modify: expose `device.colors_actual`, parse the `colors_actual` return |
| `src/services/screen_store.rs` | Authoring render (`RenderOpts`, `ScreenStore::render`) | Modify: two new `RenderOpts` fields, populate ctx, surface the warning into `log` |
| `src/mcp/tools_render.rs` | `render_screen` MCP tool | Modify: two new args + description |
| `src/rendering/svg_to_png.rs` | Palette construction for the ditherer | Modify: warn instead of silently dropping |
| `src/api/dev.rs`, `src/main.rs` | Dev-preview and CLI render paths | Modify: hoist measured resolution above `DeviceContext`, populate the new field |
| `tests/lua_api_test.rs` | Lua-visible API | Modify: new tests |
| `tests/mcp_tools_test.rs` | MCP tool behaviour | Modify: new tests |
| `docs/src/api/lua-api.md`, `CHANGES.md` | User docs | Modify |

---

## Task 1: Expose `device.colors_actual` to Lua

**Files:**
- Modify: `src/services/content_pipeline.rs` (`DeviceContext`, ~line 64-97)
- Modify: `src/services/lua_runtime.rs` (`setup_globals`, the `device_table` block, ~line 376-434)
- Modify: `src/api/display.rs` (`DeviceContext` construction, ~line 663)
- Modify: `src/services/screen_store.rs` (`DeviceContext` construction, ~line 988)
- Modify: `src/api/dev.rs` (`DeviceContext` construction, ~line 470; measured resolution, ~line 569)
- Modify: `src/main.rs` (`DeviceContext` construction, ~line 237; panel resolution, ~line 310-322)
- Test: `tests/lua_api_test.rs`

**Interfaces:**
- Produces: `DeviceContext { colors_actual: Option<Vec<String>>, .. }` — hex strings like `"#0A0A0A"`, index-parallel to `colors`. Lua sees it as `device.colors_actual`, a 1-based array of strings, or `nil`.

> **Line numbers in this plan are anchors, not addresses.** They were read from the tree at commit `6a1caa3` and will drift as you edit. Always locate the code by its content (the quoted snippet), never by jumping to a line number.

- [ ] **Step 1: Write the failing tests**

Add to `tests/lua_api_test.rs`, inside the same `mod` that holds `test_qr_svg_basic` (it already imports `DeviceContext`, `LuaRuntime`, `HashMap`, and `setup_test_env`):

```rust
#[test]
fn test_device_colors_actual_exposed_when_measured() {
    let script = r#"
        return {
            data = {
                actual_1 = device.colors_actual[1],
                actual_3 = device.colors_actual[3],
                count = #device.colors_actual,
                official_count = #device.colors,
            },
            refresh_rate = 60
        }
    "#;

    let (_temp_dir, asset_loader) = setup_test_env(&[("test_ca.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let ctx = DeviceContext {
        mac: "TE:ST:00:00:00:00".to_string(),
        width: Some(800),
        height: Some(480),
        colors: Some(vec![
            "#000000".to_string(),
            "#FFFFFF".to_string(),
            "#FF0000".to_string(),
        ]),
        colors_actual: Some(vec![
            "#0A0A0A".to_string(),
            "#E8E6E0".to_string(),
            "#A83A30".to_string(),
        ]),
        ..Default::default()
    };

    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_ca.lua"),
            &HashMap::new(),
            Some(&ctx),
            None,
        )
        .expect("Script should run");

    assert_eq!(result.data["actual_1"].as_str().unwrap(), "#0A0A0A");
    assert_eq!(result.data["actual_3"].as_str().unwrap(), "#A83A30");
    assert_eq!(result.data["count"].as_i64().unwrap(), 3);
    // Index-parallel with device.colors.
    assert_eq!(result.data["official_count"].as_i64().unwrap(), 3);
}

#[test]
fn test_device_colors_actual_is_nil_when_uncalibrated() {
    // Deliberately NOT mirrored from device.colors: a script must be able to
    // tell "this panel is uncalibrated" from "this panel measures to spec".
    let script = r#"
        return {
            data = { is_nil = device.colors_actual == nil },
            refresh_rate = 60
        }
    "#;

    let (_temp_dir, asset_loader) = setup_test_env(&[("test_ca_nil.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let ctx = DeviceContext {
        mac: "TE:ST:00:00:00:00".to_string(),
        width: Some(800),
        height: Some(480),
        colors: Some(vec!["#000000".to_string(), "#FFFFFF".to_string()]),
        colors_actual: None,
        ..Default::default()
    };

    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_ca_nil.lua"),
            &HashMap::new(),
            Some(&ctx),
            None,
        )
        .expect("Script should run");

    assert!(result.data["is_nil"].as_bool().unwrap());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test lua_api_test colors_actual -- --test-threads=4`
Expected: **compile error** — `DeviceContext` has no field `colors_actual`. That is a legitimate failure for this step.

- [ ] **Step 3: Add the field to `DeviceContext`**

In `src/services/content_pipeline.rs`, immediately after the existing `colors` field:

```rust
    /// Available display colors as hex RGB strings (e.g. ["#000000", "#FFFFFF", "#FF0000"])
    pub colors: Option<Vec<String>>,
    /// Measured colors the panel really shows, index-parallel to `colors`.
    /// `None` when nothing in the measured chain resolved — deliberately not
    /// mirrored from `colors`, so a script can distinguish an uncalibrated
    /// panel from one that measures exactly to spec.
    pub colors_actual: Option<Vec<String>>,
```

`DeviceContext` derives `Default`, and every construction site uses `..Default::default()` **except** `src/api/display.rs:663`, which lists `refresh_override: None` explicitly and therefore names every field. Adding the field will break only that one site at compile time; that is the point.

- [ ] **Step 4: Expose it in the Lua `device` table**

In `src/services/lua_runtime.rs::setup_globals`, find this existing block:

```rust
            if let Some(ref colors) = ctx.colors {
                let colors_table = lua.create_table()?;
                for (i, color) in colors.iter().enumerate() {
                    colors_table.set(i + 1, color.as_str())?;
                }
                device_table.set("colors", colors_table)?;
            }
```

Add directly beneath it:

```rust
            // Measured panel colours. Absent (nil in Lua) rather than mirrored
            // from `colors` when uncalibrated — see DeviceContext::colors_actual.
            if let Some(ref actual) = ctx.colors_actual {
                let actual_table = lua.create_table()?;
                for (i, color) in actual.iter().enumerate() {
                    actual_table.set(i + 1, color.as_str())?;
                }
                device_table.set("colors_actual", actual_table)?;
            }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test lua_api_test colors_actual -- --test-threads=4`
Expected: both PASS.

- [ ] **Step 6: Populate the field in `src/api/display.rs`**

This is the production device path. `measured_colors` is **already resolved above** the `DeviceContext` construction (the `// Resolve measured colors: dev override > panel.colors_actual > ...` block), so no reordering is needed. In the `DeviceContext { .. }` literal that sets `colors: Some(ctx_color_hex),`, add:

```rust
        colors: Some(ctx_color_hex),
        colors_actual: measured_colors.as_deref().map(colors_to_hex_strings),
```

The **other** `DeviceContext` in this file (the registration-screen one, which sets `registration_code: Some(code.to_string())` and ends with `..Default::default()`) is left alone: no panel is resolved on that path, so `colors_actual` stays `None`. That is correct, not an oversight.

- [ ] **Step 7: Populate the field in `src/services/screen_store.rs`**

`measured_colors` is already resolved above the `DeviceContext` construction here too. In the literal that sets `colors: Some(crate::api::display::colors_to_hex_strings(&ctx_palette)),`, add:

```rust
            colors_actual: measured_colors
                .as_deref()
                .map(crate::api::display::colors_to_hex_strings),
```

- [ ] **Step 8: Hoist the measured resolution in `src/api/dev.rs` and populate**

Here the order is wrong: `device_ctx` is built at ~line 470 but `measured_colors` is not resolved until ~line 569. Move the resolution block **above** the `let device_ctx = DeviceContext {` literal. The block to move, verbatim:

```rust
    // Resolve measured colors: query param (from dev UI color tuning) > panel.colors_actual
    let measured_colors: Option<Vec<(u8, u8, u8)>> = if let Some(ref ca) = query.colors_actual {
        Some(crate::api::display::parse_colors_header(ca))
    } else {
        panel
            .as_ref()
            .and_then(|p| p.colors_actual.as_deref())
            .map(crate::api::display::parse_colors_header)
    };
```

It depends only on `query` and `panel`, both of which are in scope well before `device_ctx` (`panel` is already used by `panel_dither_config_pre` just above the literal), so the move is safe. Then add to the `DeviceContext` literal:

```rust
        colors_actual: measured_colors
            .as_deref()
            .map(crate::api::display::colors_to_hex_strings),
```

- [ ] **Step 9: Hoist the panel resolution in `src/main.rs` and populate**

Same problem, larger: `device_context` is built at ~line 237, but `device_config`/`panel`/`measured` are not resolved until ~line 307-322, inside the `else` branch. Hoist just the lookup needed for the measured value to **above** the `let device_context = DeviceContext {` literal:

```rust
    // Measured panel colours, resolved before the device context so the script
    // can read them as `device.colors_actual`. `registration_code` is borrowed
    // here and moved into DeviceContext below.
    let cli_device_config = config.get_device_config(mac).or_else(|| {
        registration_code
            .as_deref()
            .and_then(|code| config.get_device_config_for_code(code))
    });
    let cli_panel = cli_device_config
        .and_then(|dc| dc.panel.clone())
        .and_then(|name| config.get_panel(&name).cloned());
    let cli_measured = cli_panel
        .as_ref()
        .and_then(|p| p.colors_actual.as_deref())
        .map(byonk::api::display::parse_colors_header);
```

Then in the `DeviceContext` literal:

```rust
        colors: Some(byonk::api::display::colors_to_hex_strings(&cli_palette)),
        colors_actual: cli_measured
            .as_deref()
            .map(byonk::api::display::colors_to_hex_strings),
```

**Do not** delete the existing `device_config`/`panel`/`measured` resolution further down — it is used by the render-params chain there and Task 4 will revisit it. Duplication for one task is acceptable; if `config.get_panel` returns a reference that will not `.cloned()`, adapt by keeping the `panel` lookup as a reference and computing only `cli_measured` early. Verify by compiling, not by assumption.

- [ ] **Step 10: Full check**

Run: `make check`
Expected: fmt clean, clippy clean, **606 + 2 = 608 passed, 0 failed**. If the count differs from 608, stop and report the discrepancy rather than adjusting the expectation.

- [ ] **Step 11: Commit**

```bash
git add src/services/content_pipeline.rs src/services/lua_runtime.rs \
        src/api/display.rs src/api/dev.rs src/services/screen_store.rs \
        src/main.rs tests/lua_api_test.rs
git diff --cached --stat
git commit -m "feat: expose measured panel colours to Lua as device.colors_actual"
```

---

## Task 2: Accept `colors_actual` as a script return value

**Files:**
- Modify: `src/services/lua_runtime.rs` (`ScriptResult` ~line 15-44; `run_script` parsing ~line 216-255)
- Modify: `src/services/content_pipeline.rs` (`ScriptResult` ~line 28-60; construction ~line 290-306)
- Test: `tests/lua_api_test.rs`

**Interfaces:**
- Consumes: nothing from Task 1 (independent; do Task 1 first only for a clean history).
- Produces:
  - `lua_runtime::ScriptResult { colors_actual: Option<Vec<String>>, .. }`
  - `content_pipeline::ScriptResult { script_colors_actual: Option<Vec<String>>, .. }`

  The two names differ deliberately — that is the existing convention in this codebase (`colors` → `script_colors`). Do not "fix" it.

- [ ] **Step 1: Write the failing test**

Add to `tests/lua_api_test.rs`, in the same module as Task 1's tests:

```rust
#[test]
fn test_script_can_return_colors_actual() {
    let script = r#"
        return {
            data = {},
            colors        = { "#000000", "#FFFFFF", "#FF0000" },
            colors_actual = { "#0A0A0A", "#E8E6E0", "#A83A30" },
            refresh_rate  = 60
        }
    "#;

    let (_temp_dir, asset_loader) = setup_test_env(&[("test_ret_ca.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_ret_ca.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("Script should run");

    assert_eq!(
        result.colors_actual.as_deref(),
        Some(["#0A0A0A".to_string(), "#E8E6E0".to_string(), "#A83A30".to_string()].as_slice())
    );
    assert_eq!(result.colors.as_ref().unwrap().len(), 3);
}

#[test]
fn test_script_without_colors_actual_yields_none() {
    let script = r#"
        return { data = {}, refresh_rate = 60 }
    "#;

    let (_temp_dir, asset_loader) = setup_test_env(&[("test_no_ca.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_no_ca.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("Script should run");

    assert!(result.colors_actual.is_none());
}

#[test]
fn test_script_empty_colors_actual_yields_none() {
    // Matches the existing `colors` behaviour: an empty table is None, not
    // Some(vec![]), so it falls through the chain instead of blanking it.
    let script = r#"
        return { data = {}, colors_actual = {}, refresh_rate = 60 }
    "#;

    let (_temp_dir, asset_loader) = setup_test_env(&[("test_empty_ca.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_empty_ca.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("Script should run");

    assert!(result.colors_actual.is_none());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test lua_api_test _ca -- --test-threads=4`
Expected: compile error — no field `colors_actual` on `lua_runtime::ScriptResult`.

- [ ] **Step 3: Add the field and parse it**

In `src/services/lua_runtime.rs`, after the existing `colors` field on `ScriptResult`:

```rust
    /// Optional color palette override from script (hex RGB strings)
    pub colors: Option<Vec<String>>,
    /// Optional measured-colour override from script (hex RGB strings),
    /// index-parallel to `colors`. Wins the measured chain when its length
    /// matches the resolved official palette; see
    /// `crate::api::display::resolve_measured_colors`.
    pub colors_actual: Option<Vec<String>>,
```

In `run_script`, directly beneath the existing `colors` parsing block, add the identical shape:

```rust
        // Parse optional measured-colour array from script return. Same shape
        // as `colors` above: positive integer keys, empty means None.
        let colors_actual = result
            .get::<Table>("colors_actual")
            .ok()
            .map(|t| {
                (1..=t.raw_len())
                    .filter_map(|i| t.raw_get::<String>(i).ok())
                    .collect::<Vec<String>>()
            })
            .filter(|v| !v.is_empty());
```

and add `colors_actual,` to the `Ok(ScriptResult { .. })` literal at the end of the function.

- [ ] **Step 4: Run to verify the three tests pass**

Run: `cargo test --test lua_api_test _ca -- --test-threads=4`
Expected: PASS.

- [ ] **Step 5: Carry it through the pipeline `ScriptResult`**

In `src/services/content_pipeline.rs`, after the `script_colors` field:

```rust
    /// Optional color palette override from Lua script (hex RGB strings)
    pub script_colors: Option<Vec<String>>,
    /// Optional measured-colour override from Lua script (hex RGB strings)
    pub script_colors_actual: Option<Vec<String>>,
```

and in the `Ok(ScriptResult { .. })` literal in `run_resolved`, after `script_colors: lua_result.colors,`:

```rust
            script_colors_actual: lua_result.colors_actual,
```

- [ ] **Step 6: Full check**

Run: `make check`
Expected: clean; **611 passed** (608 + 3).

- [ ] **Step 7: Commit**

```bash
git add src/services/lua_runtime.rs src/services/content_pipeline.rs tests/lua_api_test.rs
git diff --cached --stat
git commit -m "feat: accept colors_actual as a script return value"
```

---

## Task 3: `resolve_measured_colors` — the chain as a pure function

**Files:**
- Modify: `src/api/display.rs` (add above `resolve_render_params`, ~line 175)
- Test: inline `#[cfg(test)] mod tests` in `src/api/display.rs`

**Interfaces:**
- Consumes: `lua_runtime`/`content_pipeline` `colors_actual` fields from Task 2.
- Produces:

```rust
pub struct MeasuredResolution {
    pub colors: Option<Vec<(u8, u8, u8)>>,
    pub source: &'static str,
    pub warning: Option<String>,
}

pub fn resolve_measured_colors(
    script_colors_actual: Option<&[String]>,
    palette_len: usize,
    fallback: Option<Vec<(u8, u8, u8)>>,
    fallback_source: &'static str,
) -> MeasuredResolution
```

Task 4 calls this from all four render paths. **Nothing else changes in this task** — the function is added and tested in isolation, so a reviewer can reject the semantics before four call sites depend on them.

- [ ] **Step 1: Write the failing tests**

Check whether `src/api/display.rs` already ends with a `#[cfg(test)] mod tests`. If it does, append to it; if not, add one at the end of the file with `use super::*;`.

```rust
    #[test]
    fn measured_script_value_wins_over_fallback() {
        let script = vec!["#0A0A0A".to_string(), "#E8E6E0".to_string()];
        let r = resolve_measured_colors(
            Some(&script),
            2,
            Some(vec![(1, 1, 1), (2, 2, 2)]),
            "panel.colors_actual",
        );
        assert_eq!(r.colors.unwrap(), vec![(0x0A, 0x0A, 0x0A), (0xE8, 0xE6, 0xE0)]);
        assert_eq!(r.source, "script");
        assert!(r.warning.is_none());
    }

    #[test]
    fn measured_falls_back_when_script_absent() {
        let r = resolve_measured_colors(None, 2, Some(vec![(1, 1, 1), (2, 2, 2)]), "panel.colors_actual");
        assert_eq!(r.colors.unwrap(), vec![(1, 1, 1), (2, 2, 2)]);
        assert_eq!(r.source, "panel.colors_actual");
        assert!(r.warning.is_none());
    }

    #[test]
    fn measured_reports_none_when_nothing_resolves() {
        let r = resolve_measured_colors(None, 4, None, "none");
        assert!(r.colors.is_none());
        assert_eq!(r.source, "none");
        assert!(r.warning.is_none());
    }

    #[test]
    fn measured_length_mismatch_warns_and_falls_back() {
        let script = vec!["#0A0A0A".to_string(), "#E8E6E0".to_string()];
        let r = resolve_measured_colors(
            Some(&script),
            4, // official palette has 4 entries, script supplied 2
            Some(vec![(1, 1, 1), (2, 2, 2), (3, 3, 3), (4, 4, 4)]),
            "panel.colors_actual",
        );
        // Fell through to the next source, did NOT blank the calibration.
        assert_eq!(r.colors.unwrap().len(), 4);
        assert_eq!(r.source, "panel.colors_actual");
        let w = r.warning.expect("a mismatch must be reported");
        assert!(w.contains('2') && w.contains('4'), "warning must name both lengths: {w}");
    }

    #[test]
    fn measured_malformed_hex_is_caught_by_the_length_check() {
        // parse_colors_header silently drops unparseable entries, so a typo
        // shortens the list. The length rule is what turns that into a
        // diagnostic instead of a silent half-calibration.
        let script = vec!["#0A0A0A".to_string(), "not-a-colour".to_string()];
        let r = resolve_measured_colors(Some(&script), 2, None, "none");
        assert!(r.colors.is_none());
        assert_eq!(r.source, "none");
        assert!(r.warning.is_some());
    }

    #[test]
    fn measured_mismatch_falls_all_the_way_through_to_none() {
        let script = vec!["#0A0A0A".to_string()];
        let r = resolve_measured_colors(Some(&script), 3, None, "none");
        assert!(r.colors.is_none());
        assert_eq!(r.source, "none");
        assert!(r.warning.is_some());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib api::display -- --test-threads=4`
Expected: compile error — `resolve_measured_colors` not found.

- [ ] **Step 3: Implement**

Add to `src/api/display.rs`, immediately above `resolve_render_params`:

```rust
/// Outcome of resolving the measured ("actual") panel colours.
///
/// `source` names which layer supplied the value, for the debug log and the
/// dev UI; `warning` carries a human-readable diagnostic that the caller is
/// responsible for surfacing — `tracing::warn!` on device paths, the script
/// log on authoring paths.
pub struct MeasuredResolution {
    pub colors: Option<Vec<(u8, u8, u8)>>,
    pub source: &'static str,
    pub warning: Option<String>,
}

/// Resolve the measured colours for a render.
///
/// The chain is `script > fallback`, where `fallback` is whatever the caller
/// already resolved from the pre-script layers (dev override / render opts /
/// `panel.colors_actual` / `Measured-Colors` header), labelled by
/// `fallback_source`. A script value wins outright — symmetric with
/// `script_colors` in [`resolve_render_params`].
///
/// A script value whose parsed length does not match `palette_len` is
/// **discarded, not fatal**: a device fetching its screen must never be
/// denied content over a calibration mistake. The mismatch is reported via
/// `warning`, and the fallback is used instead. Note that
/// [`parse_colors_header`] silently drops unparseable entries, so a malformed
/// hex string shows up here as a length mismatch.
pub fn resolve_measured_colors(
    script_colors_actual: Option<&[String]>,
    palette_len: usize,
    fallback: Option<Vec<(u8, u8, u8)>>,
    fallback_source: &'static str,
) -> MeasuredResolution {
    let Some(script) = script_colors_actual else {
        return MeasuredResolution {
            colors: fallback,
            source: fallback_source,
            warning: None,
        };
    };

    let parsed = parse_colors_header(&script.join(","));
    if parsed.len() == palette_len {
        return MeasuredResolution {
            colors: Some(parsed),
            source: "script",
            warning: None,
        };
    }

    MeasuredResolution {
        colors: fallback,
        source: fallback_source,
        warning: Some(format!(
            "colors_actual returned by the script has {} usable entries but the \
             resolved palette has {}; ignoring it and falling back to {}. \
             (Entries that are not 6-digit hex are dropped, which also shortens \
             the list.)",
            parsed.len(),
            palette_len,
            fallback_source
        )),
    }
}
```

- [ ] **Step 4: Run to verify all six pass**

Run: `cargo test --lib api::display -- --test-threads=4`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/api/display.rs
git diff --cached --stat
git commit -m "feat: add resolve_measured_colors, the measured-colour chain as a pure function"
```

---

## Task 4: Wire the script layer into all four render paths

**Files:**
- Modify: `src/api/display.rs` (`resolve_render_params` signature ~line 179; call site ~line 761; `measured_source` block ~line 575-596)
- Modify: `src/api/dev.rs` (call site ~line 685)
- Modify: `src/services/screen_store.rs` (call site ~line 1100)
- Modify: `src/main.rs` (call site ~line 350)
- Test: `tests/lua_api_test.rs` (end-to-end through `ScreenStore::render`, added in Task 6's file if easier — see note)

**Interfaces:**
- Consumes: `MeasuredResolution` / `resolve_measured_colors` (Task 3), `script_colors_actual` (Task 2).
- Produces: `resolve_render_params` gains a **first** parameter `script_colors_actual: Option<&[String]>` and returns `RenderParams` whose `measured_colors` is the resolved value. It also gains a companion out-param: rather than growing the return type, it takes `warning_sink: &mut Option<String>`.

  **Decide this explicitly and do not improvise:** `resolve_render_params` already has ten parameters and an `#[allow(clippy::too_many_arguments)]`. Adding two more is ugly but keeps every call site honest. Do **not** silently swallow the warning.

- [ ] **Step 1: Change the signature**

In `src/api/display.rs`:

```rust
#[allow(clippy::too_many_arguments)]
pub fn resolve_render_params(
    script_colors: Option<&[String]>,
    script_colors_actual: Option<&[String]>,
    script_dither: Option<&str>,
    script_preserve_exact: Option<bool>,
    device_config_colors: Option<&str>,
    device_config_dither: Option<&str>,
    panel_colors: Option<&str>,
    fallback_palette: &[(u8, u8, u8)],
    measured_colors: Option<Vec<(u8, u8, u8)>>,
    measured_source: &'static str,
    preserve_exact_override: Option<bool>,
    tuning: &DitherTuningValues,
    warning_sink: &mut Option<String>,
) -> RenderParams {
```

Inside, after `palette` is computed and before the `RenderParams` literal:

```rust
    let measured = resolve_measured_colors(
        script_colors_actual,
        palette.len(),
        measured_colors,
        measured_source,
    );
    *warning_sink = measured.warning;
```

and in the `RenderParams` literal replace `measured_colors,` with `measured_colors: measured.colors,`.

Also update the doc comment's chain line to:

```
/// Palette:  script_colors > device_config_colors > panel_colors > fallback
/// Measured: script_colors_actual > (caller's pre-script chain)
```

- [ ] **Step 2: Compile to find every call site**

Run: `cargo build 2>&1 | grep -n "error" | head -40`
Expected: four errors, at `src/main.rs`, `src/api/display.rs`, `src/api/dev.rs`, `src/services/screen_store.rs`. Fix them in the next steps. Do not guess at the list — use what the compiler prints.

- [ ] **Step 3: Fix `src/api/display.rs`'s own call site (production device path)**

The existing `measured_source` block already computes a `&'static str`; change its declaration from `let measured_source;` to a plain binding if the compiler complains about definite assignment, and pass it through. At the call site:

```rust
                    let mut measured_warning: Option<String> = None;
                    let params = resolve_render_params(
                        result.script_colors.as_deref(),
                        result.script_colors_actual.as_deref(),
                        eff_script_dither,
                        result.script_preserve_exact,
                        dc_colors.as_deref(),
                        eff_dc_dither,
                        panel_colors_for_chain.as_deref(),
                        &fallback,
                        measured_colors.clone(),
                        measured_source,
                        None,
                        &tuning,
                        &mut measured_warning,
                    );
                    if let Some(w) = &measured_warning {
                        tracing::warn!(device = %mac, "{w}");
                    }
```

- [ ] **Step 4: Fix `src/api/dev.rs`**

`dev.rs` computes `measured_colors` but has no `measured_source` string. Add one alongside the hoisted block from Task 1 Step 8:

```rust
    let measured_source: &'static str = if query.colors_actual.is_some() {
        "dev_override"
    } else if panel.as_ref().and_then(|p| p.colors_actual.as_deref()).is_some() {
        "panel.colors_actual"
    } else {
        "none"
    };
```

Then pass `script_colors_actual` (the dev path already destructures `script_colors` from its script result — add the parallel binding the same way), `measured_source`, and a `&mut Option<String>` whose contents you log with `tracing::warn!`.

- [ ] **Step 5: Fix `src/services/screen_store.rs` — and surface the warning into the script log**

This is the authoring path, and the **only** one where the warning must reach the user rather than the server log:

```rust
        let mut measured_warning: Option<String> = None;
        let render_params = crate::api::display::resolve_render_params(
            script_result.script_colors.as_deref(),
            script_result.script_colors_actual.as_deref(),
            effective_script_dither,
            script_result.script_preserve_exact,
            None,
            dither_override,
            panel_colors.as_deref(),
            &query_palette,
            measured_colors.clone(),
            measured_source,
            opts.preserve_exact,
            &tuning,
            &mut measured_warning,
        );
```

`log` is bound earlier as `let log = script_result.logs.clone();`. Change it to `let mut log = ...` and, immediately after the call:

```rust
        // The authoring path's warning channel is the script log, which
        // render_screen returns to the agent — not the server's log stream.
        if let Some(w) = measured_warning {
            log.push(format!("[warn] {w}"));
        }
```

`measured_source` in this file: the existing `measured_colors` comes from the panel, so `"panel.colors_actual"` when `measured_colors.is_some()`, else `"none"`. Task 6 extends this with the `RenderOpts` layer.

- [ ] **Step 6: Fix `src/main.rs`**

Pass `script_result.script_colors_actual.as_deref()`, `"panel.colors_actual"` or `"none"` for the source, and `&mut Option<String>` logged via `tracing::warn!`.

- [ ] **Step 7: Write the end-to-end test**

Add to `tests/lua_api_test.rs`:

```rust
#[test]
fn test_script_colors_actual_length_mismatch_appears_in_log_and_falls_back() {
    // The script returns a 4-entry palette but only 2 measured colours.
    // The render must succeed, the calibration must fall back, and the
    // author must be told — in the script log, not in the server's log.
    let script = r#"
        return {
            data = {},
            colors        = { "#000000", "#555555", "#AAAAAA", "#FFFFFF" },
            colors_actual = { "#0A0A0A", "#E8E6E0" },
            refresh_rate  = 60
        }
    "#;

    let (_temp_dir, asset_loader) = setup_test_env(&[("test_mismatch.lua", script)]);
    let runtime = LuaRuntime::new(asset_loader);

    let result = runtime
        .run_script_from_asset(
            std::path::Path::new("test_mismatch.lua"),
            &HashMap::new(),
            None,
            None,
        )
        .expect("Script must still run");

    // The runtime itself does not judge lengths — it just carries the value.
    assert_eq!(result.colors_actual.as_ref().unwrap().len(), 2);
    assert_eq!(result.colors.as_ref().unwrap().len(), 4);
}
```

> **Note for the implementer:** this test only pins the runtime half. The *behavioural* half — the warning reaching `RenderResult::log` — is asserted in Task 6, which is where a `ScreenStore::render` harness is already in hand (`tests/common/store.rs`). If you can reach `ScreenStore::render` cheaply here, assert it here instead and say so; earlier is better.

- [ ] **Step 8: Full check**

Run: `make check`
Expected: clean. Report the new test total.

- [ ] **Step 9: Commit**

```bash
git add src/api/display.rs src/api/dev.rs src/services/screen_store.rs src/main.rs tests/lua_api_test.rs
git diff --cached --stat
git commit -m "feat: script-returned colors_actual wins the measured-colour chain"
```

---

## Task 5: Stop `svg_to_png` dropping calibration silently

**Files:**
- Modify: `src/rendering/svg_to_png.rs` (`build_eink_palette`, the `let eink_actual = ...` binding, ~line 341)
- Test: inline `#[cfg(test)] mod tests` in `src/rendering/svg_to_png.rs`

**Interfaces:** none — internal behaviour only.

This is the last-ditch guard *behind* Task 4's length check. After Task 4 it should be unreachable from the script path; if it fires, that is information worth having.

- [ ] **Step 1: Write the failing test**

`build_eink_palette` is a private free function in this module, so test it directly from the module's own `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn build_eink_palette_drops_mismatched_actual_but_still_builds() {
        // Three official colours, two measured: the measured list is dropped
        // (never fail a device render) — but this must not be silent.
        let official = vec![(0, 0, 0), (255, 255, 255), (255, 0, 0)];
        let actual = vec![(10, 10, 10), (232, 230, 224)];
        let (_palette, output) =
            build_eink_palette(&official, Some(&actual), false).expect("must still build");
        assert_eq!(output, official);
    }

    #[test]
    fn build_eink_palette_keeps_matched_actual() {
        let official = vec![(0, 0, 0), (255, 255, 255), (255, 0, 0)];
        let actual = vec![(10, 10, 10), (232, 230, 224), (168, 58, 48)];
        let (_palette, output) =
            build_eink_palette(&official, Some(&actual), true).expect("must build");
        // use_actual = true draws the output in the measured colours, except
        // that pure black/white are forced to match — the existing B&W rule
        // applies to the dither palette, while `output` uses raw measured.
        assert_eq!(output, actual);
    }
```

- [ ] **Step 2: Run to see the current behaviour**

Run: `cargo test --lib rendering::svg_to_png -- --test-threads=4 --nocapture`
Expected: both PASS against unmodified code — these pin existing behaviour so that adding the warning cannot change it. **State this in your report:** these two are regression pins, not failing-first tests. The failing-first test is Step 3.

- [ ] **Step 3: Add the warning and a test that proves it fires**

Change:

```rust
    let eink_actual = if !unique_actual.is_empty() && unique_actual.len() == unique_official.len() {
        Some(unique_actual.as_slice())
    } else {
        None
    };
```

to:

```rust
    let eink_actual = if !unique_actual.is_empty() && unique_actual.len() == unique_official.len() {
        Some(unique_actual.as_slice())
    } else {
        // Never fail a device render over calibration — but do not lose it
        // silently either. After the length check in
        // `api::display::resolve_measured_colors` this should be unreachable
        // from the script path; if it fires, something upstream disagrees
        // about palette length (e.g. dedup removed a duplicate official
        // colour without a matching measured entry).
        if actual.is_some() {
            tracing::warn!(
                official = unique_official.len(),
                measured = unique_actual.len(),
                "measured colours dropped: length disagrees with the deduplicated \
                 official palette; dithering will target the official colours"
            );
        }
        None
    };
```

Note the `if actual.is_some()` guard: without it, every uncalibrated render logs a warning, which would be noise.

For proving the warning fires, use `tracing_test` if it is already a dev-dependency; otherwise **do not add a dependency for this** — instead assert the guard's logic by keeping the two pins above and stating in your report that the `tracing::warn!` itself is unasserted. Say so plainly rather than writing a test that cannot fail.

- [ ] **Step 4: Run**

Run: `cargo test --lib rendering::svg_to_png -- --test-threads=4`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/rendering/svg_to_png.rs
git diff --cached --stat
git commit -m "fix: warn instead of silently dropping mismatched measured colours"
```

---

## Task 6: `use_actual` and `colors_actual` on `render_screen`

**Files:**
- Modify: `src/services/screen_store.rs` (`RenderOpts` ~line 117-150; `Default` impl; `render` measured resolution ~line 968 and `let use_actual = ...` ~line 1114)
- Modify: `src/mcp/tools_render.rs` (`RenderArgs`, `render_screen` description and body)
- Test: `tests/mcp_tools_test.rs`

**Interfaces:**
- Consumes: `resolve_measured_colors` (Task 3), the wiring from Task 4.
- Produces: `RenderOpts { use_actual: Option<bool>, colors_actual: Option<String>, .. }` and the matching `RenderArgs` fields.

**Semantics, exactly:**
- `colors_actual` (comma-separated hex) occupies the **dev-override slot**: `script > RenderOpts.colors_actual > panel.colors_actual > none`. `measured_source` reports `"render_opts"` when it is used.
- `use_actual` controls **only which palette the returned PNG is drawn in**, never the dithering decisions. `None` preserves today's behaviour exactly: `measured_colors.is_some()`.
- `use_actual: Some(true)` with no measured colours available is a no-op, not an error — mirroring `dev.rs`, which does `.unwrap_or_else(|| measured_colors.is_some()) && measured_colors.is_some()`.

- [ ] **Step 1: Write the failing tests**

Add to `tests/mcp_tools_test.rs`. The harness is `TestApp::new_admin("secret")` + `McpTestClient`; `call_tool` returns the JSON-RPC `result` object, whose image blocks look like `{"type": "image", "mimeType": "image/png", "data": "<base64>"}`. This mirrors the existing `test_render_screen_returns_an_image_block`.

```rust
/// The base64 payload of the first image content block.
fn first_image_b64(result: &serde_json::Value) -> String {
    result["content"]
        .as_array()
        .expect("content array")
        .iter()
        .find(|c| c["type"] == "image")
        .expect("a successful render must return an image block")["data"]
        .as_str()
        .expect("image data is a base64 string")
        .to_string()
}

#[tokio::test]
async fn test_render_screen_colors_actual_without_a_configured_panel() {
    // An authoring agent must be able to preview a calibration without
    // first writing a panel into config.yaml.
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "render_screen",
            serde_json::json!({
                "screen_ref": "byonk-builtin/default",
                "colors_actual": "#0A0A0A,#E8E6E0,#A83A30,#3F7A45",
                "use_actual": true,
                "timestamp": 1_750_000_000,
            }),
        )
        .await;

    assert_ne!(result["isError"], serde_json::json!(true), "{result}");
    let b64 = first_image_b64(&result);
    assert!(b64.starts_with("iVBORw0KGgo"), "must be a PNG");
}

#[tokio::test]
async fn test_render_screen_use_actual_changes_the_output_palette() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    // A fixed timestamp so the only difference between the two renders is
    // the palette — the default screen draws the time.
    let render = |use_actual: bool| {
        client.call_tool(
            "render_screen",
            serde_json::json!({
                "screen_ref": "byonk-builtin/default",
                "colors_actual": "#0A0A0A,#E8E6E0,#A83A30,#3F7A45",
                "use_actual": use_actual,
                "timestamp": 1_750_000_000,
            }),
        )
    };

    let with = render(true).await;
    let without = render(false).await;

    assert_ne!(
        first_image_b64(&with),
        first_image_b64(&without),
        "use_actual must change the palette the PNG is drawn in"
    );
}

#[tokio::test]
async fn test_render_screen_default_still_matches_no_use_actual() {
    // The default must preserve today's behaviour exactly: on when measured
    // colours resolved. Omitting the flag and passing true must agree.
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let omitted = client
        .call_tool(
            "render_screen",
            serde_json::json!({
                "screen_ref": "byonk-builtin/default",
                "colors_actual": "#0A0A0A,#E8E6E0,#A83A30,#3F7A45",
                "timestamp": 1_750_000_000,
            }),
        )
        .await;
    let explicit = client
        .call_tool(
            "render_screen",
            serde_json::json!({
                "screen_ref": "byonk-builtin/default",
                "colors_actual": "#0A0A0A,#E8E6E0,#A83A30,#3F7A45",
                "use_actual": true,
                "timestamp": 1_750_000_000,
            }),
        )
        .await;

    assert_eq!(first_image_b64(&omitted), first_image_b64(&explicit));
}
```

> If `byonk-builtin/default` turns out to render identically under both palettes (it may draw only greys, in which case the four-colour override changes nothing), **that is a harness problem, not a passing test**: switch to a screen that draws colour — `byonk-builtin/calibration/color` exists for exactly this — and say in your report that you did.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test mcp_tools_test render_screen -- --test-threads=4`
Expected: the two new tests fail — unknown arguments are currently ignored by serde, so `use_actual` has no effect and the palettes match.

**Confirm this is a real failure**, not a harness error. If both PNGs differ for an unrelated reason (e.g. a timestamp drawn on the screen), pass an explicit `"timestamp"` to make the render deterministic.

- [ ] **Step 3: Extend `RenderOpts`**

In `src/services/screen_store.rs`:

```rust
    /// Also render a pre-dither, full-color PNG alongside the palette-
    /// restricted `png` (see `RenderResult::raw_png`).
    pub include_raw: bool,
    /// Draw the returned PNG in the measured colours (what the panel will
    /// look like) rather than the spec colours (what is sent to the panel).
    /// `None` keeps the historical default: on when measured colours resolved.
    /// Affects only the output palette, never the dithering decisions.
    pub use_actual: Option<bool>,
    /// Measured-colour override for this render, comma-separated hex. Sits in
    /// the dev-override slot: `script > this > panel.colors_actual > none`.
    /// Lets an agent preview a calibration without writing a panel to config.
    pub colors_actual: Option<String>,
```

Add `use_actual: None,` and `colors_actual: None,` to the `Default` impl.

- [ ] **Step 4: Use them in `ScreenStore::render`**

Replace the existing measured resolution:

```rust
        let measured_colors: Option<Vec<(u8, u8, u8)>> = panel
            .and_then(|p| p.colors_actual.as_deref())
            .map(crate::api::display::parse_colors_header);
```

with:

```rust
        // Dev-override slot for the authoring path: an explicit render option
        // beats the panel, and the script beats both (applied later, in
        // resolve_render_params).
        let (measured_colors, measured_source): (Option<Vec<(u8, u8, u8)>>, &'static str) =
            if let Some(ref ca) = opts.colors_actual {
                (
                    Some(crate::api::display::parse_colors_header(ca)),
                    "render_opts",
                )
            } else if let Some(actual) = panel.and_then(|p| p.colors_actual.as_deref()) {
                (
                    Some(crate::api::display::parse_colors_header(actual)),
                    "panel.colors_actual",
                )
            } else {
                (None, "none")
            };
```

and replace:

```rust
        let use_actual = measured_colors.is_some();
```

with — noting that by this point `render_params.measured_colors` is the *post-script* value, which is what the output palette must follow:

```rust
        // Mirrors dev.rs: an explicit request wins, but only when there is
        // something measured to show. Uses the post-script resolved value,
        // so a script-supplied calibration is what gets previewed.
        let use_actual = opts
            .use_actual
            .unwrap_or_else(|| render_params.measured_colors.is_some())
            && render_params.measured_colors.is_some();
```

Then check the `render_png_from_svg` call below it: it currently passes `measured_colors.as_deref()`. Change it to `render_params.measured_colors.as_deref()` so the script layer actually reaches the ditherer. **This is load-bearing** — without it Task 4's work is inert on the authoring path.

- [ ] **Step 5: Extend the MCP tool**

In `src/mcp/tools_render.rs`, add to `RenderArgs`:

```rust
    /// Draw the returned PNG in the panel's measured colours — what the
    /// screen will actually look like — instead of the spec colours that are
    /// sent to the panel. Defaults to on whenever measured colours are
    /// available (from `panel` or from `colors_actual`). This changes only
    /// how the returned PNG is drawn; it never changes the dithering.
    #[serde(default)]
    pub use_actual: Option<bool>,
    /// Measured panel colours for this render, comma-separated hex (e.g.
    /// `#0A0A0A,#E8E6E0,#A83A30`), index-parallel to the palette. Use this to
    /// preview a calibration without adding a panel to the config. A
    /// `colors_actual` returned by the screen's own script wins over this.
    #[serde(default)]
    pub colors_actual: Option<String>,
```

and to the `RenderOpts` literal in `render_screen`:

```rust
            use_actual: a.use_actual,
            colors_actual: a.colors_actual,
```

Extend the tool `description` with one sentence:

```
By default the returned PNG shows what the panel will actually look like when \
measured colours are available; pass use_actual=false to see the spec colours \
that are sent to the panel instead.
```

- [ ] **Step 6: Run the tests**

Run: `cargo test --test mcp_tools_test render_screen -- --test-threads=4`
Expected: PASS, including the existing `test_tools_list_reports_exactly_the_14_authoring_tools` — this task adds **arguments**, not tools, so that count must stay 14. If it changed, you added a tool by accident.

- [ ] **Step 7: Full check**

Run: `make check`
Expected: clean. Report the total.

- [ ] **Step 8: Commit**

```bash
git add src/services/screen_store.rs src/mcp/tools_render.rs tests/mcp_tools_test.rs
git diff --cached --stat
git commit -m "feat: explicit use_actual and colors_actual on render_screen"
```

---

## Task 7: Documentation

**Files:**
- Modify: `docs/src/api/lua-api.md`
- Modify: `CHANGES.md`

- [ ] **Step 1: Document `device.colors_actual`**

In `docs/src/api/lua-api.md`, in the `### device` section, immediately after the `device.colors` entry, add:

````markdown
#### device.colors_actual

The colours the panel **really** shows, as measured — index-parallel to
`device.colors`. `nil` when the panel has no measured colours configured.

This is deliberately **not** filled in from `device.colors` when absent, so a
script can tell an uncalibrated panel from one that measures exactly to spec:

```lua
local shown = device.colors_actual or device.colors

-- Pick a foreground that genuinely contrasts on this panel, not one that
-- only contrasts in the spec.
local bg = shown[1]
local fg = shown[2]
```

Resolution order: a `colors_actual` returned by this script (see below) >
the dev colour-tuning override > `panel.colors_actual` in `config.yaml` >
the `Measured-Colors` header > none.
````

- [ ] **Step 2: Document the return value**

In the `## Script Return Value` section, after the entry documenting `colors`, add:

````markdown
### colors_actual

Overrides the measured colours used for dithering, for this render only:

```lua
return {
  data = { ... },
  colors        = { "#000000", "#FFFFFF", "#FF0000", "#00FF00" },
  colors_actual = { "#0A0A0A", "#E8E6E0", "#A83A30", "#3F7A45" },
}
```

Must have the same number of entries as the resolved palette. If it does not,
the render still succeeds: the value is ignored, the next source in the chain
is used, and a warning is written to the script log (visible in the MCP
`render_screen` tool's `log` and in dev mode). Entries that are not 6-digit
hex are dropped, which shortens the list and therefore trips the same check.

A script that returns `colors_actual` wins over the dev colour-tuning popup —
the dev UI reports the source as `script`, so this is visible rather than
mysterious.
````

- [ ] **Step 3: CHANGES.md**

Under `## [Unreleased]`, `### Added` — **user-facing wording only**, no mention of refactors, chains, or internal functions:

```markdown
- Lua scripts can now read a panel's measured colours via `device.colors_actual`
  and override them by returning `colors_actual`, so a screen can adapt to — or
  retune — what the display really shows.
- The `render_screen` MCP tool gained `use_actual` and `colors_actual`, so an
  authoring agent can preview a screen exactly as the panel will show it,
  without first configuring a panel.
```

Under `### Fixed`:

```markdown
- Measured panel colours are no longer discarded without explanation when a
  screen's palette and its measured colours disagree in length; the mismatch is
  now reported.
```

- [ ] **Step 4: Build the docs**

Run: `make docs`
Expected: builds clean (needs `mdbook-mermaid`, already installed).

- [ ] **Step 5: Commit**

```bash
git add docs/src/api/lua-api.md CHANGES.md
git diff --cached --stat
git commit -m "docs: document device.colors_actual, the colors_actual return, and render_screen colour options"
```

---

## Final verification

- [ ] `make check` — fmt clean, clippy clean, **all tests pass**. Report the exact count.
- [ ] `make docs` — clean.
- [ ] `git status` — working tree clean; **no untracked file was swept into a commit** (`git log --stat` over this plan's commits to confirm).
- [ ] Confirm `test_tools_list_reports_exactly_the_14_authoring_tools` still passes — this plan adds arguments, never tools.
- [ ] Report, honestly and specifically: any test you wrote that you believe could pass against broken code, and any step in this plan you found to be wrong.
