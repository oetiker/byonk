# Handover — Byonk

_Last updated: 2026-08-17 (session 23). **Initiative: adopt the resvg `byonk-base` branch.**
Plan Tasks 1, 2, 3, 5, 6, 7 done; **4, 8, 9, 10 remain.** Landed this session: **F20**
(`8246b92`), **F21** (`3a35030`) and **Task 7** (`a02cc6e`). Task 7 is the one that matters:
until it landed, byonk applied **no hinting at all** in production._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main` — https://github.com/oetiker/byonk/pull/30 |
| HEAD | `9db650a` — **tree clean** |
| Verified | `make check` at `9db650a`: **1120 passed, 0 failed**; clippy clean under `-D warnings` |
| Pushed | `5863c7c` is on `origin` and **all 10 CI checks passed there**. **Two commits are local only:** `a02cc6e`, `9db650a`. Pushing is the owner's call. |
| Push gotcha | The ssh-agent holds **no identities**, so `git push origin …` fails on publickey. `gh` is authenticated over HTTPS with `repo` scope — `git push https://github.com/oetiker/byonk.git <branch>` works and leaves the remote config alone. |

**resvg work happens in a different repo.** `oetiker/resvg` carries `feat/bitmap-mask-glyphs`
(upstream PR #1115), `feat/font-hinting` (upstream PR #1116), and `byonk-base`, which merges
them and is what byonk's `[patch.crates-io]` pins. **Current pin: `2e766508`** (in
`Cargo.lock`; `Cargo.toml` tracks the branch).

**The plan:** `docs/superpowers/plans/2026-08-15-resvg-byonk-base-integration.md`. Still the
authority on *what* Tasks 4, 8, 9, 10 are for — but **it has now been wrong in eight of eight
tasks touched**, including a semantic rule that was actively harmful (Task 7 below). Treat
its code as a sketch. Verify every symbol.

**The ledger:** `.superpowers/sdd/2026-08-15-resvg-byonk-base-integration/progress.md`
(git-ignored). Also there: `f11-report.md`, `f15-report.md`, `font-licensing-research.md`,
`f9-brief.md`, `f10-brief.md`, `f16-probe/`. **Ignore the two `f15-*.patch` files — neither
holds what its name claims.** `git log` on `oetiker/resvg` is the truth for resvg work.

---

# Next work: Task 8

**Migrate the 12 screens + docs, folding in F17.** Three parts:

1. **`byonk-base/v1/hinting.svg` still emits `-resvg-hinting-*` CSS**, which resvg 0.48.1
   ignores. It is **inert, not harmful** — hinting now comes from `usvg::Options`, not CSS.
   It must become a shim carrying only `shape-rendering: crispEdges`.
2. **12 files reference the include** (`grep -rl "hinting.svg" screens/`). Delete the
   `{% include %}` only where the surrounding CSS rule still has other declarations; where
   it was the rule's whole body, delete the rule.
3. **F17, folded in:** `font-family="Terminus (TTF)"` is invalid CSS — unquoted parentheses
   mean the text silently falls back to a serif, so the **Terminus TTF demo has never
   rendered Terminus**. **Verified location (the old handover named the wrong file):** the
   family string is `screens/examples/demo/font/ttf/script.lua:9`
   (`local family = "Terminus (TTF)"`), and `screen.svg:18` interpolates it **unquoted** as
   `font-family="{{ line.family }}"`. So either the Lua must emit a quoted name, the
   template must quote what it interpolates, or the family gets renamed. **Note the
   template-side fix is the general one** — any screen interpolating a family name hits
   this, not just this demo.

**Migrating is output-preserving by construction** — `FontConfig::adaptive_default` was built
to reproduce exactly what `hinting.svg` used to emit, so deleting the include should not
change a single pixel. That is testable, and Task 9's pixel diff is where it gets proven.

Docs for the directive are already written (`docs/src/api/font-hinting.md`, linked from
SUMMARY.md, plus a `### font_hinting` section in `api/lua-api.md`). Task 8 owes the
**upgrade notice** for the now-inert include.

## Remaining plan tasks

