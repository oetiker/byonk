# Byonk ↔ Home Assistant — Phase 2: HA Add-on

**Date:** 2026-06-28
**Status:** Approved (design)
**Scope of this spec:** Phase 2 only — packaging byonk as a Home Assistant Supervisor
add-on, plus the minimal byonk change that lets it read add-on options. Phases 3–4 are
referenced for context but specced separately. Phase 1 (the admin API) is done and is the
contract this builds on.

---

## 1. Background & where this fits

Byonk is a self-hosted content server for TRMNL e-ink devices: a single static Rust binary
shipped as a multi-arch Docker image (`ghcr.io/oetiker/byonk`, linux/amd64 + linux/arm64,
built `FROM scratch`). Phase 1 added a token-gated `/api/admin/*` management API, per-screen
`@params` schemas, comment-preserving `config.yaml` writes, and config hot-reload.

The umbrella goal is to make byonk **fully manageable from Home Assistant** via two
deliverables: an **HA Add-on** (this phase) and an **HA Integration** (Phase 3). Phase 2
packages byonk so a Supervisor-managed HA can run it with persistent config and expose it to
TRMNL devices on the LAN, and lays the groundwork for the Phase 3 integration to manage it
with **zero user-entered credentials**.

### Phased plan (each phase = its own spec → plan → build)

1. **Phase 1 — Byonk admin API. ✅ DONE.**
2. **Phase 2 (this spec) — HA Add-on** packaging + a minimal byonk "add-on options reader".
3. **Phase 3 — HA Integration** (`custom_components/byonk/`): config flow, HA Device per
   TRMNL, entities, and the **zero-touch trust provisioning** that consumes this phase.
4. **Phase 4 — Release & docs**: automate the add-on `version:` bump against published image
   tags, HACS metadata, docs polish.

---

## 2. Phase 2 goals & non-goals

### Goals

- Ship an installable HA add-on that runs the **prebuilt** `ghcr.io/oetiker/byonk` image
  directly (no wrapper image to build).
- Give byonk **persistent, user-editable** config/screens/fonts under the add-on config dir,
  seeded from embedded defaults on first run.
- Expose byonk on a **raw host port** so non-HA TRMNL devices reach it directly on the LAN.
- Let the add-on carry a small set of **options** (`admin_token`, `log_level`) that byonk
  reads from `/data/options.json`.
- Establish the design for **zero-touch trust**: the admin token's single source of truth is
  the add-on option; byonk only reads it; the Phase 3 integration provisions it
  automatically. The end user never sees or sets a token.
- Add the **minimal byonk code** to read add-on options. byonk stays fully usable outside HA.

### Non-goals (Phase 2)

- Any Home Assistant **integration** code (`custom_components/`) — Phase 3.
- Building a wrapper/derived image, or using s6-overlay/bashio (the image is `FROM scratch`).
- **Ingress** and exposing byonk's `/dev` preview UI (the image `CMD` is `serve`; `image:`
  add-ons can't override `CMD`; dev mode is out of scope).
- byonk **generating or persisting** an admin token (it only reads the option). Token
  generation/provisioning is the integration's job (Phase 3).
- Surfacing `registration` / `default_screen` as add-on options (the Phase 3 integration owns
  those via the admin API — keeping a single source of truth).
- Automating the add-on `version:` bump in the release pipeline — Phase 4.
- Runtime log-level change without an add-on restart (a restart re-renders `options.json`).

---

## 3. Architecture

```
TRMNL device ──LAN :3000──▶ [HA host port 3000] ──▶ byonk container (scratch image, CMD serve)
                                                       ├─ environment: CONFIG_FILE / SCREENS_DIR /
                                                       │               FONTS_DIR / BIND_ADDR
                                                       ├─ /config   (map addon_config:rw) =
                                                       │   config.yaml + screens/ + fonts/  (persistent, editable)
                                                       └─ /data/options.json (HA-managed) → admin_token, log_level

Phase 3 integration ──Supervisor API──▶ set byonk add-on options(admin_token) + restart
                     ──HTTP :3000 admin API (Bearer token)──▶ byonk /api/admin/*
```

Two moving parts:

1. **The add-on package** (`repository.yaml` + `homeassistant/byonk/`) — pure declarative
   config; references the prebuilt image; wires paths/bind via static `environment:`,
   persistence via `map:`, LAN access via `ports:`, and the two options.
