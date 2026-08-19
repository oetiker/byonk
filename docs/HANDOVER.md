# Handover — Byonk

_Last updated: 2026-08-19 (session 30). The screen preview (#38) and the Lua sandbox fix (#39)
are both **merged**. This session designed a new initiative — **the Byonk app installs its own
Home Assistant integration, and HACS is dropped** — and stopped at a committed spec and plan.
**No implementation has started. Nothing is pushed.**_

## Where the work lives

| | |
|---|---|
| Branch | `feat/addon-installs-integration`, cut from `main` @ `958b14f` |
| HEAD | `799f83c` — **2 commits, docs only, tree clean, NOT pushed** |
| `main` | `958b14f` (merge of #39, the Lua sandbox fix). Latest tag **`v0.18.0`** |
| Open PR | **#40** `ci: check the whole workspace, not just the root package`, head `011a7b5`, MERGEABLE. It touches **only `.github/workflows/ci.yml`** — the same file this branch's Task 7 edits. Different regions, so it should merge, but **merge #40 first** and rebase rather than find out during a task |
| Stray worktree | `/private/tmp/.../scratchpad/byonk-ci-workspace-checks` @ `c90440f` on `ci/workspace-wide-checks`. Origin has `011a7b5` on that branch, so the worktree is **behind**. It is in a scratchpad and can be removed |
| Push gotcha | ssh-agent holds **no identities**, so `git push origin …` fails on publickey. `gh` is authenticated over HTTPS — `git push https://github.com/oetiker/byonk.git <branch>` works and leaves the remote config alone |
| `main` protection | Ruleset `main-protect`: PR required (0 approvals), 5 required checks (`Build`, `Test`, `Check & Lint`, `Analyze (actions)`, `Analyze (rust)`), strict up-to-date. Bypass: **Repository admin** — the owner can always merge, no PAT can |

The two commits:

| Commit | What |
|---|---|
| `646e027` | The design spec |
| `799f83c` | The nine-task implementation plan |

---

# The active initiative

**Goal: a Home Assistant user installs one thing — the Byonk app — and Byonk works.**

The integration already installs the app (`config_flow.py:94` → `addon.py:39`). The missing half
is the reverse, and that is what this builds: at startup in add-on mode, byonk copies
`custom_components/byonk` out of its own image into the Home Assistant config directory, posts a
"restart Home Assistant" notification, and posts a Supervisor discovery message so a **Byonk**
Discovered card is waiting after the restart.

**Read these two, in order:**

1. `docs/superpowers/specs/2026-08-19-addon-installs-integration-design.md`
2. `docs/superpowers/plans/2026-08-19-addon-installs-integration.md`

## The five decisions behind it

1. **The app carries the integration.** Byonk needs the app anyway, so the "Get HACS" pattern — a
   throwaway installer app — would be pure overhead.
2. **HACS is dropped entirely.** `hacs.json`, the `hacs/action` CI job, `test_hacs_json_parses`
   and all HACS docs go. **hacs/default PR #9310 has been closed by the owner.** Do not re-file it.
3. **The restart nudge is a persistent notification**, not a log line and not an app that restarts
   Home Assistant by itself.
4. **Version skew warns, it does not block.** Entities keep working; a repair issue points at the
   restart.
5. **The docs merge into one task-ordered page.** `ha-addon.md` + `ha-integration.md` →
   `home-assistant.md`, one nav entry. The owner's steer: *"most HA users will not care about the
   tech details — install byonk in HA and it works."* The words *app* and *integration* appear
   only where a user must click on one.

## What to do next

**1. Pick an execution mode and run the plan.** That is exactly where this session stopped — the
choice was offered and the answer was "write a handover first". Subagent-driven development is
recommended (fresh subagent per task, review between tasks); `superpowers:executing-plans` is the
inline alternative.

**2. Merge PR #40 before Task 7**, or expect to rebase `.github/workflows/ci.yml`.

**3. Task 9 is not optional.** Nothing before it proves the chain: that Supervisor accepts the new
manifest, that the mount lands at `/homeassistant`, that the card appears. It needs the VM.

---

# Facts already established — do not re-derive these

Every one was read from current upstream source this session, not assumed. Re-deriving them costs
a dozen fetches.

| Fact | Source |
|---|---|
| `addon_config:rw` mounts at `/config`; `homeassistant_config:rw` mounts at `/homeassistant`. No clash with byonk's existing `/config` use | `supervisor/docker/const.py:193-194`, `docker/app.py:456-491` |
| The app **security rating ignores `map` and `homeassistant_api`**. It scores AppArmor, ingress, privileges, `hassio_role`, host namespaces | `supervisor/apps/utils.py:19-86` |
| Discovery service names are free-form strings; the only rule is that the app lists the service in its `discovery:` key | `supervisor/api/discovery.py` |
| `/discovery.*` needs **no API role** — it is in the security middleware's `api_bypass` set | `supervisor/api/middleware/security.py:105` |
| Discovery messages **dedupe on (app, service)** — `config` and `uuid` are `compare=False`. Posting every start is idempotent | `supervisor/discovery/__init__.py:31-99` |
| If Home Assistant is down, the message is **stored and the push skipped**; HA re-reads the list at `EVENT_HOMEASSISTANT_START`. First install produces no error | `supervisor/discovery/__init__.py:117-121`, `core/components/hassio/discovery.py:36-52` |
| HA turns a discovery message into a config flow **for the domain named by `service`**, source `hassio`. Custom integrations resolve normally | `core/components/hassio/discovery.py:113-140` |
| `/core/api/*` is gated on `homeassistant_api: true`; only `hassio*` paths are denied to apps, so `persistent_notification/create` is reachable | `supervisor/api/proxy.py:38,99-116,170-175` |
| `hacs.json` is **optional** — HACS recognises a repo from `custom_components/<domain>/manifest.json`. Deleting it breaks no existing install | HACS publish docs |
| The release workflow already fails unless `Cargo.toml`, `manifest.json` and the app `config.yaml` agree on the version | `.github/workflows/release-publisher.yml:64-66` |

Two traps the plan already works around, found in self-review:

- **`reqwest`'s `json` feature is not enabled** in `Cargo.toml`. `RequestBuilder::json` will not
  compile until Task 3 turns it on.
- **Adding a Supervisor call to `_async_update_data` breaks every existing HA test**, because they
  all drive the coordinator through the `byonk` fixture and there is no Supervisor in the harness.
  Task 6 extends the fixture *first*, before the feature exists.

---

# The test VM

See the `ha-vm-testing` skill. Its state as of session 29:

| | |
|---|---|
| byonk | **`local_byonk` only**, built from source, running |
| Published add-on | **uninstalled** (owner approved) — it was holding port 3000 |
| Config entries | hub + `Byonk Default`, healthy |
| HA core | **2026.7.2** (2026.8.x is out) |

- **`make ha-rebuild` does not sync the app manifest.** This branch changes
  `homeassistant/byonk/config.yaml`, so the VM needs a manual version bump plus `POST
  /store/reload` and `ha addons update`, or the new `map`, `homeassistant_api` and `discovery`
  keys will not be live. **Task 9 fails silently and confusingly without this.**
- **Two byonk apps cannot coexist** — both want port 3000; the loser sits in state `error`.
- **`ha core restart` returns long before HA is up.** Poll `http://localhost:8123/` for `200`.
- **Chrome on the Mac host holds a live HA session**, so the UI can be driven with the
  `claude-in-chrome` tools with no password. HA's frontend is shadow-DOM heavy: `find` and
  `read_page` return almost nothing — **work from screenshots and coordinates**. Dialogs animate;
  screenshot again before clicking. The device page jumps to the top when a dialog closes.
- **Never print the admin token.** Verify through the UI, or curl from the Mac host on port 3000
  (not from the Terminal add-on). A token-free liveness check: the endpoint answers **401** when
  the route exists and **404** when the binary is too old.

## A real bug Task 9 will probably hit

`custom_components/byonk/addon.py:29` — `_async_find_addon_item` returns the **first** store item
whose slug ends in `_byonk`, and the config entry stores that slug forever. With two byonk apps
installed (published `<hash>_byonk` and from-source `local_byonk`), **which one the integration
adopts is down to Supervisor's listing order.** Reauth re-reads the *stored* slug and never
re-discovers (`config_flow.py:128`), so a wrongly-bound entry **cannot heal itself**: `401`,
restart the wrong app, "port 3000 is already in use". The cure is deleting and re-adding the
entry, which orphans every device entry (they store the hub's `entry_id`, `__init__.py:62`).

Left unfixed deliberately — it needs a decision about what "the byonk app" means when there are
two. Task 9 runs with `local_byonk` only, which sidesteps it.

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~15–40 min here — background it**; it runs
  `cargo fmt`, not `--check`, so it rewrites files.
- `make ha-check` = `ruff` + `pytest tests_ha`. Needs `make ha-setup` once. **Instant** (~2 s).
- **`cargo test` links each test binary serially and slowly here** (~1–2 min per binary, 40
  binaries). `cargo test --test <name>` is the fast loop.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`. Add by
  explicit path; check `git diff --cached` before committing.
- **Never run `make check` while the tree is being edited.** And `make check > log; echo "EXIT=$?"`
  reports the *echo's* status — same trap with any pipe. This bites in background jobs too.
- **A sleeping laptop looks exactly like a hung build.** `ps -eo pid,etime,command` reports *wall*
  time. An IDE `cargo check --workspace` also holds the build lock.
- **Subagents must not run `make check`** — the 600 s watchdog kills them.
- **IDE diagnostics lie in this tree.** Only an actual cargo run counts.
- **Editing an embedded asset forces a rebuild** (`api/lua-api.md`, `tutorial/svg-templates.md`,
  `guide/authoring.md`, `byonk-base/`, `screens/{builtin,examples}/`).
- **In a debug build rust-embed reads from disk at runtime**, so screen edits take effect with no
  rebuild — but "no change" is then indistinguishable from a stale binary, so **prove disk-backing
  with a visible sabotage first**.
- `make docs` = `mdbook build`. `docs/book/` and `docs/src/images/` are gitignored.
- Capture every bundled screen on every panel:
  `BYONK_BIN=./target/debug/byonk ./tools/capture-renders.sh /path/to/out` — seconds, 19 captures
  across 5 panels. **Not into `/tmp`** (that is how the previous baselines were lost). A capture is
  only as wide as `tools/capture-config.yaml`'s device map.

## Changelog discipline

- **User-facing only.** CI, tooling, version automation and dev process do not belong in
  `CHANGES.md`.
- **Read `Unreleased` as a set, not a stream of appends.** Five sessions of appending once produced
  two entries that contradicted each other and two under the wrong heading.

## The release process

Two workflows; **nothing pushes to `main`**, so no PAT is involved.

| Workflow | Trigger | Does |
|---|---|---|
| `create-release-pr.yml` | `workflow_dispatch` + bugfix/feature/major | waits for CI green on the exact commit, computes the version from tags, bumps everything, opens a `release/vX.Y.Z` PR. **Nothing tagged or published** |
| `release-publisher.yml` | `push` to `main` touching `Cargo.toml` | tags, builds 5 binaries, builds and pushes the container, publishes the release, deploys docs. **No `workflow_dispatch` by design** |

To cut a release: Actions → *Create release PR* → pick the type → review → merge. Things to know:

- **The app version *is* the ghcr image tag.** For ~15–20 minutes between merge and publish, the
  app store advertises a version that does not exist. A user hitting that window gets a pull
  failure and succeeds on retry. **Deliberate owner decision**, in exchange for one PR per release.
- The publisher's guard asks whether the GitHub **release** exists, not the tag, so a failed run is
  **re-runnable from the Actions UI**. A non-404 failure when asking stops the run.
- **The publisher fires on any push touching `Cargo.toml`** — a dependabot PR will start it.
- A cancelled release leaves its branch; `create-release-pr.yml` clears a stale `release/v*` branch
  **unless it still has an open PR**.
- Both guards have been exercised for real, **exactly once each**.

---

# Queued work

| ID | What |
|---|---|
| — | **Execute the plan** — the whole active initiative |
| — | **Delete the `RELEASE_TOKEN` secret and revoke the PAT.** `gh secret list` still shows it (created 2026-07-17). 0.18.0 released without it. Carried since session 27 |
| — | **Decide whether `tests_ha` should run in CI.** It does not today, and after Task 7 removes the HACS job, `hassfest` is the only integration check in CI. The 90 Python tests run only under `make ha-check` |
| — | `addon.py:29` picks the first `*_byonk` app; decide what "the byonk app" means when there are two |
| — | Prove *Preview measured colors* on a panel that **has** a calibration. On `DEFAULT` (grey, uncalibrated) measured and spec palettes are identical, so nothing changes — correct behaviour, zero evidence |
| — | Check the device page on **HA 2026.8.x**; the camera-vs-image comparison rests on 2026.7.2's layout |
| F13 | Extend `screens/examples/demo/font/{ttf,bitmap,hinting}/` to cover Source |
| F14 | Licence + notice files. **`FONTS.md`'s "X11LuType is proportional" is wrong — it is monospaced** |
| F22 | Cosmetic: the WiFi glyph reads as a caret at 8×12. Redraw or drop it |
| F23 | The two fetching examples fail in a sandbox with `Cannot drop a runtime…` *from the fetch error path*; check whether any other blocking call in `lua_runtime.rs` shares the hazard |
| F24 | `/dev/render` shows the author nothing but an image — it passes `None` for the script log sink, so neither their `log_*` output nor byonk's authoring warnings reach the browser preview |
| Plan Task 11 | The hinted-font-trio **decision** (specimens + recommendation) |

---

# Open items the owner should decide

## Marking costs shadow detail at 16 grey levels

On `calibration/tone` the marked (measured, mapped) half should show what gamut mapping buys. On
the 4-grey panel the halves are close. **On `trmnl_x` the marked half is markedly darker and loses
shadow separation** the unmarked half keeps. `trmnl_x`'s measured palette runs `#383838`–`#B8B8B0`,
so mapping into it *should* darken — **but the unmarked half dithers against that same measured
palette**, so the gap comes from the mapping, not the inks. **Not diagnosed.** Decide which half is
the better preview before touching the mapper.

## Two TLS tests are flaky

`lua_https_tests::{test_https_with_custom_ca_cert, test_https_with_client_certificate}` fail with
`error sending request for url (https://127.0.0.1:…)` — the shape a 30 s timeout takes. Roughly
**one full run in six**.

**The best explanation is the laptop suspending, not CPU contention.** A 30 s timeout is *wall
clock*. **Before spending anything on a fix, check whether a failing run coincides with a sleep**
(`pmset -g log | grep -i sleep`). The null hypothesis — reverting `c850ea7`'s HTTP change — has
still never been tested. **If it needs fixing, do not loosen the test:** cache the
`reqwest::blocking::Client` instead of building one per request. A single shared worker thread is
the wrong answer — it would serialise every screen's HTTP on the server path.

## Three carried-forward questions

1. **`grey_count <= 2` may be the wrong rule.** On the 4-grey panel at 10–12 px mono+aliased beats
   smooth, but at 14 px smooth wins — the fix may be a **size term**. *Always name panels by config
   key:* the **4-colour** `trmnl_og_4clr` already counts as `grey_count = 2`; it is **4-grey**
   `trmnl_og` that is in question, and they behave oppositely. `FontConfig::adaptive_default` is
   the single place the rule lives.
2. **`HintingMode::Light` is byte-identical to `Normal`** — one genuinely inert knob.
3. `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and its
   `kchroma = 10` tuning are not on the crate's dithering path at all.

---

# Settled — do not reopen

- **HACS is gone.** PR #9310 closed by the owner on 2026-08-19; the app is byonk's distribution
  channel. The HACS lore (PR template rules, `hacs-bot` link requirements, alphabetical sorting) is
  history.
- **`home-assistant/brands` is obsolete for custom integrations.** Since HA 2026.3 the integration
  ships its own `brand/` directory, which beats the CDN; brands auto-closes such PRs. Shipped in
  0.17.1. This is **unrelated** to the HACS decision and still true.
- **The device preview is a `camera`, and the reason is the more-info dialog size (~990 px vs
  ~775 px) and the square thumbnail** — *not* full-width rendering, which does not happen. Settled
  by building the `image` variant, comparing side by side on the same page, and deleting it.
  `ImageEntity`'s trap, if you ever revisit: its `state` is `None` until `image_last_updated` is
  stamped, so it shows nothing at all with no error anywhere.
- **The Lua sandbox withholds `io`, `os.execute` and `os.exit`.** Merged in #39; the time helpers
  screens actually use stay. `tests/lua_sandbox_test.rs` pins it.
- **Warn on any render-scale mismatch, no integer-zoom exemption.** Owner decision, re-confirmed by
  a positive control that fires on an exact 2×.
- **Authoring warnings reach the author, not the operator.** `tracing::warn!` reaches the server
  log, not the screen's author.
- **A variant CAN be aliased.** `HintingSpec::to_usvg()` drops `aliased` because it is
  document-level, but `text-rendering` is an inheritable SVG property. Always pair `optimizeSpeed`
  with mono hinting.
- **No bundled font carries a hinting program** (measured across every glyph), so `interpreter` is
  effectively unhinted and `auto ≡ auto_fallback`. The engine axis is **not** dead — it is what
  *shows* these facts.
- **Terminus is NOT buggy.** Terminus @14 and @18 render 1 px/glyph wider — that is correct. Raised
  twice, settled twice.
- **The fonts and the resvg pin must move together.** `test_bitmap_font_render` fails if the pin
  regresses.
- **Falsified, do not chase again:** X11 vertical-metric overflow (real malformation, causes
  nothing); ink overhang in oblique faces (normal); the fvar `wght` default does not leak; Source
  Serif 4 is not pinned at `opsz` 20; upstream will not change `AutoFallback`
  (googlefonts/fontations#1151, closed); `font-weight` does not disable hinting.

---

# Lessons — these keep paying off

**New this session:**

- **Check whether the thing you are about to build already exists.** The first design here was a
  throwaway installer app, mirroring "Get HACS" — before establishing that byonk's integration
  *already* installs the app. Half the motivation evaporated on one `grep`. Ask what the codebase
  already does before designing around a gap.
- **For a permission or behaviour question, read the upstream source, not the docs.** The HA
  developer docs could not say where `homeassistant_config` mounts or what lowers a security
  rating. `supervisor/docker/const.py` and `apps/utils.py` answered both in minutes — and the
  rating answer (it ignores `map` entirely) reversed a recommendation.
- **A feature you assume a crate has may be behind a flag.** `reqwest`'s `.json()` needs the `json`
  feature; the plan compiled only on paper until that was checked.
- **Adding a call inside a shared code path breaks every test that exercises it.** A version check
  in `_async_update_data` reaches all 90 HA tests through one fixture. Look at the fixtures before
  writing the feature, not after the suite goes red.
- **A live runbook is not a historical record.** `docs/superpowers/ha-publishing.md` was nearly
  filed under "historical, leave alone" when its entire subject had just been abandoned.

**Standing:**

- **A design premise about someone else's UI is a claim, not a fact.** "HA renders a camera
  full-width" survived a whole session of implementation and a written handover before anyone
  opened a browser. It was false.
- **When a premise falls, rebuild the alternative rather than reason about it.** Twenty minutes.
- **A base class's `__init__` can undo your attribute.** `Camera.__init__` sets
  `content_type = "image/jpeg"` unconditionally.
- **A parameter slot doing double duty is a trap for the next caller.**
- **Don't fabricate a default that reads as a measurement.** 4.2 V for a device that never reported
  its battery is worse than showing nothing.
- **Put view options in the cache key, not the fingerprint.**
- **Hash a `HashMap` through a `BTreeMap`.** Iteration order varies per process, so a fingerprint
  built from one is a cache that never hits — and it looks like the cache "just doesn't work".
- **A raw string ends at the first `"#`, and hex colours are full of them.** Use `r##"…"##`.
- **A mock rebound after the patch is applied does nothing.** Mutate the existing mock's
  `side_effect`.
- **Test the cache where time is injectable, test the wiring over HTTP.**
- **Demonstrate the check fails when the thing is broken.** A test written *after* the
  implementation has never been shown to fail; sabotage stands in for the RED step.
- **"No warnings" is not coverage until the mechanism has been shown to fire**, and **a toggle that
  changes nothing visible has not been proven to work**.
- **Assert on the geometry, not the pixels, when the question is "is this legible".**
- **A CSS rule beats a presentation attribute in SVG.**
- **Fix the docs when they are the bug.** `docs/src/tutorial/svg-templates.md` has been the bug
  twice; it is embedded and served to LLM authors over MCP.
- **Work left by an agent that died is not verified work.**

---

# Carried forward

Session 29's handover — the preview verified in a browser, the camera-vs-image comparison in full,
the preview endpoint and `PreviewCache` design — is in `git show bdb9473:docs/HANDOVER.md`. **Read
it before touching `camera.py`, `preview_cache.rs` or `ScreenStore::render`**, in particular its
warning not to pass a device's configured dither as `RenderOpts::dither` (that is the *override*
slot, above the script).

Session 26's detail (the resvg `byonk-base` initiative, the render sweep, the two calibration bugs,
font licensing, the whole hinting settlement) is in `git show bf48594~1:docs/HANDOVER.md` — **read
it before touching fonts, hinting or resvg**. Session 27's (the PR-based release) is in
`git show bf48594:docs/HANDOVER.md`. The pinning initiative's detail is in
`git show 3b32762:docs/HANDOVER.md` — read before touching `eink-dither`, gamut mapping or colour
models. Keep the branch `docs/handover-session-27` @ `bf48594`; those two references live on it.
