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
| `admin_token` | *(blank)* | **Leave blank.** Managed automatically by the Byonk integration (a later release). While blank, the management API is disabled — serving screens is unaffected. |
| `log_level` | `info` | Server log verbosity (`trace`/`debug`/`info`/`warn`/`error`). |

## Configuration, screens, and fonts

The add-on maps an editable, persistent folder to `/config` inside the container,
holding `config.yaml`, `screens/`, and `fonts/`. Edit these with the **File
editor** or **Studio Code Server** add-on. Empty folders are seeded with the
embedded defaults on first start. Edits to `config.yaml` are applied without a
restart.

## Screen package cache persistence

If your `packages:` config section references remote (git-backed) screen
packages, their fetched git checkouts are cached on disk. The add-on ships
with `PACKAGES_CACHE_DIR=/data/packages` set in its manifest — `/data` is the
add-on's automatically-persistent private storage — so the cache survives
restarts and rebuilds and packages are not re-fetched every boot. You do not
need to configure anything.

(For reference: when `PACKAGES_CACHE_DIR` is unset, byonk falls back to a
temp directory, so every fetched checkout would be lost and re-fetched on each
restart. The shipped add-on sets it, so this caveat does not apply here.)

## How it relates to the Byonk integration

A companion Home Assistant **integration** (shipping in a later release) manages
device→screen mappings and global settings from the Home Assistant UI and
establishes trust with Byonk automatically — you will not need to copy or set any
token by hand.
