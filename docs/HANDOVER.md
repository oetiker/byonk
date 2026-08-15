# Handover — Byonk

_Last updated: 2026-08-16 (session 19). **Initiative: adopt the resvg `byonk-base` branch.**
Plan Tasks 1, 2, 3, 5, 6 are complete and reviewed. Tasks 4, 7, 8, 9, 10 remain.
Task 11 (the font-trio decision) ran and **the owner has decided: bundle Source.**
The second front — the bitmap-font rendering defect (F15) — is **diagnosed, fixed, pushed
to PR #1115 and merged into `byonk-base`.** byonk itself is unchanged until its
`Cargo.lock` is bumped; see "What F15 needs next"._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main` — https://github.com/oetiker/byonk/pull/30 |
| HEAD | `46a50a3` — tree clean (the commits after `f755241` are docs only) |
| Verified | `make check` green at `f755241` (474 tests, 0 failed); nothing in code has changed since |
| **Not pushed** | HEAD is ahead of origin. Push before relying on CI. |

**resvg work happens in a different repo.** `oetiker/resvg` carries three branches that
matter: `feat/bitmap-mask-glyphs` (upstream PR #1115), `feat/font-hinting` (upstream PR
#1116), and `byonk-base`, which merges them and is what byonk's `[patch.crates-io]` pins.

**The plan:** `docs/superpowers/plans/2026-08-15-resvg-byonk-base-integration.md`.
Still authoritative for Tasks 4, 7, 8, 9, 10 — but see "the plan has been wrong a lot".

**The ledger is the recovery map:**
`.superpowers/sdd/2026-08-15-resvg-byonk-base-integration/progress.md` (git-ignored).
Every ruling, finding and commit range is there in far more detail than this file.
**Trust it plus `git log` over memory.** Also in that directory: `f11-report.md`,
`f15-report.md`, `font-licensing-research.md`, and the briefs `f9-brief.md` /
`f10-brief.md` (written, not yet dispatched). **`f15-report.md` predates the upstream
landing** — its measurements hold, but its "nothing is pushed" framing and its commit
hashes are superseded by the F15 section below.

**Two obsolete `.patch` files live there — ignore both.**
`f15-resvg-bitmap-strike-fix.patch` never held the fix at all (it holds `b67da7c` +
`303e38e`, which were already on `origin/byonk-base`), and
`f15-resvg-bitmap-advance-fix.patch` held the pre-upstream version. **`git log` on
`oetiker/resvg` is now the only source of truth for the resvg work.**

---

# ⚠️ Read this before touching fonts

**The bitmap fix is live in byonk** as of `e514271` — `Cargo.lock` pins `byonk-base`
`61956742`. `make check` green (474 passed, 0 failed — the same count as before the bump).
**Terminus @14 and @18 now render 1 px/glyph wider.** That is deliberate and correct: its
cells are 8x14 and 10x18, which a single `hmtx` advance cannot express, so byonk had been
crowding it. Do not "fix" it back.

---

# Completed in this initiative (session 18)

## Plan Task 3 — resvg bump + skrifa strikes (`94ff77f`, `b6d0315`)

resvg 0.46→0.48.1 on branch `byonk-base`, tiny-skia 0.12, fontdb 0.24 from crates.io,
**fontdb fork patch dropped**, `bitmap_strikes` reimplemented in
`src/rendering/font_strikes.rs` with skrifa.

The plan's Global Constraint "`test_bitmap_strikes_exposed` must pass unchanged" was
**unsatisfiable** — that test read `face.bitmap_strikes`, a field only the deleted fork
had. Ruling: re-point the access path only, every assertion verbatim. Proven non-vacuous
by sabotage.

## Plan Task 5 — `FontConfig` (`663ca48`)

`src/rendering/font_config.rs`: `FontConfig`, `FontVariant`, `HintingSpec`,
`HintingEngine`, `HintingTarget`, `HintingMode`, `adaptive_default(grey_count)`,
`to_usvg()`. The real usvg types are **`FontHinting*`-prefixed** — the plan guessed wrong.
Verified only one `usvg` copy resolves, same patched commit as resvg.

## Plan Task 6 — the font resolver (`e4a99c6`)

`fonts: Option<&FontConfig>` threaded through `render_to_palette_png` /
`render_to_raw_png` / `rasterize_svg` / `rasterize_tone_mask`; `FontResolver` installed
with lazy variant loading. The plan's fontdb API guesses were all wrong;
`load_font_source` returns the loaded IDs directly. **Five** call sites, not three.
`Arc::make_mut` mutation persistence proven and kept as a regression tripwire.

## F1 — the 1-bit glyph fix (`2fbb35a`, `f755241`) — NOT in the plan

**The bug:** byonk asked the hinter for mono hinting on BW panels but never told the
*rasteriser*; resvg anti-aliased the hinted outlines and the dither turned those greys
into speckle. Upstream documents it: *"Since resvg anti-aliases text, this mostly serves
to align stems to the pixel grid."*

**The fix:** `usvg::Options::text_rendering = OptimizeSpeed` when the config is
`Mono { aliased: true }`. Shape chosen so `aliased` has **no home outside the `Mono`
arm** — the bad combination is unconstructible. Both `Options` sites now come from one
`parse_options()`, so frame and tone mask cannot diverge.

**Design constraint this created — must be honoured by Task 7:** aliasing is
per-element and inheritable; hinting is per-face. Once per-element hinting exists, **any
element choosing smooth/no hinting on a BW panel inherits `optimizeSpeed`** and lands in
the known-bad aliased-without-mono state (tiny-skia has no dropout control; stems drop
out). The escape hatch is **`text-rendering: optimizeLegibility`**, which restores
anti-aliasing *and keeps hinting* — proven by test. **Trap: `geometricPrecision`
restores AA but also disables hinting.**

---

# Owner decisions — treat as settled

1. **Bundle the Source trio** as the generic-family fallbacks: `sans-serif` → Source
   Sans 3, `serif` → Source Serif 4, `monospace` → Source Code Pro. Supersedes Task 2's
   interim all-generics→Outfit mapping. **Outfit stays** as the house sans, referenced by
   name everywhere it already is.
2. **No fallback magic.** No grafting X11 strikes into Source, no size-conditional family
   substitution, no bitmap/outline hybrid. Generic names → Source, full stop. Designers
   choose bitmap faces explicitly, helped by the font demo screens.
3. **Fonts need licence files**, and the X11 licensing was researched (below).
4. **Re-run the bitmap-vs-outline comparison** once the bitmap fix lands — the earlier
   answer was measured against the bug.

---

# The second front: bitmap fonts render wrong (F15) — FIXED

**byonk ships 26 X11 bitmap TTFs that do not render correctly**, documents them in
`fonts/FONTS.md` and `docs/`, and demos them at `screens/examples/demo/font/bitmap/`.
Glyphs lose chunks at every size. This **predates this initiative**.

## Root cause — the owner's, not the agent's

The agent found that strikes are painted at fractional device coordinates and the
rectangle is anti-aliased, and fixed it by **snapping placement to whole pixels**.

**The owner challenged it** — *"fonts adjust their properties based on pt size, they are
not simply scaled"* — and was right. Verified:

- `skrifa-0.44.0/src/bitmap.rs:247` exposes `pub advance: Option<f32>`, *"the horizontal
  advance width of the bitmap glyph **in pixels**"*, populated from the strike metrics
  (`:307`). Strikes carry **per-ppem** metrics; they are not the outline scaled.
- **The code threw it away**, laying every bitmap glyph out on the outline's `hmtx`
  advance, which is fractional at nearly every ppem.

**True root cause: bitmap glyphs were laid out on outline metrics instead of their own
strike metrics.** This also dissolves the "separate issue" of uneven letter spacing —
same fault. Correct advances are integers, so glyphs land on whole pixels *by
construction* rather than being snapped afterwards. **Snapping was demoted from "the fix"
to a complement** — see below for why it was still kept.

## The defect was ours, not released resvg's

Checked before assuming: the fork's `CHANGELOG.md` Unreleased section says *"Previously
only PNG bitmap glyphs were rendered."* Upstream resvg draws **colour emoji** bitmaps
only, where glyphs generally have no outline and get scaled anyway. The monochrome mask
strike path is **our own unreleased feature** (PR #1115). So there was no upstream bug to
report, and the fix belonged **inside PR #1115** rather than as a follow-up.

**Falsified, so nobody chases it again:** the vertical-metric overflow (every X11
conversion has ascender > upem). Real malformation, **not** the cause — X11Helv @34 and
X11Misc10x @20 are pixel-perfect with the same malformed ascender, and no code in the
bitmap path reads the ascender.

**Why Terminus looked flawless:** it is not immune — it breaks identically when pushed off
the grid. Its outline advance is a uniform ½ em, which lands on a whole pixel at every
strike ppem, so it never goes off-grid on its own. That same uniform advance is why it is
*wrong* at 14 and 18 px, where the real cell is 8 and 10 wide rather than 7 and 9.

## Terminus is correct — settled with measurement, do not reopen

The theory "Terminus has bugs in its advances" was raised again this session and
**measured to destruction**. All 1359 glyphs of every strike in our own
`fonts/TerminusTTF.ttf` (v4.49.3):

| ppem | 12 | **14** | 16 | **18** | 20 | 22 | 24 | 28 | 32 |
|---|---|---|---|---|---|---|---|---|---|
| strike advance (`EBLC` `horiAdvance`) | 6 | **8** | 8 | **10** | 10 | 11 | 12 | 14 | 16 |
| outline advance (`hmtx`, 500/1000 em) | 6 | **7** | 8 | **9** | 10 | 11 | 12 | 14 | 16 |
| canonical `ter-uXXn` cell width | 6 | **8** | 8 | **10** | 10 | 11 | 12 | 14 | 16 |

**The strike matches canonical Terminus at every one of the nine sizes.** The outline is
what disagrees, at exactly two. And it cannot be fixed: `hmtx` holds **one** advance per
glyph that is merely scaled by size, so it encodes one ratio — but Terminus needs 0.500
for seven sizes, 0.571 for 8×14 and 0.556 for 10×18. **No single `hmtx` value is right at
all nine.**

That limitation is irrelevant, because **the strike carries its own advance and Terminus
fills it in correctly**. The font is complete; the consumer was reading the wrong table.
*Terminus is NOT buggy. No upstream report, no patch to our copy.* This reframes the fix
usefully: it is not a workaround for one imperfect font but the **only correct handling**,
since a strike advance is unrepresentable in `hmtx` whenever cell aspect varies by size.

## The fix — PUSHED to PR #1115

Two commits on `oetiker:feat/bitmap-mask-glyphs`, root cause first:

| | |
|---|---|
| `3cd6d6a5` | `fix: space a bitmap glyph by its own strike's advance` |
| `17f41cac` | `fix: blit a bitmap strike onto whole pixels` |

