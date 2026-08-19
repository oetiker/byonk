# Handover — Byonk

_Last updated: 2026-08-18 (session 27). **The release is now PR-based and no longer needs a
PAT.** Session 26's resvg initiative is closed and merged (PR #30). This session replaced the
release workflow after it died on an expired token, and the new flow immediately caught a bug
the old one would have shipped. **0.18.0 is mid-flight — see "What to do next".**_

## Where the work lives

| | |
|---|---|
| Branch | `main` |
| HEAD | `df1c31f` — merge of PR #35 |
| Latest tag | **`v0.17.1`**. 0.18.0 is not tagged yet. |
| Merged this session | **#33** (PR-based release), **#35** (engine-requirement bump). **#34** was a release PR cut before #35 and was closed, not merged. |
| Open | **#36 `Release v0.18.0`** — awaiting review. Plus two stale dependabot PRs, #25 and #32. |
| Push gotcha | The ssh-agent holds **no identities**, so `git push origin …` fails on publickey. `gh` is authenticated over HTTPS — `git push https://github.com/oetiker/byonk.git <branch>` works and leaves the remote config alone. Used all session. |
| `main` protection | Ruleset `main-protect`: PR required (0 approvals), 5 required checks (`Build`, `Test`, `Check & Lint`, `Analyze (actions)`, `Analyze (rust)`), strict up-to-date. Bypass: **Repository admin** — you can always merge, no PAT can. |

---

# What to do next

