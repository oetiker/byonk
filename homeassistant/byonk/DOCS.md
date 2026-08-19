# Byonk

Self-hosted content server for TRMNL e-ink devices. This app runs the prebuilt
`ghcr.io/oetiker/byonk` image under Home Assistant Supervisor.

(Apps were called *add-ons* before Home Assistant 2026.2 — same thing, new name.)

This app serves screens to your TRMNL devices on host port **3000**, and
brings its own integration with it — device onboarding, entities (screen
selection, battery, signal, screen parameters, ...), and automatic admin
token provisioning. There is nothing else to install.

## Installation

You've already installed this app — a few steps left:

1. **Start** this app if it is not running yet.
2. Byonk asks you to restart Home Assistant. Do that
   (**Settings → System → Restart**).
3. After the restart, a **Byonk** card is waiting in
   **Settings → Devices & Services**. Select **Configure**.

That is the whole setup. Byonk generates its own management token, and no
token or password is ever asked of you.

## Pointing your TRMNL device at Byonk

The app publishes Byonk on host port **3000**. Configure your TRMNL device to
use `http://<your-home-assistant-host>:3000` as its server.

A newly booted device shows a **registration code** on its screen and appears as a
**Discovered** card in **Settings → Devices & Services** — click **Configure** to
pick its screen.

## Configuration

This Configuration tab is the source of truth for Byonk's server-global settings.
**Changes apply on app restart.**

- **Admin token** — leave blank. Managed automatically by Byonk. While blank,
  the management API is disabled (this does not affect serving screens to
  devices).
- **Log level** — server log verbosity (default `info`).
- **Auth mode** — device authentication mode, `api_key` or `ed25519`.
- **Screen repo refresh interval** — seconds between automatic screen repo
  refreshes (`0` = only on demand, via the integration's *Update screen repos*
  button).
- **Screen repos** — the screen repo registry: one row per remote repo with
  `handle`, `repo`, optional `pin` (branch, tag, or commit SHA) and `token` (for
  private repos). The handles `local`, `examples` and `byonk-builtin` are
  reserved (Byonk registers those itself); a row using one is ignored with a
  warning in the log.

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
