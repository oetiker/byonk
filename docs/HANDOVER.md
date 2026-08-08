# Handover — Byonk

_Last updated: 2026-08-08 (session 5) — **Gamut mapping: Task 9 done, plus a new Task 9b that fixed a real Task 8 defect Task 9 exposed.** Resume by executing Task 10. `feat/screen-store-authoring-core` remains **HELD** by standing owner decision — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `dcfcfba` |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` fully green, tree clean |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` (approved) |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

## Next action

**Resume the plan at Task 10** using the `superpowers:subagent-driven-development`
skill. Read the ledger first — it records ten owner rulings the plan text alone
does not explain, and it is the recovery map after a compaction.

Per-task loop, now proven eight times: `scripts/task-brief` → **pre-flight the
brief's code yourself** → dispatch implementer → **verify the build yourself,
and diff the landed code against something you validated** →
`scripts/review-package` → dispatch task reviewer → resolve the ⚠️ items and
escalate plan-mandated findings → `scripts/review-package` again for the fix →
scoped re-review → ledger line → next task. All three scripts live in the
skill's directory.

**Task 10 has a known defect in its plan text, already diagnosed:** its test
SVGs use `r#"…"#` around `fill="#ffffff"`, `fill="#c06020"` and `fill="#ff00aa"`.
The sequence `"#` terminates a `r#"…"#` raw string, so the test module is a
syntax error as written. Fix it to `r##"…"##` during Task 10's pre-flight. This
is the third time this exact defect has appeared (Task 8's six literals, Task
9's two, and once while authoring Task 9b's assertions).

**Task 10 must also remove the `#[allow(dead_code)]` on `rasterize_tone_mask`**
(`src/rendering/svg_to_png.rs`). It exists only because Task 9 left the method
uncalled in the library build; once Task 10 wires it into
`render_to_palette_png` it becomes a stale lint suppression.

## What landed

| Commit | What |
|---|---|
| `7bfe866` | **Task 1** — `Oklch` promoted to `color::Oklch` (`pub`) |
| `2916286` | **Task 2** — `gamut::hull`, incl. decline-to-map when the hull misses the grey axis |
| `8db9e45` | **Task 3** — `gamut::cmax`, `CmaxTable` |
| `284f645` | **Task 5** — `gamut::adapt`, `adaptation_factor` |
| `0d1185b` | **Task 4** — `gamut::knee`, `compress_chroma` |
| `b5c8b36` | **Task 6** — `GamutMapper`, `GamutOptions` |
| `7cd2395` | **Task 7** — `best_reachable` repair + `CmaxTable` oracle validation |
| `3a496ba` | **Task 8** — `src/rendering/tone_mask.rs`, the SVG tone-mask rewriter |
| `2f67d34` | **Task 8 fix** — case/whitespace-proof CSS stripping + inline-style write |
| `57bb440` | **Task 9** — `rasterize_tone_mask`, sharing the frame's exact `fit_transform` |
| `9b1d3e7` | **Task 9b** — stroke-evidence stack: the mask must not invent a stroke |
| `4a53c09` | **Task 9b fix 1** — restore case-sensitive attribute keys (`viewBox`!) |
| `dcfcfba` | **Task 9b fix 2** — module docs: three known mis-marking cases |

Plan amendments: `aa2615f`, `b986caf`, `0d7053d`, `3fd9ab8`, `03eb802`,
`f6f263d`, `636a219`, `ba8859c`, `9669ea9`, `297b10a`.

Public surface: `eink_dither::{Oklch, GamutMapper, GamutOptions}`;
`gamut::hull::{Hull, HullShape}`; `gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma`. Byonk side:
`rendering::tone_mask::{TONE_ATTR, TONE_GROUP_ATTR, has_tone_markup, build_mask_svg, ToneMaskError}`
and the private `SvgRenderer::{fit_transform, rasterize_tone_mask}`.
Shared test fixtures are `gamut::test_support::{six_colour, four_grey}` (inline
in `gamut/mod.rs` under `#[cfg(test)]`) — import them, never copy them.

## ⚠️ The lesson, now proven four sessions running

**The plan's code and constants are not evidence.** Never justify a value with a
threshold from the same unvalidated plan. Measure the real domain first: a
throwaway probe under the scratchpad, or a temporary test applied to the tree
and then reverted, settles these questions in minutes.

- **Session 3:** measure before believing your own diagnosis. Task 7's failure
  was diagnosed, re-diagnosed and finally inverted.
