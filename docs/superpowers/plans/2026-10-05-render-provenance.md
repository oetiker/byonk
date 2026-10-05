# Render Provenance (cascade piece 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The dev render becomes `POST /dev/api/render`, accepts a screen and a device together plus an uncommitted draft, and answers with an `X-Byonk-Resolution` header that states, for every render setting, every rung's value and status and which rung won.

**Architecture:** A new pure module `src/api/resolution.rs` holds the cascade types (`Rung`, `RungStatus`, `Ladder<T>`) and one generic resolver. The dev render computes its post-script render settings **through that resolver** — the report and the pixels come from the same computation, so they cannot disagree. The per-setting precedence orders are constants that encode **today's** behaviour (the `dither` inversion included); piece 3 of the spec changes the constants, not the machinery. An equivalence test pins the new resolver to the legacy one (`resolve_render_params` + `resolve_effective_tuning` + `effective_dither`) over a matrix of inputs.

**Tech Stack:** Rust (axum 0.8, serde, serde_json), Svelte 5 SPA (`webui/`, vitest), mdBook.

**Spec:** `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md` — this plan is §8 piece 2 (§4.4, §4.5, §4.6). Consumer: `docs/superpowers/specs/2026-10-04-dev-ui-work-area-design.md` §5 and §8.1–8.2.

**Branch:** `feat/render-cascade`, based on `feat/dev-editor` @ `147b3be1`.

## Rulings this plan makes (read before Task 1)

The spec leaves these open or states them in a way that cannot be built as written. Each is decided here; the owner reviews them with the plan.

1. **Piece 2 reports today's ladders; it does not flip them.** The payload uses the spec's six rung names, but each setting resolves in its *current* order — `dither` keeps `override → device base → screen`. The flip is piece 3. Each setting's report carries an `order` array so the UI can draw the true order now and keeps working when piece 3 changes it. `device_final` is always `absent` in piece 2: no config can hold it yet.
2. **The draft replaces its rung; it does not feed the `override` rung.** Spec §4.6 says "the `draft` feeds the `override` rung". Built that way, a draft panel value would win every setting, and the UI spec's own mock-up (§5: "your panel value is not used in this preview", screen wins) could never appear. The draft substitutes the rung it belongs to — `draft.base` for the device base rung, `draft.panel` for the panel rung — **wholesale**, so a preview can show the effect of *removing* a value. The `override` rung is the request's separate `override` object: today's ad-hoc knobs.
3. **The dev render does not read the live `DevOverrides` maps yet.** After a Push, the colour-tuning UI keeps editing a working copy without re-pushing. If the live map sat above the request's override, the preview would freeze on the pushed value with no widget to explain it. Reading live overrides into the dev render belongs with the ladder widget (UI plan) and the TTL (piece 3).
4. **Settings the config cannot hold yet are refused, not silently dropped.** `draft.final` (no such rung until piece 3), `draft.base.colors_actual` (device-level `colors_actual` is piece 3), `override.colors` and `override.refresh` (the pre-script device context has no slot for them, so the script would see a palette other than the one rendered) answer `400` with a message naming piece 3. Unknown JSON fields anywhere in the body answer `400` (`deny_unknown_fields`).
5. **`GET /dev/api/render` is removed in this piece, and the SPA moves to the POST in the same piece.** Two render endpoints are two resolution paths that drift (memory: five render paths must agree). The SPA change is transport only — no UI change.
6. **The pre-script device context is unchanged.** What the script sees as `device.*` is computed exactly as today, from the (draft-aware) device and panel rungs. Only post-script resolution moves onto the ladder.
7. **The renderer never receives fallback values.** When a setting resolves to `fallback`, the renderer gets `None` exactly as today; the fallback value appears only in the report. Passing explicit defaults could select a different engine code path (`resolve_dither_tuning`'s `has_tuning`).

## Global Constraints

- Never `git add -A` or `git add .`. Four files are permanently modified in the maintainer's tree and must never be staged: `config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`. Stage by explicit path; check `git diff --cached --stat` before every commit.
- Verification is these commands, never `make check` alone: `make webui` FIRST, then `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `(cd docs && mdbook build)`, plus `make webui-test`. Run the Rust suite and the webui suite one after the other, not in parallel: the journey tests time out under load.
- Node: `pnpm` only. Svelte 5 runes.
- English for code, comments, identifiers, technical docs.
- CHANGES.md: **no entry** — the dev UI and its API are unreleased; the user-visible behaviour of shipped paths (`/api/display`, CLI, admin preview, MCP) does not change in this piece.
- Specs are the historical record: do not edit either 2026-10-04 spec to match this plan.
- `X-Byonk-Resolution` is on the dev render only, never on `/api/display`.
- Header budget: the serialized header must stay under 4 KiB for a 16-entry palette on every rung that can carry one.
- Every commit ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **A wrong-length `colors_actual` on a middle rung.** The report must show that rung `rejected` with the reason and the walk must continue to a lower rung — and the image must use the lower rung. (Task 2 matrix + Task 4 integration test.)
2. **The report and the image disagree.** Any path where the handler computes a value for the renderer outside `resolve_dev_render` re-opens the bug class this piece exists to close. (Task 2 equivalence test; Task 4 test asserting the header's effective dither equals the algorithm the PNG was rendered with.)
3. **Header readable through Home Assistant Ingress.** A same-origin `fetch` can read custom response headers; Ingress proxies headers through. Not testable here — the owner's end-of-work validation must confirm the header arrives in the browser under Ingress.
4. **A screen and a device together.** `screen` must win the screen choice and the device must still supply every setting, its `params` included; with only `device`, the device's own screen. (Task 4.)
5. **Non-ASCII in a rung value.** Header values must be visible ASCII. `serde_json` escapes nothing by default; a `colors` value is hex, but a `Rejected.reason` or a dither name typed by a user may not be. The header serializer must escape non-ASCII (`\uXXXX`). (Task 1 test.)

---

## File Structure

| File | Responsibility |
|---|---|
| `src/api/resolution.rs` (new) | `Rung`, `RungStatus`, `Ladder<T>`, `resolve()`, the report types, the header name, today's precedence orders, `RungSettings`, `resolve_dev_render()` and its equivalence test |
| `src/api/mod.rs` | `pub mod resolution;` |
| `src/models/config.rs` | `CascadeValues`, `DeviceConfig::base()` |
| `src/services/content_pipeline.rs` | `ScriptResult` gains the raw refresh inputs |
| `src/api/dev.rs` | `RenderRequest` body, `handle_render` as POST through `resolve_dev_render`, the header, route change |
| `tests/dev_render_is_read_only_test.rs` | ported to POST |
| `tests/dev_render_provenance_test.rs` (new) | integration tests for the POST |
| `webui/src/lib/api.js`, `webui/src/lib/state/simulate.svelte.js`, `webui/src/lib/components/PreviewPane.svelte` | transport switch |
| `webui/test/api.test.js`, `webui/test/simulate.test.js`, `webui/test/preview.test.js` (+ any journey test that stubs the render) | updated |
| `docs/src/guide/dev-mode.md`, `docs/src/api/lua-api.md` | endpoint table, request/response section, the "answers with the PNG and nothing else" paragraph |

---

### Task 1: The ladder and its report

**Files:**
- Create: `src/api/resolution.rs`
- Modify: `src/api/mod.rs`

**Interfaces:**
- Produces:
  - `pub enum Rung { Override, DeviceFinal, Screen, DeviceBase, Panel, Fallback }` — `Serialize`, snake_case, `Ord`.
  - `pub enum RungStatus { Winner, Outranked, Rejected { reason: String }, Absent }` — `Serialize` as `{"status":"winner"}` / `{"status":"rejected","reason":"…"}`.
  - `pub struct Ladder<T> { pub over, pub device_final, pub screen, pub device_base, pub panel, pub fallback: Option<T> }` with `Default` and `fn get(&self, Rung) -> Option<&T>`.
  - `pub struct Resolved<T> { pub effective: Option<T>, pub from: Rung, pub order: Vec<Rung>, pub rungs: Vec<(Rung, Option<T>, RungStatus)> }` with `fn configured(&self) -> Option<&T>` (the effective value unless it came from `Fallback`) and `fn report(&self, to_json: impl Fn(&T) -> serde_json::Value) -> SettingReport`.
  - `pub fn resolve<T: Clone>(ladder: &Ladder<T>, order: &[Rung], accept: impl Fn(&T) -> Result<(), String>) -> Resolved<T>`
  - `pub fn accept_any<T>(_: &T) -> Result<(), String>`
  - `pub struct SettingReport`, `pub struct RungReport`, `pub type Resolution = BTreeMap<&'static str, SettingReport>`
  - `pub const RESOLUTION_HEADER: &str = "x-byonk-resolution";`
  - `pub fn header_value(r: &Resolution) -> String` — compact JSON, non-ASCII escaped.

- [ ] **Step 1: Write the failing tests**

Create `src/api/resolution.rs` with only the test module first (plus `pub mod resolution;` in `src/api/mod.rs`, keeping the module list alphabetical):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ladder(over: Option<i32>, screen: Option<i32>, base: Option<i32>, fallback: Option<i32>) -> Ladder<i32> {
        Ladder { over, screen, device_base: base, fallback, ..Default::default() }
    }

    const ORDER: &[Rung] = &[Rung::Override, Rung::DeviceFinal, Rung::Screen, Rung::DeviceBase, Rung::Panel, Rung::Fallback];

    #[test]
    fn highest_present_rung_wins_and_lower_ones_are_outranked() {
        let r = resolve(&ladder(None, Some(2), Some(3), Some(9)), ORDER, accept_any);
        assert_eq!(r.effective, Some(2));
        assert_eq!(r.from, Rung::Screen);
        let status = |rung| r.rungs.iter().find(|(x, _, _)| *x == rung).unwrap().2.clone();
        assert_eq!(status(Rung::Override), RungStatus::Absent);
        assert_eq!(status(Rung::Screen), RungStatus::Winner);
        assert_eq!(status(Rung::DeviceBase), RungStatus::Outranked);
        assert_eq!(status(Rung::Fallback), RungStatus::Outranked);
        assert_eq!(r.configured(), Some(&2));
    }

    #[test]
    fn the_order_argument_decides_not_the_field_names() {
        // Today's `dither` puts the device base above the screen.
        let order = &[Rung::Override, Rung::DeviceFinal, Rung::DeviceBase, Rung::Screen, Rung::Panel, Rung::Fallback];
        let r = resolve(&ladder(None, Some(2), Some(3), None), order, accept_any);
        assert_eq!(r.from, Rung::DeviceBase);
        assert_eq!(r.effective, Some(3));
    }

    #[test]
    fn a_rejected_rung_is_reported_and_the_walk_continues() {
        let r = resolve(&ladder(Some(-1), Some(2), None, None), ORDER, |v: &i32| {
            if *v < 0 { Err("negative".to_string()) } else { Ok(()) }
        });
        assert_eq!(r.from, Rung::Screen);
        assert_eq!(r.rungs[0].2, RungStatus::Rejected { reason: "negative".to_string() });
    }

    #[test]
    fn nothing_set_resolves_to_fallback_and_configured_is_none() {
        let r = resolve(&ladder(None, None, None, Some(9)), ORDER, accept_any);
        assert_eq!(r.from, Rung::Fallback);
        assert_eq!(r.effective, Some(9));
        assert_eq!(r.configured(), None, "the renderer must never see a fallback value");

        let empty = resolve(&Ladder::<i32>::default(), ORDER, accept_any);
        assert_eq!(empty.from, Rung::Fallback);
        assert_eq!(empty.effective, None);
    }

    #[test]
    fn report_serializes_in_the_documented_shape() {
        let r = resolve(&ladder(None, Some(2), Some(3), None), ORDER, accept_any);
        let json = serde_json::to_value(r.report(|v| serde_json::json!(v))).unwrap();
        assert_eq!(json["effective"], 2);
        assert_eq!(json["from"], "screen");
        assert_eq!(json["order"][0], "override");
        assert_eq!(json["rungs"]["screen"], serde_json::json!({"value": 2, "status": "winner"}));
        assert_eq!(json["rungs"]["device_base"], serde_json::json!({"value": 3, "status": "outranked"}));
        assert_eq!(json["rungs"]["override"], serde_json::json!({"status": "absent"}));
    }

    #[test]
    fn header_value_is_ascii_even_for_non_ascii_input() {
        let r = resolve(
            &Ladder { screen: Some("grün".to_string()), ..Default::default() },
            ORDER,
            accept_any,
        );
        let mut res = Resolution::new();
        res.insert("dither", r.report(|v| serde_json::json!(v)));
        let h = header_value(&res);
        assert!(h.is_ascii(), "{h}");
        assert!(h.contains("\\u00fc"), "{h}");
        let back: serde_json::Value = serde_json::from_str(&h).unwrap();
        assert_eq!(back["dither"]["effective"], "grün");
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib -q -- api::resolution`
Expected: compile errors — `Rung`, `resolve`, … not found.

- [ ] **Step 3: Implement**

Above the test module:

```rust
//! The render-setting cascade: rungs, their statuses, and one resolver.
//!
//! See `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md`
//! §2 and §4.4. Precedence is an argument, not a property of the types, so the
//! same machinery serves today's per-setting orders and the spec's single one.

use serde::Serialize;
use std::collections::BTreeMap;

/// Response header carrying the [`Resolution`] of a dev render.
pub const RESOLUTION_HEADER: &str = "x-byonk-resolution";

/// A level of the cascade. `Fallback` is reported but never configured: it
/// is byonk's own default or the device's self-report (spec §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rung {
    Override,
    DeviceFinal,
    Screen,
    DeviceBase,
    Panel,
    Fallback,
}

/// What happened to one rung's value during resolution (spec §4.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RungStatus {
    Winner,
    /// A higher rung carried an accepted value.
    Outranked,
    /// Supplied, but unusable — e.g. a `colors_actual` whose entry count
    /// does not match the palette.
    Rejected { reason: String },
    Absent,
}

