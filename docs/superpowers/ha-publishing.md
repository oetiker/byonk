# Byonk — Home Assistant Integration Icon (maintainer runbook)

Where the integration's brand icon comes from and why no `home-assistant/brands`
PR is needed.

> HACS was dropped on 2026-08-19 — see
> `docs/superpowers/specs/2026-08-19-addon-installs-integration-design.md`.
> The Supervisor app now installs the integration itself, so HACS has no
> remaining role.

> **2026-07 update — the home-assistant/brands PR is obsolete.** Since Home
> Assistant **2026.3**, custom integrations ship their **own** brand images in a
> `brand/` directory, which take priority over the CDN. `home-assistant/brands`
> **no longer accepts** custom-integration icons (PRs are auto-closed).
> See https://developers.home-assistant.io/blog/2026/02/24/brands-proxy-api

## 1. Brand images — shipped in-repo (done)

`custom_components/byonk/brand/` contains the integration's brand assets
(produced by `homeassistant/brands/rasterize.sh`):

- `icon.png` (256×256), `icon@2x.png` (512×512)
- `logo.png` (512×253), `logo@2x.png` (1024×506)
- optional dark variants: `dark_icon.png`, `dark_logo.png`, `dark_icon@2x.png`,
  `dark_logo@2x.png`

No `manifest.json` change is needed — HA auto-detects the directory. The icon
renders on HA **2026.3+**; on older cores it falls back to the CDN (generic
icon, since byonk is intentionally not in `home-assistant/brands`).