- **Session 4:** measure before believing a reviewer's "harmless". Task 8's
  reviewer's premise was false and the gap was real.
- **Session 5 adds three, all of which paid:**
  1. **Measure before believing a reviewer's "correct", too.** Task 9's reviewer
     argued the transforms "cannot drift" because `fit_transform` is shared.
     True but insufficient — the *inputs* are `tree.size()` of two different
     documents. Probing that found the stroke defect below.
  2. **Diff landed code against something you independently validated.** Task
     9b's implementer slipped in one undeclared line that made the mask
     rasterize **empty**, with all 27 tests passing. Only the diff-against-
     reference caught it.
  3. **Mutation-test a regression test before trusting it.** Re-introduce the
     defect and confirm that test — and only that test — fails.

**Pre-flighting the brief keeps paying.** Task 9's plan code had three defects
(raw strings, dead code, a stale count); Task 8's had five.

## The ten owner rulings — carry these forward

1. **Task 3 — tolerance belongs in the test, not the table** (`aa2615f`). The
   test backs off by `(0.08 * c).max(0.0015)`. `max_chroma` keeps its
   `l <= 0.001` guard because the Oklab→linear cubic map degenerates at black.

2. **Task 4 — the exponential shoulder was clipping real pixels**
   (`b986caf`, `0d7053d`). Replaced by the **ACES 1.3 RGC `powerP` curve,
   `t/(1+t^p)^(1/p)`, at `SHOULDER_POWER = 1.2`**.

3. **Task 5 — neither side of the plan was right** (`0d7053d`). Now
   `select_nth_unstable_by(idx, |a, b| a.total_cmp(b))` — O(n), a genuine total
   order, NaN/∞ discarded by the same percentile guard as any outlier.

4. **Knee default 0.6 → 0.8** (`3fd9ab8`). Measured: knee 0.6 renders a frame's
   vivid end at 82.4% of achievable chroma vs 91.2% at 0.8. 0.8 also sits in the
   ACES threshold band. Task 13's sweep can still overrule this.

5. **Clamp linear RGB to `[0,1]` before `Srgb::from`** (`03eb802`).
   `color::lut::linear_to_srgb` carries an **epsilon-free
   `debug_assert!(0.0..=1.0)`**, so an unclamped conversion behaves identically
   in release but **panics under `cargo test`**. **Any task converting `Oklch`
   back to `Srgb` must clamp.** Now a Global Constraint.

6. **Task 7 — the oracle was broken, not the table** (`f6f263d`). `best_reachable`
   had a real coordinate-descent trap at near-black; `cmax.rs` is correct and
   untouched, **Task 3 stands**. Thresholds are now ratios:
   `IN_LIMIT_MAX_RATIO = 0.05` (3.9× margin), `BEYOND_LIMIT_MIN_RATIO = 0.3`
   (1.53× margin). `d_out` is a 3-D distance whose nearest hull point need not be
   radial, so that check only establishes "not on the hull" — keep it generous.

7. **Task 8 — strip CSS paint case-insensitively and write the inline style too**
   (`ba8859c`).

8. **Task 9b — stroke-evidence stack, fixed before Task 10** (`297b10a`,
   session 5). See below.

9. **Task 9 — `#[allow(dead_code)]`, not `#[expect]`** (`9669ea9`). Under
   `--all-targets` the `cfg(test)` build *does* use `rasterize_tone_mask`, so an
   `#[expect]` goes unfulfilled, which is itself a warning. Task 10 removes it.

10. Standing: **the branch is HELD** — no PR, no merge to `main`.

**Constants still inherited from the plan and never challenged:**
`max_compression = 2.5`, `PERCENTILE = 0.99`, `MIN_DISCARD = 32`,
`HUE_BINS = 128`, `LIGHTNESS_BINS = 64`, `C_SEARCH_HI = 0.5`.

## Session 5's main finding — the mask was inventing strokes

Task 9 is the first code that **rasterizes** a mask, and that immediately
exposed a Task 8 defect no rewriter test could see: every `tone_mask` test
asserts on the mask **document text**, and this is only visible in pixels.

`rewrite_start` set `stroke` unconditionally to the tone paint. SVG's initial
`stroke` is `none`, so **every marked shape that declared no stroke gained one**.
Measured on a shape spanning `x = 50..=149` at a 200×200 spec:

