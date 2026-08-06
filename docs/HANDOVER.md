# Handover — Byonk

_Last updated: 2026-08-06 — **Two new implementation plans are written, reviewed and committed; nothing is implemented yet.** Branch `feat/screen-store-authoring-core`, HEAD `73beeb0`, 71 commits ahead of `origin/main` (`67b3855`), local-only (never pushed). The branch remains **HELD by user decision** — no merge, no PR, and the user has explicitly confirmed the new work lands on this same branch. **Working tree clean.** `make check` (606 passed / 0 failed / 1 ignored) and `make docs` were both green at `1bbcc2d`; the three commits since are documentation only._

## Resume here

**Start Plan A, Task 1.** The next session's job is implementation, not design. Both plans are complete, placeholder-free, and self-reviewed against the spec.

1. Read `docs/superpowers/plans/2026-08-06-plan-a-measured-colours-end-to-end.md` (7 tasks) and execute it with **`superpowers:subagent-driven-development`** — fresh subagent per task, review gate between tasks.
2. Then `docs/superpowers/plans/2026-08-06-plan-b-image-process-for-eink.md` (11 tasks). Plan B's Tasks 1–8 need nothing from Plan A; only Task 9's `palette_aware` reads `device.colors_actual`, and that task says which line to drop if Plan A is not yet in.
3. **Then**, and only then, the two verification items still outstanding from the previous initiative (below) — they were never done.

Do not open a PR or merge without being asked. The hold is deliberate and has survived several sessions.

## What is new since the last handover

| Commit | What |
|---|---|
| `6a1caa3` | **Spec** — `docs/superpowers/specs/2026-08-06-lua-colors-and-image-ops-design.md` |
| `73beeb0` | **Plan A + Plan B**, plus a colour-science correction to the spec (see below) |

The user asked for three things. One turned out to already exist, two became the plans.

- **(a) Expose `actual_colors` to Lua so a script can customise them dynamically** → Plan A.
- **(b) A Lightroom-inspired set of image operations applied before an image enters the SVG** → Plan B.
- **(c) "Can an LLM working on a screen design fully round-trip test their work by downloading the rendered PNG?"** → **Yes, and there is no download step.** `render_screen` returns the PNG *inside* the tool result as an MCP image block (`src/mcp/tools_render.rs:135`), so a vision-capable model sees it directly; `include_raw` adds the pre-dither image; a failed render returns no image block at all. The user then added a requirement that fell out of this, which became Plan A's Part 3: the agent must be able to ask for the **measured**-colour preview, as `/dev` can.

## The two plans

### Plan A — Measured colours end to end (7 tasks)

`docs/superpowers/plans/2026-08-06-plan-a-measured-colours-end-to-end.md`

1. `device.colors_actual` readable from Lua — `nil`, not mirrored, when uncalibrated.
2. `colors_actual` accepted as a script return value.
3. **`resolve_measured_colors`** — the chain extracted as a pure function and unit-tested *in isolation*, so a reviewer can reject the semantics before four call sites depend on them.
4. Wire it into all four render paths (`display.rs`, `dev.rs`, `screen_store.rs`, `main.rs`).
5. `svg_to_png` warns instead of silently dropping mismatched measured colours.
6. `use_actual` + `colors_actual` on `render_screen`.
7. Docs.

### Plan B — `image_process` for e-ink (11 tasks)

`docs/superpowers/plans/2026-08-06-plan-b-image-process-for-eink.md`

A new **`crates/eink-photo`** (zero dependencies, mirroring `eink-dither`) built operation by operation, then `src/services/image_process.rs` for codecs and geometry, then the Lua global, then an end-to-end test that vibrance actually raises the dithered chromatic share.

**Adds one dependency:** `image` 0.25, `default-features = false`, features `jpeg`/`png`/`webp`. The decoders themselves (`png`, `zune-jpeg`, `image-webp`) are **already in the lockfile via resvg** — this adds the façade, not a new decoder stack. Plan B Task 8 Step 1 says to stop and report if the lockfile gains a duplicate decoder.

