# Handover — Byonk

_Last updated: 2026-08-19 (session 28). **0.18.0 shipped** — PR #36 merged and the PR-based
release ran end to end for the first time. This session then built the **screen preview on the
Home Assistant device page**: a camera entity showing what each panel displays, backed by a new
byonk admin endpoint, with two toggles for how it is drawn. **It is committed, tested and
unpushed. Nobody has looked at it in a browser yet — that is the next job.**_

## Where the work lives

| | |
|---|---|
| Branch | `feat/ha-device-preview`, based on `origin/main` @ `94d0c8c` |
| HEAD | `2f03c1f` — **tree clean, 3 commits, NOT pushed, no PR yet** |
| `main` | `94d0c8c` (merge of #36, `Release v0.18.0`). Latest tag **`v0.18.0`** |
| Open PRs | Two stale dependabot PRs, **#25** and **#32**. Nothing else |
| Keep this branch | `docs/handover-session-27` @ `bf48594` — the previous handover, never merged. Its content is folded into this file, but **do not delete it**: the *Carried forward* section reaches session 26's and 27's handovers through `bf48594` and `bf48594~1`, and deleting the branch makes both unreachable |
| Push gotcha | The ssh-agent holds **no identities**, so `git push origin …` fails on publickey. `gh` is authenticated over HTTPS — `git push https://github.com/oetiker/byonk.git <branch>` works and leaves the remote config alone |
| `main` protection | Ruleset `main-protect`: PR required (0 approvals), 5 required checks (`Build`, `Test`, `Check & Lint`, `Analyze (actions)`, `Analyze (rust)`), strict up-to-date. Bypass: **Repository admin** — you can always merge, no PAT can |

The three commits, oldest first:

| Commit | What |
|---|---|
| `f4b754e` | `RenderOpts::params` + `RenderOpts::device` — lets `ScreenStore::render` stand in for a device |
| `7c039a0` | `GET /api/admin/devices/{key}/preview` + `PreviewCache` |
| `2f03c1f` | The Home Assistant camera, the two view switches, the refresh button |

---

# What to do next

**1. Look at it on the VM.** Everything below is tested, but *"does the picture actually render
full-width and legibly on a real device page"* is a claim no test here can make. Use the
`ha-vm-testing` skill. The specific questions:

- Does the camera appear **full-width** on the device page, or has Home Assistant's device-page
  layout changed since? (The whole reason this is a `camera` and not an `image` entity.)
- **How badly does the dither texture suffer** when an 800×480 e-ink render is scaled to card
  width? This was flagged to the owner up front as the one unknown. The *Preview dithering*
  toggle exists partly because of it — check whether the undithered view is in fact the one you
  reach for.
- Does the **`camera` component set itself up cleanly** on HAOS? It is not in `dependencies`;
  it arrives through `async_forward_entry_setups`, which installs its `PyTurboJPEG` requirement
  on demand. That path is standard but has not been exercised here.
- Does toggling a switch change the picture within the 10 s frame interval?

Remember `make ha-rebuild` does **not** sync the add-on manifest — an options-schema change
needs a manual version bump plus `POST /store/reload` and `ha addons update` on the VM. This
change adds no add-on options, so that should not bite, but it is the usual trap.

**2. Push the branch and open a PR.** Nothing is outstanding against it otherwise.

**3. Carried over from session 27, still not done: delete the `RELEASE_TOKEN` secret and revoke
the PAT.** `gh secret list` still shows it (created 2026-07-17). 0.18.0 released without it —
nothing references it any more, and removing it is the entire point of the PR-based release.

---

# Session 28 — the screen preview

## What the owner asked for, in order

1. "Enhance the HA integration so the config screen shows a copy of the configured screen" —
   clarified to **the device page**, not the config-flow form.
2. "Could it only be rendered when the device config screen is open?" — yes, and that shaped
   the whole design.
3. "We need it large, so camera sounds cool … byonk could cache its answer and only rerender
   when parameters change."
4. Cache TTL: owner chose **expire after the screen's `refresh_rate`** over pure
   parameter-change invalidation, once the frozen-clock consequence was pointed out.
5. "Add toggles to configure the preview data, allowing to switch off the dithering and the
   color mapping" — **two** toggles.

## The byonk side

`GET /api/admin/devices/{key}/preview` → `image/png`, admin-token guarded.

| Query | Effect |
|---|---|
| *(none)* | The dithered image the panel receives |
| `?force` | Re-render regardless of the cache |
| `?dither=off` | The pre-dither, full-colour rasterization — no palette restriction |
| `?measured=off` | Spec colours byonk sends to the panel, not a calibration's measured ones |

`off`/`0`/`false`/`no` (any case) mean no; **anything else means yes**, so `dither=on` and
`dither=1` keep it on. `404` when the key has no device config — nothing is assigned, so there
is nothing to preview. A **failed render returns 200 with the error image the panel itself
would show**; a broken-image icon would say only that something went wrong, not what.
Responses carry `Cache-Control: no-store` and **`X-Byonk-Preview: hit|miss`**, which is what
makes the cache observable from a test and from a running add-on.

### Why it goes through `ScreenStore::render` and not `handle_display`

`handle_display` is built around firmware headers — `Colors`, `Board`, `Measured-Colors`,
`Width`/`Height` — that a preview request does not have and **must not invent**. Reimplementing
its resolution chain would have been a second copy that drifts. Instead `ScreenStore::render`
(the authoring/MCP/`/dev/render` path) gained the two things it lacked:

- **`RenderOpts::params`** — the device's configured Lua params.
- **`RenderOpts::device: Option<DevicePreview>`** — the registry identity *and* the
  device-config layer.

`DevicePreview` carries both halves deliberately:

- **Identity** (`mac`, `firmware_version`, `battery_voltage`, `rssi`) is passed through as-is.
  A device the registry has never heard from reports `None`, which reaches Lua as a missing
  key. It does **not** fall back to the authoring placeholders — showing 4.2 V for a device
  that has never reported its battery is a fabricated reading, not a default.
- **Config layer** (`colors`, `dither`, `tuning`, `refresh`) occupies the *device-config* slots
  of `resolve_ctx_palette` / `resolve_render_params` / `resolve_effective_tuning`.

> **Do not "simplify" this by passing a device's configured dither as `RenderOpts::dither`.**
> That is the *override* slot, which sits **above** the script. The preview would then dither
> differently from the panel for any screen that picks its own algorithm. Pinned by
> `script_dither_beats_the_device_config` and its contrast case in
> `tests/screen_store_render_params_test.rs`.

One subtlety worth re-reading before touching `ScreenStore::render`: `resolve_render_params`
has only two dither slots for three layers, and resolves them as
`script_dither.or(device_config_dither)`. The override rides in the device-config slot **because
an explicit override has already blanked `effective_script_dither` above it**. That is what
keeps override > script > device-config intact. There is a comment saying so at the call site.

`run_script_direct` became YAML-native (matching `run_resolved` and `DeviceConfig::params`); the
JSON→YAML conversion moved out to `content_pipeline::json_params_to_yaml` at the one caller that
actually holds JSON, `/dev/render`'s query params.

### `PreviewCache` — the thing that makes the frame rate affordable

`src/services/preview_cache.rs`. A rendered PNG is re-served until **either**:

1. **The fingerprint moves** — screen ref, params, panel, dither, colours, tuning, refresh,
   model, geometry. Hashed through a **`BTreeMap`**, because `DeviceConfig::params` is a
   `HashMap` and a fingerprint that varies per process is a cache that never hits.
2. **It ages past the device's effective refresh rate**, floored at **`MIN_TTL_SECS = 30`**. A
   screen declaring a few seconds would otherwise turn an open device page into a render loop.

The refresh rate is the device's *effective* one, because `DevicePreview::refresh` feeds
`DeviceContext::refresh_override` — so the preview is exactly as fresh as the panel it stands
for, using a number the screen author already chose.

**Nothing runs on a timer.** Entries are examined only when a request arrives, so a preview
nobody watches costs nothing at all. That property is the whole design; do not add a background
refresh.

**The view options are in the cache *key*, not the fingerprint** — `"{key}#{dithered}{measured}"`.
Folding them into the fingerprint would leave one slot per device and re-render on every toggle
flip. Capacity is 64 because keys are per device *and* per variant.

`reload_config` calls `PreviewCache::clear()`. A device's fingerprint covers its own config but
not a **panel profile it merely points at**, so editing a panel's colours or dither tuning would
otherwise serve a stale render until the TTL elapsed.

**Known gap, accepted:** the fingerprint cannot see a *screen's source files*. Editing a screen
over MCP does not invalidate the preview; the TTL bounds it and the refresh button ends it
immediately. Documented in `docs/src/api/admin-api.md`.

## The Home Assistant side

`custom_components/byonk/camera.py`, plus switches in `switch.py` and a button in `button.py`.

**Why a camera and not an image entity.** Home Assistant renders a camera **full-width** on the
device page; an image entity is only a row thumbnail. The owner asked for large.

**Why it costs nothing idle.** `Camera._attr_should_poll = False` — Home Assistant never polls
a camera. Frames are pulled only while a browser holds the picture open. Verified by reading
`components/camera/__init__.py:442`, not assumed.

**`_attr_frame_interval = 10.0`.** Without `CameraEntityFeature.STREAM` the frontend opens the
MJPEG still-stream, and `async_get_still_stream` calls `async_camera_image()` once per
`frame_interval` — whose default is `MIN_STREAM_INTERVAL = 0.5` **seconds**, meant for video.
Left alone that is a request twice a second for as long as the page is open.

**Two bugs found while building this, both fixed:**

1. **`Camera.__init__` sets `content_type = "image/jpeg"`.** Left at that, the PNG is served
   mislabelled **and** `_async_get_image` — which picks the scaling path *purely by content
   type* — hands it to `scale_jpeg_camera_image`, i.e. libturbojpeg, which cannot read a PNG.
   Fixed in `__init__`, pinned by `test_the_camera_reports_png`.
2. The `ScreenStore::render` dither-slot ordering described above.

**The two toggles live in the device config entry's `options`, never in byonk.** Writing them
to byonk's device config would alter the real screen in order to change a picture of it.
`test_turning_off_dithering_asks_for_the_undithered_render` asserts `update_device` is never
called. No options-update listener is registered, so `async_update_entry` does **not** reload
the entry — the camera just reads the current value on its next frame.

**A failed fetch returns `None`, never raises.** An exception escaping `async_camera_image`
marks the entity unavailable and takes the device page's screen/dither/panel controls down with
it. Pinned by `test_a_failed_fetch_leaves_the_camera_alive`.

**The refresh button forces the variant on display**, not byonk's default one — refreshing the
dithered render while somebody is looking at the undithered one refreshes an image nobody can
see.

`PyTurboJPEG==1.8.0` joined `requirements_test.txt`: `homeassistant.components.camera` imports
it at module load, so without it the test module will not even collect.

## One reading the owner has not confirmed

The owner said **"color mapping"**. It was implemented as byonk's **measured-vs-spec** mapping
(`use_actual`) — drawing the palette in the colours a calibration says the panel really
produces, versus the ones byonk sends it. The other candidate reading, the palette restriction
itself, is already covered by the dithering toggle, since that returns the full-colour pre-dither
render — which is why this reading was chosen without asking. **It was flagged to the owner and
they have not responded.** If they say otherwise, `?measured` is the knob to change.

---

# Verification state

| Suite | Result |
|---|---|
| `tests/admin_preview_test.rs` | **15 passed** (auth, 404, PNG, no-store, cache hit/miss, force, config change, error image, both toggles, spelling variants, per-variant cache slots) |
| `src/services/preview_cache.rs` unit tests | **9 passed** (time is injected — nothing sleeps) |
| `tests/screen_store_render_params_test.rs` | **10 passed** (params, identity, no-fabricated-telemetry, dither precedence both ways, device colours) |
| `tests_ha` | **90 passed**, 13 of them in `test_camera.py`. `ruff` clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| Full `cargo test` | Green on `7c039a0`'s content (32 binaries, 0 failures). **A final `cargo test --workspace` on `2f03c1f` was started and not seen to completion — re-run it before pushing.** |

**Not verified:** anything in a browser. See *What to do next*.

**Note the CI gap:** `.github/workflows/ci.yml` runs hassfest and HACS validation but **does not
run `tests_ha`**. The 90 Python tests are local-only — `make ha-check` is the only thing that
runs them. Worth deciding whether to wire them into CI; a broken integration would pass CI today.

---

# The release process, as it now works

Two workflows. **Nothing pushes to `main`, so no PAT is involved anywhere.** `GITHUB_TOKEN`
cannot push to protected `main`, but it can push an ordinary branch, open a PR, and push a
**tag** — the ruleset targets branches, and tags are a separate ref namespace.

| Workflow | Trigger | Does |
|---|---|---|
| `create-release-pr.yml` | `workflow_dispatch` + bugfix/feature/major | waits for CI to be green on the exact commit, computes the version from tags, bumps everything, opens a `release/vX.Y.Z` PR. **Nothing is tagged or published** |
| `release-publisher.yml` | `push` to `main` touching `Cargo.toml` | tags, builds 5 binaries, builds and pushes the container, publishes the GitHub release, deploys the docs. **No `workflow_dispatch` by design** |

**To cut a release:** Actions → *Create release PR* → pick the type → review the PR it opens →
merge it. That is the whole procedure. **0.18.0 was the first full end-to-end run and it
worked.**

## What the release PR bumps — all in one commit, all together

| File | By |
|---|---|
| `Cargo.toml` + **`Cargo.lock`** | `cargo update --workspace`, then verified |
| `CHANGES.md` | rolls `Unreleased` into `## X.Y.Z - date` |
| `custom_components/byonk/manifest.json` | `tools/release/bump-integration-version.sh` |
| `homeassistant/byonk/config.yaml` + its `CHANGELOG.md` | `tools/release/bump-addon-version.sh` |
| `screens/**/meta.yaml` **and** `docs/src/**/*.md` | `tools/release/bump-screen-engine.sh` |

All three bump scripts have test scripts run by CI's **`Release Scripts`** job.

## Things about it you need to know

- **The add-on version *is* the ghcr image tag.** For the ~15–20 minutes between merging and
  the publisher pushing the image, the add-on store advertises a version that does not exist
  yet. A user refreshing in that window gets a pull failure and succeeds on a retry.
  **Owner decision, deliberate**, in exchange for one PR per release.
- **The publisher's guard asks whether the GitHub *release* exists, not the tag.** Guarding on
  the tag would make a run that died after tagging unrecoverable, because the tag is created
  first. As built, a failed publisher run is **re-runnable from the Actions UI**.
- **A non-404 failure when asking about the release stops the run.** Treating every error as
  "not released" would re-publish a shipped version.
- **A cancelled release leaves its branch behind.** `create-release-pr.yml` clears a stale
  `release/v*` branch **unless it still has an open PR**, which gets a pointed error.
- **The publisher fires on any push touching `Cargo.toml`** — a dependabot PR will start it.
- Both new guards (wait-for-CI-on-this-commit, clear-a-stale-release-branch) were exercised for
  real on 2026-08-18 and both worked — but **each has run exactly once**. Do not treat them as
  permanently proven.

---

# Changelog discipline

`CHANGES.md`'s `Unreleased` section has this session's entry under **New**. Two standing rules:

- **User-facing only.** CI, tooling, version automation and dev process do not belong there.
- **Read the section as a set, not as a stream of appends.** Five sessions of appending once
  produced two entries that contradicted each other and two under the wrong heading. The reader
  sees it all at once even though it was never written that way.

---

# Queued work

| ID | What |
|---|---|
| — | **Verify the preview on the VM, then push + PR.** The only thing actually pending |
| — | **Delete `RELEASE_TOKEN` and revoke the PAT** (carried from session 27) |
| — | Decide whether `tests_ha` should run in CI |
| — | Two stale dependabot PRs, #25 and #32 |
| F13 | Extend `screens/examples/demo/font/{ttf,bitmap,hinting}/` to cover Source |
| F14 | Licence + notice files. **`FONTS.md`'s "X11LuType is proportional" is wrong — it is monospaced** |
| F22 | Cosmetic: the WiFi glyph reads as a caret at 8×12. Redraw or drop it |
| F23 | The two fetching examples fail in a sandbox with `Cannot drop a runtime…` *from the fetch error path*; check whether any other blocking call in `lua_runtime.rs` shares the hazard |
| F24 | `/dev/render` shows the author nothing but an image — it passes `None` for the script log sink, so neither their `log_*` output nor byonk's authoring warnings reach the browser preview |
| Plan Task 11 | The hinted-font-trio **decision** (specimens + recommendation) |

---

# Open items the owner should decide

## Marking costs shadow detail at 16 grey levels

On `calibration/tone` the marked (measured, mapped) half is supposed to show what gamut mapping
buys. On the 4-grey panel the two halves are close. **On `trmnl_x` the marked half is markedly
darker and loses shadow separation** the unmarked half keeps. `trmnl_x`'s measured palette runs
`#383838`–`#B8B8B0`, so mapping into it *should* darken — **but the unmarked half dithers
against that same measured palette**, so the gap comes from the mapping, not the inks. On a
panel with 16 levels, losing shadow separation is the opposite of what the extra levels are for.
**Not diagnosed.** Decide which half is the better preview before touching the mapper.

## Two TLS tests are flaky

`lua_https_tests::{test_https_with_custom_ca_cert, test_https_with_client_certificate}` fail
with `error sending request for url (https://127.0.0.1:…)`, the shape a 30 s timeout takes.
Roughly **once in six** full runs.

**The best explanation is the laptop suspending, not CPU contention.** A 30 s timeout is *wall
clock*, so a suspend mid-test blows it no matter how idle the CPU is. **Before spending anything
on a fix, check whether a failing run coincides with a sleep** (`pmset -g log | grep -i sleep`).
The null hypothesis — reverting `c850ea7`'s HTTP change — has still never been tested.

**If it needs fixing, do not loosen the test.** Cache the `reqwest::blocking::Client` instead of
building one per request. A single shared worker thread is the wrong answer: it would serialise
every screen's HTTP on the server path.

## Three carried-forward questions

1. **`grey_count <= 2` may be the wrong rule.** On the 4-grey panel at 10–12 px mono+aliased
   beats smooth, but at 14 px smooth wins — the fix may be a **size term**. *Always name panels
   by config key:* the **4-colour** `trmnl_og_4clr` already counts as `grey_count = 2`; it is
   **4-grey** `trmnl_og` that is in question, and they behave oppositely.
   `FontConfig::adaptive_default` is the single place the rule lives.
2. **`HintingMode::Light` is byte-identical to `Normal`** — one genuinely inert knob.
3. `for_error_diffusion()` is applied to **every** dither (`api/builder.rs`), so HyAB and its
   `kchroma = 10` tuning are not on the crate's dithering path at all.

---

# Settled — do not reopen

- **Warn on any render-scale mismatch, no integer-zoom exemption.** Owner decision, re-confirmed
  by a positive control that fires on an exact 2×.
- **Authoring warnings reach the author, not the operator.** `tracing::warn!` reaches the server
  log, not the screen's author. Such warnings belong in the script log sink and on the CLI's
  stderr — never only in `tracing`.
- **A variant CAN be aliased.** `HintingSpec::to_usvg()` drops `aliased` because it is
  document-level, but `text-rendering` is an inheritable SVG property, so the element using a
  variant asks for it directly. Always pair `optimizeSpeed` with mono hinting.
- **No bundled font carries a hinting program** (measured across every glyph). So `interpreter`
  is effectively unhinted and `auto ≡ auto_fallback`. The engine axis is **not** dead — it is
  what *shows* these facts.
- **Terminus is NOT buggy.** Terminus @14 and @18 render 1 px/glyph wider — that is correct.
  Raised twice, settled twice.
- **The fonts and the resvg pin must move together.** `test_bitmap_font_render` fails if the pin
  regresses.
- **Falsified, do not chase again:** X11 vertical-metric overflow (real malformation, causes
  nothing); ink overhang in oblique faces (normal); the fvar `wght` default does not leak;
  Source Serif 4 is not pinned at `opsz` 20; upstream will not change `AutoFallback`
  (googlefonts/fontations#1151, closed); `font-weight` does not disable hinting.

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~15–40 min on this machine — background it**; it
  runs `cargo fmt`, not `--check`, so it rewrites files.
- `make ha-check` = `ruff` + `pytest tests_ha`. Needs `make ha-setup` once. **Instant** (~2 s) —
  no reason not to run it.
- **`cargo test` links each test binary serially and slowly here** (~1–2 min per binary, 32
  binaries). `cargo test --test <name>` for a single file is the fast loop.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`. Add by
  explicit path and check `git diff --cached` before committing.
- **Never run `make check` while the tree is being edited.** Also `make check > log; echo
  "EXIT=$?"` reports the *echo's* status. Same trap with any pipe: `cmd | tail; echo $?` reports
  `tail`. **This bites in background jobs too** — a backgrounded `cargo test … | tail` reports
  exit 0 while the test text says FAILED, and emits nothing until it finishes.
- **A sleeping laptop looks exactly like a hung build.** `ps -eo pid,etime,command` reports
  *wall* time. An IDE `cargo check --workspace` also holds the build lock and will stall your
  `cargo test` with "Blocking waiting for file lock".
- **Editing an embedded asset forces a rebuild** (`api/lua-api.md`, `tutorial/svg-templates.md`,
  `guide/authoring.md`, `byonk-base/`, `screens/{builtin,examples}/`). Other `docs/src/` pages
  are free.
- **In a debug build rust-embed reads from disk at runtime**, so screen edits take effect with
  no rebuild — but "no change" is then indistinguishable from a stale binary, so **prove
  disk-backing with a visible sabotage first**.
- **Subagents must not run `make check`** — the 600 s watchdog kills them.
- **IDE diagnostics lie in this tree.** Only an actual cargo run counts.
- `make docs` = `mdbook build`. `docs/book/` and `docs/src/images/` are gitignored.

## Capturing every bundled screen, on every panel

```bash
BYONK_BIN=./target/debug/byonk ./tools/capture-renders.sh /path/to/out
```

Seconds, not minutes. **19 captures across 5 panels.** **Do not put the output in `/tmp`** —
that is how the previous baselines were lost. **A capture is only as wide as
`tools/capture-config.yaml`'s device map**; when adding a panel profile, add a device for it
here too.

## Testing the preview endpoint by hand

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/admin/devices/DEFAULT/preview -o preview.png
curl -sD- -o /dev/null -H "Authorization: Bearer $TOKEN" \
  'http://localhost:3000/api/admin/devices/DEFAULT/preview?dither=off' | grep -i x-byonk
```

`X-Byonk-Preview` tells you whether it rendered or re-served — the fastest way to see whether
the cache is behaving. On the VM, curl from the **Mac host** on port 3000, not from the Terminal
add-on, and **never print the admin token**.

---

# Lessons — these keep paying off

- **Read the framework's source before designing around it.** "Does HA poll a camera?" and
  "what does `frame_interval` default to?" were both settled in two minutes from
  `.venv/lib/python3.13/site-packages/homeassistant/components/camera/__init__.py`. Both answers
  shaped the design, and one of them (0.5 s) would have been a serious bug if assumed.
- **A base class's `__init__` can undo your attribute.** `Camera.__init__` sets
  `content_type = "image/jpeg"` unconditionally. With multiple inheritance, check what each base
  writes — and note that neither `Camera` nor `ByonkDeviceEntity` calls `super().__init__()`, so
  both must be called explicitly.
- **A parameter slot doing double duty is a trap for the next caller.**
  `resolve_render_params`'s device-config dither slot was carrying the *override*, which worked
  only because of a blanking step above it. Adding a real device-config layer looked like a
  one-word change and was not.
- **Don't fabricate a default that reads as a measurement.** A preview showing 4.2 V for a
  device that has never reported its battery is worse than showing nothing.
- **Put view options in the cache key, not the fingerprint.** They select between images of the
  same state; a fingerprint says the state changed. Conflating them makes every toggle a
  re-render.
- **Hash a `HashMap` through a `BTreeMap`.** Iteration order varies per process, so a
  fingerprint built from one is a cache that never hits — and it will look like the cache
  "just doesn't work" rather than like a bug.
- **A raw string ends at the first `"#`, and hex colours are full of them.** `r#"…"#` around an
  SVG containing `stop-color="#000"` fails with `prefix 'fff' is unknown`. Use `r##"…"##`.
  Fourth time in this repo.
- **A mock rebound after the patch is applied does nothing.** `patch.multiple(...)` binds the
  fixture's mock into the client at setup; reassigning `state.get_device_preview = AsyncMock(…)`
  afterwards leaves the client using the original. Mutate the existing mock's `side_effect`.
- **Not every attribute is a state attribute.** `frame_interval` is read off the entity by
  `handle_async_still_stream`, not published in `state.attributes` — assert against the live
  entity via `hass.data[DATA_INSTANCES]["camera"].get_entity(...)`.
- **Test the cache where time is injectable, test the wiring over HTTP.** Nine cache tests run
  in 0 ms because `now` is a parameter. Trying to prove caching over HTTP would have needed
  either a sleep or a screen whose output changes per render — both flaky.
- **Demonstrate the check fails when the thing is broken.** A test written *after* the
  implementation has never been shown to fail; sabotage stands in for the RED step. Back the
  file up first and diff after restoring.
- **"No warnings" is not coverage until the mechanism has been shown to fire.**
- **Coverage that is wide in one dimension can be nil in another.** Ask what a sweep holds
  *constant*, not just what it covers.
- **A tool that hides a channel makes every future run lie.** When a new output channel is
  added, check what the existing harnesses do with it.
- **Assert on the geometry, not the pixels, when the question is "is this legible".** A screen
  whose labels overlap into a smear renders perfectly validly.
- **A CSS rule beats a presentation attribute in SVG.** `text { font-family: … }` silently
  overrides every `font-family="…"` attribute, and the text still renders — in the wrong face.
- **Fix the docs when they are the bug.** `docs/src/tutorial/svg-templates.md` has been the bug
  twice; it is embedded and served to LLM authors over MCP.
- **Work left by an agent that died is not verified work.**

---

# Carried forward

Session 26's detail (the resvg `byonk-base` initiative in full, the render sweep, the two
calibration bugs, font licensing, the whole hinting settlement) is in
`git show bf48594~1:docs/HANDOVER.md` — **read it before touching fonts, hinting or resvg**.
Session 27's (the PR-based release, in more detail than the summary above) is in
`git show bf48594:docs/HANDOVER.md`. The pinning initiative's detail is in
`git show 3b32762:docs/HANDOVER.md` — read before touching `eink-dither`, gamut mapping or
colour models.

`git worktree list` is clean.
