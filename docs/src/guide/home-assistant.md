# Byonk in Home Assistant

Byonk runs as a Home Assistant app: the same prebuilt `ghcr.io/oetiker/byonk`
image, storing its configuration in a persistent, editable folder and exposing
Byonk on a host port so your TRMNL devices can reach it directly on your LAN.
It brings its own integration with it — device onboarding, entities, and
automatic token provisioning — so there is nothing separate to install.

> Requires a Supervisor-managed install (Home Assistant OS or Supervised) — the
> integration controls the app via the Supervisor API and will not work on
> plain Home Assistant Core or Container.

> Apps were called *add-ons* before Home Assistant 2026.2 — same thing, new name.

## Install

1. In Home Assistant, go to **Settings → Apps → App store**.
2. Open the **⋮** menu, choose **Repositories**, add
   `https://github.com/oetiker/byonk` and select **Add**.
3. Find **Byonk** in the store, select **Install**, then **Start**.
4. Byonk asks you to restart Home Assistant. Do that
   (**Settings → System → Restart**).
5. After the restart, a **Byonk** card is waiting in
   **Settings → Devices & Services**. Select **Add**.

That is the whole setup. Byonk generates its own management token, and no
token or password is ever asked of you.

## Point your TRMNL device at Byonk

The app publishes Byonk on host port **3000**. Set your TRMNL device's server
to `http://<your-home-assistant-host>:3000`.

## Onboarding a device

Byonk ships with no devices configured — Home Assistant is the source of truth.
When a TRMNL device boots for the first time, it contacts Byonk and displays a
**registration code** on its e-ink screen while waiting to be claimed.

A **Discovered** card for the new device appears automatically in
**Settings → Devices & Services**.

1. Click **Add** on the Discovered card.
2. In the *Set up TRMNL device* form, choose the screen you want displayed on the
   device. Optionally set a dither algorithm and panel type.
3. If the chosen screen declares parameters (via the `params` schema in its `meta.yaml`), a second form
   appears to fill in those values.
4. Submit — the device is now an HA device with its own config entry, and its
   screen mapping is written to Byonk. The device starts fetching its assigned screen
   on the next refresh.

