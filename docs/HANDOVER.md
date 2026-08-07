# Handover — Byonk

_Last updated: 2026-08-07 — **Plan A and Plan B are merged; the branch is verified green; all three long-outstanding validation items are now DONE.** Nothing has been pushed or merged to `main`. `feat/screen-store-authoring-core` remains **HELD** by standing owner decision — no PR, no merge, no push. **The next action needs the owner's go-ahead.**_

## Where the work lives

| | |
|---|---|
| Branch | `feat/screen-store-authoring-core` |
| HEAD | `df3e444` |
| Worktree | `/Users/oetiker/checkouts/byonk` (the two plan worktrees are **removed**) |
| Ahead of `main` | 114 commits |
| State | `make check` green ("All checks passed!"), `make docs` green, tree clean |

Plan A (`feat/plan-a-measured-colours`) merged at `840a6c9`; Plan B (`feat/plan-b-eink-photo`) at `bc5444a`. Both branches are fully merged and **still exist** — delete them whenever you like. Both SDD workspaces are gone with their worktrees.

## What happened this session

1. **Verified the merge.** `make check` + `make docs` green; no conflict markers left in `CHANGES.md` or `docs/src/api/lua-api.md`; both plans' changelog entries coexist.
2. **Closed the A/B seam gap** (`4fa616e`) — see below.
3. **Removed both worktrees** and both SDD workspaces.
4. **Did all three outstanding validation items** — see below.
5. **Fixed two real defects the validation surfaced** (`df3e444`).

## The A/B seam test — `4fa616e`

`tests/image_process_e2e_test.rs::palette_aware_derives_its_endpoints_from_a_panels_measured_colors`.

A bare "measured panel differs from spec panel" assertion would have been **worthless** here: the neighbouring `measured_colors_change_the_final_dithered_output` already proves measured colours change the dithered PNG via the *ditherer*, a different consumer that keeps passing with `palette_aware` completely broken.

So it is a 2×2 over `{color, color_measured} × {palette_aware off, on}`, and the **control arm carries the argument**: the fixture's spec palette is `#000000..#FFFFFF`, whose `palette_endpoints` are exactly the `(0.0, 1.0)` the tone mapper already defaults to — so on that panel `palette_aware` is a provable byte-for-byte no-op. Only the measured panel's compressed range can move the output.

**Mutation-verified**: changing `lua_runtime.rs`'s captured palette from `colors_actual.or(colors)` to `colors` collapses the measured arm onto its control and fails the test, while the control assertion still passes.

## The three validation items — all DONE

**1. Drive a real MCP client — DONE.** `claude mcp add --transport http … /mcp` + a headless client run, both against a local server and the HAOS add-on. Full authoring loop exercised: `list_screens` → `copy_screen` (fork a builtin) → `write_screen_file` → `render_screen` (real dithered PNG, script `data` echoed back) → `assign_screen` (verified it rewrote `config.yaml`).

**2. HA VM over the LAN — DONE.** Built this branch from source in the VM as `local_byonk` `0.17.1-src3`, reached `/mcp` from the Mac host at `:3000` over the NAT. Handshake `2025-06-18`, server `byonk 0.17.1`, 14 tools, 12 resources, `byonk://reference/lua-api` served (46 KB), `render_screen` returned a real PNG. A real MCP client also completed the whole fork→edit→render loop against it.

**3. `/mcp` 404s with no admin token — DONE.** Confirmed locally *and* on the VM, as a **404 → 401 → 200** progression on one binary, which is what makes it meaningful: 404 = admin disabled, 401 = wrong/absent bearer, 200 = authorised.

⚠️ **A 404 alone does not prove the gate.** The VM's published add-on (`43664941_byonk` v0.17.0) also 404s on `/mcp` — because `src/mcp` does not exist at `v0.17.0` at all. Always pair a 404 with a version check.

## Defects found and fixed this session — `df3e444`

