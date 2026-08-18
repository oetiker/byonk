# Handover — Byonk

_Last updated: 2026-08-18 (session 26). **The resvg `byonk-base` initiative is COMPLETE:
all 10 plan tasks done.** Session 26 landed the last two — Task 9 (the render sweep) and
Task 10 (`test_bitmap_font_render`) — reconciled the changelog, and then, at the owner's
request, extended the sweep to the 16-grey and 6-colour panels, **which found and fixed two
bugs**. **The branch is pushed and `make check` is green. The next job is to merge PR #30.**_

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main` — https://github.com/oetiker/byonk/pull/30 |
| HEAD | `bac40ee` — **tree clean, fully pushed** (`origin` is at `bac40ee`) |
| Verified | `make check` on `bac40ee`'s content: **1138 passed, 0 failed**; clippy clean under `-D warnings` |
| CI | Green on `e7fe213` (all 10 checks). **Four commits landed after that** — re-check with `gh pr checks 30` before merging. |
| Push gotcha | The ssh-agent holds **no identities**, so `git push origin …` fails on publickey. `gh` is authenticated over HTTPS — `git push https://github.com/oetiker/byonk.git <branch>` works and leaves the remote config alone. |

**resvg work happens in a different repo.** `oetiker/resvg` carries `feat/bitmap-mask-glyphs`
(upstream PR #1115), `feat/font-hinting` (upstream PR #1116), and `byonk-base`, which merges
them and is what byonk's `[patch.crates-io]` pins. **Current pin: `2e766508`** (in
`Cargo.lock`; `Cargo.toml` tracks the branch).

**The plan** — `docs/superpowers/plans/2026-08-15-resvg-byonk-base-integration.md` — is now
**spent**. Every task is done. Do not resume from it. Task 11 in that file (the hinted-font
trio decision) was never part of the 10 and is listed under *Queued work* below. **The plan
was wrong in eleven of eleven tasks touched**; if you ever reread it, verify every symbol.

**The ledger:** `.superpowers/sdd/2026-08-15-resvg-byonk-base-integration/progress.md`
(git-ignored). Also there: `f11-report.md`, `f15-report.md`, `font-licensing-research.md`,
`f9-brief.md`, `f10-brief.md`, `f16-probe/`. **Ignore the two `f15-*.patch` files — neither
holds what its name claims.** `git log` on `oetiker/resvg` is the truth for resvg work.

---

# What to do next

1. **Merge PR #30.** `make check` is green locally on `bac40ee`; confirm CI is too (it was
   last verified on `e7fe213`, four commits back). Nothing else is outstanding against it.
   See `superpowers:finishing-a-development-branch` for the integration checklist.
2. **Decide the 16-grey tone question** below — the one thing this session found and did
   *not* resolve.
3. Then pick from *Queued work* below. Nothing there blocks the merge.

## Before merging — one thing already done, one still open

- **`CHANGES.md`'s Unreleased section has now been read as a whole** (session 26, `e7fe213`).
  Three problems were fixed: two entries contradicted each other about the generic font
  families, one framed its change against a mid-cycle interim state instead of against 0.16.0,
  and two entries were filed under *Fixed* that are a new feature and a breaking behaviour
  change. **This item is closed** — do not redo it.
- **Still open:** two overstated test names in `dither/mod.rs`. Not a merge blocker.

---

# Session 26 — what landed

## Task 9 — the render sweep (`6346790`, plus the artifact)

**All 13 bundled screens render correctly.** Nothing overflows, collides or vanishes. Every
one was opened and judged individually. The 7 deterministic screens are **byte-identical
across two consecutive captures**, so the harness's det/nondet bucketing still holds. (The
sweep grew to 19 captures across 5 panels later in the session — see below.)

**Owner artifact (every render embedded, plus the findings):**
https://claude.ai/code/artifact/7e3a6c8d-763d-4985-8f12-69c7d7fdcc99

Three things changed about what a capture proves:

- **The harness was hiding stderr.** `tools/capture-renders.sh` sent every render's stderr to
  `/dev/null`, and since Task 4 that is the *only* channel the render-scale warning uses on
  the CLI. It would have reported a clean sweep for a tree byonk was complaining about. Fixed:
  per-screen `<name>.stderr` beside the PNG, any non-empty one quoted into `MANIFEST.txt`.
- **`BYONK_BIN` now selects the binary.** `BYONK_BIN=./target/debug/byonk
  ./tools/capture-renders.sh <dir>` captures the whole set in **seconds**. The default is
  still `cargo run --release` so a fresh checkout works.
