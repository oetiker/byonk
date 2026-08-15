# Handover — Byonk

_Last updated: 2026-08-15 (session 17). **Initiative: adopt the resvg `byonk-base` branch.**
The plan is written and under execution by subagent-driven development. Tasks 1–2 of 11 are
complete and reviewed; **Task 3 (the actual resvg bump) has not started.**_

> **CI IS GREEN on PR #30** as of `d3d410d`. The three `screen_store` failures that were red
> at the start of this session are fixed. Do not go looking for them.

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| PR | **#30**, OPEN against `main` — https://github.com/oetiker/byonk/pull/30 |
| HEAD | `d3d410d` — pushed, local == origin |
| Tree | clean |
| CI | **green** at `d3d410d` (Build, Check & Lint, Test, HA Validation, Release Scripts, CodeQL) |

**The plan is the source of truth for what to do next:**
`docs/superpowers/plans/2026-08-15-resvg-byonk-base-integration.md` (committed at `68bd30f`).
It has 11 tasks with full code, exact versions, and per-task TDD steps.

**The ledger is the recovery map:**
`.superpowers/sdd/2026-08-15-resvg-byonk-base-integration/progress.md` (git-ignored). It records
per-task completion, commit ranges, deferred minors, and the baseline capture locations. Trust
it plus `git log` over memory.

The two design specs live on `origin/docs/resvg-integration`, **not merged into this branch**.
Read with `git show origin/docs/resvg-integration:<path>`:
- `docs/superpowers/specs/2026-08-14-resvg-byonk-base-design.md` — how the fork branch was built
- `docs/superpowers/specs/2026-08-14-byonk-resvg-integration-design.md` — the byonk-side plan

---

# How to resume

Use **superpowers:subagent-driven-development** with the plan file above. The workspace already
exists; the ledger tells you Tasks 1 and 2 are complete. **Resume at Task 3.**

Generate each task's brief with
`.claude/plugins/…/subagent-driven-development/scripts/task-brief <plan> <N>` and dispatch a fresh
implementer per task. Do not paste prior-task history into a dispatch.

---

# Done so far

## Task 1 — render-capture harness (commits `f001d70`, `b649d2a`, `e5a7bbf`)

byonk has **no assertions on rendered output at all**, and resvg 0.48 will change rendering
("May result in small rendering changes", plus three text-positioning fixes). This harness is the
only guard.

- `./tools/capture-renders.sh <outdir>` renders the bundled screens via the `byonk render` CLI.
  Plain shell on purpose — it must run against older checkouts.
- `tools/capture-config.yaml` is `config.yaml` with a purpose-built `devices:` block.
- **7 deterministic screens** (4 calibration + 3 font demos), byte-reproducible across runs.
  **6 nondeterministic** (`mandelbrot`, `hello`, `builtin-default`, `gphoto`, `webscrape`,
  `swiss-departure-board`) — clock- or network-driven, captured into `nondeterministic/` for
  eyeballing, never diffed.
- `mandelbrot` is nondeterministic: its script seeds RNG from `time_now()`. The plan wrongly
  assumed otherwise; the repeat-run check caught it.
- `transit` was dropped — no screen ref for it exists anywhere in the tree.

**Two fix rounds, both substantive.** The harness originally recorded `exit=0` with no way to tell
a real render from a silent fallback. Round 1 added a canary plus a distinctness check; round 2
fixed the check's real hole by re-pointing the reserved `DEFAULT` device at
`byonk-builtin/calibration/grey` — clock-free and already captured — so **any** fallback now
collides byte-identically with a known PNG, whether one screen fell back or ten. Demonstrated by
deliberately breaking a screen ref and watching `DISTINCTNESS FAIL` fire.

**Known residual (Minor, disclosed in the manifest itself):** `calibration-color` runs on the
`trmnl_og_4clr` panel, so its fallback dithers through a different palette and is not caught by
byte comparison. The general fix is to capture a grey reference per panel in use.

### Baselines — later tasks need these

