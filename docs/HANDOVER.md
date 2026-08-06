# Handover — Byonk

_Last updated: 2026-08-06 — **Plan 2 (MCP interface) is at 11/12. Tasks 1–11 are complete and individually reviewed clean.** Branch `feat/screen-store-authoring-core`, HEAD `56eeba8`, 57 commits ahead of `origin/main` (`67b3855`), local-only (never pushed). The branch remains **HELD** by user decision — no merge, no PR. **Working tree clean.** `make check` and `make docs` both green at `56eeba8`._

## Resume here

1. **Read the SDD ledger first: `.superpowers/sdd/2026-07-28-screen-store-mcp-interface/progress.md`** (git-ignored). One line per task with commit ranges, every review verdict, every deferred minor, every human ruling. Trust it plus `git log` over memory.
2. Continue with **superpowers:subagent-driven-development** at **Task 12** — the last one — then the final whole-branch review.
3. The plan is `docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md`. Extract the brief with the skill's `scripts/task-brief PLAN_FILE 12` — never hand a subagent the whole plan.

Remaining: **12** docs + CHANGES.md. Then the final review.

## Next up — Task 12, and what it must not miss

BASE will be `56eeba8`. Task 12 owns documentation and the changelog.

**CHANGES.md still has NO MCP entry of any kind.** Eleven tasks have shipped an entire MCP server — 15 tools, resources, the `/mcp` mount — and not one of them touched the changelog, by design (each deferred it here). This is the single highest-risk omission in the whole plan; do not let it slip. CHANGES.md is **user-facing only** — no CI, tooling, or dev-process entries.

Do the same pre-flight the last two tasks got: read the brief, check every claim against the code, and raise plan conflicts **before** dispatching. It has paid every time.

### Dispatch rules that have earned their place

**Put the foreground rule on the FIRST line of the dispatch, not in a constraints list.** Task 11 lost two agents to tooling, neither to the work:

- **BSD `sed`.** Implementer #1 hung on `sed -i` (macOS sed needs a backup-suffix argument) and died to the watchdog. **Tell implementers to use Edit/Write and never `sed` to modify files.** Ranged `sed -n 'A,Bp'` for reading is fine.
- **Background Monitors.** Implementer #2 spawned a Monitor for `make check` and stopped dead — the exact thing its dispatch already forbade. It resumed cleanly via SendMessage and finished. Say "run everything in the foreground, wait in-process, use a generous Bash timeout — a cold Rust build plus the full suite takes many minutes and that is expected."

Also repeat, every time:

- The plan's code blocks are **not authoritative** — eleven tasks, eleven plan defects.
- **Verify every behavioural claim in a doc comment, tool description or resource description against the code**, and fix the description, not the code.
- **Verify test discrimination by breaking the code and watching it fail**, then restore. Disclosing a weak test is valued, not penalised — and it keeps working (see Task 10 below).
- `CARGO_BUILD_JOBS=4`, `-- --test-threads=4`, stage explicit paths, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.
- Give reviewers a **call budget** and tell them to prefer `grep -n` / ranged `sed` over whole-file reads.

## The final whole-branch review

Dispatch on the **most capable available model** using superpowers:requesting-code-review's `code-reviewer.md`, with a package from
`scripts/review-package docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md 67b3855 HEAD` (merge-base is `67b3855`).

**Point it at the deferred-minor list below and at the ledger** so it can triage which must be fixed before merge. If it returns findings: ONE fix dispatch with the complete list, then exactly one scoped re-review. No second fix wave.

## The initiative

Turn byonk into a place where screens are **authored**, not just served, and make an LLM a first-class author that can develop screens against a byonk running **anywhere** — including the HA add-on inside Home Assistant — over the LAN, with no filesystem access and no Samba share.

