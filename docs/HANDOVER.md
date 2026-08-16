# Handover — Byonk

_Last updated: 2026-08-16 (session 22). **Initiative: adopt the resvg `byonk-base`
branch.** Plan Tasks 1, 2, 3, 5, 6 done; 4, 7, 8, 9, 10 remain. **F16 is DONE and landed**
(`1ce8210`). Next up: **F9, F10, and plan Tasks 4, 7, 8, 9, 10** — see Queued work._

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main` — https://github.com/oetiker/byonk/pull/30 |
| HEAD | `1ce8210` plus this handover commit — tree clean |
| Verified | `cargo test --workspace` green at `1ce8210`: **1086 passed, 0 failed, exit 0** |
| Pushed | level with `origin` as of `3e47517`; CI on #30 has seen the new pin |

**resvg work happens in a different repo.** `oetiker/resvg` carries `feat/bitmap-mask-glyphs`
(upstream PR #1115), `feat/font-hinting` (upstream PR #1116), and `byonk-base`, which merges
them and is what byonk's `[patch.crates-io]` pins. Current pin: `2e766508`.

**The plan:** `docs/superpowers/plans/2026-08-15-resvg-byonk-base-integration.md` — still
authoritative for Tasks 4, 7, 8, 9, 10, but the plan has been wrong in five of five tasks
touched. Verify every symbol.

**The ledger:** `.superpowers/sdd/2026-08-15-resvg-byonk-base-integration/progress.md`
(git-ignored). Also there: `f11-report.md`, `f15-report.md`, `font-licensing-research.md`,
`f9-brief.md`, `f10-brief.md`. **Ignore the two `f15-*.patch` files — neither holds what
its name claims.** `git log` on `oetiker/resvg` is the source of truth for resvg work.

---

# F16 — DONE

## The state in one paragraph

The importer was rewritten in `f2a075f`; the 26 regenerated fonts, the pin bump,
`FONTS.md` and the `CHANGES.md` entry landed in **`1ce8210`**. The owner chose to put the
resvg fix on **upstream PR #1115 as well as the fork**, so it was cherry-picked onto
`feat/bitmap-mask-glyphs` (`d2ef8ee8`, pushed — this **updates upstream PR #1115**) and
merged into `byonk-base` (**`2e766508`**, pushed). Byonk's `Cargo.lock` pins `2e766508`.

**The cherry-pick was not clean, and the conflict mattered:** the byonk-side test used
`FontResolver::select_bitmap`, which does not exist upstream. The upstream copy therefore
carries the new test *without* the `allow_strikes` plumbing and *without*
`declining_strikes_also_declines_their_advance`; `byonk-base` keeps both. The merge back
into `byonk-base` was checked by `git diff 235f499 <merge>` coming out **empty** — that is
the guard against the merge trap recorded under F15.

Verified after landing: resvg **1750 passed / 0 failed** on `byonk-base`, sabotage fails
only `an_outline_free_strike_is_spaced_by_its_own_advance`; byonk `cargo test --workspace`
**1086 passed / 0 failed**; the pitch ruler through byonk is **11/11** (3/11 before).
Specimens for the owner: https://claude.ai/code/artifact/ef06c1db-b5ba-467c-8cc3-3a7069e00488

Byonk's branch is pushed and PR #30 carries all of it.

## The resvg change — landed

`crates/usvg/src/text/bitmap.rs`, `mask_advance` had:

```rust
// A glyph with no outline at all keeps whatever strike it has, at any
// size, so its image is scaled and its advance is not this one.
font.outline_glyphs().get(glyph_id.into())?;
```

It is wrong. `matching_mask`, called on the next line, **already** narrows to the strike
whose ppem equals the requested size, and `glyph()` draws that strike unscaled whether or
not an outline exists. So the guard only ever suppressed the advance of a strike that
*was* being drawn — in the one case that can least afford it, because an outline-free font
has nothing but `hmtx` to fall back on and `hmtx` can be right at one size at most.
Deleting the line is the whole fix.

| | |
|---|---|
| Upstream | `d2ef8ee8` on `oetiker/resvg` `feat/bitmap-mask-glyphs` — **this is PR #1115** |
| Fork | `2e766508` on `byonk-base` (merge of the above); byonk pins this |
| **Durable copy** | `.superpowers/sdd/2026-08-15-resvg-byonk-base-integration/f16-resvg-outline-free-advance.patch` |
| Tests | `byonk-base` **1750 passed, 0 failed**; PR branch **1804 passed, 0 failed**; no reference image changed |
| Sabotage | reinstating the guard fails **only** `an_outline_free_strike_is_spaced_by_its_own_advance` — checked on both branches |

It adds a test font `BitmapMonoNoOutline.subset.ttf` (BitmapMono with its outline tables
deleted) emitted by the extended `make-bitmap-mono.py`, and consolidates the
`last_inked_column*` helpers — which also removes a **pre-existing** dead-code warning on
`byonk-base` (confirmed pre-existing by stashing).

**Why the five existing tests could not catch it: every one of them uses a font with
outlines.** Same shape of trap as the `select_bitmap` merge bug recorded under F15.

**The fonts and the pin must move together.** With the old pin the advances are correct in
the file and *ignored* at render time, giving fractional pitch — worse than before. If the
pin is ever rolled back, roll the fonts back with it.

## What was built (all committed in `f2a075f`)

`fonts/x11importer/` — pure Python + fontTools, no FontForge, no potrace, no X11 install:

| File | Job |
|---|---|
| `bdf.py` | read BDF, keep `DWIDTH` and the XLFD cell width untouched |
| `sfnt.py` | write a bitmap-only sfnt (`EBDT` fmt 1 + `EBLC` fmt 1, **no `glyf`**) |
| `verify.py` | compare the built font back against its sources; `cell_checked` reports coverage |
| `families.py` | which BDFs make up each of the 26 faces |
| `sources.py` | 10 X.Org tarballs pinned by version + SHA-256 |
| `cli.py` | fetch → build → verify → write; **refuses to write if anything disagrees** |

`fonts/tests/` — 18 tests, `make fonts-check`. `make fonts` rebuilds. Venv `.venv-fonts`
is separate from the HA `.venv` so the dependency sets cannot collide.

Also in `f2a075f`: **`EmbeddedFonts` is now restricted to font extensions**
(`src/assets.rs`). It embedded all of `fonts/`, so the new Python subdirectories broke
`test_init_fonts` (it tried to write into directories it never created), and `FONTS.md`
plus a stray `.DS_Store` were being shipped in the binary and handed to fontdb. Pinned by
`only_font_files_are_embedded`.

## Results — measured, not asserted

- 26 fonts, **same strike inventory, zero glyphs lost** (except X11Term, below).
- **8.7 MB → 4.9 MB.** Byte-identical across two runs (deterministic; `head.created`/
  `modified` are hard-coded).
- **40% of all advances changed** (58 765 of 144 412).
- Pitch ruler through byonk, the 11 rows of the old F16 table: **8 wrong before, 0 after.**
- **Unplanned cross-check that held:** the faces the old handover listed as *already
  correct* (X11Misc10x @20, X11Misc12x @24, X11Misc6x Bold @13) came out with 0% of their
  advances changed.

Owner-facing renders (**ephemeral**): `…/scratchpad/demo-before.png` vs `demo-after.png` —
the shipped `demo/font/bitmap` screen with `font_prefix: X11Misc`. `bold 18px/X11Misc9x`
was rendering as `b o l d   1 8 p x` (9 px cell spaced at 14); after, every row is tight.

## Three findings that were NOT in the diagnosis

1. **X11Term spans two foundries.** `term14.bdf` in `font-bitstream-75dpi` is
   `-DEC-Terminal-…-14-…-C-80-`; the **same filename** in `font-bitstream-100dpi` is
   `-Bitstream-Terminal-…-18-…-C-110-`. That is where its two strikes come from. **F14 must
   put two notices in one file.** (The old handover credited `font-dec-misc`; that package
   holds only `deccurs`/`decsess`.)
2. **X11Term never had a plain apostrophe.** Upstream puts `quoteright` at ENCODING 39 and
   `quoteleft` at 96. The old conversion relocated them to U+2019/U+2018, so `'` and
   `` ` `` were blank in a *terminal* font. The rebuild restores them and drops U+0152,
   U+0153, U+0178, U+2018, U+2019, U+2212 — none of which are in the ISO 8859-1 range the
   face is drawn for. Net: 194 → 195 codepoints, 188 shared.
3. **`lub*` (LucidaBright, a serif) was mapped into `X11LuSans`** beside `luRS*` (Lucida
   Sans). Only a size-dedup accident kept serif glyphs out of the sans font. Verified
   LucidaBright has **no size Lucida Sans lacks**, so dropping it changes nothing in the
   output. Prefixes are now matched as `prefix\d*$`, so `helvB` cannot swallow `helvBO08`
   and the old sort-longest-first hack is gone.

## Design decisions worth not relitigating

- **Sources are X.Org tarballs, not Debian `.pcf.gz`.** They ship the original `.bdf`, so
  nothing sits between us and the ground truth. 10 packages, SHA-256 pinned in
  `sources.py`. Needed: adobe-75/100dpi, bh-75/100dpi, bh-lucidatypewriter-75/100dpi,
  bitstream-75/100dpi, misc-misc, sony-misc.
- **Where two sources give the same pixel size** (a 14 pt face at 75 dpi and a 10 pt at
  100 dpi are both 14 px), the **75 dpi one wins** — same as the old importer, so the
  strike inventory is unchanged and only the metrics move.
- **`upem = largest_ppem × 100`**, and `hmtx` comes from the largest strike. Same
  convention the old FontForge output used.
- **Not every fixed-pitch source declares a whole-pixel cell** — `lutRS19` says `M-159`,
  an average, while all 873 glyphs advance 16. The XLFD check skips those rather than
  invent a rounding, and the run **prints how many strikes got both checks** so a skipped
  one cannot read as a pass.
- **Behaviour change the owner accepted implicitly by asking for outline-free:** at a size
  with no strike, the renderer now **scales the nearest strike** (blocky, same typeface,
  right width) instead of falling back to a soft autotraced outline. Proven by probe; on a
  4-grey panel it reads *better*, because the traced fallback resolved to mid greys.

---

# Settled — do not reopen

## Owner decisions

1. **Bundle the Source trio** as generic-family fallbacks: `sans-serif` → Source Sans 3,
   `serif` → Source Serif 4, `monospace` → Source Code Pro. **Outfit stays** as the house
   sans, referenced by name where it already is.
2. **No fallback magic.** No grafting X11 strikes into Source, no size-conditional family
   substitution, no bitmap/outline hybrid. Designers choose bitmap faces explicitly.
3. **Fonts need licence files** (research below).
4. **Bitmap fonts should have no outlines if possible** — asked for in session 21, and
   that is what F16 delivers.

## F15 — the bitmap strike fix, DONE and live

Bitmap glyphs were laid out on the outline's `hmtx` advance instead of their own strike
metrics. Fixed in `oetiker/resvg` (`3cd6d6a5` + `17f41cac`), merged into `byonk-base`
`61956742`, pinned in byonk `e514271`. **Terminus @14 and @18 render 1 px/glyph wider —
that is correct, do not "fix" it back.**

**Terminus is NOT buggy.** Measured to destruction across all 1359 glyphs of all nine
strikes: the strike advances match canonical `ter-uXXn` at every size (6 8 8 10 10 11 12
14 16); only the outline disagrees, at 14 and 18, and no single `hmtx` value can be right
at all nine. Raised twice, settled twice. **No upstream report, no patch to our copy.**

**Merge trap to remember:** `byonk-base` has host hooks upstream does not
(`FontResolver::select_bitmap`). A clean textual merge of upstream font work is **not**
evidence the semantics survived — one such merge silently produced "outline drawn but
strike advances used".

**F15 still owes a byonk-side regression test.** The resvg-side tests do not run in
byonk's suite, so nothing in byonk fails if the pin regresses.

## Falsified — do not chase again

- **X11 vertical-metric overflow** (ascender > upem in every conversion): real
  malformation, **not** a cause of anything. No code in the bitmap path reads the ascender.
- **Ink overhang in the oblique faces**: `TerminusTTF-Italic` overhangs on 40.5% of its
  glyphs too. Slanted bitmap faces overhang normally.

## Font licensing — researched

`.superpowers/sdd/…/font-licensing-research.md`. Redistribution and modification are
permitted for everything in the tree; what is missing is **notices** — `fonts/` has no
licence file at all.

| Family | Licence | Obligation |
|---|---|---|
| Outfit, Terminus (TTF) | OFL 1.1 | ship OFL text |
| X11Helv | Adobe + DEC, MIT/X11-style | notice in copies **and documentation** |
| X11LuSans, X11LuType | **Lucida** (Bigelow & Holmes) | verbatim notice in user docs **and code comments** |
| X11Term | **DEC 1991 *and* Bitstream** — see finding 1 | both notices |
| X11Misc5x–10x | public domain | none |
| **X11Misc12x**, **X11Misc8x @16** | **Sony Corp. 1987/88** | its own notice |

- **`X11Misc*` is a cell-width grouping, not a licence grouping.** Notices must be per
  source file. The importer now writes every distinct source `COPYRIGHT` into name ID 0.
- **Do not rename `X11LuSans`/`X11LuType` toward "Lucida"** — the trademark licence covers
  unmodified fonts only, and byonk modified them.

---

# Queued work

| ID | What |
|---|---|
| ~~F16~~ | **DONE** — landed in `1ce8210`. See the F16 section above. |
| **F9** | Resolve `AutoFallback` ourselves: a face with no `fpgm`/`cvt` has no usable interpreter hinting, so substitute `Auto`. Brief: `f9-brief.md` |
| **F10** | Bundle the Source trio, repoint generics, licences, docs. Brief: `f10-brief.md` |
| F13 | Extend `screens/examples/demo/font/{ttf,bitmap,hinting}/` to cover Source. |
| F14 | Licence + notice files per the table above. **`FONTS.md`'s "X11LuType is proportional" is wrong — it is monospaced**; the F16 draft already fixes this. |
| F15 | Owes a byonk-side regression test (above). |
| F17 | `font-family="Terminus (TTF)"` is invalid CSS — parentheses must be quoted, or the text silently falls back to a serif. **`screens/examples/demo/font/ttf/screen.svg` has therefore never rendered Terminus.** Quote it, rename the family, or have byonk quote on the author's behalf. Fold into Task 8. |

**F9's motivation:** eight of nine trio candidates have no TrueType hinting program — a
7-byte `prep` stub and no `fpgm`/`cvt`. skrifa's `AutoFallback` tests whether `fpgm` *or*
`prep` is non-empty, so it picks the interpreter for all nine and never falls back. byonk
sets `Auto` explicitly — keep that. **Upstream will not change this**
(googlefonts/fontations#1151, closed "No issue here"). Do not PR it.

**F10's two hazards, to settle by rendering not argument:** Source Sans 3 and Source Code
Pro default to `wght` 200, not 400; the plan says resvg always pushes `wght` from CSS
`font-weight`, but the specimen work found Source Code Pro "far too light" without an
explicit pin — both cannot be true. And Source Serif 4's `opsz` defaults to 20, while
resvg pushes non-`wght` axes only when non-default.

# Remaining plan tasks

4. Render-scale warning.
7. **`font_hinting` Lua directive** — must carry the F1 design constraint (below) *and*
   validate declared variant base families at parse time.
8. Migrate screens + docs (fold in F17).
9. State-3 capture + pixel diff + **show the owner**. Baseline
   `/tmp/byonk-renders/state2-final` — regenerate rather than trust, `/tmp` does not
   survive reboot.
10. Fix or delete `test_bitmap_font_render`.

**F1 design constraint Task 7 must honour:** aliasing is per-element and inheritable;
hinting is per-face. Once per-element hinting exists, any element choosing smooth/no
hinting on a BW panel inherits `optimizeSpeed` and lands in the known-bad
aliased-without-mono state (tiny-skia has no dropout control; stems drop out). The escape
hatch is **`text-rendering: optimizeLegibility`** — restores AA *and keeps hinting*.
**Trap: `geometricPrecision` restores AA but disables hinting.**

---

# Open questions for the owner

1. **`grey_count <= 2` may be the wrong rule.** On the 4-grey panel at 10–12 px
   mono+aliased beats smooth, but at 14 px smooth wins — the real fix may be a **size
   term**, not a wider grey threshold. *Always name panels by config key:* the **4-colour**
   `trmnl_og_4clr` already counts as `grey_count = 2`; it is **4-grey** `trmnl_og` that is
   in question, and they behave oppositely.
2. **Two inert knobs:** `HintingMode::Light` is byte-identical to `Normal`, and with
   `engine: Interpreter` the `target` has no effect.
3. A typo'd screen ref in `config.yaml` silently renders the DEFAULT screen
   (`content_pipeline.rs:204-223`). Still unfixed.
4. `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and
   its `kchroma = 10` tuning are not on the crate's dithering path at all.
