# Byonk — Home Assistant Publishing (maintainer runbook)

Getting byonk into the HACS default store, with a proper icon.

> **2026-07 update — the home-assistant/brands PR is obsolete.** Since Home
> Assistant **2026.3**, custom integrations ship their **own** brand images in a
> `brand/` directory, which take priority over the CDN. `home-assistant/brands`
> **no longer accepts** custom-integration icons (PRs are auto-closed). HACS
> likewise now accepts a local `brand/` directory instead of a brands entry.
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

## 2. hacs/default PR (default-store inclusion)

Prerequisites, all met: `custom_components/byonk/` (single integration),
`hacs.json`, `manifest.json` with `domain`/`name`/`version`/`documentation`/
`issue_tracker`/`codeowners`, a published release, and the local `brand/` dir
(HACS requires at least `brand/icon.png`).

1. Fork `https://github.com/hacs/default`.
2. In the `integration` file, add `oetiker/byonk` on its own line, keeping the
   list alphabetically sorted.
3. Open the PR. The HACS bot validates the repo (release present, `hacs.json`,
   `manifest.json`, local brand icon reachable).

## 3. After the hacs/default PR merges

- Switch `docs/src/guide/ha-integration.md` install steps from the custom-repo
  URL flow to the default-store search flow.
