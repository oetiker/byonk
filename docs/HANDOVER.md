# Handover — Byonk

_Last updated: 2026-08-05 — **Plan 2 (MCP interface) is at 9/12. Tasks 1–9 are complete and individually reviewed clean.** Branch `feat/screen-store-authoring-core`, HEAD `3bc4820`, 50 commits ahead of `origin/main` (`67b3855`), local-only (never pushed). The branch remains **HELD** by user decision — no merge, no PR. `make check` green at `3bc4820`. Working tree clean apart from the three housekeeping items noted at the bottom._

## Resume here

1. **Read the SDD ledger first: `.superpowers/sdd/2026-07-28-screen-store-mcp-interface/progress.md`** (git-ignored). One line per task with commit ranges, every review verdict, every deferred minor, every human ruling. Trust it plus `git log` over memory.
2. Continue with **superpowers:subagent-driven-development** at **Task 10**, through Task 12, then the final whole-branch review.
3. The plan is `docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md`. Extract per-task briefs with the skill's `scripts/task-brief PLAN_FILE N` — never hand a subagent the whole plan.

Remaining: **10** `meta.yaml` JSON Schema · **11** MCP resources · **12** docs + CHANGES.

## The initiative

Turn byonk into a place where screens are **authored**, not just served, and make an LLM a first-class author that can develop screens against a byonk running **anywhere** — including the HA app inside Home Assistant — over the LAN, with no filesystem access and no Samba share.

- **Spec 1** — `docs/superpowers/specs/2026-07-24-screen-store-and-mcp-design.md`, split in two:
  - **Plan 1 — Authoring core** — DONE (13/13), earlier on this branch.
  - **Plan 2 — MCP interface** — `docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md`, **9/12 done**.
- **Spec 2 — Svelte web UI at `/`** (not written). Consumes the same `ScreenStore`.
- **Spec 3 — Git commit & history** (not written).

## What Plan 2 has shipped

| Task | Commits | What landed |
|---|---|---|
| 1 | `6094b49..1136500` | Symlink-escape guard on **all three** disk read paths. Check and read resolve the **same** canonicalized path. |
| 2 | `11d7cd3..a848a02` | `ReadOutcome` + `ScreenRepoSource::read_limited`; disk sources `stat` before reading. Non-UTF-8 and oversized report distinctly. |
| 3 | `60fbf7c..592fdc9` | `mutation_lock: Mutex<()>` on `ScreenStore`; every mutator takes it first, `validate`/`render` deliberately do not. |
| 4 | `592fdc9..77fca03` | `ScreenStore::list_screens` (the one place writability is derived, **structurally**) + `delete_file`. |
| 5 | `77fca03..9c8d2fe` | `rmcp` 2.2 dep; `/mcp` mounted on the main router behind `require_admin`. No tools. |
| 6 | `9c8d2fe..031811a` | 5 read tools; `ScreenRepoKind`/`kind()`; `blocking`/`ok_json`/`store_failure` helpers; `redacted_config` shared with admin. |
| 7 | `031811a..c4cce24` | 6 edit tools. Plus `abce999`: `TestApp::new_admin_with_screens` never seeded the manifest, so `local` never registered. |
| 8 | `86c002b..618ccac` | `render_screen` + `validate_screen` with diagnostics. Plus `69cb735`, an unrelated test-isolation fix. |
| 9 | `618ccac..3bc4820` | `assign_screen`, sharing `apply_device_patch`/`apply_device_add` with the REST handlers. |

## Decisions already settled — do not re-litigate

Established by reading the vendored `rmcp` 2.2 source (at `<scratchpad>/rmcp-2.2.0/`, re-fetch with `curl -sL https://static.crates.io/crates/rmcp/rmcp-2.2.0.crate | tar xz` if gone).

