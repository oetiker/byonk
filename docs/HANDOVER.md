# Handover — Byonk

_Last updated: 2026-07-28 — **Screen-authoring initiative: Plan 1 is complete and the branch is HELD by explicit user decision (no merge, no PR). Plan 2 (MCP interface) is written and reviewed, execution not yet started.** Branch `feat/screen-store-authoring-core`, HEAD `1d599a3`, 29 commits ahead of `origin/main` (`67b3855`), local-only (never pushed). `make check` (519 tests) and `make docs` green on `9ab8b0b`. Next work: execute Plan 2 with **superpowers:subagent-driven-development**, starting at Task 1._

## The initiative

Turn byonk into a place where screens are **authored**, not just served, and make an LLM (Claude Code) a first-class author that can develop screens against a byonk running **anywhere** — including the HA app inside Home Assistant — over the LAN, with no filesystem access and no Samba share.

Design is split into **3 specs**:

- **Spec 1 — Screen store core + MCP** (`docs/superpowers/specs/2026-07-24-screen-store-and-mcp-design.md`), split into two plans:
  - **Plan 1 — Authoring core** (`docs/superpowers/plans/2026-07-26-screen-store-authoring-core.md`) — **DONE**, this branch, 13/13 tasks reviewed.
  - **Plan 2 — MCP interface** (`docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md`) — **WRITTEN, NOT STARTED**. 12 tasks. This is the next work.
- **Spec 2 — Svelte web UI at `/`** (not written). Always-on SPA, three-pane editor, live preview, full `/dev` parity, then retire `/dev` + `byonk dev`.
- **Spec 3 — Git commit & history** (not written). gix write path over local repos that are git working copies.

## Where to start next session

1. Read `docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md` — especially its **Global Constraints** and **Key decisions locked before implementation** sections, which encode findings that cost real investigation to establish (see below).
2. Invoke **superpowers:subagent-driven-development** and execute Task 1 onward. The SDD ledger goes at `.superpowers/sdd/<date>-screen-store-mcp-interface/progress.md` (git-ignored) — trust it plus `git log` over memory after any compaction.
3. Nothing is blocked. The tree is clean apart from the housekeeping noted at the bottom.

## What Plan 1 shipped (already on this branch)

**Typed writable sources.** axum 0.7→0.8 (all `:param` routes → `{param}`). `ScreenRepoSource::writable_root() -> Option<&Path>`; `DiskScreenRepoSource` → `GitScreenRepoSource`; new `LocalScreenRepoSource`. A `path:` variant on screen-repo config refs (mutually exclusive with `repo:`). `SCREENS_DIR` auto-registers as the writable handle **`local`** unless config declares one. **Writability is a structural property of the source, never a name check.** Registration happens at one site — `ScreenRepoManager::build_disk_sources`, called from both `new` and `rebuild_loader`.

**`ScreenStore`** — `src/services/screen_store.rs`, the shared core Plan 2 and Spec 2 both consume: `read_file`/`write_file` with blake3 etags and verify-before-mutate; `create_screen`/`copy_screen`/`rename_screen`/`delete_screen`; `validate` → `ValidationReport`; `render` → `RenderResult{png, raw_png, log, data, refresh_rate, error}` where every failure class returns one `RenderError{line, message}` and never a panic.

**Three built-in layers, kept separate.** (1) `byonk-base-v1` — embedded include library and Lua namespace, untouched; (2) `byonk-builtin` — minimal read-only *embedded* repo (`default` + `calibration/*`), handle string **frozen**; (3) `examples` — shipped samples, seeded to disk once as an editable repo, overridable with `EXAMPLES_DIR`. A one-time startup migration rewrites device refs `byonk-builtin/<x>` → `local/<x>` **only** for the user's own screens.

`AppState.screen_store: Arc<ScreenStore>` exists and is deliberately consumed by **no route yet** — Plan 2 is its first consumer.

## Load-bearing invariants — do not break these