## Design decisions already settled — do not re-litigate

All were decided with the user during brainstorming and are recorded in the spec's "Open questions" section (which says: none).

- **Read *and* override**, not read-only. A script both sees `device.colors_actual` and can return its own.
- **Script wins the measured chain**: `script > dev override > panel.colors_actual > Measured-Colors header > none`. Symmetric with `script_colors`, which already beats everything. The dev tuning popup going inert on such a screen is mitigated by `measured_source` reporting `"script"`.
- **A length mismatch never fails a render.** It logs (to the *script log* on the authoring path, `tracing::warn!` on device paths) and falls through to the next source.
- **`image_process` is one parametric call, not a chainable object.** The user initially chose a chain, then reversed: *"the pseudo chainable design is non-obvious."* A record-then-bake chain reads as sequential but is not; an immediate-mode chain accepts wrong orderings. One call with a fixed order makes both problems unrepresentable.
- **Purpose is e-ink survival, not a general editing toolkit.** Per-colour HSL, split toning, vignette, grain, lens correction and denoise are explicitly out: six colours cannot show the difference, and noise actively helps dithering.
- **`preset = "eink"` is a base layer** — explicit keys override it, so `{ preset = "eink", clarity = 0 }` means no clarity. This is the only rule that stays predictable as the preset's numbers are retuned.
- **`palette_aware` v1 does endpoints only.** Steering chroma toward reachable palette entries is future work; it interacts with the dithering colour science recorded in project memory and needs its own measurement.
- **`RenderOpts.colors_actual` occupies the dev-override slot** — `script > RenderOpts.colors_actual > panel > none` — and reports `measured_source = "render_opts"`.

### The colour-science correction, because it matters

The spec originally ran the **whole** tone group in linear light. That is wrong and would have shipped visibly bad output: an S-curve, an endpoint remap or a shadow lift applied in linear light crushes midtones. Corrected in `73beeb0`:

- **Linear light**: exposure, white balance. These model light and are only correct as multiplications.
- **Gamma-encoded tone domain**: `auto_levels`, blacks/whites, highlights/shadows, contrast, curve, clarity, vibrance, saturation, grayscale/invert, sharpen.

This is the same distinction `eink-dither` already draws — error diffusion in linear RGB, colour matching in OKLab. Plan B Task 4 has a test (`contrast_pivots_about_mid_grey_in_the_tone_domain`) that specifically catches a regression to the linear version.

## Facts verified in the tree while planning — trust these over re-derivation

- `resolve_render_params` has **four** call sites: `src/main.rs:350`, `src/api/display.rs:761`, `src/api/dev.rs:685`, `src/services/screen_store.rs:1100`.
- **`measured_colors` currently passes through `resolve_render_params` untouched** — there is no script layer today.
- `svg_to_png.rs:341` sets `eink_actual = None` on a post-dedup length disagreement, **silently**. A script returning its own `colors` of a different length already loses its calibration with no diagnostic.
- `screen_store.rs:1114` does `let use_actual = measured_colors.is_some()` — so MCP renders already show measured colours, but only by accident of configuration, with no way to ask or refuse.
- `parse_colors_header` (`display.rs:30`) **silently drops** entries that are not 6-digit hex, so a malformed colour shows up downstream as a length mismatch. Plan A Task 3 has a test for exactly this.
- **`DeviceContext` is built before measured colours are resolved in `dev.rs` (470 vs 569) and `main.rs` (237 vs ~320)**, but *after* in `display.rs` and `screen_store.rs`. Plan A Task 1 Steps 8–9 hoist the two that are wrong. This is the only structural churn in either plan.
- `DeviceContext` derives `Default` and every construction site uses `..Default::default()` **except** `display.rs:663`, which names every field — so adding a field breaks exactly one site at compile time.
- The two `ScriptResult` types differ deliberately: `lua_runtime::ScriptResult.colors` becomes `content_pipeline::ScriptResult.script_colors`. Do not "fix" the naming.
- `crates/eink-dither` has **zero** runtime dependencies. `eink-photo` must match.
- MCP test harness: `TestApp::new_admin("secret")` + `McpTestClient`; `call_tool` returns the JSON-RPC `result`; image blocks are `{"type":"image","mimeType":"image/png","data":"<b64>"}`.
- `ScreenStore` test fixture: `common::store::build_store(dir, &["names"])` in `tests/common/store.rs`.

