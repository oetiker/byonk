# Color tuning in the Svelte dev UI

Date: 2026-09-10
Status: design, approved in brainstorm on 2026-09-10

## Goal

Give the dev UI a way to calibrate a panel profile's measured colors against
the physical panel, and to make that calibration the real configuration.

The old vanilla UI had a color-tuning popup (`static/dev/dev.js:1239-1350`).
The Svelte rewrite dropped it silently. This spec restores it, larger than it
was: the old popup could only ever produce an in-memory override that was lost
on restart, and the user copied the result out by hand.

## Amends

This is an amendment to **`2026-09-09-dev-ui-svelte-design.md`**, which is
binding for the rewrite. Two of its sections change:

- **§1 Layout** — "Four panels, four icons" becomes five. Every rule stated
  there holds unchanged, because the new panel docks exactly like `Files`.
- **§6 Component structure** — the four work areas become five, and the four
  rune modules become five.

It also carries one fix to a pre-existing defect that this feature would
otherwise newly expose (§6 below).

Nothing here changes how a screen renders. It changes only which measured
colors the renderer is given.

## Non-goals

- **Removing `colors_actual` from a panel.** `config_writer` has no
  scalar-remove helper, only `remove_device` / `remove_screen_repo`. Adding one
  is separate work. You can always tune back toward the official colors.
- **Per-screen and per-device color overrides.** `DeviceConfig.colors` and a
  script's own `colors_actual` keep their present meaning and their present
  place in the precedence chain.
- **Pushing to a device.** byonk cannot make a device fetch. The device polls;
  byonk answers. §3 makes the poll frequent, not immediate.
- **Tuning more than one device at a time from one browser.** The server model
  permits it (§3 keys sessions by device), but the UI drives one.

## 1 · Two stages, and why

**Stage 1 is a temporary override. Stage 2 is a durable write.**

E-ink is reflective. A panel shows the light in the room it hangs in, so a
calibration is only valid under that light. The browser preview tells you what
byonk *sends*; only the panel on its own wall tells you whether the numbers are
right. So tuning must be able to reach real hardware before it is committed —
and must be able to reach it *without* being committed, so a bad experiment
never becomes the configuration.

```
1. Tune: pick the device you are standing in front of
                                   -> session starts; that device is answered
                                      refresh_rate 60 (§3)
   the panel follows from the device where its config names one
2. click a swatch, drag H / S / L, or type a hex
                                   -> the preview re-renders immediately;
                                      no server call at all (§8)
3. [Push to panel]                 -> PUT the override; the panel repaints on
                                      its next poll, within 60s
4. walk over and look at it under its own light
5. repeat 2-4
6. [Save to config]                -> writes panels.<id>.colors_actual,
                                      reloads, clears the override, ends the
                                      session
   [Discard]                       -> clears the override, ends the session
   closing the Tune panel          -> clears the override, ends the session;
                                      the working copy stays client-side
```

**Push is a button, never automatic.** The old popup pushed on a 300 ms
debounce (`dev.js:1311-1340`). That is wrong now, because a push has a physical
side effect: it drives a battery-powered device into a 60-second refresh cycle.
Dragging a slider must not do that. One click per iteration costs nothing
against a 60-second loop.

**Closing the panel clears the server-side override deliberately.** An override
that nobody is watching is a trap: it silently changes what every device using
that panel renders, with nothing on screen to say so. The working copy lives in
the `tune` rune module, not in the component, so it survives the unmount and can
be pushed again on reopen.

## 2 · Layout — the fifth panel

`Tune` docks as a fixed-width column **between the editor and the preview**.

```
+--+------+--------+----------+----------+
|# |FILES | EDITOR | TUNE     | PREVIEW  |
|# | depa | 1 --   | Panel  v |  +----+  |
|# | weat | 2 loc  | Device v |  |dev |  |
|# | hell | 3 ret  | ######   |  +----+  |
|# |      |        | H --o--  +----------+
|# |      |        | S --o--  | SIMULATE |
|  |      |        | L --o--  | Panel  v |
|  |      |        | #00994D  | Dither v |
|  |      |        | [Push]   |          |
|  |      |        | [Save]   |          |
+--+------+--------+----------+----------+
```

It sits next to the preview because that is what your eye compares: the swatch
you are dragging against the render it produces.

**It costs the preview width, never height.** §1 of the parent spec argues at
length that height is the scarce dimension, because an upright E1004 is
1200x1600 and a fixed control block under it squeezes the device to a stamp.
A column takes the other axis.

Concretely, in `AppShell.svelte` the `b` snippet of the horizontal `SplitPane`
becomes a flex row holding the optional Tune column followed by the existing
vertical `SplitPane`. The horizontal splitter's `open` condition gains `tune`:

    open={layout.isOpen("preview") || layout.isOpen("simulate") || layout.isOpen("tune")}