/// Every value a setting could take, one slot per rung.
#[derive(Debug, Clone)]
pub struct Ladder<T> {
    pub over: Option<T>,
    pub device_final: Option<T>,
    pub screen: Option<T>,
    pub device_base: Option<T>,
    pub panel: Option<T>,
    pub fallback: Option<T>,
}

impl<T> Default for Ladder<T> {
    fn default() -> Self {
        Self { over: None, device_final: None, screen: None, device_base: None, panel: None, fallback: None }
    }
}

impl<T> Ladder<T> {
    pub fn get(&self, rung: Rung) -> Option<&T> {
        match rung {
            Rung::Override => self.over.as_ref(),
            Rung::DeviceFinal => self.device_final.as_ref(),
            Rung::Screen => self.screen.as_ref(),
            Rung::DeviceBase => self.device_base.as_ref(),
            Rung::Panel => self.panel.as_ref(),
            Rung::Fallback => self.fallback.as_ref(),
        }
    }
}

/// The outcome of resolving one setting.
#[derive(Debug, Clone)]
pub struct Resolved<T> {
    pub effective: Option<T>,
    /// The winning rung; `Fallback` when no configured rung won, even if the
    /// fallback slot is empty too.
    pub from: Rung,
    /// Highest first, as passed to [`resolve`].
    pub order: Vec<Rung>,
    pub rungs: Vec<(Rung, Option<T>, RungStatus)>,
}

impl<T> Resolved<T> {
    /// The value the renderer receives: the effective value unless it is the
    /// fallback, which the engine applies on its own (plan ruling 7).
    pub fn configured(&self) -> Option<&T> {
        if self.from == Rung::Fallback { None } else { self.effective.as_ref() }
    }

    pub fn report(&self, to_json: impl Fn(&T) -> serde_json::Value) -> SettingReport {
        SettingReport {
            effective: self.effective.as_ref().map(&to_json).unwrap_or(serde_json::Value::Null),
            from: self.from,
            order: self.order.clone(),
            rungs: self
                .rungs
                .iter()
                .map(|(rung, value, status)| {
                    (*rung, RungReport { value: value.as_ref().map(&to_json), status: status.clone() })
                })
                .collect(),
        }
    }
}