- **Spec 1** — `docs/superpowers/specs/2026-07-24-screen-store-and-mcp-design.md`, split in two:
  - **Plan 1 — Authoring core** — DONE (13/13), earlier on this branch.
  - **Plan 2 — MCP interface** — `docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md`, **11/12 done**.
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
| 10 | `34dcddf..eec4818` | `meta_json_schema()` generated from `RawMeta`, + `tests/screen_meta_schema_test.rs`. One fix round (below). |
| 11 | `56eeba8` | `src/mcp/resources.rs` + `EmbeddedDocs`; `list_resources`/`read_resource`. Review clean first pass. |

## Decisions already settled — do not re-litigate

Established by reading the vendored `rmcp` 2.2 source (at `<scratchpad>/rmcp-2.2.0/`, or `~/.cargo/registry/src/*/rmcp-2.2.0/`; re-fetch with `curl -sL https://static.crates.io/crates/rmcp/rmcp-2.2.0.crate | tar xz` if gone).

- **`rmcp` 2.2**, the latest *stable*. `3.0.0-beta.4` is a prerelease — out of scope.
- **`.disable_allowed_hosts()`** — rmcp defaults to loopback only, which would reject the entire LAN/HA use case. The Bearer token already defeats DNS rebinding. **User-approved.**
- **Stateless** (`stateful_mode: false`, `json_response: true`, `NeverSessionManager`).
- **Tool failures are `Ok(CallToolResult::error(...))`, never `Err(ErrorData)`.** Clients render protocol errors opaquely, so the model never sees an `Err`'s message. `Err` is only for a panicked `spawn_blocking` join or a serialize failure. **Resources are the exception** — a resource is addressed by URI, not chosen from arguments, so an unknown URI *is* a protocol fault: `read_resource` returns `Err(ErrorData::resource_not_found(...))`.
- **`validate_screen` reporting `ok: false` is a SUCCESSFUL call** — not `is_error`. A failed **render** is `is_error: true` but must still carry its diagnostics.
- **Never `Implementation::from_build_env()`** — its `env!` expands inside rmcp, reporting rmcp's name/version.
- **Many rmcp types are `#[non_exhaustive]`** — struct literals fail with E0639. Use constructors + public-field assignment. This bit Task 11 too: `ReadResourceResult::new(contents)`, not a literal.
- **`#[tool_router(router = x, vis = "pub")]` generates an ASSOCIATED function** — combine as `Self::tools_read_router() + …`. **The plan has got this wrong in three separate task briefs.**
- **`schemars` only via `rmcp::schemars`** — reaffirmed by the user for Task 10. byonk is a single crate and rmcp is already a whole-crate dep, so there is no crate-boundary cost; a direct `schemars` dep risks cargo resolving a second version if rmcp bumps. **Do not add `schemars` to Cargo.toml.**
- **`#[serde(untagged)]` works on a schemars-derive-only type with NO `Deserialize`/`Serialize` derive**, because `schemars_derive` registers `serde` as its own helper attribute. Discovered in Task 10.
- **Every `ScreenStore` call from an async handler goes through the `blocking` helper.**
- Every POST to `/mcp` must carry `Accept: application/json, text/event-stream` (else 406) **and** a `Host` header — rmcp parses `Host` before consulting the allowlist. `tests/common/mcp.rs` sets both.
- `get_info` already advertises **`.enable_resources()`** (`src/mcp/mod.rs:129-131`) — real clients will see the Task 11 resources.

### Human rulings made during execution

- **Task 9 — `assign_screen` creates a mapping only for a REGISTRY-SEEN device.** An upsert for a device that has polled `/api/setup`, but an unseen or typo'd MAC is refused: without the gate a typo persisted a phantom device to `config.yaml`, and there is no MCP delete tool to undo it. Overrides the brief. Output carries `created: bool`.
- **Task 10 — the published `options` schema must match `parse_options`, overriding the plan.** The brief mandated `#[schemars(with = "Option<Vec<EnumOption>>")]`, which requires both `value` and `label` — but the parser also accepts bare strings (`options: [small, large]`) and maps whose `label` is optional. The plan's schema would have rejected two documents byonk accepts, and contradicted its own sibling doc comment. **Ruling: the finding governs, fix now rather than defer**, because Task 11 was about to publish it as a resource. Fixed with a derive-only `#[serde(untagged)] RawEnumOption`.