- **The canary flipped, and it is an improvement.** An unresolved screen ref used to fall back
  to DEFAULT and exit 0; `3a35030` made it hard-error. So **`exit=0` now genuinely means "this
  screen rendered"**. The script's header claimed the opposite and was corrected. Keep both
  branches — the manifest should *state* which behaviour was live, not assert one.

**The positive control matters more than the thirteen clean screens.** A scratch screen
authored at `400x240` was driven through the real `byonk render` first; it printed the scale
warning to stderr. Only then is "0 warnings" distinguishable from a dead mechanism. It also
fired on an **exact 2× zoom** — the case the plan's rule would have exempted, so the owner's
"warn on any mismatch" decision is vindicated by a second independent check.

**The planned pixel diff was dropped, with the owner's agreement — plan error #11.** Step 2
wanted state 3 diffed against a `state2b` baseline captured at `d3d410d`. That baseline lived
in `/tmp` and is gone. More importantly it would no longer isolate anything: eight commits
since `d3d410d` change text rendering on *every* screen (generics repointed to the Source
trio, hinting on by default, X11 fonts rebuilt, screens migrated off the hinting partial). The
diff would report 7 of 7 changed with nothing attributable. **Do not resurrect it.**

One thing that looks like a defect and is not: `gphoto` renders a near-blank
*"Device Registered"* screen. That text is inside `screens/examples/gphoto/screen.svg` — it is
gphoto's own "registered but not linked to Google Photos" state, not a fallback. It is also
the one nondet screen that is byte-stable between runs, because that state carries no clock.
Likewise `demo-font-ttf` omits the 32 px bold rows **by design**: `script.lua:58` drops
entries that would not fit rather than overflowing.

## Task 10 — `test_bitmap_font_render` (`58aa3e0`)

The old test rendered X11Helv, wrote a PNG to a hardcoded `/tmp` path and **asserted nothing**.

**The plan's proposed fix was vacuous** and its own Step 3 would have caught it: it wanted the
same text at a strike size vs. a non-strike size, asserting the two differ — but two different
`font-size` values differ whether or not any strike is consulted.

The replacement holds text, size and face constant and toggles the one input `select_bitmap`
actually reads, **`FontVariant::strikes`**, with two controls: `strikes = true` must reproduce
the no-variant baseline byte for byte, and the same flag on an outline font (Outfit) must
change nothing. **Sabotage-verified**: with `select_bitmap` forced to always return `true` the
test fails on the intended assertion; the source was restored and diffed byte-identical
against a backup.

**This closes F15.** Byonk's own suite now fails if the resvg pin regresses.

## The owner asked for the other panels — and that found two bugs (`b7a896f`, `70fbdef`, `bac40ee`)

The sweep above covered `trmnl_og` twelve times and `trmnl_og_4clr` once. **The 16-grey and
both 6-colour panels had never been rendered by anything.** Six pairings are now permanent in
`tools/capture-config.yaml`, so a sweep is 19 captures across 5 panels. The four calibration
screens build from `layout.width`/`layout.height`, which is what makes this cheap.

**Bug 1 — `byonk render` ignored the device's panel size (`b7a896f`).** The first attempt
returned *every* screen at 800×480, including those on 1872×1404 and 1200×1600 panels. The CLI
sized renders only from `--device`, which has two values. The palette half of the chain was
already wired, so an E1004 device rendered **in that panel's six colours at the wrong size**.
Also hit all three 296×128 Xiao panels and any operator-defined panel. **The scale warning
cannot catch this** — a screen built from `layout.*` builds itself to whatever it is told, so
the SVG and the render agree and only the panel disagrees. Now in one pure function,
`resolve_cli_display_spec`, with three tests, sabotage-verified.

**Bug 2 — `calibration/grey` fell apart at 16 entries (`bac40ee`).** One row gave each
`#RRGGBB` label a sixteenth of the width; labels smeared together, circles overlapped, the
leftmost hung off the screen. Swatches now wrap past 8 entries (4×4 at 16, 3×3 at 9); labels
and marks are sized from the *cell*, not the panel (30% of a cell is under one cap height,
which clipped row 0); the block grows to fill the band a tall panel left empty; each swatch is
labelled once instead of printing its hex twice. Rendered at 2, 4, 6, 9, 12, 16.
`tests/calibration_grey_layout_test.rs` guards it by **reading back the geometry the script
computed** — a smeared render is structurally valid, so pixels cannot tell you if text is
legible. Sabotage-verified.