/// Walk `order` (highest first). The first present value that `accept`
/// allows wins; a present value it refuses before the winner is `Rejected`
/// and the walk continues; everything present below the winner is
/// `Outranked` without being checked — the same rule
/// `display::resolve_measured_colors` applies.
pub fn resolve<T: Clone>(
    ladder: &Ladder<T>,
    order: &[Rung],
    accept: impl Fn(&T) -> Result<(), String>,
) -> Resolved<T> {
    let mut winner: Option<(Rung, T)> = None;
    let mut rungs = Vec::with_capacity(order.len());
    for &rung in order {
        let value = ladder.get(rung).cloned();
        let status = match (&value, &winner) {
            (None, _) => RungStatus::Absent,
            (Some(_), Some(_)) => RungStatus::Outranked,
            (Some(v), None) => match accept(v) {
                Ok(()) => {
                    winner = Some((rung, v.clone()));
                    RungStatus::Winner
                }
                Err(reason) => RungStatus::Rejected { reason },
            },
        };
        rungs.push((rung, value, status));
    }
    let (from, effective) = match winner {
        Some((rung, v)) => (rung, Some(v)),
        None => (Rung::Fallback, None),
    };
    Resolved { effective, from, order: order.to_vec(), rungs }
}

/// The `accept` for settings with no validity rule.
pub fn accept_any<T>(_: &T) -> Result<(), String> {
    Ok(())
}

/// One setting's entry in the [`Resolution`] payload.
#[derive(Debug, Clone, Serialize)]
pub struct SettingReport {
    pub effective: serde_json::Value,
    pub from: Rung,
    pub order: Vec<Rung>,
    pub rungs: BTreeMap<Rung, RungReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RungReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(flatten)]
    pub status: RungStatus,
}

/// Setting name → report. Keys are the spec's field names; the three gamut
/// knobs appear as `gamut.knee`, `gamut.amount`, `gamut.max_compression`
/// because they resolve field by field.
pub type Resolution = BTreeMap<&'static str, SettingReport>;

/// The header form of a [`Resolution`]: one line of JSON, every non-ASCII
/// character escaped, because HTTP header values must be visible ASCII.
pub fn header_value(resolution: &Resolution) -> String {
    let json = serde_json::to_string(resolution).expect("a Resolution always serializes");
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for unit in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}
```

Note: the `RungStatus` match on a `Some(_)` value with a rejected earlier rung and no winner yet falls in the third arm, which is correct. In the `BTreeMap<Rung, _>` the keys serialize as strings because `Rung` is a unit-variant enum; this is what `report_serializes_in_the_documented_shape` checks.

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib -q -- api::resolution`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add src/api/resolution.rs src/api/mod.rs
git diff --cached --stat
git commit -m "feat(resolution): the cascade ladder, its rung statuses and report

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `resolve_dev_render`, pinned to the legacy resolver

**Files:**
- Modify: `src/api/resolution.rs`, `src/models/config.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces:
  - `models::config::CascadeValues` — `Debug, Clone, Default, PartialEq, Deserialize, Serialize`, `#[serde(deny_unknown_fields)]`, fields exactly as spec §3: `colors, colors_actual, dither: Option<String>`, `max_error, noise_scale, chroma_clamp, strength: Option<f32>`, `refresh: Option<u32>`, `#[serde(default)] gamut: GamutTuningValues`.
  - `DeviceConfig::base(&self) -> CascadeValues` — field copy; `colors_actual: None` (no device field until piece 3).
  - `resolution::RungSettings { colors, colors_actual: Option<Vec<(u8,u8,u8)>>, dither: Option<String>, tuning: DitherTuningValues, refresh: Option<u32> }` with `Default` and `fn from_cascade(&CascadeValues) -> Self` (colour strings parsed with `display::parse_colors_header`).
  - `resolution::DevRenderInputs<'a> { over: RungSettings, screen: RungSettings, device_base: RungSettings, panel: RungSettings, panel_dither: Option<&'a PanelDitherConfig>, fallback_palette: Vec<(u8,u8,u8)>, screen_default_refresh: u32 }`
  - `resolution::DevRenderResolution { palette: Vec<(u8,u8,u8)>, measured_colors: Option<Vec<(u8,u8,u8)>>, dither: Option<String>, tuning: DitherTuningValues, refresh: u32, report: Resolution }`
  - `resolution::resolve_dev_render(&DevRenderInputs) -> DevRenderResolution`
  - The order constants `ORDER_COLORS`, `ORDER_COLORS_ACTUAL`, `ORDER_DITHER`, `ORDER_TUNING`, `ORDER_REFRESH`.

Today's orders, each written down from the code they replace (keep these comments in the source):

| constant | highest → lowest | source of truth today |
|---|---|---|
| `ORDER_COLORS` | Override, DeviceFinal, Screen, DeviceBase, Panel, Fallback | `resolve_render_params` palette chain |
| `ORDER_COLORS_ACTUAL` | Screen, Override, DeviceFinal, DeviceBase, Panel, Fallback | `resolve_render_params` prepends the script before the dev-override candidate |
| `ORDER_DITHER` | Override, DeviceFinal, DeviceBase, Screen, Panel, Fallback | `effective_dither(device_slot, script_slot)`; the dev override rides the device slot |
| `ORDER_TUNING` | Override, DeviceFinal, Screen, DeviceBase, Panel, Fallback | `resolve_effective_tuning` |
| `ORDER_REFRESH` | Override, DeviceFinal, Screen, DeviceBase, Fallback | `content_pipeline::resolve_refresh_rate` (the panel has no refresh) |

`DeviceFinal` is placed where spec §2 puts it; it is always empty in piece 2.

- [ ] **Step 1: Write the failing tests**

In `src/models/config.rs` tests (find the existing `mod tests`):

```rust
#[test]
fn cascade_values_refuses_unknown_fields() {
    let err = serde_yaml::from_str::<CascadeValues>("screen: local/x\n").unwrap_err();
    assert!(err.to_string().contains("unknown field"), "{err}");
}

#[test]
fn device_base_copies_every_cascade_field() {
    let dc: DeviceConfig = serde_yaml::from_str(
        "screen: local/x\ncolors: '#000000,#FFFFFF'\ndither: atkinson\nmax_error: 0.5\nnoise_scale: 2.0\nchroma_clamp: 0.1\nstrength: 0.8\nrefresh: 600\ngamut: { knee: 0.7 }\n",
    )
    .unwrap();
    let b = dc.base();
    assert_eq!(b.colors.as_deref(), Some("#000000,#FFFFFF"));
    assert_eq!(b.dither.as_deref(), Some("atkinson"));
    assert_eq!((b.max_error, b.noise_scale, b.chroma_clamp, b.strength), (Some(0.5), Some(2.0), Some(0.1), Some(0.8)));
    assert_eq!(b.refresh, Some(600));
    assert_eq!(b.gamut.knee, Some(0.7));
    assert_eq!(b.colors_actual, None);
}
```

In `src/api/resolution.rs` tests — the behaviour tests and the equivalence matrix:

