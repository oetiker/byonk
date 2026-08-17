# Handover — Byonk

_Last updated: 2026-08-17 (session 24). **Initiative: adopt the resvg `byonk-base` branch.**
Plan Tasks 1, 2, 3, 5, 6, 7, **8** done; **4, 9, 10 remain.** Landed this session:
**Task 8** (`da1415e`) and an unplanned but larger fix, **`c850ea7`** — `byonk render` could
never render a screen that fetches anything, and the hinting demo had been showing nothing._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main` — https://github.com/oetiker/byonk/pull/30 |
| HEAD | `c850ea7` — **tree clean** |
| Verified | `make check` at `c850ea7`: **1124 passed, 0 failed**; clippy clean under `-D warnings` |
| Pushed | `5863c7c` is on `origin`. **Five commits are local only:** `a02cc6e`, `9db650a`, `6e6e214`, `da1415e`, `c850ea7`. Pushing is the owner's call. |
| Push gotcha | The ssh-agent holds **no identities**, so `git push origin …` fails on publickey. `gh` is authenticated over HTTPS with `repo` scope — `git push https://github.com/oetiker/byonk.git <branch>` works and leaves the remote config alone. |

**resvg work happens in a different repo.** `oetiker/resvg` carries `feat/bitmap-mask-glyphs`
(upstream PR #1115), `feat/font-hinting` (upstream PR #1116), and `byonk-base`, which merges
them and is what byonk's `[patch.crates-io]` pins. **Current pin: `2e766508`** (in
`Cargo.lock`; `Cargo.toml` tracks the branch).

**The plan:** `docs/superpowers/plans/2026-08-15-resvg-byonk-base-integration.md`. Still the
authority on *what* Tasks 4, 9, 10 are for — but **it has now been wrong in nine of nine
tasks touched.** Treat its code as a sketch. Verify every symbol.

**The ledger:** `.superpowers/sdd/2026-08-15-resvg-byonk-base-integration/progress.md`
(git-ignored). Also there: `f11-report.md`, `f15-report.md`, `font-licensing-research.md`,
`f9-brief.md`, `f10-brief.md`, `f16-probe/`. **Ignore the two `f15-*.patch` files — neither
holds what its name claims.** `git log` on `oetiker/resvg` is the truth for resvg work.

---

# Open item the owner should decide first

**Two TLS tests are flaky under full-workspace load** —
`lua_https_tests::{test_https_with_custom_ca_cert, test_https_with_client_certificate}`.
They fail with `error sending request for url (https://127.0.0.1:…)`, which is the shape a
30 s timeout takes. Observed **once in three full `make check` runs**; they pass 8/8 in
isolation and 3/3 running the whole `lua_api_test` binary.

**The null hypothesis was never tested** — nobody has run the suite with `c850ea7`'s HTTP
change reverted, so "my change caused it" is unproven. What the change plausibly contributes
is **one extra OS thread per HTTP request**, on tests already sitting near the default 30 s
timeout on a saturated machine.

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
| 4 | Render-scale warning | A probe SVG with a `400x120` viewBox rendered into an 800×480 device comes out silently scaled 2×, so type meant to be judged at 9–11 px is judged at 18–22. Bit session 23. |
| 9 | State-3 capture + pixel diff + **show the owner** | Baseline `/tmp/byonk-renders/state2-final` — **regenerate rather than trust; `/tmp` does not survive reboot.** Note `tools/capture-renders.sh` drives `cargo run --release`; a debug-binary variant is much faster (see *Rendering a scratch screen*). |
| 10 | Fix or delete `test_bitmap_font_render` | |

---

# Task 8 — DONE (`da1415e`)

`byonk-base/v1/hinting.svg` is reduced to a shim and removed from all 12 bundled screens.
**Output-preserving, measured:** all 8 bundled screens that render without a network fetch are
byte-identical before and after.

