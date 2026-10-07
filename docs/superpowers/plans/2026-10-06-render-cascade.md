# Render Resolution Cascade (piece 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace byonk's five disagreeing render-setting precedence chains with one six-rung cascade (`override → device final → screen → device base → panel → fallback`), make the `final:` veto rung reachable from every writer, and tell the truth about it in the docs.

**Architecture:** Piece 2 already built the ladder (`src/api/resolution.rs`): `Rung`, `Ladder<T>`, `resolve()`, the `ORDER_*` constants and `resolve_dev_render`, which the dev render path already uses. Piece 3 does three things. (1) It adds the missing `device_final` rung to that resolver and flips two `ORDER_*` constants from today's legacy orders to the spec's. (2) It moves the other four render paths off their legacy resolvers onto the same function, then deletes the legacy resolvers, so a convergence test can assert all five paths produce identical `RenderParams` from one fixture. (3) It makes `final:` writable — through the admin PATCH, the MCP mirror and Home Assistant — which is a seven-site coupled change in `write.rs` whose most likely failure is silent.

**Tech Stack:** Rust (axum, serde_yaml 0.9.34, serde 1.0.228), `eink-dither` workspace crate, Python (Home Assistant custom component), mdBook.

**Spec:** `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md` — piece 3 is §2, §3, §4.1–4.3, §4.8, defects 5.3/5.4 and §6. Read §2 and §3 before Task 1 and §4.8 before Task 9.

**Scope note — §4.7 is NOT in this plan.** The spec bundles the `DevOverrides` TTL and the panel-keyed cascade map into piece 3, but neither is part of the flip: they govern how long a dev-UI push survives, not which rung wins. They get their own plan after this one, so this plan stays reviewable and this branch keeps producing working software at every task boundary. Nothing here depends on them; `/api/display`'s override rung keeps reading exactly the maps it reads today (`DevOverrides::dither`, `::tuning`, `::panel_colors`).

**Already done, do not redo.** The spec names work that landed on the parent branches. Verify rather than implement:
- §4.1's two broken algorithm orderings (`main.rs:429-432`, `screen_store.rs:1377`) — **FIXED** in `1486cad7` via the shared `display::effective_dither`. Both call sites now pass the same two slots they render with.
- Defect 5.3 (whole-block tuning replacement) — **FIXED**: `resolve_effective_tuning` is now `override_tuning.or(&resolve_tuning(…))`, field by field, and `/api/display`'s inline copy at the old `:1048` is gone.
- Defect 5.4 (authoring preview cannot override `strength`) — **FIXED**: `screen_store.rs` now passes `strength: opts.strength`.
- Defects 5.1 / 5.2 — fixed in `cd01ce9b` (piece 1).

---

## Rulings this plan makes (read before Task 1)

The spec is the historical record and is never amended to match an implementation (Ruling 56). Where this plan decides something the spec left open, or departs from it, it is recorded here.

1. **`resolve_dev_render` is renamed, not duplicated.** It becomes `resolve_render`, `DevRenderInputs` becomes `RenderInputs` and `DevRenderResolution` becomes `RenderResolution`. The "dev" names were piece 2's honest scoping; keeping them while four production paths call the function would be a lie in the type name. One resolver means one name.

2. **`measured_source` keeps today's `SRC_*` label strings, mapped from the winning rung.** `RenderParams::measured_source` is surfaced in the MCP `render_screen` tool's diagnostics and in server logs. The cascade could rename the labels after the rungs, but that would be a second user-visible change in a release whose migration note promises exactly one. The mapping is `Screen → SRC_SCRIPT`, `DeviceFinal`/`DeviceBase` → the new `SRC_DEVICE`, `Panel → SRC_PANEL_ACTUAL`, `Fallback → SRC_MEASURED_HEADER` when the fallback slot was filled and `SRC_NONE` otherwise. `Override` maps to a label each path supplies (`SRC_DEV_OVERRIDE` on `/api/display` and the admin preview, `SRC_RENDER_OPTS` in `ScreenStore::render` and the CLI), because the override rung genuinely has a different origin per path and the diagnostic exists to name that origin.

3. **`refresh` resolves through the shared `resolve()` and `ORDER_REFRESH`, but stays in `content_pipeline`.** §4.1 says `resolve_refresh_rate` "collapses into one resolution over `CascadeValues`". It cannot literally move into `resolve_render`: the screen rung for `refresh` is the script's return value, which only exists after `content_pipeline` has run the script, while `resolve_render` runs on the render side. `resolve_refresh_rate` is therefore reimplemented as a `Ladder<u32>` walked by the shared `resolve()` with the shared `ORDER_REFRESH` — one order constant and one walk, which is the part that stops the chains drifting. `resolve_render` keeps resolving `refresh` for the dev path, which has the script result in hand.

4. **`ignored_device_override` survives this piece.** §2 says the log line "becomes redundant once the ladder widget shows the same fact; it should stay until the widget ships". The widget is the next piece of work, not this one, so the line stays, and now reports a displaced **base** value only — a displaced `final.refresh` is impossible, since nothing outranks it.

5. **`final:` merges field by field and is cleared by one dot** — the spec's own two rulings in §4.8, restated here because they are the two decisions a `write.rs` implementer is most likely to get wrong. `clear: ["dither"]` clears the base rung only; `clear: ["final.dither"]` the final rung's field; `clear: ["final"]` the whole rung.

6. **The piece-2 equivalence test is retired, not weakened.** `resolve_dev_render` is pinned to the legacy resolver by an equivalence matrix. Task 2 flips `ORDER_DITHER`, so that test's dither rows become wrong *by design*. Task 2 inverts those rows and marks them; Task 8 deletes the test when the legacy resolvers are deleted, because by then there is nothing left to be equivalent to — the five-paths convergence test takes over the job and is strictly stronger. **Do not shrink the matrix to make it pass at any point in between** (the previous branch's hardest-won lesson).

7. **Device `colors_actual` is a plain base-rung string, not a panel-style measurement record.** The panel's `colors_actual` is a calibration of the hardware; a device's is an owner override of it for one display. Same type, same validity rule, one rung apart.

8. **The `final:` block is strict, the base rung is tolerant.** `CascadeValues` already carries `#[serde(deny_unknown_fields)]`, so `final: {screen: …}` is a startup error naming the field. The base rung keeps today's tolerant behaviour. This asymmetry is the spec's §3 decision; making the base rung strict is explicitly a separate, larger decision and is **not** in this plan.

## Global Constraints

- **Breaking release.** Exactly one behaviour changes: *a bare `devices.<key>.dither` no longer beats a screen that sets its own; move it to `final:` to keep the old behaviour.* Every other change is additive. If a task's test shows a second behaviour change, that is a bug in the task, not a new migration note.
- **`refresh` is a zero-behaviour-change move.** Existing `devices.<key>.refresh` values stay at the base rung, where the script still beats them. A test asserts this.
- **`deny_unknown_fields` on `CascadeValues`, never on the base rung.**
- **No `#[serde(flatten)]` for the base rung.** Measured on serde 1.0.228 + serde_yaml 0.9.34: flatten reports `devices.AA: invalid type: string "oops", expected f32 at line 3 column 5` where the enumerated form reports `devices.AA.strength: … at line 5 column 15`. In add-on mode that message is the only diagnostic the operator ever sees.
- **Per-algorithm tuning stays panel-only.** Device base, device final and override take flat values.
- **Algorithm first, then its numbers**, in all five paths: the per-algorithm panel block is looked up for the algorithm that is actually rendered.
- **Comments and identifiers in English.** User-facing strings may be localised; nothing here is.
- **Verification is never `make check` alone** (it has reported exit 0 with failing tests). The sequence is `make webui`, then `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `(cd docs && mdbook build)`, then `make webui-test` — the Rust suite and `make webui-test` **one after the other, never in parallel** (under load the journey tests time out at 5 s and look like real failures).
- **Never `git add -A` or `git add .`.** Four files are permanently modified in the maintainer's tree and must never be staged: `config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`. Check `git diff --cached --stat` before every commit. `content-pipeline.md` is both a permanently-modified file and a doc Task 12 must change — Task 12 says how.
- **CHANGES.md** records user-visible changes against v0.19.0, and this piece's entry must name the Home Assistant consequence.

## Review Focus

Five input classes the spec implies but no task's own happy path exercises. Each one's test is added to the task that owns the code, named in brackets.

1. **A `final:` block that sets one field, on a device that already has other `final:` fields, PATCHed.** Field-by-field merge plus the `unmanaged_device_keys` trap means the most likely implementation silently keeps the old block and discards the patch. [Task 9]
2. **`clear: ["dither"]` on a device with both rungs set.** A bare name must clear the base rung and leave `final.dither` standing. The symmetric error — clearing both — is indistinguishable from success in the response. [Task 9]
3. **A wrong-length `colors_actual` on the device base rung, with a valid one on the panel rung.** The base rung must be reported `rejected` and the panel's must render. Piece 2 pinned this for the dev path only; `/api/display` has its own history of it. [Task 3]
4. **A device with `final.dither` whose screen sets a different algorithm, where the panel has a per-algorithm block for the `final` one.** The veto must win *and* drag its own tuning block with it. This is the single input where §4.2's two-pass shape and §2's new order interact. [Task 8]
5. **A `final:` change with a warm preview cache.** `fingerprint()` not hashing `final` serves a stale image until the TTL, with no error — the exact failure mode that function's doc comment says "only shows up on the glass hours later". [Task 6]

## Spec §7 coverage

§7 lists twelve tests. Where each one lands, so a reviewer can tell a gap from a deliberate placement:

| §7 bullet | Task |
|---|---|
| A resolution table test, per setting × per rung | 2 |
| `dither` flipping, named explicitly | 2 |
| `refresh` is unchanged | 2 (resolver) + 7 (`content_pipeline`) |
| Field by field, not block by block — "on `/api/display` as well as the dev path" | 2 at the resolver, and **8**: the convergence fixture takes `max_error` from the screen and `strength` from the device base rung in the same render, asserted on all five paths at once. That is stronger than a per-path test and is why there is no separate one in Task 3. |
| `colors_actual`'s validity rule | 3 |
| Algorithm-then-numbers, per path | 8 — the fixture's panel has `atkinson: {noise_scale: 0.25}` and `stucki: {noise_scale: 0.75}`, so a path that looks the block up for the loser fails. §4.1's two broken orderings were already fixed in `1486cad7`. |
| The five call paths agree | 8 |
| The preview cache fingerprint covers `final` and `colors_actual` | 6 |
| `clear` addressing, plus the extended invariant test | 9 |
| A PATCH setting `final` survives `unmanaged_device_keys` | 9 |
| The MCP tool can set `final`, `colors_actual`, `gamut` | 10 |
| Override TTL | **not this plan** — §4.7, deferred with the rest of it |

## File Structure

**Modified — the resolver**
- `src/api/resolution.rs` — gains the `device_final` rung, the spec's `ORDER_DITHER`/`ORDER_COLORS_ACTUAL`, the rung→`SRC_*` mapping, and the renames from ruling 1. Stays the single place a precedence order is written down.

**Modified — the config shape**
- `src/models/config.rs` — `DeviceConfig` gains `colors_actual` and `final_rung`; `base()` carries `colors_actual`.

**Modified — the five render paths** (each builds `RenderInputs` and converts `RenderResolution` to its own output type; none of them computes a setting on its own any more)
- `src/api/display.rs` — `/api/display`; also loses the §4.3 conflict warning and, in Task 8, the legacy `resolve_render_params` / `resolve_effective_tuning` / `resolve_tuning` / `effective_dither`.
- `src/api/dev.rs` — follows the renames only.
- `src/services/screen_store.rs` — authoring + device preview.
- `src/main.rs` — CLI render.
- `src/api/admin/preview.rs` — admin device preview; `DevicePreview` gains the final rung and `colors_actual`, and `fingerprint()` hashes them.
- `src/services/content_pipeline.rs` — `refresh` on the shared order (ruling 3); `DeviceContext` gains the final rung's refresh.

**Modified — the write surface** (§4.8's seven coupled sites plus its two mirrors)
- `src/api/admin/write.rs` — `DeviceWrite`, `MANAGED_DEVICE_KEYS`, `CLEARABLE_DEVICE_KEYS`, `device_block`, the merge struct, `resolve_clear`, `apply_clear`, and the invariant test.
- `src/mcp/tools_device.rs` — the hand-written `ConfigureDeviceArgs` mirror.
- `custom_components/byonk/select.py`, `custom_components/byonk/config_flow.py` — the Dither control moves to `final.dither`.

**New — tests**
- `tests/render_cascade_convergence_test.rs` — Task 8's five-paths-agree test. Its own file because it is the highest-value test in the spec and must not be buried in a module someone trims.

**Modified — docs**
- `docs/src/api/lua-api.md`, `docs/src/guide/configuration.md`, `docs/src/concepts/content-pipeline.md`, `CHANGES.md`.

---

### Task 1: `final:` and device `colors_actual` in the config shape

Read spec §3 first. This task only teaches the config to *hold* the two new rungs; nothing reads them yet, so behaviour is unchanged and the suite must stay green.

**Files:**
- Modify: `src/models/config.rs` — `DeviceConfig` (the struct around `:60-120`), `DeviceConfig::base()` (`:126-138`)
- Test: `src/models/config.rs` (the existing `mod tests` at `:749`)

**Interfaces:**
- Consumes: `CascadeValues` (already present, `src/models/config.rs:109`).
- Produces:
  - `DeviceConfig::colors_actual: Option<String>` — the base rung's measured colours.
  - `DeviceConfig::final_rung: Option<CascadeValues>`, deserialised from the YAML key `final`.
  - `DeviceConfig::base() -> CascadeValues` — unchanged signature, now carrying `colors_actual`.
  - `DeviceConfig::device_final() -> CascadeValues` — the final rung, or `CascadeValues::default()` when absent. Tasks 3–7 call this so no path has to write `.clone().unwrap_or_default()` five times.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src/models/config.rs`:

