# Handover — Byonk

_Last updated: 2026-08-15 (session 16). **New initiative: adopt the resvg `byonk-base` branch.**
The prior panel-colour-pinning initiative is COMPLETE and REVIEWED; its work is on
`feat/screen-store-authoring-core` behind **open PR #30**, and everything is pushed._

> **⚠️ CI IS RED on PR #30** — 3 failing tests. **Root cause is fully diagnosed** (session 16,
> below) and the fix is decided but **not written**. Do not re-derive it.

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main`, MERGEABLE — https://github.com/oetiker/byonk/pull/30 |
| HEAD | `3b32762` — local == `origin`, PR head == HEAD. **Everything is pushed.** |
| Tree | clean |
| CI | **failing** at `3b32762` (run 31571733061) — 451 passed, **3 failed** |
| New specs | on branch `origin/docs/resvg-integration`, **not yet merged into the working branch** |

The two specs (read both before starting):

- `docs/superpowers/specs/2026-08-14-resvg-byonk-base-design.md` — how the fork branch was built
- `docs/superpowers/specs/2026-08-14-byonk-resvg-integration-design.md` — **the byonk-side plan**

Read them with `git show origin/docs/resvg-integration:<path>`, or merge that branch in first.
The integration spec explicitly targets `feat/screen-store-authoring-core`.

---

# ⚠️ START HERE — the CI failure, fully diagnosed

Three tests fail on CI and **pass locally on macOS**:

```
services::screen_store::tests::render_include_raw_produces_pre_dither_png
services::screen_store::tests::render_script_colors_actual_length_mismatch_falls_back_to_panel_and_logs_warning
services::screen_store::tests::render_script_colors_actual_wins_over_panel_colors_actual_when_lengths_match
```

## Root cause: no bundled font resolves a GENERIC font-family

`fontdb::Database::new()` defaults the generic families to **Arial / Times New Roman /
Courier New** (fontdb `src/lib.rs:263-271`). **None of those are bundled.** Verified by probe —
with only byonk's embedded fonts loaded:

```
SansSerif -> false     Serif -> false     Monospace -> false
families: ["Outfit", "Terminus (TTF)", "X11Helv", "X11LuSans", "X11LuType",
           "X11Misc10x", "X11Misc12x", "X11Misc5x", "X11Misc6x", "X11Misc7x",
           "X11Misc8x", "X11Misc9x", "X11Term"]