5. ~~`CHANGES.md` dangling fragment.~~ **Fixed in `1ce8210`.** The lost line was
   `- **Text on black-and-white panels is crisp instead of speckled.** Small type used`,
   recovered verbatim from `2fbb35a` (`git log -S` on the surviving text found it), not
   rewritten. **Still re-read the whole Unreleased section before merging #30** — one F1
   entry promises crisp BW text that is only true once Task 7 wires the real `FontConfig`.

**Owner-facing artifacts** (URLs outlive their ephemeral sources; to update, republish the
same `build_page.py` output **passing the existing URL**, or a second artifact is created):

| What | URL |
|---|---|
| **X11 Bitmap Specimens** — all 26 rebuilt faces, F16 before/after, the pitch table (session 22) | https://claude.ai/code/artifact/ef06c1db-b5ba-467c-8cc3-3a7069e00488 |
| Bitmap vs outline; F15 before/after; F16 diagnosis; F17 (session 20) | https://claude.ai/code/artifact/8fe47446-49b6-4256-9db6-429aa3b8bfb6 |
| Type trials: specimens, two bugs, the data (session 19) | https://claude.ai/code/artifact/f7ef39be-1a9d-4c97-bd95-d9b3422a515e |

---

# Lessons — these keep paying off

- **A fixture where ink width equals the advance cannot catch an advance bug.** The first
  F16 test fixtures made bitmap width == `DWIDTH`, so sabotaging the advance wiring to
  `advance = width` passed all 12 tests. Giving every fixture glyph a side bearing turned
  the same sabotage into 3 failures. This is the *same* blind spot that let F16 ship: 16
  and 24 px are the sizes where Terminus's two records agree.
