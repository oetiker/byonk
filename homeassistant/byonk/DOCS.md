# Byonk

Self-hosted content server for TRMNL e-ink devices. This app runs the prebuilt
`ghcr.io/oetiker/byonk` image under Home Assistant Supervisor.

(Apps were called *add-ons* before Home Assistant 2026.2 — same thing, new name.)

Byonk on Home Assistant comes in **two parts**, and you want both:

- **This app** — the Byonk server itself: it serves screens to your TRMNL
  devices on host port **3000**, and its Configuration tab holds Byonk's
  server-global settings (auth mode, screen repos).
- **The Byonk integration** (a HACS custom integration) — onboards your TRMNL
  devices into Home Assistant, gives each one entities (screen selection, battery,
  signal, screen parameters, ...), and provisions the admin token automatically so
  you never copy a secret by hand.

## Installation

The easiest path is to **install the integration first — it installs and starts
this app for you**, fully zero-touch.

1. In Home Assistant, open **HACS → Integrations**.
2. Three-dot menu (top right) → **Custom repositories**, add
   `https://github.com/oetiker/byonk` as an **Integration** repository.
   (Once Byonk is in the HACS default store you can skip this and just search.)
3. Search for *Byonk* in HACS, install it, and **restart Home Assistant**.
4. Go to **Settings → Devices & Services → Add Integration** and search for
   *Byonk*. It adds the Byonk app repository, installs and starts this app,
   and generates the admin token itself.

### Installing the app on its own

If you would rather run the server without the integration (no Home Assistant
device entities, manual `config.yaml` editing):

1. **Settings → Apps → App store**.
2. Three-dot menu (top right) → **Repositories**, add
   `https://github.com/oetiker/byonk`, and select **Add**.
3. Find **Byonk** in the store, click **Install**, then **Start**.

You can add the integration later at any time — it will pick up the app you
already have running.

## Pointing your TRMNL device at Byonk

The app publishes Byonk on host port **3000**. Configure your TRMNL device to
use `http://<your-home-assistant-host>:3000` as its server.

A newly booted device shows a **registration code** on its screen and appears as a
**Discovered** card in **Settings → Devices & Services** — click **Configure** to
pick its screen. (Without the integration, add the device to `config.yaml` by hand
instead.)

## Configuration

This Configuration tab is the source of truth for Byonk's server-global settings.
**Changes apply on app restart.**

- **Admin token** — leave blank. The Byonk integration provisions and manages it
  automatically. While blank, the management API is disabled (this does not affect
  serving screens to devices).
- **Log level** — server log verbosity (default `info`).
- **Auth mode** — device authentication mode, `api_key` or `ed25519`.
- **Screen repo refresh interval** — seconds between automatic screen repo
  refreshes (`0` = only on demand, via the integration's *Update screen repos*
  button).
- **Screen repos** — the screen repo registry: one row per remote repo with
  `handle`, `repo`, optional `pin` (branch, tag, or commit SHA) and `token` (for
  private repos).

These settings are read-only over the admin API — the integration deliberately
does not edit them, so this tab stays the single editor.

## Editing screens and config

Your configuration, screens, and fonts live in the app's config folder
(mapped to `/config` inside the app). Edit them with the **File editor** or
**Studio Code Server** app. Empty folders are seeded with sensible defaults on
first start. Edits to `config.yaml` are picked up without a restart.

Per-device screen mappings are best managed through the Byonk integration; manual
edits to the `devices:` section of `config.yaml` also work.

## Full documentation

<https://oetiker.github.io/byonk>
