# Handover — Byonk

_Last updated: 2026-08-06 — **Plan 2 (MCP interface) is COMPLETE, 12/12, and the final whole-branch review is clean.** Branch `feat/screen-store-authoring-core`, HEAD `1bbcc2d`, 68 commits ahead of `origin/main` (`67b3855`), local-only (never pushed). The branch remains **HELD by user decision** — no merge, no PR. **Working tree clean.** `make check` (606 passed / 0 failed / 1 ignored) and `make docs` both green at HEAD._

## Resume here

Both plans on this branch are done and reviewed. **The next work is not code — it is the real-world verification the suite cannot do**, then the user's merge decision.

1. **Post-plan verification** (below) — a real MCP client against a local `byonk serve`, then the HA VM. Neither has been done.
2. **Then** the user decides on merge. Do not open a PR or merge without being asked; the hold is deliberate and has survived several sessions.

The SDD ledger `.superpowers/sdd/2026-07-28-screen-store-mcp-interface/progress.md` (git-ignored) is still the recovery map: one line per task with commit ranges, every review verdict, every deferred minor, every human ruling. **It was deliberately NOT deleted** at plan end (the skill's default) because the branch is unmerged and verification is outstanding. Trust it plus `git log` over memory.

## Post-plan verification — the outstanding work

- [ ] **Drive a real MCP client.** `claude mcp add --transport http byonk http://localhost:3000/mcp --header "Authorization: Bearer <token>"` against a local `byonk serve`, then run the full loop: `list_screens` → `copy_screen` → edit → `render_screen` → `assign_screen`, and read the resources. **A green suite does not prove a real client negotiates the handshake** — the integration tests speak JSON-RPC directly.
- [ ] **Validate on the HA VM**, reaching `/mcp` from the Mac host over the LAN. This is precisely the case `.disable_allowed_hosts()` exists for and exactly what rmcp's loopback default would have blocked. Follow memories `ha-vm-from-source-addon-build` and `ha-vm-addon-manifest-sync-gap` — note that `make ha-rebuild` does **not** sync the add-on manifest.
- [ ] **Confirm `/mcp` returns 404** on an install with no admin token configured.

## What this branch delivers

Byonk becomes a place where screens are **authored**, not just served, with an LLM as a first-class author able to develop screens against a byonk running **anywhere** — including the HA add-on — over the LAN, with no filesystem access and no Samba share.

- **Spec** — `docs/superpowers/specs/2026-07-24-screen-store-and-mcp-design.md`
  - **Plan 1 — Authoring core** — DONE (13/13). `ScreenStore`, writable local repos, examples as an editable repo, atomic writes, validation.
  - **Plan 2 — MCP interface** — `docs/superpowers/plans/2026-07-28-screen-store-mcp-interface.md` — DONE (12/12). `/mcp` behind the admin token, **14 tools**, resources publishing byonk's own authoring references, user docs.
- **Spec 2 — Svelte web UI at `/`** (not written). Consumes the same `ScreenStore`.
- **Spec 3 — Git commit & history** (not written).

**There are 14 tools, not 15** — an earlier handover said 15 and was wrong. read: `list_screens`, `read_screen_file`, `list_screen_repos`, `list_devices`, `get_config`. edit: `write_screen_file`, `create_screen`, `copy_screen`, `rename_screen`, `delete_screen`, `delete_screen_file`. render: `render_screen`, `validate_screen`. device: `assign_screen`. Names derive from the Rust fn names — no explicit `name =` attributes. Pinned since the fix wave by `test_tools_list_reports_exactly_the_14_authoring_tools`.

## The final review's verdict

Dispatched on the most capable model over `67b3855..f158ff8` (60 commits, 116 files, +14644/−739; the prepared package was ~912 KB and had to be navigated by per-path diffs, not read whole).

**No Critical findings.** The security surface was verified solid: the reviewer could construct **no path traversal, no symlink escape and no auth bypass**, and confirmed the load-bearing concurrency invariants directly. It found two Important issues; both were fixed and independently re-verified.

Then **one fix wave** (`f158ff8..1bbcc2d`, 8 commits) and **one scoped re-review**, which returned **all 10 findings ADDRESSED, no new Critical/Important breakage, merge-ready: yes**.

### What the fix wave changed

| Commit | What |
|---|---|
| `3ae408a` | **`assign_screen` resolves the existing config key before patch-vs-add** (new `AppConfig::resolve_device_key`). |
| `f06982f` | **`write_screen_file` refuses to overwrite an existing binary asset** (new `StoreError::BinaryOverwrite`). |
| `988f361` | `Arc<Barrier>` — the concurrency gate is no longer probabilistic. |
| `b16e4db` | Pins the structural writability rule with a read-only source under the handle `local`. |
| `3636362` | `delete_screen_file` e2e + the first-ever `tools/list` assertion. |
| `6a701fb` | `delete_file` no longer creates directories on a failed delete. |
| `1e89f7a` | `include_raw` ordering/failure semantics, `width`/`height` docs, `meta.yaml` schema title. |
| `1bbcc2d` | `0.0.0.0:3000` is a default *port*, not address. |

**The two Important findings, in full, because they are subtle:**

1. **`assign_screen` silently shadowed an existing device config.** `apply_device_patch` used an exact `config.devices.get(key)`, but byonk also keys devices by **registration code** (the HA onboarding path) and tolerates **case-differing MACs**. `list_devices` resolves through both fallbacks and reports `mac = d.device_id` — so an agent following the tool's own "use `list_devices` first" instruction got: patch → exact-key miss → NotFound → registry confirms real → a **second** entry written keyed by MAC, `created: true`. The original survived but went inert (both `content_pipeline.rs:188` and `get_device_config` prefer the MAC key), silently dropping its `name`, `params`, `panel`, `dither`, `refresh`. This was deferred minor #13, logged **"Unverified"** — the final reviewer verified it and it was real.
2. **Binary assets were a data-loss round trip**, not merely an unsupported feature: `read_screen_file` returns `content: ""` for non-UTF-8 files and `write_screen_file.content` is a `String`, so the *documented* read → edit → write cycle **truncated** a screen's PNG or font.

## Decisions already settled — do not re-litigate

Established by reading the vendored `rmcp` 2.2 source (at `<scratchpad>/rmcp-2.2.0/`, or `~/.cargo/registry/src/*/rmcp-2.2.0/`; re-fetch with `curl -sL https://static.crates.io/crates/rmcp/rmcp-2.2.0.crate | tar xz` if gone).

- **`rmcp` 2.2**, the latest *stable*. `3.0.0-beta.4` is a prerelease — out of scope.
- **`.disable_allowed_hosts()`** — rmcp defaults to loopback only, which would reject the entire LAN/HA use case. The Bearer token already defeats DNS rebinding. **User-approved.**
- **Stateless** (`stateful_mode: false`, `json_response: true`, `NeverSessionManager`).
- **Tool failures are `Ok(CallToolResult::error(...))`, never `Err(ErrorData)`** — clients render protocol errors opaquely, so the model never sees an `Err`'s message. **Resources are the exception**: a resource is addressed by URI, so an unknown URI *is* a protocol fault and returns `Err(ErrorData::resource_not_found(...))`.
- **`validate_screen` reporting `ok: false` is a SUCCESSFUL call.** A failed **render** is `is_error: true` but still carries its diagnostics.
- **Never `Implementation::from_build_env()`** — its `env!` expands inside rmcp, reporting rmcp's name/version.
- **Many rmcp types are `#[non_exhaustive]`** — struct literals fail with E0639. Use constructors + public-field assignment.
- **`#[tool_router(router = x, vis = "pub")]` generates an ASSOCIATED function** — combine as `Self::tools_read_router() + …`.
- **`schemars` only via `rmcp::schemars`.** Do not add `schemars` to `Cargo.toml` — a second resolved version would break the derives.
- **`#[serde(untagged)]` works on a schemars-derive-only type with NO `Deserialize`/`Serialize` derive**, because `schemars_derive` registers `serde` as its own helper attribute.
- **Every `ScreenStore` call from an async handler goes through the `blocking` helper.**
- Every POST to `/mcp` must carry `Accept: application/json, text/event-stream` (else 406) **and** a `Host` header — rmcp parses `Host` before consulting the allowlist. `tests/common/mcp.rs` sets both.
- **The published `options` schema must match `parse_options`** — the parser accepts bare strings (`options: [small, large]`) and maps whose `label` is optional. Solved with a derive-only `#[serde(untagged)] RawEnumOption`.
- **`assign_screen` creates a mapping only for a REGISTRY-SEEN device.** An unseen or typo'd MAC is refused — without the gate a typo persisted a phantom device to `config.yaml`, and there is no MCP delete tool to undo it. Still intact after the fix wave (`tools_device.rs:108`), still tested.

## Load-bearing invariants — do not break these

- **`ScreenStore::new` must get the SAME `Arc<ScreenRepoManager>` the `ContentPipeline` has.** Guarded by `tests/screen_store_wiring_test.rs`.
- **The `byonk-builtin` handle string is frozen** — `content_pipeline.rs:215` hard-references `byonk-builtin/default`.
- **`byonk-builtin` enumerates embedded-only, but `read` keeps the `SCREENS_DIR` overlay** — so it touches the filesystem despite the name. This mismatch caused a real defect in Task 2.
- **Writability is structural** — derived from `writable_root().is_some()` (`screen_store.rs:383`), never from a handle's name. Now pinned by a test using a read-only source under the handle `local`.
- **`ScreenStore`'s mutex is `std::sync::Mutex` and not reentrant.** No mutating method may call another. Only the six mutators take it; `list_screens`/`read_file`/`validate`/`render` deliberately do not. Verified twice by review.
- **`verify_writable_parent` vs `ensure_writable_parent`** (`screen_store.rs:504-538`): the first is the canonicalize / deepest-existing-ancestor / `starts_with` guard; the second is that **plus** `create_dir_all`. Write paths (`write_file:455`, `guarded_write:568`) need the mkdir version; `delete_file:824` must not create directories. Do not merge them back.
- **The `byonk://examples/` guard is safe because `screen_ref` is a PURE STRING** compared by equality against `list_screens()` output — never joined onto a filesystem path before the check, so there is no decode/normalize/join step to exploit.
- **Option resolution for renders lives once**, in `src/api/display.rs`.
- **Device writes must never call `require_writable_global`** — a device mapping is not global config, so it stays writable in HA add-on mode.
- **A failed render emits NO image block** — all three `render()` failure branches return `..empty()`, and `empty()` sets `raw_png: None` (`screen_store.rs:943-949`). A final-review finding claimed a failed render could emit a lone *raw* block mistakable for the dithered one; that was **wrong**, verified against all three branches.

## Known-remaining minor issues (all triaged "fine to ship")

The final reviewer triaged 18 parked minors: only the two Importants above needed fixing. What remains, plus one new one from the fix wave:

1. **`write_file` now reads the entire existing target into memory on every write** (`screen_store.rs:460`), not only when `if_match` is set, with no `MAX_FILE_BYTES` guard on that read — only the incoming bytes are size-checked (`:442`). A >5 MB file placed out-of-band (Samba, `SCREENS_DIR` mount) is fully read on any write attempt. The same unguarded read already existed on the `if_match` path; it is now unconditional. *New, from the fix wave.*
2. **`stat`-then-`read` is two syscalls** — a final-component swap between them can still deliver oversized bytes. Exploiting it needs a live local process with write access to the repo, which already implies content control; a statically-planted symlink is caught.
3. **`AssetScreensSource`** (`lua_runtime.rs:97`) reads via the disk overlay without overriding `read_limited`, contradicting the trait contract. Private, no caller reaches it.
4. `list_screen_repos` hides a *configured* repo whose manifest is missing/unloadable, while the admin endpoint lists it.
5. `kind()` defaults to `Embedded` — fails to the most restrictive value, so a missing override is safe.
6. **`Severity::Warning` is dead code** — every `Issue` `validate` pushes is `Severity::Error`, so no agent will ever see a warning.
7. `resources.rs:107-113` — a *listed* example whose `meta.yaml`/`script.lua`/`screen.svg` fails to read yields a **successful 200** with the literal `(unreadable)` in place of that body. The membership guard is the sole barrier.
8. `validate_params` iterates only schema fields, so params carried across a screen change are never rejected for unknown keys. Pre-existing.
9. Generated `params` schema carries `default: null` while its type is `"object"`. Cosmetic.

### Worth filing as follow-up issues (pre-existing, out of scope)

- **`resolve_manifest_root`** (`screen_repo_loader.rs:309`) joins the untrusted manifest `root:` field with no `is_safe_rel` check. Pre-existing and unchanged — **but it now interacts with the new write path**: a manifest with `root: ../../..` sets the writable root that every guard validates *against*.
- **`walk_screen_paths`/`walk_ext_files` follow symlinked directories**, so a symlink loop in a fetched repo recurses to stack exhaustion. A DoS, not a disclosure — reads still fail the canonicalize guard. Same shape before this branch; new `walk_files_under:285` inherits it.

## Process notes that earned their place

- **Twelve tasks, twelve plan defects — every one in the PLAN's text, not the implementation.** The plan's code blocks are specific enough to look authoritative and are not. Task 12's brief alone got the site count, the version semantics and the tool count wrong. **Pre-flight every brief against the code before dispatching.**
- **Reviews are what made this work; do not weaken them.** The final reviewer independently verified deferred minor #13, which the ledger had logged as *Unverified* — and it was a real config-corrupting bug.
- **The single most valuable catch remains a test that could not fail.** Ask of every security- or contract-relevant test: would this fail against broken code? The re-reviewer broke code in a scratch worktree and watched the new tests fail for five separate findings before accepting them.
- **Implementers disclosing weak tests is the norm and it keeps paying.** The fix-wave implementer disclosed a scope caveat in its own writability test (the re-reviewer checked and it did *not* weaken it) and **disputed half of a review finding** — correctly, verified against all three `render()` failure branches. Keep telling them disclosure is valued, not penalised.
- **Descriptions are code.** Tool and resource descriptions are the only contract an MCP client sees, and an agent acts on them. Verify every behavioural claim against the code, and fix the description, not the code.
- **Put the foreground rule on the FIRST line of a dispatch.** Two agents were lost to tooling, neither to the work: BSD `sed -i` (macOS sed needs a backup-suffix argument — tell implementers to use Edit/Write and never `sed` to modify files) and a background Monitor for `make check`.
- **Tell implementers to commit incrementally.** A whole fix wave was lost to a session crash with everything uncommitted; the retry committed per finding and survived.
- Subagents stall or die past roughly ~150–230k transcript tokens; dispatch fresh rather than resuming a large one. Check `git status` before re-dispatching — a stalled agent's uncommitted work is often salvageable.
- Keep unrelated lint/fixture/test-isolation fixes in **separate commits**.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. `make docs` needs `mdbook-mermaid` (installed and working).
- **Test counts differ by convention**: `--lib` alone is ~362; **606** is lib + all integration binaries. An earlier "398" was never the whole suite. Don't read a jump as a discrepancy.
- If `cargo` is missing, add `$HOME/.cargo/bin` to `PATH` — rustup-managed via `rust-toolchain.toml` (never add cargo/rust to mise).
- **Cap parallelism at 4** for compiles and test runs — shared machine.
- Never `git add -A`/`.` — stage explicit paths, verify `git diff --cached`. There are untracked local files here, including a stray `docs/src/guide/installation.md~`. CHANGES.md is user-facing only.
- **Beware tests that derive a path via `..` from a temp dir** — several once resolved to one shared `$TMPDIR/examples` and starved each other. Nest the temp dir under a private parent.

## Working tree

**Clean** at `1bbcc2d`.