- **`rmcp` 2.2**, the latest *stable*. `3.0.0-beta.4` is a prerelease — out of scope.
- **`.disable_allowed_hosts()`** — rmcp defaults to loopback only, which would reject the entire LAN/HA use case. The Bearer token already defeats DNS rebinding. **User-approved.**
- **Stateless** (`stateful_mode: false`, `json_response: true`, `NeverSessionManager`).
- **Tool failures are `Ok(CallToolResult::error(...))`, never `Err(ErrorData)`.** Clients render protocol errors opaquely, so the model never sees an `Err`'s message. `Err` is only for a panicked `spawn_blocking` join or a serialize failure.
- **`validate_screen` reporting `ok: false` is a SUCCESSFUL call** — not `is_error`. A failed **render** is `is_error: true` but must still carry its diagnostics.
- **Never `Implementation::from_build_env()`** — its `env!` expands inside rmcp, reporting rmcp's name/version.
- **`CallToolResult` and several other rmcp types are `#[non_exhaustive]`** — struct literals fail with E0639. Use `success()`/`error()` + public-field assignment. There is no `with_structured_content`.
- **`#[tool_router(router = x, vis = "pub")]` generates an ASSOCIATED function** — combine as `Self::tools_read_router() + Self::tools_edit_router() + …`, not the module-level free-function form the plan keeps implying. **The plan has now got this wrong in three separate task briefs.**
- **`schemars` only via `rmcp::schemars`.**
- **Every `ScreenStore` call from an async handler goes through the `blocking` helper.**
- Every POST to `/mcp` must carry `Accept: application/json, text/event-stream` (else 406) **and** a `Host` header — rmcp parses `Host` before consulting the allowlist. Real clients always supply one; `tests/common/mcp.rs` sets both.

### Human rulings made during execution

- **Task 9 — `assign_screen` creates a mapping only for a REGISTRY-SEEN device.** It is an upsert (one-call onboarding for a device that has polled `/api/setup`), but an unseen or typo'd MAC is refused. Without the gate a typo persisted a phantom device to `config.yaml` and there is no MCP delete tool to undo it. This **overrides the brief's** "the device must already be known", which was self-contradictory (see below). Output carries `created: bool` so an agent can tell a create from an update.

## Load-bearing invariants — do not break these

- **`ScreenStore::new` must get the SAME `Arc<ScreenRepoManager>` the `ContentPipeline` has.** Guarded by `tests/screen_store_wiring_test.rs`.
- **The `byonk-builtin` handle string is frozen** — `content_pipeline.rs:215` hard-references `byonk-builtin/default`.
- **`byonk-builtin` enumerates embedded-only, but `read` keeps the `SCREENS_DIR` overlay** — so it touches the filesystem despite the name. This exact mismatch caused a real defect in Task 2. Any new "embedded sources are safe" reasoning must account for it.
- **Writability is structural** — derived from `writable_root()`, never from a handle's name. Same for `ScreenRepoKind`.
- **`ScreenStore`'s mutex is `std::sync::Mutex` and not reentrant.** No mutating method may call another; each MCP tool makes exactly one store call.
- **Option resolution for renders lives once**, in `src/api/display.rs`. Both `/dev/render` and `ScreenStore::render` call it.
- **Device writes must never call `require_writable_global`** — a device mapping is not global config, so it stays writable in HA add-on mode. True of `apply_device_patch`, `apply_device_add`, and `assign_screen`.

## Deferred minors — the final review must triage these

All logged with full context in the ledger. The ones with real teeth:

1. **The Task 3 concurrency gate is probabilistic** — no `std::sync::Barrier` before the concurrent create, so on a loaded runner it could pass against *unlocked* code. It is the only test gating that task. **Add a barrier.**
2. **No test pins the structural writability rule** — the listing test uses only canonically-named handles, so `writable = handle == "local"` would pass it. **Add a handle named `local` backed by a read-only source.**
3. **Binary screen assets cannot be written over MCP** — `WriteFileArgs.content` is `String`. No `background.jpg`, no custom font. Either base64 support or an explicit doc note.
4. **`stat`-then-`read` is two syscalls**; a final-component swap between them can still deliver oversized bytes. `File::open` + `Read::take(max+1)` would bound it unconditionally.
5. `delete_screen_file` ships with no end-to-end test; **no `tools/list` assertion anywhere** — the now-15-tool registration is verified only indirectly by which tools the tests happen to call.
6. `AssetScreensSource` (`lua_runtime.rs:97`) reads via the disk overlay without overriding `read_limited`, contradicting the trait contract.
7. `list_screen_repos` hides a *configured* repo whose manifest is missing/unloadable, while the admin endpoint lists it.
8. `kind()` defaults to `Embedded`, so a future source that forgets the override silently reports the most restrictive kind.
9. Pre-existing, out of scope: `resolve_manifest_root` joins the untrusted manifest `root:` field with no `is_safe_rel` check.
10. **`Severity::Warning` is dead code** — every `Issue` `ScreenStore::validate` pushes is `Severity::Error`.
11. `include_raw`'s two image blocks are indistinguishable — ordering is the only signal and it is documented nowhere. Untested through MCP.
12. `RenderArgs.width`/`height` have no doc comments, so their schema descriptions are empty while siblings have them.
13. **A device whose `config.devices` key differs from its MAC** — `assign_screen` passes `mac` straight through as the config key, so a mismatch may create a duplicate entry. Flagged by the Task 9 reviewer, **unverified**.
14. `validate_params` iterates only schema fields, so params carried across a screen change are never rejected for unknown keys — stale keys accumulate in `config.yaml`. Pre-existing.
15. **CHANGES.md has no MCP entry at all** — no task in this plan has touched it. Task 12 owns this; do not let it slip.