## Still outstanding from the previous initiative — never done

These predate the new plans and are unaffected by them. Do them after Plan B, or whenever the user asks.

- [ ] **Drive a real MCP client.** `claude mcp add --transport http byonk http://localhost:3000/mcp --header "Authorization: Bearer <token>"` against a local `byonk serve`, then the full loop: `list_screens` → `copy_screen` → edit → `render_screen` → `assign_screen`, and read the resources. **A green suite does not prove a real client negotiates the handshake** — the integration tests speak JSON-RPC directly.
- [ ] **Validate on the HA VM**, reaching `/mcp` from the Mac host over the LAN — precisely the case `.disable_allowed_hosts()` exists for. Follow memories `ha-vm-from-source-addon-build` and `ha-vm-addon-manifest-sync-gap`; `make ha-rebuild` does **not** sync the add-on manifest.
- [ ] **Confirm `/mcp` returns 404** on an install with no admin token configured.

## What this branch already delivers

Byonk is a place where screens are **authored**, not just served, with an LLM as a first-class author working against a byonk running **anywhere** — including the HA add-on — over the LAN, with no filesystem access and no Samba share.

- **Spec** — `docs/superpowers/specs/2026-07-24-screen-store-and-mcp-design.md`
  - **Plan 1 — Authoring core** — DONE (13/13). `ScreenStore`, writable local repos, examples as an editable repo, atomic writes, validation.
  - **Plan 2 — MCP interface** — DONE (12/12), final review clean. `/mcp` behind the admin token, **14 tools**, resources publishing byonk's own authoring references, user docs.
- **Spec 2 — Svelte web UI at `/`** (not written). Consumes the same `ScreenStore`.
- **Spec 3 — Git commit & history** (not written).

**There are 14 tools, not 15.** read: `list_screens`, `read_screen_file`, `list_screen_repos`, `list_devices`, `get_config`. edit: `write_screen_file`, `create_screen`, `copy_screen`, `rename_screen`, `delete_screen`, `delete_screen_file`. render: `render_screen`, `validate_screen`. device: `assign_screen`. Names derive from the Rust fn names — no explicit `name =` attributes. Pinned by `test_tools_list_reports_exactly_the_14_authoring_tools`. **Plan A adds arguments, never tools — if that count changes, something is wrong.**

The SDD ledger `.superpowers/sdd/2026-07-28-screen-store-mcp-interface/progress.md` (git-ignored) is the recovery map for that initiative: one line per task with commit ranges, every review verdict, every deferred minor, every human ruling. **Deliberately not deleted** because the branch is unmerged and verification is outstanding.

## Decisions from the MCP work — still binding

Established by reading the vendored `rmcp` 2.2 source (at `<scratchpad>/rmcp-2.2.0/`, or `~/.cargo/registry/src/*/rmcp-2.2.0/`; re-fetch with `curl -sL https://static.crates.io/crates/rmcp/rmcp-2.2.0.crate | tar xz`).

