# Byonk — Home Assistant Publishing (maintainer runbook)

Two external PRs make byonk installable from the HACS default store with a
proper icon. **File them in order** — HACS validation checks brands. Both
require a published GitHub release whose tag contains `custom_components/byonk/`.

## 1. home-assistant/brands PR (do first)

1. Fork `https://github.com/home-assistant/brands`.
2. Add these files (produced by `homeassistant/brands/rasterize.sh` in this repo):
   - `custom_integrations/byonk/icon.png`     ← `homeassistant/brands/icon.png` (256×256)
   - `custom_integrations/byonk/icon@2x.png`  ← `homeassistant/brands/icon@2x.png` (512×512)
   - `custom_integrations/byonk/logo.png`     ← `homeassistant/brands/logo.png`
   - `custom_integrations/byonk/logo@2x.png`  ← `homeassistant/brands/logo@2x.png`
3. Open the PR; the domain `byonk` must match `manifest.json`'s `domain`.

### Caveat: Icon design feedback

The home-assistant/brands review prefers icons trimmed to the artwork with a
transparent background. Byonk's icon is an intentional framed pixel-art scene
on a solid background. Expect possible reviewer feedback requesting a
trimmed/transparent variant — decide at PR time whether to trim or defend the
framed design.

## 2. hacs/default PR (after brands merges)

1. Fork `https://github.com/hacs/default`.
2. In the `integration` file, add `oetiker/byonk` on its own line, keeping the
   list alphabetically sorted.
3. Open the PR. The HACS bot validates the repo (release present, `hacs.json`,
   `manifest.json`, brands icon reachable).

## 3. After both merge

- Remove `ignore: brands` from the `hacs/action` step in `.github/workflows/ci.yml`.
- Update `docs/src/guide/ha-integration.md` install steps from the custom-repo URL
  flow to the default-store search flow.