- **`ScreenStore::new` must get the SAME `Arc<ScreenRepoManager>` the `ContentPipeline` has.** `render` resolves through `self.pipeline`'s manager, not `self.manager`. Guarded by `tests/screen_store_wiring_test.rs`, doc-noted on the field and on `new()`. That test file is also the canonical fixture pattern for building a real `ScreenStore` in tests — Plan 2's new store tests copy it.
- **The `byonk-builtin` handle string is frozen** — device configs in the wild reference it, and `content_pipeline.rs:215` hard-references `byonk-builtin/default` as the un-onboarded-device fallback.
- **`byonk-builtin` enumerates embedded-only** (`screen_paths` → `AssetLoader::list_embedded`) so a user's screens are never listed twice. But **`read` deliberately keeps the `SCREENS_DIR` overlay**, so an upgraded install's customized `default/screen.svg` keeps rendering — which means `local/default` still *shadows* the builtin fallback on read. Documented as a sharp edge in `authoring.md`. `svg_files`/`screen_files` stay merged on purpose.
- **Option resolution for renders lives once**, in `src/api/display.rs` (`resolve_preview_dimensions`, `resolve_query_palette`, `resolve_ctx_palette`, `resolve_effective_tuning`, `resolve_dither_tuning`). Both `/dev/render`'s `handle_render` and `ScreenStore::render` call it. Any new knob goes there, once.
- **The migration and `local`'s registration agree by construction** — `present_screen_dirs` goes through `LocalScreenRepoSource::load` + `screen_paths()`, the same entry point registration uses.
- **`AssetLoader::read_screen_embedded_only`** exists solely so `EmbeddedBuiltinSource::load` reads its manifest without the overlay. One caller — do not widen it.
- Startup order: `seed_and_migrate` (`main.rs:681`) → build state → serve, used by **both** `run_server` and `run_dev_server`.
- Seeding is four independently-fallible sections; a failed examples seed must never prevent config seeding.

## Plan 2 — decisions already settled (do not re-litigate)

These were established against the live `rmcp` 2.2 source. Re-deriving them costs an hour; the plan states each with its rationale.

- **`rmcp` 2.2 is the version.** `3.0.0-beta.4` exists and is a prerelease — out of scope.
- **Host validation is disabled** (`.disable_allowed_hosts()`). rmcp defaults `allowed_hosts` to loopback only, which would reject the entire LAN/Home-Assistant use case. The Bearer admin token already defeats DNS rebinding. **User-approved decision.**
- **Stateless + `json_response: true`** with `NeverSessionManager`. byonk's tools are pure request/response; this also avoids SSE-framing hazards in tests. Two-line reversal if notifications are ever needed.
- **Tool failures are `Ok(CallToolResult::error(...))`, never `Err(ErrorData)`.** rmcp's own docs (`model.rs:2999`): clients render protocol errors opaquely and the model never sees the message. Since the whole value of `StoreError::ReadOnly` is its "use `copy_screen`" hint, this decides whether an agent can recover. `Err(ErrorData)` is reserved for genuine protocol faults.
- **Never use `Implementation::from_build_env()`** — its `env!` expands inside rmcp, so it reports rmcp's crate name and version, not byonk's.
- **`schemars` only via `rmcp::schemars`** — a separate dep would risk version skew against rmcp's 1.0.
- **Every `ScreenStore` call from an MCP handler goes through `spawn_blocking`** (matching `src/api/dev.rs:496`, `src/api/display.rs:698`).
- Every POST to `/mcp` must carry `Accept: application/json, text/event-stream` or rmcp returns 406.

## Plan 2 — the three preconditions are now Tasks 1–3

These were harmless while nothing routed to `ScreenStore`. Plan 2 closes them **before** exposing the surface:

1. **Task 1 — symlink escape on read.** The disk sources apply only a lexical `is_safe_rel` check, so a repo containing `leak -> /etc/passwd` serves it. **Scope note: this goes one step beyond the original list** — `EmbeddedBuiltinSource::read` reaches `AssetLoader::read_screen`, whose `SCREENS_DIR` overlay branch (`assets.rs:207`) has the identical hole, so `byonk-builtin/<path>` would remain an open read primitive if only the two named sources were fixed. All three are covered.
2. **Task 2 — `MAX_FILE_BYTES` unenforced on `validate`'s reads** (and only checked post-hoc in `read_file`). Also fixes a latent mis-report: a non-UTF-8 file currently surfaces as "file not found".
3. **Task 3 — no internal serialization.** `create_screen`/`rename_screen` are check-then-act and their failure cleanup `remove_dir_all`s the destination — two interleaved creates can have one's cleanup delete the other's finished screen.