```rust
    use crate::api::display;
    use crate::models::{DitherTuningValues, PanelDitherConfig};
    use std::collections::HashMap;

    fn grey4() -> Vec<(u8, u8, u8)> { display::parse_colors_header("#000000,#555555,#AAAAAA,#FFFFFF") }
    fn col6() -> Vec<(u8, u8, u8)> { display::parse_colors_header("#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00") }
    fn act4() -> Vec<(u8, u8, u8)> { display::parse_colors_header("#101010,#505050,#A0A0A0,#E0E0E0") }
    fn act6() -> Vec<(u8, u8, u8)> { display::parse_colors_header("#101010,#E0E0E0,#B03030,#D0C040,#3040A0,#00994D") }

    fn inputs<'a>() -> DevRenderInputs<'a> {
        DevRenderInputs {
            over: RungSettings::default(),
            screen: RungSettings::default(),
            device_base: RungSettings::default(),
            panel: RungSettings::default(),
            panel_dither: None,
            fallback_palette: grey4(),
            screen_default_refresh: 900,
        }
    }

    #[test]
    fn a_wrong_length_colors_actual_is_rejected_and_a_lower_rung_wins() {
        let mut i = inputs();
        i.over.colors_actual = Some(act6()); // palette is 4 entries
        i.panel.colors_actual = Some(act4());
        let r = resolve_dev_render(&i);
        assert_eq!(r.measured_colors, Some(act4()));
        let rep = &r.report["colors_actual"];
        assert_eq!(rep.from, Rung::Panel);
        assert!(matches!(rep.rungs[&Rung::Override].status, RungStatus::Rejected { .. }));
    }

    #[test]
    fn panel_tuning_is_looked_up_for_the_winning_algorithm() {
        let mut algorithms = HashMap::new();
        algorithms.insert("floyd-steinberg".to_string(), DitherTuningValues { strength: Some(0.3), ..Default::default() });
        let pdc = PanelDitherConfig { defaults: DitherTuningValues { strength: Some(0.9), ..Default::default() }, algorithms };
        let mut i = inputs();
        i.panel_dither = Some(&pdc);
        i.screen.dither = Some("atkinson".to_string());
        i.device_base.dither = Some("floyd-steinberg".to_string()); // wins today
        let r = resolve_dev_render(&i);
        assert_eq!(r.dither.as_deref(), Some("floyd-steinberg"));
        assert_eq!(r.tuning.strength, Some(0.3));
        assert_eq!(r.report["strength"].from, Rung::Panel);
    }

    #[test]
    fn fallbacks_are_reported_but_not_handed_to_the_renderer() {
        let r = resolve_dev_render(&inputs());
        assert_eq!(r.dither, None);
        assert_eq!(r.report["dither"].from, Rung::Fallback);
        assert_eq!(r.report["dither"].effective, serde_json::json!("atkinson"));
        assert_eq!(r.tuning, DitherTuningValues::default());
        assert_eq!(r.report["strength"].effective, serde_json::json!(1.0));
        assert_eq!(r.palette, grey4(), "the palette fallback IS used — it is the model default");
        assert_eq!(r.report["refresh"].effective, serde_json::json!(900));
        assert_eq!(r.refresh, 900);
    }

    #[test]
    fn refresh_follows_todays_rule_screen_over_device() {
        let mut i = inputs();
        i.screen.refresh = Some(60);
        i.device_base.refresh = Some(600);
        let r = resolve_dev_render(&i);
        assert_eq!(r.refresh, 60);
        assert_eq!(r.report["refresh"].rungs[&Rung::DeviceBase].status, RungStatus::Outranked);
    }

    #[test]
    fn device_final_is_always_absent_in_piece_2() {
        let r = resolve_dev_render(&inputs());
        for (name, rep) in &r.report {
            assert_eq!(rep.rungs[&Rung::DeviceFinal].status, RungStatus::Absent, "{name}");
        }
    }

    /// The legacy computation `handle_render` performed before this piece,
    /// verbatim in structure: `effective_dither` for the algorithm, the panel
    /// block for that algorithm, `resolve_effective_tuning`, then
    /// `resolve_render_params`. The override's dither rides the device slot
    /// and blanks the script's, exactly as `dev.rs` did.
    fn legacy(i: &DevRenderInputs) -> (Vec<(u8, u8, u8)>, Option<Vec<(u8, u8, u8)>>, Option<String>, DitherTuningValues) {
        let (script_dither, device_dither) = match i.over.dither.as_deref() {
            Some(d) => (None, Some(d)),
            None => (i.screen.dither.as_deref(), i.device_base.dither.as_deref()),
        };
        let algo = display::effective_dither(device_dither, script_dither);
        let panel_tuning = i.panel_dither.map(|p| p.resolve_for_algorithm(algo.as_deref())).unwrap_or_default();
        let tuning = display::resolve_effective_tuning(&i.over.tuning, &i.screen.tuning, &i.device_base.tuning, &panel_tuning);
        let hex = |c: &Vec<(u8, u8, u8)>| display::colors_to_hex_strings(c);
        let script_colors = i.screen.colors.as_ref().map(hex);
        let script_actual = i.screen.colors_actual.as_ref().map(hex);
        let dc_colors = i.device_base.colors.as_ref().map(|c| hex(c).join(","));
        let panel_colors = i.panel.colors.as_ref().map(|c| hex(c).join(","));
        let candidates = vec![
            (display::SRC_DEV_OVERRIDE, i.over.colors_actual.clone()),
            (display::SRC_PANEL_ACTUAL, i.panel.colors_actual.clone()),
        ];
        let p = display::resolve_render_params(
            script_colors.as_deref(), script_actual.as_deref(), script_dither,
            dc_colors.as_deref(), device_dither, panel_colors.as_deref(),
            &i.fallback_palette, &candidates, &tuning, &mut Vec::new(),
        );
        let t = DitherTuningValues {
            max_error: p.max_error, noise_scale: p.noise_scale, chroma_clamp: p.chroma_clamp,
            strength: p.strength, gamut: p.gamut, ..Default::default()
        };
        (p.palette, p.measured_colors, p.dither, t)
    }

    /// The new resolver must produce exactly what the legacy one produced, for
    /// every combination below. This is what licenses moving the dev render
    /// onto the ladder; a failure here is a behaviour change, not a test to
    /// loosen.
    #[test]
    fn resolve_dev_render_matches_the_legacy_resolver() {
        let mut algorithms = HashMap::new();
        algorithms.insert("floyd-steinberg".to_string(), DitherTuningValues { strength: Some(0.3), max_error: Some(0.6), ..Default::default() });
        let pdc = PanelDitherConfig { defaults: DitherTuningValues { strength: Some(0.9), ..Default::default() }, algorithms };
        let dithers = [None, Some("atkinson"), Some("floyd-steinberg"), Some("Floyd_Steinberg")];
        let palettes = [None, Some(grey4()), Some(col6())];
        let actuals = [None, Some(act4()), Some(act6())];
        let strengths = [None, Some(0.1f32)];
        let mut cases = 0;
        for pd in [None, Some(&pdc)] {
            for od in dithers { for sd in dithers { for bd in dithers {
                for os in strengths { for ss in strengths { for bs in strengths {
                    let mut i = inputs();
                    i.panel_dither = pd;
                    i.over.dither = od.map(str::to_string);
                    i.screen.dither = sd.map(str::to_string);
                    i.device_base.dither = bd.map(str::to_string);
                    i.over.tuning.strength = os;
                    i.screen.tuning.strength = ss;
                    i.device_base.tuning.strength = bs;
                    i.screen.tuning.max_error = Some(0.4);
                    let n = resolve_dev_render(&i);
                    assert_eq!((n.palette, n.measured_colors, n.dither, n.tuning), legacy(&i), "{od:?} {sd:?} {bd:?} {os:?} {ss:?} {bs:?} panel={}", pd.is_some());
                    cases += 1;
                }}}
            }}}
        }
        for sc in palettes.clone() { for bc in palettes.clone() { for pc in palettes.clone() {
            for sa in actuals.clone() { for oa in actuals.clone() { for pa in actuals.clone() {
                let mut i = inputs();
                i.screen.colors = sc.clone();
                i.device_base.colors = bc.clone();
                i.panel.colors = pc.clone();
                i.screen.colors_actual = sa.clone();
                i.over.colors_actual = oa.clone();
                i.panel.colors_actual = pa.clone();
                let n = resolve_dev_render(&i);
                assert_eq!((n.palette, n.measured_colors, n.dither, n.tuning), legacy(&i), "{sc:?} {bc:?} {pc:?} {sa:?} {oa:?} {pa:?}");
                cases += 1;
            }}}
        }}}
        assert!(cases > 2000, "the matrix shrank to {cases} cases");
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib -q -- api::resolution models::config`
Expected: compile errors — `CascadeValues`, `resolve_dev_render`, … not found.

- [ ] **Step 3: Implement `CascadeValues` and `DeviceConfig::base()`**

In `src/models/config.rs`, after `DitherTuningValues`'s impl, add the struct with the doc comment from spec §3 ("The settings that take part in the resolution cascade. …"), the derives listed under Interfaces, and on `impl DeviceConfig`:

