# resvg `byonk-base` Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move byonk off the frozen resvg `skrifa` fork branch onto `byonk-base` (resvg 0.48.1 + bitmap glyphs + font hinting + the two new resolver hooks), replace the removed `FaceInfo::bitmap_strikes` with a byonk-side skrifa computation, expose hinting to screens as a Lua directive with a server-side adaptive default, and then settle which variable-font trio backs the generic families.

**Architecture:** byonk's contact with resvg is small and lives almost entirely in `src/rendering/svg_to_png.rs`: it builds a `fontdb::Database`, hands it to `usvg::Options`, and renders a `tiny_skia::Pixmap` through `resvg::render`. The integration keeps that shape. Font behaviour becomes a single value — `FontConfig` — computed once per render and installed on `usvg::Options` as a `FontResolver`, so the main render and the tone mask are guaranteed to resolve fonts identically. Bitmap strike sizes are recomputed from the font bytes fontdb already holds, via `Database::with_face_data` + skrifa, which is what lets the fontdb pin disappear.

**Tech Stack:** Rust (edition 2021, stable toolchain), `resvg`/`usvg` 0.48.1 from `github.com/oetiker/resvg` branch `byonk-base`, `tiny-skia` 0.12, `fontdb` 0.24 (crates.io), `skrifa` 0.44, `mlua` 0.10 (Lua 5.4), `tera` for SVG templating.

---

## Global Constraints

These bind every task. They are not repeated per task.

