# Handover — Byonk

_Last updated: 2026-08-17 (session 25). **Initiative: adopt the resvg `byonk-base` branch.**
Plan Tasks 1, 2, 3, 5, 6, 7, 8, **4** done; **9 and 10 remain.** Landed this session:
**Task 4** (`3823079`) — the render-scale warning, implemented against the plan's rule, plus
a docs bug it uncovered._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main` — https://github.com/oetiker/byonk/pull/30 |
| HEAD | `3823079` — **tree clean** |
| Verified | `make check` on the tree that became `3823079`: **1131 passed, 0 failed**; clippy clean under `-D warnings` |
| Pushed | `5863c7c` is on `origin`. **Seven commits are local only:** `a02cc6e`, `9db650a`, `6e6e214`, `da1415e`, `c850ea7`, `4cefe83`, `3823079`. Pushing is the owner's call. |
| Push gotcha | The ssh-agent holds **no identities**, so `git push origin …` fails on publickey. `gh` is authenticated over HTTPS with `repo` scope — `git push https://github.com/oetiker/byonk.git <branch>` works and leaves the remote config alone. |

**resvg work happens in a different repo.** `oetiker/resvg` carries `feat/bitmap-mask-glyphs`
(upstream PR #1115), `feat/font-hinting` (upstream PR #1116), and `byonk-base`, which merges
them and is what byonk's `[patch.crates-io]` pins. **Current pin: `2e766508`** (in
`Cargo.lock`; `Cargo.toml` tracks the branch).

**The plan:** `docs/superpowers/plans/2026-08-15-resvg-byonk-base-integration.md`. Still the
authority on *what* Tasks 9 and 10 are for — but **it has now been wrong in ten of ten tasks
touched.** Treat its code as a sketch. Verify every symbol.

**The ledger:** `.superpowers/sdd/2026-08-15-resvg-byonk-base-integration/progress.md`
(git-ignored). Also there: `f11-report.md`, `f15-report.md`, `font-licensing-research.md`,
`f9-brief.md`, `f10-brief.md`, `f16-probe/`. **Ignore the two `f15-*.patch` files — neither
holds what its name claims.** `git log` on `oetiker/resvg` is the truth for resvg work.

**Session 24's detail** (Task 8, the CLI fetch fix, `http_response()`, the three stacked
hinting-demo bugs) is in `git show 4cefe83:docs/HANDOVER.md`. Everything from it that is still
binding has been carried into this document.

---

# Open item the owner should decide first

**Two TLS tests are flaky under full-workspace load** —
`lua_https_tests::{test_https_with_custom_ca_cert, test_https_with_client_certificate}`.
They fail with `error sending request for url (https://127.0.0.1:…)`, which is the shape a
30 s timeout takes. Observed **once in three full `make check` runs** in session 24; **session
25's `make check` was clean** (1131/1131), so it is now once in four.

**The null hypothesis has still never been tested** — nobody has run the suite with
`c850ea7`'s HTTP change reverted, so "my change caused it" is unproven. What the change
plausibly contributes is **one extra OS thread per HTTP request**, on tests already sitting
near the default 30 s timeout on a saturated machine. Session 25 saw this machine hit **load
average 78** from unrelated desktop apps, which makes a load-dependent timeout entirely
plausible without any byonk defect at all.

**If it needs fixing, do not loosen the test.** The better shape is to stop building a client
per request: **cache the `reqwest::blocking::Client`**. The original panic was on *dropping*
the client's runtime inside a tokio context, so a cached client never drops there — which
removes the extra thread *and* the original bug, and is strictly less machinery than today.
A single shared worker thread is the wrong answer: it would serialise every screen's HTTP on
the server path.

---

# Remaining plan tasks

