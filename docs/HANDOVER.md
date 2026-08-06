# Handover — Byonk

_Last updated: 2026-08-06 — **Plan A is COMPLETE and ready to merge. Plan B is on Task 10 of 11.** Both run as subagent-driven development in parallel git worktrees. Nothing has been merged; `feat/screen-store-authoring-core` is untouched at `bfc844e` and remains **HELD** — no PR, no merge, no push, by standing owner decision._

## Where the work lives

| Stream | Worktree | Branch | HEAD | State |
|---|---|---|---|---|
| **Plan A** | `/Users/oetiker/checkouts/claude-worktrees/byonk-plan-a` | `feat/plan-a-measured-colours` | `c04311d` | **DONE.** All 8 tasks + final review + final fix wave, all clean. 382 lib tests / 0 failed / 1 ignored. `make check` + `make docs` green. |
| **Plan B** | `/Users/oetiker/checkouts/claude-worktrees/byonk-plan-b` | `feat/plan-b-eink-photo` | `4e5f1b6` | Tasks 1–9 done; **Task 10 fix round 1 awaiting its scoped re-review**. Task 11 (docs) then a final whole-branch review remain. |

`feat/plan-a-measured-colours` branched from `2d04902`; `feat/plan-b-eink-photo` from `efd16b1` (Plan A's Task 1, for `device.colors_actual`). Both merge back into `feat/screen-store-authoring-core` at the end.

## Resume here

1. **Plan B, Task 10** — a scoped re-review is in flight (agent may have died; check `git log` in worktree B). Verdicts five findings; the only one that matters is the `colors_actual` coverage, which must be **mutation-verified**, not merely present.
2. **Plan B, Task 11** — docs + the `gphoto` example. It owns `CHANGES.md` and `docs/` for the whole plan; every earlier task left them alone. **Plan A's Task 7 brief was incomplete for the same reason** — write Task 11's dispatch listing every user-visible thing the plan shipped, don't trust the plan's task text.
3. **Plan B final whole-branch review** on the most capable model, pointed at the ledger's `minor (deferred)` lines.
4. **Then merge both into `feat/screen-store-authoring-core`**, run `make check` + `make docs`, remove the worktrees, delete both SDD workspaces.

The ledgers are the recovery map — trust them and `git log` over memory. Both are git-ignored, so they do **not** travel with a merge; read them before dispatching anything:
- `<worktree-a>/.superpowers/sdd/2026-08-06-plan-a-measured-colours-end-to-end/progress.md`
- `<worktree-b>/.superpowers/sdd/2026-08-06-plan-b-image-process-for-eink/progress.md`

Plan A's workspace was **deliberately not deleted** (the skill would delete it after a clean final review) — it is the map for the upcoming A+B merge.

## What Plan A shipped — settled, do not re-derive

The final whole-branch review verified by mutation that **the precedence chain is consistent across all four render paths**. All converge on `resolve_render_params` (prepends `SRC_SCRIPT`, applies the length rule uniformly), `resolve_measured_colors`, `resolve_use_actual`:

| path | pre-script chain | `use_actual` | warning to |
|---|---|---|---|
| `/api/display` `display.rs:756` | script > dev_override > panel > header | hardcoded false (correct) | `tracing::warn!` |
| `/dev/render` `dev.rs:482` | script > dev_override(query) > panel | `resolve_use_actual(query…)` | `tracing::warn!` |
| authoring `screen_store.rs:1002` | script > render_opts > panel | `resolve_use_actual(opts…)` | `log.push("[warn] …")` |
| CLI `main.rs:270` | script > panel | `resolve_use_actual(flag…)` | `tracing::warn!` |

Owner rulings executed this session: **the CLI now honours measured colours** (Task 5b, new `--use-actual` flag), and **the authoring UI gets `measured_source`** (Task 6, `RenderResult` → MCP `RenderDiagnostics`).

Also settled, with evidence, in the ledger — do not re-investigate: `/api/display`'s `use_actual=false` is correct and governs only the emitted PLTE; measured colours reach the ditherer regardless (proved empirically, not just traced). `resolve_ctx_palette` has **three** call sites in `display.rs`, not one. `resolve_use_actual` is never called from `handle_display` — that endpoint has no such option at all.

## Rulings that supersede the plans

- **Plan A Task 3 signature** (owner-authorised): `resolve_measured_colors(palette_len, &[MeasuredCandidate]) -> MeasuredResolution`. The length check sits **inside the loop** and is the only colour-yielding return path, so an unvalidated candidate cannot structurally reach a caller. Labels are the `SRC_*` consts — never a fresh literal.
- **Candidates are prepended, never collapsed to a winner first.** Collapsing is lossy: a wrong-length `colors_actual` would kill calibration instead of falling through. Mutation-verified twice.
- **Plan B shared test helper**: use `assert_close` / `assert_close_tol` at `crates/eink-photo/src/lib.rs:95-105`; ignore any brief snippet defining a local `close()`.
- **Plan B `box_blur`** is O(n·radius), not the O(n) the plan's comment promised. Comment corrected, algorithm deliberately left — it always runs post-resize on a panel-sized image.
- **Lua wrong-typed scalars stay silently ignored.** mlua coerces numeric strings (`exposure = "3.0"` works and is byte-identical); only non-numeric garbage is dropped, and that matches `http_request`, `qr_svg` and the dither options. Do not "fix" it.

## Operational traps that cost time this session

- **Agents stall on backgrounded commands — five times today.** Two distinct causes: (a) the agent explicitly backgrounds a command; (b) **`make check` exceeds the Bash tool's 120s default and is AUTO-backgrounded**. Fix (b) by passing `timeout: 600000` explicitly. Put both in every dispatch. **Always `git status` the worktree before assuming a stalled agent lost work — it never had.**
- Three agents also died on pure infrastructure (connection closed, watchdog). Same drill: check the worktree, then resume or re-dispatch. A stalled *reviewer* may leave a mutation in the tree — check `git status` before trusting anything.
- `mdbook-mermaid`'s generated JS is gitignored and missing in a fresh worktree, so `make docs` fails until you run `mdbook-mermaid install docs` (writes only gitignored files).
- Cap parallelism at 2 per stream (`CARGO_BUILD_JOBS=2`, `-- --test-threads=2`) — shared machine.
- Never `git add -A`; untracked local files exist in these worktrees.

## The thing that actually finds bugs here

**Mutation testing.** Every decisive finding across both plans came from a reviewer that mutated code and ran it, never from one that read rationale. Seven plan defects were found, **all in plan-authored test code**: tests mathematically unsatisfiable, one discriminating backwards, one whose subject was never invoked behind an `Option` guard, one that passed identically with the guard it tested entirely removed, a coverage matrix that looked complete while 14 of 17 parameters could be deleted undetected, a fixture whose `meta.yaml` keys meant screens never resolved, and a test premise that was numerically impossible to satisfy.

So: **dispatch reviewers to verify a claim by running it, not to judge prose.** Ask for the mutation and the failing test name. And when a fix substitutes something for a disputed test, review *the substitution's cost* — twice a correct dispute opened a new hole.

Two habits worth continuing to reward: implementers that **disclose gaps they could not close** (three real findings came from disclosures), and implementers that **correct their own earlier reports** unprompted.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. `make docs` needs `mdbook-mermaid`.
- **`cargo clippy -- -D warnings` skips test targets.** A `neg_cmp_op_on_partial_ord` in a test went unnoticed for this reason; it breaks the moment anyone adds `--all-targets`.
- Plan A baseline: 382 lib tests + all integration suites, 0 failed, 1 pre-existing ignored.
- Plan B: `eink-photo` 63 tests; `services::image_process` 17; `lua_api_test image_process` 25; `image_process_e2e_test` 3.
- `eink-photo` became a real dependency of `byonk` at Plan B Task 8 — the root `make check` now covers it. The separate `-p eink-photo` discipline is no longer load-bearing.

## Known gaps carried forward (triaged, none blocking)

- `/dev/render` is unreachable from `TestApp` (mounted only in `run_dev_server`), so its dev_override-vs-panel ordering is unguarded. `/api/display`'s equivalent IS reachable and could be tested.
- `SRC_SCRIPT` is not asserted end-to-end through MCP `RenderDiagnostics` (guarded at `RenderResult`; the remaining hop is one `.to_string()` exercised by three other labels).
- `content_hash` covers only the SVG, not `colors_actual`/palette/dither — **pre-existing**, confirmed untouched by Plan A.
- **`CONFIG_FILE` unset silently loads the embedded default config**, logged at `trace!` only. Failure mode is a successful render against the wrong config with `measured_source="none"` and no visible signal. Pre-existing; worth a follow-up issue.
- Plan B: EXIF orientation wiring is **known-unverified** (not "working"); `Fit::None` bypasses the output-dimension cap; `format="jpeg", quality=300` silently falls back to 90.

## Still outstanding from the previous initiative — never done

- [ ] **Drive a real MCP client** — `claude mcp add --transport http byonk http://localhost:3000/mcp --header "Authorization: Bearer <token>"`, then `list_screens` → `copy_screen` → edit → `render_screen` → `assign_screen`. The integration tests speak JSON-RPC directly and do not prove a real client negotiates the handshake.
- [ ] **Validate on the HA VM**, reaching `/mcp` from the Mac host over the LAN. See memories `ha-vm-from-source-addon-build` and `ha-vm-addon-manifest-sync-gap`; `make ha-rebuild` does **not** sync the add-on manifest.
- [ ] **Confirm `/mcp` returns 404** on an install with no admin token configured.