```

`SvgRenderer::with_fonts` also calls `fontdb.load_system_fonts()`, so on **macOS** Arial resolves
and everything looks fine. On the **CI Linux runner** nothing matches, usvg logs
`No match for '…' font-family.` and **skips the text**, leaving a blank white render. Blank render
⇒ no greys ⇒ the PLTE loses the expected entries, and pre-dither vs dithered optimise down to
identical bytes. That is all three failures, one cause.

**This is also a production bug, not just a test bug.** The release image is `FROM scratch` — no
system fonts, no fontconfig — and byonk's own `byonk-base/v1/base.svg`, `header.svg`, `footer.svg`
and the built-in error screens in `template_service.rs` all use `font-family="sans-serif"`.

**It is independent of the resvg upgrade.** `byonk-base` does not fix it.

## The fix (decided, not written)

Map the generic families onto bundled fonts in `SvgRenderer::with_fonts`
(`src/rendering/svg_to_png.rs:43`), via `set_sans_serif_family` / `set_serif_family` /
`set_monospace_family` / `set_cursive_family` / `set_fantasy_family`.

**Set them AFTER `load_system_fonts()`** — on Linux that call parses fontconfig and will otherwise
overwrite the generics with whatever the host aliases (fontdb `src/lib.rs:631-636`). Deterministic
rendering across dev/CI/scratch is the whole point.

**Which fonts is the tabled question — see below.** If you need CI green before that lands, an
interim mapping of all generics to `Outfit` (+ `Terminus (TTF)` for monospace) is correct and
reversible; the owner's steer was "use that beautiful variable font".

---

# The initiative: adopt resvg `byonk-base`

## Why

byonk pins `resvg`/`usvg`/`fontdb` to the fork's **`skrifa`** branch, freezing it at resvg v0.46
while upstream shipped v0.47, v0.48, v0.48.1. The `skrifa` branch is obsolete — **upstream adopted
the harfrust/skrifa port itself in v0.48.0**. `byonk-base` re-bases byonk's needs on current
upstream and carries only what upstream has not taken: bitmap glyph rendering (PR #1115), font
hinting (PR #1116), plus two new resolver hooks.

**`byonk-base` tip is `303e38e0`** ("Sort the test module declarations") — the spec cites
`b67da7c0`, so the branch has moved. Re-check it before pinning.

## What we gain, and what we lose

**Gained — the variable-font weight bug disappears.** On the pinned `skrifa` branch,
`parser/text.rs:418` reads `if !has_wght && weight != 400`, so **CSS default weight 400 is the one
weight that gets no `wght` variation** — and `Outfit-Variable.ttf`'s default instance is Thin(100).
Measured ramp on the current build: 100–300 correct, **400 renders lighter than 200**, 500–900
correct. Our docs and `content_pipeline.rs` both write `font-family: Outfit, sans-serif` with no
weight, so this bites today.

**Verified fixed on `byonk-base`** — the guard is now plain `if !has_wght` (upstream #1099).
Confirmed by reading the branch, not just the spec.

**Lost — the `-resvg-hinting-*` CSS properties.** RazrFalcon rejected custom SVG attributes
outright. Hinting moves to a **`FontResolver`** hook, which is per-**font**, not per-element.

## Decisions taken this session (owner)

1. **Adaptive hinting becomes a server-side default.** byonk installs the resolver itself, deriving
   mono-vs-smooth from `grey_count` and using per-face strike introspection to skip hinting where a
   bitmap strike will be used. **Screens need no Lua**; the `font_hinting` directive is a pure
   override. Migration per screen is deleting one `{% include %}` line, and current output is
   preserved by construction.
2. **Font variants ship as a general Lua feature** (see next section), not a demo-only hack.
3. **Font trio choice: TABLED** — see "The tabled question".

## ⭐ Font variants — the owner's idea, verified to work

Per-element hinting looked lost, because `select_hinting` is keyed on face **ID**. It is not.

Verified on `byonk-base`:

- `select_font: Fn(&Font, &mut Arc<Database>) -> Option<ID>` — **byonk** does the family matching
  and may load fonts on demand, so an alias never has to be a real family name in fontdb.
  **No name-table surgery needed.**
- `select_hinting: Fn(ID, f32, Option<FontHintingOptions>, &Database) -> Option<FontHintingOptions>`
- `select_bitmap: Fn(ID, f32, &Database) -> bool`
- **fontdb does not dedupe identical font data** — loading `Outfit-Variable.ttf` three times gave
  three distinct IDs, all reporting family `Outfit`:
  `[(ID(1v1), "Outfit"), (ID(2v1), "Outfit"), (ID(3v1), "Outfit")]`

So N loads of the same bytes ⇒ N face IDs ⇒ N independent hinting/strike configs, selected from the
SVG by **plain `font-family`** — standard markup, exactly the mechanism RazrFalcon sanctioned
(*"As for tweaking hinting via `FontResolver` — sure."*).

Sketch of the Lua surface:

```lua
font_hinting = {
  variants = {
    ["Outfit Mono"]     = { font = "Outfit",  hinting = { target = "mono" } },
    ["Outfit Light"]    = { font = "Outfit",  hinting = { target = { mode = "light" } } },
    ["X11Helv Outline"] = { font = "X11Helv", strikes = false, hinting = { target = "mono" } },
  },
}
```

**Open implementation question, do not hand-wave it:** variants are per-script but the font
database is shared and built at startup. `select_font` takes `&mut Arc<Database>` precisely to
allow on-demand loading, and `Source::Binary` is `Arc`-backed so a copy-on-write clone duplicates
face *metadata*, not font bytes — but the caching strategy needs deciding during planning.

## Migration surface

- **`byonk-base/v1/hinting.svg`** — emits the `-resvg-hinting-*` properties, **included by 12
  screens**. It is a *versioned* `v1` asset, so changing it is a breaking change for screen authors
  → hence the docs upgrade notice. `shape-rendering: crispEdges` is a real SVG property and
  survives; so does `text-rendering="geometricPrecision"` (spec property, short-circuits to
  unhinted without consulting the resolver).
- **`screens/examples/demo/font/hinting/`** — a 9-cell engine×target grid over a *single* font,
  varying hinting per CSS class. **Ports as nine font variants** under decision 2.
- Screens including the partial: `swiss-departure-board`, `mandelbrot`, `gphoto`, `hello`,
  `webscrape`, `demo/font/bitmap`, `demo/font/ttf`, `builtin/calibration/{gamut,tone,grey,color}`.
- Docs using `sans-serif`: `docs/src/tutorial/svg-templates.md`,
  `docs/src/tutorial/first-screen.md`.

---

# The tabled question: which variable font trio

**Owner's ask:** bundle "a cool trio of matching (variable) fonts" to back the generic families.
**Status: deferred until after `byonk-base` lands**, by owner decision.

## What is already established (do not redo)

- **IBM Plex is disqualified** — Google Fonts has no variable Serif or Mono for it.
- Three complete variable superfamilies verified present in `google/fonts`, with sizes:

| Trio | Files | Total | Axes |
|---|---|---|---|
| **Source** | Source Sans 3 / Source Serif 4 / Source Code Pro | **2.0 MB** | `wght`, +`opsz` on serif |
| **Noto** | Noto Sans / Noto Serif / Noto Sans Mono | **5.5 MB** | `wdth,wght` on all three |
| **Roboto** | Roboto / Roboto Serif / Roboto Mono | **4.5 MB** | inconsistent; serif alone is 3.9 MB |

- Real family names (**not** the file names — this cost a render iteration):
  `Source Sans 3`, `Source Serif 4`, `Source Code Pro`, `Noto Sans`, `Noto Serif`,
  `Noto Sans Mono`, `Roboto`, `Roboto Serif`, `Roboto Mono`.
- `fonts/` is already **11 MB**, dominated by the X11 bitmaps.
- Outfit is byonk's house sans and is referenced **by name** in screens, docs and
  `content_pipeline.rs`. Whatever the trio, keeping Outfit named-only is the low-risk default.

## The measurement, and why it is not finished

Specimens were rendered **through byonk's own pipeline** at 10/12/14/17/20px, 800×480, dithered to
pure black/white, in
`/private/tmp/claude-501/-Users-oetiker-checkouts-byonk/92754932-3ff2-4719-9608-99d3c50975b0/scratchpad/trio/`
(session-scratch — **regenerate rather than trust it to still exist**). The nine `.ttf` files are
alongside.

Unhinted reading: **Noto** was sturdiest at 10–14px, **Source** most elegant at 17–20px but
thinnest and most dropout-prone small, **Roboto Serif** weakest small.

**The owner rejected judging on those**, correctly: *"we don't want unhinted we want hinted,
especially in the mono case this is important."* Hinting is exactly what helps most at these
sizes and could reorder the ranking.

**You can render hinted specimens on the CURRENT pinned build** — no need to wait for the
integration. The pinned `skrifa` branch still supports the CSS properties (verified names in
`parser/svgtree/names.rs`):

```
-resvg-hinting-engine        auto | native
-resvg-hinting-target        mono | smooth
-resvg-hinting-mode          normal | light | lcd | vertical-lcd
-resvg-hinting-symmetric
-resvg-hinting-preserve-linear-metrics
```

The BW config byonk ships today (from `v1/hinting.svg`) is: `target: mono`, `symmetric: false`,
`preserve-linear-metrics: true`, `mode: normal`, `engine: auto`, `shape-rendering: crispEdges`.

**Also pin `style="font-variation-settings: 'wght' 400"`** on every specimen — otherwise the
weight-400 bug above renders variable fonts at their default instance and the comparison is unfair
(Source Code Pro in particular came out far too light).

Recipe: a temporary `#[cfg(test)] mod` in `src/rendering/svg_to_png.rs` using
`SvgRenderer::with_fonts(...)` + `render_to_palette_png(svg, spec, &[(0,0,0),(255,255,255)], None,
false, None, None)`. **Revert it before committing** — it was reverted this session; the tree is
clean.