```rust
    /// The device's base rung as a `CascadeValues`. `colors_actual` is
    /// always `None`: a device-level measurement arrives with the cascade
    /// (spec §8 piece 3).
    pub fn base(&self) -> CascadeValues {
        CascadeValues {
            colors: self.colors.clone(),
            colors_actual: None,
            dither: self.dither.clone(),
            max_error: self.max_error,
            noise_scale: self.noise_scale,
            chroma_clamp: self.chroma_clamp,
            strength: self.strength,
            refresh: self.refresh,
            gamut: self.gamut.clone(),
        }
    }
```

Re-export `CascadeValues` from `crate::models` next to `DitherTuningValues` (check `src/models/mod.rs` for the existing `pub use config::{…}` line).

- [ ] **Step 4: Implement `RungSettings` and `resolve_dev_render`**

In `src/api/resolution.rs`:

```rust
use crate::api::display;
use crate::models::{normalize_algorithm_name, CascadeValues, DitherTuningValues, PanelDitherConfig};

// Today's precedence, per setting, highest first. Piece 3 of the cascade spec
// replaces all five with one order; until then each mirrors the code it took
// over from, named in the comment.

/// `resolve_render_params`: script > device > panel > fallback.
pub const ORDER_COLORS: &[Rung] = &[Rung::Override, Rung::DeviceFinal, Rung::Screen, Rung::DeviceBase, Rung::Panel, Rung::Fallback];
/// `resolve_render_params` puts the script's candidate before the dev override.
pub const ORDER_COLORS_ACTUAL: &[Rung] = &[Rung::Screen, Rung::Override, Rung::DeviceFinal, Rung::DeviceBase, Rung::Panel, Rung::Fallback];
/// `display::effective_dither`: the device outranks the screen (spec §1's inversion).
pub const ORDER_DITHER: &[Rung] = &[Rung::Override, Rung::DeviceFinal, Rung::DeviceBase, Rung::Screen, Rung::Panel, Rung::Fallback];
/// `display::resolve_effective_tuning`.
pub const ORDER_TUNING: &[Rung] = &[Rung::Override, Rung::DeviceFinal, Rung::Screen, Rung::DeviceBase, Rung::Panel, Rung::Fallback];
/// `content_pipeline::resolve_refresh_rate`: the script beats the device.
pub const ORDER_REFRESH: &[Rung] = &[Rung::Override, Rung::DeviceFinal, Rung::Screen, Rung::DeviceBase, Rung::Fallback];

/// One rung's settings, parsed. Colours are RGB because each source parses
/// its own way (a script returns a list, config holds a comma string) and the
/// resolver must compare like with like.
#[derive(Debug, Clone, Default)]
pub struct RungSettings {
    pub colors: Option<Vec<(u8, u8, u8)>>,
    pub colors_actual: Option<Vec<(u8, u8, u8)>>,
    pub dither: Option<String>,
    pub tuning: DitherTuningValues,
    pub refresh: Option<u32>,
}

impl RungSettings {
    pub fn from_cascade(v: &CascadeValues) -> Self {
        Self {
            colors: v.colors.as_deref().map(display::parse_colors_header),
            colors_actual: v.colors_actual.as_deref().map(display::parse_colors_header),
            dither: v.dither.clone(),
            tuning: DitherTuningValues {
                deprecated_error_clamp: None,
                max_error: v.max_error,
                noise_scale: v.noise_scale,
                chroma_clamp: v.chroma_clamp,
                strength: v.strength,
                gamut: v.gamut.clone(),
            },
            refresh: v.refresh,
        }
    }
}

pub struct DevRenderInputs<'a> {
    pub over: RungSettings,
    pub screen: RungSettings,
    pub device_base: RungSettings,
    /// Colours only; the panel's tuning is per algorithm and comes from
    /// `panel_dither` once the algorithm is known (spec §4.2).
    pub panel: RungSettings,
    pub panel_dither: Option<&'a PanelDitherConfig>,
    /// The model's default palette — the dev render's stand-in for the
    /// firmware `Colors` header.
    pub fallback_palette: Vec<(u8, u8, u8)>,
    /// The screen's `meta.yaml` `refresh`.
    pub screen_default_refresh: u32,
}

pub struct DevRenderResolution {
    pub palette: Vec<(u8, u8, u8)>,
    pub measured_colors: Option<Vec<(u8, u8, u8)>>,
    /// Canonical name, or `None` for the engine default.
    pub dither: Option<String>,
    /// Configured winners only — never fallbacks (plan ruling 7).
    pub tuning: DitherTuningValues,
    pub refresh: u32,
    pub report: Resolution,
}

fn ladder_of<T: Clone>(i: &DevRenderInputs, pick: impl Fn(&RungSettings) -> Option<T>, panel: Option<T>, fallback: Option<T>) -> Ladder<T> {
    Ladder {
        over: pick(&i.over),
        device_final: None,
        screen: pick(&i.screen),
        device_base: pick(&i.device_base),
        panel,
        fallback,
    }
}

fn hex_json(c: &Vec<(u8, u8, u8)>) -> serde_json::Value {
    serde_json::json!(display::colors_to_hex_strings(c).join(","))
}

pub fn resolve_dev_render(i: &DevRenderInputs) -> DevRenderResolution {
    let mut report = Resolution::new();

    // Colours first: the measured colours' validity depends on the palette.
    let colors = resolve(
        &ladder_of(i, |r| r.colors.clone(), i.panel.colors.clone(), Some(i.fallback_palette.clone())),
        ORDER_COLORS,
        accept_any,
    );
    report.insert("colors", colors.report(hex_json));
    let palette = colors.effective.clone().unwrap_or_else(|| i.fallback_palette.clone());

    let palette_len = palette.len();
    let colors_actual = resolve(
        &ladder_of(i, |r| r.colors_actual.clone(), i.panel.colors_actual.clone(), None),
        ORDER_COLORS_ACTUAL,
        |c: &Vec<(u8, u8, u8)>| {
            if c.len() == palette_len {
                Ok(())
            } else {
                Err(format!(
                    "has {} usable entries but the resolved palette has {palette_len}",
                    c.len()
                ))
            }
        },
    );
    report.insert("colors_actual", colors_actual.report(hex_json));

    // The algorithm before its numbers (spec §4.2). Compared canonically, so
    // `Floyd_Steinberg` and `floyd-steinberg` are one value.
    let dither = resolve(
        &ladder_of(
            i,
            |r| r.dither.as_deref().map(normalize_algorithm_name),
            None,
            Some("atkinson".to_string()),
        ),
        ORDER_DITHER,
        accept_any,
    );
    report.insert("dither", dither.report(|d| serde_json::json!(d)));
    let algo = dither.effective.clone().unwrap_or_else(|| "atkinson".to_string());
    let panel_tuning = i
        .panel_dither
        .map(|p| p.resolve_for_algorithm(Some(&algo)))
        .unwrap_or_default();
    let (default_max_error, default_noise_scale) =
        crate::models::config::dither_algorithm_for_name(&algo).defaults();
    let engine = eink_dither::DitherOptions::default();
    let gamut_defaults = eink_dither::GamutOptions::default();

    let mut tuning = DitherTuningValues::default();
    macro_rules! number {
        ($key:literal, $($field:ident).+, $fallback:expr) => {{
            let r = resolve(
                &ladder_of(i, |s| s.tuning.$($field).+, panel_tuning.$($field).+, $fallback),
                ORDER_TUNING,
                accept_any,
            );
            report.insert($key, r.report(|v| serde_json::json!(v)));
            tuning.$($field).+ = r.configured().copied();
        }};
    }
    number!("max_error", max_error, Some(default_max_error));
    number!("noise_scale", noise_scale, Some(default_noise_scale));
    // The engine's default is "no damping" (infinity), which JSON cannot
    // carry; it is reported as no fallback value at all.
    number!("chroma_clamp", chroma_clamp, None);
    number!("strength", strength, Some(engine.strength));
    number!("gamut.knee", gamut.knee, Some(gamut_defaults.knee));
    number!("gamut.amount", gamut.amount, Some(gamut_defaults.amount));
    number!("gamut.max_compression", gamut.max_compression, Some(gamut_defaults.max_compression));

    let refresh = resolve(
        &ladder_of(i, |r| r.refresh, None, Some(i.screen_default_refresh)),
        ORDER_REFRESH,
        accept_any,
    );
    report.insert("refresh", refresh.report(|v| serde_json::json!(v)));

    DevRenderResolution {
        palette,
        measured_colors: colors_actual.effective.clone(),
        dither: dither.configured().cloned(),
        tuning,
        refresh: refresh.effective.unwrap_or(i.screen_default_refresh),
        report,
    }
}
```

