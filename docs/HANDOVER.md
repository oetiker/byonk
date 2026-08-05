# Handover — Byonk

_Last updated: 2026-07-29 — **Plan 2 (MCP interface) is mid-execution: Tasks 1–7 of 12 are complete and individually reviewed clean. Task 8 is IN FLIGHT with uncommitted work on disk.** Branch `feat/screen-store-authoring-core`, HEAD `c4cce24`, 44 commits ahead of `origin/main` (`67b3855`), local-only (never pushed). The branch remains **HELD** by user decision — no merge, no PR. `make check` green at `c4cce24`._

## Resume here

1. **Read the SDD ledger first: `.superpowers/sdd/2026-07-28-screen-store-mcp-interface/progress.md`** (git-ignored). It has a line per task with commit ranges, every review verdict, every deferred minor, and the pre-flight rulings. Trust it plus `git log` over memory.
2. **Deal with Task 8's uncommitted work** (see next section) before anything else.
3. Continue with **superpowers:subagent-driven-development** at Task 8 (or 9 if you land 8), through Task 12, then the final whole-branch review.
4. The plan is `docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md`. Extract per-task briefs with the skill's `scripts/task-brief PLAN_FILE N` — never hand a subagent the whole plan.

## Task 8 — in flight, uncommitted

`render_screen` + `validate_screen`. The implementer was **stopped by the user** just before it ran `make check` and committed. Its last report was that it had already verified test discrimination (deliberately broke the code, watched the tests fail, restored). Nothing is committed, so git is clean at `c4cce24`.

On disk, unstaged:

- `src/mcp/tools_render.rs` — new, 172 lines (untracked)
- `src/mcp/mod.rs` — router wiring, +5/−1
- `tests/mcp_tools_test.rs` — +149 lines of new tests

`Cargo.toml` is untouched — `base64` was already a direct dependency.

**Unknown: why the user stopped it.** Ask before assuming. Either (a) finish it — run `cargo test --test mcp_tools_test -- --test-threads=4`, then `make check`, write `task-8-report.md`, commit; or (b) discard (`git checkout src/mcp/mod.rs tests/mcp_tools_test.rs && rm src/mcp/tools_render.rs`) and re-dispatch from the brief.

## The initiative

Turn byonk into a place where screens are **authored**, not just served, and make an LLM a first-class author that can develop screens against a byonk running **anywhere** — including the HA app inside Home Assistant — over the LAN, with no filesystem access and no Samba share.

- **Spec 1** — `docs/superpowers/specs/2026-07-24-screen-store-and-mcp-design.md`, split in two:
  - **Plan 1 — Authoring core** — DONE (13/13), earlier on this branch.
  - **Plan 2 — MCP interface** — `docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md`, **7/12 done**.
- **Spec 2 — Svelte web UI at `/`** (not written). Consumes the same `ScreenStore`.
- **Spec 3 — Git commit & history** (not written).

## What Plan 2 has shipped so far

| Task | Commits | What landed |
|---|---|---|
| 1 | `6094b49..1136500` | Symlink-escape guard on **all three** disk read paths (Git, Local, and the `SCREENS_DIR` overlay in `AssetLoader::read_screen`). Check and read resolve the **same** canonicalized path. |
| 2 | `11d7cd3..a848a02` | `ReadOutcome` + `ScreenRepoSource::read_limited`; disk sources `stat` before reading. `read_file`, `validate` and `copy_screen_files` all bounded. Non-UTF-8 and oversized now report distinctly. |
| 3 | `60fbf7c..592fdc9` | `mutation_lock: Mutex<()>` on `ScreenStore`; every mutator takes it first, `validate`/`render` deliberately do not. |
| 4 | `592fdc9..77fca03` | `ScreenStore::list_screens` (the one place writability is derived, **structurally**) + `delete_file` (refuses the three defining files). |
| 5 | `77fca03..9c8d2fe` | `rmcp` 2.2 dep; `/mcp` mounted on the main router behind `require_admin`. No tools. |
| 6 | `9c8d2fe..031781a` | 5 read tools; `ScreenRepoKind`/`kind()`; `blocking`/`ok_json`/`store_failure` helpers; `redacted_config` extracted so admin + MCP share one redaction. |
| 7 | `031781a..c4cce24` | 6 edit tools. Plus `abce999`, a separate fix: `TestApp::new_admin_with_screens` never seeded the manifest, so `local` never registered. |