| document | before | after |
|---|---|---|
| plain unstroked rect | `50..=150` | `50..=149` |
| `<style>.p{stroke:none;stroke-width:20}</style>` | `40..=159` | `50..=149` |
| **real `stroke-width="20"`** (control) | `40..=159` | `40..=159` |

Not sub-pixel and **not bounded**: `stroke` is a paint property so an author's
`stroke: none` is stripped from stylesheets, while `stroke-width` is preserved
as geometry, giving an error of `stroke-width / 2`. The harmful direction is an
**unmarked** shape over a marked photo — it gains a *black* stroke and erodes
the photo mask, an unmapped band around every label on a photo.

The fix is a `Stroke` evidence stack mirroring the tone stack. Two traps it must
keep avoiding, both now covered by tests:

- **Stroke-only shapes** (`<line>`, `<path fill="none" stroke="…">`) have no fill
  area; dropping their stroke erases them from the mask entirely.
- **Inherited stroke** — `<g stroke="black"><line/></g>`: writing an explicit
  `stroke="none"` on the line would override the inherited paint and erase it.

A stroke set **only** by a stylesheet is lost and that element *under*-marks —
the deliberate fail-safe direction, now the third case in the module docs'
"Known mis-marking" section (renamed from "Known over-marking" in `dcfcfba`,
since the third case shrinks rather than grows).

Also folded in: `fill_none` detection moved to the shared `declaration_value`
helper, because `value.contains("fill:none")` missed `fill : none` and
`FILL: NONE` — the same defect class as ruling 7.

### The `viewBox` near-miss — read this before trusting a green suite

Task 9b's implementer changed one undeclared line in `rewrite_start`:

```rust
-  let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
+  let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
```

`key` is **re-emitted** for every non-paint attribute. XML attribute names are
case-sensitive, so this renamed `viewBox` → `viewbox`, `preserveAspectRatio`,
`gradientUnits`, `patternTransform` and friends. Measured with a `viewBox` that
differs from `width`/`height`: usvg lost the coordinate system and **the mask
rasterized completely empty** — the whole photo region silently unmapped.

**All 27 tests passed on that code**, because every existing rasterization test
used a `viewBox` whose numbers equalled its `width`/`height`. `dcfcfba` carries
`tone_mask_preserves_camelcase_attributes`, which uses a deliberately mismatched
`viewBox` and was mutation-tested.

Note the asymmetry that caused it: CSS *property* names are case-insensitive
(hence `is_paint_declaration` lowercases), but SVG *attribute* names are
case-sensitive. Lowercasing for comparison inside `resolve_stroke` is fine;
lowercasing a key that gets re-emitted is not.

## Verified quick-xml 0.41 facts — do not re-probe

`Reader::from_reader(&[u8])`, `config_mut().check_end_names = true` (does detect
`<svg><g></svg>`), `read_event_into`, `Writer::new(Cursor::new(Vec::new()))`,
`.into_inner().into_inner()`, `attributes().with_checks(false)`,
`Attribute::from((&str, &str))`. `BytesText::unescape()` was **removed** — use
`xml10_content()`; `decode()` alone does not unescape. **Attribute values
round-trip without double-escaping** — measured.

## What remains

10. Wire into `render_to_palette_png` between rasterization and dithering
11. `GamutTuningValues` + the Lua `gamut` table
12. Thread the knobs through eight structs on the display path
13. Regression metrics + the visual golden — **a real stop point**
14. Docs + `CHANGES.md`

**Task 13 needs the owner.** It renders the calibration screen with and without
mapping and asks whether the marker stays. Mean dE is *expected to worsen*;
present both images rather than deciding alone. Its sweep covers knee
0.4/0.6/0.8 and can overrule ruling 4; it is also the right place to A/B
`SHOULDER_POWER`.

## Deferred minors — triage list for the final whole-branch review

- **Task 6:** `map_color`'s `[0,1]` clamp deviates from the plan's literal code.
  Reviewer: justified, disclosed, correct place for the boundary. Now ruling 5.
- **Task 7:** the investigation's winning dilute start was `eps = 0.005`, between
  the shipped ladder's `0.003` and `0.01` rungs. Adding it would tighten the
  witness and the worst in-limit ratio. Purely optional.
- **Task 8:** the `Event::CData` stylesheet branch is live but untested.
- **Task 8:** `strip_paint_declarations`/`_inline` split naively on `;` and `:`,
  mis-tokenizing values with embedded colons or semicolons (data URIs,
  `content: "a;b"`). Traced: the failure mode is always "left untouched" (safe).