Check before relying on them: that `eink_dither::DitherOptions` and `eink_dither::GamutOptions` are exported at the crate root (`crates/eink-dither/src/lib.rs`); if `DitherOptions` is not, use the literal `1.0` with a comment naming `DitherOptions::default()` in `crates/eink-dither/src/dither/options.rs`. Check whether `PanelDitherConfig` and `normalize_algorithm_name` are re-exported from `crate::models`; import from `crate::models::config` if not.

`resolve_render_params` returns the palette's measured colours with entries **already parsed**; `resolve_dev_render` must use the same parsed values, so the equivalence test compares like for like.

- [ ] **Step 5: Run the tests**

Run: `cargo test --lib -q -- api::resolution models::config`
Expected: all pass, including `resolve_dev_render_matches_the_legacy_resolver`. If the matrix fails, the new resolver is wrong — fix it, never the matrix. The one known pitfall: when no rung sets `dither`, legacy returns `dither: None` and so must the new one (`configured()`, not `effective`).

- [ ] **Step 6: Commit**

```bash
git add src/api/resolution.rs src/models/config.rs src/models/mod.rs
git diff --cached --stat
git commit -m "feat(resolution): resolve the dev render on the ladder, pinned to the legacy resolver

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `POST /dev/api/render` with the provenance header

**Files:**
- Modify: `src/api/dev.rs` (`RenderQuery` → `RenderRequest`, `handle_render`, `dev_router`), `src/services/content_pipeline.rs` (`ScriptResult`)
- Modify: `tests/dev_render_is_read_only_test.rs`
- Create: `tests/dev_render_provenance_test.rs`
- Modify: `docs/src/guide/dev-mode.md`, `docs/src/api/lua-api.md`

**Interfaces:**
- Consumes: Task 2's `CascadeValues`, `RungSettings`, `DevRenderInputs`, `resolve_dev_render`, `RESOLUTION_HEADER`, `header_value`.
- Produces (the wire contract Task 4 and the UI plan rely on):

```json
{
  "screen": "local/hello",          // optional when device is given
  "device": "kitchen",              // optional when screen is given; MAC, config key or registration code
  "panel": "trmnl_x",               // optional; beats the device's panel
  "model": "og",                    // optional, default "og"
  "width": 800, "height": 480,      // optional
  "draft": {                        // optional; each present rung REPLACES the configured one
    "base":  { /* CascadeValues */ },
    "panel": { "colors": "…", "colors_actual": "…", "dither": { /* PanelDitherConfig */ } }
  },
  "override": { /* CascadeValues */ },
  "simulated": { "battery_voltage": 4.2, "rssi": -50, "timestamp": 1700000000, "params": {} },
  "view": { "use_actual": true }
}
```

Answer: `200 image/png` with `X-Byonk-Resolution: <header_value>`; errors keep today's `RenderErrorResponse` JSON with `400` (bad request, refused field), `404` (unknown device), `500` (script/render).

`ScriptResult` gains `pub script_refresh_rate: Option<u32>` (the script's own `refresh_rate` when `> 0`) and `pub screen_default_refresh: u32` (`meta.refresh.unwrap_or(900)`), set next to the existing `resolve_refresh_rate` call. Every `ScriptResult { … }` literal in the tree must set them — find them with `grep -rn "ScriptResult {" src tests`.

- [ ] **Step 1: Port the read-only tests to the POST**

In `tests/dev_render_is_read_only_test.rs`, change the module doc's first line to `` `POST /dev/api/render` is a preview. … `` and replace each `get_with_headers("/dev/api/render?…", &[AUTH])` with the POST equivalent. The two device tests become:

```rust
    let preview = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"device":"kitchen","width":800,"height":480,"override":{"dither":"floyd-steinberg"}}"#,
        )
        .await;
```

```rust
    let preview = app
        .post_json(
            "/dev/api/render",
            &[AUTH],
            r#"{"device":"kitchen","width":800,"height":480,"override":{"strength":0.2,"max_error":0.1}}"#,
        )
        .await;
```

Port the third test the same way: each former query parameter goes where the table in Step 5 says. Keep every assertion and comment that is not about the transport.

- [ ] **Step 2: Write the new integration tests**

Create `tests/dev_render_provenance_test.rs`. Reuse the fixture shape of `dev_render_is_read_only_test.rs` (copy `scaffold_gradient_screen` and adapt): a second screen `local/plain` whose script returns `dither = "atkinson"` and `refresh_rate = 60`, and a device `kitchen` (panel `e1004`, `dither: floyd-steinberg`, `refresh: 600`, `params: { greeting: hi }`) whose screen is `local/gradient`.

```rust
mod common;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

fn resolution(r: &common::TestResponse) -> serde_json::Value {
    let raw = r
        .headers
        .get("x-byonk-resolution")
        .expect("a dev render must carry X-Byonk-Resolution")
        .to_str()
        .expect("the header must be visible ASCII");
    serde_json::from_str(raw).unwrap()
}

#[tokio::test]
async fn the_header_reports_every_setting_with_every_rung() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app.post_json("/dev/api/render", &[AUTH], r#"{"screen":"local/plain"}"#).await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    assert!(r.is_png());
    let res = resolution(&r);
    for key in ["colors", "colors_actual", "dither", "max_error", "noise_scale", "chroma_clamp", "strength", "gamut.knee", "gamut.amount", "gamut.max_compression", "refresh"] {
        let s = &res[key];
        assert!(s.is_object(), "missing {key}");
        for rung in ["override", "device_final", "screen", "device_base", "panel", "fallback"] {
            assert!(s["rungs"][rung]["status"].is_string(), "{key}.{rung}");
        }
    }
    assert_eq!(res["dither"]["from"], "screen");
    assert_eq!(res["refresh"]["effective"], 60);
}

#[tokio::test]
async fn screen_and_device_together_take_the_screen_and_the_devices_settings() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json("/dev/api/render", &[AUTH], r#"{"screen":"local/plain","device":"kitchen"}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res = resolution(&r);
    // Today the device outranks the screen for dither (plan ruling 1).
    assert_eq!(res["dither"]["effective"], "floyd-steinberg");
    assert_eq!(res["dither"]["from"], "device_base");
    assert_eq!(res["dither"]["rungs"]["screen"]["value"], "atkinson");
    assert_eq!(res["dither"]["rungs"]["screen"]["status"], "outranked");
    // The screen still wins refresh; the device's value is outranked.
    assert_eq!(res["refresh"]["effective"], 60);
    assert_eq!(res["refresh"]["rungs"]["device_base"]["value"], 600);
    // The device has no `colors`; its panel's palette wins.
    assert_eq!(res["colors"]["from"], "panel");
}

#[tokio::test]
async fn device_alone_renders_its_own_screen() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app.post_json("/dev/api/render", &[AUTH], r#"{"device":"kitchen"}"#).await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    // local/gradient returns no dither and refresh_rate 3600.
    assert_eq!(resolution(&r)["refresh"]["effective"], 3600);
}

#[tokio::test]
async fn a_draft_base_replaces_the_configured_device_rung() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    // The draft has no dither: replacing the rung REMOVES the device's
    // floyd-steinberg, so the screen's atkinson wins.
    let r = app
        .post_json("/dev/api/render", &[AUTH], r#"{"screen":"local/plain","device":"kitchen","draft":{"base":{"strength":0.5}}}"#)
        .await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let res = resolution(&r);
    assert_eq!(res["dither"]["from"], "screen");
    assert_eq!(res["strength"]["rungs"]["device_base"]["value"], 0.5);
}

#[tokio::test]
async fn a_draft_panel_with_a_wrong_length_calibration_is_rejected_in_the_report() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app
        .post_json("/dev/api/render", &[AUTH], r##"{"screen":"local/plain","panel":"e1004","draft":{"panel":{"colors":"#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00","colors_actual":"#101010,#E8E8E8"}}}"##)
        .await;
    assert!(r.status.is_success(), "a calibration mistake must not fail the render");
    let res = resolution(&r);
    assert_eq!(res["colors_actual"]["rungs"]["panel"]["status"], "rejected");
    assert!(res["colors_actual"]["rungs"]["panel"]["reason"].as_str().unwrap().contains("has 2 usable entries"));
}