## Load-bearing invariants — do not break these

- **`ScreenStore::new` must get the SAME `Arc<ScreenRepoManager>` the `ContentPipeline` has.** Guarded by `tests/screen_store_wiring_test.rs`.
- **The `byonk-builtin` handle string is frozen** — `content_pipeline.rs:215` hard-references `byonk-builtin/default`.
- **`byonk-builtin` enumerates embedded-only, but `read` keeps the `SCREENS_DIR` overlay** — so it touches the filesystem despite the name. This exact mismatch caused a real defect in Task 2.
- **Writability is structural** — derived from `writable_root()`, never from a handle's name. Same for `ScreenRepoKind`.
- **`ScreenStore`'s mutex is `std::sync::Mutex` and not reentrant.** No mutating method may call another. `list_screens` and `read_file` do **not** take it (only the six mutators at `screen_store.rs:433/549/588/669/723/770`), so Task 11's sequential read calls are safe.
- **The `byonk://examples/` guard is safe because `screen_ref` is a PURE STRING** compared by equality against `list_screens()` output — never joined onto a filesystem path before the check, so there is no decode/normalize/join step to exploit. Verified against four traversal shapes beyond the pinned test.
- **Option resolution for renders lives once**, in `src/api/display.rs`. Both `/dev/render` and `ScreenStore::render` call it.
- **Device writes must never call `require_writable_global`** — a device mapping is not global config, so it stays writable in HA add-on mode.

## Deferred minors — the final review must triage these

All logged with full context in the ledger. The ones with real teeth:

1. **The Task 3 concurrency gate is probabilistic** — no `std::sync::Barrier` before the concurrent create, so on a loaded runner it could pass against *unlocked* code. It is the only test gating that task. **Add a barrier.**
2. **No test pins the structural writability rule** — the listing test uses only canonically-named handles, so `writable = handle == "local"` would pass it. **Add a handle named `local` backed by a read-only source.**
3. **Binary screen assets cannot be written over MCP** — `WriteFileArgs.content` is `String`. No `background.jpg`, no custom font. Either base64 support or an explicit doc note.
4. **`stat`-then-`read` is two syscalls**; a final-component swap between them can still deliver oversized bytes. `File::open` + `Read::take(max+1)` would bound it unconditionally.
5. `delete_screen_file` ships with no end-to-end test; **no `tools/list` assertion anywhere** — the 15-tool registration is verified only indirectly.
6. `AssetScreensSource` (`lua_runtime.rs:97`) reads via the disk overlay without overriding `read_limited`, contradicting the trait contract.
7. `list_screen_repos` hides a *configured* repo whose manifest is missing/unloadable, while the admin endpoint lists it.
8. `kind()` defaults to `Embedded`, so a future source that forgets the override silently reports the most restrictive kind.
9. Pre-existing, out of scope: `resolve_manifest_root` joins the untrusted manifest `root:` field with no `is_safe_rel` check.
10. **`Severity::Warning` is dead code** — every `Issue` `ScreenStore::validate` pushes is `Severity::Error`.
11. `include_raw`'s two image blocks are indistinguishable — ordering is the only signal and it is documented nowhere.
12. `RenderArgs.width`/`height` have no doc comments, so their schema descriptions are empty while siblings have them.
13. **A device whose `config.devices` key differs from its MAC** — `assign_screen` passes `mac` straight through as the config key, so a mismatch may create a duplicate entry. **Unverified.**
14. `validate_params` iterates only schema fields, so params carried across a screen change are never rejected for unknown keys. Pre-existing.
15. **CHANGES.md has no MCP entry at all.** Task 12 owns this.
16. **Task 10:** the generated `params` carries `default: null` while its type is `"object"` — schemars derives the default from `serde_yaml::Value::default()`, independent of the `with =` override. Cosmetic.
17. **Task 10:** the schema's top-level `title` is the Rust-internal type name `"RawMeta"`, in a document published to LLM authors.
18. **Task 11:** `resources.rs:107-113` — if one of `meta.yaml`/`script.lua`/`screen.svg` fails to read for a *listed* example, the response is a **successful** 200 with the literal `(unreadable)` in place of that body, and no signal in the JSON-RPC envelope. The membership guard is the sole barrier. Raised independently by both the implementer and the reviewer.

