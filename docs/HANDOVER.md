# Handover — Byonk

_Last updated: 2026-08-06 — **Plans A and B are being executed in parallel in two git worktrees, by subagent-driven development.** Plan A: 4 of 7 tasks complete. Plan B: 7 of 11 complete. Every completed task passed a task review and, where needed, a fix round plus a scoped re-review. Nothing has been merged back yet; `feat/screen-store-authoring-core` is untouched at `2d04902` and remains **HELD** — no PR, no merge, no push, by standing user decision._

## Where the work lives

Two worktrees, both outside the project (this Mac has no `/scratch`):

| Stream | Worktree | Branch | HEAD | State |
|---|---|---|---|---|
| **Plan A** | `/Users/oetiker/checkouts/claude-worktrees/byonk-plan-a` | `feat/plan-a-measured-colours` | `58d334a` | Tasks 1–4 of 7 done. Suite **628 passed / 0 failed / 1 ignored**. Tree clean. |
| **Plan B** | `/Users/oetiker/checkouts/claude-worktrees/byonk-plan-b` | `feat/plan-b-eink-photo` | `6b7ff9d` | Tasks 1–7 of 11 done. **63** `eink-photo` tests. Tree clean. |

`feat/plan-a-measured-colours` branched from `2d04902`. `feat/plan-b-eink-photo` branched from **`efd16b1`** — Plan A's Task 1 — because Plan B's Task 9 (`palette_aware`) needs `device.colors_actual`. That dependency is already satisfied; nothing needs dropping.

**Both branches merge back into `feat/screen-store-authoring-core` at the end.** Expect conflicts only in `CHANGES.md`, `docs/src/api/lua-api.md`, and possibly `lua_runtime.rs` (Plan A touched the script-return parsing; Plan B Task 9 adds the `image_process` global).

## Resume here

Run both streams with **`superpowers:subagent-driven-development`**, one fresh implementer per task, a task review after each, a fix round plus scoped re-review when the review finds Critical/Important issues.

- **Plan A — next is Task 5** (`svg_to_png` warns instead of silently dropping mismatched measured colours), then Task 6 (`use_actual` + `colors_actual` on `render_screen`), Task 7 (docs — owns `CHANGES.md` and `docs/src/api/lua-api.md`).
- **Plan B — next is Task 8** (the codec and geometry layer, `src/services/image_process.rs`; **adds the `image` 0.25 dependency** — its Step 1 says to stop and report if the lockfile gains a duplicate decoder), then Task 9 (the `image_process` Lua global), Task 10 (the end-to-end test), Task 11 (docs + the `gphoto` example).
- Then a **final whole-branch review per stream** on the most capable model, pointed at the ledger's deferred-minor and parked lines.
- Then merge both into `feat/screen-store-authoring-core`, run `make check` + `make docs`, and remove the worktrees.

The ledgers are the recovery map — trust them and `git log` over memory:
- `<worktree-a>/.superpowers/sdd/2026-08-06-plan-a-measured-colours-end-to-end/progress.md`
- `<worktree-b>/.superpowers/sdd/2026-08-06-plan-b-image-process-for-eink/progress.md`

Both are git-ignored, so they do **not** travel with a merge. Read them before dispatching anything.

## Rulings that supersede the plans — do not re-derive these

**Plan A, Task 3 signature (owner-authorised).** The plan's two-argument `resolve_measured_colors(palette_len, script, fallback, fallback_source)` extracted only the *top* link of a four-link chain. Replaced by:

```rust
pub type MeasuredCandidate = (&'static str, Option<Vec<(u8, u8, u8)>>);
pub fn resolve_measured_colors(palette_len: usize, candidates: &[MeasuredCandidate]) -> MeasuredResolution
```

It walks candidates in precedence order, applies the length rule **uniformly at every position** (the check sits inside the loop and is the only return path yielding colours, so an unvalidated candidate cannot structurally reach the caller), accumulates warnings joined with `"; "`, and returns the first survivor or `SRC_NONE`. Labels are the consts `SRC_SCRIPT` / `SRC_DEV_OVERRIDE` / `SRC_PANEL_ACTUAL` / `SRC_MEASURED_HEADER` / `SRC_NONE` at `src/api/display.rs:197-201` — never a fresh string literal.

**Plan B, shared test helper (owner-authorised).** Plan B's Global Constraints mandate a shared `assert_close`, but its task code blocks kept defining local per-module epsilon helpers. **The constraint governs.** Use `assert_close(a, b)` / `assert_close_tol(a, b, tol)` at `crates/eink-photo/src/lib.rs:95-105`; plain `assert!` for inequalities is the accepted pattern. Deviate from any brief snippet that defines a local `close()`.