#[tokio::test]
async fn fields_the_config_cannot_hold_yet_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    for body in [
        r#"{"screen":"local/plain","draft":{"final":{"dither":"atkinson"}}}"#,
        r##"{"screen":"local/plain","device":"kitchen","draft":{"base":{"colors_actual":"#000000"}}}"##,
        r##"{"screen":"local/plain","override":{"colors":"#000000,#FFFFFF"}}"##,
        r#"{"screen":"local/plain","override":{"refresh":60}}"#,
        r#"{"screen":"local/plain","dither":"atkinson"}"#,
    ] {
        let r = app.post_json("/dev/api/render", &[AUTH], body).await;
        assert!(r.status.is_client_error(), "{body} → {}", r.status);
    }
}

#[tokio::test]
async fn neither_screen_nor_device_is_a_bad_request() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app.post_json("/dev/api/render", &[AUTH], "{}").await;
    assert_eq!(r.status, axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_get_render_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let r = app.get_with_headers("/dev/api/render?screen=local/plain", &[AUTH]).await;
    assert_eq!(r.status, axum::http::StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn the_header_stays_inside_its_budget_for_sixteen_entry_palettes() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app(dir.path());
    let grey16: Vec<String> = (0..16).map(|i| { let v = i * 17; format!("#{v:02X}{v:02X}{v:02X}") }).collect();
    let g = grey16.join(",");
    let body = format!(
        r#"{{"screen":"local/plain","device":"kitchen","draft":{{"base":{{"colors":"{g}"}},"panel":{{"colors":"{g}","colors_actual":"{g}"}}}},"override":{{"colors_actual":"{g}"}}}}"#
    );
    let r = app.post_json("/dev/api/render", &[AUTH], &body).await;
    assert!(r.status.is_success(), "{} {}", r.status, r.text());
    let len = r.headers.get("x-byonk-resolution").unwrap().len();
    assert!(len < 4096, "X-Byonk-Resolution is {len} bytes");
}
```

The `app()` helper builds the config like `dev_render_is_read_only_test.rs` does, with the `kitchen` device described above.

Also add one test that the image and the report agree on the algorithm: render the gradient screen as `kitchen` once with `{"device":"kitchen"}` and once with `{"device":"kitchen","override":{"dither":"floyd-steinberg"}}`; the header reports `floyd-steinberg` for both and the two PNG bodies are byte-identical. Then once with `"override":{"dither":"atkinson"}`: the header reports `atkinson` and the PNG differs.

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -q --test dev_render_provenance_test --test dev_render_is_read_only_test`
Expected: FAIL — `405 Method Not Allowed` on every POST.

- [ ] **Step 4: Add the raw refresh inputs to `ScriptResult`**

In `src/services/content_pipeline.rs`, add the two fields to `ScriptResult` with doc comments ("the script's own `refresh_rate`, when it set one (`> 0`) — the screen rung of the refresh ladder" / "the screen's `meta.yaml` `refresh`, or 900 — the refresh fallback") and set them where `resolve_refresh_rate` is called:

```rust
            script_refresh_rate: (lua_result.refresh_rate > 0).then_some(lua_result.refresh_rate),
            screen_default_refresh: screen_default,
```

Set them in every other `ScriptResult { … }` literal too (tests: `None` and `900`).

- [ ] **Step 5: Replace `RenderQuery` with `RenderRequest`**

In `src/api/dev.rs`:

```rust
/// Body of `POST /dev/api/render`. See `docs/src/guide/dev-mode.md`
/// ("Rendering a preview") for the contract.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    /// Screen repo ref. With `device`, this screen is rendered with the
    /// device's settings; without it, the device's own screen is rendered.
    pub screen: Option<String>,
    /// MAC, config key, or registration code.
    pub device: Option<String>,
    /// Panel profile id; beats the device's own `panel`.
    pub panel: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub draft: RenderDraft,
    #[serde(default, rename = "override")]
    pub over: CascadeValues,
    #[serde(default)]
    pub simulated: SimulatedInputs,
    #[serde(default)]
    pub view: ViewOptions,
}

/// An uncommitted edit. Each present rung REPLACES the configured one, so a
/// preview can show the effect of removing a value (plan ruling 2).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderDraft {
    pub base: Option<CascadeValues>,
    pub panel: Option<PanelDraft>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDraft {
    pub colors: Option<String>,
    pub colors_actual: Option<String>,
    pub dither: Option<crate::models::PanelDitherConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatedInputs {
    pub battery_voltage: Option<f32>,
    pub rssi: Option<i32>,
    pub timestamp: Option<i64>,
    /// Merged over the device's configured params.
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewOptions {
    pub use_actual: Option<bool>,
}

/// Fields the cascade cannot carry until piece 3 (plan ruling 4).
fn refused_until_cascade(req: &RenderRequest) -> Option<&'static str> {
    if req.draft.base.as_ref().is_some_and(|b| b.colors_actual.is_some()) {
        return Some("draft.base.colors_actual: a device-level colors_actual is not supported yet");
    }
    if req.over.colors.is_some() {
        return Some("override.colors is not supported: the script could not see it");
    }
    if req.over.refresh.is_some() {
        return Some("override.refresh is not supported");
    }
    None
}
```

`PanelDitherConfig` must deserialize from JSON. Check its `Deserialize` impl in `src/models/config.rs`; it is written against a generic deserializer, so JSON of the YAML shape (`{"max_error": 1.0, "floyd-steinberg": {"noise_scale": 0.6}}`) works — add a unit test in `config.rs` that proves it (`serde_json::from_str::<PanelDitherConfig>(…)`), since the draft depends on it.

`deny_unknown_fields` makes `draft.final` and every old query name sent as a top-level key (`"dither"`, `"mac"`, …) a `400`/`422`. axum's `Json` extractor answers a body that does not match the type with `422 Unprocessable Entity`; the refusal test accepts any `4xx`. Keep it that way — do not add a custom rejection.

- [ ] **Step 6: Rewrite `handle_render`**

Signature: `pub async fn handle_render(State(state): State<AppState>, Json(req): Json<RenderRequest>) -> Response`.

The body, in order — keep every existing comment that still applies:

1. `refused_until_cascade` → `400` with `RenderErrorResponse { error, details: None }`.
2. **Device and screen.** If `req.device` is set, look it up as today (`get_device_config(..).or_else(|| get_device_config_for_code(..))`, `404` when absent). `screen_ref = req.screen.clone().unwrap_or(device.screen)`; params start from the device's params. If only `req.screen` is set, no device. Neither → `400` "Either 'screen' or 'device' is required".
3. **The rungs, draft-aware.** `device_rung: CascadeValues = req.draft.base.clone().unwrap_or_else(|| device.map(|d| d.base()).unwrap_or_default())`. Panel: `let panel_cfg = req.panel.as_deref().or(device.panel).and_then(|n| config.get_panel(n));` Then the panel rung's colours and dither come from `req.draft.panel` when present (all three fields, `None` meaning absent), else from `panel_cfg`. Width/height stay geometry from `panel_cfg` as today (`resolve_preview_dimensions`).
4. **Pre-script context — unchanged logic**, fed from the rungs: `device_config_colors = device_rung.colors`, `device_config_dither = device_rung.dither`, the `dc_*` numbers and `dc_gamut` from `device_rung`, `panel_colors` and `panel.colors_actual` and the panel dither config from the panel rung. The dev-override slots read `req.over.dither` and `req.over.colors_actual` where they read `query.dither` and `query.colors_actual`. `params`: device params extended by `req.simulated.params`. `battery_voltage`, `rssi`, `timestamp` from `req.simulated`. `DeviceContext.mac` is `req.device` or `"dev-simulator"`.
5. **Run the script** exactly as today, additionally returning `script_result.script_refresh_rate` and `script_result.screen_default_refresh` from the blocking closure.
6. **Post-script: `resolve_dev_render`.** Build `DevRenderInputs` — `over: RungSettings::from_cascade(&req.over)`, `device_base: RungSettings::from_cascade(&device_rung)`, `screen` from the script result (`colors` via `display::parse_colors_header(&v.join(","))`, `colors_actual` via `display::parse_measured_color_list`, the five tuning numbers, `gamut.unwrap_or_default()`, `refresh: script_refresh_rate`), `panel` with only `colors`/`colors_actual` parsed with `parse_colors_header`, `panel_dither`, `fallback_palette: query_palette` (`resolve_query_palette(&req.model, None)`), `screen_default_refresh`. Delete the old `effective_*_dither` / `resolve_effective_tuning` / `resolve_render_params` block. Feed the renderer from the result: `final_palette = res.palette`, `measured_colors = res.measured_colors`, `final_dither = res.dither`, and the `DitherTuning` from `res.tuning` built exactly as `display::resolve_dither_tuning` builds it from `RenderParams` (construct a `RenderParams` from `res` and call it, or factor a `resolve_dither_tuning_from(&DitherTuningValues)` used by both — do not duplicate the `has_tuning` rule).
7. The `colors_actual` mismatch warning: log each `Rejected` reason with `tracing::warn!` as the old path did, so the server log is unchanged.
8. Answer the PNG with `(HeaderName::from_static(RESOLUTION_HEADER), header_value(&res.report))` added to the existing headers.

Route: `.route("/render", post(handle_render))`.

- [ ] **Step 7: Run the tests**

Run: `cargo test -q --test dev_render_provenance_test --test dev_render_is_read_only_test && cargo test --lib -q -- api::`
Expected: all pass.

- [ ] **Step 8: Document the endpoint**

`docs/src/guide/dev-mode.md`: in the endpoint table replace the `GET /dev/api/render` row with `` | `POST /dev/api/render` | Render a screen to PNG under simulated device context; the answer carries `X-Byonk-Resolution` (see below) | ``. Add a section after the table, `### Rendering a preview`, with: the request body from this task's Interfaces (as a JSON block, without the `//` comments — describe each field in a table instead), the draft-replaces-its-rung rule, the refused fields and why, and the response header with a short real example (one setting) and the meaning of each status. State that the per-setting `order` is today's precedence and that `dither` puts the device above the screen.

`docs/src/api/lua-api.md` around line 1330: the paragraph saying `/dev/api/render` "answers with the PNG and nothing else" is now false for this case. Replace the two sentences with: on `/dev/api/render` the warning goes to the server's `tracing` output, and the rejected value and its reason are reported in the response's `X-Byonk-Resolution` header (the dev UI does not display it yet). Leave the rest of the section alone.

Run: `(cd docs && mdbook build)` — expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/api/dev.rs src/services/content_pipeline.rs tests/dev_render_is_read_only_test.rs tests/dev_render_provenance_test.rs docs/src/guide/dev-mode.md docs/src/api/lua-api.md src/models/config.rs
git diff --cached --stat
git commit -m "feat(dev): POST /dev/api/render with screen+device, draft rungs and X-Byonk-Resolution

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

(Add any other file that held a `ScriptResult` literal.)

---

### Task 4: The SPA renders through the POST

**Files:**
- Modify: `webui/src/lib/api.js`, `webui/src/lib/state/simulate.svelte.js`, `webui/src/lib/components/PreviewPane.svelte`, `webui/src/lib/state/tune.svelte.js` (comment only)
- Modify: `webui/test/api.test.js`, `webui/test/simulate.test.js`, `webui/test/preview.test.js`, and every test that stubs the render fetch (`grep -rln "api/render\|simulate.url" webui/test`)

**Interfaces:**
- Consumes: Task 3's wire contract.
- Produces:
  - `api.renderPreview(body) → Promise<Response>` — `fetch("api/render", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) })`, relative URL (Ingress), returns the raw `Response` so the caller reads the blob and, in the UI plan, the header.
  - `simulate.request(ref) → object` — replaces `simulate.url(ref)`; same rules (a)–(f), same throw on invalid params JSON.

Mapping, old query → body (behaviour must not change):

| old query | body |
|---|---|
| `screen` | `screen` |
| `mac` | `device` |
| `width`, `height` | `width`, `height` |
| `battery_voltage`, `rssi`, `timestamp` | `simulated.*` |
| `params` (JSON **string**) | `simulated.params` (the parsed **object**) |
| `panel` | `panel` |
| `use_actual` (`"true"`/`"false"`) | `view.use_actual` (boolean) |
| `dither`, `max_error`, `noise_scale`, `chroma_clamp`, `strength`, `colors_actual` | `override.*` |

Empty values are omitted, as `qs()` omitted them; `deny_unknown_fields` on the server makes a typo a `4xx`, which the preview surfaces through the existing `!res.ok` path.

- [ ] **Step 1: Rewrite the tests to the new contract**

`webui/test/simulate.test.js`: every `const u = simulate.url("local/hello")` + query-string assertion becomes `const b = simulate.request("local/hello")` + an assertion on the object, per the mapping table. Example of the shape:

```js
const b = simulate.request("local/hello");
expect(b.screen).toBe("local/hello");
expect(b.width).toBe(800);
expect(b.simulated.params).toEqual({});
expect(b.override?.dither).toBeUndefined();
```

Each existing test keeps its intent (rules (a)–(f)); only the observation changes. Keep the test that `request()` throws on invalid params JSON.

`webui/test/api.test.js`: replace the `renderUrl` string-builder entries with a `renderPreview` entry asserting `expectedUrl: "api/render"`, `expectedMethod: "POST"`, and that the body is the JSON of the argument; delete the test at the old `renderUrl(…)` line 256 or convert it to the same object check.

`webui/test/preview.test.js` and the journey tests: where a `fetch` mock parsed `new URL(url…).searchParams.get("screen")`, read `JSON.parse(init.body).screen` instead (`fetch` mocks receive `(url, init)`). Add one assertion that the request is a `POST` to a relative `api/render`.

- [ ] **Step 2: Run them to see them fail**

Run: `cd webui && pnpm exec vitest run`
Expected: FAIL — `simulate.request is not a function`, `renderPreview` missing.

- [ ] **Step 3: Implement**

`api.js`: delete `renderUrl`; add

```js
/** POST a preview render (see docs/src/guide/dev-mode.md, "Rendering a preview"). Relative, for HA Ingress. */
export const renderPreview = (body) =>
  fetch("api/render", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
```

`simulate.svelte.js`: rename `url(ref)` to `request(ref)` and build the object per the mapping table, keeping each rule's comment; omit `undefined`/`""`/`null` fields; `simulated.params` is the parsed object. Update the doc comments that say `simulate.url()` (also in `PreviewPane.svelte` and `tune.svelte.js:218`).

`PreviewPane.svelte` `renderNow()`: `const res = await api.renderPreview(simulate.request(ref));` in place of `fetch(simulate.url(ref))`; everything after it unchanged. Import `* as api from "../api.js"` if not imported.

- [ ] **Step 4: Run the tests**

Run: `make webui && make webui-test`
Expected: 28 files, all pass (counts may change with rewritten tests; zero failures).

- [ ] **Step 5: Commit**

```bash
git add webui/src/lib/api.js webui/src/lib/state/simulate.svelte.js webui/src/lib/components/PreviewPane.svelte webui/src/lib/state/tune.svelte.js webui/test/
git diff --cached --stat
git commit -m "feat(webui): render previews through POST /dev/api/render

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Whole-piece verification

- [ ] **Step 1:** `make webui`
- [ ] **Step 2:** `cargo fmt --check` — expected: no output.
- [ ] **Step 3:** `cargo clippy --workspace --all-targets -- -D warnings` — expected: clean.
- [ ] **Step 4:** `cargo test --workspace` — expected: 0 failed. (Run alone.)
- [ ] **Step 5:** `make webui-test` — expected: 0 failed. (Run after Step 4 finishes.)
- [ ] **Step 6:** `(cd docs && mdbook build)` — expected: no warnings.
- [ ] **Step 7:** Confirm the embedded bundle is current: `make webui` ran in Step 1, before any `cargo` build in Steps 3–4 (RustEmbed bakes `webui/dist` in at compile time; an empty `dist` still compiles).
- [ ] **Step 8:** `git status --short` shows only the maintainer's four files.