```rust
#[test]
fn device_final_rung_parses_from_the_final_key() {
    let yaml = r#"
screen: local/hello
dither: floyd-steinberg
final:
  dither: atkinson
  refresh: 1800
"#;
    let dc: DeviceConfig = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(dc.dither.as_deref(), Some("floyd-steinberg"), "base rung");
    let f = dc.final_rung.expect("final rung present");
    assert_eq!(f.dither.as_deref(), Some("atkinson"));
    assert_eq!(f.refresh, Some(1800));
    // Untouched fields of the final rung stay None, so the resolver walks
    // past them to a lower rung instead of vetoing with nothing.
    assert_eq!(f.strength, None);
}

#[test]
fn absent_final_key_is_none_not_an_empty_rung() {
    let dc: DeviceConfig = serde_yaml::from_str("screen: local/hello").expect("parses");
    assert!(dc.final_rung.is_none());
    // …and `device_final()` still answers, so callers need no branch.
    assert_eq!(dc.device_final(), CascadeValues::default());
}

/// The asymmetry spec §3 asks for: the `final:` block is strict so that a
/// non-cascade key there is a startup error naming the field, because in
/// add-on mode that message is the only diagnostic the operator sees.
#[test]
fn a_non_cascade_key_in_final_is_an_error_naming_the_field() {
    let yaml = r#"
screen: local/hello
final:
  screen: local/other
"#;
    let err = serde_yaml::from_str::<DeviceConfig>(yaml)
        .expect_err("`final.screen` must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("screen"),
        "the error must name the offending field, got: {msg}"
    );
}

/// The base rung keeps today's tolerant behaviour. Making it strict is a
/// separate, larger decision about every existing config.yaml (spec §3).
#[test]
fn an_unknown_key_on_the_base_rung_is_still_tolerated() {
    let yaml = r#"
screen: local/hello
some_future_key: 42
"#;
    serde_yaml::from_str::<DeviceConfig>(yaml).expect("base rung stays tolerant");
}

#[test]
fn device_colors_actual_is_a_base_rung_string() {
    let yaml = r#"
screen: local/hello
colors_actual: "#1C5240,#FFFFFF"
"#;
    let dc: DeviceConfig = serde_yaml::from_str(yaml).expect("parses");
    assert_eq!(dc.colors_actual.as_deref(), Some("#1C5240,#FFFFFF"));
}

/// `base()` is how every render path reads the base rung. A field it forgets
/// is a setting that silently stops working at that rung only.
#[test]
fn base_carries_every_cascade_field_including_the_new_ones() {
    let yaml = r#"
screen: local/hello
colors: "#000000,#FFFFFF"
colors_actual: "#1C5240,#FEFEFE"
dither: stucki
max_error: 1.5
noise_scale: 0.6
chroma_clamp: 2.0
strength: 0.9
refresh: 900
gamut:
  knee: 0.8
"#;
    let dc: DeviceConfig = serde_yaml::from_str(yaml).expect("parses");
    let b = dc.base();
    assert_eq!(b.colors.as_deref(), Some("#000000,#FFFFFF"));
    assert_eq!(b.colors_actual.as_deref(), Some("#1C5240,#FEFEFE"));
    assert_eq!(b.dither.as_deref(), Some("stucki"));
    assert_eq!(b.max_error, Some(1.5));
    assert_eq!(b.noise_scale, Some(0.6));
    assert_eq!(b.chroma_clamp, Some(2.0));
    assert_eq!(b.strength, Some(0.9));
    assert_eq!(b.refresh, Some(900));
    assert_eq!(b.gamut.knee, Some(0.8));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib models::config::tests 2>&1 | tail -20`
Expected: FAIL — `no field `final_rung` on type `DeviceConfig``, `no method named `device_final``, `no field `colors_actual``.

- [ ] **Step 3: Add the two fields and the accessor**

In `src/models/config.rs`, immediately after the existing `colors` field of `DeviceConfig`:

```rust
    /// Optional measured-colour override for this one display,
    /// comma-separated hex. The **base rung** of `colors_actual`: it outranks
    /// the panel's calibration (which describes the hardware type) and is
    /// outranked by a screen that measures its own, and by `final:`.
    ///
    /// Same validity rule as every other rung: an entry count that does not
    /// match the resolved palette is reported `rejected` and the resolver
    /// walks on down (spec §4.4).
    pub colors_actual: Option<String>,
```

At the end of the `DeviceConfig` field list, after `min_png_bytes`:

```rust
    /// The **final rung** — the owner's veto: "whatever the screen says, on
    /// *this* display, no." Nothing outranks it but a live dev-UI override.
    ///
    /// `final` is a reserved word in Rust, hence the rename. Read it through
    /// [`DeviceConfig::device_final`], which saves every call site an
    /// `unwrap_or_default`.
    ///
    /// Strict, unlike the base rung: `CascadeValues` carries
    /// `deny_unknown_fields`, so `final: {screen: …}` fails at startup with
    /// the field named rather than being silently dropped (spec §3).
    #[serde(rename = "final", default)]
    pub final_rung: Option<CascadeValues>,
```

Then extend the `impl DeviceConfig` block at `:122`: replace the `colors_actual: None` line in `base()` and add the accessor.

```rust
impl DeviceConfig {
    /// The device's base rung as a `CascadeValues`.
    pub fn base(&self) -> CascadeValues {
        CascadeValues {
            colors: self.colors.clone(),
            colors_actual: self.colors_actual.clone(),
            dither: self.dither.clone(),
            max_error: self.max_error,
            noise_scale: self.noise_scale,
            chroma_clamp: self.chroma_clamp,
            strength: self.strength,
            refresh: self.refresh,
            gamut: self.gamut.clone(),
        }
    }

    /// The device's final rung, or an empty rung when there is none. Every
    /// render path reads it through this, so "no `final:` block" and "a
    /// `final:` block that sets nothing" resolve identically — which they
    /// must, or deleting the last field from the block would change pixels.
    pub fn device_final(&self) -> CascadeValues {
        self.final_rung.clone().unwrap_or_default()
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib models::config::tests 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Confirm nothing else moved**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | grep -E "^test result:|FAILED"`
Expected: clippy clean; 0 failed. Both fields are additive and unread, so any failure here is a real surprise — do not paper over it.

- [ ] **Step 6: Commit**

```bash
git add src/models/config.rs
git diff --cached --stat   # must show ONLY src/models/config.rs
git commit -m "feat(config): the device final rung and a device-level colors_actual

The two new rungs of the cascade, held but not yet read. \`final:\` is
strict (deny_unknown_fields via CascadeValues) so a non-cascade key there
is a startup error naming the field; the base rung stays tolerant.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: the ladder gains the final rung, and the spec's order

Read spec §2 ("Why this ordering") first. **This is the task that flips `dither`.** After it, the dev render path already behaves per spec; the other four still run their legacy resolvers and therefore now *disagree* with the dev path until Tasks 3–7 land. That is expected and temporary; Task 8's convergence test is what proves it is over.

**Files:**
- Modify: `src/api/resolution.rs` — `ORDER_DITHER` (`:243`), `ORDER_COLORS_ACTUAL` (`:234`), `DevRenderInputs` (`:303`), `ladder_of` (`:329`), `resolve_dev_render` (`:349`), and the `Rung` impl
- Modify: `src/api/dev.rs:24` (the import) and `:778-790` (the construction) — renames only
- Test: `src/api/resolution.rs` (`mod tests` at `:470`)

**Interfaces:**
- Consumes: Task 1's `CascadeValues`.
- Produces:
  - `RenderInputs<'a>` (was `DevRenderInputs`) — same fields plus `pub device_final: RungSettings` between `over` and `screen`, and `pub override_source: &'static str` (ruling 2).
  - `RenderResolution` (was `DevRenderResolution`) — same fields plus `pub measured_source: &'static str`.
  - `resolve_render(&RenderInputs) -> RenderResolution` (was `resolve_dev_render`).
  - `Rung::measured_source(&self, override_source: &'static str, fallback_present: bool) -> &'static str`.
  - `ORDER_DITHER` = `[Override, DeviceFinal, Screen, DeviceBase, Panel, Fallback]`.
  - `ORDER_COLORS_ACTUAL` = the same six in the same order — every setting now shares one order.

- [ ] **Step 1: Write the failing resolution-table test**

This is spec §7's first three bullets in one table. Add to `mod tests` in `src/api/resolution.rs`:

```rust
use crate::models::config::{CascadeValues, GamutTuningValues};

/// A distinct value for every cascade setting, keyed by rung number so a
/// winner names its rung unambiguously. `colors` and `colors_actual` get the
/// same entry count, or the validity rule would reject the measurement and
/// mask the ordering this test is about.
fn rung_values(n: u8) -> CascadeValues {
    let hex = format!("#{0:02X}{0:02X}{0:02X},#FFFFFF", n * 16);
    CascadeValues {
        colors: Some(hex.clone()),
        colors_actual: Some(hex),
        dither: Some(DITHER_BY_RUNG[n as usize].to_string()),
        max_error: Some(n as f32),
        noise_scale: Some(n as f32),
        chroma_clamp: Some(n as f32),
        strength: Some(n as f32),
        refresh: Some(100 * n as u32),
        gamut: GamutTuningValues {
            knee: Some(n as f32),
            amount: Some(n as f32),
            max_compression: Some(n as f32),
        },
    }
}

/// Real canonical algorithm names — the resolver normalises and the renderer
/// matches canonical names only, so a made-up name would silently become
/// atkinson and the test would pass for the wrong reason.
const DITHER_BY_RUNG: [&str; 5] = [
    "burkes",          // 0 — unused, keeps the indices readable
    "stucki",          // 1 — panel (never supplied; see below)
    "sierra",          // 2 — device base
    "jarvis-judice-ninke", // 3 — screen
    "atkinson",        // 4 — device final
];
const OVERRIDE_DITHER: &str = "floyd-steinberg";

fn inputs(rungs: &[Rung]) -> (RungSettings, RungSettings, RungSettings, RungSettings) {
    let pick = |r: Rung, n: u8| {
        if rungs.contains(&r) {
            RungSettings::from_cascade(&rung_values(n))
        } else {
            RungSettings::default()
        }
    };
    let mut over = pick(Rung::Override, 5);
    over.dither = Some(OVERRIDE_DITHER.to_string());
    (
        over,
        pick(Rung::DeviceFinal, 4),
        pick(Rung::Screen, 3),
        pick(Rung::DeviceBase, 2),
    )
}

/// Build a `RenderInputs` in which exactly `rungs` carry values. The panel
/// rung supplies colours only — a panel has a `dither:` *block*, not an
/// algorithm name, and its numbers come from `panel_dither` once the
/// algorithm is known (spec §4.2).
fn table_inputs(rungs: &[Rung]) -> RenderInputs<'static> {
    let (over, device_final, screen, device_base) = inputs(rungs);
    let panel = if rungs.contains(&Rung::Panel) {
        RungSettings {
            colors: Some(display::parse_colors_header("#111111,#FFFFFF")),
            colors_actual: Some(display::parse_colors_header("#121212,#FEFEFE")),
            ..Default::default()
        }
    } else {
        RungSettings::default()
    };
    RenderInputs {
        over,
        device_final,
        screen,
        device_base,
        panel,
        panel_dither: None,
        fallback_palette: display::parse_colors_header("#000000,#FFFFFF"),
        fallback_colors_actual: None,
        screen_default_refresh: 900,
        override_source: display::SRC_DEV_OVERRIDE,
    }
}

fn winner(res: &Resolution, setting: &str) -> Rung {
    res.get(setting)
        .unwrap_or_else(|| panic!("no report for `{setting}`"))
        .from
}

/// Every setting resolves through the SAME order. This is the test a newly
/// added setting with a precedence order of its own fails — which is exactly
/// how `dither` came to be inverted relative to its four neighbours.
#[test]
fn resolution_table_every_setting_walks_the_same_six_rungs() {
    const SETTINGS: [&str; 10] = [
        "colors", "colors_actual", "dither", "max_error", "noise_scale",
        "chroma_clamp", "strength", "gamut.knee", "gamut.amount", "refresh",
    ];
    // Highest first. Peel one rung off the top at a time; the next must win.
    // The panel and fallback rungs are per-setting (a panel has no `dither`,
    // no `refresh`), so the walk stops at DeviceBase and the lower rungs get
    // their own assertions below.
    let ladder = [Rung::Override, Rung::DeviceFinal, Rung::Screen, Rung::DeviceBase];
    for peeled in 0..ladder.len() {
        let present = &ladder[peeled..];
        let res = resolve_render(&table_inputs(present)).report;
        let expected = present[0];
        for setting in SETTINGS {
            assert_eq!(
                winner(&res, setting), expected,
                "`{setting}` with rungs {present:?} must be won by {expected:?}"
            );
        }
        // Everything still present below the winner is Outranked, never Absent
        // — a rung the builder forgot reports Absent and would pass a
        // winner-only assertion.
        for setting in SETTINGS {
            let report = &res[setting];
            for lower in &present[1..] {
                let found = report.rungs.iter().find(|(r, _)| r == lower);
                let (_, entry) = found.unwrap_or_else(|| panic!(
                    "`{setting}`: rung {lower:?} is missing from the report, so the \
                     ladder builder never offered it a slot"
                ));
                assert_eq!(
                    entry.status, RungStatus::Outranked,
                    "`{setting}`: rung {lower:?} below the winner must be Outranked"
                );
            }
        }
    }
}

/// Spec §7, named explicitly because this is the one behaviour change in the
/// release: today the device's bare `dither` beats a screen that sets its own.
#[test]
fn dither_screen_beats_device_base() {
    let res = resolve_render(&table_inputs(&[Rung::Screen, Rung::DeviceBase]));
    assert_eq!(res.dither.as_deref(), Some(DITHER_BY_RUNG[3]));
    assert_eq!(winner(&res.report, "dither"), Rung::Screen);
}

#[test]
fn dither_device_final_beats_screen() {
    let res = resolve_render(&table_inputs(&[Rung::DeviceFinal, Rung::Screen]));
    assert_eq!(res.dither.as_deref(), Some(DITHER_BY_RUNG[4]));
    assert_eq!(winner(&res.report, "dither"), Rung::DeviceFinal);
}

#[test]
fn dither_override_beats_everything() {
    let res = resolve_render(&table_inputs(&[
        Rung::Override, Rung::DeviceFinal, Rung::Screen, Rung::DeviceBase,
    ]));
    assert_eq!(res.dither.as_deref(), Some(OVERRIDE_DITHER));
    assert_eq!(winner(&res.report, "dither"), Rung::Override);
}

/// `colors_actual` loses its exception: piece 2 put the screen above the
/// override to match the legacy resolver. Spec §2 gives it the same order as
/// everything else, so a dev-UI calibration push now wins — which is the
/// whole point of the override rung.
#[test]
fn colors_actual_override_beats_the_screen() {
    let res = resolve_render(&table_inputs(&[Rung::Override, Rung::Screen]));
    assert_eq!(winner(&res.report, "colors_actual"), Rung::Override);
}

/// Spec §7: the test that would have caught the first draft's inverted
/// migration instruction. `refresh` is a ZERO behaviour change — a script's
/// `refresh_rate` still beats a bare `devices.<k>.refresh`.
#[test]
fn refresh_script_still_beats_the_device_base_rung() {
    let res = resolve_render(&table_inputs(&[Rung::Screen, Rung::DeviceBase]));
    assert_eq!(res.refresh, 300, "the script's 100*3, not the device's 100*2");
    assert_eq!(winner(&res.report, "refresh"), Rung::Screen);
}

/// …and the one new capability: an owner's veto on how often a
/// battery-powered display wakes up.
#[test]
fn refresh_final_beats_the_script() {
    let res = resolve_render(&table_inputs(&[Rung::DeviceFinal, Rung::Screen]));
    assert_eq!(res.refresh, 400);
    assert_eq!(winner(&res.report, "refresh"), Rung::DeviceFinal);
}

/// Field by field, not block by block (regression test for defect 5.3, at
/// the resolver level): the final rung setting one number must leave the
/// others to the rungs below it.
#[test]
fn a_rung_setting_one_field_leaves_the_others_to_lower_rungs() {
    let mut i = table_inputs(&[Rung::DeviceBase]);
    i.device_final = RungSettings {
        tuning: crate::models::DitherTuningValues {
            strength: Some(0.25),
            ..Default::default()
        },
        ..Default::default()
    };
    let res = resolve_render(&i);
    assert_eq!(res.tuning.strength, Some(0.25), "the final rung's own field");
    assert_eq!(
        res.tuning.max_error, Some(2.0),
        "max_error must still come from the device base rung"
    );
    assert_eq!(winner(&res.report, "strength"), Rung::DeviceFinal);
    assert_eq!(winner(&res.report, "max_error"), Rung::DeviceBase);
}

/// Ruling 2's mapping, which Tasks 3–6 depend on for their diagnostics.
#[test]
fn measured_source_names_the_rung_that_won() {
    let res = resolve_render(&table_inputs(&[Rung::Screen]));
    assert_eq!(res.measured_source, display::SRC_SCRIPT);
    let res = resolve_render(&table_inputs(&[Rung::DeviceFinal]));
    assert_eq!(res.measured_source, display::SRC_DEVICE);
    let res = resolve_render(&table_inputs(&[Rung::Panel]));
    assert_eq!(res.measured_source, display::SRC_PANEL_ACTUAL);
    let res = resolve_render(&table_inputs(&[Rung::Override]));
    assert_eq!(res.measured_source, display::SRC_DEV_OVERRIDE);
    let res = resolve_render(&table_inputs(&[]));
    assert_eq!(res.measured_source, display::SRC_NONE);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib api::resolution 2>&1 | tail -25`