> **`calibration/color` already handled 6 entries cleanly.** If another screen needs the same
> treatment, that is the one to copy.

---

# Open items the owner should decide

## NEW — marking costs shadow detail at 16 grey levels

On `calibration/tone` the marked (measured, mapped) half is supposed to show what gamut
mapping buys. On the 4-grey panel the two halves are close. **On `trmnl_x` the marked half is
markedly darker and loses shadow separation** the unmarked half keeps — visible in the hair and
the left of the face. Capture: `calibration-tone-16grey`, and in the artifact below.

There is a reading where this is honest: `trmnl_x`'s measured palette runs `#383838`–`#B8B8B0`,
a narrow real range, so mapping into it *should* darken. **But the unmarked half dithers
against that same measured palette**, so the gap comes from the mapping, not the inks — and on
a panel with 16 levels, losing shadow separation is the opposite of what the extra levels are
for. **Not diagnosed.** Decide which half is the better preview before touching the mapper.
It only became visible because the harness had never rendered this panel.

## Two TLS tests are flaky

`lua_https_tests::{test_https_with_custom_ca_cert, test_https_with_client_certificate}`.
They fail with `error sending request for url (https://127.0.0.1:…)`, the shape a 30 s timeout
takes. Observed **once in three** full `make check` runs in session 24; sessions 25 and 26 were
clean across three more runs, so it is now **once in six**.

**The null hypothesis has still never been tested** — nobody has run the suite with `c850ea7`'s
HTTP change reverted, so "my change caused it" is unproven.

**The best explanation is now the laptop suspending, not CPU contention.** The owner pointed
out (session 26) that the alarming load averages on this machine — 78 in session 25, 40 in
session 26 — are an **artefact of the device sleeping**, not real contention. That reframes it
usefully: a 30 s timeout is *wall clock*, so a suspend in the middle of a test blows it
instantly no matter how idle the CPU is. That fits a failure that is rare, unreproducible, and
confined to the two tests with the longest network waits. **Before spending anything on a fix,
check whether the failing runs coincide with a sleep** (`pmset -g log | grep -i sleep`, or
`log show --predicate 'eventMessage contains "Wake"'`).

**If it needs fixing, do not loosen the test.** Cache the `reqwest::blocking::Client` instead
of building one per request. The original panic was on *dropping* the client's runtime inside a
tokio context, so a cached client never drops there — that removes the extra thread *and* the
original bug, with less machinery than today. A single shared worker thread is the wrong
answer: it would serialise every screen's HTTP on the server path.

---

# Settled — do not reopen

## Byonk warns when a screen renders at the wrong scale (Task 4)

`SvgRenderer::scale_warning(svg_w, svg_h, spec) -> Option<String>`, next to `fit_transform` and
fed the same numbers so the two cannot disagree. **Owner chose: warn on any size mismatch, no
integer-zoom exemption.** Session 26's positive control re-confirmed it fires on an exact 2×.

`rasterize_svg` fills a **`&mut Option<String>` out-parameter**. Each caller decides:
`ScreenStore::render` → `RenderResult::log` as `[warn] …`; `main.rs` (`byonk render`) →
**stderr**; `api/display.rs`, `api/dev.rs`, `render_to_raw_png` and `content_pipeline`'s
internal call → `&mut None`.

**Authoring warnings reach the author, not the operator.** `tracing::warn!` was rejected: it
reaches the server log, not the screen's author. A new warning of this kind belongs in the
script log sink and on the CLI's stderr — never only in `tracing`.

## Why integer zoom is not a free pass — read from the pinned resvg, not reasoned

Source: `~/.cargo/git/checkouts/resvg-b4a0ccb9ea26de88/2e76650/`

- `crates/usvg/src/text/flatten.rs:283` — `let ppem = glyph.font_size();`, and the
  `GlyphHinting::ppem` doc says it outright: *"Derived from the font size, so hinted glyphs
  only land on whole pixels at an unscaled render."* The hinting ppem is the **user-unit** font
  size; the render transform is not involved.
- `crates/usvg/src/text/flatten.rs:68` — `snap_bitmap_glyph` returns early unless the scale is
  1.0 within `1e-4`. **Bitmap faces lose strike snapping at any zoom**, integer included.
- And the plain one: every dimension the author chose is displayed at the wrong size.