- **Demonstrate the check fails when the thing is broken.** Sabotage caught real holes four
  times across these sessions. A test that passes with the fix reverted is worthless.
- **When the data is right and the render is still wrong, suspect the consumer's guards.**
  F16's advances were correct in the file and the pitch was still fractional; the cause was
  an over-broad early-return in our own renderer. Read the code path, do not re-measure the
  data.
- **Two code paths that must agree need the same predicate.** `glyph()` drew an exact
  strike for an outline-free glyph while `mask_advance()` refused to space it. The
  asymmetry *was* the bug, and the doc comment on `matching_mask` had even warned about
  the mirror-image case.
- **Always carry a control through a font measurement.** Outfit proved a diff was the
  strike change; `TerminusTTF` proved the derived-advance signature was ours;
  `TerminusTTF-Italic` killed the overhang theory. A measurement with no control cannot
  tell "this font is broken" from "this is how fonts are".
- **Check the domain fact before calling something a bug**, and **ask whose bug it is
  before deciding where to fix it.**
- **A screen that renders is not a screen that rendered what you asked for.** The canary
  device caught a silent fallback within minutes this session (see Build/verify).
- **The plan's code is not evidence.** Wrong in five of five tasks touched. Verify symbols.
- **An isolating experiment can be sound and still stop one level short** — it only varies
  what the experimenter already believes matters. **Show the owner renders early;** owner
  domain knowledge broke open the aliasing bug, the bitmap-advance bug and F16.
