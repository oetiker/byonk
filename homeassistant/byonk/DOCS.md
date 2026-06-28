# Byonk

Self-hosted content server for TRMNL e-ink devices. This add-on runs the prebuilt
`ghcr.io/oetiker/byonk` image under Home Assistant Supervisor.

## Installation

1. In Home Assistant, go to **Settings → Add-ons → Add-on Store**.
2. Open the **⋮** menu (top right) → **Repositories**, add
   `https://github.com/oetiker/byonk`, and close.
3. Find **Byonk** in the store and click **Install**, then **Start**.

## Pointing your TRMNL device at Byonk

The add-on publishes Byonk on host port **3000**. Configure your TRMNL device to
use `http://<your-home-assistant-host>:3000` as its server.

## Configuration

- **Admin token** — leave blank. It is managed automatically by the Byonk Home
  Assistant integration (a later release). While blank, the management API is
  disabled (this does not affect serving screens to devices).
- **Log level** — server log verbosity (default `info`).

## Editing screens and config

Your configuration, screens, and fonts live in the add-on's config folder
(mapped to `/config` inside the add-on). Edit them with the **File editor** or
**Studio Code Server** add-on. Empty folders are seeded with sensible defaults on
first start.

Changes to device→screen mappings are best made through the Byonk integration
once it is available; manual edits to `config.yaml` are also picked up without a
restart.