## Known parked items (non-blocking, reviewed and judged)

- **The `byonk: "0.15"` docs defect is now Plan 2 Task 12 Step 1** — `docs/src/tutorial/first-screen.md:67,187` and `docs/src/api/admin-api.md:537,573` still show `byonk: "0.15"`, and `first-screen.md:67` mislabels it "minimum engine version". It is a caret range, so `"0.15"` *excludes* 0.17.x, and an author following the tutorial hand-writes a false compat warning.
- An explicit `local: { path: /elsewhere }` in config suppresses `SCREENS_DIR` auto-registration, but the migration (which runs before config load) would still rewrite refs to `local/x`. Exotic; the add-on path is blocked by the reserved-handle check.
- `svg_files` staying merged means each `byonk-builtin` render registers every `.svg` under `SCREENS_DIR` as a Tera template — correctness-neutral, O(SCREENS_DIR) reads. Pre-existing.
- 5 `clippy --all-targets` warnings, all pre-existing before this branch, all in test code; `make check` does not run that form.
- **The HA app's options schema still has no `path:` shape**, deliberately: `apply_to_config` runs *after* config validation so a `path`+`repo` pair would bypass `ScreenRepoRef::validate`, and `build_disk_sources` checks `path` first so it would silently win. `local`/`examples` auto-register with zero config, so nothing is blocked. Explicitly out of scope for Plan 2.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. `make docs` builds clean (needs `cargo install mdbook-mermaid` once).
- If `cargo` is not found, add `$HOME/.cargo/bin` to `PATH` — the toolchain is rustup-managed via `rust-toolchain.toml` (never add cargo/rust to mise).
- **Cap parallelism at 4** for compiles and test runs — the build machine is shared.
- **Global constraints:** never `git add -A`/`.` (stage explicit paths, verify `git diff --cached`); CHANGES.md is user-facing only; `byonk-base-v1` untouched; scratch release image (no `/tmp`, state under `/data`).
- **After Plan 2:** connect a real MCP client against a local `byonk serve` and drive list → copy → edit → render → assign (a passing suite does not prove a real client negotiates the handshake), then validate on the HA VM per memories `ha-vm-from-source-addon-build` and `ha-vm-addon-manifest-sync-gap`.

## Uncommitted working-tree state

Unrelated to either plan — agent-setup housekeeping, left deliberately uncommitted:

- `.gitignore` — un-ignores `.claude/skills/` so shared skills can be checked in.
- `CLAUDE.md` — trimmed (dropped stale Key Directories / Release Process / mdBook sections; HA VM detail moved out).
- `.claude/skills/ha-vm-testing/SKILL.md` — new, untracked, holds the moved HA VM workflow.

Land these as one small commit whenever convenient; they do not affect Plan 2.

## Process notes worth keeping

- Plan 1 executed with **superpowers:subagent-driven-development** (v6.2.0): per task, brief → implementer → review package → task reviewer → fix loop → ledger line. Plan 2 will run the same way.
- **Reviews on opus repeatedly found real defects a sonnet implementer missed** — including a migration that would have rewritten `byonk-builtin/default` on every genuinely-upgraded install while all seven of its tests passed, because the fixtures never reproduced a real upgraded `SCREENS_DIR`. Keep reviewing the mutating, migration and auth surfaces at that tier. Plan 2's Tasks 1–3 and 7–9 are exactly that kind of surface.
- Subagents whose transcripts exceeded ~230k tokens stalled or died twice; handing findings over as a **file** and dispatching a fresh implementer was the reliable recovery.
- Writing Plan 2 against the vendored `rmcp` source rather than from memory caught four things that would each have produced working-looking but wrong code. When a plan depends on an unfamiliar third-party API, read that crate's source first.