Expected: FAIL — `cannot find struct `RenderInputs``, `cannot find function `resolve_render``, `no field `device_final``, `SRC_DEVICE` not found.

- [ ] **Step 3: Flip the two order constants**

In `src/api/resolution.rs`, replace `ORDER_COLORS_ACTUAL` and `ORDER_DITHER` with the spec's single order, and retitle the comment block above them.

```rust
// One order, for every setting. Spec §2: `override → device final → screen →
// device base → panel → fallback`. The constants stay per-setting rather than
// collapsing into one so that a future setting that genuinely needs a
// different walk has to say so here, in the one file that owns precedence,
// instead of growing its own chain in a handler.

/// Spec §2's order.
pub const ORDER_COLORS: &[Rung] = &[
    Rung::Override,
    Rung::DeviceFinal,
    Rung::Screen,
    Rung::DeviceBase,
    Rung::Panel,
    Rung::Fallback,
];
/// Spec §2's order. Piece 2 put `Screen` on top to match the legacy
/// `resolve_render_params`, which resolved the script's candidate before the
/// caller's override chain. The override rung exists so a dev-UI calibration
/// push can show what it does, so it goes back on top.
pub const ORDER_COLORS_ACTUAL: &[Rung] = ORDER_COLORS;
/// Spec §2's order. **This is the release's one behaviour change**: the
/// legacy `display::effective_dither` put `DeviceBase` above `Screen`, so a
/// bare `devices.<k>.dither` vetoed a screen that set its own. The veto now
/// lives in `final:`, where it can be told apart from a default.
pub const ORDER_DITHER: &[Rung] = ORDER_COLORS;
/// Spec §2's order.
pub const ORDER_TUNING: &[Rung] = ORDER_COLORS;
/// Spec §2's order. A panel has no `refresh`, so its rung is always absent;
/// it is listed so every setting reports the same six rungs.
pub const ORDER_REFRESH: &[Rung] = ORDER_COLORS;
```

- [ ] **Step 4: Add the final rung, the override-source label and the renames**

Still in `src/api/resolution.rs`. Add the rung→label mapping next to `Rung`:

```rust
impl Rung {
    /// The `SRC_*` label this rung contributes to
    /// `RenderParams::measured_source` (plan ruling 2).
    ///
    /// The labels are the ones byonk has always reported, kept because
    /// `measured_source` reaches the MCP `render_screen` tool's diagnostics
    /// and this release promises exactly one behaviour change. `Override`
    /// takes the caller's label: the override rung genuinely has a different
    /// origin per path, and naming that origin is what the diagnostic is for.
    pub fn measured_source(
        self,
        override_source: &'static str,
        fallback_present: bool,
    ) -> &'static str {
        match self {
            Rung::Override => override_source,
            Rung::DeviceFinal | Rung::DeviceBase => display::SRC_DEVICE,
            Rung::Screen => display::SRC_SCRIPT,
            Rung::Panel => display::SRC_PANEL_ACTUAL,
            Rung::Fallback if fallback_present => display::SRC_MEASURED_HEADER,
            Rung::Fallback => display::SRC_NONE,
        }
    }
}
```

Rename `DevRenderInputs` to `RenderInputs` and add the two fields:

```rust
/// Everything the cascade needs from one render, whichever path asked for it.
///
/// Renamed from `DevRenderInputs` in piece 3: four production paths build it
/// now, and a type called "dev" that `/api/display` constructs would be a lie
/// in the name.
pub struct RenderInputs<'a> {
    pub over: RungSettings,
    pub device_final: RungSettings,
    pub screen: RungSettings,
    pub device_base: RungSettings,
    /// Colours only; the panel's tuning is per algorithm and comes from
    /// `panel_dither` once the algorithm is known (spec §4.2).
    pub panel: RungSettings,
    pub panel_dither: Option<&'a PanelDitherConfig>,
    /// The model's default palette — this path's stand-in for the firmware
    /// `Colors` header.
    pub fallback_palette: Vec<(u8, u8, u8)>,
    /// The firmware `Measured-Colors` header, the fallback below the panel
    /// rung for `colors_actual` (spec §2: a hardware self-report is evidence,
    /// not policy, so it is a fallback and not a rung). Only `/api/display`
    /// ever has one; the other four paths pass `None`.
    pub fallback_colors_actual: Option<Vec<(u8, u8, u8)>>,
    /// The screen's `meta.yaml` `refresh`.
    pub screen_default_refresh: u32,
    /// What to call the override rung in `measured_source` — `SRC_DEV_OVERRIDE`
    /// on the device paths, `SRC_RENDER_OPTS` where a `RenderOpts` supplied it.
    pub override_source: &'static str,
}
```

Rename `DevRenderResolution` to `RenderResolution` and add `measured_source`:

```rust
pub struct RenderResolution {
    pub palette: Vec<(u8, u8, u8)>,
    pub measured_colors: Option<Vec<(u8, u8, u8)>>,
    /// Which rung supplied `measured_colors`, as a `SRC_*` label.
    pub measured_source: &'static str,
    /// Canonical name, or `None` for the engine default.
    pub dither: Option<String>,
    /// Configured winners only — never fallbacks (piece 2's ruling 7).
    pub tuning: DitherTuningValues,
    pub refresh: u32,
    pub report: Resolution,
}
```

Fill the new slot in `ladder_of` — the one-line change this whole flip rests on:

```rust
fn ladder_of<T: Clone>(
    i: &RenderInputs,
    pick: impl Fn(&RungSettings) -> Option<T>,
    panel: Option<T>,
    fallback: Option<T>,
) -> Ladder<T> {
    Ladder {
        over: pick(&i.over),
        device_final: pick(&i.device_final),
        screen: pick(&i.screen),
        device_base: pick(&i.device_base),
        panel,
        fallback,
    }
}
```

In `resolve_render`, the `colors_actual` `ladder_of` call currently passes `None` for its fallback. Pass the new field instead:

```rust
    let colors_actual = resolve(
        &ladder_of(
            i,
            |r| r.colors_actual.clone(),
            i.panel.colors_actual.clone(),
            i.fallback_colors_actual.clone(),
        ),
```

Rename `resolve_dev_render` to `resolve_render` and populate `measured_source` from the winner, beside the existing `measured_colors` line:

```rust
    DevRenderResolution {        // → RenderResolution
        palette,
        measured_colors: colors_actual.effective.clone(),
        measured_source: colors_actual
            .from
            .measured_source(i.override_source, colors_actual.rungs.iter().any(
                |(r, v, _)| *r == Rung::Fallback && v.is_some(),
            )),
        …
```

- [ ] **Step 5: Follow the renames through `dev.rs`**

`src/api/dev.rs:24` — the import becomes `header_value, resolve_render, RenderInputs, Resolution, Rung, RungSettings, RungStatus,`.

`src/api/dev.rs:778` — the construction gains the two new fields. The dev render has no device-final rung of its own yet (the POST body carries a draft and a device key; the device's `final:` block arrives with Task 3's reader, which this call then uses):

```rust
    let res = resolve_render(&RenderInputs {
        over: RungSettings::from_cascade(&req.over),
        device_final: RungSettings::from_cascade(&device_final_rung),
        screen: out.script,
        device_base: RungSettings::from_cascade(&device_rung),
        panel: RungSettings {
            colors: panel_colors.as_deref().map(display::parse_colors_header),
            colors_actual: panel_actual,
            ..Default::default()
        },
        panel_dither: panel_dither_config.as_ref(),
        fallback_palette: query_palette,
        fallback_colors_actual: None,
        screen_default_refresh: out.screen_default_refresh,
        override_source: display::SRC_DEV_OVERRIDE,
    });
```

`device_final_rung` is built next to the existing `device_rung`, from the same device config, with `dc.device_final()`. Find where `device_rung` is assigned (around `:640`) and add the sibling binding there; a draft that sets `final:` fields is piece 2's `req.over`, not this rung.

- [ ] **Step 6: Add `SRC_DEVICE`**

In `src/api/display.rs`, beside the other `SRC_*` consts at `:225-233`:

```rust
/// A device rung supplied the measurement — base or final. The two are one
/// label because the diagnostic answers "whose decision was this", and both
/// rungs belong to the device owner; the `X-Byonk-Resolution` report is where
/// the exact rung is named.
pub const SRC_DEVICE: &str = "device";
```

- [ ] **Step 7: Invert the piece-2 equivalence matrix's dither rows**

The equivalence test added in piece 2 pins `resolve_render` to the legacy resolvers. The flip makes its `dither` and `colors_actual` expectations wrong **by design** (ruling 6). Find it (`grep -rn "equivalen" src/api/resolution.rs src/api/dev.rs tests/`), invert exactly those expectations, and put the reason in the test:

```rust
    // Piece 3 flipped ORDER_DITHER and ORDER_COLORS_ACTUAL (spec §2), so the
    // ladder deliberately DISAGREES with the legacy resolvers on these two
    // settings. The rows below encode the new, intended answer; every other
    // row still pins the ladder to the legacy behaviour. This whole test is
    // retired in Task 8, when the legacy resolvers are deleted and the
    // five-paths convergence test takes over.
```

**Do not delete rows and do not shrink the matrix.** If a row cannot be made to express the new order, that is a finding about the resolver, not about the row.

- [ ] **Step 8: Run the tests**

Run: `cargo test --lib api::resolution 2>&1 | tail -15`
Expected: PASS.

Run: `cargo test --workspace 2>&1 | grep -E "^test result:|FAILED|panicked" | head -30`
Expected: failures **only** in tests that assert the legacy `dither` precedence — `display::tests::resolve_render_params_device_dither_beats_script_dither` and its neighbours at `src/api/display.rs:1821-1910`, plus any dev-path test that hard-codes the old order. Those legacy tests describe `resolve_render_params`, which still exists and is still correct about itself, so **leave them alone** — Task 8 deletes them with the function. Any failure outside that set is a real regression: stop and diagnose with `superpowers:systematic-debugging`.

Record the exact failing-test list in the commit message. The next task's reviewer needs to tell an expected red from a new one.

- [ ] **Step 9: Commit**

```bash
git add src/api/resolution.rs src/api/dev.rs src/api/display.rs
git diff --cached --stat
git commit -m "feat(resolution): the final rung, and spec order for dither and colors_actual

One order for every setting: override > device final > screen > device
base > panel > fallback. Two constants flip: dither (the device's bare
value no longer vetoes a screen) and colors_actual (the override rung goes
back above the screen). resolve_dev_render becomes resolve_render, since
four production paths are about to call it.

The dev path now resolves per spec while the other four still run their
legacy resolvers, so they disagree until the whole piece lands. Known
intentional failures at this commit: <paste the list from step 8>.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: `/api/display` resolves on the ladder

The path that reaches real hardware. It also loses §4.3's conflict warning, which is now a description of the cascade working.

**Files:**
- Modify: `src/api/resolution.rs` — add `RenderResolution::into_params()`
- Modify: `src/api/display.rs:1032-1085` (the resolution block inside the render closure), `:417-446` (the conflict warning)
- Test: `src/api/display.rs` (`mod tests`), plus `tests/render_cascade_convergence_test.rs` created in Task 8

**Interfaces:**
- Consumes: Task 2's `RenderInputs`, `resolve_render`, `RenderResolution`; Task 1's `DeviceConfig::base()` / `::device_final()`.
- Produces: `RenderResolution::into_params(self) -> RenderParams` — the one conversion from a cascade result to the type the renderer and the API response already speak. Tasks 4, 5 and 6 call it too, so no path converts by hand.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `src/api/display.rs`. These go through `resolve_render` + `into_params`, because that is what the handler will do:

```rust
/// Review Focus 3. A wrong-length measurement on the device base rung must
/// be reported `rejected` and the walk must continue — and the image must use
/// the lower rung's value, not fall back to nominal colours.
#[test]
fn display_rejects_a_wrong_length_device_colors_actual_and_uses_the_panel() {
    use crate::api::resolution::{resolve_render, RenderInputs, Rung, RungSettings, RungStatus};
    let res = resolve_render(&RenderInputs {
        over: RungSettings::default(),
        device_final: RungSettings::default(),
        screen: RungSettings::default(),
        device_base: RungSettings {
            // Three entries against a two-entry palette.
            colors_actual: Some(parse_colors_header("#111111,#222222,#333333")),
            ..Default::default()
        },
        panel: RungSettings {
            colors: Some(parse_colors_header("#000000,#FFFFFF")),
            colors_actual: Some(parse_colors_header("#0A0A0A,#F5F5F5")),
            ..Default::default()
        },
        panel_dither: None,
        fallback_palette: parse_colors_header("#000000,#FFFFFF"),
        screen_default_refresh: 900,
        override_source: SRC_DEV_OVERRIDE,
    });
    let report = &res.report["colors_actual"];
    let (_, base) = report.rungs.iter().find(|(r, _)| *r == Rung::DeviceBase)
        .expect("the device base rung must appear in the report");
    assert!(
        matches!(base.status, RungStatus::Rejected { .. }),
        "a wrong-length measurement must be Rejected, got {:?}", base.status
    );
    assert_eq!(report.from, Rung::Panel, "the walk must continue downwards");
    let params = res.into_params();
    assert_eq!(
        params.measured_colors, Some(parse_colors_header("#0A0A0A,#F5F5F5")),
        "the IMAGE must use the panel's measurement, not nominal colours"
    );
    assert_eq!(params.measured_source, SRC_PANEL_ACTUAL);
}

/// §4.3: a device/screen dither disagreement is the cascade working as
/// specified, so it is no longer a warning. With `final:` available the
/// operator has a way to say which they meant.
#[test]
fn a_dither_disagreement_produces_no_warning() {
    use crate::api::resolution::{resolve_render, RenderInputs, RungSettings};
    let res = resolve_render(&RenderInputs {
        over: RungSettings::default(),
        device_final: RungSettings::default(),
        screen: RungSettings { dither: Some("stucki".into()), ..Default::default() },
        device_base: RungSettings { dither: Some("atkinson".into()), ..Default::default() },
        panel: RungSettings::default(),
        panel_dither: None,
        fallback_palette: parse_colors_header("#000000,#FFFFFF"),
        screen_default_refresh: 900,
        override_source: SRC_DEV_OVERRIDE,
    });
    let params = res.into_params();
    assert_eq!(params.dither.as_deref(), Some("stucki"), "the screen wins now");
    assert!(
        params.warnings.is_empty(),
        "the cascade working as specified is not a warning: {:?}", params.warnings
    );
}
```

`RenderParams` has no `warnings` field today — the legacy resolver pushed into a caller-owned `Vec`, because four callers each wanted the lines somewhere different. **Step 3 adds `RenderParams::warnings: Vec<String>`**: with one producer, an out-param only lets a caller forget to read it, and `/api/display` already has a `render_warnings` vector it threads through by hand for exactly this.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib api::display 2>&1 | tail -20`
Expected: FAIL — `no method named `into_params``.

- [ ] **Step 3: Add `into_params`**

In `src/api/resolution.rs`:

```rust
impl RenderResolution {
    /// The shape the renderer and `/api/display`'s response already speak.
    ///
    /// One conversion for all five paths: a path that built its own
    /// `RenderParams` from a `RenderResolution` could drop a field, and the
    /// convergence test would then be comparing two different mistakes.
    pub fn into_params(self) -> display::RenderParams {
        display::RenderParams {
            palette: self.palette,
            measured_colors: self.measured_colors,
            measured_source: self.measured_source,
            dither: self.dither,
            max_error: self.tuning.max_error,
            noise_scale: self.tuning.noise_scale,
            chroma_clamp: self.tuning.chroma_clamp,
            strength: self.tuning.strength,
            gamut: self.tuning.gamut,
            // The measurement's own validity complaints. A disagreement
            // between rungs is NOT one of them (spec §4.3).
            warnings: self
                .report
                .get("colors_actual")
                .map(rejection_warnings)
                .unwrap_or_default(),
        }
    }
}

/// The `Rejected` reasons from one setting's report, as the warning lines the
/// device paths log and the authoring paths put in the script log.
fn rejection_warnings(report: &SettingReport) -> Vec<String> {
    report
        .rungs
        .iter()
        .filter_map(|(rung, entry)| match &entry.status {
            RungStatus::Rejected { reason } => {
                Some(format!("measured colours from the {rung:?} rung ignored: {reason}"))
            }
            _ => None,
        })
        .collect()
}
```

Add `pub warnings: Vec<String>` to `RenderParams` (`src/api/display.rs:80-95`) with a doc comment naming what does and does not belong in it.

- [ ] **Step 4: Move the handler onto the ladder**

Replace `src/api/display.rs:1032-1085` — from the `// Dev UI dither override beats everything` comment through the `for w in &render_warnings` loop — with the cascade construction. The surrounding bindings (`dev_dither`, `dev_tuning`, `dc_colors`, `dc_dither`, `dc_tuning_for_closure`, `panel_dither_for_closure`, `panel_colors_for_chain`, `header_colors_for_chain`, `fallback`, `result.script_*`) all already exist; what changes is that they are sorted into rungs instead of threaded through a nine-argument function.

```rust
                    // One ladder, spec §2. Every rung is a slot here; nothing
                    // below decides a precedence of its own.
                    let params = resolve_render(&RenderInputs {
                        over: RungSettings {
                            colors_actual: override_colors_parsed.clone(),
                            dither: dev_dither.as_deref().map(normalize_algorithm_name),
                            tuning: dev_tuning.clone().unwrap_or_default(),
                            ..Default::default()
                        },
                        device_final: RungSettings::from_cascade(&device_final_rung),
                        screen: RungSettings {
                            colors: result.script_colors.as_deref()
                                .map(|c| parse_colors_header(&c.join(","))),
                            // parse_measured_color_list, NOT a join into
                            // parse_colors_header: a script entry that itself
                            // contains a comma would fracture into extra
                            // "valid" colours under a join/re-split, inflate
                            // the parsed count and mask a genuine length
                            // mismatch. The parser's doc comment forbids it.
                            colors_actual: result.script_colors_actual.as_deref()
                                .map(parse_measured_color_list),
                            dither: result.script_dither.as_deref().map(normalize_algorithm_name),
                            tuning: script_tuning,
                            // `refresh` is resolved in content_pipeline, which
                            // is the only place the script's return value is in
                            // hand (plan ruling 3).
                            refresh: None,
                        },
                        device_base: RungSettings::from_cascade(&device_base_rung),
                        panel: RungSettings {
                            colors: panel_colors_for_chain.as_deref().map(parse_colors_header),
                            colors_actual: panel_actual_parsed.clone(),
                            ..Default::default()
                        },
                        panel_dither: panel_dither_for_closure.as_ref(),
                        // The firmware `Colors` / `Measured-Colors` headers are
                        // fallbacks, not rungs: a hardware self-report is
                        // evidence, not policy (spec §2).
                        fallback_palette: header_colors_for_chain
                            .as_deref()
                            .map(parse_colors_header)
                            .unwrap_or_else(|| fallback.clone()),
                        screen_default_refresh: 0,
                        override_source: SRC_DEV_OVERRIDE,
                    })
                    .into_params();
                    for w in &params.warnings {
                        tracing::warn!(device = %mac, "{w}");
                    }
```

Two bindings this needs that do not exist yet — add them beside the existing `dc_*` bindings, before the closure:

```rust
    let device_base_rung = device_config.map(|dc| dc.base()).unwrap_or_default();
    let device_final_rung = device_config.map(|dc| dc.device_final()).unwrap_or_default();
```

This is the one path with a `Measured-Colors` header, so it is the only one that fills `fallback_colors_actual` (declared in Task 2): pass `header_parsed`, the already-parsed header value from `:875`. That is also what makes `Rung::measured_source`'s `fallback_present` argument mean anything.

Then delete the now-unused `pre_script_measured_candidates` consumer in the final resolution (the **pre-script** winner at `:897-905` stays — it feeds `DeviceContext.colors_actual`, which the script reads before any of this runs).

- [ ] **Step 5: Delete the conflict warning**

Remove `src/api/display.rs:417-446`'s `if let (Some(device_algo), Some(script_algo))` block and the `warnings.push(…)` inside it. `resolve_render_params` keeps compiling until Task 8; its `warnings` out-param stays for the measurement complaints.

Also delete the test that asserts the warning fires — `resolve_render_params_warns_when_script_and_device_dither_disagree` (`:1863`) — and keep `resolve_render_params_does_not_warn_when_dither_names_are_aliases` (`:1890`) only if it still passes trivially; if it now asserts nothing, delete it and say so.

- [ ] **Step 6: Run the tests**

Run: `cargo test --lib api::display 2>&1 | tail -20` → the two new tests PASS.
Run: `cargo test --workspace 2>&1 | grep -E "^test result:|FAILED" | head -20`
Expected: the legacy-order failures from Task 2 still fail; `/api/display` integration tests that assert the old dither precedence now fail too — those are the behaviour change, so **update them to the new order** rather than deleting them, and name each one in the commit.

- [ ] **Step 7: Commit**

```bash
git add src/api/resolution.rs src/api/display.rs
git diff --cached --stat
git commit -m "feat(display): /api/display resolves on the cascade ladder

The production path builds a RenderInputs and converts the result with
into_params, so no setting is decided in the handler. The firmware Colors
and Measured-Colors headers move into the fallback slots they belong in
(spec §2: a hardware self-report is evidence, not policy).

The script/device dither conflict warning is deleted (spec §4.3): with
final: available, a disagreement is the cascade working as specified.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: `ScreenStore::render` resolves on the ladder

The authoring preview and the device preview share this path. Its override rung is `RenderOpts`.

**Files:**
- Modify: `src/services/screen_store.rs:1370-1440` (the resolution block), `DevicePreview` (`grep -n "pub struct DevicePreview" src/services/screen_store.rs`)
- Test: `src/services/screen_store.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 3's `into_params`, Task 2's `RenderInputs`.
- Produces: `DevicePreview::final_rung: CascadeValues` and `DevicePreview::colors_actual: Option<String>` — how a caller hands this path a device's two rungs. Task 6 fills them.

- [ ] **Step 1: Write the failing test**

`write_grey_screen` / `distinct_colors` in this file's tests make tuning visible in pixels: a flat mid-grey between two palette entries renders 1 colour at strength 0 and 2–3 with diffusion. Use that shape so the assertion is about the image, not only the struct.

```rust
/// The device's `final:` rung must reach the rendered pixels through this
/// path, not just through `/api/display`. A preview that ignores it shows the
/// operator an image their panel will never produce.
#[tokio::test]
async fn device_final_strength_reaches_the_rendered_image() {
    let (store, dir) = store_with_grey_screen().await;
    let mut device = DevicePreview::for_test("AA:BB:CC:DD:EE:FF");
    device.tuning.strength = Some(1.0);          // base rung: diffuse
    device.final_rung.strength = Some(0.0);      // final rung: veto it

    let opts = RenderOpts { device: Some(device), ..grey_opts(&dir) };
    let res = store.render("local/grey", &opts).await.expect("renders");

    assert_eq!(
        distinct_colors(&res.png), 1,
        "final.strength = 0 must win over base strength = 1.0, giving a flat block"
    );
}
```

Add `DevicePreview::for_test` and reuse the existing grey-screen helpers; if their names differ, `grep -n "fn write_grey_screen\|fn distinct_colors" src/services/screen_store.rs` and use what is there rather than adding a second set.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib screen_store 2>&1 | tail -15`
Expected: FAIL — `no field `final_rung` on type `DevicePreview``.

- [ ] **Step 3: Give `DevicePreview` both rungs**

```rust
    /// The device's base-rung measured colours, if it has its own.
    pub colors_actual: Option<String>,
    /// The device's final rung — the owner's veto. Empty when the device has
    /// no `final:` block, which resolves identically to having an empty one.
    pub final_rung: CascadeValues,
```

- [ ] **Step 4: Move the resolution onto the ladder**

Replace `src/services/screen_store.rs:1379-1440` — from the `// Same two slots as the resolve_render_params call below` comment through the `resolve_render_params(…)` call — with:

```rust
        let render_params = crate::api::resolution::resolve_render(&RenderInputs {
            // `RenderOpts` is this path's override rung: an agent or the
            // authoring UI asking "what would this look like".
            over: RungSettings {
                colors_actual: opts.colors_actual.as_deref()
                    .map(crate::api::display::parse_colors_header),
                dither: opts.dither.as_deref().map(crate::models::normalize_algorithm_name),
                tuning: opts_tuning,
                ..Default::default()
            },
            device_final: RungSettings::from_cascade(
                &opts.device.as_ref().map(|d| d.final_rung.clone()).unwrap_or_default(),
            ),
            screen: RungSettings {
                colors: script_result.script_colors.as_deref()
                    .map(|c| crate::api::display::parse_colors_header(&c.join(","))),
                // parse_measured_color_list, NOT a join (see its doc comment):
                // a script entry containing a comma would inflate the parsed
                // count and mask a length mismatch.
                colors_actual: script_result.script_colors_actual.as_deref()
                    .map(crate::api::display::parse_measured_color_list),
                dither: script_result.script_dither.as_deref()
                    .map(crate::models::normalize_algorithm_name),
                tuning: script_tuning,
                refresh: None,
            },
            device_base: RungSettings {
                colors: device_colors.as_deref().map(crate::api::display::parse_colors_header),
                colors_actual: opts.device.as_ref().and_then(|d| d.colors_actual.as_deref())
                    .map(crate::api::display::parse_colors_header),
                dither: device_dither.map(crate::models::normalize_algorithm_name),
                tuning: device_tuning,
                refresh: None,
            },
            panel: RungSettings {
                colors: panel_colors.as_deref().map(crate::api::display::parse_colors_header),
                colors_actual: panel_actual,
                ..Default::default()
            },
            panel_dither: panel_dither_config.as_ref(),
            fallback_palette: query_palette,
            fallback_colors_actual: None,
            screen_default_refresh: 0,
            override_source: crate::api::display::SRC_RENDER_OPTS,
        })
        .into_params();
```

The `effective_algo` / `panel_tuning` pre-pass above it goes away: `resolve_render` does the algorithm-then-numbers two-pass itself, which is the point of moving here. Keep `opts_tuning`, `script_tuning` and `device_tuning` — they are already built just above.

`panel_actual` may not exist in this scope under that name; `grep -n "colors_actual" src/services/screen_store.rs` around the block and use the binding that holds the panel's parsed measurement.

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib screen_store 2>&1 | grep -E "^test result:|FAILED"`
Expected: the new test PASSES. `measured_source must name the script…` (`:3159`) must still pass — ruling 2 keeps the labels precisely so it can.

- [ ] **Step 6: Commit**

```bash
git add src/services/screen_store.rs
git diff --cached --stat
git commit -m "feat(screen-store): the authoring and device previews resolve on the ladder

RenderOpts is this path's override rung; DevicePreview gains the device's
final rung and its base colors_actual, so a preview shows what the panel
will actually produce. The hand-rolled algorithm-then-numbers pre-pass
goes away — resolve_render does it.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: the CLI render resolves on the ladder

**The CLI has no test harness.** Verify by a sandbox run, not by reading — this is a recorded trap from the parent branch.

**Files:**
- Modify: `src/main.rs:383-460` (the measured-colour chain, the algorithm pre-pass and the `resolve_render_params` call)

**Interfaces:**
- Consumes: Task 3's `into_params`.
- Produces: nothing new.

- [ ] **Step 1: Move the resolution onto the ladder**

Replace `src/main.rs:429-460` and the `resolve_measured_colors` call at `:383` with one `resolve_render`. The CLI's override rung is its own flags (`dither`, `cli_max_error`, `cli_strength`); its device rungs come from the device config the `--device` flag selected, through `base()` and `device_final()`.

```rust
        let render_params = byonk::api::resolution::resolve_render(&RenderInputs {
            over: RungSettings {
                dither: dither.as_deref().map(byonk::models::normalize_algorithm_name),
                tuning: DitherTuningValues {
                    max_error: cli_max_error,
                    strength: cli_strength,
                    ..Default::default()
                },
                ..Default::default()
            },
            device_final: RungSettings::from_cascade(&device_final_rung),
            screen: RungSettings {
                colors: script_result.script_colors.as_deref()
                    .map(|c| byonk::api::display::parse_colors_header(&c.join(","))),
                // parse_measured_color_list, NOT a join (see its doc comment):
                // a script entry containing a comma would inflate the parsed
                // count and mask a length mismatch.
                colors_actual: script_result.script_colors_actual.as_deref()
                    .map(byonk::api::display::parse_measured_color_list),
                dither: script_result.script_dither.as_deref()
                    .map(byonk::models::normalize_algorithm_name),
                tuning: script_tuning,
                refresh: None,
            },
            device_base: RungSettings::from_cascade(&device_base_rung),
            panel: RungSettings {
                colors: panel_colors.as_deref()
                    .map(byonk::api::display::parse_colors_header),
                colors_actual: panel_actual,
                ..Default::default()
            },
            panel_dither: panel_dither_config.as_ref(),
            fallback_palette: fallback,
            fallback_colors_actual: None,
            screen_default_refresh: 0,
            override_source: byonk::api::display::SRC_RENDER_OPTS,
        })
        .into_params();
```

`device_base_rung` / `device_final_rung` come from the selected device config (`dc.base()` / `dc.device_final()`, `CascadeValues::default()` when `--device` was not given). `script_tuning`, `panel_colors`, `panel_actual` and `panel_dither_config` all exist in this scope already at `:383-460` — read them there and use those bindings rather than introducing new ones; if one is named differently, use the name that is in the file.

- [ ] **Step 2: Build and verify by a sandbox run**

```bash
cargo build 2>&1 | tail -5
```

Then render twice through the real binary, once with the device's base `dither` and once with it moved to `final:`, against a screen whose `script.lua` returns its own `dither`. Set up a scratch screen repo under the scratchpad directory:

```bash
SB="$(mktemp -d)"            # scratch, outside the repo
mkdir -p "$SB/screens/cascade"
cat > "$SB/screens/byonk-screens.yaml" <<'YAML'
name: cascade-sandbox
screens:
  cascade: { title: Cascade }
YAML
cat > "$SB/screens/cascade/meta.yaml" <<'YAML'
title: Cascade
refresh: 900
YAML
cat > "$SB/screens/cascade/script.lua" <<'LUA'
return { dither = "stucki" }
LUA
cat > "$SB/screens/cascade/screen.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
  <rect width="800" height="480" fill="#808080"/>
</svg>
SVG
cat > "$SB/config.yaml" <<'YAML'
devices:
  AA:BB:CC:DD:EE:FF:
    screen: local/cascade
    dither: atkinson
YAML
CONFIG_FILE="$SB/config.yaml" SCREENS_DIR="$SB/screens" \
  ./target/debug/byonk render local/cascade --device AA:BB:CC:DD:EE:FF -o "$SB/base.png" 2>&1 | tail -5
```

Expected in the log: the resolved algorithm is **stucki** — the screen beats the device's bare `dither`. Then move the device's value into `final:`:

```bash
cat > "$SB/config.yaml" <<'YAML'
devices:
  AA:BB:CC:DD:EE:FF:
    screen: local/cascade
    final:
      dither: atkinson
YAML
CONFIG_FILE="$SB/config.yaml" SCREENS_DIR="$SB/screens" \
  ./target/debug/byonk render local/cascade --device AA:BB:CC:DD:EE:FF -o "$SB/final.png" 2>&1 | tail -5
```

Expected: **atkinson** — the veto wins. If the two renders produce byte-identical PNGs, the algorithm is not reaching the renderer; that is the bug, not a flat fixture (the grey rect is between two palette entries on purpose).

Paste both resolved-algorithm log lines into the commit message. A reviewer cannot re-run this cheaply and has no test to read instead.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git diff --cached --stat
git commit -m "feat(cli): the CLI render resolves on the cascade ladder

Verified by sandbox run (the CLI path has no test harness):
  devices.<k>.dither: atkinson + script dither stucki -> <paste>
  devices.<k>.final.dither: atkinson + script stucki  -> <paste>

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: the admin device preview, and the cache that must notice

§4.1 says this path matters twice. The handler already delegates to `ScreenStore::render`, so Task 4 did the resolution — what is missing is that `DevicePreview` is built field by field here and `fingerprint()` hashes those fields. Miss either and the admin preview is wrong; miss the second and it is wrong *silently, hours later*.

**Files:**
- Modify: `src/api/admin/preview.rs:126-148` (the `DevicePreview` construction), `:244-277` (`fingerprint`)
- Test: `src/api/admin/preview.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 4's `DevicePreview::final_rung` / `::colors_actual`; Task 1's `DeviceConfig::device_final()`.
- Produces: nothing new.

- [ ] **Step 1: Write the failing tests**

```rust
/// Review Focus 5. `fingerprint` is the preview cache's only idea of "has
/// anything changed". A `final:` edit it cannot see serves a stale image
/// until the TTL — the failure this function's own doc comment says "only
/// shows up on the glass hours later".
#[test]
fn fingerprint_changes_when_only_the_final_rung_changes() {
    let base: DeviceConfig = serde_yaml::from_str("screen: local/hello").unwrap();
    let vetoed: DeviceConfig = serde_yaml::from_str(
        "screen: local/hello\nfinal:\n  dither: atkinson\n",
    ).unwrap();
    let opts = RenderOpts::default();
    assert_ne!(
        fingerprint(&base, "og", &opts),
        fingerprint(&vetoed, "og", &opts),
        "a final: change must invalidate the cached preview"
    );
}

#[test]
fn fingerprint_changes_when_only_device_colors_actual_changes() {
    let a: DeviceConfig = serde_yaml::from_str("screen: local/hello").unwrap();
    let b: DeviceConfig = serde_yaml::from_str(
        "screen: local/hello\ncolors_actual: \"#1C5240,#FFFFFF\"\n",
    ).unwrap();
    let opts = RenderOpts::default();
    assert_ne!(fingerprint(&a, "og", &opts), fingerprint(&b, "og", &opts));
}

/// Two different `final:` blocks must not collide, or the cache serves one
/// device's veto to a device that set a different one.
#[test]
fn fingerprint_distinguishes_two_different_final_rungs() {
    let a: DeviceConfig = serde_yaml::from_str(
        "screen: local/hello\nfinal:\n  dither: atkinson\n").unwrap();
    let b: DeviceConfig = serde_yaml::from_str(
        "screen: local/hello\nfinal:\n  dither: stucki\n").unwrap();
    let opts = RenderOpts::default();
    assert_ne!(fingerprint(&a, "og", &opts), fingerprint(&b, "og", &opts));
}

/// And the preview must render what `/api/display` would: the two rungs have
/// to be *passed*, not only hashed.
#[test]
fn the_preview_passes_both_device_rungs_to_the_renderer() {
    let dc: DeviceConfig = serde_yaml::from_str(
        "screen: local/hello\ncolors_actual: \"#1C5240,#FFFFFF\"\nfinal:\n  strength: 0.0\n",
    ).unwrap();
    let preview = device_preview_for(&dc, "AA:BB:CC:DD:EE:FF", None);
    assert_eq!(preview.colors_actual.as_deref(), Some("#1C5240,#FFFFFF"));
    assert_eq!(preview.final_rung.strength, Some(0.0));
}
```

The last test needs the `DevicePreview` construction extracted from the handler into a named function — do that in Step 3. A field-by-field build inlined in a 60-line handler is exactly how `gamut` went missing from the write surface for a release.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib admin::preview 2>&1 | tail -15`
Expected: FAIL — the three `assert_ne!` fingerprints are equal; `device_preview_for` not found.

- [ ] **Step 3: Extract the builder and fill both rungs**

In `src/api/admin/preview.rs`, lift the `DevicePreview { … }` literal out of the `RenderOpts` construction:

```rust
/// Build the preview's device identity and its two cascade rungs.
///
/// Named rather than inlined so a field added to `DevicePreview` has one
/// place to be added and one place to be tested. The admin preview must
/// render the same image `/api/display` would for the same device; a rung
/// missing here is a preview that quietly disagrees with the glass.
fn device_preview_for(
    device_config: &DeviceConfig,
    key: &str,
    seen: Option<&SeenDevice>,
) -> DevicePreview {
    DevicePreview {
        mac: key.to_string(),
        firmware_version: seen.map(|d| d.firmware_version.clone()),
        battery_voltage: seen.and_then(|d| d.battery_voltage),
        rssi: seen.and_then(|d| d.rssi),
        colors: device_config.colors.clone(),
        colors_actual: device_config.colors_actual.clone(),
        dither: device_config.dither.clone(),
        tuning: DitherTuningValues {
            deprecated_error_clamp: None,
            max_error: device_config.max_error,
            noise_scale: device_config.noise_scale,
            chroma_clamp: device_config.chroma_clamp,
            strength: device_config.strength,
            gamut: device_config.gamut.clone(),
        },
        refresh: device_config.refresh,
        final_rung: device_config.device_final(),
    }
}
```

Use `SeenDevice`'s real type name — `grep -n "seen" src/api/admin/preview.rs | head` and match it. Then in the handler: `device: Some(device_preview_for(device_config, &key, seen.as_ref()))`.

- [ ] **Step 4: Hash both rungs in `fingerprint`**

Append to `fingerprint`, before `hex::encode`:

```rust
    hasher.update([0]);
    hasher.update(format!("{:?}", device_config.colors_actual).as_bytes());
    hasher.update([0]);
    // The whole final rung, not a field list: a field added to CascadeValues
    // is then covered automatically. `CascadeValues` derives Debug, and the
    // derived form distinguishes every field, which is all the cache needs —
    // it is a change detector, not a stable identifier.
    hasher.update(format!("{:?}", device_config.final_rung).as_bytes());
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib admin::preview 2>&1 | grep -E "^test result:|FAILED"`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/api/admin/preview.rs
git diff --cached --stat
git commit -m "feat(admin): the device preview carries both cascade rungs, cache included

DevicePreview gains the device's final rung and colors_actual, and
fingerprint() hashes them — without the second half a final: edit serves a
stale preview until the TTL, silently. The DevicePreview build is extracted
into device_preview_for so a new field has one place to be added and tested.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: `refresh` on the shared order

Read plan ruling 3 and spec §2's "`refresh` is in, at `base`" before starting. **This must be a zero behaviour change for every existing config**, and the test that proves it is the one that would have caught the spec's first draft inverting the migration.

**Files:**
- Modify: `src/services/content_pipeline.rs:27-49` (`resolve_refresh_rate`), `DeviceContext` (`:99-130`), the call site (`:358-375`)
- Modify: `src/api/display.rs` — where `DeviceContext` is built, to pass the final rung's refresh
- Test: `src/services/content_pipeline.rs` (`mod tests` at `:818`)

**Interfaces:**
- Consumes: Task 2's `resolve()` + `ORDER_REFRESH`; Task 1's `DeviceConfig::device_final()`.
- Produces: `DeviceContext::refresh_final: Option<u32>`; `resolve_refresh_rate(lua_refresh: u32, device_base: Option<u32>, device_final: Option<u32>, screen_default: u32) -> ResolvedRefresh` — same return type, one more argument.

- [ ] **Step 1: Write the failing tests**

```rust
/// Spec §2: bringing `refresh` into the cascade at the base rung is a ZERO
/// behaviour change. The script still wins, and the displaced device value is
/// still reported so the operator can be told it is inert.
#[test]
fn refresh_script_still_beats_the_device_base_rung() {
    let r = resolve_refresh_rate(120, Some(600), None, 900);
    assert_eq!(r.rate, 120);
    assert_eq!(r.ignored_device_override, Some(600));
}

/// The one new capability: an owner's veto on how often a battery-powered
/// display wakes up.
#[test]
fn final_refresh_beats_the_script() {
    let r = resolve_refresh_rate(120, Some(600), Some(1800), 900);
    assert_eq!(r.rate, 1800);
    assert_eq!(
        r.ignored_device_override, None,
        "nothing outranks the final rung, so nothing it displaces is `ignored`"
    );
}

/// `0` still means "unset" at every layer, including the new one — the config
/// writer emits no `refresh: 0`, but a hand-edited file can carry one.
#[test]
fn a_zero_final_refresh_is_unset_not_a_veto_of_zero() {
    let r = resolve_refresh_rate(0, Some(600), Some(0), 900);
    assert_eq!(r.rate, 600, "the base rung must still apply");
}

#[test]
fn an_empty_ladder_falls_back_to_the_screen_default() {
    assert_eq!(resolve_refresh_rate(0, None, None, 900).rate, 900);
}

/// Plan ruling 4: the log line stays until the ladder widget ships, and now
/// reports a displaced BASE value only.
#[test]
fn only_a_displaced_base_value_is_reported_as_ignored() {
    assert_eq!(resolve_refresh_rate(120, None, None, 900).ignored_device_override, None);
    assert_eq!(resolve_refresh_rate(120, Some(0), None, 900).ignored_device_override, None);
}
```

Every existing test in this module calls `resolve_refresh_rate` with three arguments and must gain a `None` in the new third position — that mechanical edit is part of this step, and each one keeps asserting exactly what it asserted before. **If an existing expectation has to change, stop:** that is a behaviour change `refresh` was not supposed to have.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib content_pipeline 2>&1 | tail -15`
Expected: FAIL — `this function takes 4 arguments but 3 were supplied` on the old call shape, and the two new rung tests unimplemented.

- [ ] **Step 3: Reimplement on the shared order**

```rust
/// Resolve the effective refresh rate on the cascade ladder (spec §2).
///
/// Walked by `api::resolution::resolve` with the shared `ORDER_REFRESH`, so
/// `refresh` cannot drift out of step with the other eight settings — which
/// is the whole reason the cascade exists. It stays here rather than in
/// `resolve_render` because the screen's rung is the script's `refresh_rate`
/// return value, which only exists on this side of the script run (plan
/// ruling 3).
///
/// `0` means "unset" at every layer.
pub(crate) fn resolve_refresh_rate(
    lua_refresh: u32,
    device_base: Option<u32>,
    device_final: Option<u32>,
    screen_default: u32,
) -> ResolvedRefresh {
    use crate::api::resolution::{resolve, accept_any, Ladder, Rung, ORDER_REFRESH};
    let nonzero = |v: Option<u32>| v.filter(|&r| r > 0);
    let ladder = Ladder {
        over: None,
        device_final: nonzero(device_final),
        screen: nonzero(Some(lua_refresh)),
        device_base: nonzero(device_base),
        panel: None,
        fallback: Some(screen_default),
    };
    let resolved = resolve(&ladder, ORDER_REFRESH, accept_any);
    ResolvedRefresh {
        rate: resolved.effective.unwrap_or(screen_default),
        // Only a BASE value displaced by the screen is worth telling the
        // operator about: a displaced `final.refresh` is impossible, and a
        // base value displaced by `final` was overridden on purpose by the
        // same person. Stays until the dev UI's ladder widget shows the same
        // fact (plan ruling 4).
        ignored_device_override: (resolved.from == Rung::Screen)
            .then(|| nonzero(device_base))
            .flatten(),
    }
}
```

- [ ] **Step 4: Thread the final rung through `DeviceContext`**

Add beside `refresh_override`:

```rust
    /// The device's `final.refresh` — the owner's veto on wake-up cadence.
    /// Outranks the script, unlike [`DeviceContext::refresh_override`].
    pub refresh_final: Option<u32>,
```

Update the call site (`:361-363`):

```rust
        let device_base = device_ctx.and_then(|c| c.refresh_override);
        let device_final = device_ctx.and_then(|c| c.refresh_final);
        let refresh =
            resolve_refresh_rate(lua_result.refresh_rate, device_base, device_final, screen_default);
```

Then fill `refresh_final` at every `DeviceContext` construction: `grep -rn "refresh_override:" src/` and add `refresh_final:` beside each. On `/api/display` it is `device_config.and_then(|dc| dc.device_final().refresh)`; where no device config exists, `None`.

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib content_pipeline 2>&1 | grep -E "^test result:|FAILED"` → PASS.
Run: `cargo test --workspace 2>&1 | grep -cE "FAILED"` — compare against Task 3's recorded count. It must not have grown.

- [ ] **Step 6: Commit**

```bash
git add src/services/content_pipeline.rs src/api/display.rs
git diff --cached --stat
git commit -m "feat(refresh): resolve refresh on the shared cascade order

resolve_refresh_rate is reimplemented over Ladder + ORDER_REFRESH, so
refresh walks the same order as the other eight settings. Existing
devices.<k>.refresh values stay at the base rung, where the script still
beats them — a zero behaviour change, with a test that says so. New:
final.refresh, an owner's veto on wake-up cadence.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: the five paths agree — and the legacy resolvers go

Spec §7 calls this the highest-value test in the design. It is also the licence to delete the old code: until it passes, "all five paths resolve on the ladder" is a claim about five diffs rather than a fact about one fixture.

**Files:**
- Create: `tests/render_cascade_convergence_test.rs`
- Modify: `src/api/display.rs` — delete `resolve_render_params` (`:405`), `resolve_effective_tuning` (`:166`), `resolve_tuning` (`:100`), `effective_dither` (`:372`) and their tests (`:1799-2040`, `:2106`)
- Modify: wherever the piece-2 equivalence matrix lives — delete it (ruling 6)

**Interfaces:**
- Consumes: Tasks 3–7.
- Produces: nothing; this task only removes.

- [ ] **Step 1: Write the convergence test**

One fixture, five paths, identical `RenderParams`. The fixture must carry **more than one value per contested slot** — a fixture that cannot express the bug will never see it, and this bit the parent branch twice.

```rust
//! Spec §7: "The five call paths agree." One device, one screen, one panel,
//! resolved through `/api/display`, `POST /dev/api/render`,
//! `ScreenStore::render`, the CLI and `/api/admin/devices/{key}/preview`.
//!
//! The fixture deliberately contests every rung at once: the panel has a
//! per-algorithm dither block, the device sets a base rung AND a final rung,
//! and the script sets its own algorithm and tuning. A fixture that set one
//! value per setting would pass against four different resolvers.

const CONFIG: &str = r#"
panels:
  trmnl_x:
    name: TRMNL X
    colors: "#000000,#555555,#AAAAAA,#FFFFFF"
    colors_actual: "#0A0A0A,#4F4F4F,#A6A6A6,#F5F5F5"
    dither:
      defaults:
        max_error: 1.0
      algorithms:
        atkinson:
          noise_scale: 0.25
        stucki:
          noise_scale: 0.75
devices:
  AA:BB:CC:DD:EE:FF:
    screen: local/cascade
    panel: trmnl_x
    dither: sierra          # base rung — the screen outranks this now
    strength: 0.9           # base rung
    final:
      dither: atkinson      # the veto: beats the script's stucki
"#;

/// The script sets its own algorithm and one tuning number, so the screen
/// rung is contested too.
const SCRIPT: &str = r#"return { dither = "stucki", max_error = 2.0 }"#;

/// Review Focus 4, asserted by the fixture itself: `final.dither = atkinson`
/// must win over the script's `stucki`, AND the panel's per-algorithm block
/// must be looked up for `atkinson` — so `noise_scale` must be 0.25, not
/// 0.75 and not the flat default. Getting the veto right while looking its
/// tuning up for the loser is the one failure §4.2 exists to prevent.
fn expected() -> ExpectedParams {
    ExpectedParams {
        dither: Some("atkinson"),
        noise_scale: Some(0.25),
        max_error: Some(2.0),   // the script's, which no higher rung sets
        strength: Some(0.9),    // the device base rung's
        palette_hex: "#000000,#555555,#AAAAAA,#FFFFFF",
        measured_hex: Some("#0A0A0A,#4F4F4F,#A6A6A6,#F5F5F5"),
    }
}

#[tokio::test]
async fn all_five_render_paths_resolve_identically() {
    let fx = Fixture::new(CONFIG, SCRIPT).await;

    let display = fx.via_api_display().await;
    let dev = fx.via_dev_render().await;
    let store = fx.via_screen_store().await;
    let cli = fx.via_cli().await;
    let admin = fx.via_admin_preview().await;

    for (name, got) in [
        ("/api/display", &display),
        ("POST /dev/api/render", &dev),
        ("ScreenStore::render", &store),
        ("CLI render", &cli),
        ("/api/admin/devices/{key}/preview", &admin),
    ] {
        assert_eq!(got, &expected(), "{name} disagrees with the cascade");
    }
}
```

**The convergence test must be HANDLER-level, not resolver-level.** Each `via_*` drives the path's own entry point — the axum handler, `ScreenStore::render`, the CLI's render function — and never builds a `RenderInputs` by hand. A test that constructs `RenderInputs` itself re-tests the resolver (already covered in Task 2) and would still pass with two rungs swapped in a handler, which is precisely the wiring this piece rewires. If a path's resolution is unreachable without a refactor, extract its rung mapping into a named function and drive that.

`Fixture` and `ExpectedParams` are this file's own helpers. Each `via_*` must reach the real resolution for that path — for `/api/display` and the two HTTP previews, read the resolved values out of the `X-Byonk-Resolution` header where the path emits one and out of the handler's `RenderParams` otherwise; for the CLI, call the same function `main.rs` calls rather than shelling out. **Do not** compare PNG bytes: two paths can render the same pixels from different settings, and the point is the settings.

If one `via_*` cannot observe a path's `RenderParams` without a refactor, do the refactor — extracting the resolution into a named function is the same move Task 6 made, and byonk's APIs are ours to change.

- [ ] **Step 2: Run it to see it fail honestly**

Run: `cargo test --test render_cascade_convergence_test 2>&1 | tail -30`
Expected: it must fail **before** you touch anything else, naming which path disagrees. A convergence test that passes on its first run is testing nothing — check the fixture actually contests the rungs.

- [ ] **Step 3: Fix whichever path disagrees**

Use `superpowers:systematic-debugging`. The likely finding is a rung a Task 3–7 construction left `Default::default()`. **Do not weaken the fixture.**

- [ ] **Step 4: Delete the legacy resolvers**

Once the test is green, delete from `src/api/display.rs`: `resolve_render_params`, `resolve_effective_tuning`, `resolve_tuning`, `effective_dither`, and the test block at `:1799-2040` plus `:2106`. `resolve_measured_colors` and the `MeasuredCandidate` type stay **only** if the pre-script chain at `:880-905` still uses them — it feeds `DeviceContext.colors_actual`, which the script reads before any resolution happens, and that is not a cascade. If nothing else calls them, delete those too.

Delete the piece-2 equivalence matrix (ruling 6): with the legacy resolvers gone there is nothing to be equivalent to, and this test is strictly stronger.

- [ ] **Step 5: Full verification**

```bash
make webui
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | grep -E "^test result:|FAILED"
make webui-test 2>&1 | tail -5
(cd docs && mdbook build)
```

Expected: **zero failures.** Every intentional red recorded in Tasks 2–7 is resolved by this point — the tests that asserted the legacy order are gone with the function they described. If anything is still red here, it is a real defect; do not carry it into Task 9.

- [ ] **Step 6: Commit**

```bash
git add tests/render_cascade_convergence_test.rs src/api/display.rs
git diff --cached --stat
git commit -m "test(resolution): the five render paths agree, and the legacy resolvers go

One fixture contesting every rung at once, resolved through /api/display,
POST /dev/api/render, ScreenStore::render, the CLI and the admin device
preview, must produce identical RenderParams. With that green,
resolve_render_params, resolve_effective_tuning, resolve_tuning and
effective_dither are deleted, along with piece 2's equivalence matrix —
there is nothing left to be equivalent to.

Suite green: fmt, clippy, cargo test, webui-test, mdbook.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: the write surface — seven coupled sites

**Read spec §4.8 in full before touching anything.** This is the task whose most likely implementation fails *silently*: `MANAGED_DEVICE_KEYS` lists what `device_block` writes, and anything missing from it is carried across verbatim by `unmanaged_device_keys` and re-inserted **after** `device_block`. So a `final` left off that list means every PATCH that sets or clears `final` is overwritten by the `final` block already in the file, with no error. `gamut` is in exactly that state today, which is why §1 calls it unreachable.

**Files:**
- Modify: `src/api/admin/write.rs` — `DeviceWrite` (`:22-55`), `resolve_clear`'s also-set table (`:221-236`), `apply_clear` (`:252-273`), `MANAGED_DEVICE_KEYS` (`:280`), `CLEARABLE_DEVICE_KEYS` (`:303`), `device_block` (`:364+`), the merge struct (`:539-561`), the invariant test (`:968`)
- Test: `src/api/admin/write.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 1's `CascadeValues`.
- Produces:
  - `DeviceWrite::final_rung: Option<CascadeValues>` (`#[serde(rename = "final")]`), `::colors_actual: Option<String>`, `::gamut: Option<GamutTuningValues>`.
  - `MANAGED_DEVICE_KEYS: [&str; 17]`, `CLEARABLE_DEVICE_KEYS: [&str; 16]`.
  - `clear` accepts `"final"` and `"final.<field>"` for each `CascadeValues` field.

- [ ] **Step 1: Write the failing tests**

```rust
/// Review Focus 1 and spec §4.8's trap. The failure is silent: the response
/// is 200 and the config keeps the OLD final block.
#[test]
fn a_patch_setting_final_survives_unmanaged_device_keys() {
    let yaml = "devices:\n  AA:\n    screen: local/a\n    final:\n      dither: stucki\n";
    let kept = unmanaged_device_keys(yaml, "AA");
    assert!(
        !kept.contains_key(&serde_yaml::Value::from("final")),
        "`final` must be MANAGED, or every patch that sets it is silently \
         overwritten by the block already in the file"
    );
}

/// Ruling 5 / spec §4.8: field by field, like every other rung. Setting
/// `final.dither` must not wipe `final.strength`.
#[test]
fn setting_one_final_field_keeps_the_others() {
    let existing = CascadeValues {
        dither: Some("stucki".into()),
        strength: Some(0.4),
        ..Default::default()
    };
    let body = CascadeValues { dither: Some("atkinson".into()), ..Default::default() };
    let merged = merge_final(Some(&body), Some(&existing));
    let m = merged.expect("a final rung survives the merge");
    assert_eq!(m.dither.as_deref(), Some("atkinson"), "the patch's field wins");
    assert_eq!(m.strength, Some(0.4), "the untouched field must survive");
}

/// Review Focus 2 / ruling 5: a bare name clears the BASE rung only. The
/// symmetric error — clearing both — is indistinguishable from success in the
/// response, so only a test catches it.
#[test]
fn a_bare_clear_name_clears_the_base_rung_only() {
    let mut merged = DeviceWrite {
        dither: Some("sierra".into()),
        final_rung: Some(CascadeValues {
            dither: Some("atkinson".into()),
            ..Default::default()
        }),
        ..device_write_for_test()
    };
    apply_clear(&mut merged, &["dither".to_string()]);
    assert_eq!(merged.dither, None, "the base rung is cleared");
    assert_eq!(
        merged.final_rung.as_ref().and_then(|f| f.dither.as_deref()),
        Some("atkinson"),
        "`clear: [\"dither\"]` must never reach the final rung"
    );
}

#[test]
fn a_dotted_clear_name_clears_that_final_field_only() {
    let mut merged = DeviceWrite {
        dither: Some("sierra".into()),
        final_rung: Some(CascadeValues {
            dither: Some("atkinson".into()),
            strength: Some(0.4),
            ..Default::default()
        }),
        ..device_write_for_test()
    };
    apply_clear(&mut merged, &["final.dither".to_string()]);
    assert_eq!(merged.dither.as_deref(), Some("sierra"), "the base rung stands");
    let f = merged.final_rung.as_ref().expect("the rung itself survives");
    assert_eq!(f.dither, None);
    assert_eq!(f.strength, Some(0.4), "its other fields survive");
}

#[test]
fn clearing_final_removes_the_whole_rung() {
    let mut merged = DeviceWrite {
        final_rung: Some(CascadeValues {
            dither: Some("atkinson".into()),
            strength: Some(0.4),
            ..Default::default()
        }),
        ..device_write_for_test()
    };
    apply_clear(&mut merged, &["final".to_string()]);
    assert!(merged.final_rung.is_none());
}

#[test]
fn an_unknown_dotted_clear_name_is_rejected_by_name() {
    let err = resolve_clear(
        &DeviceWrite { ..device_write_for_test() },
        &Some(vec!["final.screen".to_string()]),
    )
    .expect_err("`final.screen` is not a cascade field");
    assert!(format!("{err:?}").contains("final.screen"));
}

/// Setting and clearing the same thing in one request is a caller mistake at
/// every rung, not just the base one.
#[test]
fn a_final_field_cannot_be_set_and_cleared_in_one_request() {
    let body = DeviceWrite {
        final_rung: Some(CascadeValues { dither: Some("atkinson".into()), ..Default::default() }),
        ..device_write_for_test()
    };
    resolve_clear(&body, &Some(vec!["final.dither".to_string()]))
        .expect_err("set and cleared in the same request");
}

#[test]
fn device_block_emits_the_final_mapping_and_the_two_new_base_keys() {
    let w = DeviceWrite {
        colors_actual: Some("#1C5240,#FFFFFF".into()),
        gamut: Some(GamutTuningValues { knee: Some(0.8), ..Default::default() }),
        final_rung: Some(CascadeValues { dither: Some("atkinson".into()), ..Default::default() }),
        ..device_write_for_test()
    };
    let block = device_block(&w, "local/a");
    assert_eq!(
        block.get(&serde_yaml::Value::from("colors_actual")).and_then(|v| v.as_str()),
        Some("#1C5240,#FFFFFF")
    );
    let gamut = block.get(&serde_yaml::Value::from("gamut")).expect("gamut is emitted");
    assert!(gamut.as_mapping().is_some(), "gamut is a nested mapping");
    let f = block.get(&serde_yaml::Value::from("final")).expect("final is emitted");
    assert_eq!(
        f.as_mapping().and_then(|m| m.get(&serde_yaml::Value::from("dither")))
            .and_then(|v| v.as_str()),
        Some("atkinson")
    );
    // A final rung that sets nothing must not leave `final: {}` behind.
    let empty = DeviceWrite { final_rung: Some(CascadeValues::default()), ..device_write_for_test() };
    assert!(device_block(&empty, "local/a").get(&serde_yaml::Value::from("final")).is_none());
}

/// The round trip, end to end: a f32 in the final rung must come back out of
/// config.yaml as the decimal the caller sent, like a base-rung one.
#[test]
fn a_final_rung_f32_is_written_as_the_decimal_the_caller_sent() {
    let w = DeviceWrite {
        final_rung: Some(CascadeValues { strength: Some(0.9), ..Default::default() }),
        ..device_write_for_test()
    };
    let yaml = serde_yaml::to_string(&device_block(&w, "local/a")).unwrap();
    assert!(
        yaml.contains("strength: 0.9"),
        "`yaml_f32` must apply inside the final rung too, got:\n{yaml}"
    );
}
```

`device_write_for_test()` is a helper returning a `DeviceWrite` with `screen: Some("local/a")` and everything else `None`; add it if the module has no equivalent. `merge_final` is the named merge introduced in Step 3.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib admin::write 2>&1 | tail -20`
Expected: FAIL — `no field `final_rung``, `cannot find function `merge_final``, and `a_patch_setting_final_survives_unmanaged_device_keys` failing because `final` is unmanaged.

- [ ] **Step 3: Change all seven sites, in this order**

**(1) `DeviceWrite`** — add after `colors`:

```rust
    /// Measured colours for this one display, comma-separated hex. The base
    /// rung: it outranks the panel's calibration and is outranked by a screen
    /// that measures its own.
    pub colors_actual: Option<String>,
    /// Gamut mapping for continuous-tone regions. Base rung.
    ///
    /// Reachable only since piece 3: it was in `DeviceConfig` but in neither
    /// `DeviceWrite` nor `MANAGED_DEVICE_KEYS`, so it could be hand-edited
    /// into config.yaml and nothing could set it. That is the §4.8 trap, and
    /// this field is the proof it bites.
    pub gamut: Option<GamutTuningValues>,
    /// The final rung — the owner's veto. Merged **field by field**, like
    /// every other rung: setting `final.dither` must not wipe
    /// `final.strength`. A field is removed only through `clear`, which is
    /// why both `final.<field>` and `final` are clearable.
    #[serde(rename = "final")]
    pub final_rung: Option<CascadeValues>,
```

**(2) + (3) the two key lists** — add the same three names to both. The invariant test at `:968` asserts `CLEARABLE == MANAGED - screen`, so adding to one without the other fails there; that test is the reason this coupling is survivable, so extend the counts (`[&str; 17]` / `[&str; 16]`) rather than reaching for `Vec`.

**(4) `device_block`** — emit the new keys. The final rung is a nested mapping built by one shared function, so base and final cannot format a float differently:

```rust
/// One rung's settings as a YAML mapping. Shared by the base rung's inline
/// emission and the `final:` block so `yaml_f32` applies to both — a float
/// written one way at one rung and another way at the other churns the diff
/// on every rewrite.
fn cascade_mapping(v: &CascadeValues) -> serde_yaml::Mapping {
    let mut m = serde_yaml::Mapping::new();
    if let Some(c) = &v.colors {
        m.insert("colors".into(), c.as_str().into());
    }
    if let Some(c) = &v.colors_actual {
        m.insert("colors_actual".into(), c.as_str().into());
    }
    if let Some(d) = &v.dither {
        m.insert("dither".into(), d.as_str().into());
    }
    for (key, value) in [
        ("max_error", v.max_error),
        ("noise_scale", v.noise_scale),
        ("chroma_clamp", v.chroma_clamp),
        ("strength", v.strength),
    ] {
        if let Some(n) = value {
            m.insert(key.into(), yaml_f32(n));
        }
    }
    if let Some(r) = v.refresh.filter(|&r| r > 0) {
        m.insert("refresh".into(), serde_yaml::Value::from(r));
    }
    if let Some(g) = gamut_mapping(&v.gamut) {
        m.insert("gamut".into(), g.into());
    }
    m
}

/// A `GamutTuningValues` as a mapping, or `None` when it sets nothing — an
/// empty `gamut: {}` in the file would be noise.
fn gamut_mapping(g: &GamutTuningValues) -> Option<serde_yaml::Mapping> {
    if g.is_empty() {
        return None;
    }
    let mut m = serde_yaml::Mapping::new();
    for (key, value) in [
        ("knee", g.knee),
        ("amount", g.amount),
        ("max_compression", g.max_compression),
    ] {
        if let Some(n) = value {
            m.insert(key.into(), yaml_f32(n));
        }
    }
    Some(m)
}
```

In `device_block`, after the existing tuning loop:

```rust
    if let Some(c) = &w.colors_actual {
        m.insert("colors_actual".into(), c.as_str().into());
    }
    if let Some(g) = w.gamut.as_ref().and_then(gamut_mapping) {
        m.insert("gamut".into(), g.into());
    }
    // An empty final rung emits nothing: `final: {}` in the file would read
    // as a veto that vetoes nothing.
    if let Some(f) = &w.final_rung {
        let mapping = cascade_mapping(f);
        if !mapping.is_empty() {
            m.insert("final".into(), mapping.into());
        }
    }
```

**(5) the merge struct** — ruling 5's field-by-field merge, as a named function so it is testable without a request:

```rust
/// Merge the patch's final rung onto the existing one, field by field.
///
/// `body.or(existing)` on the whole `Option` would be whole-block
/// replacement, which §5.3 condemns for tuning and which is wrong here for
/// the same reason: a patch setting `final.dither` would silently drop
/// `final.strength`. Returns `None` when neither side has a rung, so
/// `device_block` emits no `final:` key at all.
fn merge_final(
    body: Option<&CascadeValues>,
    existing: Option<&CascadeValues>,
) -> Option<CascadeValues> {
    match (body, existing) {
        (None, None) => None,
        (Some(b), None) => Some(b.clone()),
        (None, Some(e)) => Some(e.clone()),
        (Some(b), Some(e)) => Some(CascadeValues {
            colors: b.colors.clone().or_else(|| e.colors.clone()),
            colors_actual: b.colors_actual.clone().or_else(|| e.colors_actual.clone()),
            dither: b.dither.clone().or_else(|| e.dither.clone()),
            max_error: b.max_error.or(e.max_error),
            noise_scale: b.noise_scale.or(e.noise_scale),
            chroma_clamp: b.chroma_clamp.or(e.chroma_clamp),
            strength: b.strength.or(e.strength),
            refresh: b.refresh.or(e.refresh),
            gamut: b.gamut.or(&e.gamut),
        }),
    }
}
```

In the merge struct, add:

```rust
        colors_actual: body.colors_actual.clone().or(existing.colors_actual.clone()),
        gamut: Some(
            body.gamut.clone().unwrap_or_default().or(&existing.gamut),
        ),
        final_rung: merge_final(body.final_rung.as_ref(), existing.final_rung.as_ref()),
```

**(6) `resolve_clear`'s also-set table** — the dotted names need their own arm, because "is this field also being set" now has to look inside the final rung:

```rust
        let also_set = match name.as_str() {
            // …the existing base-rung arms, plus…
            "colors_actual" => w.colors_actual.is_some(),
            "gamut" => w.gamut.is_some(),
            "final" => w.final_rung.is_some(),
            dotted if dotted.starts_with("final.") => {
                w.final_rung.as_ref().is_some_and(|f| cascade_field_is_set(f, &dotted[6..]))
            }
            _ => false,
        };
```

and the acceptance check above it must admit the dotted form:

```rust
        let acceptable = CLEARABLE_DEVICE_KEYS.contains(&name.as_str())
            || name.strip_prefix("final.").is_some_and(is_cascade_field);
        if !acceptable {
            // …the existing error, with the dotted form named in the hint…
        }
```

with two small helpers beside them:

```rust
/// The field names a `final.<name>` clear may address — `CascadeValues`'
/// own fields, and nothing else. `final: {screen: …}` is a config error at
/// startup (spec §3); `clear: ["final.screen"]` must be an API error for the
/// same reason, named rather than ignored.
const CASCADE_FIELDS: [&str; 9] = [
    "colors", "colors_actual", "dither", "max_error", "noise_scale",
    "chroma_clamp", "strength", "refresh", "gamut",
];

fn is_cascade_field(name: &str) -> bool {
    CASCADE_FIELDS.contains(&name)
}

fn cascade_field_is_set(v: &CascadeValues, field: &str) -> bool {
    match field {
        "colors" => v.colors.is_some(),
        "colors_actual" => v.colors_actual.is_some(),
        "dither" => v.dither.is_some(),
        "max_error" => v.max_error.is_some(),
        "noise_scale" => v.noise_scale.is_some(),
        "chroma_clamp" => v.chroma_clamp.is_some(),
        "strength" => v.strength.is_some(),
        "refresh" => v.refresh.is_some(),
        "gamut" => !v.gamut.is_empty(),
        _ => false,
    }
}
```

**(7) `apply_clear`** — the three new base arms and the two final forms:

```rust
            "colors_actual" => merged.colors_actual = None,
            "gamut" => merged.gamut = None,
            // The whole rung, as one key.
            "final" => merged.final_rung = None,
            dotted if dotted.starts_with("final.") => {
                if let Some(f) = merged.final_rung.as_mut() {
                    clear_cascade_field(f, &dotted[6..]);
                }
                // A rung whose every field was cleared emits nothing anyway
                // (`device_block`), so there is no empty-rung case to handle.
            }
```

with the setter twin of `cascade_field_is_set`:

```rust
fn clear_cascade_field(v: &mut CascadeValues, field: &str) {
    match field {
        "colors" => v.colors = None,
        "colors_actual" => v.colors_actual = None,
        "dither" => v.dither = None,
        "max_error" => v.max_error = None,
        "noise_scale" => v.noise_scale = None,
        "chroma_clamp" => v.chroma_clamp = None,
        "strength" => v.strength = None,
        "refresh" => v.refresh = None,
        "gamut" => v.gamut = GamutTuningValues::default(),
        // Unreachable: `resolve_clear` rejects anything else.
        _ => {}
    }
}
```

**(8) the invariant test** — extend it so the dotted form is covered too:

```rust
/// Every `CascadeValues` field must be addressable as `final.<field>`, or a
/// final-rung setting becomes a one-way door — the same contract
/// `every_managed_key_except_screen_is_clearable` enforces for the base rung.
#[test]
fn every_cascade_field_is_clearable_in_the_final_rung() {
    for field in CASCADE_FIELDS {
        let name = format!("final.{field}");
        resolve_clear(&device_write_for_test(), &Some(vec![name.clone()]))
            .unwrap_or_else(|e| panic!("`{name}` must be clearable: {e:?}"));
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib admin::write 2>&1 | grep -E "^test result:|FAILED"`
Expected: PASS, including the two pre-existing invariant tests.

- [ ] **Step 5: Verify the trap end to end through the real handler**

The unit tests cover the pieces; §4.8's failure is in their composition. Run an actual PATCH against a config that already has a `final:` block and read the file back:

Run: `cargo test --workspace 2>&1 | grep -E "^test result:|FAILED"` and, if no integration test covers `PATCH /api/admin/devices/{key}`, add one in this task that (a) PATCHes `final.dither` onto a device whose file already has `final: {strength: 0.4}`, (b) re-reads the written `config.yaml`, (c) asserts **both** fields are present. That is the only assertion shape that catches `unmanaged_device_keys` putting the old block back.

- [ ] **Step 6: Commit**

```bash
git add src/api/admin/write.rs
git diff --cached --stat
git commit -m "feat(admin): final:, colors_actual and gamut are writable

Spec §4.8's seven coupled sites. `final` joins MANAGED_DEVICE_KEYS, without
which every PATCH setting it is silently overwritten by the block already in
the file — the state `gamut` has been in since it was added to DeviceConfig,
which this commit also fixes.

The final rung merges field by field, so setting final.dither cannot wipe
final.strength. `clear` takes one dot: \`dither\` clears the base rung,
\`final.dither\` the final rung's field, \`final\` the whole rung.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: the MCP `configure_device` mirror

`ConfigureDeviceArgs` is a **hand-written mirror** of `DeviceWrite` with its own `JsonSchema` and its own field-by-field conversion. It does not inherit Task 9's work. `gamut` has been missing from it since `gamut` existed, which is the drift this task fixes and then pins.

**Files:**
- Modify: `src/mcp/tools_device.rs` — `ConfigureDeviceArgs` (`:19-73`), the `body()` closure (`:126-143`)
- Test: `src/mcp/tools_device.rs` (`mod tests`), or the module's existing test location

**Interfaces:**
- Consumes: Task 9's `DeviceWrite` fields.
- Produces: nothing new.

- [ ] **Step 1: Write the failing test**

A test per field would drift as fast as the mirror does. Pin the *relationship* instead:

```rust
/// `ConfigureDeviceArgs` is a hand-written mirror of `DeviceWrite`. Only a
/// test keeps the two in step — `gamut` went missing from this mirror for a
/// whole release because nothing checked.
#[test]
fn the_mcp_mirror_carries_every_device_write_field() {
    // Every settable field, as the agent-facing names. `key` is excluded: it
    // is the entry name, not a setting, and the tool takes `mac` instead.
    const MIRRORED: [&str; 18] = [
        "screen_ref", "panel", "dither", "colors", "colors_actual", "gamut",
        "params", "refresh", "name", "max_error", "noise_scale",
        "chroma_clamp", "strength", "temperature_profile",
        "maximum_compatibility", "min_png_bytes", "final", "clear",
    ];
    let schema = schemars::schema_for!(ConfigureDeviceArgs);
    let json = serde_json::to_value(&schema).expect("schema serialises");
    let props = json["properties"].as_object().expect("an object schema");
    for field in MIRRORED {
        assert!(
            props.contains_key(field),
            "`{field}` is missing from the MCP tool's schema, so no agent can set it"
        );
    }
}

/// …and carrying it in the schema is not enough: the `body()` closure must
/// actually pass it on.
#[test]
fn the_mcp_mirror_passes_the_new_fields_to_device_write() {
    let args = ConfigureDeviceArgs {
        mac: "AA:BB:CC:DD:EE:FF".into(),
        colors_actual: Some("#1C5240,#FFFFFF".into()),
        gamut: Some(GamutTuningValues { knee: Some(0.8), ..Default::default() }),
        final_rung: Some(CascadeValues { dither: Some("atkinson".into()), ..Default::default() }),
        ..configure_device_args_for_test()
    };
    let w = args.to_device_write();
    assert_eq!(w.colors_actual.as_deref(), Some("#1C5240,#FFFFFF"));
    assert_eq!(w.gamut.and_then(|g| g.knee), Some(0.8));
    assert_eq!(
        w.final_rung.and_then(|f| f.dither), Some("atkinson".to_string()),
        "the body() closure must pass the final rung, not drop it"
    );
}
```

The second test needs the `body()` closure extracted into `ConfigureDeviceArgs::to_device_write(&self) -> DeviceWrite`. Do that in Step 3 — a conversion inside a closure inside an async method cannot be tested, which is part of why it drifted.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --lib mcp::tools_device 2>&1 | tail -15`
Expected: FAIL — `colors_actual`, `gamut` and `final` missing from the schema; `to_device_write` not found.

- [ ] **Step 3: Add the three fields and extract the conversion**

In `ConfigureDeviceArgs`, with agent-facing doc comments (the schema's descriptions are the only documentation an agent ever reads):

```rust
    /// Measured colours for this one display, comma-separated `#rrggbb` with
    /// the same entry count as the resolved palette. Overrides the panel
    /// profile's calibration for this device only; a screen that measures its
    /// own still outranks it.
    pub colors_actual: Option<String>,
    /// Gamut mapping for continuous-tone regions: `knee` (where compression
    /// begins), `amount` (0 = off, 1 = full) and `max_compression`.
    pub gamut: Option<GamutTuningValues>,
    /// The device owner's **veto**. Anything set here outrules the screen's
    /// own choice — use it when a setting must hold on this display whatever
    /// the script asks for, for example a `refresh` floor on a
    /// battery-powered panel. Settings outside this block are defaults the
    /// screen may override. Merged field by field; remove a field with
    /// `clear: ["final.<field>"]`, or the whole block with `clear: ["final"]`.
    #[serde(rename = "final")]
    pub final_rung: Option<CascadeValues>,
```

Then replace the `body` closure with a method:

```rust
impl ConfigureDeviceArgs {
    /// The `DeviceWrite` this call means.
    ///
    /// A named method rather than a closure inside `configure_device`: the
    /// conversion is the half of the mirror that drifts (`gamut` reached
    /// `DeviceConfig` and never reached here), and a closure cannot be tested.
    pub fn to_device_write(&self) -> DeviceWrite {
        DeviceWrite {
            key: None,
            screen: self.screen_ref.clone(),
            panel: self.panel.clone(),
            dither: self.dither.clone(),
            colors: self.colors.clone(),
            colors_actual: self.colors_actual.clone(),
            gamut: self.gamut.clone(),
            params: self.params.clone().map(params_to_yaml),
            refresh: self.refresh,
            name: self.name.clone(),
            max_error: self.max_error,
            noise_scale: self.noise_scale,
            chroma_clamp: self.chroma_clamp,
            strength: self.strength,
            temperature_profile: self.temperature_profile.clone(),
            maximum_compatibility: self.maximum_compatibility,
            min_png_bytes: self.min_png_bytes,
            final_rung: self.final_rung.clone(),
            clear: self.clear.clone(),
        }
    }
}
```

and in `configure_device`, `let body = || a.to_device_write();`.

`CascadeValues` and `GamutTuningValues` must derive `schemars::JsonSchema` for this to compile. Add the derive in `src/models/config.rs` behind no feature gate — `schemars` is already a dependency there if other models derive it; check with `grep -n "JsonSchema" src/models/config.rs` and follow whatever pattern is already in place.

Also extend the `clear` field's doc comment to document the dotted form, since the MCP description is an agent's only reference:

```rust
    /// Settings to remove from this device, by name — for example
    /// `["noise_scale"]`. Omitting a field means "leave it alone", so this is
    /// the only way to take a setting back and let the panel's value (or the
    /// firmware's default) apply again. A field cannot be set and cleared in
    /// the same call.
    ///
    /// One dot addresses the veto block: `"dither"` clears the device's
    /// default, `"final.dither"` clears its veto, and `"final"` removes the
    /// whole veto block. A bare name never touches `final`.
    pub clear: Option<Vec<String>>,
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib mcp 2>&1 | grep -E "^test result:|FAILED"`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/mcp/tools_device.rs src/models/config.rs
git diff --cached --stat
git commit -m "feat(mcp): configure_device can set final:, colors_actual and gamut

The MCP args are a hand-written mirror of DeviceWrite and inherit nothing.
The body() closure becomes to_device_write(), so the half that drifts is
testable, and a schema test pins every DeviceWrite field to the mirror —
gamut had been missing from it since gamut existed.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Home Assistant — the Dither control moves to `final.dither`

Spec §4.8's last subsection. The HA Dither select has **always meant an owner's veto**, because the base rung was a veto until Task 2. Leaving it on the base rung would mean a user picks "atkinson", the entity *shows* "atkinson", and the device renders whatever the screen asked for — a Home Assistant entity reporting a value that is not in effect, which is the exact failure this spec exists to remove, relocated into HA. Moving it keeps existing HA users at **zero behaviour change**.

**Files:**
- Modify: `custom_components/byonk/select.py:80-87` (`ByonkDitherSelect`)
- Modify: `custom_components/byonk/config_flow.py:193-207` (onboarding)
- Test: the component's existing test location (`ls custom_components/byonk/../../tests 2>/dev/null; grep -rln "ByonkDitherSelect" --include=*.py .`)

**Interfaces:**
- Consumes: Task 9's `final` support in `PATCH /api/admin/devices/{key}`.
- Produces: nothing new.

- [ ] **Step 1: Write the failing test**

If the repo has no Python test harness for the component, say so in the commit and verify through the HA UI instead — but check first, and do not assume. Where tests exist:

```python
async def test_dither_select_writes_the_final_rung(hass, coordinator):
    """The HA Dither control is an owner's veto and must stay one.

    Writing the base rung after the cascade flip would leave the entity
    showing a value the device does not render.
    """
    entity = ByonkDitherSelect(coordinator, "AA:BB:CC:DD:EE:FF")
    await entity.async_select_option("atkinson")
    assert coordinator.last_write == {"final": {"dither": "atkinson"}}


async def test_dither_select_reads_the_final_rung(coordinator):
    coordinator.data.devices["AA:BB:CC:DD:EE:FF"] = {
        "dither": "stucki",                 # a default the screen may override
        "final": {"dither": "atkinson"},    # the veto
    }
    entity = ByonkDitherSelect(coordinator, "AA:BB:CC:DD:EE:FF")
    assert entity.current_option == "atkinson", (
        "the entity must report the value in effect, not the base-rung default"
    )
```

- [ ] **Step 2: Run it to verify it fails**

Run whatever the component's test command is (`grep -rn "pytest\|hassfest" Makefile .github/workflows/*.yml | head`).
Expected: FAIL — the write is `{"dither": "atkinson"}`.

- [ ] **Step 3: Move both halves**

`_ByonkSelect.current_option` reads `device.get(self._field)`. `ByonkDitherSelect` needs its own, because its value lives one level down:

```python
class ByonkDitherSelect(_ByonkSelect):
    # The HA control is the device OWNER's veto, so it reads and writes
    # byonk's `final` rung, not the bare `dither` key. byonk's cascade puts a
    # bare `dither` BELOW the screen's own choice, so writing there would show
    # the user a value their panel does not render. `final` has outranked the
    # screen since byonk 0.20.0, which is what this control has always meant.
    _field = "dither"

    def _base_options(self) -> list[str]:
        return self.coordinator.data.dither

    @property
    def current_option(self) -> str | None:
        device = self.device
        if not device:
            return None
        # `or {}` and not `.get("final", {})`: byonk omits an empty rung, but a
        # hand-edited config.yaml can carry `final:` with nothing under it,
        # which arrives as null.
        return (device.get("final") or {}).get("dither")

    async def async_select_option(self, option: str) -> None:
        await self._write({"final": {"dither": option}})
```

`self.device` is `ByonkDeviceEntity`'s own property (`custom_components/byonk/entity.py:50`), the same one `_ByonkSelect.current_option` reads at `select.py:61`.

`config_flow.py:193-207` collects `dither` into `self._extra` at onboarding and PATCHes it the same way. Move it:

```python
            self._extra = {k: user_input[k] for k in ("panel",) if user_input.get(k)}
            if user_input.get("dither"):
                # The onboarding Dither choice is the owner's veto, exactly as
                # the Dither entity is — see ByonkDitherSelect.
                self._extra["final"] = {"dither": user_input["dither"]}
```

Leave the form schema alone: the user-facing control does not change, only where its value lands.

- [ ] **Step 4: Run the tests and `hassfest`**

Run the component's test command → PASS. Then whatever lint the repo runs on the component (`grep -rn "hassfest\|ruff" .github/workflows/*.yml | head`).

- [ ] **Step 5: Commit**

```bash
git add custom_components/byonk/select.py custom_components/byonk/config_flow.py
git diff --cached --stat
git commit -m "fix(ha): the Dither control writes the final rung

byonk 0.20.0's cascade puts a bare \`dither\` below the screen's own choice.
The HA Dither select has always meant the owner's veto — it was one because
the base rung was one — so it moves to \`final.dither\`, in the entity and at
onboarding. Existing HA users see zero behaviour change; left on the base
rung the entity would report a value the device does not render.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: documentation, and the migration note

Read spec §6. Three shipped pages become **actively false** after Task 2 — a screen author reading them would conclude their `dither` is inert and remove it. This task is not optional polish; it is the half of the breaking change the user actually encounters.

**Files:**
- Modify: `docs/src/api/lua-api.md:1283-1300` (`refresh` + `colors` chains), `:1364-1377` (the dither chain)
- Modify: `docs/src/guide/configuration.md:519-540` (both priority tables) and a new `final:` section
- Modify: `docs/src/concepts/content-pipeline.md:253,268` (two priority chains) — **permanently modified in the maintainer's tree; see Step 4**
- Modify: `CHANGES.md`

**Interfaces:** none.

- [ ] **Step 1: Rewrite the `lua-api.md` chains**

At `:1364-1377`, the numbered dither chain ("1. Dev UI override 2. Device config 3. Script 4. Default") and the sentence "**The device outranks the screen here.**" Every sentence changes. Replace with the cascade, and say plainly what moved:

```markdown
The algorithm is resolved on byonk's render cascade, highest first:

1. A live dev-UI override
2. The device's `final.dither` — the owner's veto
3. The script's `dither` — **this is you**
4. The device's `dither` — the owner's default
5. `atkinson`

**The screen outranks the device's default, and loses to its veto.** A device
sets `dither:` to say "normally use this here", which your script may override
because only the script knows what it just drew; it sets `final: {dither: …}`
to say "on this display, no" — for instance because the panel hangs in a dark
hallway and the owner can see the result and you cannot.

> **Changed in 0.20.0.** Before 0.20.0 a bare `devices.<key>.dither` outranked
> the script. If you maintain a config that relied on that, move the value
> into `final:`.
```

At `:1283-1287`, the `refresh` chain is **still correct** under spec §2 — confirm rather than rewrite, and add the one new rung (`final.refresh` above the script). At `:1288-1300`, the `colors` chain is reframed as the same five rungs plus the firmware `Colors` header as a *fallback* below them all.

- [ ] **Step 2: Rewrite `configuration.md`'s tables and add the `final:` section**

Both priority tables at `:519-540` become one, because there is now one order. Delete the "the algorithm suits the panel rather than the content" rationale — it was the justification for the inversion and is no longer byonk's position.

The new section documents the three additions with a worked example:

```markdown
### `final:` — the owner's veto

Settings written directly under a device are **defaults**: a screen may
override them, because the script is the only participant that can inspect
the content it is about to render. Settings under `final:` are **vetoes**:
nothing overrides them.

```yaml
devices:
  AA:BB:CC:DD:EE:FF:
    screen: local/weather
    panel: trmnl_x

    dither: floyd-steinberg    # a default — a screen may pick its own
    strength: 0.9              # a default
    colors_actual: "#1C5240,#00994D,…"   # this display's own measurement

    final:
      dither: atkinson         # a veto — no screen overrides this
      refresh: 1800            # this panel runs on a battery
```

`final:` accepts the same settings as the base rung — `colors`,
`colors_actual`, `dither`, `max_error`, `noise_scale`, `chroma_clamp`,
`strength`, `refresh` and `gamut` — and nothing else: an unrecognised key
there is a startup error, so a typo cannot silently do nothing.

Through the admin API or the MCP tool, `final:` merges field by field, and a
field is removed with one dot: `clear: ["dither"]` removes the default,
`clear: ["final.dither"]` the veto, `clear: ["final"]` the whole block.
```

- [ ] **Step 3: Write the CHANGES.md entry**

Under `## [Unreleased]`, user-visible only — no CI, tooling or process:

```markdown
### Changed

- **BREAKING: a device's `dither` no longer overrides a screen that sets its
  own.** Settings written directly under a device are now defaults the screen
  may override; the new `final:` block holds settings nothing overrides. To
  keep a device's old behaviour, move its `dither` into `final:`:

  ```yaml
  devices:
    AA:BB:CC:DD:EE:FF:
      final:
        dither: atkinson
  ```

  **Home Assistant users need do nothing.** The integration's Dither control
  has always meant "on this display, no", and now writes `final.dither`, so
  what it shows is what the device renders. Update the integration together
  with byonk: an older integration writes the device's default instead, and
  its Dither entity would then report a value the screen overrides.

  `refresh` is unaffected — a screen's `refresh_rate` already outranked a
  device's `refresh`, and still does.

### Added

- `final:` on a device: a block whose settings no screen can override.
  Accepts `colors`, `colors_actual`, `dither`, `max_error`, `noise_scale`,
  `chroma_clamp`, `strength`, `refresh` and `gamut`. `final.refresh` is new
  capability — a cap on how often a battery-powered display wakes up.
- `colors_actual` on a device: this one display's measured colours,
  overriding its panel profile's calibration.
- `gamut` is now settable through the admin API and the MCP `configure_device`
  tool. It was documented in `config.yaml` but no writer could reach it.
- Every render setting now resolves through one documented order in all five
  render paths — the device preview, the admin preview, the dev preview, the
  CLI and the device itself. They previously disagreed.
```

- [ ] **Step 4: `content-pipeline.md` — coordinate, never stage as a side effect**

This file is **permanently modified in the maintainer's working tree** AND a doc the spec says must change. Do not `git add` it as part of any other commit and do not `git checkout` it.

```bash
git diff --stat docs/src/concepts/content-pipeline.md   # the maintainer's edits
```

Make the two priority-chain corrections at `:253` and `:268` on top of what is there, then show the maintainer the combined diff and ask before staging this one file. If they prefer to carry it themselves, commit the rest of Task 12 without it and say so in the commit message — an uncommitted doc correction is visible; a silently reverted maintainer edit is not.

- [ ] **Step 5: Build the docs and check the links**

```bash
(cd docs && mdbook build) 2>&1 | tail -5
grep -rn "device outranks the screen\|Device config
2\." docs/src/ || echo "no stale dither-chain text left"
```

Expected: mdbook exit 0; the grep finds nothing. Also re-read the three pages rendered, not just the source — a priority table that is right in Markdown and broken in HTML is still a page that misleads.

- [ ] **Step 6: Commit**

```bash
git add docs/src/api/lua-api.md docs/src/guide/configuration.md CHANGES.md
git diff --cached --stat   # content-pipeline.md must NOT appear unless agreed
git commit -m "docs: the render cascade, and the 0.20.0 migration note

Three pages said the device outranks the screen on dither, which stops
being true in this release — a screen author reading them would have
removed a dither that works. One priority table replaces the two, and
configuration.md gains a final: section.

CHANGES.md names the Home Assistant consequence: the integration must be
updated together with byonk, or its Dither entity reports a value the
screen overrides.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

## Final verification (after Task 12)

Run the whole recipe, in this order, Rust and webui **never in parallel**:

```bash
make webui
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace 2>&1 | grep -E "^test result:|FAILED"
make webui-test 2>&1 | tail -5
(cd docs && mdbook build)
```

All green, zero failures. Then:

1. **Grep for a sixth resolver.** `grep -rn "\.or(.*dither\|dither.*\.or(" src/ --include=*.rs` — any surviving hand-rolled precedence on a cascade setting is a path the convergence test does not cover, which means there is a sixth path nobody listed. Report it rather than fixing it quietly.
2. **One fresh whole-branch review on the most capable model**, over `147b3be1..HEAD` — this branch plus `feat/dev-editor` below it, since the owner wants one PR for the whole admin UI. Tell the reviewer that `.superpowers/` is git-ignored, so ledger corrections do not appear in the committed diff and reporting them as missing is a false positive.
3. **Then the dev-UI work-area plan** (`docs/superpowers/specs/2026-10-04-dev-ui-work-area-design.md`), which is the last piece of the PR, followed by the §4.7 override-TTL plan.
4. **The owner validates the UI and the HA VM once, at the end** — not here, and not mid-implementation.