- Substitution happens in `form_glyph_clusters`, the single font-units→user-units point,
  so `text-anchor`, `textLength`, `letter-spacing`, `dx`/`dy`, `textPath`, decorations and
  bbox all keep working. `matching_mask` is shared by layout and flattening so the two
  cannot drift.
- **Only a mask strike that will actually be drawn contributes an advance.** Colour
  bitmaps and outline-less glyphs are untouched, so the emoji path this code was built
  for is unaffected — which is why **no reference image changed**.
- **Snapping was demoted but kept, on evidence.** With correct advances ordinary text is
  already grey-free at every size, but `x="20.5"` → 150 grey px, `letter-spacing="0.5"` →
  67, `text-anchor="middle"` with odd width → 150. Conversely snapping *alone* stutters
  the pitch (`10 11 10 10 10 11 10`); with advances it is constant.
- **The test font now carries a 14 px strike.** `make-bitmap-mono.py` builds
  `PPEMS = (14, 16, 24)`. 14 is the only size where strike (8) and outline (7) disagree;
  16 and 24 agree, which is exactly why nothing caught this before. Verified our
  `fonts/TerminusTTF.ttf` reproduces the previously committed test font **byte-for-byte in
  every table except `head.modified`**, so the added strike is the only real change.
- **Both fixes are sabotage-proven, and cross-cleanly:** disabling the advance wiring
  fails `a_strike_is_spaced_by_its_own_advance` (`left: 7.0, right: 8.0`) while the
  snapping test still passes; disabling snapping fails
  `a_strike_is_blitted_onto_whole_pixels` while the advance test still passes. Each test
  catches only its own fix, which is also the proof that the two commits are independent.