- **`rmcp` 2.2**, the latest *stable*. `3.0.0-beta.4` is a prerelease — out of scope.
- **`.disable_allowed_hosts()`** — rmcp defaults to loopback only, which would reject the entire LAN/HA use case. The Bearer token already defeats DNS rebinding. **User-approved.**
- **Stateless** (`stateful_mode: false`, `json_response: true`, `NeverSessionManager`).
- **Tool failures are `Ok(CallToolResult::error(...))`, never `Err(ErrorData)`** — clients render protocol errors opaquely, so the model never sees an `Err`'s message. **Resources are the exception**: a resource is addressed by URI, so an unknown URI *is* a protocol fault.
- **`validate_screen` reporting `ok: false` is a SUCCESSFUL call.** A failed **render** is `is_error: true` but still carries its diagnostics.
- **Never `Implementation::from_build_env()`** — its `env!` expands inside rmcp, reporting rmcp's name/version.
- **Many rmcp types are `#[non_exhaustive]`** — struct literals fail with E0639. Use constructors + public-field assignment.
- **`#[tool_router(router = x, vis = "pub")]` generates an ASSOCIATED function** — combine as `Self::tools_read_router() + …`.
- **`schemars` only via `rmcp::schemars`.** Do not add `schemars` to `Cargo.toml`.
- **Every `ScreenStore` call from an async handler goes through the `blocking` helper.**
- Every POST to `/mcp` needs `Accept: application/json, text/event-stream` (else 406) **and** a `Host` header. `tests/common/mcp.rs` sets both.
- **`assign_screen` creates a mapping only for a REGISTRY-SEEN device.** A typo'd MAC is refused; without the gate a typo persisted a phantom device to `config.yaml`, and there is no MCP delete tool to undo it.

## Load-bearing invariants — do not break these

- **`ScreenStore::new` must get the SAME `Arc<ScreenRepoManager>` the `ContentPipeline` has.** Guarded by `tests/screen_store_wiring_test.rs`.
- **The `byonk-builtin` handle string is frozen** — `content_pipeline.rs:215` hard-references `byonk-builtin/default`.
- **`byonk-builtin` enumerates embedded-only, but `read` keeps the `SCREENS_DIR` overlay** — it touches the filesystem despite the name. This mismatch caused a real defect once.
- **Writability is structural** — derived from `writable_root().is_some()` (`screen_store.rs:383`), never from a handle's name.
- **`ScreenStore`'s mutex is `std::sync::Mutex` and not reentrant.** No mutating method may call another. Only the six mutators take it; `list_screens`/`read_file`/`validate`/`render` deliberately do not.
- **`verify_writable_parent` vs `ensure_writable_parent`** (`screen_store.rs:504-538`): the first is the canonicalize / deepest-existing-ancestor / `starts_with` guard; the second is that **plus** `create_dir_all`. Write paths need the mkdir version; `delete_file:824` must not create directories. Do not merge them back.
- **The `byonk://examples/` guard is safe because `screen_ref` is a PURE STRING** compared by equality against `list_screens()` output — never joined onto a path before the check.
- **Option resolution for renders lives once**, in `src/api/display.rs`. Plan A extends it there and nowhere else.
- **Device writes must never call `require_writable_global`** — a device mapping is not global config, so it stays writable in HA add-on mode.
- **A failed render emits NO image block** — all three `render()` failure branches return `..empty()`, and `empty()` sets `raw_png: None` (`screen_store.rs:943-949`).

## Known-remaining minor issues (all triaged "fine to ship")

1. **`write_file` reads the entire existing target into memory on every write** (`screen_store.rs:460`), not only when `if_match` is set, with no `MAX_FILE_BYTES` guard on that read — only the incoming bytes are size-checked (`:442`).
2. **`stat`-then-`read` is two syscalls** — a final-component swap between them can still deliver oversized bytes. Requires a live local process with write access, which already implies content control; a statically-planted symlink is caught.
3. **`AssetScreensSource`** (`lua_runtime.rs:97`) reads via the disk overlay without overriding `read_limited`, contradicting the trait contract. Private, no caller reaches it.
4. `list_screen_repos` hides a *configured* repo whose manifest is missing/unloadable, while the admin endpoint lists it.
5. `kind()` defaults to `Embedded` — fails to the most restrictive value, so a missing override is safe.
6. **`Severity::Warning` is dead code** — every `Issue` `validate` pushes is `Severity::Error`.
7. `resources.rs:107-113` — a *listed* example whose body fails to read yields a **successful 200** with the literal `(unreadable)`. The membership guard is the sole barrier.
8. `validate_params` iterates only schema fields, so params carried across a screen change are never rejected for unknown keys. Pre-existing.
9. Generated `params` schema carries `default: null` while its type is `"object"`. Cosmetic.