- **`tools/ha-vm/rebuild.sh` was missing `docs/src`** from its build-context sync list. `EmbeddedDocs` (`src/assets.rs`) rust-embeds three pages from `docs/src/` so MCP can serve them as `byonk://reference/*`; without them the add-on build dies with `folder '/build/docs/src/' does not exist`. **Any new `#[derive(RustEmbed)]` folder is a build input and must be added to that list.**
- **`docs/src/guide/authoring.md` claimed the MCP tools "aren't wired up yet".** That page *is* one of the three embedded pages — served as `byonk://reference/authoring` — so an LLM author was handed a contract saying the tools it is holding do not exist. Replaced with the real fork-from-MCP workflow.

## Known gaps carried forward (triaged, none blocking)

Two MCP API rough edges, **found only because a real client was driven** — direct JSON-RPC tests cannot surface them. Neither is fixed; both are cheap:

- **`copy_screen`'s `to_handle` / `to_name` have no doc comments**, so they reach the client with empty schema descriptions. A real client guessed `to_handle="local/mcpval"` and failed twice before landing on `to_handle="local"`, `to_name="mcpval"`. `to_name` is also a path segment, **not** the title — the copy keeps the source's `meta.yaml` title.
- **`assign_screen`'s description is misleading**: it says a MAC `list_devices` never reported is rejected, but `list_devices` only reports registry-*seen* devices while `assign_screen` also accepts any MAC already in `config.devices`. So a configured-but-never-connected device is assignable yet invisible to the tool the error text tells you to call. A genuinely unknown MAC *is* correctly rejected (verified).

Carried over from the plans, unchanged:

- `/dev/render` is unreachable from `TestApp` (mounted only in `run_dev_server`), so its dev_override-vs-panel ordering is unguarded. `/api/display`'s equivalent IS reachable and could be tested.
- `SRC_SCRIPT` is not asserted end-to-end through MCP `RenderDiagnostics`.
- `content_hash` covers only the SVG, not `colors_actual`/palette/dither — pre-existing.
- **`CONFIG_FILE` unset silently loads the embedded default config**, logged at `trace!` only — a successful render against the wrong config with no visible signal. Pre-existing; worth an issue.
- Plan B's six triaged items, incl. EXIF orientation being **known-unverified**, `Fit::None` bypassing the output-dimension cap, and a docs/field-count mismatch.

## Settled — do not re-derive

- The measured-colour precedence chain is consistent across all four render paths, verified by mutation; all converge on `resolve_render_params` / `resolve_measured_colors` / `resolve_use_actual`.
- `/api/display`'s hardcoded `use_actual=false` is correct and governs only the emitted PLTE; measured colours reach the ditherer regardless.
- Candidates are **prepended, never collapsed to a winner first** — collapsing is lossy.
- Lua wrong-typed scalars stay silently ignored; that matches `http_request`/`qr_svg`. Do not "fix" it.

## Build / verify

- `make check` = fmt + `clippy -- -D warnings` + tests. **Pass `timeout: 600000`** — it exceeds the Bash tool's 120 s default and gets auto-backgrounded otherwise.
- `make docs` needs `mdbook-mermaid`; in a fresh worktree run `mdbook-mermaid install docs` first.
- `cargo clippy -- -D warnings` **skips test targets** — a lint in a test will not be caught until someone adds `--all-targets`.
- Cap parallelism at 2 (`CARGO_BUILD_JOBS=2`) — shared machine. Never `git add -A`.

## HA VM state

Left **running**; stop with `make ha-vm-stop`. The published add-on `43664941_byonk` v0.17.0 was restored to started; `local_byonk` v0.17.1-src3 is installed but stopped. See memories `ha-vm-from-source-addon-build` (updated today — includes the `ha addons options` subcommand not existing, and the Supervisor-REST-API workaround) and `ha-vm-addon-manifest-sync-gap`.

## The thing that actually finds bugs here

**Mutation testing, and driving the real thing.** Every decisive finding came from running a mutation or exercising a real client — never from reading rationale. This session: the seam test was only trusted after a mutation killed it, and both `df3e444` defects were invisible to the whole test suite because nothing built the add-on or read the doc as a client.

## Next action

Owner decision on the HELD branch: PR + merge to `main`, or keep holding. Optionally first: the two MCP doc/schema fixes above, and delete the two merged plan branches.