## Process notes that earned their place

- **Eleven tasks, eleven plan defects — every one in the PLAN's text, not the implementation.** The plan's code blocks are specific enough to look authoritative and are not. **Reviews are what is making this work; do not weaken them.** Task 11 was the first brief whose *APIs* all checked out — and it still shipped a duplicate constant and a doc comment naming five fields for a four-element tuple.
- **Pre-flight the brief before dispatching, and read it properly.** Task 10's pre-flight raised five alarms; four dissolved on a careful read, because it had assumed the brief derived `JsonSchema` on `ScreenMeta` when it actually derived on `RawMeta`. Only the fifth was real. A wrong pre-flight costs a human interrupt and nearly cost a redesign — check what the brief *says*, not what you expect it to say.
- **Descriptions are code.** Tool and resource descriptions are the only contract an MCP client ever sees, and an agent acts on them. Task 8 shipped a `validate_screen` description promising something `TemplateService::validate_template` documents the opposite of. **Verify every behavioural claim against the code, and fix the description, not the code.**
- **The single most valuable catch remains a test that could not fail.** Ask of every security- or contract-relevant test: would this fail against broken code? Task 8's reviewer broke *three* lines at once and all 16 tests stayed green. Task 10's `test_params_schema_describes_the_field_descriptor` only greps for token presence and could never have caught the `options` defect. **Where it matters, break the code and watch it fail.**
- **Implementers disclosing weak tests is now the norm and it keeps paying.** Task 10's implementer disclosed unprompted that the brief's `refresh` assertion is blind — schemars treats any `Option<T>` as schema-optional regardless of `#[serde(default)]`, so removing the attribute does not turn the test red. Task 11's disclosed the `(unreadable)` degradation. Keep telling them disclosure is valued, not penalised.
- **Reviewers verifying claims empirically** caught what reading alone would not. Task 10's re-reviewer reverted the fix and watched the new test fail; Task 11's wrote four traversal shapes the pinned test never covered. Keep asking for it, and name the highest-risk lines so a scoped re-review spends its budget well.
- **Do not tell an implementer "verified, do not correct from memory" about a third-party API.** The crate source is the authority.
- Subagents stall or die past roughly ~150–230k transcript tokens; dispatch fresh rather than resuming a large one. A stalled agent's *uncommitted work is usually salvageable* — check `git status` before re-dispatching, and hand the successor an honest inventory of what is there and a warning not to trust it.
- Keep unrelated lint/fixture/test-isolation fixes in **separate commits**.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. `make docs` needs `mdbook-mermaid` (installed and working as of Task 11).
- If `cargo` is missing, add `$HOME/.cargo/bin` to `PATH` — rustup-managed via `rust-toolchain.toml` (never add cargo/rust to mise).
- **Cap parallelism at 4** for compiles and test runs — shared machine.
- Never `git add -A`/`.` — stage explicit paths, verify `git diff --cached`. CHANGES.md is user-facing only.
- **Beware tests that derive a path via `..` from a temp dir.** Task 8's `make check` failure was several tests resolving to one shared `$TMPDIR/examples`; `seed_examples` only seeds a missing-or-empty dir, so leftovers starved later runs. Nest the temp dir under a private parent.
- **After Plan 2:** connect a real MCP client against a local `byonk serve` and drive list → copy → edit → render → assign, and read the new resources (a green suite does not prove a real client negotiates the handshake), then validate on the HA VM per memories `ha-vm-from-source-addon-build` and `ha-vm-addon-manifest-sync-gap`.

## Working tree

**Clean** at `56eeba8`.