### Worth filing as follow-up issues (pre-existing, out of scope)

- **`resolve_manifest_root`** (`screen_repo_loader.rs:309`) joins the untrusted manifest `root:` field with no `is_safe_rel` check — and it now sets the writable root that every guard validates *against*.
- **`walk_screen_paths`/`walk_ext_files` follow symlinked directories**, so a symlink loop in a fetched repo recurses to stack exhaustion. A DoS, not a disclosure.

## Process notes that earned their place

- **Twelve tasks, twelve plan defects — every one in the PLAN's text, not the implementation.** Plan code blocks are specific enough to look authoritative and are not. **Pre-flight every brief against the code before dispatching.** The two new plans were written *after* reading every signature they reference, and their line numbers are explicitly labelled as anchors to locate by content, not addresses to jump to — but that does not make them right.
- **Reviews are what made the last initiative work; do not weaken them.** The final reviewer independently verified a deferred minor the ledger had logged as *Unverified* — and it was a real config-corrupting bug.
- **The single most valuable catch remains a test that could not fail.** Ask of every contract-relevant test: would this fail against broken code? Both new plans state this as a global constraint and several tasks say explicitly what to do when a test turns out to measure nothing (Plan B Task 10 asserts `vivid > 0.0` purely as that guard).
- **Implementers disclosing weak tests is the norm and it keeps paying.** One disclosed a scope caveat in its own test; another **disputed half of a review finding** — correctly. Keep telling them disclosure is valued, not penalised. Both new plans say so in their constraints and ask for it again in their final-verification checklists.
- **Descriptions are code.** Tool and resource descriptions are the only contract an MCP client sees, and an agent acts on them. Verify every behavioural claim against the code, and fix the description, not the code.
- **Put the foreground rule on the FIRST line of a dispatch.** Two agents were lost to tooling, neither to the work: BSD `sed -i` (macOS sed needs a backup-suffix argument — tell implementers to use Edit/Write and never `sed`) and a background Monitor for `make check`.
- **Tell implementers to commit incrementally.** A whole fix wave was lost to a session crash with everything uncommitted; the retry committed per finding and survived.
- Subagents stall or die past roughly ~150–230k transcript tokens; dispatch fresh rather than resuming a large one. Check `git status` before re-dispatching — a stalled agent's uncommitted work is often salvageable.
- Keep unrelated lint/fixture/test-isolation fixes in **separate commits**.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. `make docs` needs `mdbook-mermaid` (installed and working).
- **Test counts differ by convention**: `--lib` alone is ~362; **606** is lib + all integration binaries. Don't read a jump as a discrepancy. Plan A's tasks state their expected running totals — if one differs, stop and report rather than adjusting the expectation.
- If `cargo` is missing, add `$HOME/.cargo/bin` to `PATH` — rustup-managed via `rust-toolchain.toml` (never add cargo/rust to mise).
- **Cap parallelism at 4** for compiles and test runs — shared machine.
- Never `git add -A`/`.` — stage explicit paths, verify `git diff --cached`. There are untracked local files here, including a stray `docs/src/guide/installation.md~`. CHANGES.md is user-facing only.
- **Beware tests that derive a path via `..` from a temp dir** — several once resolved to one shared `$TMPDIR/examples` and starved each other. Nest the temp dir under a private parent.

## Working tree

**Clean** at `73beeb0`.