## A variant CAN be aliased. The flag is document-level; the effect is not.

`HintingSpec::to_usvg()` deliberately drops `aliased`, because aliasing reaches usvg through
`Options::text_rendering`, which is document-level. **But `text-rendering` is an ordinary
inheritable SVG property, so the element using a variant asks for it directly.**

Measured, and pinned by `a_mono_variant_plus_optimize_speed_equals_the_document_level_aliased_mono`
(sabotage-verified):

| comparison | result |
|---|---|
| `docsmooth` == `varsmooth` | identical — **control**: loading a 2nd copy of the font is not itself a difference |
| `docmono` == `varmono` | identical — a variant honours `target = "mono"` |
| `docmonoalias` == `varmonoalias` | **identical** — variant + `text-rendering="optimizeSpeed"` reaches full mono+aliased |
| `varmonoalias` != `varsmoothalias` | differ — aliased-without-mono is the known-bad state |

**This is the point of variants**: part of a screen can be made genuinely 1-bit crisp, on a
grey panel as well as a black-and-white one. Always pair `optimizeSpeed` with mono hinting.

## No bundled font carries a hinting program

Measured with fontTools across **every glyph**, not just `fpgm`/`prep`:

| font | glyphs | hinted glyphs | instruction bytes |
|---|---|---|---|
| Outfit, Source Sans 3, Source Serif 4, Source Code Pro | 414–2478 | **0** | 0 |
| Terminus TTF | 1359 | 1 | 46 |
| all X11 faces | 0 outlines | 0 | 0 |

