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
   **Settings → Devices & Services**. Select **Add**.

That is the whole setup. Byonk generates its own management token, and no
token or password is ever asked of you.

## Pointing your TRMNL device at Byonk

The app publishes Byonk on host port **3000**. Configure your TRMNL device to
use `http://<your-home-assistant-host>:3000` as its server.

A newly booted device shows a **registration code** on its screen and appears as a
**Discovered** card in **Settings → Devices & Services** — click **Add** to
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
- **Dev UI username** / **Dev UI password** — only needed to open the screen
  editor from outside Home Assistant, see *Screen editor* below. Leave both
  blank to disable that external route; the Byonk sidebar entry keeps working
  either way.

These settings are read-only over the admin API — the integration deliberately
does not edit them, so this tab stays the single editor.

## Editing screens and config

Your configuration, screens, and fonts live in the app's config folder
(mapped to `/config` inside the app). Edit them with the **File editor** or
**Studio Code Server** app. Empty folders are seeded with sensible defaults on
first start. Byonk does not watch `config.yaml` for outside changes, so an
edit made this way takes effect only after you restart the app.

Per-device screen mappings are best managed through the Byonk integration,
which writes live with no restart needed; manual edits to the `devices:`
section of `config.yaml` also work, but only take effect after a restart. A key byonk does not know stops the app from starting; the log names the device and the key.

## Screen editor

The **Byonk** entry in the Home Assistant sidebar opens the screen editor.
No extra login is needed — Home Assistant already knows who you are, and
opens the editor for you.

**Not seeing it?** Home Assistant leaves the sidebar entry switched off for a
newly installed app, whatever the app asks for. Open Byonk's own page under
**Settings**, turn on **Show in sidebar**, and it appears. You only do this
once.

The editor lets you browse the screens in your screen repos, edit their Lua
scripts and SVG templates, save with conflict detection (so two open tabs
can't silently overwrite each other's changes), see a live preview of the
rendered screen, and calibrate a panel's measured colours against the device
itself.

Editing works in the one **writable** screen repo, `local` (your own config
folder), set up automatically — no configuration needed. The `examples`
screens, the built-in `byonk-builtin` screens and any repo added through the
screen repo registry above (`repo:`, fetched from git) are read-only: you can
browse them and copy their screens, but not edit them in place. To change an
example, copy it into `local` first — **Copy** in the editor's Library, or
the MCP `copy_screen` tool — and edit the copy.

Byonk itself has another writable repo type, `path:` instead of `repo:`
(see the [screen repo authoring
guide](https://oetiker.github.io/byonk/latest/guide/authoring.html#the-path-config-variant)) —
but this app doesn't expose it. The Screen repos option above only accepts
`repo:` rows, and it replaces the whole registry on every restart, so a
`path:` entry hand-added to `config.yaml` would not survive one. `local` is
the only writable repo this app offers.

### Opening the editor from outside Home Assistant

The sidebar entry is normally all you need. If you want to reach the editor
directly, without going through Home Assistant, set **Dev UI username** and
**Dev UI password** above and open `http://<host>:3000/dev/`. Leave either
one blank and this external route stays disabled — the sidebar entry is
unaffected either way.

## Full documentation

<https://oetiker.github.io/byonk>