`PANELS` in `layout.svelte.js` becomes `["files", "editor", "tune", "preview",
"simulate"]` — icon-rail order is layout order, and the persisted record is
keyed by name, so reordering the array does not invalidate stored layouts.
`DEFAULTS.panels` gains `tune: false`: the other four default open, but a panel
whose whole purpose is calibration should not occupy width until it is asked
for.

`IconRail` needs one entry in each of its two maps. Its markup is already
generic over `PANELS` and needs no change. The icon is lucide `Palette`, which
exists in the pinned `@lucide/svelte` 1.42.0 — **do not move that pin** (parent
spec, and the branch's standing constraint).

Every §1 rule holds unchanged. Every combination of five is legal, including
none. A hidden panel gives its width to its neighbours and gets it back at the
size it had. A splitter exists only while both neighbours are open.

## 3 · The tuning session

**A tuning session holds one device at a 60-second refresh so that iterations
are fast.**

byonk cannot push. The device asks, and byonk answers with a `refresh_rate`
resolved by `resolve_refresh_rate` (`src/services/content_pipeline.rs:27`) as
script > device override > screen default. Left alone, a calibration loop runs
at whatever cadence the screen chose, which is minutes.

Sixty seconds, not less: a six-color panel needs most of a minute to repaint,
and you want time to look at the result before it goes again.

**State.** `DevOverrides` (`src/server.rs:37`) gains:

    pub tuning_sessions: Arc<RwLock<HashMap<String /* device key */, Instant>>>,

**Lifetime.** The entry's `Instant` is `last_touched`. It is set when the
session starts and refreshed by every override push and by a save. The session
ends when:

- the client ends it — `Save`, `Discard`, deselecting the device, or closing
  the Tune panel; **and**
- as a backstop, when `last_touched.elapsed()` exceeds **30 minutes**.

The backstop is not optional. A closed lid or a crashed tab must not leave a
battery-powered device repainting every minute until byonk restarts.

**Expiry is lazy.** `/api/display` checks the map for the requesting device; if
the entry is present and fresh it answers 60, and if it is present and stale it
removes it and behaves normally. There is no background task, so there is
nothing to schedule, leak or shut down.

**This deliberately overrides a stated rule.** `content_pipeline.rs` says the
screen wins "because only it knows when its content next changes". During a
tuning session that is not true — the thing changing the content is a person
with a slider. So the override is narrow (one device, bounded time) and it is
logged at `info` with the reason, so a surprising 60-second cadence is
explainable from the log alone.

## 4 · Server: the color override, re-keyed

Today `POST /dev/api/panel-colors` takes `{device, colors_actual}` and stores it
in `DevOverrides.panel_colors`, keyed by **device config key**
(`src/api/dev.rs:351-383`). `/api/display` reads it at
`src/api/display.rs:813`, also by device key.

**Re-key it to the panel.** Tuning is a property of a panel profile, not of one
device: two devices with the same panel share the same measured colors. This is
an API change, which the project's standing guidance permits. Note the existing
route already spells the path parameter `panel` while the handler binds it to
`device` (`src/api/dev.rs:868-869`) — the rename makes the code agree with the
URL.

`src/api/display.rs:788-806` already resolves the panel through three branches
(device config, `Board` header, `Model` header) and builds a `panel_source`
label from the panel's config key in each. It gains a `panel_key:
Option<String>` carrying that key on its own, and the lookup at line 813 uses
it.

The override keeps its present position in the measured-colors precedence
chain — `SRC_DEV_OVERRIDE` stays first, ahead of `panel.colors_actual` and the
`Measured-Colors` header. Only the key changes.

## 5 · Server: the durable write

**`Save` writes `panels.<id>.colors_actual` into `config.yaml`.**

`config_writer::set_scalar` (`src/services/config_writer.rs:64`) is exactly the
right tool: it is comment-preserving, it replaces the key in place when it
exists, and it adds it to the parent mapping when it does not — which is the
common case, since `colors_actual` is optional.

    set_scalar(&yaml, &["panels", panel_id, "colors_actual"], value)

The write goes through the existing `persist` helper
(`src/api/admin/write.rs:423`): atomic temp-file write plus rename, then
`reload_config`, and on a reload failure it restores the previous file and
returns an error. **`persist` must move** out of `api/admin/write.rs` so both
callers share it; it is private today.

The handler takes `state.write_lock` and calls `require_file_config` for the
same reasons the admin writers do — an embedded, read-only config cannot be
written, and concurrent writers would interleave.

**It is not blocked in add-on mode.** `require_writable_global`
(`src/api/admin/write.rs:70`) makes global config read-only under the Home
Assistant add-on because the add-on Options form is its sole editor. That
argument does not reach `panels`: the Options schema
(`homeassistant/byonk/config.yaml:42`) carries `admin_token`, `log_level`,
`auth_mode`, `screen_repo_refresh_interval`, `dev_username`, `dev_password` and
`screen_repos`, and no panel key at all. Nothing else owns `panels`, so the
guard does not apply. This matters: the dev UI reached from the HA sidebar is
the main way this feature will be used.

After a successful save the panel list the client holds is stale, so the client
re-fetches its metadata (§8).

## 6 · Server: the add-on reload fix

**`reload_config` must re-apply the add-on options overlay.**

`apply_to_config` (`src/addon_options.rs:128`) folds `/data/options.json` onto
the parsed config, and it is called **once, at startup**
(`src/main.rs:889`). `reload_config` (`src/server.rs:249`) reparses
`config.yaml` and swaps it in without re-applying it. So under the add-on, any
config write resets `screen_repos`, `auth_mode` and
`screen_repo_refresh_interval` to the file's values and discards what the
Options form set, until the next restart.

This is pre-existing — device writes already do it, and the HA integration
writes devices. It is fixed here rather than left, because `Save to config`
would otherwise ship a way to clear a user's screen-repo registry from the HA
sidebar.

The fix needs no new state. `options_path()` (`src/addon_options.rs:67`) already
resolves the path including its env override, and `read_options` returns
`Missing` for a non-add-on run, for which `apply_to_config` is a no-op:

    let mut fresh = AppConfig::load_from_assets(&state.asset_loader)?;
    let opts = addon_options::read_options(&addon_options::options_path());
    addon_options::apply_to_config(&opts, &mut fresh);
    state.config.store(Arc::new(fresh));

`state.admin_token` is unaffected either way — it is resolved once into an
`AppState` field (`src/server.rs:187`) and is not re-read per request.

## 7 · HTTP surface

All under `/dev/api/`, all behind the existing dev login.

| Method | Path | Body | Does |
|---|---|---|---|
| `PUT` | `panel-colors/{panel}` | `{colors_actual}` | set the temporary override; refresh `last_touched` |
| `DELETE` | `panel-colors/{panel}` | — | clear the temporary override |
| `POST` | `panel-colors/{panel}/save` | `{colors_actual}` | write `config.yaml`, reload, clear the override |
| `PUT` | `tuning-session/{device}` | — | start or refresh a session |
| `DELETE` | `tuning-session/{device}` | — | end a session |

`POST /dev/api/panel-colors` and `DELETE /dev/api/panel-colors/{panel}` in
their present device-keyed form are **removed**, together with the
`PanelColorsRequest.device` field.

Errors follow the dev API's existing shape. A save against an unknown panel id
is `404`; against an embedded config, `409`, with the message naming
`CONFIG_FILE` as the admin writers' does.

## 8 · Client: `TunePane` and the `tune` store

**The preview needs no new server call.** `simulate.url()` already puts
`colors_actual` on the render query whenever a panel is chosen
(`webui/src/lib/state/simulate.svelte.js:203-209`), and the server honours it.
The Tune panel's working copy feeds that query, so dragging a slider re-renders
the preview immediately and touches no server state. This is the reason stage 1
and stage 2 can be separate at all.

**How the working copy reaches the query, exactly.** `simulate` gains one
nullable field, `colors_actual_override`, and `url()` prefers it over
`#selectedPanel().colors_actual` when it is set *and* names the panel the query
already carries. `tune` writes that field; `simulate` never imports `tune`.

The direction matters. `tune` must read `simulate` to default its panel
selector, so the reverse import would close a cycle between two rune modules
that are both evaluated at page load. Parking the field on `simulate` keeps one
direction, and follows the precedent already set for `simulate.error`: the
field lives on the store the render path reads, not on the store that
conceptually owns it.

The alternative — mutating `colors_actual` on the object inside
`screens.meta.panels`, which is what the old UI did to its `<option>`
dataset — is rejected. It edits shared metadata that other components read as
the configured truth, so a tuning session in progress would make the Simulate
panel's swatches lie about what is in `config.yaml`.

**`TunePane.svelte`** holds, top to bottom:

- a **panel** selector, from `screens.meta.panels`, defaulting to the panel
  `SimulateControls` has chosen;
- a **device** selector, from `screens.meta.devices`. Selecting a device starts
  the session (§3) and, where `DeviceInfo.panel` is set, sets the panel too;
- the **swatch row** for the working copy, one swatch selected;
- **H / S / L sliders and a hex field** for the selected swatch, ported from
  `dev.js:1239-1305`, with the `hexToHsl` / `hslToHex` helpers at
  `dev.js:1227-1237`;
- an **original / live** comparison pair, so the change is visible as a
  difference and not only as an absolute;
- **`[Push to panel]`**, **`[Save to config]`**, **`[Discard]`**.

`DeviceInfo.panel` is `Option<String>` (`src/api/dev.rs:107`), because a
device may resolve its panel from the firmware `Board` header at runtime rather
than from config. When it is absent the device selector cannot pre-fill the
panel and the user picks it; the UI says so rather than guessing.

**The `tune` rune module** is the fifth, alongside `screens`, `editor`,
`simulate` and `layout`. It holds: the panel id, the device key, the
working copy of `colors_actual` (mirrored into
`simulate.colors_actual_override` on every change), the selected swatch index,
the config-side baseline it is compared against, whether an override is
currently pushed, and any error from the three calls. It is a module and not component state precisely so that
closing the panel does not lose a half-finished calibration.

**An active override must be visible from outside the Tune panel.** It changes
what every real device using that panel renders. `StatusBar` shows that an
override is live and for which panel, next to the validity and watch
indicators.

After a successful save, the client re-fetches screen metadata so
`PanelInfo.colors_actual` (`src/api/dev.rs:116-123`) reflects the written value
and the baseline stops reading as changed.

## 9 · Behaviour that must not regress

- **Every URL the client builds stays relative.** The dev UI is served under
  Home Assistant Ingress at an unpredictable prefix. `api/panel-colors/...`,
  never `/dev/api/panel-colors/...`.
- **The measured-colors precedence chain keeps its order and its length rule.**
  Re-keying the override moves nothing in the chain.
- **A device with no tuning session is answered exactly as before.** The 60
  second answer is reachable only through a fresh session entry.
- **The dev UI stays invisible when dev is unconfigured.** These routes mount
  inside the existing dev router and inherit `require_dev`.
- **No nested page route under `/dev`.** These are API routes; `require_dev`
  redirects to the relative string `"login"` and hard-codes depth 1.
- **The build still emits exactly `index.html`, `app.js`, `app.css`.** No new
  component may ship an asset file.
- **No dependency pin moves**, and no new dependency is added.

## 10 · Testing

Server:

- a device inherits its panel's override — set for panel P, request
  `/api/display` as a device that resolves to P by device config, by `Board`
  header, and by `Model` header;
- a device that resolves to a different panel is unaffected;
- a fresh session yields `refresh_rate == 60`; an entry older than 30 minutes
  yields the normal rate and is removed; no entry yields the normal rate;
- `DELETE tuning-session` restores the normal rate immediately;
- `set_scalar` writes `panels.<id>.colors_actual` into a `config.yaml` that
  carries comments, both when the key exists and when it does not, leaving
  every comment in place;
- a save whose result fails to reparse rolls the file back and returns an
  error;
- a save against an embedded config returns `409`, and against an unknown panel
  `404`;
- `reload_config` in add-on mode preserves `screen_repos`, `auth_mode` and
  `screen_repo_refresh_interval` from `options.json` — the regression test for
  §6, written against a temp `options.json` via the `options_path()` env
  override.

Client (vitest + jsdom, per the parent spec §7):

- the working copy drives `simulate.url()`'s `colors_actual`, so a slider move
  changes the render URL and issues no fetch;
- `simulate.colors_actual_override` is ignored when it names a different panel
  than the query carries, and clearing it restores the configured
  `colors_actual` in the render URL;
- `Push` issues exactly one `PUT` and marks the override live; `Discard` issues
  the `DELETE` and clears it;
- closing the Tune panel clears the override and ends the session, and the
  working copy survives;
- selecting a device starts a session; deselecting ends it;
- `Save` re-fetches metadata and the baseline stops reading as changed;
- each of the three calls failing surfaces an error rather than an unhandled
  rejection;
- `layout` persists and restores `tune` alongside the other four.

## 11 · Risks and open points

- **The forced 60-second refresh is the one place the dev UI steers
  `/api/display`.** It is bounded to one device and 30 minutes and it is
  logged, but it is a real coupling between a dev-mode surface and the device
  protocol. If it ever grows a second reason to exist, it wants its own
  concept rather than another special case.
- **A tuning session costs battery.** Sixty seconds on a six-color panel is
  close to the most expensive thing byonk can ask of a device. The 30-minute
  backstop bounds an accident; it does not make a long deliberate session
  cheap.
- **`colors_actual` cannot be removed from a panel** through this UI — see
  Non-goals.
- **The re-key is a breaking API change** to `/dev/api/panel-colors`. Nothing
  outside the dev UI calls it, and the old UI it served is deleted by this
  branch.
- **`reload_config` re-reading `options.json` means a save picks up Options
  edits made since startup.** That is a behaviour change, and the right one:
  the alternative is re-applying a snapshot that may itself be stale.
