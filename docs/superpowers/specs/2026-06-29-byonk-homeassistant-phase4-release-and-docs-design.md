# Byonk ↔ Home Assistant — Phase 4: Release, Validation & Docs (Design)

_Date: 2026-06-29 — depends on Phase 3 (`80ea75e`, PR #22)_

## Context

Phases 1–3 are merged: byonk has a token-gated admin API (Phase 1), a Supervisor
add-on packaged as a direct-image add-on (Phase 2), and a Supervised/HAOS-only HA
integration that auto-installs the add-on, provisions the admin token zero-touch, and
mirrors `config.yaml` into HA entities + config subentries (Phase 3).

Nothing has been published for real-world use yet, and **nobody has run the full
add-on + integration stack on an actual Home Assistant install**. Phase 4 closes that
gap and makes byonk distributable.

## Goal

1. **Validate the whole HA stack end-to-end on a real Home Assistant OS install** —
   this is the gating deliverable. No external publishing happens until it passes.
2. Automate add-on/integration **versioning** in the release pipeline.
3. Prepare byonk for **HACS** distribution (default-list) and **home-assistant/brands**
   registration, including a byonk icon, with the external PRs drafted for the
   maintainer to file.
4. Polish user **docs** to match the now-shipping integration.

## Invariants (must not regress)

- The integration stays **Supervised/HAOS-only**.
- **Zero-touch / no-redundancy trust**: the admin token's single home is the add-on
  option; the config entry stores no token.
- Byonk's `config.yaml` is the **source of truth** the integration mirrors.
- **No Rust behavior changes** are required by this phase. (Phase 4b touches the
  release *workflow* and may add a small bump *script*, not byonk's runtime.)

---

## 4a — Local HAOS test harness (gating; do first)

A reproducible, scripted way to boot Home Assistant OS on this Apple-Silicon Mac,
headless, and exercise the add-on + integration. Lives in the repo so it is reusable.

### Why HAOS (not Container)

The integration requires the **Supervisor** (it installs the add-on via the Supervisor
store API and refuses to run without `hassio`). Plain HA Container has no Supervisor, so
only Home Assistant OS (or a Supervised install) can validate the trust flow. We use the
official **`generic-aarch64`** HAOS image, which *virtualizes* on M1 (near-native), not
emulates.

### Components

- **`tools/ha-vm/run.sh`** — boots HAOS headless under QEMU:
  - `qemu-system-aarch64 -M virt,accel=hvf,highmem=on -cpu host -smp 4 -m 4096`
  - UEFI via pflash: read-only `edk2-aarch64-code.fd` + a per-VM writable varstore copy.
  - Disk: the HAOS `generic-aarch64` qcow2 (persistent across runs).
  - User-mode net with host-forwards: **8123** (HA UI), **3000** (byonk), and an SMB
    port for the deploy loop (host `4445` → guest `445`, alternate host port to avoid
    clashing with macOS file sharing on 445).
  - `-display none -serial mon:stdio` (headless; serial shows the HA CLI).
- **`tools/ha-vm/fetch.sh`** — downloads + verifies (sha) the pinned HAOS image and
  decompresses it; resolves the QEMU EFI firmware path (Homebrew `qemu`, falling back to
  UTM's bundled QEMU). HAOS version is a pinned default, overridable via env.
- **`tools/ha-vm/deploy.sh`** — mounts the VM's Samba `config` share (forwarded SMB
  port) and `rsync`s `custom_components/byonk/` into `/config/custom_components/byonk/`.
  Integration reload after a copy is manual (HA *Developer Tools → Restart* or `ha core
  restart` on the serial console) — we do not plumb a long-lived token for dev-only
  reloads.
- **`tools/ha-vm/README.md`** — usage, the one-time HAOS onboarding + Samba-add-on
  setup steps, and the validation checklist (below).
- **Makefile targets**: `ha-vm` (fetch if needed + boot), `ha-vm-stop`, `ha-vm-clean`
  (wipe disk + varstore), `ha-deploy` (rsync the integration).
- **`.gitignore`**: ignore the downloaded image, the working disk, and the varstore copy
  (large/binary, machine-local).

### Networking note (documented limitation)

User-mode NAT makes 8123/3000 reachable from the Mac but does **not** let real TRMNL
hardware on the LAN reach the VM. That is sufficient to validate the add-on + integration
logic (what gates publishing). Bridged-mode / real-device testing is explicitly out of
scope for Phase 4 and noted as a follow-up.

### Validation checklist (the "test" for 4a)

Run against the booted VM and record pass/fail in the SDD ledger:

1. **Add-on store**: add `https://github.com/oetiker/byonk` as a repository; the *Byonk*
   add-on appears, installs (pulls the published `ghcr.io/oetiker/byonk` image), and
   starts. Port 3000 serves a screen.
2. **Integration install**: `custom_components/byonk` deployed via `make ha-deploy`; after
   restart, *Byonk* is discoverable in **Add Integration**.
3. **Zero-touch trust**: adding the integration (with the add-on *not* pre-installed)
   auto-adds the repo, installs + starts the add-on, provisions the admin token into the
   add-on option, and reads it back. The config entry stores **no** token. A *Byonk
   Server* hub device appears.
4. **Hub entities**: registration switch, auth-mode select, default-screen select,
   pending-devices sensor all present and reflect `config.yaml`.
5. **Onboarding**: a pending device (registration code) raises a *Pending Byonk device*
   Repairs issue; **Add device** form lists the code; selecting a screen registers the
   device by **MAC**; a per-TRMNL HA device with the documented sensors + selects appears.
6. **Subentry mirroring + edit**: changing a screen/param via the subentry **Configure**
   form writes through to `config.yaml` and back into entities; per-screen `@params`
   render as HA selectors.
7. **Reauth**: blanking/invalidating the add-on token raises *Re-authentication required*;
   resolving re-provisions without manual input; a transient connection error does **not**
   trigger a reauth loop.
8. **Device removal grace**: a device that disappears survives one poll (2-strike grace)
   before its subentry is removed.

Any failure here is fixed (in the relevant phase's code) **before** 4b/4c go live.

---

## 4b — Release & version automation (after 4a passes)

### Add-on version

The add-on is a direct-image add-on: its `version:` **must equal an already-published
`ghcr.io/oetiker/byonk` tag** (no local image build). The current `release.yml` `version`
job commits + tags **before** `build-container` pushes the image, so the add-on bump must
happen **after** the image is published.

- Extract the bump into a testable script **`tools/release/bump-addon-version.sh
  <version>`** that updates `homeassistant/byonk/config.yaml` `version:` and prepends a
  `homeassistant/byonk/CHANGELOG.md` entry. (Testable locally → TDD; the workflow just
  calls it.)
- New **`update-addon-version` job** in `release.yml`: `needs: [version,
  build-container]`, checks out `main`, runs the script with
  `needs.version.outputs.version`, commits + pushes to `main` as the github-actions bot.
  This guarantees the add-on never references an unpublished tag.
- **`breaking_versions:`**: when `inputs.release_type == major`, the script also appends
  the new version to `breaking_versions:` in the add-on `config.yaml` (Supervisor shows an
  update warning).

### Integration version

HACS surfaces the GitHub release tag, but `manifest.json` `version` should match. The
`version` job (where `Cargo.toml`/`CHANGES.md` are bumped) also sets
`custom_components/byonk/manifest.json` `version` to the new byonk version, so the
integration version tracks byonk releases.

### Verification

Workflow logic is reviewed for correctness; the extracted bump script is unit-tested
(fixture `config.yaml`/`CHANGELOG.md`/`manifest.json` → assert the rewritten output) so
the risky string-munging is covered without a live release.

---

## 4c — HACS default-list + brands prep (after 4a; externals filed by the maintainer)

### In-repo readiness

- **`hacs.json`**: keep minimal but correct for a single integration under
  `custom_components/byonk/` (`name`, `homeassistant` min version aligned to
  `manifest.json`, `render_readme` as desired). Verify against HACS expectations.
- **`manifest.json`**: confirm all HACS/hassfest-required keys (`domain`, `name`,
  `version`, `documentation`, `issue_tracker`, `codeowners`, `iot_class`, `config_flow`).
- **CI validation**: add a `home-assistant` validation job to `ci.yml` running **hassfest**
  and the **HACS action** against the integration, so submission-readiness is enforced on
  every push (this is the in-repo proof the externals will pass review).

### Brand icon

- Generate a simple byonk mark as **SVG** (committed source, e.g.
  `homeassistant/brands/byonk.svg`) and rasterize to the brands-required PNGs:
  `icon.png` (256×256), `icon@2x.png` (512×512), and `logo.png` (+ `logo@2x.png` if a
  wordmark is wanted). Rasterize with a reproducible script (`rsvg-convert`/Inkscape,
  resolved at implementation). Design: minimal, e-ink-appropriate (high-contrast mono),
  legible at 256px.

### Drafted external submissions (maintainer files them)

A doc **`docs/superpowers/ha-publishing.md`** with copy-pasteable instructions + exact
file contents/locations for:
- **home-assistant/brands** PR: `custom_integrations/byonk/{icon,icon@2x,logo}.png`.
- **hacs/default** PR: add `oetiker/byonk` to the `integration` list (alphabetical).
- Ordering note: brands first (HACS validation checks brands), then default-list; both
  require a published GitHub release containing the integration.

These are **not** auto-filed; they need the maintainer's GitHub identity and external
review.

---

## 4d — Docs polish (alongside 4c)

- **`docs/src/guide/ha-addon.md`**: drop "in a later release" language now that the
  integration ships; clarify that `admin_token` is normally managed by the integration but
  can be set manually for standalone add-on use.
- **`docs/src/guide/ha-integration.md`**: align with the shipped flow; once accepted to
  HACS default-list, update install instructions (custom-repo → default search). Until
  then, keep the custom-repo steps.
- **Dev docs**: the test-VM harness is documented in `tools/ha-vm/README.md` and linked
  from the contributor/dev section (kept out of the user-facing mdBook install flow).
- **`CHANGES.md`** (Unreleased): note the release-automation + docs changes that are
  user-visible; the add-on `CHANGELOG.md` is handled by the 4b bump script going forward.

---

## Sequencing & gating

```
4a (VM harness + validation)  ──passes──▶  4b (version automation)
                                          4c (HACS/brands prep + CI validation)
                                          4d (docs)
                                                │
                                                ▼
                          maintainer files brands + hacs/default PRs
                                                │
                                                ▼
                                    first "real" release
```

4b/4c/4d may proceed in parallel once 4a passes; external PRs are the final, manual step.

## Out of scope

- Bridged networking / real TRMNL hardware against the VM.
- Any byonk Rust runtime change.
- Auto-filing the external (brands / hacs-default) PRs.
- Supervised-on-Debian install path (HAOS VM is the chosen test bed).

## Testing summary

| Deliverable | How verified |
|---|---|
| 4a harness | HAOS boots headless; 8123 reachable; validation checklist passes |
| 4b automation | unit-tested bump script; workflow logic review |
| 4c readiness | hassfest + HACS action green in CI; icons render at target sizes |
| 4d docs | `make docs` clean; pages match shipped behavior |