| State | Path | Notes |
|---|---|---|
| state 1 (pre-#30 `main`) | `/tmp/byonk-renders/state1` | **Only 3 of 13 are genuine renders**; the rest silently fell back on the old tree. Context for human assessment, **not a diff target**. |
| state 2 (#30, pre-integration) | `/tmp/byonk-renders/state2-final` | The real baseline. Note the `-final` suffix — the plan text says `state2`, which is the stale pre-fix capture. |

The state-1 worktree is at
`/private/tmp/claude-501/-Users-oetiker-checkouts-byonk/6b605fbb-…/scratchpad/byonk-state1`
(`/scratch` is read-only on this machine). `/tmp` captures will not survive a reboot —
**regenerate rather than trust them.**

## Task 2 — generic font families resolve to bundled fonts (commit `d3d410d`)

`fontdb::Database::new()` defaults the CSS generics to Arial / Times New Roman / Courier New, none
of which byonk bundles. macOS masked it via `load_system_fonts()`; on CI Linux nothing matched,
usvg skipped the text, and the blank render failed three `screen_store` tests.

**It was also a production bug.** The release image is `FROM scratch` — no system fonts — and
byonk's own `v1/base.svg`, `header.svg`, `footer.svg` and the built-in error screens all use
`font-family="sans-serif"`. That text rendered blank on real devices.

Fix: five `set_*_family` calls in `SvgRenderer::with_fonts`, mapping all generics to `Outfit` and
monospace to `Terminus (TTF)`. **They must stay after `load_system_fonts()`** — on Linux that call
parses fontconfig and would otherwise overwrite them.

**The mapping is interim.** Task 11 settles the variable-font trio and will revisit it.

---

# Task 3 is next: the resvg bump

Bump `resvg` 0.46→0.48.1, `tiny-skia` 0.11→0.12, `fontdb` 0.23→0.24 (crates.io), point
`resvg`/`usvg` at branch `byonk-base`, **drop the fontdb patch**, and reimplement `bitmap_strikes`
in byonk with skrifa. One task on purpose: dropping the patch removes the field, so the tree does
not compile between the two changes.

## The dependency audit is DONE — do not redo it

Verified against the actual crates on 2026-08-15:

- **Only `tiny-skia` and `fontdb` cross the resvg API boundary.** `usvg` arrives via
  `resvg::usvg`, so it is consistent by construction. `image`, `zune-jpeg`, `image-webp` and `png`
  share no types with resvg.
- **`tiny-skia` 0.12's only breaking change is `RadialGradient::new`** gaining a start-radius
  argument. byonk never constructs one; it uses `Pixmap`, `Color::WHITE/BLACK`, `as_mut()`,
  `Transform`. Expect no code changes despite the major bump.
- **`fontdb` 0.24 has no `FaceInfo::bitmap_strikes`** (zero occurrences in its `lib.rs`).
  Everything else byonk touches is unchanged. `Database::with_face_data(id, |data, index| …)`
  exists and is how to get font bytes back per face.
- **`skrifa` 0.44** is what usvg uses — pin the same to keep one copy. Verified:
  `BitmapStrikes::new(&FontRef)` → `.iter()` → `.ppem() -> f32`.
- **`png` 0.17 and 0.18 already coexist** in this tree. Not a problem.
- **`byonk-base` tip is `303e38e0`** ("Sort the test module declarations"). The specs cite
  `b67da7c0`; the branch moved one commit.
- resvg/usvg are edition 2024, MSRV 1.85.0. Local toolchain is stable 1.97.1 — fine.
- `usvg::Options` gained `font_hinting`; `FontResolver` now has four hooks. byonk builds `Options`
  with `..Default::default()`, which absorbs both.

**Contract tests that must pass completely unchanged:** `test_bitmap_strikes_exposed` and
`test_bitmap_font_families` in `src/rendering/svg_to_png.rs`. If either needs editing, the
substitution is wrong — stop.

A reference clone of the fork may still be at
`…/scratchpad/resvg` (branch `byonk-base`); re-clone if gone.

## Then, in order

4. Render-scale warning. 5. `FontConfig` + adaptive hinting default. 6. Install the `FontResolver`
(the one place the plan is guessing at fontdb API names — it says so). 7. `font_hinting` Lua
directive. 8. Migrate the 12 screens + docs. 9. State-3 capture + pixel diff + **show the owner**.
10. Fix or delete `test_bitmap_font_render`. 11. **The hinted font-trio comparison** — the
owner-facing decision this whole sequence was ordered around.

---

# Lessons from this session — these keep paying off

- **Demonstrate a check fails when the thing is broken.** Twice this session that caught a real
  defect that reasoning alone missed: Task 1's fallback detection had a hole invisible from the
  code (the 4clr-panel palette), and **the plan's own Task 2 test was tautological** — it derived
  its "bundled fonts" set from the very database it queried, so it could never fail, and it passed
  with the fix reverted. The implementer caught it and rebuilt it around an independent scratch
  database.
- **The plan's code is not evidence.** Two of the plan's concrete details were wrong on contact
  with the repo: the `capture-config.yaml` screen keys, and that test. Expect more.
- **A reviewer can be wrong too.** A reviewer asserted `main.rs` hard-errors on an unresolved
  screen ref; the implementer tested it and disproved it. Verify contested claims in the code
  yourself before picking a side.
- **Subagents stall when they background a long build and end their turn.** Tell implementers to
  run cargo in the **foreground** with a generous timeout (Bash accepts up to 600000 ms), and to
  poll within the turn if they must background something. This cost several restarts.

---

# Build / verify

- `make check` = fmt + clippy + full test suite. **~10 min — background it**, and note it runs
  `cargo fmt`, not `--check`, so it rewrites files in place.
- **Subagents must not run `make check`** — the 600 s stream watchdog kills them mid-run. Give
  them `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets` + targeted `cargo test`.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine.
- `cargo test` takes only **one** filter argument.
- Pre-existing `#[ignore]` failures, unrelated to anything current:
  `preprocess::preprocessor::tests::{test_process_with_resize, test_resize_before_enhancement,
  test_resize_full_pipeline_with_photo_preset}` — they panic in `resize_lanczos` by design.
- **Never `git add -A`.** `/Users/oetiker/checkouts/byonk/examples/` is an untracked near-copy of
  `screens/examples/`. Add by explicit path; verify `git diff --cached`.
- `make docs` needs `mdbook-mermaid`.
- Rendering a builtin screen needs a device with an explicit `panel:` — without it you silently
  get a greyscale render. `tools/capture-config.yaml` is a working example.
- **Before changing visible text on a builtin screen, `grep -rn "<old label>" src/ tests/`** — two
  tests assert screen labels literally, and adding a builtin screen has a fan-out
  (`tests/builtin_package.rs` and `tests/screen_schemas_test.rs` hardcode the shipped count).

---

# Open questions for the owner

1. **A typo'd screen ref in `config.yaml` silently renders the wrong screen.** In
   `run_script_for_device` (`src/services/content_pipeline.rs:204-223`), a registered device whose
   `screen:` fails to resolve falls through to the DEFAULT screen — no error, no warning. An
   operator sees a plausible display and no signal anything is wrong. Verified in code; there is
   even a test asserting the fallback. **Out of scope for this plan — is it worth its own fix?**
2. **The font trio** (Task 11): Source vs Noto vs Roboto, to be decided from **hinted** specimens
   rendered through byonk's own pipeline. IBM Plex is disqualified (no variable Serif or Mono).
   The owner rejected deciding from unhinted renders.
3. `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and its
   `kchroma = 10` tuning are not on the crate's dithering path at all — documented, not changed, a
   live design question.

---

# Carried forward from the completed pinning initiative

That initiative is **done and reviewed** (15 sessions). The full detail — the fixture trap, the 23
standing rulings, measurement method, open dithering defects — is in
`git show 3b32762:docs/HANDOVER.md`. Read it before touching `eink-dither`, gamut mapping, or
anything about colour models.

Still binding: **render something and show the owner, early and often** — it has paid off in
sessions 8, 11, 12, 14, 15 and 16. **Changelog and docs are part of the diff.** **IDE diagnostics
lie in this tree** — verify with an actual `cargo` run.

Also still open on PR #30 itself: re-read `CHANGES.md`'s Unreleased section as a whole before
merge (it is long and was written across many sessions), and two overstated test names in
`dither/mod.rs`.
