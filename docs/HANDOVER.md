# Handover — Byonk

_Last updated: 2026-07-28 — **Screen-authoring initiative: Plan 1 (authoring core) is COMPLETE.** All 13 tasks implemented, individually reviewed, and closed out by a whole-branch review + one fix wave + a scoped re-review — no merge blockers. Branch `feat/screen-store-authoring-core`, HEAD `9ab8b0b`, 25 commits ahead of `main` (`67b3855`), local-only (not pushed). `make check` and `make docs` green on HEAD. **Awaiting the integration decision (merge / PR / hold).** Next work: write Plan 2 (MCP interface), then Specs 2 and 3._

## The initiative

Turn byonk into a place where screens are **authored**, not just served, and make an LLM (Claude Code) a first-class author that can develop screens against a byonk running **anywhere** — including the HA app inside Home Assistant — over the LAN, with no filesystem access and no Samba share.

Design is split into **3 specs**:
- **Spec 1 — Screen store core + MCP** (`docs/superpowers/specs/2026-07-24-screen-store-and-mcp-design.md`). Split into two plans: **Plan 1 — Authoring core** (`docs/superpowers/plans/2026-07-26-screen-store-authoring-core.md`) ← **DONE, this branch**; **Plan 2 — MCP interface** (not written) — Component 5 of spec 1, `rmcp` streamable-HTTP at `/mcp` on the main router, admin-token gated (same `require_admin` Bearer as `/api/admin/*`). That's why Task 1 bumped axum to 0.8.
- **Spec 2 — Svelte web UI at `/`** (not written). Always-on SPA, three-pane editor, live preview, full `/dev` parity, then retire `/dev` + `byonk dev`.
- **Spec 3 — Git commit & history** (not written). gix write path over local repos that are git working copies.

## What Plan 1 shipped

**Typed writable sources (Tasks 1–5).** axum 0.7→0.8 (all `:param` routes → `{param}`). `ScreenRepoSource::writable_root() -> Option<&Path>`; `DiskScreenRepoSource` → `GitScreenRepoSource`; new `LocalScreenRepoSource`. A `path:` variant on screen-repo config refs (mutually exclusive with `repo:`, validated inside `AppConfig::load_from_assets`). `SCREENS_DIR` auto-registers as the writable handle **`local`** unless config declares one. **Writability is a structural property of the source, never a name check.** Registration happens at one site — `ScreenRepoManager::build_disk_sources`, called from both `new` and `rebuild_loader` — so the initial snapshot and every rebuild are identical by construction.

**`ScreenStore` (Tasks 6–9)** — `src/services/screen_store.rs`, the shared core Plan 2 and Spec 2 both consume:
- `read_file`/`write_file` with blake3 etags; strictly verify-before-mutate (resolve root → canonicalize base → canonicalize deepest existing ancestor → prefix check → `create_dir_all` → unique tmp → rename → `rebuild_loader()`).
- `create_screen`/`copy_screen`/`rename_screen`/`delete_screen`, `StarterKind::Minimal`.
- `validate` → `ValidationReport{ok, issues}` (meta schema + Lua compile-only + SVG registration via `TemplateService`).
- `render(screen_ref, RenderOpts) -> RenderResult{png, raw_png, log, data, refresh_rate, error}` — one `RenderError{line, message}` for every failure class; every failure path returns a result, never a panic.

**Three built-in layers, kept separate (Tasks 10–12).** (1) `byonk-base-v1` — embedded include library **and** Lua namespace, untouched by this work; (2) `byonk-builtin` — minimal read-only *embedded* repo (`default` + `calibration/*`), handle string **frozen**; (3) `examples` — shipped samples, embedded separately, seeded to disk once as an editable repo, overridable with the new **`EXAMPLES_DIR`** env var. Byonk no longer copies builtin screens into `SCREENS_DIR`; that dir gets only a `local` manifest. A one-time startup migration rewrites the leftover manifest and rewrites device refs `byonk-builtin/<x>` → `local/<x>` **only** for the user's own screens.

**Wiring + docs (Task 13).** `AppState.screen_store: Arc<ScreenStore>` — no route consumes it yet, by design. New user page `docs/src/guide/authoring.md`; CHANGES.md Unreleased written.

## Load-bearing invariants — do not break these

