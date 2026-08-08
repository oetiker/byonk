# Handover — Byonk

_Last updated: 2026-08-08 (session 4) — **Gamut mapping: Tasks 1–8 landed and reviewed clean.** Resume by executing Task 9. `feat/screen-store-authoring-core` remains **HELD** by standing owner decision — no PR, no merge to `main`._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `2f67d34` |
| Worktree | `/Users/oetiker/checkouts/byonk` (working in place, no worktree) |
| State | `make check` fully green, tree clean |
| Plan | `docs/superpowers/plans/2026-08-08-gamut-mapping.md` |
| Spec | `docs/superpowers/specs/2026-08-07-gamut-mapping-design.md` (approved) |
| Ledger | `.superpowers/sdd/2026-08-08-gamut-mapping/progress.md` (git-ignored) |

## Next action

**Resume the plan at Task 9** using the `superpowers:subagent-driven-development`
skill. Read the ledger first — it records eight owner rulings that the plan text
alone does not explain, and it is the recovery map after a compaction.

Per-task loop, now proven six times: `scripts/task-brief` → **pre-flight the
brief's code yourself** → dispatch implementer → **verify the build yourself** →
`scripts/review-package` → dispatch task reviewer → resolve the ⚠️ items and
escalate plan-mandated findings → `scripts/review-package` again for the fix →
scoped re-review → ledger line → next task. All three scripts live in the
skill's directory.

Task 9 rasterizes the mask and **must share the frame's exact fit transform** —
that sharing is the whole correctness requirement; a mask rasterized with a
different transform is silently misaligned.

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

Plan amendments: `aa2615f`, `b986caf`, `0d7053d`, `3fd9ab8`, `03eb802`,
`f6f263d`, `636a219`, `ba8859c`.

Public surface: `eink_dither::{Oklch, GamutMapper, GamutOptions}`;
`gamut::hull::{Hull, HullShape}`; `gamut::cmax::{CmaxTable, HUE_BINS, LIGHTNESS_BINS}`;
`gamut::adapt::{adaptation_factor, PERCENTILE, MIN_DISCARD}`;
`gamut::knee::compress_chroma`. Byonk side:
`rendering::tone_mask::{TONE_ATTR, TONE_GROUP_ATTR, has_tone_markup, build_mask_svg, ToneMaskError}`.
Shared test fixtures are `gamut::test_support::{six_colour, four_grey}` (inline
in `gamut/mod.rs` under `#[cfg(test)]`) — import them, never copy them.

## ⚠️ The lesson, now proven three sessions running

**The plan's code and constants are not evidence.** Never justify a value with a
threshold from the same unvalidated plan. Measure the real domain first: a
throwaway probe crate under the scratchpad with a path dependency on the real
crate settles these questions in minutes, and has done so repeatedly.

**Session 3 added: measure before believing your own diagnosis.** Task 7's
failure was diagnosed, re-diagnosed and finally inverted; three plausible
hypotheses were each refuted by measurement before the true cause emerged.

**Session 4 added: measure before believing a reviewer's "harmless", too.**
Task 8's reviewer judged a gap "very likely functionally harmless" with a
plausible argument. Measurement showed the premise was false and the gap was a
real silent-corruption path. A review conclusion is a claim like any other.

**Session 4 also proved pre-flighting the brief pays.** Task 8's plan code did
not compile at all. Extracting the brief's code into a probe crate before
dispatching caught four defects in minutes — including one silent-corruption
bug — and the implementer then landed it byte-identical on the first pass.
**Do this for every remaining task.**

## The eight owner rulings — carry these forward

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
   (`ba8859c`, session 4). See below.

8. Standing: **the branch is HELD** — no PR, no merge to `main`.

**Constants still inherited from the plan and never challenged:**
`max_compression = 2.5`, `PERCENTILE = 0.99`, `MIN_DISCARD = 32`,
`HUE_BINS = 128`, `LIGHTNESS_BINS = 64`, `C_SEARCH_HI = 0.5`.