**Plan B, `box_blur` complexity (controller ruling).** The plan's doc comment promised an O(n) running sum; the code is O(n·radius). **Corrected the comment, did not rewrite the algorithm** — `image_process.rs` owns decode/crop/**resize** as steps 1–3 and `eink-photo` holds steps 4–16, so the blur always runs post-resize on a panel-sized image (~800×480, ~1.5e8 tap operations at the radius-40 clamp), never on a 4000px source.

## The verification gap that has been silently weakening Plan B

**A clean root `make check` proves nothing about `crates/eink-photo`.** The crate is a workspace member but *not* a dependency of `byonk`, and the Makefile's clippy/test invocations are unscoped, so the root gate does not compile, lint, or test it. This was true and unnoticed for two tasks. Every Plan B dispatch must require, and every review must confirm, **pasted output** of:

```
cargo test -p eink-photo -- --test-threads=2
cargo clippy -p eink-photo -- -D warnings      # no --all-targets; that's where allow(dead_code) is load-bearing
```

This resolves itself once Task 9 wires the crate into `byonk`. Until then it does not.

## What is done

### Plan A (`docs/superpowers/plans/2026-08-06-plan-a-measured-colours-end-to-end.md`)

1. **`efd16b1`** — `device.colors_actual` readable from Lua; `nil`, never mirrored, when uncalibrated. Hoisted `DeviceContext` construction above measured-colour resolution in `dev.rs` and `main.rs`.
2. **`2b1bfb2`** — scripts can return `colors_actual`. `lua_runtime::ScriptResult.colors_actual` / `content_pipeline::ScriptResult.script_colors_actual`. The `colors`/`script_colors` naming asymmetry is **deliberate** — do not "fix" it.
3. **`22247d4`** — the chain as a pure function (candidate-list signature above), 9 tests covering every position class. `parse_measured_color_list` parses per entry; `parse_hex_color` factored out.
4. **`58d334a`** — wired into all four render paths, `main.rs` duplication collapsed, `measured_source` exposed on `RenderParams`, pre-script winner derived from the array so the two cannot drift.

### Plan B (`docs/superpowers/plans/2026-08-06-plan-b-image-process-for-eink.md`)

1. **`1d34ad3`** scaffold — `Params`, `Preset`, `Sharpen`, `PhotoError`, pass-through `process`. **Zero dependencies, and `[dependencies]` stays empty for the whole plan.**
2. **`19712f5`** `color.rs` — true piecewise IEC 61966-2-1 transfer pair (0.04045 / 0.0031308 / 2.4), `luminance`; `apply_exposure` as a linear-light multiply.
3. **`5b33cd0`** white balance (linear light), `measure_endpoints` (0.005/0.995 percentiles), `apply_endpoints` (tone domain).
4. **`90bea2f`** highlights/shadows, contrast, `apply_curve` (constructs `BadCurve`).
5. **`e74618f`** `presence.rs` — separable box blur (clamped borders), clarity, sharpen.
6. **`c83df93`** `colorops.rs` — vibrance, saturation, grayscale, invert.
7. **`6b7ff9d`** the assembled pipeline, `Preset::Eink` as a base layer, `palette_aware` endpoints, `OutOfRange` finally constructed, all `allow(dead_code)` removed.

**The assembled order, verified against each operation's body:** `exposure → white balance` | `endpoints → highlights/shadows → contrast → curve` | `clarity → vibrance → saturation → grayscale → invert` | `sharpen`. `apply_exposure`/`apply_white_balance` round-trip internally, so the buffer is in the **tone domain at every step boundary**.

## Two things needing a decision

1. **The CLI render path ignores measured colours entirely.** `src/main.rs:366-398` computes `render_params`, but `:414-418` passes `None, false` to `render_png_from_svg` — `measured_colors` and `measured_source` are never destructured out. **Pre-existing**, not introduced by Plan A, confirmed identical at `22247d4`. Needs an explicit call before any later task assumes the CLI honours measured colours.
2. **`screen_store` never surfaces `measured_source`.** The authoring path pushes the *warning* into `RenderResult.log` but not the winning label. If the authoring UI should render it verbatim the way the dev tuning popup does, that wiring is still missing.

## Settled and traced — do not re-investigate

**`/api/display` hardcodes `use_actual=false` (`display.rs:1145`) and that is CORRECT.** It governs only the emitted palette (PLTE / grey LUT, `svg_to_png.rs:353-364`). `measured_colors` still reaches the ditherer unconditionally: `display.rs:963` `.with_colors_actual` → `content_cache` → `:1114` read back → `:1144` as the `actual` argument → `svg_to_png.rs:342-349` builds `eink_actual` **without consulting `use_actual`** → `EinkPalette::new` → `Palette::new` precomputes `actual_srgb/linear/oklab/chroma`, and matching uses the actual colours (`crates/eink-dither/src/palette/palette.rs:150-231`). Measured colours steer which palette **index** each pixel gets while PLTE stays nominal — right, because the device maps index → physical ink. **Plan A's device path is functional.**