Remaining: **8** render/validate · **9** `assign_screen` · **10** `meta.yaml` JSON Schema · **11** MCP resources · **12** docs + CHANGES.

## Decisions already settled — do not re-litigate

Established by reading the vendored `rmcp` 2.2 source (at `<scratchpad>/rmcp-2.2.0/`, re-fetch with `curl -sL https://static.crates.io/crates/rmcp/rmcp-2.2.0.crate | tar xz` if gone).

- **`rmcp` 2.2**, the latest *stable*. `3.0.0-beta.4` is a prerelease — out of scope.
- **`.disable_allowed_hosts()`** — rmcp defaults `allowed_hosts` to loopback only, which would reject the entire LAN/HA use case. The Bearer token already defeats DNS rebinding. **User-approved.**
- **Stateless** (`stateful_mode: false`, `json_response: true`, `NeverSessionManager`).
- **Tool failures are `Ok(CallToolResult::error(...))`, never `Err(ErrorData)`.** Clients render protocol errors opaquely, so the model never sees an `Err`'s message. `StoreError::ReadOnly`'s `copy_hint` is the agent's instruction for what to do next — it must arrive as visible content. `Err` is only for a panicked `spawn_blocking` join or a serialize failure.
- **`validate_screen` reporting `ok: false` is a SUCCESSFUL call** — not `is_error`. A failed **render** is `is_error: true` but must still carry its diagnostics.
- **Never `Implementation::from_build_env()`** — its `env!` expands inside rmcp, reporting rmcp's name/version.
- **`CallToolResult` and several other rmcp types are `#[non_exhaustive]`** — struct literals fail with E0639. Use `success()`/`error()` + public-field assignment. There is no `with_structured_content`.
- **`#[tool_router(router = x, vis = "pub")]` generates an ASSOCIATED function** — combine as `Self::tools_read_router() + Self::tools_edit_router() + …`, not the module-level free-function form the rmcp docs imply.
- **`schemars` only via `rmcp::schemars`.** A separate dep risks version skew.
- **Every `ScreenStore` call from an async handler goes through the `blocking` helper.**
- Every POST to `/mcp` must carry `Accept: application/json, text/event-stream` (else 406) **and** a `Host` header — rmcp parses `Host` before consulting the allowlist, so it 400s when there is neither `Host` nor a URI authority. Real clients always supply one; the in-process test harness does not, so `tests/common/mcp.rs` sets both.

## Load-bearing invariants — do not break these

- **`ScreenStore::new` must get the SAME `Arc<ScreenRepoManager>` the `ContentPipeline` has.** Guarded by `tests/screen_store_wiring_test.rs`.
- **The `byonk-builtin` handle string is frozen** — `content_pipeline.rs:215` hard-references `byonk-builtin/default`.
- **`byonk-builtin` enumerates embedded-only, but `read` keeps the `SCREENS_DIR` overlay** — so it touches the filesystem despite the name. This exact mismatch caused a real defect in Task 2 (its `read_limited` took the read-then-check default on the grounds that "embedded contents are resident in the binary", which is false for this source). Any new "embedded sources are safe" reasoning must account for it.
- **Writability is structural** — derived from `writable_root()`, never from a handle's name. Same for `ScreenRepoKind`.
- **`ScreenStore`'s mutex is `std::sync::Mutex` and not reentrant.** No mutating method may call another; each MCP tool makes exactly one store call.
- **Option resolution for renders lives once**, in `src/api/display.rs`. Both `/dev/render` and `ScreenStore::render` call it.

## Deferred minors — the final review must triage these

All are logged with full context in the ledger. The ones with real teeth:

1. **The Task 3 concurrency gate is probabilistic** — no `std::sync::Barrier` before the concurrent create, so on a 1-vCPU or loaded runner it could pass against *unlocked* code. It is the only test gating that task. **Add a barrier.**
2. **No test pins the structural writability rule** — the listing test uses only canonically-named handles, so `writable = handle == "local"` would pass it. Load-bearing for every edit tool. **Add a handle named `local` backed by a read-only source.**
3. **Binary screen assets cannot be written over MCP** — `WriteFileArgs.content` is `String`, though `ScreenStore::write_file` takes bytes. No `background.jpg`, no custom font. Either base64 support or an explicit doc note.
4. **`stat`-then-`read` is two syscalls** on a user-writable dir; a final-component swap between them can still deliver oversized bytes. `File::open` + `Read::take(max+1)` would bound it unconditionally.
5. `delete_screen_file` ships with no end-to-end test; no `tools/list` assertion anywhere.
6. `AssetScreensSource` (`lua_runtime.rs:97`) reads via the disk overlay without overriding `read_limited`, contradicting the trait contract. Private/dev-only today.
7. `list_screen_repos` hides a *configured* repo whose manifest is missing/unloadable, while the admin endpoint lists it.
8. `kind()` defaults to `Embedded`, so a future source that forgets the override silently reports the most restrictive kind.
9. Pre-existing, out of scope: `resolve_manifest_root` joins the untrusted manifest `root:` field with no `is_safe_rel` check, so a git-fetched repo can steer the read guard's root.

## Process notes that earned their place this run

- **Every task so far had its real defect in the PLAN's text, not the implementation.** Five tasks, five plan defects — a TOCTOU in a guard, a false "embedded is in-binary" claim, a wrong return-shape assumption, uncompilable struct literals, a signature with nowhere to report an error. The plan's code blocks are specific enough to look authoritative and are not. **Reviews are what is making this work; do not weaken them.**
- **The single most valuable catch was a test that could not fail.** `test_get_config_redacts_secrets` passed with every line of redaction deleted, because its fixture set the admin token on the in-memory config while the code read the config *file*. It looked correct and named the right property. **Ask of every security-relevant test: would this fail against broken code?** Where it matters, break the code locally and watch it fail — implementers were asked to do this and it worked.
- **Implementers disclosing weak tests is now the norm** — four separate agents caught non-discriminating tests in their own work and said so. Keep telling them that disclosure is valued, not penalised.
- **Reviewers verifying claims empirically** (rebuilding against pre-fix code, neutralizing overrides) caught things reading alone would not. Keep asking for it.
- **Do not tell an implementer "verified, do not correct from memory" about a third-party API.** Two such facts were wrong; the implementer checked the vendored source and was right. The crate source is the authority.
- Subagents stall or die past roughly ~150–230k transcript tokens; dispatch fresh rather than resuming a large one. Several agents also stalled by spawning background jobs and Monitors — **instruct them to run everything in the foreground**.
- Reviews on large files died repeatedly during open-ended exploration. **Give reviewers a call budget and tell them to prefer `grep -n` / ranged `sed` over whole-file reads** (`screen_store.rs` is ~1200 lines).
- Keep unrelated lint/fixture fixes in **separate commits** — one task mixed them in and the review flagged it; the next task split them correctly.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. `make docs` needs `cargo install mdbook-mermaid` once.
- If `cargo` is missing, add `$HOME/.cargo/bin` to `PATH` — rustup-managed via `rust-toolchain.toml` (never add cargo/rust to mise).
- **Cap parallelism at 4** for compiles and test runs — shared machine.
- Never `git add -A`/`.` — stage explicit paths, verify `git diff --cached`. CHANGES.md is user-facing only.
- **After Plan 2:** connect a real MCP client against a local `byonk serve` and drive list → copy → edit → render → assign (a green suite does not prove a real client negotiates the handshake), then validate on the HA VM per memories `ha-vm-from-source-addon-build` and `ha-vm-addon-manifest-sync-gap`.

## Uncommitted working-tree state (unrelated to Plan 2)

Agent-setup housekeeping, deliberately not committed:

- `.gitignore` — un-ignores `.claude/skills/`
- `CLAUDE.md` — trimmed; HA VM detail moved into the `ha-vm-testing` skill
- `.claude/skills/ha-vm-testing/SKILL.md` — new, untracked

Land as one small commit whenever convenient. Do not let these get swept into a task commit.