> **Note:** What an un-onboarded (or registered-but-unassigned) device displays on
> its e-ink panel is controlled by the **Byonk Default** device's Screen select
> (see [Entities](#entities) below) — change it there any time, live, no restart
> needed.

Removing an HA device (via **Settings → Devices & Services → Delete**) removes its
mapping from Byonk. Byonk mappings that have no corresponding HA device are pruned
automatically.

## Entities

### Hub device (Byonk Server)

| Entity | Type | Description |
|--------|------|-------------|
| Registration enabled | Switch | Allow new TRMNL devices to register |
| Update screen repos | Button | Trigger an immediate refresh of all screen repos (see below) |
| *Screen repo status* (one per screen repo) | Sensor | Diagnostic sensor per non-builtin screen repo — see [Monitoring screen repos](#monitoring-screen-repos) below |

The remaining server-global settings — `auth_mode` and `screen_repo_refresh_interval` —
are **not** exposed as entities here; they're edited in
[Settings](#settings) below (changes apply on app restart).

### Byonk Default device

Alongside the hub, Byonk automatically creates a **Byonk Default**
device — no setup step needed. Its single **Screen** select entity sets the
screen assigned to byonk's reserved `devices.DEFAULT` entry: the screen shown by
every un-onboarded device (with its pairing code) and by any registered device
with no screen of its own. Changes apply live, no restart required.

### Per-device entities (one device per TRMNL)

| Entity | Type | Description |
|--------|------|-------------|
| Battery voltage | Sensor | Battery voltage (V) |
| Signal strength | Sensor | Wi-Fi RSSI (dBm) |
| Last seen | Sensor | Timestamp of last check-in |
| Firmware version | Sensor | Firmware version string |
| Screen preview | Camera | Picture of what the panel is showing, rendered by Byonk. It sits in the **Sensors** card; click it for a full-size view. |
| Refresh preview | Button | Re-render the screen preview now (see [Screen Preview](#screen-preview)) |
| Preview dithering | Switch | On: the dithered image the panel receives. Off: the screen before dithering, in full color. Affects the preview only |
| Preview measured colors | Switch | On: the measured colors a calibration says the panel really produces. Off: the spec colors byonk sends to it. Affects the preview only |
| Model | Sensor | Verbatim `Model` header reported by the device |
| Screen | Select | Active screen assigned to this device |
| Dither | Select | Dither algorithm override |
| Panel | Select | Panel profile override |
| Refresh interval | Number | Per-device refresh interval in seconds (`0` = no override). Precedence: screen's Lua `refresh_rate` > this override > screen's static default |
| *Screen parameter* (one per param) | Text / Number / Switch / Select | Each parameter declared in the current screen's parameter schema (the `params` block in its `meta.yaml`) appears as its own entity in the **Controls** card (type mapped from the schema: string→Text, int/float→Number, bool→Switch, enum→Select). Changes apply instantly. The set of entities updates automatically when you assign a different screen to the device. |

## Editing device settings

To change the screen for an already-onboarded device, use the **Screen** select
entity on the device card.  To adjust dither algorithm or panel type, use the
**Dither** or **Panel** select entities.

To update the per-screen parameters, use the live entities in the device's **Controls**
card — each parameter of the current screen appears as its own Text, Number, Switch, or
Select entity and applies instantly.  The set of parameter entities updates automatically
when you change the device's screen.

**Device naming**: the device's name is owned by Home Assistant. Rename the device
the usual way (device card → pencil icon) and byonk will mirror the name automatically
when you rename the device in Home Assistant. No changes are needed in byonk's config directly.

## Screen preview

Each device page shows a **Screen preview** camera: a picture of what that
device's panel is displaying, rendered by Byonk from the device's own screen,
parameters, panel profile and dither settings. Change the **Screen** select and
the picture follows, so you can see the effect of a setting without walking over
to the device.

The **Byonk Default** device has one too — that is the screen every
un-onboarded or unassigned device shows.

**It costs nothing while you are not looking at it.** Home Assistant never polls
a camera; frames are only pulled while a browser has the picture open. Byonk in
turn keeps the rendered image and only re-renders when the device's configuration
changes or the screen's own refresh rate elapses, so an open device page does not
turn into a render loop or hammer whatever APIs the screen's script calls.

That last rule has one consequence worth knowing: a screen whose *data* moves on
its own — a clock, a weather forecast — can sit still in the preview while the
panel has moved on. Press **Refresh preview** to force a fresh render.

A screen that fails to render shows the error image the panel itself would
display, rather than an empty box.

### What the preview shows

Two switches on the device page change how the preview is drawn. **Neither
changes what the device displays** — they are Home Assistant's own view
settings, stored on the device's config entry, and nothing is written back to
Byonk's device configuration.

**Preview dithering** — on by default. Off returns the screen *before*
dithering: a full-color rasterization with no palette restriction. That is the
version to look at when you are checking a layout, since dithering at card size
obscures fine detail. Turn it back on to judge how the screen will actually
reproduce on e-ink.

**Preview measured colors** — on by default. Byonk draws the palette in the
*measured* colors a [panel calibration](configuration.md) says the panel really
produces, which is what makes the preview look like the physical device rather
than like an idealised screen. Off draws the *spec* colors Byonk sends to the
panel instead. With no calibration configured the two are the same. This switch
has no effect while **Preview dithering** is off, because an undithered render
has no palette to map.

> **Note:** the preview is rendered at the panel's own resolution and scaled to
> fit the card. E-ink dither patterns are fine-grained, so some of that texture is
> lost at card size — click the picture to see it full size.

## Settings

| Option | Default | Notes |
|--------|---------|-------|
| `admin_token` | *(blank)* | **Leave blank.** Managed automatically by Byonk. While blank, the management API is disabled — serving screens is unaffected. |
| `log_level` | `info` | Server log verbosity (`trace`/`debug`/`info`/`warn`/`error`). |
| `auth_mode` | `api_key` | Device authentication mode (`api_key` or `ed25519`). |
| `screen_repo_refresh_interval` | `0` | Seconds between automatic screen repo refreshes (`0` = disabled — refresh only via the **Update screen repos** button, see [Entities](#entities) above). |
| `screen_repos` | *(empty)* | The screen repo registry: a repeatable list of `handle` / `repo` / `pin` (branch, tag, or commit SHA) / `token` (optional, for private repos) rows — add one row per remote screen repo. |

**This Configuration tab is the source of truth for Byonk's server-global
configuration** — `auth_mode`, `screen_repo_refresh_interval`, and the screen repo
registry. Home Assistant Supervisor writes your changes to `/data/options.json`,
and Byonk reads them back on startup.

**Changes apply on app restart** — there is no live-reload for app options
(this is a Home Assistant Supervisor limitation, not a Byonk one). Restart the
app after saving to apply a change. The restart is quick, per-device screen
mappings are unaffected (they're Byonk's own persisted state), and already-fetched
screen repo checkouts are cached on disk, so unchanged screen repos are not re-fetched.

These settings are **read-only over the admin API** — attempts to change them
there are rejected with a 409 pointing back to this Configuration tab. This tab
is the only editor. Per-device screen/dither/panel assignment and the two live
operational controls (the registration switch, the "Update screen repos"
button) are unaffected and continue to work from the entities described in
[Entities](#entities) above.

## Configuration, screens and fonts

The app maps an editable, persistent folder to `/config` inside the container,
holding `config.yaml`, `screens/` (your writable `local` screen repo),
`examples/` (the shipped worked-example screens, seeded once so you can read,
run, and fork them), and `fonts/`. Edit these with the **File editor** or
**Studio Code Server** app. Empty folders are seeded with the embedded
defaults on first start — see [Screen Authoring](authoring.md) for what gets
seeded where. Edits to `config.yaml` are applied without a restart.

> **Note:** the `screen_repos:` section and the `auth_mode` /
> `screen_repo_refresh_interval` settings in `config.yaml` are **ignored** —
> those come from [Settings](#settings) above instead. `config.yaml` still
> supplies everything else: per-device mappings (`devices:`, normally managed
> through the entities in [Entities](#entities) above), including the reserved
> `devices.DEFAULT` entry that controls what an un-onboarded or unassigned
> device displays — set live from the **Byonk Default** device's Screen select
> (see [Entities](#entities) above), no restart needed.

## Screen repo cache persistence

If the `screen_repos` list in [Settings](#settings) above references remote
(git-backed) screen repos, their fetched git checkouts are cached on disk.
The app ships with `SCREEN_REPOS_CACHE_DIR=/data/packages` set in its manifest —
`/data` is the app's automatically-persistent private storage — so the cache
survives restarts and rebuilds and screen repos are not re-fetched every boot. You
do not need to configure anything.

(For reference: when `SCREEN_REPOS_CACHE_DIR` is unset, byonk falls back to a
temp directory, so every fetched checkout would be lost and re-fetched on each
restart. The shipped app sets it, so this caveat does not apply here.)

## Monitoring screen repos

Screen repos (see [Screen Repos Section](configuration.md#screen-repos-section) in
the Configuration guide) are **added, edited, and removed in
[Settings](#settings) above** — not here. This page's entities give you
read-only monitoring and one operational control:

Each screen repo gets a diagnostic **status sensor** (e.g.
`sensor.byonk_disttest_status`) on the *Byonk Server* hub device, whose state is
the fetch status (`fetching`, `ready`, `error`, ...) and whose attributes include
the resolved commit (`resolved_sha`), `last_fetched` time, `repo`, `pin`, and any
`error`.

When a screen repo fails to fetch, Byonk raises a Home Assistant
**Repair** issue (**Settings → System → Repairs**) carrying the fetch error, so a
broken screen repo surfaces visibly rather than only in the status sensor's
attributes. The issue clears automatically once the screen repo fetches
successfully again.

Press the hub device's **Update screen repos** button to trigger an immediate
content refresh of every already-configured screen repo (a git pull on the existing
pin — equivalent to waiting for the `screen_repo_refresh_interval` set in
[Settings](#settings) above); the status sensors update once the fetch completes. This
button does not add, remove, or repin screen repos — only [Settings](#settings)
does that.

## Re-authentication

If the admin token stored in the app options becomes invalid (for example after
reinstalling the app), Home Assistant will raise a *Re-authentication required*
notification.  Click **Re-authenticate**, and Byonk will read or
re-provision the token automatically — no manual input is needed.

## Upgrading from an earlier install

If you installed Byonk through HACS before version 0.19.0, remove it from
HACS. The Byonk app now keeps the integration up to date by itself, and HACS
would otherwise offer you a second, competing copy. Nothing else is needed —
your devices and settings are unaffected.
