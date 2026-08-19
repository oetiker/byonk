# Handover — Byonk

_Last updated: 2026-08-19 (session 29). The device-page screen preview built in session 28 has
now **been looked at in a browser on the VM**, which was the one thing session 28 could not
claim. It works — but **the reason it was built as a camera turned out to be false**, and that
was checked by building the alternative and comparing. The branch is tested, verified in a real
UI, and **still unpushed with no PR**._

## Where the work lives

| | |
|---|---|
| Branch | `feat/ha-device-preview`, based on `origin/main` @ `94d0c8c` |
| HEAD | `ec0be6c` — **tree clean, 5 commits, NOT pushed, no PR yet** |
| `main` | `94d0c8c` (merge of #36, `Release v0.18.0`). Latest tag **`v0.18.0`** |
| Open PRs | Two stale dependabot PRs, **#25** and **#32**. Nothing else |
| Keep this branch | `docs/handover-session-27` @ `bf48594` — never merged. **Do not delete it**: the *Carried forward* section reaches sessions 26 and 27 through `bf48594` and `bf48594~1` |
| Push gotcha | The ssh-agent holds **no identities**, so `git push origin …` fails on publickey. `gh` is authenticated over HTTPS — `git push https://github.com/oetiker/byonk.git <branch>` works and leaves the remote config alone |
| `main` protection | Ruleset `main-protect`: PR required (0 approvals), 5 required checks (`Build`, `Test`, `Check & Lint`, `Analyze (actions)`, `Analyze (rust)`), strict up-to-date. Bypass: **Repository admin** — you can always merge, no PAT can |

The five commits, oldest first:

| Commit | What |
|---|---|
| `f4b754e` | `RenderOpts::params` + `RenderOpts::device` — lets `ScreenStore::render` stand in for a device |
| `7c039a0` | `GET /api/admin/devices/{key}/preview` + `PreviewCache` |
| `2f03c1f` | The Home Assistant camera, the two view switches, the refresh button |
| `fea552d` | Session 28's handover |
| `ec0be6c` | The camera docstring, corrected against what the UI actually does |

---

# What to do next

**1. Push the branch and open a PR.** Everything below is verified. Nothing is outstanding
against the branch itself.

**2. Confirm the layout on HA 2026.8.2 before you trust the comparison.** The VM runs
**2026.7.2**; 2026.8.2 is out. The entire camera-vs-image decision rests on one version's device
page layout, and that layout is exactly the thing that already proved false once.

**3. Carried over from session 27, still not done: delete the `RELEASE_TOKEN` secret and revoke
the PAT.** `gh secret list` still shows it (created 2026-07-17). 0.18.0 released without it.

**4. Decide whether `tests_ha` should run in CI.** `.github/workflows/ci.yml` runs hassfest and
HACS validation but **not the 90 Python tests** — `make ha-check` is the only thing that does. A
broken integration would pass CI today.

---

# Session 29 — the preview, seen at last

## The finding that matters

**Home Assistant does not render a camera full-width on the device page.** On 2026.7.2 it is a
row in the *Sensors* card with a small thumbnail. That claim was the stated reason the preview
was built as a `camera` and not an `image` entity, and it is wrong.

It was not corrected by argument. An `image` entity variant was **built, deployed and compared
side by side on the same device page**, and the camera still won — on different grounds:

| | Camera | Image entity |
|---|---|---|
| Device page | Row in *Sensors*, **square** thumbnail | Row in *Sensors*, **circle-cropped** thumbnail — an 800×480 screen loses its corners |
| More-info dialog | Fills it edge to edge, **~990 px** | Inset, plus a header row, **~775 px** |
| State | `Idle` | a relative timestamp — genuinely more useful |
| Refresh while open | automatic, every 10 s | only when `image_last_updated` moves |
| Requirement | `PyTurboJPEG` | none |

The experiment was then deleted. `camera.py`'s docstring now records the measured reasons
(`ec0be6c`). **If you ever revisit this, `ImageEntity` has a trap worth knowing: its `state` is
`None` until `image_last_updated` is set, so an image entity that never stamps it shows nothing
at all, with no error anywhere.**

## What else the browser confirmed

| Question session 28 could not answer | Answer |
|---|---|
| Does the `camera` component set up cleanly on HAOS? | **Yes.** No errors; the on-demand `PyTurboJPEG` install path is fine |
| Is the PNG served correctly? | **Yes** — the `content_type` fix holds; the picture renders |
| Does the dither texture survive being scaled? | **Yes**, at dialog size it is completely legible. That worry was unfounded |
| Does a toggle change the picture? | **Yes** — *Preview dithering* off gives the full-colour pre-dither render, visibly and promptly |
| Does *Preview measured colors* work? | **Unproven.** No visible change on `DEFAULT`, which is a grey panel with no calibration — so measured and spec palettes are identical there. Not evidence of a bug; not evidence of correctness either. **To test it you need a device on a panel with a calibration** |

## A real bug found on the way, not fixed

`custom_components/byonk/addon.py:29` — `_async_find_addon_item` returns the **first** store item
whose slug ends in `_byonk`, and the config entry then stores that slug forever. With two byonk
add-ons installed (the published `<hash>_byonk` and a from-source `local_byonk`), **which one the
integration adopts is down to the order the Supervisor happens to list them.**

This is not only a test-VM curiosity: reauth re-reads the *stored* slug and never re-discovers
(`config_flow.py:128`), so an entry bound to the wrong add-on **cannot heal itself** — it fails
with `401`, tries to restart the wrong add-on, and hits "port 3000 is already in use". The only
cure is deleting and re-adding the entry, which also orphans every device entry, because those
store the hub's `entry_id` (`__init__.py:62`).

Left unfixed deliberately — it needs a decision about what "the byonk add-on" means when there
are two.

## State of the test VM, which is now different

| | |
|---|---|
| byonk | **`local_byonk` only**, built from this branch's source, running |
| Published add-on | **uninstalled** (owner approved) — it was holding port 3000 and blocking the from-source build |
| Config entries | hub + `Byonk Default`, both freshly re-added and healthy |
| `TRMNL 94:A9:90:8C:6D:18` | **gone.** That device lived in the published add-on's config; the new server reports one device |
| HA core | 2026.7.2 |

The add-on still reports version `0.17.1-src3` — that string is the VM's add-on manifest, not
the code, which is current. Chrome on the Mac host holds a live HA session as *Byonk Admin*, so
the UI can be driven without credentials. **The HA owner password is not written down anywhere
in this repo** — `byonk`/`byonk` in `tools/ha-vm/README.md` is the *Samba* add-on's.

---

# The feature itself, as built in session 28

## The byonk side

`GET /api/admin/devices/{key}/preview` → `image/png`, admin-token guarded.

| Query | Effect |
|---|---|
| *(none)* | The dithered image the panel receives |
| `?force` | Re-render regardless of the cache |
| `?dither=off` | The pre-dither, full-colour rasterization — no palette restriction |
| `?measured=off` | Spec colours byonk sends to the panel, not a calibration's measured ones |

`off`/`0`/`false`/`no` (any case) mean no; **anything else means yes**. `404` when the key has no
device config. A **failed render returns 200 with the error image the panel itself would show**;
a broken-image icon would say only that something went wrong, not what. Responses carry
`Cache-Control: no-store` and **`X-Byonk-Preview: hit|miss`**, which is what makes the cache
observable from a test and from a running add-on.

### Why it goes through `ScreenStore::render` and not `handle_display`

`handle_display` is built around firmware headers — `Colors`, `Board`, `Measured-Colors`,
`Width`/`Height` — that a preview request does not have and **must not invent**. Instead
`ScreenStore::render` gained the two things it lacked:

- **`RenderOpts::params`** — the device's configured Lua params.
- **`RenderOpts::device: Option<DevicePreview>`** — the registry identity *and* the device-config
  layer.

`DevicePreview` carries both halves deliberately:

- **Identity** (`mac`, `firmware_version`, `battery_voltage`, `rssi`) passes through as-is. A
  device the registry has never heard from reports `None`, which reaches Lua as a missing key. It
  does **not** fall back to the authoring placeholders — showing 4.2 V for a device that has never
  reported its battery is a fabricated reading, not a default.
- **Config layer** (`colors`, `dither`, `tuning`, `refresh`) occupies the *device-config* slots of
  `resolve_ctx_palette` / `resolve_render_params` / `resolve_effective_tuning`.

> **Do not "simplify" this by passing a device's configured dither as `RenderOpts::dither`.**
> That is the *override* slot, which sits **above** the script. The preview would then dither
> differently from the panel for any screen that picks its own algorithm. Pinned by
> `script_dither_beats_the_device_config` and its contrast case in
> `tests/screen_store_render_params_test.rs`.

One subtlety worth re-reading before touching `ScreenStore::render`: `resolve_render_params` has
only two dither slots for three layers, and resolves them as
`script_dither.or(device_config_dither)`. The override rides in the device-config slot **because
an explicit override has already blanked `effective_script_dither` above it**. There is a comment
saying so at the call site.

`run_script_direct` became YAML-native (matching `run_resolved` and `DeviceConfig::params`); the
JSON→YAML conversion moved out to `content_pipeline::json_params_to_yaml` at the one caller that
actually holds JSON, `/dev/render`'s query params.

### `PreviewCache`

`src/services/preview_cache.rs`. A rendered PNG is re-served until **either**:

1. **The fingerprint moves** — screen ref, params, panel, dither, colours, tuning, refresh, model,
   geometry. Hashed through a **`BTreeMap`**, because `DeviceConfig::params` is a `HashMap` and a
   fingerprint that varies per process is a cache that never hits.
2. **It ages past the device's effective refresh rate**, floored at **`MIN_TTL_SECS = 30`**.

**Nothing runs on a timer.** Entries are examined only when a request arrives, so a preview
nobody watches costs nothing at all. That property is the whole design; do not add a background
refresh.

**The view options are in the cache *key*, not the fingerprint** — `"{key}#{dithered}{measured}"`.
Folding them into the fingerprint would leave one slot per device and re-render on every toggle
flip. Capacity is 64 because keys are per device *and* per variant.

`reload_config` calls `PreviewCache::clear()`. A device's fingerprint covers its own config but
not a **panel profile it merely points at**.

**Known gap, accepted:** the fingerprint cannot see a *screen's source files*. Editing a screen
over MCP does not invalidate the preview; the TTL bounds it and the refresh button ends it
immediately. Documented in `docs/src/api/admin-api.md`.

## The Home Assistant side

`custom_components/byonk/camera.py`, plus switches in `switch.py` and a button in `button.py`.

**Why it costs nothing idle.** `Camera._attr_should_poll = False` — Home Assistant never polls a
camera. Frames are pulled only while a browser holds the picture open. Verified by reading
`components/camera/__init__.py:442`, not assumed.

**`_attr_frame_interval = 10.0`.** Without `CameraEntityFeature.STREAM` the frontend opens the
MJPEG still-stream, and `async_get_still_stream` calls `async_camera_image()` once per
`frame_interval` — whose default is `MIN_STREAM_INTERVAL = 0.5` **seconds**, meant for video.

**A bug found while building this, fixed:** `Camera.__init__` sets `content_type = "image/jpeg"`.
Left at that, the PNG is served mislabelled **and** `_async_get_image` — which picks the scaling
path *purely by content type* — hands it to `scale_jpeg_camera_image`, i.e. libturbojpeg, which
cannot read a PNG. Pinned by `test_the_camera_reports_png`.

**The two toggles live in the device config entry's `options`, never in byonk.** Writing them to
byonk's device config would alter the real screen in order to change a picture of it.
`test_turning_off_dithering_asks_for_the_undithered_render` asserts `update_device` is never
called. No options-update listener is registered, so `async_update_entry` does **not** reload the
entry — the camera reads the current value on its next frame.

**A failed fetch returns `None`, never raises.** An exception escaping `async_camera_image` marks
the entity unavailable and takes the device page's screen/dither/panel controls down with it.
Pinned by `test_a_failed_fetch_leaves_the_camera_alive`.

**The refresh button forces the variant on display**, not byonk's default one.

`PyTurboJPEG==1.8.0` is in `requirements_test.txt`: `homeassistant.components.camera` imports it
at module load, so without it the test module will not even collect.

## One reading the owner has not confirmed

The owner said **"color mapping"**. It was implemented as byonk's **measured-vs-spec** mapping
(`use_actual`). The other candidate reading, the palette restriction itself, is already covered by
the dithering toggle. **It was flagged and they have not responded.** If they say otherwise,
`?measured` is the knob to change. Note this toggle is also the one still unproven in a browser.

---

# Verification state

| Suite | Result |
|---|---|
| `cargo test --workspace` | **1172 passed, 0 failed, 52 ignored** across 40 binaries, on this branch's code |
| `tests/admin_preview_test.rs` | 15 passed (auth, 404, PNG, no-store, cache hit/miss, force, config change, error image, both toggles, spelling variants, per-variant cache slots) |
| `src/services/preview_cache.rs` unit tests | 9 passed (time is injected — nothing sleeps) |
| `tests/screen_store_render_params_test.rs` | 10 passed |
| `tests_ha` | **90 passed**. `ruff` clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| **In a browser, on the VM** | **Done.** See *Session 29* |

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
merge it. **0.18.0 was the first full end-to-end run and it worked.**

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

- **The add-on version *is* the ghcr image tag.** For the ~15–20 minutes between merging and the
  publisher pushing the image, the add-on store advertises a version that does not exist yet. A
  user refreshing in that window gets a pull failure and succeeds on a retry. **Owner decision,
  deliberate**, in exchange for one PR per release.
- **The publisher's guard asks whether the GitHub *release* exists, not the tag.** As built, a
  failed publisher run is **re-runnable from the Actions UI**.
- **A non-404 failure when asking about the release stops the run.**
- **A cancelled release leaves its branch behind.** `create-release-pr.yml` clears a stale
  `release/v*` branch **unless it still has an open PR**.
- **The publisher fires on any push touching `Cargo.toml`** — a dependabot PR will start it.
- Both new guards were exercised for real on 2026-08-18 and both worked — but **each has run
  exactly once**.

---

# Changelog discipline

`CHANGES.md`'s `Unreleased` section has session 28's entry under **New**. Two standing rules:

- **User-facing only.** CI, tooling, version automation and dev process do not belong there.
- **Read the section as a set, not as a stream of appends.** Five sessions of appending once
  produced two entries that contradicted each other and two under the wrong heading.

---

# Queued work

| ID | What |
|---|---|
| — | **Push the branch + open a PR.** The only thing actually pending |
| — | **Check the device page on HA 2026.8.2** before trusting the camera-vs-image comparison |
| — | **Delete `RELEASE_TOKEN` and revoke the PAT** (carried from session 27) |
| — | Decide whether `tests_ha` should run in CI |
| — | `addon.py:29` picks the first `*_byonk` add-on; decide what to do when there are two |
| — | Prove *Preview measured colors* on a panel that has a calibration |
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
`#383838`–`#B8B8B0`, so mapping into it *should* darken — **but the unmarked half dithers against
that same measured palette**, so the gap comes from the mapping, not the inks. **Not diagnosed.**
Decide which half is the better preview before touching the mapper.

## Two TLS tests are flaky

`lua_https_tests::{test_https_with_custom_ca_cert, test_https_with_client_certificate}` fail with
`error sending request for url (https://127.0.0.1:…)`, the shape a 30 s timeout takes. Roughly
**once in six** full runs.

**The best explanation is the laptop suspending, not CPU contention.** A 30 s timeout is *wall
clock*. **Before spending anything on a fix, check whether a failing run coincides with a sleep**
(`pmset -g log | grep -i sleep`). The null hypothesis — reverting `c850ea7`'s HTTP change — has
still never been tested.

**If it needs fixing, do not loosen the test.** Cache the `reqwest::blocking::Client` instead of
building one per request. A single shared worker thread is the wrong answer: it would serialise
every screen's HTTP on the server path.

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

- **The preview is a camera, and the reason is the dialog size and the square thumbnail** — not
  full-width rendering, which does not happen. Settled by building both and looking.
- **Warn on any render-scale mismatch, no integer-zoom exemption.** Owner decision, re-confirmed
  by a positive control that fires on an exact 2×.
- **Authoring warnings reach the author, not the operator.** `tracing::warn!` reaches the server
  log, not the screen's author.
- **A variant CAN be aliased.** `HintingSpec::to_usvg()` drops `aliased` because it is
  document-level, but `text-rendering` is an inheritable SVG property. Always pair `optimizeSpeed`
  with mono hinting.
- **No bundled font carries a hinting program** (measured across every glyph). So `interpreter` is
  effectively unhinted and `auto ≡ auto_fallback`. The engine axis is **not** dead — it is what
  *shows* these facts.
- **Terminus is NOT buggy.** Terminus @14 and @18 render 1 px/glyph wider — that is correct.
  Raised twice, settled twice.
- **The fonts and the resvg pin must move together.** `test_bitmap_font_render` fails if the pin
  regresses.
- **Falsified, do not chase again:** X11 vertical-metric overflow (real malformation, causes
  nothing); ink overhang in oblique faces (normal); the fvar `wght` default does not leak; Source
  Serif 4 is not pinned at `opsz` 20; upstream will not change `AutoFallback`
  (googlefonts/fontations#1151, closed); `font-weight` does not disable hinting.

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~15–40 min on this machine — background it**; it
  runs `cargo fmt`, not `--check`, so it rewrites files.
- `make ha-check` = `ruff` + `pytest tests_ha`. Needs `make ha-setup` once. **Instant** (~2 s).
- **`cargo test` links each test binary serially and slowly here** (~1–2 min per binary, 40
  binaries). `cargo test --test <name>` for a single file is the fast loop.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`. Add by
  explicit path and check `git diff --cached` before committing.
- **Never run `make check` while the tree is being edited.** Also `make check > log; echo
  "EXIT=$?"` reports the *echo's* status. Same trap with any pipe: `cmd | tail; echo $?` reports
  `tail`. **This bites in background jobs too.**
- **A sleeping laptop looks exactly like a hung build.** `ps -eo pid,etime,command` reports *wall*
  time. An IDE `cargo check --workspace` also holds the build lock.
- **Editing an embedded asset forces a rebuild** (`api/lua-api.md`, `tutorial/svg-templates.md`,
  `guide/authoring.md`, `byonk-base/`, `screens/{builtin,examples}/`).
- **In a debug build rust-embed reads from disk at runtime**, so screen edits take effect with no
  rebuild — but "no change" is then indistinguishable from a stale binary, so **prove disk-backing
  with a visible sabotage first**.
- **Subagents must not run `make check`** — the 600 s watchdog kills them.
- **IDE diagnostics lie in this tree.** Only an actual cargo run counts.
- `make docs` = `mdbook build`. `docs/book/` and `docs/src/images/` are gitignored.

## Working the HA VM

See the `ha-vm-testing` skill. Beyond it:

- **`make ha-rebuild` does not sync the add-on manifest.** An options-schema change needs a manual
  version bump plus `POST /store/reload` and `ha addons update` on the VM.
- **Two byonk add-ons cannot coexist** — they both want port 3000, and the loser sits in state
  `error`. `ha addons` lists what is installed.
- **`ha core restart` returns long before HA is up.** Poll `http://localhost:8123/` for `200`.
- **Chrome on the Mac host holds a live HA session**, so the UI can be driven with the
  `claude-in-chrome` tools without any password. HA's frontend is shadow-DOM heavy: `find` and
  `read_page` return almost nothing, so **work from screenshots and coordinates**. Dialogs animate
  — screenshot again before clicking, or the click lands on nothing.
- **The device page jumps to the top** when a dialog closes; re-screenshot before the next click.

## Capturing every bundled screen, on every panel

```bash
BYONK_BIN=./target/debug/byonk ./tools/capture-renders.sh /path/to/out
```

Seconds, not minutes. **19 captures across 5 panels.** **Do not put the output in `/tmp`** — that
is how the previous baselines were lost. **A capture is only as wide as
`tools/capture-config.yaml`'s device map**; when adding a panel profile, add a device for it here
too.

## Testing the preview endpoint by hand

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  http://localhost:3000/api/admin/devices/DEFAULT/preview -o preview.png
curl -sD- -o /dev/null -H "Authorization: Bearer $TOKEN" \
  'http://localhost:3000/api/admin/devices/DEFAULT/preview?dither=off' | grep -i x-byonk
```

`X-Byonk-Preview` tells you whether it rendered or re-served. On the VM, curl from the **Mac
host** on port 3000, not from the Terminal add-on, and **never print the admin token**. A quick
liveness check that needs no token: the endpoint answers **401** when the route exists and
**404** when the binary is too old.

---

# Lessons — these keep paying off

- **A design premise about someone else's UI is a claim, not a fact.** "HA renders a camera
  full-width" survived a whole session of implementation and a written handover before anyone
  opened a browser. It was false. **Nothing about a rendered layout is verified until it has been
  looked at.**
- **When a premise falls, rebuild the alternative rather than reason about it.** The camera still
  won, but the honest reasons were only visible with both entities on the same page. It cost about
  twenty minutes.
- **Read the framework's source before designing around it.** "Does HA poll a camera?",
  "what does `frame_interval` default to?", and "why does an image entity show nothing?" were each
  settled in minutes from the package in `.venv/`.
- **A base class's `__init__` can undo your attribute.** `Camera.__init__` sets
  `content_type = "image/jpeg"` unconditionally. Neither `Camera` nor `ByonkDeviceEntity` calls
  `super().__init__()`, so both must be called explicitly.
- **An entity whose state is `None` renders as nothing, silently.** `ImageEntity` has no state
  until `image_last_updated` is stamped, and reports no error when it is missing.
- **A parameter slot doing double duty is a trap for the next caller.**
  `resolve_render_params`'s device-config dither slot was carrying the *override*.
- **Don't fabricate a default that reads as a measurement.** A preview showing 4.2 V for a device
  that has never reported its battery is worse than showing nothing.
- **Put view options in the cache key, not the fingerprint.**
- **Hash a `HashMap` through a `BTreeMap`.** Iteration order varies per process, so a fingerprint
  built from one is a cache that never hits — and it will look like the cache "just doesn't work".
- **A raw string ends at the first `"#`, and hex colours are full of them.** Use `r##"…"##`.
  Fourth time in this repo.
- **A mock rebound after the patch is applied does nothing.** Mutate the existing mock's
  `side_effect`.
- **Not every attribute is a state attribute.** `frame_interval` is read off the entity by
  `handle_async_still_stream`, not published in `state.attributes`.
- **Test the cache where time is injectable, test the wiring over HTTP.**
- **Demonstrate the check fails when the thing is broken.** A test written *after* the
  implementation has never been shown to fail; sabotage stands in for the RED step.
- **"No warnings" is not coverage until the mechanism has been shown to fire.**
- **A toggle that changes nothing visible has not been proven to work.** *Preview measured colors*
  looks identical on a panel with no calibration — which is correct behaviour and zero evidence.
- **Coverage that is wide in one dimension can be nil in another.**
- **Assert on the geometry, not the pixels, when the question is "is this legible".**
- **A CSS rule beats a presentation attribute in SVG.**
- **Fix the docs when they are the bug.** `docs/src/tutorial/svg-templates.md` has been the bug
  twice; it is embedded and served to LLM authors over MCP.
- **Work left by an agent that died is not verified work.**

---

# Carried forward

Session 28's handover — the preview's design in the form it was first written, before the
full-width claim fell — is in `git show fea552d:docs/HANDOVER.md`. Session 26's detail (the resvg
`byonk-base` initiative in full, the render sweep, the two calibration bugs, font licensing, the
whole hinting settlement) is in `git show bf48594~1:docs/HANDOVER.md` — **read it before touching
fonts, hinting or resvg**. Session 27's (the PR-based release) is in
`git show bf48594:docs/HANDOVER.md`. The pinning initiative's detail is in
`git show 3b32762:docs/HANDOVER.md` — read before touching `eink-dither`, gamut mapping or colour
models.

`git worktree list` is clean.
