# Handover — Byonk

_Last updated: 2026-07-27 — **Screen-authoring initiative: executing Plan 1 (authoring core) via subagent-driven development. Tasks 1–7 of 13 DONE and reviewed-clean; Task 8 (`ScreenStore::validate`) is next.** Branch `feat/screen-store-authoring-core` (off `chore/rust-toolchain-pin` @ `12385da`), HEAD `5b04d4d`, local-only (not pushed). All gates green through Task 7._

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

1. **Ledger is the source of truth:** `cat .superpowers/sdd/2026-07-26-screen-store-authoring-core/progress.md` (git-ignored). Note the path: the ledger moved from the old flat `.superpowers/sdd/progress.md` into a **per-plan workspace** (the old file is left as `progress.md.migrated-to-workspace`; ignore it). The ledger lists each task's status, commit range, deferred minors, and the durable facts later tasks need. Tasks with a `Task <N>: complete` line are DONE — do not re-dispatch. `git log --oneline 12385da..HEAD` corroborates.
2. **Resume execution** with the **superpowers:subagent-driven-development** skill (v6.2.0), continuing at **Task 8**. Per task: `scripts/task-brief PLAN N` → dispatch implementer subagent (fresh, model per task) → `scripts/review-package PLAN BASE HEAD` → dispatch task reviewer → fix loop → ledger line. Scripts live in the skill dir: `/Users/oetiker/.claude/plugins/cache/claude-plugins-official/superpowers/6.2.0/skills/subagent-driven-development/scripts/` (run from the repo root).
3. **BASE for Task 8's review package = current HEAD `5b04d4d`.**
4. Task briefs, implementer reports, and review packages for tasks 6–7 are in the workspace dir alongside the ledger — useful context, not required reading.

## Done (Tasks 1–7, all reviewed clean)

**Tasks 1–5 — "typed writable sources"** (details in the ledger):
- **T1 `002a21a`** axum 0.7→0.8 (+ swagger-ui 8→9), all `:param` routes → `{param}`.
- **T2 `2a6e62a`** `writable_root() -> Option<&Path>` on `ScreenRepoSource`; `DiskScreenRepoSource` → `GitScreenRepoSource`.
- **T3 `2d5006a`** `LocalScreenRepoSource` (writable); shared disk-walk helpers extracted.
- **T4 `4b2e143`** `ScreenRepoRef.path` variant + `validate()` (rejects `repo`+`path`), wired inside `AppConfig::load_from_assets`.
- **T5 `87e4919`** `path:` entries + `SCREENS_DIR` register as writable local sources; `SCREENS_DIR` auto-registers as handle **`local`** unless config declares one. Added `AssetLoader::screens_dir()`, `server::build_screen_repo_manager`, `DiskSource{Git,Local}`, `ScreenRepoLoader::source_for(handle)`.

**Task 6 `27252b6` + `afb5fdb`** — `ScreenStore` (`src/services/screen_store.rs`): `read_file`/`write_file`, blake3 etags, `StoreError{ReadOnly{copy_hint},NotFound,Conflict,Traversal,TooLarge,Io}`. Opus review found 7 Important defects, all fixed + tested:
- `split_ref` now validates **both** halves via `safe_rel` (an empty tail let a caller overwrite the repo manifest).
- Write path is strictly **verify-before-mutate**: resolve root once → canonicalize base → canonicalize deepest existing ancestor of the target's parent → prefix check → `create_dir_all` → unique tmp (`<file>.byonk-tmp-<pid>-<rand>`) → `rename` → `rebuild_loader()`. A non-canonicalizable base is a hard error, never a skipped check.
- `if_match` against a deleted file returns `Conflict` (was silently resurrecting it).
- **`LocalScreenRepoSource::writable_root()` returns `manifest_root`, not the bare `root`** — write and read now share a base (the struct's `root` field is gone).
- `rand` is now used in production code (already a normal dependency); `blake3` added.

**Task 7 `5211927` + `5b04d4d`** — `create_screen`/`copy_screen`/`rename_screen`/`delete_screen` + `StarterKind::Minimal`. Opus review found 2 Important defects, both fixed + tested:
- `copy_screen` now `safe_rel`s the source-supplied path suffix (the canonicalize guard provably misses a lexical `..` when the destination dir doesn't exist yet).
- `EmbeddedBuiltinSource::screen_files` merges the `list_screens()` set with a `walk_files_under()` walk of the `SCREENS_DIR` overlay — otherwise forking an overlay screen silently dropped its images (`collect_screen_files` filters to lua/svg/yaml). `AssetLoader::collect_screen_files` was deliberately **not** widened.
- New trait method **`ScreenRepoSource::screen_files(&self, screen_path) -> Vec<String>`, no default impl, 8 implementors** (incl. test stubs in `lua_runtime.rs` / `template_service.rs` — those two files' edits are required, not scope creep).
- Starter templates are three `&str` consts in `screen_store.rs`; `assets.rs` untouched, no new embed root. `create_screen`/`copy_screen` clean up their destination on partial failure; `MAX_FILE_BYTES` enforced on copy.

## Next (Tasks 8–13)

- **T8** `validate` (meta schema + Lua compile-only + SVG/include resolution) → `ValidationReport{ok, issues}`.
- **T9** `render` with diagnostics (png + raw + captured Lua `log_*` + returned data + line-numbered Lua error). Touches `lua_runtime.rs` to capture logs per-run.
- **T10** split embedded `screens/` → minimal `byonk-builtin` (`default` + `calibration/*`) + shipped `examples`. ⚠️ Plan Step 5 says `git add -A screens/` — **override to explicit-path staging** (constraint: no `git add -A`).
- **T11** seed `local` manifest + `examples` repo; stop copying builtin screens into SCREENS_DIR.
- **T12** one-time migration: rewrite old `byonk-builtin/<user-screen>` device refs → `local/<x>` (genuine builtins untouched); idempotent.
- **T13** wire `ScreenStore` into `AppState` + docs (`docs/src/guide/authoring.md`) + CHANGES.md.

## Key decisions (durable)

- **Three built-in layers, kept separate:** (1) `byonk-base-v1` include library — embedded, universal, versioned, read-only, **untouched by this work**; (2) `byonk-builtin` repo — minimal read-only `default` + `calibration/*`; (3) `examples` — shipped, seeded to disk, editable. Writability is a **structural property of the source** (`writable_root()`), never a name check.
- **`byonk-builtin` handle string is frozen** (device configs reference it). Only what it embeds changes.
- MCP (Plan 2) = `rmcp` streamable-HTTP at `/mcp` on the main router, admin-token gated (same `require_admin` Bearer as `/api/admin/*`). That's why axum 0.8 was a prerequisite.
- Reviews on the security-bearing store tasks (6, 7) ran on **opus** and both found real defects the sonnet implementer missed — keep reviewing the mutating surface at that tier.
- After the whole plan: validate the HA add-on options-schema change on the VM (memory `ha-vm-addon-manifest-sync-gap`).

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests (green through T7; 23 `screen_store` tests). `make docs` must stay clean.
- **Global constraints (bind every task):** never `git add -A`/`.` (stage explicit paths, verify `git diff --cached`); CHANGES.md = user-facing only; `byonk-base-v1` untouched; scratch release image (no `/tmp`, on-disk state under `/data`); Rust toolchain via rustup/`rust-toolchain.toml`.

## After Plan 1

Final whole-branch review (opus, `superpowers:requesting-code-review`) — point it at the ledger's `minor (deferred)` lines for triage — then `superpowers:finishing-a-development-branch`. Then write Plan 2 (MCP) and Spec 2/3.