| # | What | Notes |
|---|---|---|
| 8 | Migrate screens + docs | Above. Fold in F17. |
| 4 | Render-scale warning | **Worth doing — it bit again in session 23.** A probe SVG with a `400x120` viewBox rendered into an 800×480 device came out silently scaled 2×, so type meant to be judged at 9–11 px was judged at 18–22. |
| 9 | State-3 capture + pixel diff + **show the owner** | Baseline `/tmp/byonk-renders/state2-final` — **regenerate rather than trust; `/tmp` does not survive reboot.** |
| 10 | Fix or delete `test_bitmap_font_render` | |

---

# Task 7 — DONE (`a02cc6e`)

**Hinting is now actually applied.** Task 6 threaded `Option<&FontConfig>` through the render
and **every production call site passed `None`**, so nothing in byonk was hinted anywhere.
That is why the `CHANGES.md` entry promising crisp BW text was not yet true. It is now.

**The structural decision worth keeping:** hinting resolves **inside**
`ContentPipeline::render_png_from_svg`, from the palette, *not* by the caller. Making five
call sites each responsible for supplying the adaptive default is how it goes missing on one
of them — silently, because an unhinted screen still renders perfectly well.
`a_screen_with_no_directive_still_gets_the_panel_s_hinting` pins it: a screen that said
nothing must render differently from one that said `font_hinting = false`. Sabotage-verified
by making a call site drop the config.

## The Lua surface, as shipped

```lua
font_hinting = false            -- hinting off entirely
font_hinting = {
  engine = "auto",              -- interpreter | auto | auto_fallback
  target = "mono",              -- shorthand for { mode = "mono" }
  -- target = { mode = "mono", aliased = false },
  -- target = { mode = "light", symmetric = true, preserve_linear_metrics = false },
  variants = {
    ["Crisp Body"] = { font = "Outfit", hinting = { target = "mono" } },
  },
}
```

`mode` is the discriminator: mono's extra knob is `aliased`, smooth's are `symmetric` and
`preserve_linear_metrics`. Smooth's defaults match what a grey panel would have got anyway.

## Three more plan errors (this is what took eight of eight)

1. **The plan's surface cannot express `aliased`** — the one knob that makes BW text crisp.
2. Its field is `symmetric`; the real one is `symmetric_rendering`.
3. **Its core rule is harmful.** "A present directive replaces the default wholesale" means
   `font_hinting = { variants = ... }` silently discards the adaptive mono hinting a BW panel
   depends on, for an author who only meant to add a variant. **`FontHintingDirective`
   separates "stated no default" (`None`) from "explicitly off" (`Some(None)`)**;
   `resolve(grey_count)` applies it. Test:
   `naming_only_variants_keeps_the_panel_s_adaptive_default`.

**It also wired two sites the plan never listed** — `api/dev.rs` and `screen_store.rs` (the
MCP authoring render) — and had to carry the directive through **`CachedContent`**, because
the device path renders from that cache, not from the script result. Without that, a screen's
hinting would have worked on the CLI and over MCP and quietly **not on the device**.

## Validation and the F1 warning

Both handover requirements met, and both fail loudly rather than rendering something else:

- **Variant base families are checked when the script runs.** `select_font` falls through to
  the default selector when `db.query` misses, so an unresolvable base family lands on the
  generic mapping instead of erroring.
- **Variant *names* are checked too.** The name is a hook byonk intercepts before the default
  selector; if it is also a real family, that family is shadowed.
- **The F1 constraint ships as a warning naming the offending variants**, pointing at
  `text-rendering="optimizeLegibility"` and away from `geometricPrecision`. It is checked
  against `grey_count = 2` — "would this be wrong on the panel where it *can* be wrong?" —
  because the same screen may be rendered on any panel.

**Proven by render, not only by tests:** unhinted-then-aliased vs mono-hinted-then-aliased on
a 2-colour palette. The unhinted row is visibly broken — `illiIL1` mush, stems dropping — and
is the F1 failure mode made visible. Rig: `…/scratchpad/probe/{hint-auto,hint-off}` +
`cfg2.yaml` (**ephemeral**).

