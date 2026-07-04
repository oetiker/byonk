# Add-on-owned global config — Design (redirection of Screen Packages Plan 3)

_Date: 2026-07-04 · Status: **DESIGN — approved in brainstorming, spec under user review**_

## 1. Why this exists

Screen Packages **Plan 3** ("HA package management") put byonk's server-global
configuration — the package registry and the singleton settings — **into the HA
integration**: packages as config subentries, settings via the integration's
Options Flow, all written to byonk over the admin API. Plan 3 is implemented,
reviewed clean, and merge-ready, but **its placement is wrong for what the user
wants**. On live-VM verification the user clarified the intended split (this was
a genuine spec-phase misunderstanding, not an implementation bug):

> "All the global configuration for byonk lives in the [add-on] config screen.
> The byonk instance under Devices should only exhibit read-only information
> which could be used in monitoring byonk."

This spec redirects to that model. **The Plan-3 merge is held**; parts are
reused, the config-*write* parts are reverted (see §9).

## 2. The model (three parties, no shared files)

The integration and byonk **never share a config file**. The integration talks to
byonk **only over the admin API**. That invariant holds before and after this change.

- **App (add-on) Options form = source of truth for server-global config.** When
  the user edits the byonk add-on's Configuration tab and saves, the HA Supervisor
  writes `/data/options.json`; **byonk reads its global config from there** at
  startup (extending the existing `src/addon_options.rs` reader). This is a channel
  entirely internal to the add-on container — the integration has no knowledge of it.
- **byonk owns its own config** as today. In add-on mode, global config (settings +
  packages) is supplied by `options.json` and is **read-only via the admin API**
  (the app Options form is the only editor). byonk still persists **per-device
  mappings** it receives via the API to its own config file.
- **The integration = admin API only.** Read-only for global state (monitoring
  sensors); still **writes per-device** mappings; hosts a few **live operational
  controls** (see §6). It never edits global config.

### Apply model (accepted trade-off)

App options are startup config: HA has **no live-reload for app options**
(confirmed against the HA developer docs — `/data/options.json` is parsed at
startup; there is no supervisor notification channel). So **changing global config
in the app Configure screen restarts byonk.** The user has accepted this: byonk
restarts in a few seconds, per-device mappings are byonk's own persisted state
(untouched), and Plan-2's SHA cache means unchanged package repos are **not**
re-cloned on restart. TRMNL e-ink devices poll infrequently, so the restart is
invisible to them.

Rejected alternatives (documented so the decision is legible):
- **Integration Options Flow** (what Plan 3 built): live, no restart — but it lives
  on the *integration*, not the app screen. Contradicts the user's placement goal.
- **App Ingress web UI:** live + in the app, but a large custom-UI build. Not worth it.

## 3. Goals / non-goals