- **`ScreenStore::new` must get the SAME `Arc<ScreenRepoManager>` the `ContentPipeline` has.** `render` resolves through `self.pipeline`'s manager, not `self.manager`. Guarded by `tests/screen_store_wiring_test.rs`, doc-noted on the field and on `new()`.
- **The `byonk-builtin` handle string is frozen** — device configs in the wild reference it, and `content_pipeline.rs:215` hard-references `byonk-builtin/default` as the un-onboarded-device fallback.
- **`byonk-builtin` enumerates embedded-only** (`screen_paths` → `AssetLoader::list_embedded`) so a user's screens are never listed twice. But **`read` deliberately keeps the `SCREENS_DIR` overlay**, so an upgraded install's customized `default/screen.svg` keeps rendering — which means `local/default` still *shadows* the builtin fallback on read. Documented as a sharp edge in `authoring.md`. `svg_files`/`screen_files` stay merged on purpose (single consumer, `TemplateService::build_tera`; `copy_screen` gates on the narrowed `screen_paths`).
- **Option resolution for renders lives once**, in `src/api/display.rs` (`resolve_preview_dimensions`, `resolve_query_palette`, `resolve_ctx_palette`, `resolve_effective_tuning`, `resolve_dither_tuning`). Both `/dev/render`'s `handle_render` and `ScreenStore::render` call it. Any new knob goes there, once.
- **The migration and `local`'s registration agree by construction** — `present_screen_dirs` goes through `LocalScreenRepoSource::load` + `screen_paths()`, the same entry point registration uses.
- **`AssetLoader::read_screen_embedded_only`** exists solely so `EmbeddedBuiltinSource::load` reads its manifest without the overlay (otherwise a user's `local` manifest could unregister `byonk-builtin`). One caller — do not widen it.
- Startup order: `seed_and_migrate` (`main.rs:681`) → build state → serve, used by **both** `run_server` and `run_dev_server`.
- Seeding is four independently-fallible sections; a failed examples seed must never prevent config seeding. The local-manifest seed gate is "`byonk-screens.yaml` does not exist", not "dir is empty".

## Preconditions for Plan 2 (MCP) — close these in the commit that first exposes each surface

These are harmless today **only** because nothing routes to `ScreenStore`:
1. **`MAX_FILE_BYTES` is not enforced on `validate`'s reads** (only `read_file` caps). A git-fetched or Samba-dropped file is unbounded → an API-triggered `validate` is an unbounded read into memory.
2. **The disk sources' `read` follows symlinks** — `GitScreenRepoSource::read` / `LocalScreenRepoSource::read` apply only a lexical `is_safe_rel` check, so a repo containing `leak -> /etc/passwd` leaks it. Add canonicalize-and-prefix-check. (The *write* path is already fully guarded.)
3. **`ScreenStore` has no internal serialization** — `create_screen`/`rename_screen` have TOCTOU windows, and two concurrent creates can interleave such that one's cleanup deletes the other's work. MCP and UI callers are concurrent.

## Known parked items (non-blocking, reviewed and judged)

- `docs/src/tutorial/first-screen.md:67,187` and `docs/src/api/admin-api.md:537,573` still show `byonk: "0.15"`, and `first-screen.md:67` mislabels it "minimum engine version" — it is a caret range, so `"0.15"` *excludes* 0.17.x. An author following the tutorial hand-writes the exact false compat warning the fix wave removed. **~4-line docs fix, worth doing soon.**
- An explicit `local: { path: /elsewhere }` in config suppresses `SCREENS_DIR` auto-registration, but the migration (which runs before config load) would still rewrite refs to `local/x`. Exotic; the add-on path is blocked by the reserved-handle check.
- `svg_files` staying merged means each `byonk-builtin` render registers every `.svg` under `SCREENS_DIR` as a Tera template — correctness-neutral, O(SCREENS_DIR) reads. Pre-existing.
- 5 `clippy --all-targets` warnings, all pre-existing before this branch, all in test code; `make check` does not run that form.
- The HA app's options schema (`AddonScreenRepo`) still has no `path:` shape. Deliberate: `apply_to_config` runs *after* config validation so a `path`+`repo` pair would bypass `ScreenRepoRef::validate`, and `build_disk_sources` checks `path` first so it would silently win — adding the field alone is unsafe. `local`/`examples` auto-register with zero config, so nothing is blocked.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests (**519 tests green on `9ab8b0b`**). `make docs` builds clean (needs `cargo install mdbook-mermaid` once).
- If `cargo` is not found, add `$HOME/.cargo/bin` to `PATH` — the toolchain is rustup-managed via `rust-toolchain.toml` (never add cargo/rust to mise).
- **Global constraints:** never `git add -A`/`.` (stage explicit paths, verify `git diff --cached`); CHANGES.md is user-facing only; `byonk-base-v1` untouched; scratch release image (no `/tmp`, state under `/data`).
- **After merge:** validate on the HA VM — the app's options-schema sync gotcha is in memory `ha-vm-addon-manifest-sync-gap`; the from-source build recipe is in `ha-vm-from-source-addon-build`.

## Process notes worth keeping

- Execution used **superpowers:subagent-driven-development** (v6.2.0): per task, brief → implementer → review package → task reviewer → fix loop → ledger line. The ledger lived at `.superpowers/sdd/2026-07-26-screen-store-authoring-core/progress.md` (git-ignored; deleted once the branch was finished).
- **Reviews on opus repeatedly found real defects a sonnet implementer missed** — including a migration that would have rewritten `byonk-builtin/default` on every genuinely-upgraded install while all seven of its tests passed, because the fixtures never reproduced a real upgraded `SCREENS_DIR`. Keep reviewing the mutating and migration surfaces at that tier.
- Subagents whose transcripts exceeded ~230k tokens stalled or died twice; handing findings over as a **file** and dispatching a fresh implementer was the reliable recovery.