**`shape-rendering` never applied to text at all.** usvg reads it only when converting a
*shape* element (`converter.rs:969`); `<text>` takes its rasterization from `text-rendering`
(`parser/text.rs:112`). Emptying the partial entirely changed no pixel — verified against a
control, since an earlier probe adding `fill:` to the same file *did* change output. The shim
still emits `crispEdges` so an out-of-tree screen including it from a non-text rule is
unaffected.

**F17 folded in and confirmed real:** `font-family="Terminus (TTF)"` unquoted is invalid CSS,
so the Terminus demo had never rendered Terminus — it fell back to a serif. Fixed
**template-side**, which is the general fix. Signature that confirms the diagnosis: the
Terminus demo changed, the bitmap demo (plain-identifier families) stayed byte-identical.

`docs/src/tutorial/svg-templates.md` carried ~80 lines documenting the removed
`-resvg-hinting-*` properties. **That page is embedded and served to LLM authors over MCP**,
so it was teaching the wrong thing to the reader least able to check. Replaced.

---

# `c850ea7` — the fetch fix, and the demo that showed nothing

## `byonk render` could never fetch

It drove the whole render synchronously from inside `#[tokio::main]`, so Lua's
`reqwest::blocking` client was built and dropped on a tokio worker thread and tokio panicked
in runtime shutdown. The script fell into its failure path and the render died with a
confusing `data.width` error. **No screen that fetches anything had ever rendered from the
CLI.**

Every other caller — `api/display.rs`, `api/dev.rs`, `mcp/` — steps onto a blocking thread by
hand first, which is how the CLI came to be the one that forgot. **Fixed at the choke point:**
`send_http_request_off_runtime` runs the request on a thread with no tokio context.
`build_http_spec` collects the options once so `http_request` and `http_response` cannot
drift. Both `webscrape` and `swiss-departure-board` now render live data from the CLI.

**Lua's HTTP path had no test coverage whatsoever.** The three "HTTP tests" in
`tests/lua_api_test.rs` start a mock server and then assert only that a URL string contains a
path — they never run Lua, and the comment admits it. Three real tests were written failing
first; the first reproduces the production panic exactly.

## New Lua API: `http_response()`

Owner's call, and the reason for it: a script could not tell a 404 or a 500 from data, because
`http_get` returns only the body. Returns `ok` / `status` / `body` / `headers` / `error` /
`from_cache` and **does not raise** — what a failure means is the screen's business.
Responses are now cached **only when they succeed**, so an error page is no longer served as
data for a whole `cache_ttl` window.

> **Owner decision:** byonk intervenes only when Lua *crashes*. The two fetching examples call
> `error()` when they cannot carry on, which puts the message on the device's error screen
> (`display.rs:1032`) and exits the CLI non-zero. Verified end to end. They previously returned
> a half-filled table holding an `error` field their templates never referenced, so **any**
> fetch failure ended in `Variable 'data.width' not found`.

## The hinting demo had three stacked bugs

The owner did not believe the demo, twice, and was right both times. Every cell looked alike.

1. **The stylesheet's `text { font-family: Outfit; }` overrode every cell's own
   `font-family` attribute.** In SVG a presentation attribute is the **lowest**-priority
   source, so a CSS rule beats it. The variants were never selected. This is why `smooth` and
   `off` were byte-identical. Nothing warns you — the text renders fine in the base font.
2. **Fractional baselines.** Only the top-left cell sat on the pixel grid; the rest were ⅓ and
   ⅔ of a pixel off in both axes. Hinting fits the outline to the grid and the layout then slid
   the glyph off it — **3–5% of the ink lost to dropped stems**, more than the difference
   between two engines. The demo was violating byonk's own documented whole-pixel advice.
3. **The mono column was never drawn 1-bit** (below).

All three fixed: **no two of the nine cells are identical.** `auto/smooth` vs `auto/off` went
from 0 differing pixels to 832; `auto/mono` vs `interpreter/mono` from 11 to 558.

---

# Settled this session — do not reopen

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