- **Branch:** all work lands on `feat/screen-store-authoring-core` (PR #30). Do not open a new branch.
- **Never `git add -A` or `git add .`** — `/Users/oetiker/checkouts/byonk/examples/` is an untracked near-copy of `screens/examples/` and gets swept in. Add by explicit path and verify `git diff --cached` before every commit.
- **`make check` takes ~10 minutes — always run it with `run_in_background: true`.** Foreground `sleep` is blocked, and a subagent running it in the foreground dies to the 600 s stream watchdog. Subagents use `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` plus a targeted `cargo test`.
- **`make check` runs `cargo fmt`, not `cargo fmt --check`** — it rewrites files in place. Re-inspect `git status` after it runs.
- **Cap parallelism at 2:** prefix cargo invocations with `CARGO_BUILD_JOBS=2`. Shared machine.
- **`cargo test` takes only one filter argument.** Two filters silently ignore the second.
- **Pre-existing `#[ignore]`d failures, unrelated to this work — do not chase them:** `preprocess::preprocessor::tests::{test_process_with_resize, test_resize_before_enhancement, test_resize_full_pipeline_with_photo_preset}`. They panic in `resize_lanczos` by design.
- **These two tests must pass unchanged** (`src/rendering/svg_to_png.rs`): `test_bitmap_strikes_exposed` and `test_bitmap_font_families`. They are the contract for the fontdb substitution. If a task needs to change them, stop and raise it.
- **`CHANGES.md` Unreleased entries are user-facing only.** Describe what a byonk operator or screen author sees change. No CI, tooling, refactor, or dependency-hygiene notes.
- **Pinned upstream versions** (audited 2026-08-15, all verified against the actual crates — do not re-derive):
  - `byonk-base` tip: `303e38e02350b5d88502f3b6ffb9918fcfd8fc35` ("Sort the test module declarations"). The spec cites `b67da7c0`; the branch has moved by one commit.
  - `resvg` / `usvg` `0.48.1`, edition 2024, MSRV **1.85.0**. Local toolchain is `stable` (1.97.1) — no change needed.
  - `tiny-skia` **0.12.0**. Its only breaking change from 0.11 is `RadialGradient::new` gaining a start-radius argument; byonk never constructs one.
  - `fontdb` **0.24** from crates.io, no longer patched. `FaceInfo::bitmap_strikes` does not exist there (verified: zero occurrences in `fontdb-0.24.0/src/lib.rs`).
  - `skrifa` **0.44** — same version usvg uses, so one copy in the tree.
  - `png` 0.17 and 0.18 **already coexist** in the tree today. Not a shared type across the resvg boundary. Leave both alone.
- **The only crates that cross the resvg API boundary are `tiny-skia` and `fontdb`.** `usvg` reaches byonk through `resvg::usvg`, so it is consistent by construction. `image`, `zune-jpeg`, `image-webp`, and `png` share no types with resvg.

---

## File Structure

| File | Responsibility after this plan |
|---|---|
| `Cargo.toml` | Dependency versions and the `[patch.crates-io]` pin. `fontdb` patch removed, `skrifa` added. |
| `src/rendering/svg_to_png.rs` | `SvgRenderer`. Owns the `fontdb::Database`, the per-face bitmap-strike map, generic-family mapping, and installs the `FontResolver`. Grew large; **do not split it in this plan** — a split would collide with PR #30's diff. |
| `src/rendering/font_config.rs` | **New.** `FontConfig` / `FontVariant` / `HintingSpec`: the resolved, renderer-facing font behaviour for one render, plus the adaptive default derived from the palette. Pure data + pure functions, no resvg types beyond `usvg`'s hinting enums. |
| `src/rendering/font_strikes.rs` | **New.** `bitmap_strikes_for(data: &[u8], index: u32) -> Vec<u16>` — the skrifa replacement for the removed fontdb field. One responsibility, trivially unit-testable against the bundled X11 fonts. |
| `src/services/lua_runtime.rs` | Parses the `font_hinting` key off a script's return table into `FontConfig`. |
| `src/services/content_pipeline.rs` | Builds the Lua `fonts` global from the renderer, and threads `FontConfig` from `ScriptResult` into the render call. |
| `byonk-base/v1/hinting.svg` | Becomes a no-op shim carrying only `shape-rendering: crispEdges`; the `-resvg-hinting-*` properties are deleted. |
| `tools/capture-renders.sh` | **New.** Renders every diffable bundled screen to a directory, for the three-state comparison. Plain shell over the existing `byonk render` CLI so it runs on any checkout, including pre-#30 ones. |
| `docs/src/reference/font-hinting.md` | **New.** The `font_hinting` directive and the upgrade notice for `v1/hinting.svg`. |

---

## Task ordering, and one deviation from the spec

The spec's sequence is followed, with one change: **the generic-font-family fix (Task 2) comes before the resvg bump.**

Reason: three tests are red on CI today because no bundled font resolves a generic family (`fontdb::Database::new()` defaults the generics to Arial / Times New Roman / Courier New, none of which byonk bundles; macOS masks it via `load_system_fonts()`). Doing the resvg bump against a red CI means new breakage cannot be told from old. The fix is independent of resvg — `byonk-base` does not address it — so it is safe to land first.

It does change rendered output on macOS (text that silently fell back to Arial now uses a bundled font), which is exactly why Task 1 captures the baselines *before* it.

---

### Task 1: A render-capture harness, and the two pre-integration baselines

byonk has **no assertions on rendered output at all**. resvg 0.48.0's changelog says plainly *"May result in small rendering changes compared to older versions"*, on top of three text-positioning fixes (#1043, #1040, #1056), against hand-tuned fixed-panel layouts. The only guard is capturing renders before and after and diffing them.

The capture is a **shell script over the existing `byonk render` CLI**, not Rust, because it has to run against pre-#30 checkouts where any new Rust code would not exist.

**Files:**
- Create: `tools/capture-renders.sh`
- Create: `tools/capture-config.yaml`

**Interfaces:**
- Produces: `tools/capture-renders.sh <output-dir>` — renders each diffable screen to `<output-dir>/<screen-name>.png` and writes `<output-dir>/MANIFEST.txt` listing screen name, panel, and exit status.

**Not every screen is diffable.** A pixel diff is only meaningful for a deterministic render. These screens are **excluded from the diff** because they fetch network data or display the current time, and must be assessed by eye instead — the script still renders them, into a `nondeterministic/` subdirectory, so they can be looked at:

`gphoto`, `webscrape`, `transit`, `swiss-departure-board`, `hello` (shows a clock), `byonk-builtin/default` (shows a clock).

Deterministic and diffable: `mandelbrot`, `demo/font/bitmap`, `demo/font/ttf`, `demo/font/hinting`, and the four calibration screens `byonk-builtin/calibration/{gamut,tone,grey,color}`.

- [ ] **Step 1: Write the capture config**

A device per screen, each with an explicit `panel:` — without one you silently get a greyscale render regardless of the panel's real palette. Create `tools/capture-config.yaml` by copying `config.yaml` and replacing its `devices:` block with exactly this (keep `panels:`, `screens:`, `registration:`, `auth_mode:` as they are):

```yaml
devices:
  DEFAULT:
    screen: byonk-builtin/default
  "AA:BB:CC:00:00:01":
    panel: trmnl_og
    screen: byonk-builtin/calibration/gamut
  "AA:BB:CC:00:00:02":
    panel: trmnl_og
    screen: byonk-builtin/calibration/tone
  "AA:BB:CC:00:00:03":
    panel: trmnl_og
    screen: byonk-builtin/calibration/grey
  "AA:BB:CC:00:00:04":
    panel: trmnl_og_4clr
    screen: byonk-builtin/calibration/color
  "AA:BB:CC:00:00:05":
    panel: trmnl_og
    screen: mandelbrot
  "AA:BB:CC:00:00:06":
    panel: trmnl_og
    screen: demo-font-bitmap
  "AA:BB:CC:00:00:07":
    panel: trmnl_og
    screen: demo-font-ttf
  "AA:BB:CC:00:00:08":
    panel: trmnl_og
    screen: demo-font-hinting
  "AA:BB:CC:00:00:11":
    panel: trmnl_og
    screen: hello
  "AA:BB:CC:00:00:12":
    panel: trmnl_og
    screen: byonk-builtin/default
```

Then confirm every `screen:` value above actually exists in the `screens:` block or as a builtin:

```bash
CONFIG_FILE=tools/capture-config.yaml cargo run --release -- render \
  --mac AA:BB:CC:00:00:05 --output /tmp/probe.png
```

Expected: exits 0 and writes a PNG. If a screen key is wrong the command errors naming it — fix the key rather than guessing.

- [ ] **Step 2: Write the capture script**

```bash
#!/usr/bin/env bash
# Render every bundled screen for the three-state comparison around the
# resvg byonk-base integration. Runs against any checkout, because it drives
# only the `byonk render` CLI.
set -uo pipefail

OUT="${1:?usage: capture-renders.sh <output-dir>}"
CFG="${CONFIG_FILE:-tools/capture-config.yaml}"
mkdir -p "$OUT/nondeterministic"

# mac:name:bucket   bucket = det | nondet
SCREENS="
AA:BB:CC:00:00:01:calibration-gamut:det
AA:BB:CC:00:00:02:calibration-tone:det
AA:BB:CC:00:00:03:calibration-grey:det
AA:BB:CC:00:00:04:calibration-color:det
AA:BB:CC:00:00:05:mandelbrot:det
AA:BB:CC:00:00:06:demo-font-bitmap:det
AA:BB:CC:00:00:07:demo-font-ttf:det
AA:BB:CC:00:00:08:demo-font-hinting:det
AA:BB:CC:00:00:11:hello:nondet
AA:BB:CC:00:00:12:builtin-default:nondet
"

: > "$OUT/MANIFEST.txt"
for entry in $SCREENS; do
  mac="${entry%:*:*}"
  rest="${entry#"$mac":}"
  name="${rest%:*}"
  bucket="${rest##*:}"
  [ "$bucket" = det ] && dir="$OUT" || dir="$OUT/nondeterministic"
  CONFIG_FILE="$CFG" cargo run --release --quiet -- \
      render --mac "$mac" --output "$dir/$name.png" >/dev/null 2>&1
  echo "$name $bucket exit=$?" >> "$OUT/MANIFEST.txt"
done
cat "$OUT/MANIFEST.txt"
```

`set -e` is deliberately **not** used: one screen failing to render must not abort the sweep, and the manifest records the failure.

- [ ] **Step 3: Verify the script captures the current state**

Run: `chmod +x tools/capture-renders.sh && ./tools/capture-renders.sh /tmp/byonk-renders/state2`
Expected: `MANIFEST.txt` shows `exit=0` for all 8 deterministic screens. If any is non-zero, fix the config entry before continuing — a missing baseline is worse than no baseline, because it looks like coverage.

- [ ] **Step 4: Verify the capture is actually reproducible**

Run it a second time into a different directory and diff:

```bash
./tools/capture-renders.sh /tmp/byonk-renders/state2-repeat
for f in /tmp/byonk-renders/state2/*.png; do
  cmp -s "$f" "/tmp/byonk-renders/state2-repeat/$(basename "$f")" \
    || echo "NON-DETERMINISTIC: $(basename "$f")"
done
```

Expected: **no output.** Any screen printed here is non-deterministic and must be moved to the `nondet` bucket in the script — otherwise it will show up as a false positive in every later diff and destroy trust in the whole comparison. Fix the script and re-verify before continuing.

- [ ] **Step 5: Capture the pre-#30 baseline (state 1)**

State 1 is `main` before PR #30, on the current pinned `skrifa` build. Use a worktree so the working tree is untouched:

```bash
git worktree add /scratch/oetiker/claude-worktrees/byonk-state1 main
cp tools/capture-renders.sh tools/capture-config.yaml \
   /scratch/oetiker/claude-worktrees/byonk-state1/tools/
cd /scratch/oetiker/claude-worktrees/byonk-state1 \
  && CARGO_BUILD_JOBS=2 ./tools/capture-renders.sh /tmp/byonk-renders/state1
```

Expected: renders complete. Some may fail if a screen only exists on the #30 branch — that is fine and expected; record it in the manifest and move on. State 1 is context for the manual assessment, not a diff target.

- [ ] **Step 6: Commit**

```bash
git add tools/capture-renders.sh tools/capture-config.yaml
git commit -m "test: add a render-capture harness for the resvg integration diff"
```

Do **not** commit the PNGs — they are large, and they are working artifacts, not fixtures.

---

### Task 2: Generic font families resolve without system fonts

This is the live CI failure and a production bug. `fontdb::Database::new()` sets the generic families to Arial / Times New Roman / Courier New (`fontdb-0.24.0/src/lib.rs`, and the same in 0.23). None are bundled. `SvgRenderer::with_fonts` calls `load_system_fonts()`, so on macOS Arial resolves and everything looks fine; on the CI Linux runner nothing matches, usvg logs `No match for '…' font-family.` and **skips the text**, leaving a blank white render.

That blank render is what fails all three tests: no text ⇒ no greys ⇒ the PLTE loses expected entries, and the pre-dither and dithered images optimise down to identical bytes.

It is also a production bug: the release image is `FROM scratch` — no system fonts, no fontconfig — and `byonk-base/v1/{base,header,footer}.svg` plus the built-in error screens in `template_service.rs` all use `font-family="sans-serif"`.

**Files:**
- Modify: `src/rendering/svg_to_png.rs:43-68` (`SvgRenderer::with_fonts`)
- Test: `src/rendering/svg_to_png.rs` (test module at the bottom of the same file)
- Modify: `CHANGES.md`

**Interfaces:**
- Produces: no signature change. `SvgRenderer::with_fonts` gains internal generic-family mapping.

**The mapping is interim.** Task 9 settles which trio backs the generics; until then all generics map to `Outfit`, byonk's house sans, except monospace which maps to the bundled `Terminus (TTF)`. Reversible by construction.

- [ ] **Step 1: Write the failing test**

The test must assert the generic resolves **to a bundled face**, not merely that it resolves — on macOS, `query` succeeds today via Arial, so a bare `is_some()` assertion would pass without the fix and prove nothing.

Add to the test module in `src/rendering/svg_to_png.rs`:

```rust
#[test]
fn generic_families_resolve_to_bundled_fonts() {
    use fontdb::{Family, Query};

    let renderer = SvgRenderer::default();
    let db = &renderer.fontdb;

    // The bundled families. Anything outside this set means we resolved a
    // system font, which does not exist in the `FROM scratch` release image.
    let bundled: std::collections::HashSet<String> = db
        .faces()
        .filter_map(|f| f.families.first().map(|(n, _)| n.clone()))
        .collect();

    for generic in [
        Family::SansSerif,
        Family::Serif,
        Family::Monospace,
        Family::Cursive,
        Family::Fantasy,
    ] {
        let id = db
            .query(&Query {
                families: &[generic],
                ..Default::default()
            })
            .unwrap_or_else(|| panic!("{generic:?} did not resolve at all"));

        let family = db
            .face(id)
            .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
            .expect("resolved face must have a family name");

        assert!(
            bundled.contains(&family),
            "{generic:?} resolved to {family:?}, which is not a bundled font; \
             it would not resolve in the FROM scratch release image"
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib generic_families_resolve_to_bundled_fonts -- --nocapture`
Expected: **FAIL** on macOS with a message naming a system font (most likely `"SansSerif resolved to \"Arial\""`). If it passes on your machine, you have no system fonts installed — confirm by checking `db.len()` against the bundled count before assuming the test is wrong.

- [ ] **Step 3: Implement the mapping**

In `src/rendering/svg_to_png.rs`, in `with_fonts`, insert **after** the `load_system_fonts()` call and before the `tracing::info!`:

```rust
        // Load system fonts as fallback
        fontdb.load_system_fonts();

        // Point the generic families at fonts we actually ship.
        //
        // fontdb defaults these to Arial / Times New Roman / Courier New, none
        // of which byonk bundles. That is invisible on a developer machine —
        // `load_system_fonts` above finds them — but the release image is
        // `FROM scratch`, so on the device nothing matches, usvg skips the text
        // and the screen renders blank. byonk's own `v1/base.svg`,
        // `v1/header.svg`, `v1/footer.svg` and the built-in error screens all
        // ask for `sans-serif`.
        //
        // These must be set AFTER `load_system_fonts()`: on Linux that call
        // parses fontconfig and overwrites the generics with whatever the host
        // aliases them to. Deterministic rendering across dev, CI and the
        // release image is the point.
        fontdb.set_sans_serif_family("Outfit");
        fontdb.set_serif_family("Outfit");
        fontdb.set_cursive_family("Outfit");
        fontdb.set_fantasy_family("Outfit");
        fontdb.set_monospace_family("Terminus (TTF)");
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib generic_families_resolve_to_bundled_fonts -- --nocapture`
Expected: **PASS**.

- [ ] **Step 5: Run the three previously failing tests**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib screen_store::tests::render_ -- --nocapture`
Expected: `render_include_raw_produces_pre_dither_png`, `render_script_colors_actual_length_mismatch_falls_back_to_panel_and_logs_warning`, and `render_script_colors_actual_wins_over_panel_colors_actual_when_lengths_match` all PASS. They passed on macOS before too — the real proof is CI in Step 8.

- [ ] **Step 6: Capture state 2b and see what moved**

```bash
./tools/capture-renders.sh /tmp/byonk-renders/state2b
for f in /tmp/byonk-renders/state2/*.png; do
  b="$(basename "$f")"
  cmp -s "$f" "/tmp/byonk-renders/state2b/$b" || echo "CHANGED: $b"
done
```

Expected: screens using `sans-serif` change; screens naming `Outfit` or an X11 font explicitly do not. **Open the changed PNGs and look at them.** Text moving from Arial to Outfit is the intended change; text *disappearing* or overflowing its box is not, and means a layout was tuned against Arial metrics.

- [ ] **Step 7: Add the changelog entry**

In `CHANGES.md`, under `## [Unreleased]` → `### Fixed`:

```markdown
- Text using the generic `sans-serif`, `serif`, `monospace`, `cursive` or `fantasy`
  font families now renders on the device. These resolved only to fonts byonk does
  not ship, so on the released container image — which contains no system fonts —
  such text was silently dropped and the screen rendered blank. They now resolve to
  bundled fonts (Outfit, and Terminus for monospace).
```

- [ ] **Step 8: Commit and confirm CI goes green**

```bash
git add src/rendering/svg_to_png.rs CHANGES.md
git diff --cached --stat
git commit -m "fix: resolve generic font families to bundled fonts"
git push
gh pr checks 30 --watch
```

Expected: the three `screen_store` failures are gone. **This is the gate for the task** — the local run in Step 5 does not test the failing condition, since macOS has the fonts.

---

### Task 3: Move to `byonk-base`, and recompute bitmap strikes with skrifa

The dependency bump and the `bitmap_strikes` replacement are one task: dropping the fontdb patch is what removes the field, so the tree does not compile between them.

**Files:**
- Modify: `Cargo.toml` (dependency versions, `[patch.crates-io]`)
- Create: `src/rendering/font_strikes.rs`
- Modify: `src/rendering/mod.rs` (declare the module)
- Modify: `src/rendering/svg_to_png.rs` (strike map on `SvgRenderer`)
- Modify: `src/services/content_pipeline.rs:154`
- Modify: `CHANGES.md`

**Interfaces:**
- Produces: `crate::rendering::font_strikes::bitmap_strikes_for(data: &[u8], index: u32) -> Vec<u16>` — ppem sizes of a face's embedded bitmap strikes, **sorted ascending, deduplicated**. Empty for a face with no strikes or unparseable data.
- Produces: `SvgRenderer::bitmap_strikes(&self, id: fontdb::ID) -> &[u16]` — the same list, cached per face at load time.
- Consumes: nothing from earlier tasks.

Sortedness is not cosmetic: `test_bitmap_strikes_exposed` asserts it, and the Lua table is 1-indexed and documented as ascending.

- [ ] **Step 1: Write the failing test for the strike computation**

Create `src/rendering/font_strikes.rs` containing only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// X11Helv is a bitmap pixel font byonk bundles; it is the fixture the
    /// existing `test_bitmap_strikes_exposed` relies on.
    fn x11helv_bytes() -> Vec<u8> {
        let loader = crate::assets::AssetLoader::new_embedded();
        loader
            .font_data()
            .into_iter()
            .find(|(name, _)| name.contains("X11Helv"))
            .map(|(_, data)| data.into_owned())
            .expect("X11Helv must be bundled")
    }

    #[test]
    fn x11helv_strikes_are_non_empty_and_ascending() {
        let strikes = bitmap_strikes_for(&x11helv_bytes(), 0);
        assert!(
            !strikes.is_empty(),
            "X11Helv is a bitmap font and must report strikes"
        );
        for w in strikes.windows(2) {
            assert!(w[0] < w[1], "strikes must be ascending and deduplicated: {strikes:?}");
        }
    }

    #[test]
    fn an_outline_font_reports_no_strikes() {
        // The control: without this, a `bitmap_strikes_for` that returned a
        // hardcoded non-empty list would pass the test above.
        let loader = crate::assets::AssetLoader::new_embedded();
        let outfit = loader
            .font_data()
            .into_iter()
            .find(|(name, _)| name.contains("Outfit"))
            .map(|(_, data)| data.into_owned())
            .expect("Outfit must be bundled");
        assert!(
            bitmap_strikes_for(&outfit, 0).is_empty(),
            "Outfit is an outline font and must report no strikes"
        );
    }

    #[test]
    fn garbage_input_reports_no_strikes() {
        assert!(bitmap_strikes_for(b"not a font at all", 0).is_empty());
    }
}
```

Check the exact `AssetLoader` constructor and font-data accessor names against `src/assets.rs:397` before running — the accessor is documented there as "Get all font data (for loading into fontdb)". Use whatever it is actually called; do not invent a name.

- [ ] **Step 2: Run the test to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib font_strikes -- --nocapture`
Expected: **FAIL to compile**, `cannot find function bitmap_strikes_for`.

- [ ] **Step 3: Implement the strike computation**

Prepend to `src/rendering/font_strikes.rs`:

```rust
//! Bitmap strike sizes for a font face.
//!
//! This used to be `fontdb::FaceInfo::bitmap_strikes`, a field byonk's fork of
//! fontdb carried. Upstream fontdb has no such field and never will — it is a
//! separate project from resvg, so no resvg PR could add it — and carrying the
//! fork was the only reason byonk pinned fontdb to the resvg repository at all.
//!
//! It was never new capability, only a cached convenience: skrifa exposes the
//! data directly, over the same font bytes byonk already owns. Computing it here
//! is what lets the fontdb pin disappear.

use skrifa::{bitmap::BitmapStrikes, FontRef, MetadataProvider as _};

/// The ppem sizes of `data`'s embedded bitmap strikes, ascending and deduplicated.
///
/// Returns empty for an outline-only font and for data that does not parse —
/// an unreadable face is not an error here, it simply has no strikes to offer.
pub fn bitmap_strikes_for(data: &[u8], index: u32) -> Vec<u16> {
    let Ok(font) = FontRef::from_index(data, index) else {
        return Vec::new();
    };

    let mut sizes: Vec<u16> = BitmapStrikes::new(&font)
        .iter()
        .map(|s| s.ppem().round() as u16)
        .filter(|&ppem| ppem > 0)
        .collect();

    sizes.sort_unstable();
    sizes.dedup();
    sizes
}
```

`ppem()` returns `f32` in skrifa 0.44; the existing Lua contract is `Vec<u16>`, hence the round. Drop the unused `MetadataProvider` import if the compiler says it is not needed — `BitmapStrikes::new` is an inherent constructor.

- [ ] **Step 4: Declare the module**

In `src/rendering/mod.rs`, alongside the existing module declarations:

```rust
pub mod font_strikes;
```

- [ ] **Step 5: Update `Cargo.toml`**

In `[dependencies]`:

```toml
# Rendering
resvg = "0.48.1"
tiny-skia = "0.12"
png = "0.17"
fontdb = "0.24"
# Bitmap strike introspection. Replaces the `bitmap_strikes` field byonk's
# fontdb fork carried; see src/rendering/font_strikes.rs. Pinned to the version
# usvg uses so the tree holds one copy.
skrifa = "0.44"
```

And replace the `[patch.crates-io]` block entirely:

```toml
[patch.crates-io]
# resvg with bitmap glyph rendering, font hinting, and the per-font hinting and
# bitmap-strike resolver hooks. Everything else is upstream 0.48.1.
# https://github.com/oetiker/resvg/tree/byonk-base
resvg = { git = "https://github.com/oetiker/resvg.git", branch = "byonk-base" }
usvg  = { git = "https://github.com/oetiker/resvg.git", branch = "byonk-base" }
```

The `fontdb` patch line is **deleted**, not commented out.

- [ ] **Step 6: Resolve and confirm exactly one copy of each shared crate**

```bash
CARGO_BUILD_JOBS=2 cargo update -p resvg -p usvg 2>&1 | tail -20
cargo tree -i tiny-skia
cargo tree -i fontdb
cargo tree -i skrifa
```

Expected: `tiny-skia v0.12.x`, `fontdb v0.24.x`, `skrifa v0.44.x`, **each appearing exactly once**, with both byonk and resvg/usvg listed underneath. Two versions of `tiny-skia` or `fontdb` means byonk and resvg hold different copies of the same type and it will not compile — stop and fix the versions rather than working around the error.

- [ ] **Step 7: Cache the strike map on the renderer**

In `src/rendering/svg_to_png.rs`, add the field:

```rust
pub struct SvgRenderer {
    /// Font database for text rendering
    fontdb: Arc<fontdb::Database>,
    /// Bitmap strike sizes per face, ascending.
    ///
    /// Computed once at load time rather than per query: `with_face_data`
    /// re-parses the font, and the Lua `fonts` global reads this for every face
    /// on every script run.
    strikes: std::collections::HashMap<fontdb::ID, Vec<u16>>,
}
```

At the end of `with_fonts`, after the generic-family calls from Task 2 and before constructing `Self`:

```rust
        let strikes = fontdb
            .faces()
            .map(|face| {
                let sizes = fontdb
                    .with_face_data(face.id, |data, index| {
                        crate::rendering::font_strikes::bitmap_strikes_for(data, index)
                    })
                    .unwrap_or_default();
                (face.id, sizes)
            })
            .collect();

        Self {
            fontdb: Arc::new(fontdb),
            strikes,
        }
```

`fontdb.faces()` borrows `fontdb` immutably while the closure also borrows it — collect the ids first if the borrow checker objects:

```rust
        let ids: Vec<fontdb::ID> = fontdb.faces().map(|f| f.id).collect();
        let strikes = ids
            .into_iter()
            .map(|id| {
                let sizes = fontdb
                    .with_face_data(id, crate::rendering::font_strikes::bitmap_strikes_for)
                    .unwrap_or_default();
                (id, sizes)
            })
            .collect();
```

Add the accessor next to `font_faces`:

```rust
    /// Bitmap strike sizes for a face, ascending. Empty for outline fonts.
    pub fn bitmap_strikes(&self, id: fontdb::ID) -> &[u16] {
        self.strikes.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }
```

- [ ] **Step 8: Update the one consumer**

In `src/services/content_pipeline.rs`, the loop at line ~145 currently reads `face.bitmap_strikes.clone()`. Replace that line with:

```rust
                    bitmap_strikes: renderer.svg_renderer.bitmap_strikes(face.id).to_vec(),
```

If the borrow checker objects to calling a method on `renderer.svg_renderer` while iterating `renderer.svg_renderer.font_faces()`, collect the face data first:

```rust
        let faces: Vec<_> = renderer
            .svg_renderer
            .font_faces()
            .map(|f| {
                (
                    f.id,
                    f.families.first().map(|(n, _)| n.clone()),
                    format!("{:?}", f.style),
                    f.weight.0,
                    format!("{:?}", f.stretch),
                    f.monospaced,
                    f.post_script_name.clone(),
                )
            })
            .collect();
```

then build `font_families` from `faces`, calling `bitmap_strikes(id)` per entry.

- [ ] **Step 9: Build and fix the compile errors**

Run: `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets 2>&1 | tail -60`

Expected breakages and what to do:
- `Options` struct-literal errors in `rasterize_svg` / `rasterize_tone_mask` — there should be none, because both use `..Default::default()`. If one appears, it means a field lost its `Default`; read the error, do not guess.
- `tiny_skia` errors — there should be none. byonk uses only `Pixmap`, `Color::WHITE`, `Color::BLACK`, `as_mut()`, `Transform`, none of which changed in 0.12.
- Anything mentioning two versions of `tiny-skia` or `fontdb` — go back to Step 6.

**IDE diagnostics lie in this tree. Only an actual `cargo` run counts.**

- [ ] **Step 10: Run the contract tests**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib svg_to_png -- --nocapture`
Expected: `test_bitmap_strikes_exposed` and `test_bitmap_font_families` **PASS unchanged**. If either needed editing to pass, the substitution is wrong — stop and raise it.

- [ ] **Step 11: Run the full suite in the background**

Run (background): `CARGO_BUILD_JOBS=2 make check`
Expected: no new failures beyond the three pre-existing `#[ignore]`d `preprocess` ones. Re-check `git status` afterwards — `make check` reformats in place.

- [ ] **Step 12: Add the changelog entry**

```markdown
- Updated the SVG renderer to resvg 0.48.1, which brings a faster and more correct
  text engine. Text positioning and glyph advances are more accurate, so some screens
  may shift by a pixel or two.
```

- [ ] **Step 13: Commit**

```bash
git add Cargo.toml Cargo.lock src/rendering/font_strikes.rs src/rendering/mod.rs \
        src/rendering/svg_to_png.rs src/services/content_pipeline.rs CHANGES.md
git diff --cached --stat
git commit -m "feat: move to resvg byonk-base and compute bitmap strikes with skrifa"
```

---

### Task 4: Warn when the render scale is not 1.0

Hinting grid-fits glyphs to whole pixels, which only survives at scale 1.0 or an integer zoom. byonk satisfies this by construction — `layout.width`/`layout.height` handed to Lua are the panel's native pixels, so a screen authored at those dimensions renders at exactly 1.0. A screen that **hardcodes 800×480** instead renders at a fractional scale on any other panel and silently loses both hinting and sharpness.

This is worth its own task because it is useful independently of hinting and is the cheapest possible guard against an invisible authoring mistake.

**Files:**
- Modify: `src/rendering/svg_to_png.rs` (`fit_transform`, `rasterize_svg`)
- Test: same file
- Modify: `CHANGES.md`

**Interfaces:**
- Consumes: `SvgRenderer::fit_transform` from Task 3's tree (unchanged signature).
- Produces: nothing new; a `tracing::warn!` side effect.

- [ ] **Step 1: Write the failing test**

`tracing` output is awkward to assert on. Test the decision instead of the logging by extracting the predicate:

```rust
#[test]
fn render_scale_flags_only_non_unit_scales() {
    // Authored at the panel's own size: the intended case.
    assert!(!SvgRenderer::scale_is_degraded(800.0, 480.0, DisplaySpec::OG));
    // Hardcoded 800x480 shown on a TRMNL X: fractional scale, degraded.
    assert!(SvgRenderer::scale_is_degraded(800.0, 480.0, DisplaySpec::X));
    // An exact integer zoom keeps glyphs on whole pixels, so it is fine.
    assert!(!SvgRenderer::scale_is_degraded(936.0, 702.0, DisplaySpec::X));
}
```

`DisplaySpec::X` is 1872×1404, so 936×702 scales by exactly 2.

- [ ] **Step 2: Run the test to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib render_scale_flags_only_non_unit_scales`
Expected: **FAIL to compile**, `no function or associated item named scale_is_degraded`.

- [ ] **Step 3: Implement**

Next to `fit_transform` in `src/rendering/svg_to_png.rs`:

```rust
    /// Whether an SVG of `svg_w` x `svg_h` lands on a non-integer zoom in `spec`.
    ///
    /// Hinted glyph outlines are grid-fitted to whole pixels, which only holds
    /// at scale 1.0 or an exact integer zoom. A screen that hardcodes its
    /// dimensions instead of using `layout.width`/`layout.height` renders at a
    /// fractional scale on any other panel and loses that, on top of the
    /// resampling blur it already suffers.
    fn scale_is_degraded(svg_w: f32, svg_h: f32, spec: DisplaySpec) -> bool {
        let scale = (spec.width as f32 / svg_w).min(spec.height as f32 / svg_h);
        (scale - scale.round()).abs() > 1e-4
    }
```

And in `rasterize_svg`, after `fit_transform` is computed:

```rust
        let svg_size = tree.size();
        let transform = Self::fit_transform(svg_size.width(), svg_size.height(), spec);

        if Self::scale_is_degraded(svg_size.width(), svg_size.height(), spec) {
            tracing::warn!(
                svg_width = svg_size.width(),
                svg_height = svg_size.height(),
                panel_width = spec.width,
                panel_height = spec.height,
                "SVG is not authored at the panel size, so it renders at a \
                 fractional scale: text will be blurred and hinting has no effect. \
                 Use layout.width and layout.height instead of hardcoded dimensions."
            );
        }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib render_scale_flags_only_non_unit_scales`
Expected: **PASS**.

- [ ] **Step 5: Check the warning does not fire for the bundled screens**

Run: `RUST_LOG=warn ./tools/capture-renders.sh /tmp/byonk-renders/scale-check 2>&1 | grep -i "fractional scale" || echo "no screen renders at a fractional scale"`
Expected: the "no screen" message. If a bundled screen trips it, that screen has the hardcoded-dimensions bug and should be noted for Task 6 — do not silence the warning.

- [ ] **Step 6: Changelog and commit**

```markdown
- Byonk now logs a warning when a screen's SVG is not authored at the device's
  pixel dimensions. Such a screen is rescaled to fit, which blurs its text; use
  `layout.width` and `layout.height` rather than hardcoded numbers.
```

```bash
git add src/rendering/svg_to_png.rs CHANGES.md
git commit -m "feat: warn when a screen renders at a fractional scale"
```

---

### Task 5: `FontConfig` — the resolved font behaviour for one render

Pure data and pure functions, no Lua and no renderer wiring. Separated from Task 6 so the parsing and the adaptive default can be tested exhaustively without a render in the loop.

**Files:**
- Create: `src/rendering/font_config.rs`
- Modify: `src/rendering/mod.rs`
- Test: in `src/rendering/font_config.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct FontConfig {
      pub default: Option<HintingSpec>,
      pub variants: std::collections::BTreeMap<String, FontVariant>,
  }
  pub struct FontVariant {
      pub font: String,
      pub strikes: Option<bool>,
      pub hinting: Option<Option<HintingSpec>>, // outer None = inherit, inner None = off
  }
  pub struct HintingSpec {
      pub engine: HintingEngine,      // Interpreter | Auto | AutoFallback
      pub target: HintingTarget,      // Mono | Smooth { mode: HintingMode,
                                      //                 symmetric_rendering: bool,
                                      //                 preserve_linear_metrics: bool }
  }
  pub enum HintingMode { Normal, Light, Lcd, VerticalLcd }
  impl FontConfig {
      pub fn adaptive_default(grey_count: usize) -> Self;
  }
  impl HintingSpec {
      pub fn to_usvg(&self) -> usvg::FontHintingOptions;
  }
  ```
  Exact enum shapes must mirror `usvg`'s `FontHintingOptions` — read `crates/usvg/src/text/hinting.rs` on `byonk-base` and copy the variant names rather than inventing them.
- Consumes: nothing.

**The adaptive default** reproduces today's `v1/hinting.svg` exactly, so migrating a screen preserves its output by construction: `grey_count <= 2` → `target: Mono`, `symmetric_rendering: false`, `preserve_linear_metrics: true`, `mode: Normal`, `engine: Auto`; `grey_count > 2` → `target: Smooth` with `mode: Normal`, `engine: Auto`.

- [ ] **Step 1: Read the upstream types**

```bash
sed -n '1,80p' /private/tmp/claude-501/-Users-oetiker-checkouts-byonk/6b605fbb-3037-43ac-b94d-fbc35c32f407/scratchpad/resvg/crates/usvg/src/text/hinting.rs
```

If that scratch clone is gone, re-clone: `git clone --branch byonk-base --depth 5 https://github.com/oetiker/resvg.git`. Copy the enum and struct shapes verbatim; do not work from this plan's summary of them.

- [ ] **Step 2: Write the failing test for the adaptive default**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bw_panels_get_mono_hinting_matching_the_old_partial() {
        let cfg = FontConfig::adaptive_default(2);
        let h = cfg.default.expect("a BW panel must be hinted");
        assert!(matches!(h.target, HintingTarget::Mono));
        assert!(matches!(h.engine, HintingEngine::Auto));
    }

    #[test]
    fn greyscale_panels_get_smooth_hinting() {
        let cfg = FontConfig::adaptive_default(4);
        let h = cfg.default.expect("a greyscale panel must be hinted");
        assert!(
            matches!(h.target, HintingTarget::Smooth { .. }),
            "greyscale must not use mono hinting; the old v1/hinting.svg gated \
             mono on grey_count <= 2"
        );
    }

    #[test]
    fn the_two_defaults_actually_differ() {
        // Without this, both branches returning the same value would pass the
        // two tests above and the adaptivity would be a no-op.
        assert_ne!(
            format!("{:?}", FontConfig::adaptive_default(2).default),
            format!("{:?}", FontConfig::adaptive_default(4).default),
        );
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib font_config`
Expected: **FAIL to compile**.

- [ ] **Step 4: Implement the types and `adaptive_default`**

Write the structs from the Interfaces block with `#[derive(Debug, Clone, PartialEq)]`, `HintingEngine` and `HintingTarget` mirroring usvg's, `Default` on `HintingSpec` giving `engine: Auto, target: Smooth { mode: Normal, symmetric_rendering: true, preserve_linear_metrics: false }`, and:

```rust
impl FontConfig {
    /// The server-side default, reproducing what `byonk-base/v1/hinting.svg`
    /// emitted before hinting moved behind a resolver: mono on a black-and-white
    /// panel, smooth once there are greys to anti-alias with.
    ///
    /// Screens need no Lua to get this. The `font_hinting` directive is a pure
    /// override, so migrating a screen is deleting its `{% include %}` line and
    /// its output is preserved by construction.
    pub fn adaptive_default(grey_count: usize) -> Self {
        let target = if grey_count <= 2 {
            HintingTarget::Mono
        } else {
            HintingTarget::Smooth {
                mode: HintingMode::Normal,
                symmetric_rendering: false,
                preserve_linear_metrics: true,
            }
        };
        Self {
            default: Some(HintingSpec { engine: HintingEngine::Auto, target }),
            variants: Default::default(),
        }
    }
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib font_config`
Expected: all three PASS.

- [ ] **Step 6: Add `to_usvg` and a round-trip test**

```rust
    #[test]
    fn hinting_spec_maps_onto_usvg_faithfully() {
        let spec = HintingSpec {
            engine: HintingEngine::Interpreter,
            target: HintingTarget::Smooth {
                mode: HintingMode::Light,
                symmetric_rendering: true,
                preserve_linear_metrics: true,
            },
        };
        let out = spec.to_usvg();
        assert!(matches!(out.engine, usvg::HintingEngine::Interpreter));
        // Assert the target's fields survive, not merely its discriminant —
        // a mapping that dropped `mode` would pass a discriminant-only check.
        match out.target {
            usvg::HintingTarget::Smooth { mode, symmetric_rendering, preserve_linear_metrics } => {
                assert!(matches!(mode, usvg::HintingMode::Light));
                assert!(symmetric_rendering);
                assert!(preserve_linear_metrics);
            }
            other => panic!("expected Smooth, got {other:?}"),
        }
    }
```

Adjust the `usvg::` paths to whatever Step 1 showed. Implement `to_usvg` to make it pass.

- [ ] **Step 7: Commit**

```bash
git add src/rendering/font_config.rs src/rendering/mod.rs
git commit -m "feat: add FontConfig with the adaptive hinting default"
```

---

### Task 6: Install the font resolver, and thread `FontConfig` through the render

**Files:**
- Modify: `src/rendering/svg_to_png.rs` (`rasterize_svg`, `rasterize_tone_mask`, `render_to_palette_png`, `render_to_raw_png`)
- Modify: `src/services/content_pipeline.rs` (`render_png_from_svg`, `render_raw_png_from_svg`)
- Test: `src/rendering/svg_to_png.rs`

**Interfaces:**
- Consumes: `FontConfig`, `FontVariant`, `HintingSpec` from Task 5; `SvgRenderer::bitmap_strikes` from Task 3.
- Produces: `render_to_palette_png` and `render_to_raw_png` gain a trailing `fonts: Option<&FontConfig>` parameter; `rasterize_svg` and `rasterize_tone_mask` likewise.

**The tone mask must use the same `FontConfig` as the main render.** The mask is the same document with paint forced to black and white, rasterized separately, and its pixels select which of the main render's pixels are continuous-tone. If the two resolved fonts differently, the mask would be offset from the text it is masking. Both call sites therefore take the same value — this is a correctness requirement, not tidiness.

- [ ] **Step 1: Write the failing test**

The observable effect of hinting is that pixels change. Assert that, and assert the comparison is non-degenerate:

```rust
#[test]
fn hinting_changes_the_rendered_pixels() {
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
      <rect width="800" height="480" fill="#fff"/>
      <text x="20" y="40" font-family="Outfit" font-size="11"
            style="font-variation-settings: 'wght' 400">Hamburgefonstiv 0123456789</text>
    </svg>"##;

    let r = SvgRenderer::default();
    let bw = &[(0u8, 0u8, 0u8), (255, 255, 255)];

    let unhinted = r
        .render_to_palette_png(SVG.as_bytes(), DisplaySpec::OG, bw, None, false, None, None, None)
        .expect("unhinted render");

    let cfg = FontConfig::adaptive_default(2);
    let hinted = r
        .render_to_palette_png(
            SVG.as_bytes(), DisplaySpec::OG, bw, None, false, None, None, Some(&cfg),
        )
        .expect("hinted render");

    assert_ne!(
        unhinted, hinted,
        "mono hinting at 11px must change the rasterisation; identical output \
         means the resolver is not reaching the renderer"
    );
}
```

If this ever needs relaxing, the fixture is wrong (too large a font size, or a font with no useful hints) — do not weaken the assertion.

- [ ] **Step 2: Run to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib hinting_changes_the_rendered_pixels`
Expected: **FAIL to compile** — `render_to_palette_png` takes 7 arguments, not 8.

- [ ] **Step 3: Thread the parameter**

Add `fonts: Option<&FontConfig>` as the final parameter of `render_to_palette_png`, `render_to_raw_png`, `rasterize_svg`, and `rasterize_tone_mask`. All four already carry `#[allow(clippy::too_many_arguments)]` or are private. Pass it straight through: `render_to_palette_png` hands the same value to both `rasterize_svg` and `rasterize_tone_mask`.

Update the two `usvg::Options` construction sites identically:

```rust
        let options = usvg::Options {
            fontdb: self.fontdb.clone(),
            font_hinting: fonts.and_then(|f| f.default.as_ref()).map(HintingSpec::to_usvg),
            font_resolver: self.font_resolver(fonts),
            ..Default::default()
        };
```

- [ ] **Step 4: Build the resolver**

Add to `impl SvgRenderer`:

```rust
    /// A `FontResolver` implementing `fonts`' variants and the strike policy.
    ///
    /// Variants exist because `select_hinting` and `select_bitmap` are keyed on
    /// face ID, and fontdb does not deduplicate identical font data: loading the
    /// same bytes N times yields N distinct IDs all reporting the same family.
    /// So a variant is a second load of an existing font, reachable from the SVG
    /// through a plain `font-family` — standard markup, no custom attributes.
    fn font_resolver<'a>(&'a self, fonts: Option<&'a FontConfig>) -> usvg::FontResolver<'a> {
        let mut resolver = usvg::FontResolver::default();
        let Some(cfg) = fonts else { return resolver };

        // Alias -> the face ID we loaded for it. Populated lazily by
        // select_font and read by the other two hooks, which receive only an
        // ID. The hooks are `Fn + Send + Sync`, hence the mutex.
        let aliases: Arc<std::sync::Mutex<std::collections::HashMap<usvg::fontdb::ID, String>>> =
            Default::default();

        let variants = cfg.variants.clone();
        let seen = aliases.clone();
        let base_selector = usvg::FontResolver::default_font_selector();
        resolver.select_font = Box::new(move |font, db| {
            for family in &font.families {
                let usvg::FontFamily::Named(name) = family else { continue };
                let Some(variant) = variants.get(name.as_str()) else { continue };

                // A second load of the same bytes yields a second face ID —
                // fontdb does not deduplicate identical font data — and that
                // is what gives the variant its own hinting and strike config.
                let source = db
                    .faces()
                    .find(|f| f.families.iter().any(|(n, _)| n == &variant.font))
                    .map(|f| (f.source.clone(), f.index))?;
                let before = db.len();
                usvg::fontdb::Database::load_font_source(Arc::make_mut(db), source.0);
                let id = db.faces().nth(before).map(|f| f.id)?;
                seen.lock().unwrap().insert(id, name.clone());
                return Some(id);
            }
            base_selector(font, db)
        });

        let variants = cfg.variants.clone();
        let seen = aliases.clone();
        resolver.select_hinting = Box::new(move |id, _size, global, _db| {
            let alias = seen.lock().unwrap().get(&id).cloned();
            match alias.and_then(|a| variants.get(&a).and_then(|v| v.hinting.clone())) {
                Some(spec) => spec.map(|s| s.to_usvg()),   // Some(None) = off
                None => global,                            // not a variant: inherit
            }
        });

        let variants = cfg.variants.clone();
        let seen = aliases;
        resolver.select_bitmap = Box::new(move |id, _size, _db| {
            let alias = seen.lock().unwrap().get(&id).cloned();
            alias
                .and_then(|a| variants.get(&a).and_then(|v| v.strikes))
                .unwrap_or(true) // resvg's default: strikes are used
        });

        resolver
    }
```

**The API names above are unverified and are the one place this plan is guessing.** `load_font_source`, `Database::len`, and re-finding the newly loaded face by position are plausible from fontdb 0.24's surface but were not compiled. Read `fontdb-0.24.0/src/lib.rs` and adjust; if there is no way to learn the ID of a just-loaded face, capture the id set before and after and take the difference. Do not paper over a compile error with a different design without saying so.

**On the strategy, which is a real decision:** `select_font` receives `&mut Arc<Database>` precisely so a resolver may load fonts on demand, and `Source::Binary` is `Arc`-backed, so `Arc::make_mut` duplicates face *metadata*, not font bytes. The alternative — loading every declared variant eagerly in `with_fonts` — cannot work: variants are declared per script, and the database is built once at startup. Lazy loading is therefore the only option, not merely the nicer one. Say that in a doc comment.

**Verify the mutation persists before building on it.** Write a throwaway test that declares one variant and asserts the ID `select_font` returns is not the base font's ID and that a subsequent `select_hinting` call receives it. If `Arc::make_mut`'s clone does not survive into the rest of the parse, the whole variant design needs rethinking and that must surface here, not in Task 8.

- [ ] **Step 5: Run the test to verify it passes**

Run: `CARGO_BUILD_JOBS=2 cargo test --lib hinting_changes_the_rendered_pixels -- --nocapture`
Expected: **PASS**.

- [ ] **Step 6: Add a variant test**

The case that justifies variants at all: two runs of the same family, hinted differently, in one document.

```rust
#[test]
fn a_variant_hints_differently_from_its_base_font_in_one_document() {
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
      <rect width="800" height="480" fill="#fff"/>
      <text x="20" y="40" font-family="Outfit" font-size="11"
            style="font-variation-settings: 'wght' 400">Hamburgefonstiv 0123456789</text>
      <text x="20" y="80" font-family="Outfit Mono" font-size="11"
            style="font-variation-settings: 'wght' 400">Hamburgefonstiv 0123456789</text>
    </svg>"##;

    let r = SvgRenderer::default();
    let bw = &[(0u8, 0u8, 0u8), (255, 255, 255)];
    let render = |cfg: &FontConfig| {
        r.render_to_palette_png(SVG.as_bytes(), DisplaySpec::OG, bw, None, false, None, None, Some(cfg))
            .expect("render")
    };

    // Baseline: no variants at all. "Outfit Mono" is not a family, so the
    // second line falls back to the same face as the first.
    let mut plain = FontConfig::adaptive_default(4);
    plain.variants.clear();

    let mut with_variant = plain.clone();
    with_variant.variants.insert(
        "Outfit Mono".to_string(),
        FontVariant {
            font: "Outfit".to_string(),
            strikes: None,
            hinting: Some(Some(HintingSpec {
                engine: HintingEngine::Auto,
                target: HintingTarget::Mono,
            })),
        },
    );

    assert_ne!(
        render(&plain),
        render(&with_variant),
        "declaring an Outfit Mono variant must change the second line's \
         rasterisation; identical output means select_font never resolved it"
    );

    // Control: a variant nothing in the document references must change
    // nothing. Without this, a resolver applying its hinting to every face
    // would pass the assertion above.
    let mut unused = plain.clone();
    unused.variants.insert(
        "Outfit Unused".to_string(),
        FontVariant {
            font: "Outfit".to_string(),
            strikes: None,
            hinting: Some(Some(HintingSpec {
                engine: HintingEngine::Auto,
                target: HintingTarget::Mono,
            })),
        },
    );
    assert_eq!(
        render(&plain),
        render(&unused),
        "a variant no element uses must not affect the render"
    );
}
```

`adaptive_default(4)` is used as the base so the *document* default is smooth and the variant's mono is genuinely different from it.

- [ ] **Step 7: Update the call sites**

`src/services/content_pipeline.rs` `render_png_from_svg` and `render_raw_png_from_svg` gain the same trailing parameter and pass it down. `src/main.rs:452`, `src/api/display.rs:185` and `:1152` pass `None` for now — Task 7 wires the real value.

- [ ] **Step 8: Full check and commit**

Run (background): `CARGO_BUILD_JOBS=2 make check`
Expected: no new failures.

```bash
git add src/rendering/svg_to_png.rs src/services/content_pipeline.rs src/main.rs src/api/display.rs
git commit -m "feat: install a font resolver so hinting and strikes are per font"
```

---

### Task 7: The `font_hinting` Lua directive

**Files:**
- Modify: `src/services/lua_runtime.rs` (`ScriptResult`, the parse site at ~line 402)
- Modify: `src/services/content_pipeline.rs` (pass `ScriptResult.font_hinting` into the render)
- Modify: `src/api/display.rs`, `src/main.rs` (supply the adaptive default when a script gives nothing)
- Test: `tests/lua_api_test.rs`

**Interfaces:**
- Consumes: `FontConfig` from Task 5.
- Produces: `ScriptResult.font_hinting: Option<FontConfig>`.

**Precedence, stated once:** an absent `font_hinting` means the server's `FontConfig::adaptive_default(grey_count)`. A present `font_hinting` **replaces** it wholesale — it is an override, not a merge, because a merge would make "I want no hinting here" inexpressible. `font_hinting = false` means no hinting at all.

The Lua surface is the spec's, plus variants:

```lua
font_hinting = {
  engine = "auto",                 -- interpreter | auto | auto_fallback
  target = "mono",                 -- or { mode = "normal", symmetric = false,
                                   --      preserve_linear_metrics = true }
  variants = {
    ["Outfit Mono"]     = { font = "Outfit",  hinting = { target = "mono" } },
    ["X11Helv Outline"] = { font = "X11Helv", strikes = false,
                            hinting = { target = "mono" } },
  },
}
```

- [ ] **Step 1: Write the failing tests**

In `tests/lua_api_test.rs`, following the existing pattern there (see the `bitmap_strikes` test at line ~953 for how a script's return value is asserted). Cover, one test each:

1. A script returning no `font_hinting` produces `ScriptResult.font_hinting == None`.
2. `font_hinting = { target = "mono" }` parses to `HintingTarget::Mono` with the default engine.
3. `font_hinting = { target = { mode = "light", symmetric = true, preserve_linear_metrics = true } }` parses with all three fields carried through — assert each field, not just the variant.
4. `font_hinting = false` parses to `Some(FontConfig { default: None, variants: empty })` — hinting explicitly off, distinct from absent.
5. A variant with `strikes = false` and a sibling `hinting` parses both.
6. A variant given as `{ font = "Outfit" }` with no `hinting` inherits (its `hinting` field is `None`).
7. `font_hinting = { target = "nonsense" }` is **rejected with an error naming the bad value**, not silently defaulted. Assert on the error text.

Write all seven out in full — do not write "and similar for the others".

- [ ] **Step 2: Run to verify they fail**

Run: `CARGO_BUILD_JOBS=2 cargo test --test lua_api_test font_hinting -- --nocapture`
Expected: FAIL — the field does not exist.

- [ ] **Step 3: Add the field and the parser**

Add `pub font_hinting: Option<FontConfig>` to `ScriptResult` with a doc comment stating the precedence rule above. Parse it at the existing site (~line 402) alongside `error_clamp` / `noise_scale`.

Note the existing code uses `result.get::<f32>("error_clamp").ok()`, which swallows a malformed value. **Do not follow that pattern here** — test 7 requires a malformed target to be an error. Use an explicit match that distinguishes "absent" from "present but wrong".

- [ ] **Step 4: Run to verify they pass**

Run: `CARGO_BUILD_JOBS=2 cargo test --test lua_api_test font_hinting -- --nocapture`
Expected: all seven PASS.

- [ ] **Step 5: Wire precedence at the render sites**

In `src/services/content_pipeline.rs`, where the palette is already known and `grey_count` is computed (~line 447), resolve:

```rust
let font_config = script_result
    .font_hinting
    .clone()
    .unwrap_or_else(|| FontConfig::adaptive_default(grey_count));
```

and pass `Some(&font_config)` into the render. Do the same in `src/api/display.rs` and `src/main.rs`.

- [ ] **Step 6: Prove the default reaches production renders**

Add a test asserting that a screen with no `font_hinting` still renders hinted — i.e. its output differs from a render explicitly given `font_hinting = false`. Without this, the adaptive default could be silently dropped at a call site and every unit test above would still pass.

- [ ] **Step 7: Changelog and commit**

```markdown
- Font hinting is now applied automatically: mono hinting on black-and-white panels,
  smooth hinting where the panel has greys. Screens no longer need to include
  `byonk-base-v1/hinting.svg` to get sharp text.
- New optional `font_hinting` directive in a screen's Lua return value, overriding
  the automatic behaviour per document or per font, including declaring font
  variants that hint the same font two different ways in one screen.
```

```bash
git add src/services/lua_runtime.rs src/services/content_pipeline.rs \
        src/api/display.rs src/main.rs tests/lua_api_test.rs CHANGES.md
git commit -m "feat: add the font_hinting Lua directive"
```

---

### Task 8: Migrate the 12 screens, rebuild the hinting demo, write the docs

`byonk-base/v1/hinting.svg` is a **versioned `v1` asset**, so changing it is a breaking change for screen authors — hence the docs upgrade notice.

**Files:**
- Modify: `byonk-base/v1/hinting.svg`
- Modify: the 12 screens listed below
- Modify: `screens/examples/demo/font/hinting/` (script + template)
- Create: `docs/src/reference/font-hinting.md`
- Modify: `docs/src/SUMMARY.md`, `docs/src/tutorial/svg-templates.md`, `docs/src/tutorial/first-screen.md`
- Modify: `CHANGES.md`

The 12 screens (verified by grep, do not re-derive):
`screens/builtin/calibration/{color,gamut,grey,tone}/screen.svg`, `screens/builtin/default/screen.svg`, `screens/examples/demo/font/{bitmap,ttf}/screen.svg`, `screens/examples/{gphoto,hello,mandelbrot,swiss-departure-board,webscrape}/screen.svg`.

- [ ] **Step 1: Reduce the partial to what still works**

`shape-rendering: crispEdges` is a real SVG property and survives. The `-resvg-hinting-*` properties do not exist on `byonk-base`. Replace `byonk-base/v1/hinting.svg` with:

```
{# Retained for compatibility: hinting is now applied automatically by the
   server, adaptively per panel, and no longer needs anything in the document.
   Override it from Lua with the `font_hinting` directive.

   Only `shape-rendering` remains, which is a standard SVG property. #}
shape-rendering: crispEdges;
```

Keeping the include working rather than deleting it means an out-of-tree screen that includes it does not break.

- [ ] **Step 2: Verify no screen's rendering changed**

Run: `./tools/capture-renders.sh /tmp/byonk-renders/state3a` and diff against the Task 7 state.
Expected: **no differences**, because `adaptive_default` reproduces exactly what the partial emitted. Any difference means `adaptive_default` does not match the old partial — fix `adaptive_default`, not the screen.

- [ ] **Step 3: Remove the includes**

Delete the `{% include "byonk-base-v1/hinting.svg" %}` line from each of the 12 screens **only where the surrounding CSS rule still has other declarations**. Where the include was the rule's only content, delete the whole rule.

Watch for screens that relied on `shape-rendering: crispEdges` — those must keep it, written out literally.

- [ ] **Step 4: Re-capture and diff**

Run the capture and diff against Step 2's output.
Expected: differences **only** on screens that lost `crispEdges`. If one appears, put the property back literally. Open every changed PNG.

- [ ] **Step 5: Rebuild the hinting demo as variants**

`screens/examples/demo/font/hinting/` is a 9-cell engine × target grid over a single font, varying hinting per CSS class — which the resolver cannot do, since it is keyed per font. Port it to nine font variants: nine `font_hinting.variants` entries, all `font = "Outfit"`, one per engine × target combination, and nine `font-family` values in the template.

This is the demo that proves variants work end to end. If it cannot be rebuilt, variants do not deliver what Task 6 claims — stop and raise it.

- [ ] **Step 6: Render the demo and look at it**

Run: `CONFIG_FILE=tools/capture-config.yaml cargo run --release -- render --mac AA:BB:CC:00:00:08 --output /tmp/hinting-demo.png`
Then **open the PNG and give the owner the path.** The nine cells must be visibly different from each other. Nine identical cells is a passing render and a broken feature — this is exactly the failure the standing rulings warn about.

- [ ] **Step 7: Write the docs**

`docs/src/reference/font-hinting.md` covering: what the server does automatically and why it is adaptive; the full `font_hinting` directive with every field; variants, with the nine-cell demo as the worked example; the limitation that a bitmap strike is only ever used at the size it was drawn for, and that the `fonts` global is how a script discovers those sizes.

Then the upgrade notice, prominent:

> **Upgrading:** `byonk-base-v1/hinting.svg` no longer emits hinting properties —
> hinting moved into the server and is applied automatically. Screens including it
> keep working and render identically; the include can simply be deleted. Screens
> that set `-resvg-hinting-*` properties directly must move that configuration into
> the `font_hinting` Lua directive, as those properties no longer exist.

Add it to `docs/src/SUMMARY.md`. In `docs/src/tutorial/svg-templates.md` and `docs/src/tutorial/first-screen.md`, leave `sans-serif` as-is — Task 2 made it work — but note it resolves to Outfit.

- [ ] **Step 8: Build the docs**

Run: `make docs` (needs `mdbook-mermaid`).
Expected: builds clean, new page present.

- [ ] **Step 9: Changelog and commit**

```markdown
- `byonk-base-v1/hinting.svg` no longer emits hinting properties; hinting is applied
  by the server instead. Screens including it render identically and the include can
  be removed. Screens setting `-resvg-hinting-*` properties directly must move that
  configuration into the `font_hinting` Lua directive.
```

```bash
git add byonk-base/v1/hinting.svg screens/ docs/src/reference/font-hinting.md \
        docs/src/SUMMARY.md docs/src/tutorial/svg-templates.md \
        docs/src/tutorial/first-screen.md CHANGES.md
git diff --cached --stat
git commit -m "feat: migrate screens off the hinting partial and document the directive"
```

Before committing a screen whose visible text changed, run `grep -rn "<old label>" src/ tests/` — two tests assert screen labels literally.

---

### Task 9: State 3, the pixel diff, and the manual assessment

**Files:** none modified. This is a verification task with an owner checkpoint.

- [ ] **Step 1: Capture state 3**

Run: `./tools/capture-renders.sh /tmp/byonk-renders/state3`

- [ ] **Step 2: Diff against state 2b**

State 2b is the post-Task-2 pre-resvg state, which isolates the resvg change from the generic-family change.

```bash
for f in /tmp/byonk-renders/state2b/*.png; do
  b="$(basename "$f")"
  cmp -s "$f" "/tmp/byonk-renders/state3/$b" || echo "CHANGED: $b"
done
```

- [ ] **Step 3: Look at every changed screen, and hand them to the owner**

For each changed screen, open both PNGs side by side at 1:1. resvg 0.48.0 fixed glyph advances, absolute-transform inheritance, and double-applied transforms — small text shifts are expected and fine. Text overflowing its box, colliding, or vanishing is not.

**Give the owner the file paths and say what changed.** Do not summarise from a downscaled view; the standing rulings record that judging a render from a downscaled PNG has produced wrong conclusions here before.

- [ ] **Step 4: Check the non-deterministic screens by eye**

`/tmp/byonk-renders/state3/nondeterministic/` cannot be diffed. Open each and confirm it looks right. Note in the report that these were assessed visually, not diffed — a silent gap in coverage reads as coverage.

- [ ] **Step 5: Full gate**

Run (background): `CARGO_BUILD_JOBS=2 make check`, then `git push` and `gh pr checks 30 --watch`.
Expected: green.

---

### Task 10: Fix or delete `test_bitmap_font_render`

`src/rendering/svg_to_png.rs:470` renders, writes a PNG to a hardcoded `/tmp` path, prints its size, and **asserts nothing**. It looks like coverage during exactly the change where coverage matters. The hardcoded `/tmp` is a second problem: the release image is `FROM scratch` and has no `/tmp`.

- [ ] **Step 1: Give it a real assertion**

Replace the print-and-write with an assertion that the bitmap font actually rendered — render the same text once with X11Helv at a size the font carries a strike for and once at a size it does not, and assert the two outputs differ. That is the behaviour #1115 defines, and it fails if strikes stop working.

If a meaningful assertion cannot be written, **delete the test** and say so — an honest gap beats a fake one.

- [ ] **Step 2: Remove the `/tmp` write**

Delete it, or use `tempfile` (already a dev-dependency) if the file is genuinely needed.

- [ ] **Step 3: Verify the test fails when the feature is absent**

Temporarily make `select_bitmap` return `false` for every font, run the test, confirm it FAILS, then revert. A test that passes with the feature disabled proves nothing — this is the check that caught a vacuous test on the resvg side.

- [ ] **Step 4: Commit**

```bash
git add src/rendering/svg_to_png.rs
git commit -m "test: assert what test_bitmap_font_render was only printing"
```

---

### Task 11: The hinted font trio — render specimens and put the decision to the owner

This is the question the owner tabled until `byonk-base` landed. It is a **decision task, not an implementation task**: it produces specimens and a recommendation. Bundling the chosen trio is separate work, planned after the owner decides.

**What is already established — do not redo it:**

- **IBM Plex is disqualified**: Google Fonts has no variable Serif or Mono for it.
- Three complete variable superfamilies exist in `google/fonts`:

| Trio | Families | Total | Axes |
|---|---|---|---|
| **Source** | Source Sans 3 / Source Serif 4 / Source Code Pro | 2.0 MB | `wght`, `opsz` on serif |
| **Noto** | Noto Sans / Noto Serif / Noto Sans Mono | 5.5 MB | `wdth,wght` on all three |
| **Roboto** | Roboto / Roboto Serif / Roboto Mono | 4.5 MB | inconsistent; serif alone 3.9 MB |

- Those are the **real family names**, not the file names — getting this wrong cost a render iteration once already.
- `fonts/` is already 11 MB, dominated by the X11 bitmaps.
- Outfit is byonk's house sans, referenced **by name** in screens, docs and `content_pipeline.rs`. Keeping Outfit named-only is the low-risk default whatever the trio.
- An **unhinted** reading was done and the owner rejected it as a basis for deciding: *"we don't want unhinted we want hinted, especially in the mono case this is important."* Hinting helps most at exactly these sizes and can reorder the ranking.

- [ ] **Step 1: Fetch the nine fonts**

```bash
cd /private/tmp/claude-501/-Users-oetiker-checkouts-byonk/6b605fbb-3037-43ac-b94d-fbc35c32f407/scratchpad
git clone --depth 1 --filter=blob:none --sparse https://github.com/google/fonts.git gfonts
cd gfonts && git sparse-checkout set ofl/sourcesans3 ofl/sourceserif4 ofl/sourcecodepro \
  ofl/notosans ofl/notoserif ofl/notosansmono \
  apache/roboto apache/robotoserif apache/robotomono
find . -name "*[VF]*.ttf" -o -name "*wght*.ttf" | sort
```

- [ ] **Step 2: Build the specimen generator**

A temporary `#[cfg(test)] mod` in `src/rendering/svg_to_png.rs` that, for each of the nine fonts, renders a specimen at **10, 12, 14, 17 and 20 px** on an 800×480 panel dithered to pure black and white, via `SvgRenderer::with_fonts(...)` and `render_to_palette_png(svg, spec, &[(0,0,0),(255,255,255)], None, false, None, None, Some(&cfg))`.

Two things that are easy to get wrong and both invalidate the comparison:

1. **Pin the weight.** Put `style="font-variation-settings: 'wght' 400"` on every specimen. Without it a variable font renders at its default instance — Source Code Pro in particular came out far too light last time.
2. **Render hinted**, with the BW configuration byonk actually ships: `FontConfig::adaptive_default(2)` — mono target, symmetric off, preserve-linear-metrics on, normal mode, auto engine.

Also render each specimen **unhinted** alongside, so the owner can see what hinting is contributing rather than only the end state.

- [ ] **Step 3: Render and verify the specimens are not degenerate**

Confirm the hinted and unhinted renders of the same font **differ**. If they are identical, hinting is not reaching the specimen renderer and every conclusion drawn from them would be worthless.

- [ ] **Step 4: Give the owner the paths and a recommendation**

Write the specimens to a stable directory, list the paths, and state a recommendation with its reasoning — legibility at 10–14 px first, since that is where byonk's screens live, then elegance at 17–20 px, then the `fonts/` size cost. **Do not decide alone**; the owner asked to see them.

- [ ] **Step 5: Revert the specimen generator**

It is a throwaway. Delete the temporary test module and confirm `git status` is clean before finishing. This exact module was written and reverted once before.

---

## Follow-ups deliberately not in this plan

- **Bundling the chosen trio** and repointing the generic families at it. Blocked on Task 11's decision. It will revisit Task 2's interim `Outfit`/`Terminus` mapping.
- **Upstream's `wdth`/`ital`/`slnt` bug** — resvg always pushes `wght` but pushes the others only when non-default, so a font whose default instance is `wdth 75` stays condensed for markup saying `font-stretch: normal`. Recorded in the `byonk-base` spec as a genuine upstream bug and out of scope.
- **Splitting `src/rendering/svg_to_png.rs`.** It is large and getting larger, but a split would collide with PR #30's diff. Worth doing after #30 merges.
- **PR #30's own remaining items:** re-reading the whole `CHANGES.md` Unreleased section as a unit before merge, and the two overstated test names in `dither/mod.rs`. Separately, `for_error_diffusion()` is applied to *every* dither in `api/builder.rs`, so HyAB and its `kchroma = 10` tuning are not on the crate's dithering path at all — documented, not changed, and a live design question for the owner.