| # | What | Notes |
|---|---|---|
| 9 | State-3 capture + pixel diff + **show the owner** | Baseline `/tmp/byonk-renders/state2-final` — **regenerate rather than trust; `/tmp` does not survive reboot.** `tools/capture-renders.sh` drives `cargo run --release`; a debug-binary variant is much faster and session 25 proved it works (see *Rendering a scratch screen*). |
| 10 | Fix or delete `test_bitmap_font_render` | |

---

# Task 4 — DONE (`3823079`)

Byonk now warns when a screen's SVG is not the size of the panel it is drawn on.

## The plan's rule was wrong, and this is plan error #10

The plan's `scale_is_degraded` **exempts exact integer zooms**, and its own test asserts
`!scale_is_degraded(936, 702, DisplaySpec::X)`. But the case that motivated Task 4 — a
`400x120` probe rendered into 800×480, which bit session 23 — **is an exact 2× zoom**. The
plan's rule would have stayed silent on the bug it was written to catch.

**Owner chose: warn on any size mismatch, no exemption.** The predicate is
`SvgRenderer::scale_warning(svg_w, svg_h, spec) -> Option<String>`, next to `fit_transform`
and fed the same numbers, so the two cannot disagree.

## Why integer zoom is not a free pass — read from the pinned resvg, not reasoned

Source: `~/.cargo/git/checkouts/resvg-b4a0ccb9ea26de88/2e76650/`

- `crates/usvg/src/text/flatten.rs:283` — `let ppem = glyph.font_size();`, and the
  `GlyphHinting::ppem` doc comment says it outright: *"Derived from the font size, so hinted
  glyphs only land on whole pixels at an unscaled render."* The hinting ppem is the
  **user-unit** font size; the render transform is not involved. Integer zoom preserves the
  grid fit geometrically, but the glyph is fitted for one size and shown at another.
- `crates/usvg/src/text/flatten.rs:68` — `snap_bitmap_glyph` returns early unless the scale
  is 1.0 within `1e-4`. **Bitmap faces lose strike snapping at any zoom**, integer included,
  and the strike raster is then resampled.
- And the plain one: every dimension the author chose is displayed at the wrong size, which
  is what makes type impossible to judge.

## The channel — owner's decision, and it needed the CLI too

`tracing::warn!` was rejected: it reaches the server log, not the screen's author.

`rasterize_svg` fills a **`&mut Option<String>` out-parameter**, mirroring
`api::display::resolve_render_params`' existing `measured_warning`. Each caller decides:

| caller | where it goes |
|---|---|
| `ScreenStore::render` | `RenderResult::log` as `[warn] …` — the MCP/authoring path |
| `main.rs` (`byonk render`) | **stderr** |
| `api/display.rs`, `api/dev.rs`, `render_to_raw_png`, `content_pipeline`'s internal call | `&mut None` |

**The CLI leg was not optional.** `byonk render` never prints the script log at all, and
`/dev/render` passes `None` for its log sink (`content_pipeline.rs`, `run_script_direct` doc
comment) — so the sink alone would have left the warning invisible in the exact tool that
invites this mistake. `render_to_raw_png` stays silent so the authoring path, which renders
twice when `include_raw` is set, does not log one mistake twice.

## How it was verified

- **7 tests, all RED first.** 5 on the predicate, 1 through `render_to_palette_png` (proves
  usvg's *resolved* document size is what feeds it), 1 through `ScreenStore::render` (proves
  the log channel).
- **Sabotage 1:** implemented the plan's integer-zoom exemption → **3 tests fail**, including
  the one named for the missed case. The fractional test still passed, which is exactly the
  narrow slice the plan's rule does catch.
- **Sabotage 2:** removed the `screen_store` log push → `no scale warning in []`.
- **All 13 bundled screens render clean** (debug binary + `tools/capture-config.yaml`).
- **Positive control, because "0 warnings" is otherwise indistinguishable from a dead
  mechanism:** a scratch repo with a correct screen (silent) and a `400x240` screen (warns)
  driven through the real `byonk render`. Recipe below under *Rendering a scratch screen*.