## Process notes that earned their place

- **Every task so far had its real defect in the PLAN's text, not the implementation — nine for nine.** The plan's code blocks are specific enough to look authoritative and are not. Task 9's brief was even **internally inconsistent**: its own Step-1 test called `register_device` (registry entry only) and then `assign_screen`, but the `apply_device_patch` it specified requires a `config.devices` entry and 404s without one, so the brief's test could never pass against the brief's own design. **Reviews are what is making this work; do not weaken them.**
- **Tool descriptions are code.** They are the only contract an MCP client ever sees, and an agent acts on them. Task 8 shipped a `validate_screen` description promising it resolved the `{% include %}` chain — `TemplateService::validate_template` documents the opposite. Task 9's brief claimed assigning "replaces the device's params with the new screen's defaults" — there is no `meta.yaml` defaults lookup anywhere on that path. **Verify every behavioural claim in a description against the code, and fix the description, not the code.**
- **The single most valuable catch remains a test that could not fail.** Ask of every security- or contract-relevant test: would this fail against broken code? Task 8's reviewer broke *three* lines at once — `is_error`, the empty-PNG guard, and the `Severity` mapping — and all 16 tests stayed green. **Where it matters, break the code and watch it fail.** Both Task 8's and Task 9's implementers claimed to have done this; Task 8's claim proved false on review, Task 9's held up.
- **Implementers disclosing weak tests is now the norm and it keeps paying.** Task 9's implementer disclosed unprompted that the brief's two tests were blind to an "always create, never update" regression and added a third that catches it; the reviewer reproduced exactly that. Keep telling them disclosure is valued, not penalised.
- **Reviewers verifying claims empirically** (rebuilding against pre-fix code, breaking a line and restoring) caught what reading alone would not. Keep asking for it, and name the two highest-risk lines so a scoped re-review spends its budget well.
- **Do not tell an implementer "verified, do not correct from memory" about a third-party API.** The crate source is the authority.
- Subagents stall or die past roughly ~150–230k transcript tokens; dispatch fresh rather than resuming a large one. Several also stalled by spawning background jobs and Monitors — **instruct them to run everything in the foreground**.
- Reviews on large files died repeatedly during open-ended exploration. **Give reviewers a call budget and tell them to prefer `grep -n` / ranged `sed` over whole-file reads** (`screen_store.rs` is ~2400 lines, `api/admin/write.rs` is large too).
- Keep unrelated lint/fixture/test-isolation fixes in **separate commits** — Tasks 7, 8 and 9 all did this correctly.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. `make docs` needs `cargo install mdbook-mermaid` once.
- If `cargo` is missing, add `$HOME/.cargo/bin` to `PATH` — rustup-managed via `rust-toolchain.toml` (never add cargo/rust to mise).
- **Cap parallelism at 4** for compiles and test runs — shared machine.
- Never `git add -A`/`.` — stage explicit paths, verify `git diff --cached`. CHANGES.md is user-facing only.
- **Beware tests that derive a path via `..` from a temp dir.** Task 8's `make check` failure was `write_through_examples_handle_with_real_derived_dot_dot_root` resolving `<SCREENS_DIR>/../examples` to one shared `$TMPDIR/examples`; `seed_examples` only seeds a missing-or-empty dir, so leftovers from an earlier run starved every later run. Nest the temp dir under a private parent.
- **After Plan 2:** connect a real MCP client against a local `byonk serve` and drive list → copy → edit → render → assign (a green suite does not prove a real client negotiates the handshake), then validate on the HA VM per memories `ha-vm-from-source-addon-build` and `ha-vm-addon-manifest-sync-gap`.

## Uncommitted working-tree state (unrelated to Plan 2)

Agent-setup housekeeping, deliberately not committed and left alone across all of Plan 2:

- `.gitignore` — un-ignores `.claude/skills/`
- `CLAUDE.md` — trimmed; HA VM detail moved into the `ha-vm-testing` skill
- `.claude/skills/ha-vm-testing/SKILL.md` — new, untracked

Land as one small commit whenever convenient. Every task dispatch has warned implementers not to stage these; keep doing that until they land.
