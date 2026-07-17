# Home Assistant Add-on

Byonk can run as a Home Assistant Supervisor add-on. The add-on runs the same
prebuilt `ghcr.io/oetiker/byonk` image, stores its configuration in a persistent,
editable folder, and exposes Byonk on a host port so your TRMNL devices can reach
it directly on your LAN.

> Requires a Supervisor-managed install (Home Assistant OS or Supervised).

## Install

1. **Settings → Add-ons → Add-on Store**.
2. **⋮ → Repositories**, add `https://github.com/oetiker/byonk`, then close.
3. Open **Byonk** in the store, **Install**, then **Start**.

## Point your TRMNL device at Byonk

The add-on publishes Byonk on host port **3000**. Set your TRMNL device's server
to `http://<your-home-assistant-host>:3000`.

## Options

| Option | Default | Notes |
|--------|---------|-------|
| `admin_token` | *(blank)* | **Leave blank.** Managed automatically by the Byonk integration. While blank, the management API is disabled — serving screens is unaffected. |
| `log_level` | `info` | Server log verbosity (`trace`/`debug`/`info`/`warn`/`error`). |
| `auth_mode` | `api_key` | Device authentication mode (`api_key` or `ed25519`). |
| `package_refresh_interval` | `0` | Seconds between automatic screen-package refreshes (`0` = disabled — refresh only via the integration's **Update packages** button). |
| `packages` | *(empty)* | The screen-package registry: a repeatable list of `handle` / `repo` / `pin` (branch, tag, or commit SHA) / `token` (optional, for private repos) rows — add one row per remote package. |

## Global configuration: settings and screen packages

**This Configuration tab is the source of truth for Byonk's server-global
configuration** — `auth_mode`, `package_refresh_interval`, and the screen-package
registry. Home Assistant Supervisor writes your changes to `/data/options.json`,
and Byonk reads them back on startup.

**Changes apply on add-on restart** — there is no live-reload for add-on options
(this is a Home Assistant Supervisor limitation, not a Byonk one). Restart the
add-on after saving to apply a change. The restart is quick, per-device screen
mappings are unaffected (they're Byonk's own persisted state), and already-fetched
package checkouts are cached on disk, so unchanged packages are not re-fetched.

While running as the add-on, these settings are **read-only over the admin API** —
attempts to change them there (including from the Byonk integration) are rejected
with a 409 pointing back to this Configuration tab. This tab is the only editor.
Per-device screen/dither/panel assignment and the two live operational controls
(the registration switch, the "Update packages" button) are unaffected and
continue to work from the [Byonk integration](ha-integration.md).

## Configuration, screens, and fonts

The add-on maps an editable, persistent folder to `/config` inside the container,
holding `config.yaml`, `screens/`, and `fonts/`. Edit these with the **File
editor** or **Studio Code Server** add-on. Empty folders are seeded with the
embedded defaults on first start. Edits to `config.yaml` are applied without a
restart.

> **Note:** while running as the add-on, the `packages:` section and the
> `auth_mode` / `package_refresh_interval` settings in `config.yaml` are
> **ignored** — those come from the Configuration tab above instead. `config.yaml`
> still supplies everything else: per-device mappings (`devices:`, normally
> managed by the [Byonk integration](ha-integration.md)), including the reserved
> `devices.DEFAULT` entry that controls what an un-onboarded or unassigned device
> displays — set live from the **Byonk Default** device's Screen select in the
> [Byonk integration](ha-integration.md), no restart needed.

## Screen package cache persistence

If the `packages` list in the Configuration tab above references remote
(git-backed) screen packages, their fetched git checkouts are cached on disk.
The add-on ships with `PACKAGES_CACHE_DIR=/data/packages` set in its manifest —
`/data` is the add-on's automatically-persistent private storage — so the cache
survives restarts and rebuilds and packages are not re-fetched every boot. You
do not need to configure anything.

(For reference: when `PACKAGES_CACHE_DIR` is unset, byonk falls back to a
temp directory, so every fetched checkout would be lost and re-fetched on each
restart. The shipped add-on sets it, so this caveat does not apply here.)

## How it relates to the Byonk integration

A companion Home Assistant **[integration](ha-integration.md)** establishes trust
with Byonk automatically — you will not need to copy or set any token by hand —
and manages per-device screen/dither/panel mappings from the Home Assistant UI.
It also surfaces read-only monitoring for the config you set here (per-package
status sensors) and two live operational controls (a registration switch and an
"Update packages" button). It does **not** edit `auth_mode`,
`package_refresh_interval`, or the package registry — those are only editable
here, in the add-on's Configuration tab.