## The docs were teaching the bug — second time this initiative

`docs/src/tutorial/svg-templates.md` said *"Set `viewBox` to `0 0 800 480` for TRMNL OG"* and
hardcoded the numbers in its opening example. **That page is one of the three `EmbeddedDocs`
served to LLM authors over MCP**, so it was instructing the reader least able to check to
write exactly the screen byonk now warns about. Rewritten to build the size from
`layout.width`/`layout.height` (both `layout.center_x` and `layout.center_y` exist in the
template namespace — see `content_pipeline::build_layout_context`), with the three costs and
the warning text documented. Later snippets on the page still write `800x480` literally and
the page now **says so**, rather than contradicting itself.

> Session 24 replaced ~80 lines of this same file. Treat it as a known trap: it is the most
> load-bearing authoring doc byonk ships, and it goes stale silently.

---

# Settled — do not reopen

## A variant CAN be aliased. The flag is document-level; the effect is not.

`HintingSpec::to_usvg()` deliberately drops `aliased`, because aliasing reaches usvg through
`Options::text_rendering`, which is document-level. **But `text-rendering` is an ordinary
inheritable SVG property, so the element using a variant asks for it directly.**

Measured, and pinned by `a_mono_variant_plus_optimize_speed_equals_the_document_level_aliased_mono`
(sabotage-verified — with `select_hinting` made to ignore the variant override, it fails):

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