2. **The byonk add-on-options reader** — a small module that reads `/data/options.json` at
   `serve` startup and feeds `admin_token`/`log_level` into byonk's existing mechanisms.
   Everything else (admin API, token gate, seeding, hot-reload) already exists from Phase 1.

---

## 4. Add-on repository structure

Supervisor requires `repository.yaml` at the **git repo root**, but it searches recursively
for each add-on's `config.yaml`, so the add-on itself lives under `homeassistant/byonk/`.
A user adds the repo by URL (`https://github.com/oetiker/byonk`) in
Settings → Add-ons → Add-on Store → ⋮ → Repositories.

```
repository.yaml                        # at repo root (Supervisor requirement)
homeassistant/byonk/
  config.yaml                          # add-on manifest
  DOCS.md                              # HA "Documentation" tab
  CHANGELOG.md                         # HA "Changelog" tab
  icon.png                             # store icon (square)
  logo.png                             # store logo (wide)
  translations/en.yaml                 # option labels + help text
```

### `repository.yaml` (repo root)

```yaml
name: Byonk Add-ons
url: https://github.com/oetiker/byonk
maintainer: Tobias Oetiker <tobi@oetiker.ch>
```

### `homeassistant/byonk/config.yaml`

```yaml
name: Byonk
version: "<published ghcr image tag>"   # MUST equal a published ghcr.io/oetiker/byonk tag
slug: byonk
description: Self-hosted content server for TRMNL e-ink devices
url: https://github.com/oetiker/byonk/tree/main/homeassistant/byonk
image: ghcr.io/oetiker/byonk
arch:
  - amd64
  - aarch64
init: true
ports:
  3000/tcp: 3000
ports_description:
  3000/tcp: TRMNL device + admin API access (point your device here)
map:
  - addon_config:rw
environment:
  CONFIG_FILE: /config/config.yaml
  SCREENS_DIR: /config/screens
  FONTS_DIR: /config/fonts
  BIND_ADDR: 0.0.0.0:3000
options:
  admin_token: ""
  log_level: info
schema:
  admin_token: "password?"
  log_level: "list(trace|debug|info|warn|error)"
```

Notes:

- **`image:` (no `{arch}`)** — Supervisor pulls `{image}:{version}` and Docker resolves the
  multi-arch manifest to the host platform. Therefore **`version:` must equal a tag that is
  already published** to `ghcr.io/oetiker/byonk` (gotcha: `image:` add-ons have no local
  "rebuild" — publish the image tag *before* bumping `version:`). Keeping these in lockstep
  is automated in Phase 4; for Phase 2 we set a correct initial value and document the rule.
- **`init: true`** (default) — Docker's tiny init is PID 1 and reaps/forwards signals; byonk
  runs as its child. Safe for a no-shell scratch binary.
- **`environment:`** is static (author-set, not user-tunable) — exactly right for the fixed
  paths/bind. It is *not* a channel for user options; that is `options`/`schema` →
  `/data/options.json`.
- **`map: [addon_config:rw]`** mounts an editable, persistent dir at **`/config`** inside the
  container, surfaced on the host at `/addon_configs/<repo>_byonk` (reachable via the File
  editor / Samba / VS Code add-ons). byonk's `CONFIG_FILE`/`SCREENS_DIR`/`FONTS_DIR` point
  here, and its existing `seed_if_configured()` seeds empty dirs from embedded defaults on
  first run.
- **`ports: {3000/tcp: 3000}`** publishes the container port on the host so LAN TRMNL devices
  reach `http://<ha-host>:3000`. No `host_network`, no Ingress.
- **`admin_token` option** is declared so the Phase 3 integration can write it via the
  Supervisor API, but is documented as **integration-managed — leave blank**. Blank →
  admin API dormant (404), which is the correct state until the integration provisions it.
  `password?` masks the field and makes it optional.

---

## 5. Zero-touch trust model (single source of truth)

The end user must **never** copy, paste, or set a token to make HA trust byonk. The token has
exactly **one home — the add-on option** — and flows like this:

1. **byonk reads only.** At `serve` startup byonk reads `admin_token` from
   `/data/options.json` and feeds it into its existing token resolution. It never generates
   and never persists a token. With a blank/absent token, `/api/admin/*` returns 404 (Phase 1
   behavior) — dormant until provisioned.