**F1 design constraint, still binding on anything that touches hinting:** aliasing is
per-element and inheritable; hinting is per-face. An element choosing smooth/no hinting on a
BW panel inherits `optimizeSpeed` and lands in the known-bad aliased-without-mono state
(tiny-skia has no dropout control; stems drop out). Escape hatch:
**`text-rendering: optimizeLegibility`** — restores AA *and keeps hinting*. **Trap:
`geometricPrecision` restores AA but disables hinting.**

---

# Session 23's other two fixes

**F20 — the shipped components (`8246b92`).** `header.svg` and `status_bar.svg` both claimed
the header's top-right corner, and the timestamp's ink cut through the battery outline.
**Owner chose: the icons own the corner, the timestamp moves to the footer.** Rendering it
surfaced two faults the report had missed — the icons were dark grey on a *black* bar, so
near-invisible regardless; and `updated_at` was drawn by `header.svg` **and** `footer.svg`,
printing the time twice. Icons now default to light ink with a `status_color` override.

> **Breaking:** a screen including `header.svg` alone and relying on `updated_at` must now
> include `footer.svg` too.

Still open, cosmetic: the WiFi glyph is an 8×12 three-arc path whose inner arcs collapse at
that size, so it reads as a caret. Pre-existing; only noticeable now that it is visible.

**F21 — the silent screen-ref fallback (`3a35030`).** A device that **is** configured and
whose `screen:` does not resolve now returns `ContentError::DeviceScreenUnresolved`, naming
both the device and the ref. A device with **no** config still falls back to DEFAULT — that
is what the fallback is for, and its test still passes. The device-polling path already
rendered a visible error SVG for any `ContentError`, so a panel shows a message instead of
the wrong screen; the CLI exits non-zero and writes no file. Also dropped `main.rs`'s
`"Script error:"` wrapper, which double-prefixed real script errors and mislabelled every
other variant.

> **Consequence: the canary-device workaround is no longer needed for *configured* devices.**
> A probe rig can point a device at a bogus ref and assert the error instead. Unconfigured
> devices still fall back, so the canary still has a job there.

---

# Settled — do not reopen

## Owner decisions

1. **Bundle the Source trio** as generic-family fallbacks: `sans-serif` → Source Sans 3,
   `serif` → Source Serif 4, `monospace` → Source Code Pro. **Outfit stays** as the house
   sans, referenced by name where it already is.
2. **No fallback magic.** No grafting X11 strikes into Source, no size-conditional family
   substitution, no bitmap/outline hybrid. Designers choose bitmap faces explicitly.
3. **Fonts need licence files** (table below).
4. **Bitmap fonts should have no outlines if possible** — delivered by F16.
5. **F20: status icons own the header corner; the timestamp lives in the footer.**

## F15 / F16 — the bitmap work, done and live

- **The fonts and the resvg pin must move together.** With the old pin the advances are
  correct in the file and *ignored* at render time, giving fractional pitch — worse than
  before. If the pin is rolled back, roll the fonts back with it.
- **Terminus is NOT buggy.** Measured across all 1359 glyphs of all nine strikes: strike
  advances match canonical `ter-uXXn` at every size; only the outline disagrees, at 14 and
  18, and no single `hmtx` value can be right at all nine. **Terminus @14 and @18 render
  1 px/glyph wider — that is correct, do not "fix" it back.** Raised twice, settled twice.
- **Merge trap:** `byonk-base` has host hooks upstream does not
  (`FontResolver::select_bitmap`). A clean *textual* merge of upstream font work is **not**
  evidence the semantics survived — one such merge silently produced "outline drawn but
  strike advances used". Guard by diffing the merge result against the pre-merge tree.
- **A bitmap face only renders as a bitmap at a size it has a strike for**, and nothing warns
  you. At other sizes the nearest strike is scaled (blocky, right width). `fonts/FONTS.md`
  lists the sizes per family.
- Full archaeology, if ever needed: `git show 9db650a~1:docs/HANDOVER.md`.

## Falsified — do not chase again

- **X11 vertical-metric overflow** (ascender > upem): real malformation, **not** a cause of
  anything. No code in the bitmap path reads the ascender.
- **Ink overhang in the oblique faces**: `TerminusTTF-Italic` overhangs on 40.5% of its
  glyphs too. Slanted bitmap faces overhang normally.