---

# Sequence to resume

The integration spec's own sequence, with session-16 additions marked ⭐:

0. **Dependency audit** — `tiny-skia` 0.11→0.12 (major, byonk passes `pixmap.as_mut()` into
   `resvg::render`), `fontdb` 0.23→0.24, `resvg` 0.46→0.48.1. Enumerate every crate shared across
   the resvg API boundary; a mismatch means two copies of a type and it will not compile. MSRV
   1.85.0. Step 0 needs no code and is where the surprises are.
1. Capture baseline renders (state 1 = pre-#30, state 2 = #30 without integration).
2. Bump deps, point `resvg`/`usvg` at `byonk-base`, **drop the fontdb patch**, reimplement
   `bitmap_strikes` in byonk with skrifa (`BitmapStrikes::new(&font_ref).iter().map(|s| s.ppem())`,
   kept sorted ascending). **Get it green before adding features.**
3. ⭐ Land the generic-family fix (independent of everything above — could go first to unred CI).
4. Add the `font_hinting` Lua directive + variants + the server-side adaptive default, and the
   render-scale warning (`rasterize_svg` should warn when the computed scale ≠ 1.0).
5. Migrate the 12 screens + rebuild the hinting demo as variants; write the docs upgrade notice.
6. Capture state 3, pixel-diff against state 2, hand differing screens to manual assessment.
7. Fix or delete `test_bitmap_font_render` — it renders, writes to `/tmp`, prints, and **asserts
   nothing**.
8. ⭐ Re-run the hinted trio comparison and settle the font question.

`test_bitmap_strikes_exposed` and `test_bitmap_font_families` must pass **unchanged** — they are
the contract for the fontdb substitution.

**Caveat from the spec, worth repeating:** resvg 0.48.0's changelog says plainly *"May result in
small rendering changes compared to older versions"*, on top of three text-positioning fixes
(#1043, #1040, #1056) — against hand-tuned fixed-panel layouts, and **byonk has no assertions on
rendered output today**.

---

# Build / verify

- `make check` = fmt + `clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`.
  **~10 min — background it** (`run_in_background: true`; foreground `sleep` is blocked).
- **`make check` runs `cargo fmt`, not `--check`** — it rewrites files in place.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine.
- **Subagents must not run `make check` in the foreground** — the stream watchdog fires at 600 s of
  silence and the agent dies mid-run. Give them
  `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` + `cargo test -p <crate> --lib`.
- `cargo test` takes only **one** filter argument.
- Pre-existing `#[ignore]` failures, unrelated to any current work:
  `preprocess::preprocessor::tests::{test_process_with_resize, test_resize_before_enhancement,
  test_resize_full_pipeline_with_photo_preset}` — they panic at `resize_lanczos` by design.
- **Never `git add -A`.** `/Users/oetiker/checkouts/byonk/examples/` is an untracked near-copy of
  `screens/examples/`. Add by explicit path; verify `git diff --cached`.
- `make docs` needs `mdbook-mermaid`.
- Rendering a builtin screen needs a device — copy `config.yaml`, point `CONFIG_FILE` at the copy,
  append a throwaway device **with a `panel:`** (without it you silently get a greyscale render):
  ```
  CONFIG_FILE=/tmp/x.yaml cargo run --release -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/out.png
  ```
- **Before changing visible text on a builtin screen, `grep -rn "<old label>" src/ tests/`** — two
  tests assert screen labels literally, and adding a builtin screen has a fan-out
  (`tests/builtin_package.rs` and `tests/screen_schemas_test.rs` hardcode the shipped count).

---

# Carried forward from the completed pinning initiative

That initiative is **done and reviewed** (15 sessions). **The full detail — the fixture trap, the
23 standing rulings, measurement method, open dithering defects — is in the previous version of
this file: `git show 3b32762:docs/HANDOVER.md`.** Read it before touching `eink-dither`, gamut
mapping, or anything about colour models.

The rules that generalise beyond it, and still bind:

- **The plan's code and constants are not evidence.** Measure before believing the plan, your own
  diagnosis, a reviewer's "harmless", the spec, or your own eyes on a downscaled PNG. Thirteen
  plan-authored tests measured unfounded; eight doc comments claimed what their own code disproved.
- **A comparison test must assert its comparison is non-degenerate**, and a test claiming "X
  rescues this case" must assert the case needed rescuing.
- **A clean review of every task does not mean the feature is right, or the docs are right.** A
  shipped screen was visibly broken while every task reviewed clean and the gate was green.
  **Render something and show the owner, early and often** — it has now paid off in sessions 8, 11,
  12, 14, 15 **and 16** (the owner asked to see the font specimens and rejected the unhinted basis
  for the decision). **Budget for it.**
- **Changelog and docs are part of the diff** — review them against the code, not against the plan.
- **IDE diagnostics lie in this tree.** Verify with an actual `cargo` run; never take a subagent's
  "all green" at face value.

## What is still left on PR #30 itself

1. **Get CI green** — the three failures above.
2. **Merge prep** — re-read `CHANGES.md`'s Unreleased section as a whole; it is long and was
   written across many sessions. Ruling 21's combined gamut+pinning entry is written.
3. Non-blocking minors from the final review's triage: two overstated test names in
   `dither/mod.rs`; `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so
   HyAB and its `kchroma = 10` tuning are not on the crate's dithering path at all — **documented,
   not changed; a live design question for the owner.**
