# Handover — Byonk

_Last updated: 2026-07-26 — **Screen-authoring initiative: executing Plan 1 (authoring core) via subagent-driven development. Tasks 1–5 of 13 DONE and reviewed-clean; Task 6 (ScreenStore read/write) is next.** Branch `feat/screen-store-authoring-core` (off `chore/rust-toolchain-pin` @ `12385da`), HEAD `87e4919`, local-only (not pushed). All gates green through Task 5._

## The initiative

Turn byonk into a place where screens are **authored**, not just served, and make an LLM (Claude Code) a first-class author that can develop screens against a byonk running **anywhere** — including the HA add-on inside HA — over the LAN, with no filesystem access and no Samba share. Creating a screen means editing three languages (yaml + lua + svg); today that's read-only, hand-edited over a share.

Design is split into **3 specs** (spec 1 is in flight):
- **Spec 1 — Screen store core + MCP** (`docs/superpowers/specs/2026-07-24-screen-store-and-mcp-design.md`). A typed writable screen-source model + a shared `ScreenStore` mutation/validate/render core + an always-on MCP interface at `/mcp`.
- **Spec 2 — Svelte web UI at `/`** (not written). Always-on SPA, three-pane editor, live preview, full `/dev` parity, then retire `/dev` + `byonk dev`.
- **Spec 3 — Git commit & history** (not written). gix write path over local repos that are git working copies.

Spec 1 is itself split into **2 implementation plans**:
- **Plan 1 — Authoring core** (`docs/superpowers/plans/2026-07-26-screen-store-authoring-core.md`, 13 tasks): axum bump → typed writable sources → `ScreenStore` → builtin/examples split + migration. **← executing now.**
- **Plan 2 — MCP interface** (not written): Component 5 of spec 1, on top of Plan 1's finished core.

## How to resume (READ FIRST)

1. **Ledger is the source of truth:** `cat .superpowers/sdd/progress.md` (git-ignored). It lists each task's status + commit range. Tasks marked complete are DONE — do not re-dispatch. `git log --oneline 12385da..HEAD` corroborates.
2. **Resume execution** with the **superpowers:subagent-driven-development** skill, continuing at **Task 6**. The workflow: for each task — `scripts/task-brief PLAN N` → dispatch implementer subagent (fresh, model per task) → `scripts/review-package BASE HEAD` → dispatch task reviewer → fix loop if needed → append one line to the ledger. Scripts live in the skill dir: `/Users/oetiker/.claude/plugins/cache/claude-plugins-official/superpowers/6.1.1/skills/subagent-driven-development/scripts/` (run them from the repo root — they resolve the repo via cwd).
3. BASE for Task 6's review package = current HEAD `87e4919`.

## Done (Tasks 1–5 — "typed writable sources" component, all reviewed clean)

- **T1 `002a21a`** axum 0.7→0.8 (+ swagger-ui 8→9). Found/fixed 3 extra `:param` routes in `api/admin/mod.rs` beyond the plan's list. No utoipa bump needed.
- **T2 `2a6e62a`** `writable_root() -> Option<&Path>` (default `None`) on `ScreenRepoSource`; renamed `DiskScreenRepoSource` → `GitScreenRepoSource`.
- **T3 `2d5006a`** `LocalScreenRepoSource` (writable; `writable_root()=Some(root)`). Shared `walk_screen_paths`/`walk_ext_files`/`resolve_manifest_root` extracted (Git behavior preserved). `tempfile` added as dev-dep.
- **T4 `4b2e143`** `ScreenRepoRef.path` variant + `validate()` (rejects `repo`+`path` together), wired **inside** `AppConfig::load_from_assets` so no caller can bypass.
- **T5 `87e4919`** [opus-reviewed] `path:` entries + `SCREENS_DIR` register as writable local sources. `SCREENS_DIR` auto-registers as handle **`local`** unless config declares one (explicit wins). Key design: `AssetLoader::screens_dir()` accessor + `server::build_screen_repo_manager` derivation (no `main.rs` threading); shared `build_disk_sources()` so initial-snapshot == every-rebuild; `DiskSource{Git,Local}` enum; `ScreenRepoLoader::source_for(handle)` added (Task 6 depends on it). `rebuild_loader` no longer skips `path:` entries.

## Next (Tasks 6–13)

- **T6** `ScreenStore` (`src/services/screen_store.rs`): `read_file`/`write_file` with blake3 **etag** optimistic concurrency + path-traversal (canonicalize-then-verify-prefix) safety; `StoreError{ReadOnly{copy_hint},NotFound,Conflict,Traversal,TooLarge,Io}`. Adds `blake3` dep. Consumes `manager.loader().source_for()` + `writable_root()`.
- **T7** create/copy/rename/delete screens (copy = fork-to-edit for read-only sources).
- **T8** `validate` (meta schema + Lua compile + SVG/include resolution).
- **T9** `render` with diagnostics (png + raw + captured Lua `log_*` + returned data + line-numbered Lua error). Touches `lua_runtime.rs` to capture logs per-run.
- **T10** split embedded `screens/` → minimal `byonk-builtin` (`default` + `calibration/*`) + shipped `examples` (hello, mandelbrot, webscrape, gphoto, swiss-departure-board, demo/font). ⚠️ Plan Step 5 uses `git add -A screens/` — **override to explicit-path staging** (constraint: no `git add -A`).
- **T11** seed `local` manifest + `examples` repo; stop copying builtin screens into SCREENS_DIR.
- **T12** one-time migration: rewrite old `byonk-builtin/<user-screen>` device refs → `local/<x>` (genuine builtins untouched); idempotent.
- **T13** wire `ScreenStore` into `AppState` + docs (`docs/src/guide/authoring.md`) + CHANGES.md.

## Key decisions (durable)

- **Three built-in layers, kept separate:** (1) `byonk-base-v1` include library — embedded, universal, versioned, read-only, **untouched by this work**; (2) `byonk-builtin` repo — minimal read-only `default` + `calibration/*`; (3) `examples` — shipped, seeded to disk, editable. Writability is a **structural property of the source** (`writable_root()`), never a name check.
- **`byonk-builtin` handle string is frozen** (device configs reference it). Only what it embeds changes.
- MCP (Plan 2) = `rmcp` streamable-HTTP at `/mcp` on the main router, admin-token gated (same `require_admin` Bearer as `/api/admin/*`). That's why axum 0.8 was a prerequisite.
- After the whole plan: validate the HA add-on options-schema change on the VM (memory `ha-vm-addon-manifest-sync-gap`).

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests (green through T5). `make docs` must stay clean.
- **Global constraints (bind every task):** never `git add -A`/`.` (stage explicit paths, verify `git diff --cached`); CHANGES.md = user-facing only; `byonk-base-v1` untouched; scratch release image (no `/tmp`, on-disk state under `/data`); Rust toolchain via rustup/`rust-toolchain.toml`.

## After Plan 1

Final whole-branch review (opus, `superpowers:requesting-code-review`), then `superpowers:finishing-a-development-branch`. Then write Plan 2 (MCP) and Spec 2/3.