Consequences, all confirmed by render: **`interpreter` is effectively unhinted**,
**`auto` ≡ `auto_fallback`** (byonk's `resolve_auto_fallback` doing its job), and `interpreter`
is *visibly worse* when aliased.

> The engine axis is **not** dead — that earlier call was wrong, an artefact of a demo bug. It
> is the axis that *shows* these facts, and `auto ≡ auto_fallback` is a live check that byonk's
> auto-fallback fix still works.

## Smooth hinting is real but cannot look dramatic

Document `smooth` vs document `off` differ on 35–72% of the ink — but almost entirely as
**grey-level shifts on anti-aliased edges**, not changed coverage. Only aliasing makes hinting
visually obvious. `hinting = false` on a variant is byte-identical to document-level
`font_hinting = false`, so "off" is genuinely off.

---

# Carried forward — still binding

## Owner decisions

1. **Bundle the Source trio** as generic-family fallbacks: `sans-serif` → Source Sans 3,
   `serif` → Source Serif 4, `monospace` → Source Code Pro. **Outfit stays** as the house sans.
2. **No fallback magic.** Designers choose bitmap faces explicitly.
3. **Fonts need licence files** (table below).
4. **Bitmap fonts should have no outlines if possible** — delivered by F16.
5. **F20: status icons own the header corner; the timestamp lives in the footer.**
6. **byonk intervenes only when Lua crashes.** The two fetching examples call `error()` when
   they cannot carry on, which puts the message on the device's error screen
   (`display.rs:1032`) and exits the CLI non-zero.
7. **Authoring warnings reach the author, not the operator** (see *Settled*).

## The Lua surface, as shipped

```lua
font_hinting = false            -- hinting off entirely
font_hinting = {
  engine = "auto",              -- interpreter | auto | auto_fallback
  target = "mono",              -- shorthand for { mode = "mono" }
  -- target = { mode = "mono", aliased = false },
  variants = {
    ["Crisp Body"] = { font = "Outfit", hinting = { target = "mono" } },
  },
}
```

`mode` is the discriminator: mono's extra knob is `aliased`, smooth's are `symmetric` and
`preserve_linear_metrics` (the real field is `symmetric_rendering`). A variant also takes
`strikes = true|false`, which is what `select_bitmap` reads — **that knob is now the basis of
the bitmap regression test, so do not remove it.** A directive that names **only** variants
keeps the panel's adaptive default — pinned by
`naming_only_variants_keeps_the_panel_s_adaptive_default`.

**F1 constraint, binding on anything touching hinting:** aliasing is per-element and
inheritable; hinting is per-face. An element choosing smooth/no hinting on a BW panel inherits
`optimizeSpeed` and lands in the known-bad aliased-without-mono state (tiny-skia has no dropout
control). Escape hatch: **`text-rendering: optimizeLegibility`** — restores AA *and keeps
hinting*. **Trap: `geometricPrecision` restores AA but disables hinting.** Byonk warns, naming
the offending variants, checked against `grey_count = 2`.

## Naming rule for variant aliases

**Name them for their purpose, never `<RealFamily> <TechnicalTerm>`.** `"Outfit Mono"` reads as
a monospaced Outfit; use `["Crisp Body"] = { font = "Outfit", … }`. Byonk enforces the "not a
real family" half at parse time. **Always name the fallback in the document:**
`font-family="'Crisp Body', Outfit"` — and see the CSS trap in *Lessons*.

## F15 / F16 — the bitmap work, done and live

- **The fonts and the resvg pin must move together.** If the pin is rolled back, roll the fonts
  back with it. **Byonk now has a test that fails when this happens** (Task 10).
- **Terminus is NOT buggy. Terminus @14 and @18 render 1 px/glyph wider — that is correct.**
  Raised twice, settled twice.
- **Merge trap:** `byonk-base` has host hooks upstream does not (`FontResolver::select_bitmap`,
  `select_font`). A clean *textual* merge is **not** evidence the semantics survived. Diff the
  merge result against the pre-merge tree.
- **A bitmap face only renders as a bitmap at a size it has a strike for**, and nothing warns
  you. `fonts/FONTS.md` lists the sizes per family.

## Falsified — do not chase again

- **X11 vertical-metric overflow**: real malformation, **not** a cause of anything.
- **Ink overhang in the oblique faces**: slanted bitmap faces overhang normally.
- **F10's two hazards, both FALSE**: the fvar `wght` default does not leak, and Source Serif 4
  is not pinned at `opsz` 20.
- **F9 / `AutoFallback`:** upstream will not change it (googlefonts/fontations#1151, closed).
  byonk sets `Auto` explicitly and `resolve_auto_fallback` corrects the interpreter choice —
  keep both. Do not PR it.
- **`font-weight` does not disable hinting.** Measured at weight 400 and 500, both hinted.

## Font licensing — researched, awaiting F14

`.superpowers/sdd/…/font-licensing-research.md`. Redistribution and modification are permitted
for everything in the tree; what is missing is **notices** — `fonts/` has no licence file.

| Family | Licence | Obligation |
|---|---|---|
| Outfit, Terminus (TTF), Source trio | OFL 1.1 | ship OFL text |
| X11Helv | Adobe + DEC, MIT/X11-style | notice in copies **and documentation** |
| X11LuSans, X11LuType | **Lucida** (Bigelow & Holmes) | verbatim notice in user docs **and code comments** |
| X11Term | **DEC 1991 *and* Bitstream** | both notices, in one file |
| X11Misc5x–10x | public domain | none |
| **X11Misc12x**, **X11Misc8x @16** | **Sony Corp. 1987/88** | its own notice |

- **`X11Misc*` is a cell-width grouping, not a licence grouping.** Notices must be per source
  file.
- **Do not rename `X11LuSans`/`X11LuType` toward "Lucida"** — the trademark licence covers
  unmodified fonts only, and byonk modified them.

---

# Queued work

| ID | What |
|---|---|
| — | **Merge PR #30.** The only thing actually pending. |
| F13 | Extend `screens/examples/demo/font/{ttf,bitmap,hinting}/` to cover Source. |
| F14 | Licence + notice files per the table above. **`FONTS.md`'s "X11LuType is proportional" is wrong — it is monospaced.** |
| F22 | Cosmetic: the WiFi glyph reads as a caret at 8×12. Redraw or drop it. |
| F23 | The two fetching examples fail in a sandbox with `Cannot drop a runtime…` *from the fetch error path* — `c850ea7` covers the request itself, but check whether any other blocking call in `lua_runtime.rs` shares the hazard. |
| F24 | `/dev/render` shows the author nothing but an image — it passes `None` for the script log sink, so neither their `log_*` output nor byonk's authoring warnings reach the browser preview. Worth deciding whether that is intended. |
| Plan Task 11 | The hinted-font-trio **decision** (specimens + recommendation), never part of the 10. See the plan file from line 1430. Bundling whatever is chosen is separate work. |

**F15 is CLOSED** (Task 10 gave it the byonk-side regression test it was owed).

---

# Open questions for the owner

1. **`grey_count <= 2` may be the wrong rule.** On the 4-grey panel at 10–12 px mono+aliased
   beats smooth, but at 14 px smooth wins — the fix may be a **size term**, not a wider grey
   threshold. *Always name panels by config key:* the **4-colour** `trmnl_og_4clr` already
   counts as `grey_count = 2`; it is **4-grey** `trmnl_og` that is in question, and they behave
   oppositely. `FontConfig::adaptive_default` is the single place the rule lives.
2. **One genuinely inert knob remains:** `HintingMode::Light` is byte-identical to `Normal`.
3. `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and its
   `kchroma = 10` tuning are not on the crate's dithering path at all.

**Owner-facing artifacts** (URLs outlive their ephemeral sources; to update, republish
**passing the existing URL**, or a second artifact is created):

| What | URL |
|---|---|
| **Render sweep — 19 captures across 5 panels, the two bugs it found, Task 10** (session 26) | https://claude.ai/code/artifact/7e3a6c8d-763d-4985-8f12-69c7d7fdcc99 |
| Task 8 + the three demo bugs + the fetch fix (session 24) | https://claude.ai/code/artifact/dede3454-3192-47d6-8e45-97a71440a08f |
| X11 Bitmap Specimens — all 26 rebuilt faces, F16 before/after, the pitch table (session 22) | https://claude.ai/code/artifact/ef06c1db-b5ba-467c-8cc3-3a7069e00488 |
| Bitmap vs outline; F15 before/after; F16 diagnosis; F17 (session 20) | https://claude.ai/code/artifact/8fe47446-49b6-4256-9db6-429aa3b8bfb6 |
| Type trials: specimens, two bugs, the data (session 19) | https://claude.ai/code/artifact/f7ef39be-1a9d-4c97-bd95-d9b3422a515e |

---

# Lessons — these keep paying off

- **"No warnings" is not coverage until the mechanism has been shown to fire.** Thirteen clean
  screens prove nothing on their own — a warning that is never wired up produces the same
  output. Build a positive control that must trip it. Session 26 did, and only then was the
  sweep worth reporting.
- **Coverage that is wide in one dimension can be nil in another.** The harness rendered 13
  screens and looked thorough — on one of five panels. Both bugs session 26 found lived in the
  dimension nobody was varying. Ask what the sweep holds *constant*, not just what it covers.
- **A parameter that is only ever passed one value is untested, not correct.** `--device` had
  two settings and everything used the default, so nothing noticed it was the *only* input to
  a decision the panel should have owned. The palette half of the same chain was already right,
  which made the wrong half look right too.
- **A tool that hides a channel makes every future run lie.** The capture harness discarded
  stderr, which is the only place the CLI puts authoring warnings. Whenever a new output
  channel is added, **check what the existing harnesses do with it** — they were written before
  it existed.
- **Ask what a check would prove *today*, not what it proved when it was written.** The state2b
  diff was a good idea in Task 3's world and worthless in Task 9's, because eight intervening
  commits made everything change for known reasons. A stale comparison that reports "all
  changed" reads like coverage.
- **Demonstrate the check fails when the thing is broken.** A test written *after* the
  implementation has never been shown to fail, so sabotage stands in for the RED step. Sessions
  25 and 26 each sabotaged; back the file up first and diff after restoring.
- **A rule can be right about the mechanism and still wrong about the decision.** The plan's
  integer-zoom exemption was *correct* that integer zoom keeps a hinted outline on whole
  pixels — and still wrong, because that was never the main cost. **Check that a rule's stated
  justification actually covers the case that motivated the rule.**
- **The plan's code is not evidence. Eleven of eleven tasks touched were wrong.** Verify every
  symbol.
- **A default nothing asks for is a default that goes missing.** Resolve such defaults at the
  single choke point. Five callers each having to remember `spawn_blocking` is how the CLI
  forgot; the same shape produced the scale warning's out-param design.
- **Always carry a control through a measurement.** Cells that *must* be identical differ by
  244–284 px purely from position, because error diffusion depends on position. **Only
  grey-free (aliased) content is exactly comparable.**
- **Two things that look the same are not necessarily the same thing.** "Why do these cells
  look alike" turned out to be three independent causes stacked on each other.
- **A CSS rule beats a presentation attribute in SVG.** `text { font-family: … }` silently
  overrides every `font-family="…"` attribute on matching elements, and the text still renders
  — in the wrong face. Third font-family failure this initiative, after F17's unquoted
  parentheses and the `Source Sans 3` digit-suffix trap.
- **Put text on whole-pixel positions, not just whole-pixel sizes.** A fractional baseline
  slides the fitted glyph straight back off the grid.
- **Fix the docs when they are the bug.** `docs/src/tutorial/svg-templates.md` has been the bug
  **twice** in this initiative. It is embedded and served to LLM authors over MCP, so a stale
  line there teaches the reader least able to check.
- **Read a changelog section as a set, not as a stream of appends.** Five sessions of appending
  produced two entries that contradicted each other and two filed under the wrong heading. The
  reader sees it all at once even though it was never written that way.
- **Assert on the geometry, not the pixels, when the question is "is this legible".** A screen
  whose labels overlap into a smear renders perfectly validly; no pixel comparison catches it.
  `RenderResult::data` hands back exactly what the script computed and the SVG places things
  with, which makes screen layout testable — see `tests/calibration_grey_layout_test.rs`.
- **A raw string ends at the first `"#`, and hex colours are full of them.** `r#"…"#` around
  YAML containing `colors: "#000000,…"` fails to parse in a way that reads as nonsense
  (`expected `;`, found `000000``). Use `r##"…"##`. Third time on this branch.
- **A sleeping laptop looks exactly like a hung build.** `ps -eo pid,etime,command` reports
  *wall* time, which keeps counting through sleep. Check `uptime` too — this machine hit **load
  average 78** and a normally-2-minute compile took over 20. An IDE `cargo check --workspace`
  also holds the build lock and will stall your `cargo test` with "Blocking waiting for file
  lock".
- **`cargo check --lib --tests` is the fast way to see a signature-change RED.**
- **Verify a background job is actually running before reporting on it.** The log's mtime plus
  `ps` settles it; a `pgrep` pattern that misses `cargo-clippy` does not.
- **A screen that renders is not a screen that rendered what you asked for.** Carry a canary
  string *in the render itself*.
- **`test -s` both files before believing a `cmp`.** `cmp -s a b` against a non-existent `b`
  exits non-zero, exactly like "the files differ".
- **A flattering test string hides font defects.** `illiIL1 xXHv`, not `Render jpq 0123`.
- **When the data is right and the render is still wrong, suspect the consumer's guards.**
- **Work left by an agent that died is not verified work.**
- **Never run `make check` while the tree is being edited.** Also `make check > log; echo
  "EXIT=$?"` reports the *echo's* status — use `|| echo FAILED >> log`. Same trap with any
  pipe: `cmd | tail; echo $?` reports `tail`. **This bites in background jobs too**: a
  backgrounded `cargo test … | tail` reports exit 0 while the test text says FAILED, and it
  emits nothing until it finishes, so there is no interim progress to read.
- **A saved artifact is not evidence that it holds what its name says.**

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~15 min — background it**; it runs `cargo fmt`,
  not `--check`, so it rewrites files. **Green state = 1138 passed, 0 failed.**
- **Changing `Cargo.lock`'s resvg pin forces a full rebuild of usvg/resvg — 10+ min.**
- **Editing an embedded asset forces a rebuild.** `EmbeddedDocs` embeds exactly three files
  (`api/lua-api.md`, `tutorial/svg-templates.md`, `guide/authoring.md`); `EmbeddedBase` embeds
  `byonk-base/`; `EmbeddedScreens`/`EmbeddedExamples` embed `screens/{builtin,examples}/`.
  Editing any of those mid-`make check` corrupts the run. Other `docs/src/` pages are free.
- **In a debug build rust-embed reads from disk at runtime**, so screen / `byonk-base` edits
  take effect with **no rebuild**. This makes render-probe iteration fast — but "no change" is
  then indistinguishable from a stale binary, so **prove disk-backing with a visible sabotage
  first**.
- **Subagents must not run `make check`** — the 600 s watchdog kills them.
- `CARGO_BUILD_JOBS=2` — shared machine. `cargo test` takes only **one** filter, and a filter
  matches the *whole path*, so `--lib 'rendering::svg_to_png::tests::a'` is how you run a
  group of tests whose names share no substring.
- Pre-existing `#[ignore]` failures, unrelated: `preprocess::preprocessor::tests::{…}`.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`.
- **IDE diagnostics lie in this tree**, and they lag behind scripted rewrites. Only an actual
  cargo run counts.
- `make docs` = `mdbook build`; mdbook is installed. `docs/book/` is gitignored.
- **`docs/src/images/` is gitignored** — `hintdemo.png` is refreshed locally, never committed.

## Capturing every bundled screen, on every panel

```bash
BYONK_BIN=./target/debug/byonk ./tools/capture-renders.sh /path/to/out
```

Seconds, not minutes. **19 captures across 5 panels** — 4-grey, 4-colour, 16-grey and both
6-colour. Writes `MANIFEST.txt` with the canary verdict, per-screen exit codes, any stderr
byonk produced, and the distinctness verdict. **Do not put the output in `/tmp`** — that is how
the previous baselines were lost. Deterministic screens land at the top level,
non-deterministic ones in `nondeterministic/`.

**A capture is only as wide as `tools/capture-config.yaml`'s device map.** Twelve of the
original thirteen devices were on one panel, and two bugs hid in that gap for the whole
initiative. When adding a panel profile, add a device for it here too.

## Rendering a scratch screen

Validated end to end in sessions 25 and 26 — this recipe works:

1. Make a directory with a `byonk-screens.yaml` manifest (`name`, `description`, `author`,
   `license`). **Without the manifest the repo is skipped and every render silently falls
   back.**
2. Each screen needs `meta.yaml` (`title`, `description`, `byonk`, `refresh`), `script.lua`
   and `screen.svg`. A bare `name:` in `meta.yaml` is **not** enough.
3. **`script.lua` must return `{ data = { … }, refresh_rate = N }`** — a bare table of values
   fails with `error converting Lua nil to table`. **The template reads them under `data.`**:
   `{{ data.foo }}`, not `{{ foo }}`. Both cost a round trip in session 26.
4. Register it in a config copy. **`EXAMPLES_DIR` registers under the fixed handle `examples`,
   NOT the manifest's `name:`**, so use the config instead:
   ```yaml
   screen_repos:
     probe: { path: /abs/path/to/dir }
   devices:
     "AA:BB:CC:00:00:71": { panel: trmnl_og, screen: probe/myscreen }
   ```
   Seed the copy from `tools/capture-config.yaml`, which already has the panels and a
   `DEFAULT` device.
5. `CONFIG_FILE=<cfg> ./target/debug/byonk render --mac AA:BB:CC:00:00:71 --output x.png`

Notes:

- **Build the SVG from `layout.width`/`layout.height`.** Byonk warns on stderr if you do not.
- **Put text at integer x/y in any probe that judges hinting.**
- **Renders are dithered, and error diffusion depends on position** — two identical treatments
  at different places on one page do *not* come out byte-identical.
- `--colors "#000000,#FFFFFF"` forces a 2-colour panel — the BW/mono-hinting case.
- `--use-actual false` gives spec colours (pixel diffs); the default gives measured colours
  (judging type).
- **Swapping fonts without rebuilding:** `FONTS_DIR=<dir>` overrides embedded fonts **by
  filename**.
- PIL is available; `Image.NEAREST` at 3–6× is what makes pixel-level differences legible.

## Fonts

- `make fonts-setup` (once) → `.venv-fonts`; `make fonts-check` (18 tests, instant);
  `make fonts` (rebuild all 26, deterministic). Downloads cache in `fonts/.x11-cache/`.
- **`.venv-fonts/bin/python` has fontTools** — use it to interrogate the bundled faces
  directly (hinting programs, strikes, metrics) instead of inferring from renders.
- **Working on resvg:** clone `oetiker/resvg` into the scratchpad. Its suite is fast (~11 s,
  1750 tests) and safe in the foreground. To test byonk against a local resvg, point
  `[patch.crates-io]` at `<clone>/crates/{resvg,usvg}` — **back up `Cargo.toml` and
  `Cargo.lock` first and restore them after.**
- The patched resvg source is readable at
  `~/.cargo/git/checkouts/resvg-b4a0ccb9ea26de88/<rev>/` — faster than cloning when you only
  need to check what usvg does. Session 25 settled the whole integer-zoom question from
  `crates/usvg/src/text/flatten.rs` without building anything.

---

# Carried forward

The pinning initiative is done and reviewed; detail in `git show 3b32762:docs/HANDOVER.md` —
read before touching `eink-dither`, gamut mapping or colour models. Session 23's detail (F20,
F21, Task 7 archaeology) is in `git show 6e6e214:docs/HANDOVER.md`; session 24's (Task 8, the
CLI fetch fix, `http_response()`, the hinting demo) is in `git show 4cefe83:docs/HANDOVER.md`;
session 25's (Task 4 in full, the docs bug it uncovered) is in
`git show 5a531a1:docs/HANDOVER.md`.

`git worktree list` is clean.
