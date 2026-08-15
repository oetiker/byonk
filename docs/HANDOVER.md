# Handover — Byonk

_Last updated: 2026-08-15 (session 18). **Initiative: adopt the resvg `byonk-base` branch.**
Plan Tasks 1, 2, 3, 5, 6 are complete and reviewed. Tasks 4, 7, 8, 9, 10 remain.
Task 11 (the font-trio decision) ran and **the owner has decided: bundle Source.**
Along the way this session opened a second front — a real rendering defect in bitmap
fonts — which is **mid-diagnosis with an agent still running.**_

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main` — https://github.com/oetiker/byonk/pull/30 |
| HEAD | `f755241` — tree clean |
| Verified | `make check` green at `f755241` (474 tests, 0 failed) |
| **Not pushed** | HEAD is ahead of origin. Push before relying on CI. |

**The plan:** `docs/superpowers/plans/2026-08-15-resvg-byonk-base-integration.md`.
Still authoritative for Tasks 4, 7, 8, 9, 10 — but see "the plan has been wrong a lot".

**The ledger is the recovery map:**
`.superpowers/sdd/2026-08-15-resvg-byonk-base-integration/progress.md` (git-ignored).
Every ruling, finding and commit range is there in far more detail than this file.
**Trust it plus `git log` over memory.** Also in that directory: `f11-report.md`,
`f15-report.md`, `font-licensing-research.md`, and the briefs `f9-brief.md` /
`f10-brief.md` (written, not yet dispatched).

---

# ⚠️ Something is running right now

An agent (`superpowers:systematic-debugging`) is redoing the bitmap-font fix — see
**F15** below. It works in a **scratchpad clone of the resvg fork**, not in byonk, and
was told to leave the byonk tree clean. Check `git status` before assuming.

**No push to `oetiker/resvg` is authorized.** The owner did not answer that question;
they challenged the diagnosis instead — and were right.

---

# What this session finished

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

# The second front: bitmap fonts render wrong (F15)

**byonk ships 26 X11 bitmap TTFs that do not render correctly**, documents them in
`fonts/FONTS.md` and `docs/`, and demos them at `screens/examples/demo/font/bitmap/`.
Glyphs lose chunks at every size. This **predates this initiative**.

## Root cause — the owner's, not the agent's

The agent found that strikes are painted at fractional device coordinates and the
rectangle is anti-aliased, and fixed it by **snapping placement to whole pixels**
(fork commit `0e3a6cb`; patch preserved at
`.superpowers/sdd/…/f15-resvg-snap-bitmap-glyphs.patch`).

**The owner challenged it** — *"fonts adjust their properties based on pt size, they are
not simply scaled"* — and was right. Verified:

- `skrifa-0.44.0/src/bitmap.rs:247` exposes `pub advance: Option<f32>`, *"the horizontal
  advance width of the bitmap glyph **in pixels**"*, populated from the strike metrics
  (`:307`). Strikes carry **per-ppem** metrics; they are not the outline scaled.
- **The fork throws it away.** `grep -n advance crates/usvg/src/text/bitmap.rs` returns
  nothing — only `inner_bearing_x/y` and `ppem_x/y`. The advance reaching layout comes
  from the shaper over the outline `hmtx`, and is fractional at nearly every ppem.

**True root cause: bitmap glyphs are laid out on outline metrics instead of their own
strike metrics.** This also dissolves the "separate issue" of uneven letter spacing —
same fault. Correct advances are integers, so glyphs land on whole pixels *by
construction* rather than being snapped afterwards. **Snapping is a band-aid and is not
being landed as the fix.**

**Falsified, so nobody chases it again:** the vertical-metric overflow (every X11
conversion has ascender > upem). Real malformation, **not** the cause — X11Helv @34 and
X11Misc10x @20 are pixel-perfect with the same malformed ascender, and no code in the
bitmap path reads the ascender.

**Why Terminus looked flawless:** it is not immune. It breaks identically when pushed off
the grid — its advance is exactly ½ em and its strikes are at even ppem, so it never
lands off it.

## The fix — done, in the fork, NOT pushed

Fork clone commits `0e3a6cb` (snapping) + `e72efe6` (strike advances), +47/−7 lines, no
API change. **The clone is ephemeral; the durable artifact is
`.superpowers/sdd/…/f15-resvg-bitmap-strike-fix.patch`.** byonk tree untouched.

- **Verified, not assumed:** `advance` is `Some` and a whole number of pixels for *every*
  strike byonk ships (X11Helv `H`@12 → strike 9 vs `hmtx` 8.82).
- Substitution happens in `form_glyph_clusters`, the single font-units→user-units point,
  so `text-anchor`, `textLength`, `letter-spacing`, `dx`/`dy`, `textPath`, decorations and
  bbox all keep working. The real coupling introduced: **layout now consults
  `select_bitmap`** (once per shaping run). `matching_mask` is shared by layout and
  flattening so the two cannot drift.
- **Snapping was demoted but kept, on evidence.** With correct advances ordinary text is
  already grey-free at every size, but `x="20.5"` → 150 grey px, `letter-spacing="0.5"` →
  67, `text-anchor="middle"` with odd width → 150. Conversely snapping *alone* stutters
  the pitch (`10 11 10 10 10 11 10`); with advances it is constant.
- **New finding: Terminus disagrees with itself at 14 and 18 px** — 8 px strike cell
  against a 7 px outline advance. The "flawless control" was crowding glyphs all along.
  Hence Terminus @14 is the one render that changes (3154 px) — **deliberate and correct**.
  Controller eyeballed `x11fix2/Terminus_{before,after}_4x.png`: identical at 8/10/11/12,
  wider and uncrowded at 14. X11Misc10x: 0 px changed.
- Pitch is now even at every size for every face (measured ruler in `x11fix2/index.md`).
- 2 new usvg unit tests pin the substitution; 1747 render tests pass, **no reference
  image changed**. An upstream end-to-end test is blocked: the only monochrome test font
  has strikes at 16/24 where Terminus agrees with itself, and regenerating it is not
  byte-reproducible here — recommendation in the report.
- Rows at 8/10/11 px in the Terminus sheets look poor in *both* before and after: Terminus
  has **no strike below 12**, so those are the outline path. Pre-existing, out of scope.

**To land:** owner authorization to push `oetiker/resvg`, then bump `Cargo.lock`, add a
byonk-side regression test (it cannot exist before the pin moves), and a `CHANGES.md`
entry. Then **re-run the bitmap-vs-outline comparison** (owner already asked for this).

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
| F15 | The bitmap fix — **in flight**. | — |

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

---

# Carried forward

The pinning initiative is done and reviewed; its detail is in
`git show 3b32762:docs/HANDOVER.md` — read before touching `eink-dither`, gamut mapping,
or colour models.

Still open on PR #30 itself: re-read `CHANGES.md`'s Unreleased section as a whole before
merge (long, written across many sessions — and one F1 entry promises crisp BW text that
is only true once Task 7 wires the real `FontConfig`; all production call sites still pass
`None`), and two overstated test names in `dither/mod.rs`.