## Plan defects found so far — the plans' text, not the implementations

Four confirmed, all in plan-authored test code. This matches the previous initiative's pattern exactly (twelve tasks, twelve plan defects, every one in the plan's text).

1. **Plan A Task 2** — a verbatim test used `r#"..."#` around Lua containing `"#000000"`, which terminates the raw string early and does not compile. Fixed to `r##"..."##`; verified byte-for-byte that only the delimiters changed.
2. **Plan B Task 5** — `sharpen_raises_edge_contrast_more_than_clarity_at_the_same_amount` is **mathematically unsatisfiable**. Box-blur unsharp amplitude is `1 + k(1 − (2r+1)^-6)`, monotonically increasing in radius, so clarity always beats sharpen on *any* magnitude metric (variance 0.2304 vs 0.2302, step edge 0.0956 vs 0.0922, overshoot 0.9606 vs 0.9333). The plan's 128×128 escape hatch does not help. Replaced with a spatial-**footprint** test (768 vs 2304 px, 3× margin).
3. **Plan B Task 6** — the amount=50 cases are unsatisfiable against the plan's *own* reference implementation (both rails clamp), and worse, the vibrance test **discriminates backwards**: a plain-saturation stand-in scores 1.1911 while real vibrance scores 1.1794, because the signal there is almost entirely clamping asymmetry, which vibrance's weighting *reduces*.
4. **Plan B Task 7** — `process_applies_operations_in_the_fixed_order` sets no `sharpen`, and `process` guards the call with `if let Some(s) = p.sharpen`, so `apply_sharpen` was **never invoked**; its position was unobservable by construction.

## Process lessons earned in this run

- **A correct dispute can still open a hole. Check what a substitution *costs*, not just whether it was right.** Twice: Task 5's footprint substitute let a no-op `apply_sharpen` pass the entire suite (`fs < fc` → `0 < 2304` → true); Task 6's rework carried a tolerance over unrescaled so the test passed the very mutant its comment claimed to catch. Both were caught by pointing the review at the substitution rather than the dispute.
- **Mutation testing is what made the hardest reviews decisive.** Task 7's reviewer copied the crate to a scratch dir, moved operations, and re-ran — proving one test was the sole detector of a domain-group swap and that sharpen's position was entirely unpinned. Ask reviewers to do this when ordering or wiring is the risk.
- **Ask reviewers to verify a claim, not to judge prose.** Every substantive catch this run came from a reviewer that re-derived arithmetic or ran code, not one that read a rationale.
- **The foreground rule needs enforcing, not just stating.** Two implementers backgrounded `make check` and stalled waiting for a notification subagents never receive — despite the rule being the literal first line of their dispatch. Both were recovered by resuming with the correction; no work was lost. Check `git status` in the worktree before assuming a stalled agent lost anything.
- **Dispatch fresh past ~150k transcript tokens.** Plan A's Task 4 implementer reached ~235k and finished, but the fix round went to a fresh agent carrying the brief, the report file, and the findings — which worked cleanly.
- **Implementers disclosing gaps they could not close is working and should keep being praised.** Task 7's implementer flagged a coverage gap rather than claiming it covered; the reviewer then showed it was one parameter away from closable.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. `make docs` needs `mdbook-mermaid`.
- **Cap parallelism at 2 per stream** while both run (`CARGO_BUILD_JOBS=2`, `-- --test-threads=2`) — shared machine, combined limit 4.
- Never `git add -A`/`.` — stage explicit paths, check `git diff --cached`. Untracked local files exist here.
- `CHANGES.md` is user-facing only. Plan A Task 7 and Plan B Task 11 own the docs; earlier tasks correctly touch neither.
- If `cargo` is missing, add `$HOME/.cargo/bin` to PATH (rustup via `rust-toolchain.toml`; never add cargo/rust to mise).

## Still outstanding from the previous initiative — never done

Unaffected by this work. Do them after both plans land, or whenever asked.

- [ ] **Drive a real MCP client** — `claude mcp add --transport http byonk http://localhost:3000/mcp --header "Authorization: Bearer <token>"`, then `list_screens` → `copy_screen` → edit → `render_screen` → `assign_screen`. A green suite does not prove a real client negotiates the handshake; the integration tests speak JSON-RPC directly.
- [ ] **Validate on the HA VM**, reaching `/mcp` from the Mac host over the LAN. See memories `ha-vm-from-source-addon-build` and `ha-vm-addon-manifest-sync-gap`; `make ha-rebuild` does **not** sync the add-on manifest.
- [ ] **Confirm `/mcp` returns 404** on an install with no admin token configured.