- **A saved artifact is not evidence that it holds what its name says.** Diff a preserved
  patch against what it claims to preserve — `f16-resvg-outline-free-advance.patch` was
  applied to a clean tree and checked before being trusted.
- **Never run `make check` while the tree is being edited.** Also `make check > log; echo
  "EXIT=$?"` reports the *echo's* status — use `|| echo FAILED >> log`.
- **Judge type at true size.** 124.7 dpi, so 10 px = 5.8 pt.
- **A flattering test string hides font defects.** Pick sample text that stresses the
  widest glyphs (`x X H v /`, not `Render jpq 0123`).
- **Set CSS `height: auto` on any image you scale by width alone.**

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~10 min — background it**; it runs
  `cargo fmt`, not `--check`, so it rewrites files. Green state = **1086 passed**.
- **Changing `Cargo.lock`'s resvg pin forces a full rebuild of usvg/resvg and everything
  downstream — ~10+ min. Always background it**; a 600 s foreground timeout will kill it.
- **Subagents must not run `make check`** — the 600 s watchdog kills them. Give them
  `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` + targeted `cargo test`.
- `CARGO_BUILD_JOBS=2` — shared machine. `cargo test` takes only **one** filter.
- Pre-existing `#[ignore]` failures, unrelated: `preprocess::preprocessor::tests::{…}`.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`.
- **IDE diagnostics lie in this tree.** Only an actual cargo run counts.
- **Do not split `src/rendering/svg_to_png.rs`** — it would collide with PR #30's diff.

## Fonts

- `make fonts-setup` (once) → `.venv-fonts`; `make fonts-check` (18 tests, instant);
  `make fonts` (rebuild all 26, deterministic). Downloads cache in `fonts/.x11-cache/`
  (git-ignored).
- **Rendering a scratch screen.** Put screens in a directory with a `byonk-screens.yaml`
  manifest — **without it the repo is skipped and every render silently falls back.**
  **`EXAMPLES_DIR` registers under the fixed handle `examples`, NOT the manifest's
  `name:`.** To use your own handle, put it in the config instead:
  ```yaml
  screen_repos:
    probe: { path: /abs/path/to/dir }
  ```
  then `CONFIG_FILE=<cfg> ./target/debug/byonk render --mac <mac> --output x.png`.
- **Always include a canary device** whose `screen:` cannot resolve, point `DEFAULT` at
  `byonk-builtin/calibration/grey`, and `cmp` the two renders. Identical bytes mean you
  captured the fallback, not your screen. This caught a real mistake in session 21.
- `--use-actual false` gives spec colours (use for pixel diffs); the default gives the
  panel's measured colours (use for judging type).
- **Measuring pitch without assuming a glyph width:** render the same glyph N and 2N
  times, `pitch = (ink₂ₙ − inkₙ) / N`. Both bitmap width and side bearings cancel.
  **The whole rig is preserved** in `.superpowers/sdd/…/f16-probe/`: the ruler screen, a
  `cfg.yaml` with one device per family and a canary, `measure_pitch.py` (prints all 11
  rows with a verdict), and `build_page.py` (rebuilds the specimen artifact). Fix the
  absolute paths in `cfg.yaml` and `build_page.py` first — they name a dead scratchpad.
- **Swapping fonts without rebuilding:** `FONTS_DIR=<dir>` overrides embedded fonts **by
  filename**. Extract the committed ones with
  `git show HEAD:fonts/X11Foo.ttf > <dir>/X11Foo.ttf` to get a "before".
- **A bitmap face only renders as a bitmap at a size it has a strike for**, and nothing
  warns you. Terminus: 12 14 16 18 20 22 24 28 32. Check the strike list before concluding
  a render is wrong. `fonts/FONTS.md` lists them per family.
- **Working on resvg:** clone `oetiker/resvg` into the scratchpad. Its suite is fast
  (~11 s, 1750 tests) and safe in the foreground — this is *not* byonk's `make check`. To
  test byonk against a local resvg, point `[patch.crates-io]` in `Cargo.toml` at
  `<clone>/crates/{resvg,usvg}` — **back up `Cargo.toml` and `Cargo.lock` first and
  restore them after**, or you commit a path that only exists on one machine.
  `make-bitmap-mono.py` needs `fontTools` and a copy of `fonts/TerminusTTF.ttf` named
  `TerminusTTF-Regular.ttf` beside it; output is reproducible apart from `head.modified`,
  so restore the committed `BitmapMono.subset.ttf` after regenerating.

## Housekeeping

Two **stale scratch worktrees** are registered in this repo and their directories are
ephemeral. Remove with `git worktree remove` (or `git worktree prune`) when convenient:

```
…/6b605fbb-…/scratchpad/byonk-state1   (main)
…/bc0fc7e3-…/scratchpad/byonk-before   (detached 744fec8)
```

---

# Carried forward

The pinning initiative is done and reviewed; detail in `git show 3b32762:docs/HANDOVER.md`
— read before touching `eink-dither`, gamut mapping or colour models.

Still open on PR #30: re-read `CHANGES.md`'s Unreleased section as a whole before merge
(one F1 entry promises crisp BW text that is only true once Task 7 wires the real
`FontConfig`; all production call sites still pass `None`), and two overstated test names
in `dither/mod.rs`.
