# Handover — Byonk

_Last updated: 2026-08-19 (session 31). The **app-installs-its-own-integration** initiative is
**implemented, reviewed and validated end to end on the QEMU HAOS VM**. All nine plan tasks are
done. **Nothing is pushed and no PR exists.**_

## Where the work lives

| | |
|---|---|
| Branch | `feat/addon-installs-integration`, rebased onto `origin/main` @ `36e2384` (the #40 merge) |
| HEAD | `41a1b3b` — 18 commits, tree clean, **NOT pushed** |
| `main` | `36e2384`. Latest tag **`v0.18.0`** |
| Open PRs | none |
| Push gotcha | ssh-agent holds **no identities**, so `git push origin …` fails on publickey. `gh` is authenticated over HTTPS — `git push https://github.com/oetiker/byonk.git <branch>` works and leaves the remote config alone |
| `main` protection | Ruleset `main-protect`: PR required (0 approvals), 5 required checks (`Build`, `Test`, `Check & Lint`, `Analyze (actions)`, `Analyze (rust)`), strict up-to-date. Bypass: **Repository admin** — the owner can always merge, no PAT can |

---

# What this branch does

**A Home Assistant user installs one thing — the Byonk app — and Byonk works.**

At startup in add-on mode the app copies `custom_components/byonk` out of its own image into the
Home Assistant config directory (`/homeassistant`), posts a "restart Home Assistant" persistent
notification, and posts a Supervisor discovery message so a **Byonk** card is waiting after the
restart. HACS is removed from the project entirely.

Spec: `docs/superpowers/specs/2026-08-19-addon-installs-integration-design.md`
Plan: `docs/superpowers/plans/2026-08-19-addon-installs-integration.md`

## The shape of it, in code

| Piece | Where |
|---|---|
| The writer | `src/ha_integration.rs` — `install()`, `notify_restart()`, `announce_discovery()`, `install_and_announce()` |
| The call site | `src/main.rs:896-903`, gated on `addon_mode`, `tokio::spawn`ed so it can never delay the listener |
| Manifest capabilities | `homeassistant/byonk/config.yaml` — `homeassistant_config:rw`, `homeassistant_api: true`, `discovery: [byonk]` |
| The image | `Dockerfile.release` — `COPY custom_components/byonk ./custom_components/byonk` on **both** arch stages |
| The Discovered card | `custom_components/byonk/config_flow.py` — `async_step_hassio` / `async_step_hassio_confirm`, sharing `_async_create_hub_entry` with the manual route |
| The version warning | `coordinator.py::_async_check_version` + `addon.py::async_get_addon_version` |

---

# What to do next

**1. Open the PR.** The branch is finished, reviewed and hardware-validated. Nothing is blocking
it.

**2. Two decisions the owner should make** (below): whether `tests_ha` should run in CI, and
whether to notify the user when the install is *refused*.

**3. After merge**, the release path is unchanged — Actions → *Create release PR* → pick the
type → review → merge.

---

# Validated on the VM — do not re-run these to confirm

All nine checklist items pass, on HA core 2026.7.2 with the app built from source. This is what
the branch actually does, observed, not inferred.

| # | Result |
|---|---|
| 1 | Supervisor accepted the new manifest — app installed and started, no schema error |
| 2 | `/homeassistant/custom_components/byonk` appeared on first start, all files incl. `brand/` and `translations/`, manifest version matching the app, no staging dirs left behind |
| 3 | Notification: *"Byonk installed its Home Assistant integration (0.18.0). Restart Home Assistant to finish setting up Byonk."* |
| 4 | After the HA restart, a **Byonk** Discovered card appeared, with byonk's own icon — the in-repo `brand/` directory works |
| 5 | The card opened the `hassio_confirm` form; **Submit** created the hub entry with no token prompt |
| 6 | Hub and **Byonk Default** entities all present |
| 7 | Restarting the app alone: no second write, no second notification, entries healthy |
| 8 | A planted foreign `custom_components/byonk` was **refused** with a warning and left untouched, marker file intact; removing it self-healed on the next app start |
| 9 | An older installed version produced *"updated from 0.17.0 to 0.18.0"* (replacing, not stacking); the version-mismatch repair issue appeared with correct placeholders and **cleared** once the versions agreed |

**Limitation, stated honestly:** the VM builds from source with its own
`/addons/byonk/Dockerfile`, so this validates the manifest keys, the mount, the write, the
notification, discovery and the card — but **not `Dockerfile.release` itself**. Only a
published-image install exercises that, which is post-release.

## Two traps in the VM rig, both now fixed in-repo

- **`tools/ha-vm/rebuild.sh` did not sync `custom_components`**, so a from-source app had no
  `/app/custom_components/byonk` and `install` failed on every start — *quietly*, because
  `make ha-deploy` writes the directory by hand and the VM looks healthy. Fixed; `custom_components`
  is now in the sync list.
- **That fix is necessary but not sufficient, and this will bite a fresh VM.** The VM's local
  add-on does **not** build from `Dockerfile.release` — it builds from `/addons/byonk/Dockerfile`,
  which lives only inside the VM and is **not tracked in this repo**. It has no
  `COPY custom_components …` line. On this VM it was added by hand during Task 9:

  ```dockerfile
  COPY --from=builder /build/custom_components/byonk /app/custom_components/byonk
  ```

  Without it, `make ha-rebuild` still produces an app with no integration and `install` logs
  "no readable manifest.json" on every start, with nothing else looking wrong. The same is true
  of `/addons/byonk/config.yaml`, which is also VM-only and needed the three new manifest keys
  added by hand.
- **`make ha-rebuild` still does not sync the app manifest.** A change to
  `homeassistant/byonk/config.yaml` needs a manual version bump in `/addons/byonk/config.yaml`,
  then `ha store reload`, then `ha addons update local_byonk`. **`ha store reload` is the working
  command — `ha addons reload` silently leaves `version_latest` stale.**

Two smaller VM facts worth keeping:

- `make ha-ssh CMD="a; b"` runs only the first command remotely and the rest **on the Mac** (the
  Makefile does not quote `$(CMD)`). Use `bash tools/ha-vm/ssh.sh "…"`.
- A persistent notification does **not** survive the HA restart it asks for — they are in-memory.
  Harmless, since the restart is the action it wanted.

---

# Decisions taken during execution that the owner may want to revisit

Twelve rulings were made while running the plan; these three changed behaviour and are worth
knowing about. The full list is in the session's final message.

1. **The ownership guard runs before the version check** in `install()`. The plan had it the
   other way round, which meant a foreign `custom_components/byonk` carrying byonk's *own*
   version string was silently treated as "already installed" — no refusal, no warning. The spec
   makes ownership the gate. Pinned by `refuses_a_foreign_directory_even_when_its_version_string_matches_ours`,
   and exercised for real as VM checklist item 8.
2. **The swap is rename-based**, not delete-then-rename. `remove_dir_all` is not atomic over a
   multi-file directory; a partial failure left the target half-destroyed *and* permanently
   wedged, because the remains no longer carried a `manifest.json` naming `domain: byonk`, so
   every later start hit the ownership guard and refused forever. Now: stage into `.byonk-new`,
   rename the old target aside to `.byonk-old`, rename staging into place, restore on failure,
   delete the backup last. A crash between the renames leaves the target simply **absent**, so
   the next start installs fresh.
3. **The integration's own version is read through the loader**, never from `manifest.json` on
   disk — the app has already overwritten that file, so a disk read would make the two numbers
   always agree and the warning could never fire. `test_compares_against_the_loaded_integration_not_the_manifest_file`
   pins it, and was falsified (made to fail against a disk-reading implementation) before being
   accepted.

---

# Open items the owner should decide

## Should `tests_ha` run in CI?

It does not today. Task 7 removed the `hacs/action` job, so **`hassfest` is now the only
integration check in CI**, and the 98 Python tests plus ruff run only under `make ha-check`
locally. The stakes went up with this branch: the integration is now baked into the released app
image, so a Python regression ships inside the server. Carried since session 30; worth settling
now.

## Should a refused install tell the user?

`install_and_announce` announces discovery on **every** start, including when the install was
`Refused` or `Failed`. That is what the spec asks for, but it means a user with a foreign
`custom_components/byonk` in the way, or a read-only config dir, gets a **Byonk** Discovered card
with nothing working behind it — and the only trace is a `tracing::warn!` in the app log. The
notification channel is already open two lines below; a refusal notification would cost almost
nothing and turn a silent dead end into a readable one.

## Smaller things the final review left open, deliberately

- `install()`'s scratch-directory cleanup sits *behind* the version short-circuit, so a
  `.byonk-new`/`.byonk-old` left by a killed container is only cleared on a start that also
  writes. Self-healing whenever versions differ; one `if` away from unconditional.
- `_async_check_version` runs inside the coordinator's `_async_update_data`, so every 60 s
  refresh adds a Supervisor round-trip, and an exception that is neither `AddonError` nor
  `KeyError` would fail the whole data update. Both known failure modes are handled.
- `reqwest::Client::new()` sets no timeout, so a wedged Supervisor leaks one detached task.
- The merged guide documents first install only — it never describes what an app *update* looks
  like, though that is when the repair issue and the update notification appear.
- `docs/src/guide/home-assistant.md` hard-codes "before version 0.19.0" while the tree is at
  0.18.0. Correct only if the next release is 0.19.0.
- The guide asserts that HACS's removal deletes the integration files from disk. That is
  standard HACS behaviour but was **not observed** here. The instructions are safe either way —
  restarting the app before restarting Home Assistant is correct whether or not HACS deletes.

---

# Queued work (carried forward)

| ID | What |
|---|---|
| — | **Delete the `RELEASE_TOKEN` secret and revoke the PAT.** `gh secret list` still shows it (created 2026-07-17). 0.18.0 released without it. Carried since session 27 |
| — | `addon.py:29` picks the first `*_byonk` app; decide what "the byonk app" means when there are two. A wrongly-bound entry **cannot heal itself** — reauth re-reads the stored slug and never re-discovers |
| — | Prove *Preview measured colors* on a panel that **has** a calibration. On `DEFAULT` (grey, uncalibrated) measured and spec palettes are identical, so nothing changes |
| — | Check the device page on **HA 2026.8.x**; the camera-vs-image comparison rests on 2026.7.2's layout |
| F13 | Extend `screens/examples/demo/font/{ttf,bitmap,hinting}/` to cover Source |
| F14 | Licence + notice files. **`FONTS.md`'s "X11LuType is proportional" is wrong — it is monospaced** |
| F22 | Cosmetic: the WiFi glyph reads as a caret at 8×12. Redraw or drop it |
| F23 | The two fetching examples fail in a sandbox with `Cannot drop a runtime…` *from the fetch error path*; check whether any other blocking call in `lua_runtime.rs` shares the hazard |
| F24 | `/dev/render` shows the author nothing but an image — it passes `None` for the script log sink, so neither their `log_*` output nor byonk's authoring warnings reach the browser preview |
| Plan Task 11 | The hinted-font-trio **decision** (specimens + recommendation) |

---

# Build / verify

- `make check` = fmt + clippy + full suite, **~15–40 min here — background it**; it runs
  `cargo fmt`, not `--check`, so it rewrites files.
- `make ha-check` = `ruff` + `pytest tests_ha`. **Instant** (~2 s), 98 tests.
- **`make check > log; echo "EXIT=$?"` reports the *echo's* status.** This bit for real this
  session: a background wrapper reported "exit code 0" while clippy had failed with 101. Log the
  exit code inside the file and read it.
- **`cargo test` links each test binary serially and slowly here.** `cargo test --test <name>` is
  the fast loop.
- **Never `git add -A`.** `examples/` is an untracked near-copy of `screens/examples/`.
- **Subagents must not run `make check`** — the 600 s watchdog kills them. They also must not
  background anything: three agents this session stalled waiting on a background job that never
  woke them.
- **`clippy::await_holding_lock` is deny-level here.** A `std::sync::MutexGuard` held across
  `.await` will not compile; use `tokio::sync::Mutex`.
- **A sleeping laptop looks exactly like a hung build.**
- **IDE diagnostics lie in this tree.** Only an actual cargo run counts.
- `make docs` = `mdbook build`. `docs/book/` and `docs/src/images/` are gitignored.

## Changelog discipline

- **User-facing only.** CI, tooling, version automation and dev process do not belong in
  `CHANGES.md`.
- **Read `Unreleased` as a set, not a stream of appends.**

## The release process

Two workflows; **nothing pushes to `main`**, so no PAT is involved.

| Workflow | Trigger | Does |
|---|---|---|
| `create-release-pr.yml` | `workflow_dispatch` + bugfix/feature/major | waits for CI green on the exact commit, computes the version from tags, bumps everything, opens a `release/vX.Y.Z` PR. **Nothing tagged or published** |
| `release-publisher.yml` | `push` to `main` touching `Cargo.toml` | tags, builds 5 binaries, builds and pushes the container, publishes the release, deploys docs |

- **The app version *is* the ghcr image tag.** For ~15–20 minutes between merge and publish the
  app store advertises a version that does not exist. **Deliberate owner decision.**
- The publisher's guard asks whether the GitHub **release** exists, not the tag, so a failed run
  is **re-runnable from the Actions UI**.
- **The publisher fires on any push touching `Cargo.toml`** — a dependabot PR will start it.
- The release guard already fails unless `Cargo.toml`, `custom_components/byonk/manifest.json`
  and `homeassistant/byonk/config.yaml` agree on the version, so **the image can never ship an
  integration whose version disagrees with the app** — which is exactly what the repair issue
  compares.

---

# Settled — do not reopen

- **HACS is gone.** PR #9310 closed by the owner on 2026-08-19; the app is byonk's distribution
  channel. The HACS lore is history.
- **`home-assistant/brands` is obsolete for custom integrations.** Since HA 2026.3 the
  integration ships its own `brand/` directory, which beats the CDN. Confirmed working on the VM
  this session — the Discovered card carries byonk's own icon.
- **The Discovered card's button is labelled "Add", not "Configure".** Verified on screen; the
  docs said "Configure" and were wrong in five places, now fixed.
- **The device preview is a `camera`**, and the reason is the more-info dialog size and the
  square thumbnail — *not* full-width rendering, which does not happen.
- **The Lua sandbox withholds `io`, `os.execute` and `os.exit`.** Checked again this session from
  the other direction: the new `homeassistant_api: true` and `homeassistant_config:rw`
  privileges are **not reachable from screen Lua**, because `os.getenv` and `io` are both denied.
- **Warn on any render-scale mismatch, no integer-zoom exemption.**
- **Authoring warnings reach the author, not the operator.**
- **A variant CAN be aliased.** Always pair `optimizeSpeed` with mono hinting.
- **No bundled font carries a hinting program**, so `interpreter` is effectively unhinted.
- **Terminus is NOT buggy.** Raised twice, settled twice.
- **The fonts and the resvg pin must move together.**
- **Falsified, do not chase again:** X11 vertical-metric overflow; ink overhang in oblique faces;
  the fvar `wght` default does not leak; Source Serif 4 is not pinned at `opsz` 20; upstream will
  not change `AutoFallback`; `font-weight` does not disable hinting.

---

# Open items carried forward, unrelated to this branch

## Marking costs shadow detail at 16 grey levels

On `calibration/tone` the marked half should show what gamut mapping buys. **On `trmnl_x` the
marked half is markedly darker and loses shadow separation** the unmarked half keeps — and the
unmarked half dithers against that same measured palette, so the gap comes from the mapping, not
the inks. **Not diagnosed.** Decide which half is the better preview before touching the mapper.

## Two TLS tests are flaky — and the sleep hypothesis is now FALSIFIED

`lua_https_tests::{test_https_with_custom_ca_cert, test_https_with_client_certificate}` fail with
`error sending request for url (https://127.0.0.1:…)` — the shape a 30 s timeout takes. Roughly
**one full run in six**. They failed again in this session's full `make check`.

**The standing explanation — "the laptop suspending, not CPU contention" — is wrong.** The check
this handover has been prescribing for several sessions was finally run against a real failing
run: `pmset -g log` shows **zero sleep or wake transitions** in the hours around it (the last was
19:04, the run ended 23:05), and pmset reports `caffeinate` was actively preventing sleep
throughout. The machine was meanwhile running the QEMU HAOS VM, several subagents and a docs
build at once. **So contention — the explanation previously dismissed — is the surviving one.**
Re-running the binary alone on a quiet machine: 8 passed, 0 failed.

**If it needs fixing, do not loosen the test:** cache the `reqwest::blocking::Client` instead of
building one per request. That prescription is unchanged, and now better motivated. A single
shared worker thread is still the wrong answer — it would serialise every screen's HTTP on the
server path.

## Three carried-forward questions

1. **`grey_count <= 2` may be the wrong rule.** At 10–12 px mono+aliased beats smooth, but at
   14 px smooth wins — the fix may be a **size term**. *Always name panels by config key:* the
   **4-colour** `trmnl_og_4clr` already counts as `grey_count = 2`; it is **4-grey** `trmnl_og`
   that is in question, and they behave oppositely.
2. **`HintingMode::Light` is byte-identical to `Normal`** — one genuinely inert knob.
3. `for_error_diffusion()` is applied to **every** dither, so HyAB and its `kchroma = 10` tuning
   are not on the crate's dithering path at all.

---

# Lessons — these keep paying off

**New this session:**

- **A premise about someone else's UI is a claim until you look.** The docs told users to select
  **Configure** on the Discovered card, through a spec, a plan, an implementation and two
  reviews. Home Assistant renders **Add**. One screenshot settled it.
- **Test the upgrade path you actively instruct.** The final review caught that our own
  HACS-removal instructions — remove from HACS, restart Home Assistant — would leave a working
  install broken, because the app writes the integration only when *the app* starts. Every
  checklist item passed; the one path nobody walked was the one the docs recommend.
- **A trailing `echo` eats the exit code.** `make check > log 2>&1; echo "EXIT=$?"` in a
  background job reported success while clippy had failed with 101 — and again later while the
  suite had failed with 2. Log the status *into* the file and read it back. It bit twice in one
  session.
- **Run the diagnostic the last session left you.** The "laptop suspending" explanation for the
  flaky TLS tests survived several handovers because nobody spent the two minutes on
  `pmset -g log`. It was wrong.
- **A fix that reorders a guard needs a test that fails under the old order.** Ruling 6's test
  plants a foreign directory carrying byonk's *own* version — the one case the old ordering got
  wrong. It then caught the same thing for real on the VM.
- **When an implementer flags a deviation, that is the process working.** Every escalation this
  session was correct, and the one finding that mattered in Task 8 was the deviation that was
  *not* disclosed.

**Standing:**

- **When a premise falls, rebuild the alternative rather than reason about it.**
- **A base class's `__init__` can undo your attribute.**
- **A parameter slot doing double duty is a trap for the next caller.**
- **Don't fabricate a default that reads as a measurement.**
- **Put view options in the cache key, not the fingerprint.**
- **Hash a `HashMap` through a `BTreeMap`.**
- **A raw string ends at the first `"#`, and hex colours are full of them.** Use `r##"…"##`.
- **A mock rebound after the patch is applied does nothing.**
- **Demonstrate the check fails when the thing is broken.** A test written *after* the
  implementation has never been shown to fail; sabotage stands in for the RED step.
- **"No warnings" is not coverage until the mechanism has been shown to fire.**
- **Assert on the geometry, not the pixels, when the question is "is this legible".**
- **A CSS rule beats a presentation attribute in SVG.**
- **Fix the docs when they are the bug.**
- **Work left by an agent that died is not verified work.**

---

# Carried forward

Session 30's handover — the spec and plan for this initiative, and the upstream Supervisor facts
they rest on — is in `git show 48befad:docs/HANDOVER.md`. Those facts (where each mapping mounts,
what the security rating scores, how discovery dedupes and replays) were all confirmed correct in
practice this session.

Session 29's (the screen preview, the camera-vs-image comparison, `PreviewCache`) is in
`git show bdb9473:docs/HANDOVER.md` — **read it before touching `camera.py`, `preview_cache.rs` or
`ScreenStore::render`**, in particular its warning not to pass a device's configured dither as
`RenderOpts::dither`.

Session 26's (the resvg `byonk-base` initiative, the render sweep, font licensing, the hinting
settlement) is in `git show bf48594~1:docs/HANDOVER.md` — **read it before touching fonts, hinting
or resvg**. Session 27's (the PR-based release) is in `git show bf48594:docs/HANDOVER.md`. The
pinning initiative's detail is in `git show 3b32762:docs/HANDOVER.md`. Keep the branch
`docs/handover-session-27` @ `bf48594`; those two references live on it.