- Verified at `17f41cac`: `cargo fmt --check` clean, clippy only pre-existing warnings
  (the change *removes* a parameter), **1734 tests, 0 failures, no reference image
  changed**.
- **Adapted for upstream, three ways**, because the fix was written on `byonk-base` which
  also carries the hinting PR: the `select_bitmap` gating was dropped (that hook is not in
  #1115), a `[GlyphHinting::ppem]` doc link was reworded, and two `select_bitmap` tests
  were removed from the new test file.
- Byonk-side renders from the earlier verification round (pitch rulers, before/after
  sheets) are in the ephemeral `…/scratchpad/x11fix2/index.md`.
- Rows at 8/10/11 px in the Terminus sheets look poor in *both* before and after: Terminus
  has **no strike below 12**, so those are the outline path. Pre-existing, out of scope.

## `byonk-base` is merged and pushed — `61956742`

`feat/bitmap-mask-glyphs` merged into `byonk-base`, pushed. **1749 tests, 0 failures**,
fmt clean, tree clean.

**The merge needed one semantic fix, and it is the interesting part.** `byonk-base` commit
`b67da7c` had added `FontResolver::select_bitmap`; the upstream version of the advance fix
resolves `strike_source` **unconditionally**, because upstream has no such hook. Git merged
that cleanly and silently produced a bug: a host declining a font's strikes got its
**outline drawn but its strike advances used** — at 14 px, an 8 px pitch around a 7 px
glyph. `strike_source` is gated on `select_bitmap` again.

**The pre-existing reference-image test could never have caught this** — verified by
sabotage: with the gate removed, `strikes_can_be_declined_for_a_font` still passes, because
the document it renders uses 16 and 24, the sizes where Terminus's two advances agree. The
new `declining_strikes_also_declines_their_advance` is what pins it (`left: 8.0,
right: 7.0` when the gate is removed). **This is the trap to remember for any future merge
of upstream font work into `byonk-base`: byonk-base has host hooks upstream does not, and
a clean textual merge is not evidence the semantics survived.**

## What F15 needs next

Done: the pin bump and the `CHANGES.md` entry, both in `e514271`.

1. **A byonk-side regression test.** It could not exist before the pin moved, and there is
   still none — byonk has no test that fails if the pin regresses. The resvg-side tests
   (`a_strike_is_spaced_by_its_own_advance`, `declining_strikes_also_declines_their_advance`)
   do not run in byonk's suite.
2. **Re-run the bitmap-vs-outline comparison** (owner asked; the earlier answer was
   measured against the bug). Expect **Terminus @14 and @18 to change and nothing else**.
   This is also the owner-facing check that the fix looks right at true size — the
   Lessons below say show renders early.

---

# Font licensing — researched, evidence in the workspace

`.superpowers/sdd/…/font-licensing-research.md`. **Redistribution and modification are
permitted for everything in the tree** — the owner's earlier conclusion holds. What is
missing is **notices**; `fonts/` has no licence file of any kind.

Provenance read from each TTF's own `name` ID 0 (the importer preserved it):

| Family | Licence | Notice obligation |
|---|---|---|
| Outfit, Terminus (TTF) | OFL 1.1 | ship OFL text |
| X11Helv | Adobe 1984/87 + DEC 1988/91, MIT/X11-style | notice in copies **and documentation** |
| X11LuSans, X11LuType | **Lucida** (Bigelow & Holmes) | **verbatim notice in user documentation AND code comments** |
| X11Term | DEC 1991 | notice |
| X11Misc5x–10x | public domain | none |
| **X11Misc12x** | **Sony Corp. 1987/88** — *not* public domain | its own notice |

Two things to remember:
- **`X11Misc*` is a cell-width grouping the importer invented, not a licence grouping.**
  Notices must be per file.
- **Do not rename `X11LuSans`/`X11LuType` toward "Lucida".** The trademark licence
  applies only to *unmodified* fonts, and byonk modified them. The current names are
  correct.

---

# Queued work — briefs already written

| ID | What | Brief |
|---|---|---|
| **F9** | Resolve `AutoFallback` ourselves: a face with no `fpgm` and no `cvt` has no usable interpreter hinting, so substitute `Auto`. | `f9-brief.md` |
| **F10** | Bundle the Source trio, repoint generics, licences, docs. | `f10-brief.md` |
| F13 | Extend `screens/examples/demo/font/{ttf,bitmap,hinting}/` to cover Source. | — |
| F14 | Licence + notice files per the research above; fix `FONTS.md` (it lists `X11LuType` under *Proportional* — it is monospaced, 1 distinct advance). | — |
| F15 | The bitmap fix — **done**: PR #1115 + merged to `byonk-base`. Needs the `Cargo.lock` bump. | — |

**F9's motivation:** eight of the nine trio candidates have **no TrueType hinting
program** — a 7-byte `prep` stub (`b8 01 ff 85 b0 04 8d` = `SCANCTRL`/`SCANTYPE`, i.e.
"enable dropout control", which tiny-skia does not implement) and no `fpgm`/`cvt`. Only
Roboto has a real one. skrifa's `AutoFallback` tests whether `fpgm` *or* `prep` is
non-empty, so it picks the interpreter for all nine and **never falls back**. usvg
*defaults* to `AutoFallback`. byonk sets `Auto` explicitly — keep that.
**Upstream will not change this:** googlefonts/fontations#1151, closed 2024 as
"No issue here for Fontations" — skrifa matches FreeType deliberately. **Do not PR it.**

**F10's two hazards, both to settle by rendering, not argument:**
Source Sans 3 and Source Code Pro **default to `wght` 200**, not 400 (Source Serif 4
defaults to 400). The plan says resvg always pushes `wght` from CSS `font-weight`, which
would make that moot — but the specimen work found Source Code Pro "far too light"
without an explicit pin. Both cannot be true. Second: Source Serif 4's `opsz` defaults to
**20**, and resvg pushes non-`wght` axes only when non-default — consistent with it being
the worst serif at 10 px and the best at 20 px.

---

# Remaining plan tasks

4. Render-scale warning. **7. `font_hinting` Lua directive** — must carry the F1 design
constraint above, *and* validate declared variant base families at parse time (a typo'd
variant silently renders in a different typeface; booked from Task 6's review).
8. Migrate screens + docs. 9. State-3 capture + pixel diff + **show the owner**.
10. Fix or delete `test_bitmap_font_render`.

**Baselines for Task 9** (regenerate rather than trust — `/tmp` does not survive reboot):
`/tmp/byonk-renders/state2-final` is the real pre-integration baseline; `state1` is
context only (10 of 13 silently fell back).

---

# Open questions for the owner

1. **`grey_count <= 2` may be the wrong rule.** On the 4-grey panel at 10–12 px,
   mono+aliased beats smooth (only 44% of ink reaches true black under smooth, on a ~3:1
   contrast panel) — but at 14 px smooth wins on the same panel. The specimen agent argues
   the real fix is a **size term**, not a wider grey threshold. Renders exist; not decided.
   Note the **4-colour** panel (`trmnl_og_4clr`) already counts as `grey_count = 2` (red
   and yellow are not greys) and gets mono+aliased today — it is the **4-grey**
   (`trmnl_og`) panel that is in question. *Always name panels by config key; the two
   behave oppositely.*
2. **Two inert knobs** byonk's API exposes: `HintingMode::Light` is byte-identical to
   `Normal`, and with `engine: Interpreter` the `target` has no effect.
3. A typo'd screen ref in `config.yaml` silently renders the DEFAULT screen
   (`content_pipeline.rs:204-223`). Still unfixed.
4. `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and
   its `kchroma = 10` tuning are not on the crate's dithering path at all.

**Owner-facing artifact** (all specimens, both bugs, the data):
https://claude.ai/code/artifact/f7ef39be-1a9d-4c97-bd95-d9b3422a515e — redeploy by
republishing `…/scratchpad/type-trials.html` (built by `scratchpad/build_page.py`).
Both live under this session's scratchpad and are **ephemeral**.

---

# Lessons — these keep paying off

- **The plan's code is not evidence.** Wrong in *five of five* tasks touched this session:
  a tautological test, an unsatisfiable constraint, wrong usvg type names, wrong fontdb
  API, wrong font-repo paths, an incomplete call-site list. Verify every symbol.
- **Demonstrate the check fails when the thing is broken.** Sabotage caught real holes
  three times. A test that passes with the fix reverted is worthless.
- **Check the domain fact before calling something a bug.** Three theories were wrong
  here: the X11 vertical-metric overflow (real malformation, not the cause), "TerminusTTF's
  odd-width strikes are mispackaged", and "Terminus has bugs in its advances" (raised
  twice; the strikes match canonical Terminus at all nine sizes — see the table above).
  Measure the font, then check what the font is *supposed* to be.
- **Ask whose bug it is before deciding where to fix it.** I asserted the advance defect
  was upstream resvg's; the fork `CHANGELOG` said otherwise in one line — upstream renders
  only PNG bitmap glyphs, and the mask path is our own unreleased feature. That single
  check moved the fix from "follow-up patch" into PR #1115, where it belongs.
- **A saved artifact is not evidence that it holds what its name says.**
  `f15-resvg-bitmap-strike-fix.patch` was recorded as *the* durable copy of the fix and
  actually contained two already-pushed commits. The real work survived only because the
  ephemeral clone happened not to have been cleaned up. Diff a preserved patch against
  what it claims to preserve.
- **Order a commit series by cause, not by the order you discovered things.** The fix was
  written snapping-first, then advances; shipped advances-first. Reordering it stopped
  commit 1 from asserting a cause that commit 2 overturns.
- **An isolating experiment can be sound and still stop one level short.** It only varies
  what the experimenter already believes matters. The owner's domain knowledge broke open
  both the aliasing bug and the bitmap-advance bug. **Show the owner renders early.**
- **Never run `make check` while an agent mutates the tree.** It failed on a mid-edit
  syntax error and the result was meaningless. Also: `make check > log; echo "EXIT=$?"`
  reports the *echo's* status — use `|| echo FAILED >> log`.
- **Judge type at true size.** 124.7 dpi, so 10 px = 5.8 pt. Conclusions drawn from 5×
  crops did not survive 1:1 renders.
- **Verify a claim before relaying it.** Twice this session I passed on a subagent
  conclusion that was wrong (a "regressed" demo screen that was inert; "strikes arrive
  resampled" when it was edge coverage).

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~10 min — background it**; it runs
  `cargo fmt`, not `--check`, so it rewrites files.
- **Subagents must not run `make check`** — the 600 s watchdog kills them. Give them
  `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` + targeted `cargo test`.
- Tell implementers to run cargo in the **foreground** with a generous timeout (Bash
  accepts 600000 ms). Backgrounding and ending the turn stalls them.
- `CARGO_BUILD_JOBS=2` — shared machine. `cargo test` takes only **one** filter.
- Pre-existing `#[ignore]` failures, unrelated:
  `preprocess::preprocessor::tests::{test_process_with_resize,
  test_resize_before_enhancement, test_resize_full_pipeline_with_photo_preset}`.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`.
- **IDE diagnostics lie in this tree** — they showed phantom `E0061`s at `e4a99c6` that a
  real `cargo check` disproved. Only an actual cargo run counts.
- **Do not split `src/rendering/svg_to_png.rs`** — it would collide with PR #30's diff.
- Rendering a builtin screen needs a device with an explicit `panel:`; see
  `tools/capture-config.yaml`.
- Fonts for the trio work are at `…/scratchpad/gfonts/ofl/` — Roboto is under `ofl/`,
  **not** `apache/` as the plan says.
- **Working on resvg:** clone `oetiker/resvg` into the scratchpad. Its test suite is fast
  (~11 s, 1734 tests) and safe to run in the foreground — this is *not* byonk's `make
  check`. `crates/resvg/tests/fonts/make-bitmap-mono.py` regenerates the bitmap test font;
  it needs `fontTools` and `TerminusTTF-Regular.ttf` beside it, which is a copy of byonk's
  `fonts/TerminusTTF.ttf`. Output is reproducible apart from `head.modified`.

---

# Carried forward

The pinning initiative is done and reviewed; its detail is in
`git show 3b32762:docs/HANDOVER.md` — read before touching `eink-dither`, gamut mapping,
or colour models.

Still open on PR #30 itself: re-read `CHANGES.md`'s Unreleased section as a whole before
merge (long, written across many sessions — and one F1 entry promises crisp BW text that
is only true once Task 7 wires the real `FontConfig`; all production call sites still pass
`None`), and two overstated test names in `dither/mod.rs`.