## Task 8 in full — session 4's main finding

The plan's Task 8 code **did not compile**, and had a silent-corruption bug.
Four defects found by pre-flight (`636a219`), then one more by review plus
measurement (`ba8859c`):

- **Raw-string delimiters.** SVG literals contain `"#` (`fill="#ff0000"`,
  `href="#sym"`), which terminates `r#"…"#`. Six literals need `r##"…"##`. The
  delimiters are load-bearing — a note in the plan says so.
- **quick-xml 0.41 removed `BytesText::unescape()`** → use `xml10_content()`.
  `decode()` alone does not unescape.
- **The `<defs>` test always failed** — it scanned the whole document tail,
  which legitimately contains `fill="#000000"` on the following `<use>`.
- **Start-form `<image>…</image>` was never replaced**, only the self-closing
  form, so the real photograph would have reached the mask document and
  thresholded into an arbitrary mask. Now replaced with its subtree dropped
  (`image_skip_depth`). All `<image>` uses in the tree today are self-closing,
  so this was latent.
- **CSS stripping missed legal variants.** Measured over 11 declaration forms:
  `FILL: red` and `Fill: red` survived (property names are case-insensitive),
  and `fill : red` / `fill\t: red` / `fill\n: red` survived (whitespace before
  the colon is legal; `rsplit(whitespace).next()` yields `""` there). A survivor
  beats the presentation attribute and **inverts that element's mask polarity**.
  Fixed by trimming and lowercasing the property name, **and** by writing the
  paint into the inline `style` as Deviation §3 always promised.

**Verified quick-xml 0.41 facts — do not re-probe:** `Reader::from_reader(&[u8])`,
`config_mut().check_end_names = true` (does detect `<svg><g></svg>`),
`read_event_into`, `Writer::new(Cursor::new(Vec::new()))`,
`.into_inner().into_inner()`, `attributes().with_checks(false)`,
`Attribute::from((&str, &str))`. **Attribute values round-trip without
double-escaping** — reading the raw escaped value and re-pushing leaves
`a&amp;b&lt;c` byte-identical. Measured.

## What remains

9. Rasterize the mask, sharing the frame's exact fit transform
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
- **Task 8:** the `Event::CData` stylesheet branch is live but untested (no
  example screen uses CDATA stylesheets).
- **Task 8:** `strip_paint_declarations`/`_inline` split naively on `;` and `:`,
  mis-tokenizing values with embedded colons or semicolons (data URIs,
  `content: "a;b"`). Traced: the failure mode is always "left untouched" (safe),
  never corrupted output.
- **Task 8:** `resolve_tone` drops attribute-iteration errors via `.flatten()`
  while `rewrite_start` propagates them. No observable effect — the same
  malformed attributes error out in `rewrite_start`, so `build_mask_svg` still
  returns `Err`. Style wart only.
- **Task 8:** element names are matched as raw bytes (`b"image"`, `b"defs"`,
  `b"style"`), so namespace-prefixed elements (`<svg:image>`) would be
  mis-handled, and `<symbol>` gets no `<defs>`-style paint stripping. Neither
  appears in any screen today — dormant.
- **Task 8:** `image_to_rect` never inspects a `style` on the source `<image>`,
  so `style="display:none"` there would hide the real element while the mask
  rect still paints it — over-marking beyond the two cases Deviation §4
  documents. Obscure but real.
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
value**: every task so far was independently re-verified by the controller, and
that re-verification has caught real issues more than once. (Task 8's implementer
reported "63 byonk tests" when the real figure was 424 — understated, harmless,
but exactly why the controller re-runs the gate.)

## Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`. **Pass `timeout: 600000`** — it exceeds the Bash
  tool's 120 s default and gets auto-backgrounded otherwise.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. Never `git add -A`.
- byonk lib suite is **424 tests** (410 + Task 8's 18 − pre-existing overlap);
  `tone_mask` alone is 18.
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