> I first called the engine axis "dead". **That was wrong** — it was suppressed by demo bug 1.
> It is the axis that *shows* these facts, and `auto ≡ auto_fallback` is a live check that
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
6. **byonk intervenes only when Lua crashes** (this session, above).

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
`font-family="'Crisp Body', Outfit"` — and see the CSS trap below.

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
- **`font-weight` does not disable hinting.** Suspected during the demo investigation;
  measured at weight 400 and 500, both hinted.

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
| F23 | **New.** The two fetching examples fail in a sandbox with `Cannot drop a runtime…` *from the fetch error path* — the `c850ea7` fix covers the request itself, but check whether any other blocking call in `lua_runtime.rs` shares the hazard. |

---

# Open questions for the owner

1. **`grey_count <= 2` may be the wrong rule.** On the 4-grey panel at 10–12 px mono+aliased
   beats smooth, but at 14 px smooth wins — the fix may be a **size term**, not a wider grey
   threshold. *Always name panels by config key:* the **4-colour** `trmnl_og_4clr` already
   counts as `grey_count = 2`; it is **4-grey** `trmnl_og` that is in question, and they behave
   oppositely. `FontConfig::adaptive_default` is the single place the rule lives.
2. **One genuinely inert knob remains:** `HintingMode::Light` is byte-identical to `Normal`.
   (The other former entry — "`interpreter` makes `target` inert" — is now better stated as
   *the interpreter has nothing to run on any bundled font*.)
3. `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and its
   `kchroma = 10` tuning are not on the crate's dithering path at all.
4. **Before merging #30: re-read `CHANGES.md`'s Unreleased section as a whole.** It has grown
   across four sessions and has never been read as a set. Also: two overstated test names in
   `dither/mod.rs`.

**Owner-facing artifacts** (URLs outlive their ephemeral sources; to update, republish
**passing the existing URL**, or a second artifact is created):

| What | URL |
|---|---|
| **Task 8 + the three demo bugs + the fetch fix** (session 24) | https://claude.ai/code/artifact/dede3454-3192-47d6-8e45-97a71440a08f |
| X11 Bitmap Specimens — all 26 rebuilt faces, F16 before/after, the pitch table (session 22) | https://claude.ai/code/artifact/ef06c1db-b5ba-467c-8cc3-3a7069e00488 |
| Bitmap vs outline; F15 before/after; F16 diagnosis; F17 (session 20) | https://claude.ai/code/artifact/8fe47446-49b6-4256-9db6-429aa3b8bfb6 |
| Type trials: specimens, two bugs, the data (session 19) | https://claude.ai/code/artifact/f7ef39be-1a9d-4c97-bd95-d9b3422a515e |

---

# Lessons — these keep paying off

- **Demonstrate the check fails when the thing is broken.** A test written *after* the
  implementation has never been shown to fail, so sabotage is the only thing standing in for
  the RED step. Session 24 sabotaged `select_hinting` to prove the aliasing test had teeth.
- **A default nothing asks for is a default that goes missing.** Resolve such defaults at the
  single choke point. The CLI fetch bug is the same shape: five callers each had to remember
  `spawn_blocking`, and one forgot.
- **The plan's code is not evidence. Nine of nine tasks touched were wrong.** Verify every
  symbol.
- **Always carry a control through a measurement.** Session 24's first variant measurement said
  "all ten differ" — that was **error diffusion leaking between rows of one image**. Rendering
  one variant per image, with a duplicated screen as a byte-identical control, gave the true
  answer: four distinct appearances. The finished demo still shows this: cells that *must* be
  identical differ by 244–284 px purely from position. **Only the aliased mono column, which
  has no greys, is exactly comparable.**
- **Two things that look the same are not necessarily the same thing.** Chasing "why do these
  cells look alike" found three independent causes stacked on each other. Stopping at the first
  plausible one would have shipped the other two.
- **A CSS rule beats a presentation attribute in SVG.** `text { font-family: … }` silently
  overrides every `font-family="…"` attribute on matching elements, and the text still renders
  — in the wrong face. **Third font-family failure this initiative**, after F17's unquoted
  parentheses and the `Source Sans 3` digit-suffix trap.
- **Put text on whole-pixel positions, not just whole-pixel sizes.** Hinting fits to the pixel
  grid; a fractional baseline slides the fitted glyph straight back off it.
- **Verify a background job is actually running before reporting on it.** Session 24 twice read
  a log that had not caught up and drew the wrong conclusion — once "still running" when it had
  finished, once "killed mid-suite" when it had not. `ps -eo pid,etime,command` plus the log's
  mtime settles it; a `pgrep` pattern that misses `cargo-clippy` does not.
- **A screen that renders is not a screen that rendered what you asked for.** Carry a canary
  string *in the render itself*.
- **`test -s` both files before believing a `cmp`.** `cmp -s a b` against a non-existent `b`
  exits non-zero, exactly like "the files differ".
- **Judge type at true size, and check the render scale.** Exactly what plan Task 4 is for.
- **A flattering test string hides font defects.** `illiIL1 xXHv`, not `Render jpq 0123`.
- **When the data is right and the render is still wrong, suspect the consumer's guards.**
- **Fix the docs when they are the bug.** Shipping a component means shipping how to use it.
- **Work left by an agent that died is not verified work.**
- **Never run `make check` while the tree is being edited.** Also `make check > log; echo
  "EXIT=$?"` reports the *echo's* status — use `|| echo FAILED >> log`. Same trap with any
  pipe: `cmd | tail; echo $?` reports `tail`.
- **A saved artifact is not evidence that it holds what its name says.**

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~15 min — background it**; it runs `cargo fmt`,
  not `--check`, so it rewrites files. **Green state = 1124 passed, 0 failed.**
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
- `CARGO_BUILD_JOBS=2` — shared machine. `cargo test` takes only **one** filter.
- Pre-existing `#[ignore]` failures, unrelated: `preprocess::preprocessor::tests::{…}`.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`.
- **IDE diagnostics lie in this tree.** Only an actual cargo run counts.
- **Do not split `src/rendering/svg_to_png.rs`** — it would collide with PR #30's diff.
- `make docs` = `mdbook build`; mdbook is installed. `docs/book/` is gitignored.
- **`docs/src/images/` is gitignored** — `hintdemo.png` is refreshed locally, never committed.