- **F10's two hazards, both FALSE**, settled by rendering: the fvar `wght` default does *not*
  leak (resvg pushes 400 from CSS), and Source Serif 4 is *not* pinned at `opsz` 20 (`opsz`
  tracks font size). The earlier specimen finding that Source Code Pro was "far too light"
  without an explicit `wght` pin is **falsified for byonk's pipeline**.
- **F9 / `AutoFallback`:** skrifa tests whether `fpgm` *or* `prep` is non-empty, so it picks
  the interpreter for fonts with only a 7-byte `prep` stub. byonk sets `Auto` explicitly —
  keep that. **Upstream will not change it** (googlefonts/fontations#1151, closed "No issue
  here"). Do not PR it.

## Font licensing — researched, awaiting F14

`.superpowers/sdd/…/font-licensing-research.md`. Redistribution and modification are
permitted for everything in the tree; what is missing is **notices** — `fonts/` has no
licence file at all.

| Family | Licence | Obligation |
|---|---|---|
| Outfit, Terminus (TTF), Source trio | OFL 1.1 | ship OFL text |
| X11Helv | Adobe + DEC, MIT/X11-style | notice in copies **and documentation** |
| X11LuSans, X11LuType | **Lucida** (Bigelow & Holmes) | verbatim notice in user docs **and code comments** |
| X11Term | **DEC 1991 *and* Bitstream** — it spans two foundries | both notices, in one file |
| X11Misc5x–10x | public domain | none |
| **X11Misc12x**, **X11Misc8x @16** | **Sony Corp. 1987/88** | its own notice |

- **`X11Misc*` is a cell-width grouping, not a licence grouping.** Notices must be per source
  file. The importer writes every distinct source `COPYRIGHT` into name ID 0.
- **Do not rename `X11LuSans`/`X11LuType` toward "Lucida"** — the trademark licence covers
  unmodified fonts only, and byonk modified them.

## Naming rule for variant aliases

**Name them for their purpose, never `<RealFamily> <TechnicalTerm>`.** An alias is a name the
author invents and `select_font` intercepts, so it *must not* be a real family — that is the
mechanism, not a wart. But `"Outfit Mono"` (the plan's example, meaning *Outfit with mono
hinting*) reads as a monospaced Outfit to every later reader; **the owner queried it on
sight.** Use `["Crisp Body"] = { font = "Outfit", hinting = { target = "mono" } }`. Byonk now
*enforces* the "not a real family" half at parse time.

Second rule from the same episode: **an alias that resolves to nothing makes a test depend on
where unresolved families land**, which is the generic mapping. Always name the fallback in
the document: `font-family="'Crisp Body', Outfit"`.

---

# Queued work

| ID | What |
|---|---|
| F13 | Extend `screens/examples/demo/font/{ttf,bitmap,hinting}/` to cover Source. |
| F14 | Licence + notice files per the table above. **`FONTS.md`'s "X11LuType is proportional" is wrong — it is monospaced.** |
| F15 | **Owes a byonk-side regression test.** The resvg-side tests do not run in byonk's suite, so nothing in byonk fails if the pin regresses. |
| F17 | Fold into Task 8 (above). |
| F22 | Cosmetic: the WiFi glyph reads as a caret at 8×12. Redraw or drop it. |

---

# Open questions for the owner

1. **`grey_count <= 2` may be the wrong rule.** On the 4-grey panel at 10–12 px mono+aliased
   beats smooth, but at 14 px smooth wins — the real fix may be a **size term**, not a wider
   grey threshold. *Always name panels by config key:* the **4-colour** `trmnl_og_4clr`
   already counts as `grey_count = 2`; it is **4-grey** `trmnl_og` that is in question, and
   they behave oppositely. **Now cheap to explore** — `FontConfig::adaptive_default` is the
   single place the rule lives.
2. **Two inert knobs:** `HintingMode::Light` is byte-identical to `Normal`, and with
   `engine: Interpreter` the `target` has no effect. Both are documented as inert.
3. `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and its
   `kchroma = 10` tuning are not on the crate's dithering path at all.
4. **Before merging #30: re-read `CHANGES.md`'s Unreleased section as a whole.** It has grown
   across three sessions and has never been read as a set. The entry promising crisp BW text
   **is now true** (Task 7) and its `geometricPrecision` advice has been corrected to
   `optimizeLegibility`. Also: two overstated test names in `dither/mod.rs`.

**Owner-facing artifacts** (URLs outlive their ephemeral sources; to update, republish
**passing the existing URL**, or a second artifact is created):

| What | URL |
|---|---|
| **X11 Bitmap Specimens** — all 26 rebuilt faces, F16 before/after, the pitch table (session 22) | https://claude.ai/code/artifact/ef06c1db-b5ba-467c-8cc3-3a7069e00488 |
| Bitmap vs outline; F15 before/after; F16 diagnosis; F17 (session 20) | https://claude.ai/code/artifact/8fe47446-49b6-4256-9db6-429aa3b8bfb6 |
| Type trials: specimens, two bugs, the data (session 19) | https://claude.ai/code/artifact/f7ef39be-1a9d-4c97-bd95-d9b3422a515e |

**Session 23's renders were never published** and their scratchpad is ephemeral: the F20
corner zoom and the Task 7 hinted/unhinted comparison. Regenerate from the rigs if wanted.

---

# Lessons — these keep paying off

- **Demonstrate the check fails when the thing is broken.** Sabotage has caught real holes in
  every session that used it. In session 23, seven sabotages on Task 7 alone were each caught
  by their own test. A test that passes with the fix reverted is worthless — and a test
  written *after* the implementation has never been shown to fail at all, so sabotage is the
  only thing standing in for the RED step.
- **A default nothing asks for is a default that goes missing.** Hinting is applied by the
  server, so no screen can reveal a call site that forgot it — the screen just renders
  unhinted. Resolve such defaults at the single choke point, and pin them with a test that
  compares "said nothing" against "explicitly off".
- **The plan's code is not evidence. Eight of eight tasks touched were wrong**, once with a
  rule that would have silently degraded output. Verify every symbol.
- **A screen that renders is not a screen that rendered what you asked for.** Carry a canary
  string *in the render itself* — session 23's probes printed `CANARY AUTO` / `CANARY OFF` in
  the image, which is stronger than comparing file bytes.
- **`test -s` both files before believing a `cmp`.** `cmp -s a b` against a non-existent `b`
  exits non-zero, exactly like "the files differ".
- **Judge type at true size, and check the render scale.** Session 23 judged 9–11 px type
  that was silently rendered at 2× because the probe's viewBox did not match the device.
  This is exactly what plan Task 4 is for.
- **A flattering test string hides font defects.** `x X H v /` and `illiIL1`, not
  `Render jpq 0123`. (`Hamburgefonstiv` is a *type-design* proof word — representative, not
  extreme.)
- **When the data is right and the render is still wrong, suspect the consumer's guards.**
- **Two code paths that must agree need the same predicate.**
- **Always carry a control through a measurement.** A measurement with no control cannot tell
  "this is broken" from "this is how it is". Same for warnings: the test that the warning
  *doesn't* fire is what stops it firing unconditionally.
- **A template that reads bare names cannot see a namespaced context.** byonk hands Tera
  `data`/`device`/`params`/`layout`. **When a component documents inputs, render it once with
  those inputs set and once without, and require the two to differ.**
- **Fix the docs when they are the bug.** F18's crash *was* a usage example; F20's overlap was
  half a documentation problem. Shipping a component means shipping how to use it.
- **Work left by an agent that died is not verified work.** Re-run everything, sabotage checks
  included, before trusting it.
- **Never run `make check` while the tree is being edited.** Also `make check > log; echo
  "EXIT=$?"` reports the *echo's* status — use `|| echo FAILED >> log`. Same trap with any
  pipe: `cmd | tail; echo $?` reports `tail`.
- **A saved artifact is not evidence that it holds what its name says.** Diff it against what
  it claims to preserve.
- **Set CSS `height: auto` on any image you scale by width alone.**

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~10 min — background it**; it runs `cargo fmt`,
  not `--check`, so it rewrites files. **Green state = 1120 passed, 0 failed.**
- **Changing `Cargo.lock`'s resvg pin forces a full rebuild of usvg/resvg and everything
  downstream — 10+ min. Always background it**; a foreground timeout will kill it.
- **Editing an embedded asset forces a rebuild.** `EmbeddedDocs` embeds exactly three files
  (`api/lua-api.md`, `tutorial/svg-templates.md`, `guide/authoring.md`); `EmbeddedBase`
  embeds `byonk-base/`. Editing any of those mid-`make check` corrupts the run. Other
  `docs/src/` pages are free.
- **Subagents must not run `make check`** — the 600 s watchdog kills them. Give them
  `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` + a targeted `cargo test`.
- `CARGO_BUILD_JOBS=2` — shared machine. `cargo test` takes only **one** filter.
- Pre-existing `#[ignore]` failures, unrelated: `preprocess::preprocessor::tests::{…}`.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`.
- **IDE diagnostics lie in this tree.** Only an actual cargo run counts.
- **Do not split `src/rendering/svg_to_png.rs`** — it would collide with PR #30's diff.
- `make docs` = `mdbook build`; mdbook is installed. `docs/book/` is gitignored.

## Rendering a scratch screen

- Put screens in a directory with a `byonk-screens.yaml` manifest — **without it the repo is
  skipped and every render silently falls back.** **`EXAMPLES_DIR` registers under the fixed
  handle `examples`, NOT the manifest's `name:`.** Use the config instead:
  ```yaml
  screen_repos:
    probe: { path: /abs/path/to/dir }
  ```
  then `CONFIG_FILE=<cfg> ./target/debug/byonk render --mac <mac> --output x.png`.
- **Match the SVG's viewBox to the device**, or the render is silently scaled (see Task 4).
- **Put a canary string in the image**, e.g. `CANARY AUTO`. Since F21 a *configured* device
  with an unresolvable ref errors instead of falling back, so the old canary-device trick is
  only needed for unconfigured devices.
- `--colors "#000000,#FFFFFF"` forces a 2-colour panel — the BW/mono-hinting case.
- `--use-actual false` gives spec colours (use for pixel diffs); the default gives the
  panel's measured colours (use for judging type).
- **Measuring pitch without assuming a glyph width:** render the same glyph N and 2N times,
  `pitch = (ink₂ₙ − inkₙ) / N`. Both bitmap width and side bearings cancel. Rig preserved in
  `.superpowers/sdd/…/f16-probe/` (ruler screen, `cfg.yaml`, `measure_pitch.py`,
  `build_page.py`) — **fix the absolute paths first, they name a dead scratchpad.**
- **Swapping fonts without rebuilding:** `FONTS_DIR=<dir>` overrides embedded fonts **by
  filename**. Get a "before" with `git show HEAD:fonts/X11Foo.ttf > <dir>/X11Foo.ttf`.
- PIL is available for cropping/zooming renders; `Image.NEAREST` at 3–6× is what makes
  pixel-level differences legible.

## Fonts

- `make fonts-setup` (once) → `.venv-fonts`; `make fonts-check` (18 tests, instant);
  `make fonts` (rebuild all 26, deterministic). Downloads cache in `fonts/.x11-cache/`.
- **Working on resvg:** clone `oetiker/resvg` into the scratchpad. Its suite is fast (~11 s,
  1750 tests) and safe in the foreground — this is *not* byonk's `make check`. To test byonk
  against a local resvg, point `[patch.crates-io]` at `<clone>/crates/{resvg,usvg}` — **back
  up `Cargo.toml` and `Cargo.lock` first and restore them after**, or you commit a path that
  exists on one machine only.

## Housekeeping

Two **stale scratch worktrees** are registered and both report **`prunable`** — the reboot
took their directories. `git worktree prune` is safe and removes both:

```
…/6b605fbb-…/scratchpad/byonk-state1   (main)
…/bc0fc7e3-…/scratchpad/byonk-before   (detached 744fec8)
```

---

# Carried forward

The pinning initiative is done and reviewed; detail in `git show 3b32762:docs/HANDOVER.md` —
read before touching `eink-dither`, gamut mapping or colour models.