2. **The Phase 3 integration provisions automatically** (specced in Phase 3; described here so
   Phase 2's contract is clear). Running in HA Core, it shares Core's `SUPERVISOR_TOKEN`,
   which bypasses the Supervisor add-on role gate, so it can — for the byonk add-on slug —
   `set_addon_options(admin_token=<generated>)` and `restart_addon()` via the `hassio`
   `AddonManager` (the same pattern the official `zwave_js` integration uses; requires
   `after_dependencies: ["hassio"]` and a Supervisor install). After the restart byonk
   re-reads `options.json` and the admin API comes alive.
3. **No duplicate copies.** The integration reads the token back from the add-on option when
   it needs to authenticate rather than caching its own; the add-on option remains the sole
   source of truth. byonk's `config.yaml` does **not** hold an add-on-provisioned token.

Outside Supervisor (bare `docker run`), nothing here applies: byonk keeps its Phase 1
behavior (`BYONK_ADMIN_TOKEN` env or `config.admin.token`), and the admin token can also be
set directly in the editable `config.yaml`.

### Why not Ingress (considered, rejected)

Reaching byonk's admin API through the Supervisor Ingress proxy would need no shared secret,
but it forces byonk to gate admin access **by network interface** (a separate ingress
listener / ingress path-prefix handling) so the raw LAN port can't reach `/api/admin/*`. That
is a substantially larger byonk change and discards Phase 1's already-correct uniform token
gate. Token-via-options reuses Phase 1 unchanged and keeps `/api/admin/*` protected on every
interface, including the LAN port.

---

## 6. byonk add-on-options reader (the only code change)

A small, well-bounded module — byonk's only Phase 2 change. It must not affect non-add-on
runs (file absent → complete no-op).

### Module

- New `src/addon_options.rs` exposing:
  - `struct AddonOptions { admin_token: Option<String>, log_level: Option<String> }`
    (`serde::Deserialize`, unknown keys ignored, all fields optional).
  - `fn load() -> Option<AddonOptions>` — reads the options file if present; returns `None`
    if absent; on read/parse error logs a `warn!` and returns `None` (never fatal).
  - The path is `/data/options.json`, **overridable via env `BYONK_OPTIONS_FILE`** so tests
    can point at a temp file.

### Wiring (only in `run_server()`; not `dev`, not `render`, not `status`)

- **Log level** — computed *before* `tracing_subscriber` init. Precedence:
  explicit `RUST_LOG` env → `log_level` option (rendered as
  `byonk=<lvl>,tower_http=<lvl>`) → existing built-in default
  (`byonk=debug,tower_http=debug`). Unknown level strings fall back to the default with a
  `warn!`.
- **Admin token** — *before* `create_app_state`. If `admin_token` is present and non-empty
  **and** `BYONK_ADMIN_TOKEN` env is unset/empty, set the env var to that value so the
  existing `server.rs` resolution (`env → config.admin.token`) picks it up. Explicit
  `BYONK_ADMIN_TOKEN` env always wins. Blank/absent → leave admin API dormant. **No token is
  ever logged** (the user never needs it).
- `server.rs` and the Phase 1 token gate are **unchanged**.

### Behavior summary

| `options.json` state | Effect on admin API |
|---|---|
| absent (non-HA run) | unchanged Phase 1 behavior (env / `config.admin.token`) |
| present, `admin_token` empty | dormant (404) until provisioned |
| present, `admin_token` set | admin API active with that token |
| present, malformed | `warn!`, ignored; server still starts |

---

## 7. Persistence, networking, seeding

- **Persistent + editable config:** `/config` (from `map: addon_config:rw`) holds
  `config.yaml`, `screens/`, `fonts/`. Survives add-on restarts/updates; editable by the user
  via the File editor for power-user screen authoring. The admin API and Phase 3 integration
  write `config.yaml` here (Phase 1's comment-preserving writer + hot-reload).
- **Seeding:** byonk's existing `seed_if_configured()` populates empty `/config` subdirs from
  embedded defaults on first start — fresh installs work with zero configuration.
- **`/data`:** holds HA-managed `options.json` only; byonk reads it, never writes it.
- **Networking:** raw host port `3000` for non-HA TRMNL clients; no Ingress, no host network.
  The admin API shares the same port and stays token-gated.

---

## 8. Versioning & updates (Phase 2 scope vs Phase 4)

- The add-on `version:` **is** the Docker tag Supervisor pulls. Bumping it (after the matching
  image tag is published) makes the Supervisor UI offer an update, which pulls the new tag.
- **Phase 2** sets a correct initial `version:` matching the current published image and
  documents the lockstep rule + the "publish image before bump" gotcha.
- **Phase 4** automates the bump (release workflow updates `homeassistant/byonk/config.yaml`
  `version:` when a new image tag is published) and adds `breaking_versions:` handling.

---

## 9. Documentation & changelog

- **mdBook:** new page **"Home Assistant Add-on"** under *Getting Started* (alongside
  Installation), wired into `docs/src/SUMMARY.md`: how to add the repo URL and install; what
  the add-on does; that **the Phase 3 integration handles trust automatically and the
  `admin_token` option should be left blank**; pointing TRMNL devices at `http://<ha-host>:3000`;
  where the editable config lives. A short note that the integration itself ships in Phase 3.
- **Add-on docs:** `homeassistant/byonk/DOCS.md` (HA Documentation tab) and `CHANGELOG.md`.
- **CHANGES.md:** Unreleased entries for the add-on package and the byonk add-on-options
  reader.

---

## 10. Testing (TDD)

Follow the project's TDD discipline; write tests first.

- **Add-on options reader (unit):**
  - file absent → `load()` returns `None`, no env mutation.
  - valid file → fields parsed; unknown keys ignored.
  - admin-token precedence: explicit `BYONK_ADMIN_TOKEN` env wins over the option; option
    used when env unset; blank option leaves things dormant.
  - log-level mapping: known levels render the expected filter; unknown level falls back with
    a warning; `RUST_LOG` env overrides the option.
  - malformed JSON → `warn!` + `None` (no panic).
  - (tests drive the file via `BYONK_OPTIONS_FILE`.)
- **Admin-API integration (reuse Phase 1 axum harness):** with an `options.json` (via
  `BYONK_OPTIONS_FILE`) carrying a token, `/api/admin/*` is reachable with the matching
  `Bearer`; with a blank/no token it returns 404.
- **Add-on manifest (static):** a Rust test parses `repository.yaml` and
  `homeassistant/byonk/config.yaml` as YAML and asserts the required keys and invariants —
  `image == ghcr.io/oetiker/byonk`, `arch` ⊇ {amd64, aarch64}, `ports` maps `3000/tcp`,
  `map` includes `addon_config:rw`, `environment` sets the four expected vars, `schema` has
  `admin_token` + `log_level`, and `slug == byonk`.
- **`make check`** (fmt + clippy + tests) clean; **`make docs`** clean.

### Manual acceptance checklist (documented, not automated)

On a real Supervisor install: add the repo URL → install byonk → it starts and serves a
device on `:3000` over the LAN; with the `admin_token` blank, `/api/admin/*` returns 404;
setting the option (simulating Phase 3) + restart makes the admin API respond; edits to
`/config/config.yaml` persist across an add-on restart and an update.

---

## 11. Risks & mitigations

- **Zero-touch depends on Supervisor.** The integration's provisioning only works on
  HAOS/Supervised installs. *Mitigation:* a Phase-3 concern; Phase 2's add-on is independently
  usable, and bare-docker users keep the Phase 1 token mechanisms. Documented.
- **`version:` ↔ image-tag drift.** A bump without a published tag breaks the pull (no local
  rebuild for `image:` add-ons). *Mitigation:* documented rule now; automated in Phase 4.
- **Options reader leaking into non-HA runs.** *Mitigation:* absent file → strict no-op; env
  always overridable; only wired into `serve`.
- **`addon_config` mount path / repo-hash specifics vary by install type** (`local-` vs repo
  hash). *Mitigation:* byonk uses the in-container `/config` path (stable); host-side path is
  only referenced in docs as "via the File editor add-on".
- **Add-on options API surface is not stability-guaranteed** (`aiohasupervisor` migration).
  *Mitigation:* a Phase-3 concern (the integration pins/guards against Core versions); Phase 2
  only reads `options.json`, whose format is stable.

---

## 12. Acceptance criteria (Phase 2 done when…)

1. `repository.yaml` (repo root) + `homeassistant/byonk/` add-on exist and parse; the manifest
   test passes.
2. The add-on references `ghcr.io/oetiker/byonk` directly, sets the four `environment:` paths,
   maps an editable persistent `/config`, publishes host port 3000, and declares the
   `admin_token` + `log_level` options.
3. byonk reads `admin_token` + `log_level` from `/data/options.json` at `serve` start, with
   the precedence and graceful-degradation rules of §6; non-HA runs are unaffected.
4. A token supplied via `options.json` activates `/api/admin/*`; a blank token leaves it
   dormant (404) — no token is generated, persisted, or logged by byonk.
5. `make check` and `make docs` pass; `CHANGES.md`, the mdBook add-on page, and the add-on
   `DOCS.md`/`CHANGELOG.md` are written.