`fpgm` is empty everywhere; `prep` is a 7-byte stub on the four variable fonts. Consequences,
all confirmed by render: **`interpreter` is effectively unhinted** (19 differing px vs
hinting-off), **`auto` ≡ `auto_fallback`** (byonk's `resolve_auto_fallback` doing its job), and
`interpreter` is *visibly worse* when aliased — uneven stems, 421 ink px vs 376.

> The engine axis is **not** dead — that earlier call was wrong, and was an artefact of a demo
> bug. It is the axis that *shows* these facts, and `auto ≡ auto_fallback` is a live check that
> byonk's auto-fallback fix still works. Install a hinted font via `FONTS_DIR` and the rows
> separate further.

## Smooth hinting is real but cannot look dramatic

Document `smooth` vs document `off` differ on 35–72% of the ink (more at small sizes) — but
almost entirely as **grey-level shifts on anti-aliased edges**, not as changed coverage. Two
anti-aliased renders always look alike at a glance. Only aliasing makes hinting visually
obvious. `hinting = false` on a variant is byte-identical to document-level
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
7. **Authoring warnings reach the author, not the operator** (Task 4, above). A new warning of
   this kind belongs in the script log sink and on the CLI's stderr — never only in `tracing`.

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
`preserve_linear_metrics` (the real field is `symmetric_rendering`). A directive that names
**only** variants keeps the panel's adaptive default — pinned by
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
  back with it.
- **Terminus is NOT buggy. Terminus @14 and @18 render 1 px/glyph wider — that is correct.**
  Raised twice, settled twice.
- **Merge trap:** `byonk-base` has host hooks upstream does not (`FontResolver::select_bitmap`).
  A clean *textual* merge is **not** evidence the semantics survived. Diff the merge result
  against the pre-merge tree.
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
| F13 | Extend `screens/examples/demo/font/{ttf,bitmap,hinting}/` to cover Source. |
| F14 | Licence + notice files per the table above. **`FONTS.md`'s "X11LuType is proportional" is wrong — it is monospaced.** |
| F15 | **Owes a byonk-side regression test.** The resvg-side tests do not run in byonk's suite, so nothing in byonk fails if the pin regresses. |
| F22 | Cosmetic: the WiFi glyph reads as a caret at 8×12. Redraw or drop it. |
| F23 | The two fetching examples fail in a sandbox with `Cannot drop a runtime…` *from the fetch error path* — `c850ea7` covers the request itself, but check whether any other blocking call in `lua_runtime.rs` shares the hazard. |
| F24 | **New.** `/dev/render` shows the author nothing but an image — it passes `None` for the script log sink, so neither their `log_*` output nor byonk's authoring warnings reach the browser preview. Worth deciding whether that is intended. |

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
4. **Before merging #30: re-read `CHANGES.md`'s Unreleased section as a whole.** It has grown
   across five sessions and has never been read as a set. Also: two overstated test names in
   `dither/mod.rs`.

**Owner-facing artifacts** (URLs outlive their ephemeral sources; to update, republish
**passing the existing URL**, or a second artifact is created):

| What | URL |
|---|---|
| **Task 8 + the three demo bugs + the fetch fix** (session 24) | https://claude.ai/code/artifact/dede3454-3192-47d6-8e45-97a71440a08f |
| X11 Bitmap Specimens — all 26 rebuilt faces, F16 before/after, the pitch table (session 22) | https://claude.ai/code/artifact/ef06c1db-b5ba-467c-8cc3-3a7069e00488 |
| Bitmap vs outline; F15 before/after; F16 diagnosis; F17 (session 20) | https://claude.ai/code/artifact/8fe47446-49b6-4256-9db6-429aa3b8bfb6 |
| Type trials: specimens, two bugs, the data (session 19) | https://claude.ai/code/artifact/f7ef39be-1a9d-4c97-bd95-d9b3422a515e |

Session 25 produced no artifact — Task 4 has no visual result worth showing.

---

# Lessons — these keep paying off

- **A rule can be right about the mechanism and still wrong about the decision.** The plan's
  integer-zoom exemption was *correct* that integer zoom keeps a hinted outline on whole
  pixels — and still wrong, because that was never the main cost. **Check that a rule's stated
  justification actually covers the case that motivated the rule.** It did not: the exemption
  silenced the exact bug the task existed for.
- **"No warnings" is not coverage until the mechanism has been shown to fire.** Thirteen clean
  screens prove nothing on their own — a warning that is never wired up produces the same
  output. Build a positive control that must trip it.
- **Demonstrate the check fails when the thing is broken.** A test written *after* the
  implementation has never been shown to fail, so sabotage is the only thing standing in for
  the RED step. Session 25 sabotaged twice: the predicate (to the plan's rule) and the log
  wiring.
- **The plan's code is not evidence. Ten of ten tasks touched were wrong.** Verify every
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
- **Put text on whole-pixel positions, not just whole-pixel sizes.** Hinting fits to the pixel
  grid; a fractional baseline slides the fitted glyph straight back off it.
- **Fix the docs when they are the bug.** `docs/src/tutorial/svg-templates.md` has now been
  the bug **twice** in this initiative. It is embedded and served to LLM authors over MCP, so
  a stale line there teaches the reader least able to check.
- **A sleeping laptop looks exactly like a hung build.** `ps -eo pid,etime,command` reports
  *wall* time, which keeps counting through sleep, so a large `etime` is not evidence of a
  hang. Check `uptime` too — this machine hit **load average 78** from unrelated desktop apps
  and a normally-2-minute compile took over 20.
- **`cargo check --lib --tests` is the fast way to see a signature-change RED.** A full
  `cargo test` build costs minutes on this machine; the type error arrives in seconds.
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
  pipe: `cmd | tail; echo $?` reports `tail`.
- **A saved artifact is not evidence that it holds what its name says.**

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~15 min — background it**; it runs `cargo fmt`,
  not `--check`, so it rewrites files. **Green state = 1131 passed, 0 failed.**
- **Changing `Cargo.lock`'s resvg pin forces a full rebuild of usvg/resvg — 10+ min.**
- **Editing an embedded asset forces a rebuild.** `EmbeddedDocs` embeds exactly three files
  (`api/lua-api.md`, `tutorial/svg-templates.md`, `guide/authoring.md`); `EmbeddedBase` embeds
  `byonk-base/`; `EmbeddedScreens`/`EmbeddedExamples` embed `screens/{builtin,examples}/`.
  Editing any of those mid-`make check` corrupts the run. Other `docs/src/` pages are free.
- **In a debug build rust-embed reads from disk at runtime**, so screen / `byonk-base` edits
  take effect with **no rebuild**. Verified by deliberate sabotage. This makes render-probe
  iteration fast — but "no change" is then indistinguishable from a stale binary, so **prove
  disk-backing with a visible sabotage first**.
- **Subagents must not run `make check`** — the 600 s watchdog kills them.
- `CARGO_BUILD_JOBS=2` — shared machine. `cargo test` takes only **one** filter, and a filter
  matches the *whole path*, so `--lib 'rendering::svg_to_png::tests::a'` is how you run a
  group of tests whose names share no substring.
- Pre-existing `#[ignore]` failures, unrelated: `preprocess::preprocessor::tests::{…}`.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`.
- **IDE diagnostics lie in this tree**, and they also lag behind edits made by scripted
  rewrites. Only an actual cargo run counts.
- **Do not split `src/rendering/svg_to_png.rs`** — it would collide with PR #30's diff.
- `make docs` = `mdbook build`; mdbook is installed. `docs/book/` is gitignored.
- **`docs/src/images/` is gitignored** — `hintdemo.png` is refreshed locally, never committed.

## Rendering a scratch screen

Validated end to end in session 25 — this recipe works:

1. Make a directory with a `byonk-screens.yaml` manifest (`name`, `description`, `author`,
   `license`). **Without the manifest the repo is skipped and every render silently falls
   back.**
2. Each screen needs `meta.yaml` (`title`, `description`, `byonk`, `refresh`), `script.lua`
   and `screen.svg`. A bare `name:` in `meta.yaml` is **not** enough — the screen is reported
   as "not provided".
3. Register it in a config copy. **`EXAMPLES_DIR` registers under the fixed handle `examples`,
   NOT the manifest's `name:`**, so use the config instead:
   ```yaml
   screen_repos:
     probe: { path: /abs/path/to/dir }
   devices:
     "AA:BB:CC:00:00:71": { panel: trmnl_og, screen: probe/myscreen }
   ```
   Seed the copy from `tools/capture-config.yaml`, which already has the panels and a
   `DEFAULT` device.
4. `CONFIG_FILE=<cfg> ./target/debug/byonk render --mac AA:BB:CC:00:00:71 --output x.png`

Notes:

- **Build the SVG from `layout.width`/`layout.height`.** Byonk now warns on stderr if you do
  not — that is Task 4, and the warning names both sizes and the scale factor.
- **Put text at integer x/y in any probe that judges hinting.** A fractional baseline costs
  3–5% of the ink and will swamp what you are measuring.
- **Renders are dithered, and error diffusion depends on position** — two identical treatments
  at different places on one page do *not* come out byte-identical. Render one variant per
  image, or compare only aliased (grey-free) content.
- `--colors "#000000,#FFFFFF"` forces a 2-colour panel — the BW/mono-hinting case.
- `--use-actual false` gives spec colours (use for pixel diffs); the default gives measured
  colours (use for judging type).
- **Swapping fonts without rebuilding:** `FONTS_DIR=<dir>` overrides embedded fonts **by
  filename**.
- PIL is available; `Image.NEAREST` at 3–6× is what makes pixel-level differences legible.
- **The debug binary is the fast capture rig** Task 9 wants. Session 25 rendered all 13
  bundled screens through it in seconds using `tools/capture-config.yaml`'s device map;
  `tools/capture-renders.sh` uses `cargo run --release` and also swallows stderr, so it would
  hide the new warning.

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
CLI fetch fix, `http_response()`, the hinting demo) is in `git show 4cefe83:docs/HANDOVER.md`.

The two stale scratch worktrees noted in the last handover were **pruned** in session 25;
`git worktree list` is clean.