## Rendering a scratch screen

- Put screens in a directory with a `byonk-screens.yaml` manifest — **without it the repo is
  skipped and every render silently falls back.** Each screen also needs a `meta.yaml` with
  `title`/`description`/`byonk`/`refresh`; a bare `name:` is **not** enough and the screen is
  reported as "not provided". **`EXAMPLES_DIR` registers under the fixed handle `examples`,
  NOT the manifest's `name:`.** Use the config instead:
  ```yaml
  screen_repos:
    probe: { path: /abs/path/to/dir }
  ```
  then `CONFIG_FILE=<cfg> ./target/debug/byonk render --mac <mac> --output x.png`.
- **Match the SVG's viewBox to the device**, or the render is silently scaled (see Task 4).
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
- **A fast capture rig** (debug binary, same device map as `tools/capture-renders.sh`) is worth
  rebuilding for Task 9 — the shipped script uses `cargo run --release`.

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
  need to check what usvg does.

## Housekeeping

Two **stale scratch worktrees** are registered and both report **`prunable`**.
`git worktree prune` is safe and removes both:

```
…/6b605fbb-…/scratchpad/byonk-state1   (main)
…/bc0fc7e3-…/scratchpad/byonk-before   (detached 744fec8)
```

---

# Carried forward

The pinning initiative is done and reviewed; detail in `git show 3b32762:docs/HANDOVER.md` —
read before touching `eink-dither`, gamut mapping or colour models. Session 23's detail (F20,
F21, Task 7 archaeology) is in `git show 6e6e214:docs/HANDOVER.md`.