**Finish the 0.18.0 release: review and merge [PR #36](https://github.com/oetiker/byonk/pull/36),
`Release v0.18.0`.** It was opened by `Create release PR` at 16:07 on 2026-08-18 and holds the
whole bump — Cargo, changelog, both HA manifests, the 13 screens and the 2 doc examples.

**Merging it is the first end-to-end run `Release publisher` has ever had.** Watch it: tag →
5 binaries → container → GitHub release → docs deploy. If it dies partway, **re-run it from the
Actions UI** — it is built to be re-runnable, because its guard asks whether the *release*
exists rather than whether the tag does.

Then, and only after a release completes successfully:

1. **Delete the `RELEASE_TOKEN` secret and revoke the PAT.** Nothing references it any more.
   This is the whole point of the change.
2. Pick from *Queued work*.

---

# The release process, as it now works

Two workflows. **Nothing pushes to `main`, so no PAT is involved anywhere.** `GITHUB_TOKEN`
cannot push to protected `main`, but it can push an ordinary branch, open a PR, and push a
**tag** — the ruleset targets branches, and tags are a separate ref namespace.

| Workflow | Trigger | Does |
|---|---|---|
| `create-release-pr.yml` | `workflow_dispatch` + bugfix/feature/major | waits for CI to be green on the exact commit, computes the version from tags, bumps everything, opens a `release/vX.Y.Z` PR. **Nothing is tagged or published.** |
| `release-publisher.yml` | `push` to `main` touching `Cargo.toml` | tags, builds 5 binaries, builds and pushes the container, publishes the GitHub release, deploys the docs. **No `workflow_dispatch` by design.** |

**To cut a release:** Actions → *Create release PR* → pick the type → review the PR it opens →
merge it. That is the whole procedure.

## Both new guards were exercised for real on 2026-08-18, and both worked

The release was fired **21 seconds after** PR #35's merge, while that merge's CI was still
running, and the stale `release/v0.18.0` branch from the closed #34 was still on origin. So the
first live run happened to test both of the things that had never been tested:

| Guard | Evidence |
|---|---|
| **Wait for CI on this exact commit** | CI on `df1c31f` ran 16:02:05 → 16:06:58. The verify step ran 16:02:32 → **16:07:08** — it polled for 4m36s and finished 10 s after CI concluded. |
| **Clear a stale release branch** | `release/v0.18.0` moved `941f51f` → `ab2a014`. The old tip was not an ancestor of the new one, so the push could only succeed after the delete. |

Neither had been provable before a real release. Do not assume they are now permanently proven —
they have each run **once**.

## What the release PR bumps

All in one commit, and all of them must move together:

| File | By |
|---|---|
| `Cargo.toml` + **`Cargo.lock`** | `cargo update --workspace`, then verified |
| `CHANGES.md` | rolls `Unreleased` into `## X.Y.Z - date` |
| `custom_components/byonk/manifest.json` | `tools/release/bump-integration-version.sh` |
| `homeassistant/byonk/config.yaml` + its `CHANGELOG.md` | `tools/release/bump-addon-version.sh` |
| `screens/**/meta.yaml` **and** `docs/src/**/*.md` | `tools/release/bump-screen-engine.sh` |

All three bump scripts have test scripts run by CI's **`Release Scripts`** job.

## Things about it you need to know

- **The add-on version *is* the ghcr image tag.** `homeassistant/byonk/config.yaml` has
  `image: ghcr.io/oetiker/byonk` and `version:`, so Supervisor pulls `<image>:<version>`.
  It is bumped **in the release PR**, which means that for the ~15–20 minutes between merging
  and the publisher pushing the image, the add-on store advertises a version that does not
  exist yet. A user refreshing in that window gets a pull failure and succeeds on a retry.
  **Owner decision, deliberate**, in exchange for one PR per release. The alternative
  considered was a second auto-merged PR after the container push.
- **The publisher's guard asks whether the GitHub *release* exists, not the tag.** Guarding on
  the tag would make a run that died after tagging unrecoverable, because the tag is created
  first. As built, a failed publisher run is **re-runnable from the Actions UI**.
- **A non-404 failure when asking about the release stops the run.** Treating every error as
  "not released" would re-publish a shipped version. Verified against a live 401 and a dead host.
- **A cancelled release leaves its branch behind**, and the next attempt builds the same branch
  name from a different base — a non-fast-forward rejection. `create-release-pr.yml` clears a
  stale `release/v*` branch, **unless it still has an open PR**, which gets a pointed error.
- **The publisher fires on any push touching `Cargo.toml`** — a dependabot PR will start it. It
  finds the release already published and exits in seconds. That is intended.
- **The version is read back out of `Cargo.toml`** and cross-checked against both HA manifests.
  A drift fails loudly rather than pointing the add-on at an image that was never built.

---

# Session 27 — what landed and why

## The old release died on an expired PAT (#33)

`RELEASE_TOKEN` was a PAT on the ruleset's bypass list, created 2026-07-17 and dead 32 days
later. The 0.18.0 run failed on its first step with `could not read Username for
'https://github.com': terminal prompts disabled` — a 401 making git fall back to prompting.
Diagnosis was clean because every *other* checkout on the same commit at the same minute used
`GITHUB_TOKEN` and worked.

**A PAT with a default expiry is a scheduled outage.** The fix was to stop needing one.

## The new flow caught a bug on its first use (#35)

The 0.18.0 release PR failed CI on `every_shipped_screen_is_compatible_with_this_engine`.

A screen declares `byonk: "0.17"`, which `src/models/compat.rs` reads as `^0.17` — that is
`>=0.17.0, <0.18.0`. Bumping the engine to 0.18.0 put **all 13 shipped screens** outside their
own declared range, so `GET /api/admin/screens` would have warned on every one of them,
including `byonk-builtin/default`, the fallback screen.

**This was not new.** The guard test's own comment records the same drift shipping in 0.16.0,
and `compat::engine_compat_req()` exists so newly *scaffolded* screens can never be born stale.
Nothing did the same for the screens byonk ships, so every minor release had to break this test.

> **The old flow could not have caught it.** It pushed the bump and the tag together, so 0.18.0
> would have been tagged, built and pushed to ghcr before CI went red on `main`. The PR gate
> stopped it with nothing published. This is the single best argument for the new shape.

`bump-screen-engine.sh` covers **two** places because they must move together:
`docs/src/api/admin-api.md` reproduces `examples/swiss-departure-board/meta.yaml` verbatim, so
bumping only the screens would make the docs contradict them. 15 files on a feature release; a
bugfix release changes nothing, which is correct — 0.17.2 is inside `^0.17`.

## Two latent bugs fixed on the way

- **`Cargo.lock` never moved with `Cargo.toml`.** It records byonk's own version, so every
  release commit byonk has ever cut was internally inconsistent — dirty the moment anyone built
  the tag.
- **Release-note extraction deleted the changelog's last line** when the released section was
  the final one in the file, because it stripped a trailing heading that was not there.

## Copilot review — six findings, all real

Both PRs were reviewed. Worth reading the threads on #33 and #35; the substantive ones:

- A cancelled release's branch blocks every retry (reproduced as a real non-fast-forward).
- The release-exists guard treated any error as "not released".
- **`series="${version%.*}"` was silently wrong**: `"0.18"` became series `"0"`, and neither
  guard caught it, because `"0"` is neither empty nor equal to the input. Now matched against
  `^([0-9]+)\.([0-9]+)\.([0-9]+)$` with the series read from capture groups.
- **A comment claimed the Markdown rewrite was confined to fenced YAML blocks. It is not** — it
  is a plain column-0 match. Worse, the test that appeared to back the claim passed for a
  different reason (its prose mention sits mid-line). Comment corrected; a test now pins the
  real behaviour.

One suggestion was **declined with reasoning**: moving the test digest to `sha256sum`. macOS
ships no such command, and this suite runs on a maintainer's laptop as well as CI, where
`shasum` is already proven by the passing `Release Scripts` job.

## Every action was upgraded

checkout v4→**v7**, cache v4→**v6**, upload-artifact v4→**v7**, download-artifact v4→**v8**,
gh-release v2→**v3**, configure-pages v4→**v6**, deploy-pages v4→**v5**, upload-pages-artifact
v3→**v5**, docker setup-qemu/setup-buildx v3→**v4**, docker login v3→**v4**, docker build-push
v5→**v7**. In `ci.yml` and `docs.yml` too. The failed run had already begun warning that
`checkout@v4` was being forced onto Node 24.

`ilammy/setup-nasm@v1`, `hassfest@master` and `hacs/action@main` stay on their moving tags —
that is those projects' own convention.

---

# Settled — do not reopen

## resvg `byonk-base` — the initiative is complete

All 10 plan tasks done in sessions 25–26; merged as PR #30. **resvg work happens in a different
repo.** `oetiker/resvg` carries `feat/bitmap-mask-glyphs` (upstream PR #1115),
`feat/font-hinting` (upstream PR #1116), and `byonk-base`, which merges them and is what byonk's
`[patch.crates-io]` pins. **Current pin: `2e766508`** (in `Cargo.lock`; `Cargo.toml` tracks the
branch).

The plan file `docs/superpowers/plans/2026-08-15-resvg-byonk-base-integration.md` is **spent**.
**It was wrong in eleven of eleven tasks touched**; if you reread it, verify every symbol.
Task 11 in that file (the hinted-font-trio decision) was never part of the 10 — see *Queued work*.

## Byonk warns when a screen renders at the wrong scale

`SvgRenderer::scale_warning(svg_w, svg_h, spec) -> Option<String>`, next to `fit_transform` and
fed the same numbers so the two cannot disagree. **Owner chose: warn on any size mismatch, no
integer-zoom exemption.**

`rasterize_svg` fills a **`&mut Option<String>` out-parameter**. Each caller decides:
`ScreenStore::render` → `RenderResult::log` as `[warn] …`; `main.rs` (`byonk render`) →
**stderr**; `api/display.rs`, `api/dev.rs`, `render_to_raw_png` and `content_pipeline`'s
internal call → `&mut None`.

**Authoring warnings reach the author, not the operator.** `tracing::warn!` was rejected: it
reaches the server log, not the screen's author.

## Why integer zoom is not a free pass — read from the pinned resvg, not reasoned

Source: `~/.cargo/git/checkouts/resvg-b4a0ccb9ea26de88/2e76650/`

- `crates/usvg/src/text/flatten.rs:283` — the hinting ppem is the **user-unit** font size; the
  render transform is not involved.
- `crates/usvg/src/text/flatten.rs:68` — `snap_bitmap_glyph` returns early unless the scale is
  1.0 within `1e-4`. **Bitmap faces lose strike snapping at any zoom**, integer included.
- And the plain one: every dimension the author chose is displayed at the wrong size.

## A variant CAN be aliased. The flag is document-level; the effect is not.

`HintingSpec::to_usvg()` deliberately drops `aliased`, because aliasing reaches usvg through
`Options::text_rendering`, which is document-level. **But `text-rendering` is an ordinary
inheritable SVG property, so the element using a variant asks for it directly.** Pinned by
`a_mono_variant_plus_optimize_speed_equals_the_document_level_aliased_mono`.

**This is the point of variants**: part of a screen can be made genuinely 1-bit crisp, on a grey
panel as well as a black-and-white one. Always pair `optimizeSpeed` with mono hinting.

## No bundled font carries a hinting program

Measured with fontTools across every glyph: Outfit, the Source trio and all X11 faces have
**zero** hinted glyphs; Terminus has one. Consequences, all confirmed by render: **`interpreter`
is effectively unhinted**, **`auto` ≡ `auto_fallback`**, and `interpreter` is *visibly worse*
when aliased. The engine axis is **not** dead — `auto ≡ auto_fallback` is a live check that
byonk's auto-fallback fix still works.

## Smooth hinting is real but cannot look dramatic

Document `smooth` vs document `off` differ on 35–72% of the ink, but almost entirely as
**grey-level shifts on anti-aliased edges**. Only aliasing makes hinting visually obvious.

---

# Carried forward — still binding

## Owner decisions

1. **Bundle the Source trio** as generic-family fallbacks: `sans-serif` → Source Sans 3,
   `serif` → Source Serif 4, `monospace` → Source Code Pro. **Outfit stays** as the house sans.
2. **No fallback magic.** Designers choose bitmap faces explicitly.
3. **Fonts need licence files** (see F14).
4. **Bitmap fonts should have no outlines if possible** — delivered by F16.
5. **F20: status icons own the header corner; the timestamp lives in the footer.**
6. **byonk intervenes only when Lua crashes.**
7. **Authoring warnings reach the author, not the operator.**
8. **The add-on version ships in the release PR**, accepting the image gap (session 27).

## The Lua surface, as shipped

```lua
font_hinting = false            -- hinting off entirely
font_hinting = {
  engine = "auto",              -- interpreter | auto | auto_fallback
  target = "mono",              -- shorthand for { mode = "mono" }
  variants = {
    ["Crisp Body"] = { font = "Outfit", hinting = { target = "mono" } },
  },
}
```

`mode` is the discriminator: mono's extra knob is `aliased`, smooth's are `symmetric` and
`preserve_linear_metrics` (the real field is `symmetric_rendering`). A variant also takes
`strikes = true|false`, which `select_bitmap` reads — **that knob is the basis of the bitmap
regression test, so do not remove it.** A directive naming **only** variants keeps the panel's
adaptive default.

**F1 constraint:** aliasing is per-element and inheritable; hinting is per-face. An element
choosing smooth/no hinting on a BW panel inherits `optimizeSpeed` and lands in the known-bad
aliased-without-mono state. Escape hatch: **`text-rendering: optimizeLegibility`** — restores AA
*and keeps hinting*. **Trap: `geometricPrecision` restores AA but disables hinting.**

## Naming rule for variant aliases

**Name them for their purpose, never `<RealFamily> <TechnicalTerm>`.** Use
`["Crisp Body"] = { font = "Outfit", … }`. **Always name the fallback in the document:**
`font-family="'Crisp Body', Outfit"`.

## F15 / F16 — the bitmap work, done and live

- **The fonts and the resvg pin must move together.** Byonk has a test that fails otherwise
  (`test_bitmap_font_render`, session 26).
- **Terminus is NOT buggy. Terminus @14 and @18 render 1 px/glyph wider — that is correct.**
- **Merge trap:** `byonk-base` has host hooks upstream does not
  (`FontResolver::select_bitmap`, `select_font`). A clean *textual* merge is **not** evidence the
  semantics survived.
- **A bitmap face only renders as a bitmap at a size it has a strike for**, and nothing warns you.
  `fonts/FONTS.md` lists the sizes per family.

## Falsified — do not chase again

- **X11 vertical-metric overflow**: real malformation, **not** a cause of anything.
- **Ink overhang in the oblique faces**: slanted bitmap faces overhang normally.
- **F10's two hazards, both FALSE**: the fvar `wght` default does not leak, and Source Serif 4
  is not pinned at `opsz` 20.
- **F9 / `AutoFallback`:** upstream will not change it (googlefonts/fontations#1151, closed).
  Keep both byonk's explicit `Auto` and `resolve_auto_fallback`. Do not PR it.
- **`font-weight` does not disable hinting.**

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

- **`X11Misc*` is a cell-width grouping, not a licence grouping.** Notices must be per source file.
- **Do not rename `X11LuSans`/`X11LuType` toward "Lucida"** — the trademark licence covers
  unmodified fonts only, and byonk modified them.

---

# Queued work

| ID | What |
|---|---|
| — | **Finish 0.18.0** (see the top), then **delete `RELEASE_TOKEN`**. |
| — | Two stale dependabot PRs: **#25** (cargo, 7 updates) and **#32** (pytest in /fonts). |
| F13 | Extend `screens/examples/demo/font/{ttf,bitmap,hinting}/` to cover Source. |
| F14 | Licence + notice files per the table above. **`FONTS.md`'s "X11LuType is proportional" is wrong — it is monospaced.** |
| F22 | Cosmetic: the WiFi glyph reads as a caret at 8×12. Redraw or drop it. |
| F23 | The two fetching examples fail in a sandbox with `Cannot drop a runtime…` *from the fetch error path*; check whether any other blocking call in `lua_runtime.rs` shares the hazard. |
| F24 | `/dev/render` shows the author nothing but an image — it passes `None` for the script log sink, so neither their `log_*` output nor byonk's authoring warnings reach the browser preview. |
| — | `CHANGES.md` has a **duplicate `## 0.1.0` heading** (lines 849 and 866). Harmless but it makes release-note extraction for 0.1.0 stop at the first one. |
| — | Two overstated test names in `dither/mod.rs`. |
| Plan Task 11 | The hinted-font-trio **decision** (specimens + recommendation). See the plan file from line 1430. |

---

# Open items the owner should decide

## Marking costs shadow detail at 16 grey levels

On `calibration/tone` the marked (measured, mapped) half is supposed to show what gamut mapping
buys. On the 4-grey panel the two halves are close. **On `trmnl_x` the marked half is markedly
darker and loses shadow separation** the unmarked half keeps. `trmnl_x`'s measured palette runs
`#383838`–`#B8B8B0`, so mapping into it *should* darken — **but the unmarked half dithers against
that same measured palette**, so the gap comes from the mapping, not the inks. **Not diagnosed.**
Decide which half is the better preview before touching the mapper.

## Two TLS tests are flaky

`lua_https_tests::{test_https_with_custom_ca_cert, test_https_with_client_certificate}`, failing
with `error sending request for url (https://127.0.0.1:…)` — the shape a 30 s timeout takes.
Seen once in six full runs.

**The best explanation is the laptop suspending, not CPU contention.** A 30 s timeout is *wall
clock*, so a suspend mid-test blows it no matter how idle the CPU is. **Before spending anything
on a fix, check whether the failing runs coincide with a sleep** (`pmset -g log | grep -i sleep`).
**The null hypothesis has still never been tested** — nobody has run the suite with `c850ea7`'s
HTTP change reverted.

**If it needs fixing, do not loosen the test.** Cache the `reqwest::blocking::Client` instead of
building one per request. A single shared worker thread is the wrong answer: it would serialise
every screen's HTTP on the server path.

## Smaller ones

1. **`grey_count <= 2` may be the wrong rule.** On the 4-grey panel at 10–12 px mono+aliased
   beats smooth, but at 14 px smooth wins — the fix may be a **size term**. *Always name panels
   by config key:* the **4-colour** `trmnl_og_4clr` already counts as `grey_count = 2`; it is
   **4-grey** `trmnl_og` that is in question. `FontConfig::adaptive_default` is the single place.
2. **`HintingMode::Light` is byte-identical to `Normal`.**
3. `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and its
   `kchroma = 10` tuning are not on the crate's dithering path at all.

**Owner-facing artifacts** (to update, republish **passing the existing URL**):

| What | URL |
|---|---|
| Render sweep — 19 captures across 5 panels, the two bugs it found (session 26) | https://claude.ai/code/artifact/7e3a6c8d-763d-4985-8f12-69c7d7fdcc99 |
| Task 8 + the three demo bugs + the fetch fix (session 24) | https://claude.ai/code/artifact/dede3454-3192-47d6-8e45-97a71440a08f |
| X11 Bitmap Specimens — all 26 rebuilt faces (session 22) | https://claude.ai/code/artifact/ef06c1db-b5ba-467c-8cc3-3a7069e00488 |
| Bitmap vs outline; F15/F16/F17 (session 20) | https://claude.ai/code/artifact/8fe47446-49b6-4256-9db6-429aa3b8bfb6 |
| Type trials: specimens, two bugs, the data (session 19) | https://claude.ai/code/artifact/f7ef39be-1a9d-4c97-bd95-d9b3422a515e |

---

# Lessons — these keep paying off

## From session 27

- **A credential with a default expiry is a scheduled outage.** A 30-day PAT created in July
  took the release down in August, on a day nobody was thinking about tokens. Prefer a design
  that needs no long-lived secret over one that needs a calendar reminder.
- **Gate the release behind the same review the code gets.** A version bump is a change like any
  other, and CI should judge it *before* anything is tagged or pushed to a registry. The old flow
  tagged first and learned second; the new one caught a real defect with nothing published.
- **A parameter that is only ever given valid input is untested, not correct.** `${version%.*}`
  turned `"0.18"` into the series `"0"` and no guard noticed, because `"0"` is neither empty nor
  equal to the input. It was unreachable from the workflow — and still worth fixing, because the
  script has a usage line and the failure is silent.
- **A comment that overclaims is worse than no comment when a test seems to back it.** The claim
  "confined to fenced YAML blocks" was false, and the test that looked like evidence passed for
  a different reason entirely. When a test agrees with a comment, check *why* it passes.
- **Check the tool's dialect before trusting a one-liner you cannot run.** `sed '${/^## [0-9]/d}'`
  is GNU-only and dies on a Mac. Anything you cannot execute locally is unverified; rewriting it
  in perl made it testable on both.
- **Reproduce the failure locally before writing the fix, then show the fix flips it.** RED at
  engine 0.18.0 with the screens untouched, GREEN after the bump script. That pair is worth more
  than any amount of reading.
- **Read the review's suppressed comments.** Copilot's most serious finding on each PR was in the
  collapsed section, not the inline threads.

## Carried forward

- **"No warnings" is not coverage until the mechanism has been shown to fire.** Build a positive
  control that must trip it.
- **Coverage that is wide in one dimension can be nil in another.** Ask what a sweep holds
  *constant*, not just what it covers.
- **A tool that hides a channel makes every future run lie.** Whenever a new output channel is
  added, check what the existing harnesses do with it.
- **Ask what a check would prove *today*, not what it proved when it was written.**
- **Demonstrate the check fails when the thing is broken.** A test written *after* the
  implementation has never been shown to fail, so sabotage stands in for the RED step. Back the
  file up first and diff after restoring.
- **A rule can be right about the mechanism and still wrong about the decision.**
- **A default nothing asks for is a default that goes missing.** Resolve such defaults at the
  single choke point.
- **Always carry a control through a measurement.** Error diffusion depends on position, so only
  grey-free (aliased) content is exactly comparable.
- **A CSS rule beats a presentation attribute in SVG.** `text { font-family: … }` silently
  overrides every `font-family="…"` attribute, and the text still renders — in the wrong face.
- **Put text on whole-pixel positions, not just whole-pixel sizes.**
- **Fix the docs when they are the bug.** `docs/src/tutorial/svg-templates.md` has been the bug
  **twice**. It is embedded and served to LLM authors over MCP.
- **Read a changelog section as a set, not as a stream of appends.**
- **Assert on the geometry, not the pixels, when the question is "is this legible".** See
  `tests/calibration_grey_layout_test.rs`.
- **A raw string ends at the first `"#`, and hex colours are full of them.** Use `r##"…"##`.
- **A sleeping laptop looks exactly like a hung build.** `ps -eo pid,etime,command` reports *wall*
  time, which keeps counting through sleep. Check `uptime` too.
- **`cargo check --lib --tests` is the fast way to see a signature-change RED.**
- **Verify a background job is actually running before reporting on it.**
- **A screen that renders is not a screen that rendered what you asked for.** Carry a canary
  string *in the render itself*.
- **`test -s` both files before believing a `cmp`.**
- **A flattering test string hides font defects.** `illiIL1 xXHv`, not `Render jpq 0123`.
- **When the data is right and the render is still wrong, suspect the consumer's guards.**
- **Work left by an agent that died is not verified work.**
- **Never run `make check` while the tree is being edited.** Also `make check > log; echo
  "EXIT=$?"` reports the *echo's* status. Same trap with any pipe: `cmd | tail; echo $?` reports
  `tail`. **This bites in background jobs too.**
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
- **In a debug build rust-embed reads from disk at runtime**, so screen / `byonk-base` edits take
  effect with **no rebuild** — but "no change" is then indistinguishable from a stale binary, so
  **prove disk-backing with a visible sabotage first**.
- **Subagents must not run `make check`** — the 600 s watchdog kills them.
- `CARGO_BUILD_JOBS=2` — shared machine. `cargo test` takes only **one** filter, and a filter
  matches the *whole path*.
- Pre-existing `#[ignore]` failures, unrelated: `preprocess::preprocessor::tests::{…}`.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`.
- **IDE diagnostics lie in this tree.** Only an actual cargo run counts.
- `make docs` = `mdbook build`; mdbook is installed. `docs/book/` is gitignored.
- **`docs/src/images/` is gitignored** — `hintdemo.png` is refreshed locally, never committed.

## Linting the workflows

Neither tool is installed; both download as a single binary into the scratchpad:

```bash
curl -sSL https://github.com/rhysd/actionlint/releases/download/v1.7.7/actionlint_1.7.7_darwin_arm64.tar.gz | tar -xzf - actionlint
curl -sSL https://github.com/koalaman/shellcheck/releases/download/v0.10.0/shellcheck-v0.10.0.darwin.aarch64.tar.xz | tar -xJf - --strip-components=1 shellcheck-v0.10.0/shellcheck
PATH="$PWD:$PATH" ./actionlint .github/workflows/*.yml
```

**`actionlint` only lints the shell inside `run:` blocks when `shellcheck` is on `PATH`** — a
clean run without it is much weaker than it looks. `docs.yml` has pre-existing SC2086/SC2012
style findings; the release workflows are clean.

## Capturing every bundled screen, on every panel

```bash
BYONK_BIN=./target/debug/byonk ./tools/capture-renders.sh /path/to/out
```

Seconds, not minutes. **19 captures across 5 panels** — 4-grey, 4-colour, 16-grey and both
6-colour. Writes `MANIFEST.txt` with the canary verdict, per-screen exit codes, any stderr byonk
produced, and the distinctness verdict. **Do not put the output in `/tmp`** — that is how the
previous baselines were lost.

**A capture is only as wide as `tools/capture-config.yaml`'s device map.** When adding a panel
profile, add a device for it here too.

## Rendering a scratch screen

1. Make a directory with a `byonk-screens.yaml` manifest (`name`, `description`, `author`,
   `license`). **Without the manifest the repo is skipped and every render silently falls back.**
2. Each screen needs `meta.yaml` (`title`, `description`, `byonk`, `refresh`), `script.lua` and
   `screen.svg`. A bare `name:` in `meta.yaml` is **not** enough.
3. **`script.lua` must return `{ data = { … }, refresh_rate = N }`**. **The template reads them
   under `data.`**: `{{ data.foo }}`, not `{{ foo }}`.
4. Register it in a config copy. **`EXAMPLES_DIR` registers under the fixed handle `examples`,
   NOT the manifest's `name:`**, so use the config instead:
   ```yaml
   screen_repos:
     probe: { path: /abs/path/to/dir }
   devices:
     "AA:BB:CC:00:00:71": { panel: trmnl_og, screen: probe/myscreen }
   ```
   Seed the copy from `tools/capture-config.yaml`.
5. `CONFIG_FILE=<cfg> ./target/debug/byonk render --mac AA:BB:CC:00:00:71 --output x.png`

Notes:

- **Build the SVG from `layout.width`/`layout.height`.** Byonk warns on stderr if you do not.
- **Put text at integer x/y in any probe that judges hinting.**
- **Renders are dithered, and error diffusion depends on position.**
- `--colors "#000000,#FFFFFF"` forces a 2-colour panel. `--use-actual false` gives spec colours;
  the default gives measured colours.
- **Swapping fonts without rebuilding:** `FONTS_DIR=<dir>` overrides embedded fonts **by filename**.
- PIL is available; `Image.NEAREST` at 3–6× is what makes pixel-level differences legible.

## Fonts

- `make fonts-setup` (once) → `.venv-fonts`; `make fonts-check` (18 tests, instant); `make fonts`
  (rebuild all 26, deterministic). Downloads cache in `fonts/.x11-cache/`.
- **`.venv-fonts/bin/python` has fontTools** — use it to interrogate the bundled faces directly
  instead of inferring from renders.
- **Working on resvg:** clone `oetiker/resvg` into the scratchpad. Its suite is fast (~11 s, 1750
  tests). To test byonk against a local resvg, point `[patch.crates-io]` at
  `<clone>/crates/{resvg,usvg}` — **back up `Cargo.toml` and `Cargo.lock` first and restore after.**
- The patched resvg source is readable at
  `~/.cargo/git/checkouts/resvg-b4a0ccb9ea26de88/<rev>/` — faster than cloning.

---

# Carried forward

Detail from earlier sessions, in git rather than here:

| Session | What | Where |
|---|---|---|
| pinning initiative | `eink-dither`, gamut mapping, colour models — **read before touching any of them** | `git show 3b32762:docs/HANDOVER.md` |
| 23 | F20, F21, Task 7 archaeology | `git show 6e6e214:docs/HANDOVER.md` |
| 24 | Task 8, the CLI fetch fix, `http_response()`, the hinting demo | `git show 4cefe83:docs/HANDOVER.md` |
| 25 | Task 4 in full, the docs bug it uncovered | `git show 5a531a1:docs/HANDOVER.md` |
| 26 | the render sweep, `test_bitmap_font_render`, the two panel bugs | `git show c1c8194:docs/HANDOVER.md` |

`git worktree list` is clean.