- **Task 8:** `resolve_tone` drops attribute-iteration errors via `.flatten()`
  while `rewrite_start` propagates them. No observable effect. Style wart only.
- **Task 8:** element names are matched as raw bytes (`b"image"`, `b"defs"`,
  `b"style"`), so namespace-prefixed elements (`<svg:image>`) would be
  mis-handled, and `<symbol>` gets no `<defs>`-style paint stripping. Neither
  appears in any screen today — dormant. **Note this is the same case-and-name
  matching family as the `viewBox` near-miss; worth one deliberate pass.**
- **Task 8:** `image_to_rect` never inspects a `style` on the source `<image>`,
  so `style="display:none"` there would hide the real element while the mask
  rect still paints it.
- **Task 9b:** `resolve_stroke` only sees presentation attributes and inline
  styles. A stroke set solely by a stylesheet rule under-marks. Deliberate and
  documented; revisit only if a real screen hits it.
- Two pre-existing rustdoc warnings in `eink-dither` (private intra-doc link to
  `apply_error`; unresolved link to `with_distance_metric`). Not from this work.
- `gamut/hull.rs` uses three different epsilon tolerances. Each is locally
  defensible; the reviewer wants a comment saying they are deliberately different.
- `adapt.rs`: `max_compression < 1.0` collapses `R` to `1.0` regardless of
  content — defensible, but untested and unmentioned in the plan.
- `adapt.rs`: no test exercises literal `NaN` input (only `INFINITY`).

## ⚠️ Read this before trusting any dithering measurement

**Flat-patch dE is actively misleading.** A flat patch is a single colour; every
artifact that matters is at a boundary *between* colours. In the previous
initiative, every arm that improved patch dE made the rendered image worse.
Render the field and look.

- `cargo test -p eink-dither --test visual_compare -- --ignored --nocapture`
  writes pairs, crops and triptychs to `target/dither-compare/`.

**IDE diagnostics lie in this tree.** They have repeatedly reported unresolved
imports and missing functions in a tree that built cleanly. Verify with an
actual `cargo` run. Equally, **do not take a subagent's "all green" at face
value** — every task so far was independently re-verified by the controller, and
that re-verification has caught real issues repeatedly, including session 5's
empty-mask regression.

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **Pass `timeout: 600000`** — it exceeds the Bash
  tool's 120 s default and gets auto-backgrounded otherwise.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. Never `git add -A`.
- **byonk lib suite is 437 tests** (+1 ignored), `tone_mask` alone is 28.
  Base before session 5 was 427; Task 9 added 2, Task 9b added 8.
  (The previous handover's figure of 424 was wrong — re-measure, don't inherit.)
- `make docs` needs `mdbook-mermaid`.
- **`cargo test -p eink-dither --lib -- --ignored` takes ~5 minutes** and
  currently reports **3 pre-existing failures unrelated to this work**:
  `preprocess::preprocessor::tests::{test_process_with_resize,
  test_resize_before_enhancement, test_resize_full_pipeline_with_photo_preset}`
  panic at `preprocess/resize.rs:26`. `resize_lanczos()` panics **by design**
  ("resize not available (image crate removed)"); `resize.rs` was last touched
  in `8b52e62`, long before this initiative. Do not mistake these for a
  regression — but they are dead tests guarding a dead code path and deserve
  their own cleanup someday.
- **`Dockerfile` is broken independently** — it copies `Cargo.toml`,
  `Cargo.lock` and `src/` but never `crates/`, so the workspace cannot resolve
  `eink-dither`. Releases are unaffected (`Dockerfile.release`, CI-built
  binaries). Out of scope, untouched.

## Open dithering defects — independent of this work

Gamut mapping is the identity on in-gamut targets, so it neither fixes nor
worsens these:

1. **Dark warm under-mixing.** At 45° L0.32 black lands at 1% against an
   optimal 47%. In gamut, bound 0.000, so no gamut excuse.
2. **Scalloped arcs at ink-set boundaries.** Survives every kernel and noise
   scale. **No working hypothesis.**
3. **Flat fills collapse to one ink** — `#C06020` renders solid red.

The selector work that tried to fix these is **three-for-three refuted** and
should not be resumed without a new idea; `crates/eink-dither/tests/spike_simplex.rs`
is the deliberate record of what does not work. `AtkinsonHybrid` remains an
unlanded candidate that beats Atkinson on both axes — changing the default
alters rendering for every device, so it is the owner's call.