**Goals**
- All *static* server-global config editable in the byonk **app Configure screen**.
- The integration's Byonk hub device is a **read-only monitoring** surface for global state.
- Per-device screen/dither/panel assignment stays in the integration (fits HA's device UI + discovery).
- byonk's core stays a general self-hosted server; **standalone byonk is unchanged** (`config.yaml` + full read/write admin API).

**Non-goals**
- No live/hot editing of global config from the integration (that's the rejected Plan-3 model).
- No custom byonk web UI / Ingress panel.
- No config migration for existing installs (no external users; the VM may be regenerated).

## 4. App Options schema (`homeassistant/byonk/config.yaml`)

Extend the add-on manifest's `schema:`/`options:` (today only `admin_token`,
`log_level`). Add the **static** global config:

```yaml
options:
  admin_token: ""
  log_level: info
  auth_mode: api_key
  default_screen: byonk-builtin/default
  registration_screen: ""            # "" = built-in code screen
  package_refresh_interval: 0        # seconds; 0 = off
  packages: []
schema:
  admin_token: "password?"
  log_level: "list(trace|debug|info|warn|error)"
  auth_mode: "list(api_key|ed25519)"
  default_screen: "str"
  registration_screen: "str?"
  package_refresh_interval: "int(0,)"
  packages:
    - handle: "str"
      repo: "str"
      pin: "str?"
      token: "password?"
```

The `packages:` list renders as repeatable rows in the HAOS Options form, exactly
like the SSH add-on's "Authorized Keys" list.

**Known UX limitation (accept):** the HAOS Options schema is static, so
`default_screen` / `registration_screen` are **free-text screen refs** (e.g.
`byonk-builtin/useful/swiss-departure-board`) — the form cannot offer a dynamic
dropdown populated from `GET /screens` the way the Plan-3 Options Flow did. This is
the cost of the app-screen location. Document the available refs in the add-on's
Documentation tab.

## 5. byonk server changes

1. **Extend `AddonOptions`** (`src/addon_options.rs`) to also parse `auth_mode`,
   `default_screen`, `registration_screen`, `package_refresh_interval`, and
   `packages` (a `Vec<PackageRef>` — reuse the existing package-ref shape:
   `handle`/`repo`/`pin?`/`token?`). Keep the module's guarantees: never writes the
   file, never logs a token, no-op when the file is absent.
2. **Feed them into byonk on startup.** When the options file is present
   (`ReadResult::Parsed`), byonk is in **add-on mode**:
   - Global **settings** from `options.json` override `AppConfig`'s settings.
   - The **package registry** is taken from `options.json` (registered + fetched via
     the existing Plan-2 machinery; status tracked as today).
   - `config.yaml` continues to provide **`devices:`** (per-device mappings the
     integration writes over the API). byonk merges: options.json (settings +
     packages) + config.yaml (devices) + screens dir.
3. **Global-config admin writes become read-only in add-on mode.** `PATCH /settings`
   and `POST/PATCH/DELETE /packages` and the package `update` endpoints that mutate
   the *registry* return a clear read-only error (e.g. 409 with a message pointing to
   the app Options form) when add-on mode is active. **Per-device** writes
   (`PATCH /devices`) stay allowed (device mappings are byonk-managed, not in
   options.json). Read endpoints (`GET /packages|config|devices|screens|pending`)
   are unchanged.
   - Note: a package **content refresh** (git pull on the existing pin — no registry
     change) MAY stay allowed as an operational action; decide at plan time whether
     `POST /packages/update` counts as a registry mutation (reject) or a content
     refresh (allow). Recommendation: **allow** content refresh (it changes no config).
4. **Standalone mode unchanged:** options file absent → byonk behaves exactly as
   today (config.yaml is the full read/write source; admin API fully writable).
5. **Token handling:** package tokens live in `options.json` (a `password?` field,
   supervisor-managed on disk) and are read by byonk for git auth. byonk's
   `GET /packages` continues to **redact** tokens (`PackageInfo` has no token field).
   The integration never sees a token — simpler than Plan 3's write-only-token dance.

## 6. Integration changes (HA)

**Removed** (revert of Plan 3's *write* paths):
- Package **Add / Reconfigure / Delete subentries** and their flows (Plan 3 Tasks 3, 4, 6).
- The hub **delete-propagation** update-listener (Task 6) + the Issue-1 phantom-delete fix (moot once the listener is gone).
- The subentry **reconcile** in the coordinator (Task 5).
- The global **Options Flow** that writes `registration_screen`/`auth_mode`/`package_refresh_interval` (Task 7).

**Kept** (they are read-only or per-device or operational):
- **Per-package status sensors** (Task 9) — now the primary "monitoring" surface. State = fetch status; attrs = sha/last_fetched/error/repo/pin/pin_kind. Read-only.
- **Per-device** entities + screen/dither/panel mapping + discovery/onboarding (unchanged; still writes via `PATCH /devices`).
- The **auth-mode / new-device-screen select entities stay removed** (Task 8) — those settings now live in the app Options.

**Operational controls (live, kept in the integration — NOT static config):**
- **Registration toggle** (`registration_enabled`): the one setting toggled
  *frequently* (enable → onboard a device → disable). Behind a restart it would be
  painful, so it stays a **live integration switch** over the API — NOT in the app
  Options. _(Open decision for spec review — see §11.)_
- **"Update packages" button** (Task 10): triggers a content refresh (git pull), an
  operational action, not a registry edit. Stays, provided §5.3 allows content refresh.

Net: the integration writes **only** per-device mappings + two operational toggles;
all *static* global config is read-only there.

## 7. Config ownership summary

| Config | Owner / editor | Channel | Apply |
|---|---|---|---|
| `auth_mode`, `default_screen`, `registration_screen`, `package_refresh_interval` | App Options form | options.json → byonk | restart |
| Package registry (`handle/repo/pin/token`) | App Options form | options.json → byonk | restart |
| `registration_enabled` (toggle) | Integration switch | admin API | live |
| Package content refresh (git pull) | Integration button / interval | admin API | live |
| Per-device screen/dither/panel/params | Integration | admin API | live |
| Package fetch status / sha / errors | byonk (read) | `GET /packages` → sensors | live |

## 8. Testing

- **byonk (Rust):** unit tests for the extended `AddonOptions` parse (settings +
  packages list; unknown keys ignored; token redaction preserved). Tests for add-on
  mode: global-config writes rejected, per-device writes allowed, standalone mode
  unchanged. Reuse `BYONK_OPTIONS_FILE` to point at a temp options file.
- **Integration (pytest):** status sensors read from `GET /packages` (unchanged Task
  9 tests); per-device mapping still writes; assert the removed flows/entities are
  gone. Registration switch + Update button still function.
- **Add-on manifest:** `addon_manifest_test` asserts the new schema keys exist.
- **Live VM:** edit packages/settings in the app Options → save → byonk restarts →
  `GET /config` + `GET /packages` reflect the change; status sensors update;
  per-device mapping still editable in the integration.

## 9. Plan 3 branch disposition

Branch `feat/screen-packages-p2-distribution` @ `c2022c6` carries Plan 1 + Plan 2 +
Plan 3. **Hold its merge.**

| Plan 3 work | Disposition |
|---|---|
| Task 1 — API client package methods | **Reuse** read methods; the *write* methods stay for standalone byonk but are unused by the integration |
| Task 2 — coordinator fetches packages | **Reuse** (feeds status sensors) |
| Task 3, 4 — add/reconfigure subentry flows | **Revert** |
| Task 5 — subentry reconcile | **Revert** |
| Task 6 — delete propagation + Issue-1 fix | **Revert** |
| Task 7 — global Options Flow | **Revert** |
| Task 8 — remove settings selects | **Keep** (selects move to app Options) |
| Task 9 — package status sensors | **Keep** (the monitoring surface) |
| Task 10 — Update packages button | **Keep** (operational action) |

Whether to revert on this branch or branch fresh from the reused subset is a
**planning decision** (writing-plans), not part of this spec.

## 10. Standalone byonk (unchanged, stated explicitly)

byonk is a general self-hosted server, not only an HA add-on. With **no**
`/data/options.json` present, byonk behaves exactly as today: `config.yaml` is the
full read/write config source and the admin API is fully writable. Add-on mode is
purely additive and gated on the options file's presence.

## 11. Open decisions for spec review

1. **`registration_enabled` placement.** Recommendation: keep it a **live
   integration switch** (frequently toggled; restart-per-toggle is bad UX), even
   though it is technically "global config." Confirm, or move it into the app Options
   (accepting restart-per-toggle).
2. **`POST /packages/update` in add-on mode.** Recommendation: **allow** (content
   refresh, changes no config). Confirm, or treat it as a registry mutation and reject.
3. **`default_screen` exposure.** Plan 3 deliberately did not surface it; this spec
   adds it to the app Options as free-text. Confirm it should be user-editable.
```
