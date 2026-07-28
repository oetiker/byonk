# MCP Interface Implementation Plan (Spec 1, Plan 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose byonk's `ScreenStore` over an admin-token-gated MCP endpoint at `/mcp`, so an LLM can author screens against a byonk running anywhere on the LAN — including inside Home Assistant — with no filesystem access and no Samba share.

**Architecture:** An `rmcp` streamable-HTTP tower service is mounted on the main axum router at `/mcp`, behind the same `require_admin` Bearer check that gates `/api/admin/*` (no token configured ⇒ 404, invisible). A single `ByonkMcp` handler holds `AppState` and delegates every tool to the existing `ScreenStore` / `ScreenRepoManager` / config-writer paths — the MCP layer adds no screen logic of its own. Authoring contracts (Lua API, SVG/base-library reference, `meta.yaml` JSON Schema) are served as MCP resources from the already-maintained `docs/src/` pages, embedded with `rust-embed`.

**Tech Stack:** Rust (stable, rustup-managed), axum 0.8, `rmcp` 2.2 (latest **stable**; 3.0 is beta — do not use it), `rust-embed` 8, `schemars` 1.0 via `rmcp::schemars`, `tokio`.

## Global Constraints

- **`rmcp` version is `2.2`** — the latest stable release. `3.0.0-beta.4` exists; it is a prerelease and is out of scope.
- **Never `git add -A` or `git add .`** — stage explicit paths, then verify with `git diff --cached` before committing. Untracked local files exist in this tree and must not be swept in.
- **`CHANGES.md` is user-facing only** — describe user-visible changes; keep CI/tooling/dev-process out.
- **`byonk-base-v1` is untouched** by this plan.
- **The `byonk-builtin` handle string is frozen** — `content_pipeline.rs:215` hard-references `byonk-builtin/default`.
- **Release image is `FROM scratch` and has no `/tmp`** — never rely on `std::env::temp_dir()`; scratch state goes under `/data`.
- **Cap parallelism at 4** for all compiles and test runs (`cargo test -- --test-threads=4`, `make` targets already behave). The build machine is shared.
- **English** for all identifiers, comments, and technical docs.
- **Gates:** `make check` (fmt + `clippy -- -D warnings` + tests) and `make docs` must be green at the end of every task.
- **`ScreenStore::new` must keep receiving the same `Arc<ScreenRepoManager>` the `ContentPipeline` holds** — guarded by `tests/screen_store_wiring_test.rs`. Do not restructure that wiring.

---

## Key decisions locked before implementation

These were settled against the live `rmcp` 2.2 source; they are not open questions.

1. **Host validation is disabled** (`.disable_allowed_hosts()`). `rmcp`'s default `allowed_hosts` is `["localhost", "127.0.0.1", "::1"]`, which rejects the primary use case (driving byonk at `homeassistant.local:3000` over the LAN). The Bearer admin token already defeats DNS rebinding — a rebound browser request carries no token and gets 401. Origin validation stays off (rmcp's default: empty list ⇒ disabled).

2. **Stateless mode with JSON responses** (`stateful_mode: false`, `json_response: true`, `NeverSessionManager`). byonk's MCP surface is pure request/response: no sampling, no progress notifications, no resource subscriptions. Stateless means no in-memory session table, nothing to leak or restore across restarts, and plain `application/json` responses instead of SSE framing — which also keeps the integration tests to straightforward JSON assertions. If server→client notifications are ever needed, switching to `stateful_mode: true` + `LocalSessionManager` is a two-line change in one place (`src/mcp/mod.rs`).

3. **Every POST to `/mcp` must carry `Accept: application/json, text/event-stream`** — rmcp enforces this in both modes (`tower.rs:1016`) and returns 406 otherwise. The test helper sets it.

4. **`Implementation::from_build_env()` must NOT be used.** Its `env!("CARGO_CRATE_NAME")` expands inside rmcp at rmcp's compile time, so it reports `rmcp` and rmcp's version, not byonk's. `get_info` sets `server_info` explicitly.

5. **`schemars` is used via `rmcp::schemars`** (rmcp re-exports it at `src/lib.rs:38`). Do not add a separate `schemars` dependency — a version skew against rmcp's `1.0` would break the derives.

6. **Tools are split across modules** using `#[tool_router(router = <name>, vis = "pub")]` on several inherent `impl ByonkMcp` blocks, combined with `+` in `ByonkMcp::new`. This is the documented rmcp pattern for multi-file tool sets.

7. **All `ScreenStore` calls from MCP handlers go through `tokio::task::spawn_blocking`.** `ScreenStore` is entirely synchronous and `render` runs Lua (with blocking HTTP), resvg, and dithering. This matches the existing pattern at `src/api/dev.rs:496` and `src/api/display.rs:698`.

8. **A failed operation is a *tool-level* error — `Ok(CallToolResult::error(...))`, never `Err(ErrorData)`.** This is rmcp's documented rule (`model.rs:2999`): MCP clients render protocol errors opaquely ("Tool result missing due to internal error") and the message never reaches the model, whereas a tool-level error's `content` is shown. Since the whole value of `StoreError::ReadOnly` is its `copy_screen` hint telling the agent what to do next, every `ScreenStore` failure must surface as a tool-level error. `Err(ErrorData)` is reserved for genuine protocol faults — a panicked `spawn_blocking` task, or a response that fails to serialize. Consequence: every fallible tool returns `Result<CallToolResult, ErrorData>`, not `Json<T>`.

9. **`CallToolResult` has no builder for structured content.** Its fields are public (`content`, `structured_content`, `is_error`, `meta`); construct it directly. `CallToolResult::success(v)` sets `is_error: Some(false)` and `structured_content: None`.

---

## File structure

**New files**

| File | Responsibility |
|---|---|
| `src/mcp/mod.rs` | `ByonkMcp` handler struct, `ServerHandler` impl (`get_info` + resource methods), the `/mcp` mount + auth middleware, shared error mapping |
| `src/mcp/tools_read.rs` | Read-context tools: `list_screens`, `read_screen_file`, `list_screen_repos`, `list_devices`, `get_config` |
| `src/mcp/tools_edit.rs` | Mutating tools: `write_screen_file`, `create_screen`, `copy_screen`, `rename_screen`, `delete_screen`, `delete_screen_file` |
| `src/mcp/tools_render.rs` | `render_screen`, `validate_screen` |
| `src/mcp/tools_device.rs` | `assign_screen` |
| `src/mcp/resources.rs` | Embedded `docs/src/` reference pages + the generated `meta.yaml` JSON Schema, exposed as MCP resources |
| `tests/mcp_transport_test.rs` | Auth/handshake/protocol-level tests |
| `tests/mcp_tools_test.rs` | Tool behaviour tests |
| `tests/mcp_resources_test.rs` | Resource listing/reading tests |
| `tests/common/mcp.rs` | `McpTestClient` helper (JSON-RPC over `TestApp`) |
| `tests/common/store.rs` | `build_store(dir, screens)` fixture shared by Tasks 2–4 (created in Task 2) |
| `tests/screen_repo_symlink_test.rs` | Task 1 — escape-guard tests |
| `tests/screen_store_limits_test.rs` | Task 2 — size-cap and UTF-8 reporting tests |
| `tests/screen_store_concurrency_test.rs` | Task 3 — concurrent-mutation tests |
| `tests/screen_store_listing_test.rs` | Task 4 — `list_screens` / `delete_file` tests |
| `tests/screen_meta_schema_test.rs` | Task 10 — schema/parser agreement tests |
| `docs/src/guide/mcp.md` | User guide: connecting an LLM to byonk over MCP |

**Modified files**

| File | Change |
|---|---|
| `src/services/screen_repo_loader.rs` | Symlink-safe reads (Task 1); `read_limited` trait method (Task 2); `ScreenRepoKind` + `kind()` (Task 6) |
| `src/assets.rs` | Symlink-safe `SCREENS_DIR` overlay read (Task 1); embed the reference docs (Task 11) |
| `src/services/screen_store.rs` | Bounded reads (Task 2), mutation lock (Task 3), `list_screens` + `delete_file` (Task 4) |
| `src/models/screen_meta.rs`, `src/models/param_schema.rs` | `JsonSchema` derives + `meta_json_schema()` (Task 10) |
| `src/api/admin/write.rs` | Extract a reusable device-upsert core (Task 9) |
| `src/server.rs` | Mount `/mcp` in `build_router` (Task 5) |
| `src/lib.rs` | `pub mod mcp;` (Task 5) |
| `Cargo.toml` | `rmcp` dependency (Task 5) |
| `docs/src/SUMMARY.md`, `CHANGES.md` | Task 12 |

---

## Task 1: Symlink-safe reads in every disk-backed screen source

Closes precondition 2 from `docs/HANDOVER.md`. Today `GitScreenRepoSource::read` and `LocalScreenRepoSource::read` apply only the *lexical* `is_safe_rel` check, so a repo containing `leak.txt -> /etc/passwd` serves that file's contents. The write path is already canonicalize-guarded; the read path is not.

**Scope note — this goes one step beyond the handover's list.** `EmbeddedBuiltinSource::read` (`screen_repo_loader.rs:390`) delegates to `AssetLoader::read_screen`, whose `SCREENS_DIR` overlay branch (`assets.rs:207-214`) does a bare `fs::read(dir.join(rel))` and has exactly the same exposure. Fixing only the two named sources would leave `byonk-builtin/<path>` as an open read primitive. All three are fixed here.

**Files:**
- Modify: `src/services/screen_repo_loader.rs` (add helper; `GitScreenRepoSource::read` ~line 243, `LocalScreenRepoSource::read` ~line 296)
- Modify: `src/assets.rs:207-214` (the `read_screen` overlay branch)
- Test: `tests/screen_repo_symlink_test.rs` (new)

**Interfaces:**
- Consumes: `is_safe_rel(rel: &str) -> bool` (`screen_repo_loader.rs:98`)
- Produces: `pub(crate) fn read_within(root: &Path, rel: &str) -> Option<Vec<u8>>` in `screen_repo_loader.rs` — used again by Task 2

- [ ] **Step 1: Write the failing test**

Create `tests/screen_repo_symlink_test.rs`:

```rust
//! A screen repo containing a symlink that points outside its root must not
//! serve the target's contents. Unix-only: symlink creation on Windows needs
//! elevated privileges and byonk targets Linux/macOS.
#![cfg(unix)]

use byonk::services::screen_repo_loader::{LocalScreenRepoSource, ScreenRepoSource};

/// Build a minimal writable repo at `root` with one real screen, plus a
/// symlink `leak.txt` pointing at `secret` outside the repo.
fn fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("hello")).unwrap();
    std::fs::write(
        repo.join("byonk-screens.yaml"),
        "name: local\ndescription: d\nauthor: a\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("hello/meta.yaml"),
        "title: Hello\ndescription: d\nbyonk: \"0.17\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("hello/script.lua"), "return {}\n").unwrap();
    std::fs::write(repo.join("hello/screen.svg"), "<svg/>\n").unwrap();

    let secret = dir.join("secret.txt");
    std::fs::write(&secret, "TOP SECRET").unwrap();
    std::os::unix::fs::symlink(&secret, repo.join("leak.txt")).unwrap();
    std::os::unix::fs::symlink(&secret, repo.join("hello/leak.txt")).unwrap();
    repo
}

#[test]
fn test_symlink_escaping_repo_root_is_not_readable() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = fixture(tmp.path());
    let src = LocalScreenRepoSource::load(&repo).expect("load repo");

    assert!(
        src.read("leak.txt").is_none(),
        "symlink at the repo root leaked its target"
    );
    assert!(
        src.read("hello/leak.txt").is_none(),
        "symlink inside a screen dir leaked its target"
    );
}

#[test]
fn test_real_files_still_read() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = fixture(tmp.path());
    let src = LocalScreenRepoSource::load(&repo).expect("load repo");

    let bytes = src.read("hello/script.lua").expect("real file must read");
    assert_eq!(bytes, b"return {}\n");
}

#[test]
fn test_symlink_staying_inside_repo_still_reads() {
    // Escape is the thing being blocked — an internal symlink is legitimate
    // (e.g. a shared asset linked into two screens) and must keep working.
    let tmp = tempfile::tempdir().unwrap();
    let repo = fixture(tmp.path());
    std::os::unix::fs::symlink(repo.join("hello/script.lua"), repo.join("hello/alias.lua")).unwrap();
    let src = LocalScreenRepoSource::load(&repo).expect("load repo");

    assert_eq!(
        src.read("hello/alias.lua").expect("internal symlink must read"),
        b"return {}\n"
    );
}
```

Check whether `tempfile` is already a dev-dependency; if not, add `tempfile = "3"` under `[dev-dependencies]` in `Cargo.toml`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test screen_repo_symlink_test -- --test-threads=4`
Expected: `test_symlink_escaping_repo_root_is_not_readable` FAILS — `read` returns `Some("TOP SECRET")`.

- [ ] **Step 3: Add the shared helper**

In `src/services/screen_repo_loader.rs`, next to `is_safe_rel`:

```rust
/// Read `root/rel`, refusing anything that resolves outside `root`.
///
/// `is_safe_rel` is a *lexical* guard — it stops `../` and absolute paths in
/// the request string, but cannot see a symlink planted on disk. A repo is
/// arbitrary content (git-fetched, Samba-dropped, or hand-placed), so the
/// resolved target is canonicalized and prefix-checked against the
/// canonicalized root before any bytes are read. Symlinks that stay inside
/// the repo still resolve normally.
pub(crate) fn read_within(root: &Path, rel: &str) -> Option<Vec<u8>> {
    if !is_safe_rel(rel) {
        return None;
    }
    let canon_root = std::fs::canonicalize(root).ok()?;
    let canon_target = std::fs::canonicalize(canon_root.join(rel)).ok()?;
    if !canon_target.starts_with(&canon_root) {
        tracing::warn!(
            root = %canon_root.display(),
            rel,
            "refused screen-repo read escaping the repo root"
        );
        return None;
    }
    std::fs::read(&canon_target).ok()
}
```

Add `use std::path::Path;` to the file's imports if it is not already there.

- [ ] **Step 4: Use it in both disk sources**

Replace the body of `GitScreenRepoSource::read` and `LocalScreenRepoSource::read` (both currently `if !is_safe_rel(rel) { return None; } std::fs::read(self.manifest_root.join(rel)).ok()`) with:

```rust
    fn read(&self, rel: &str) -> Option<Vec<u8>> {
        read_within(&self.manifest_root, rel)
    }
```

- [ ] **Step 5: Close the same hole in the `SCREENS_DIR` overlay**

In `src/assets.rs`, `read_screen`, replace the overlay branch:

```rust
        // Try external first if path configured
        if let Some(ref dir) = self.screens_dir {
            // Canonicalize-and-prefix-check: `SCREENS_DIR` is user-writable
            // (Samba share, HA `/config/screens`), so a symlink planted there
            // must not become a read primitive for the whole filesystem. This
            // is the same guard `screen_repo_loader::read_within` applies to
            // git/local repos; kept inline here because `read_screen` returns
            // `io::Result<Cow<'static, [u8]>>`, not `Option<Vec<u8>>`.
            let full_path = dir.join(relative_path);
            if full_path.exists() {
                // Read the CANONICALIZED path, not `full_path` — checking one
                // path and reading another re-follows the symlink and leaves a
                // swap window open on exactly the user-writable directory this
                // guard defends. `read_within` does the same.
                let checked = std::fs::canonicalize(dir)
                    .ok()
                    .zip(std::fs::canonicalize(&full_path).ok())
                    .filter(|(root, target)| target.starts_with(root))
                    .map(|(_, target)| target);
                if let Some(target) = checked {
                    tracing::trace!(path = %target.display(), "Loading screen from filesystem");
                    return Ok(Cow::Owned(fs::read(&target)?));
                }
                tracing::warn!(
                    path = %full_path.display(),
                    "refused SCREENS_DIR read escaping the screens directory"
                );
            }
        }
```

Note the behaviour on refusal: it falls through to the embedded lookup, which is correct — a rejected overlay path is treated as "not present on disk".

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --test screen_repo_symlink_test -- --test-threads=4`
Expected: 3 tests PASS.

- [ ] **Step 7: Run the full gate**

Run: `make check`
Expected: fmt clean, clippy clean, all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/services/screen_repo_loader.rs src/assets.rs tests/screen_repo_symlink_test.rs Cargo.toml
git commit -m "fix: reject symlinks escaping a screen repo on every read path"
```

---

## Task 2: Bound every screen-file read by `MAX_FILE_BYTES`

Closes precondition 1. `ScreenStore::validate` reads via `source.read_string(...)` with no size cap, so an API-triggered validate on a git-fetched or Samba-dropped file is an unbounded read into memory. `read_file` checks the length only *after* the whole file is in memory, so its cap does not actually bound the allocation either.

This task adds a size check that happens **before** the read for disk-backed sources, and fixes a latent mis-report: today a non-UTF-8 file makes `read_string` return `None`, which `validate` reports as `"file not found"`.

**Files:**
- Modify: `src/services/screen_repo_loader.rs` (trait method + two disk overrides)
- Modify: `src/services/screen_store.rs` (`read_file` ~line 330, `validate` ~line 708)
- Test: `tests/screen_store_limits_test.rs` (new)

**Interfaces:**
- Consumes: `read_within` (Task 1); `MAX_FILE_BYTES` (`screen_store.rs:21`)
- Produces:
  - `pub enum ReadOutcome { Found(Vec<u8>), Missing, TooLarge }` in `screen_repo_loader.rs`
  - `fn ScreenRepoSource::read_limited(&self, rel: &str, max_bytes: usize) -> ReadOutcome`

- [ ] **Step 1: Write the failing test**

Create `tests/screen_store_limits_test.rs`:

```rust
//! `validate` and `read_file` must refuse oversized files without first
//! loading them into memory, and must distinguish missing / too-large /
//! not-UTF-8 in their reports.

use byonk::services::screen_repo_loader::{LocalScreenRepoSource, ReadOutcome, ScreenRepoSource};

const MAX: usize = 5 * 1024 * 1024;

fn repo_with(dir: &std::path::Path, file: &str, bytes: &[u8]) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("big")).unwrap();
    std::fs::write(
        repo.join("byonk-screens.yaml"),
        "name: local\ndescription: d\nauthor: a\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("big/meta.yaml"),
        "title: Big\ndescription: d\nbyonk: \"0.17\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("big/screen.svg"), "<svg/>\n").unwrap();
    std::fs::write(repo.join(file), bytes).unwrap();
    repo
}

#[test]
fn test_read_limited_reports_too_large_for_oversized_file() {
    let tmp = tempfile::tempdir().unwrap();
    let oversized = vec![b'x'; MAX + 1];
    let repo = repo_with(tmp.path(), "big/script.lua", &oversized);
    let src = LocalScreenRepoSource::load(&repo).unwrap();

    assert!(matches!(
        src.read_limited("big/script.lua", MAX),
        ReadOutcome::TooLarge
    ));
}

#[test]
fn test_read_limited_reports_missing_for_absent_file() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with(tmp.path(), "big/script.lua", b"return {}\n");
    let src = LocalScreenRepoSource::load(&repo).unwrap();

    assert!(matches!(
        src.read_limited("big/nope.lua", MAX),
        ReadOutcome::Missing
    ));
}

#[test]
fn test_read_limited_returns_bytes_under_the_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with(tmp.path(), "big/script.lua", b"return {}\n");
    let src = LocalScreenRepoSource::load(&repo).unwrap();

    match src.read_limited("big/script.lua", MAX) {
        ReadOutcome::Found(b) => assert_eq!(b, b"return {}\n"),
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn test_read_limited_respects_the_symlink_guard() {
    // The cap must not become a way around Task 1's escape check.
    #[cfg(unix)]
    {
        let tmp = tempfile::tempdir().unwrap();
        let repo = repo_with(tmp.path(), "big/script.lua", b"return {}\n");
        let secret = tmp.path().join("secret.txt");
        std::fs::write(&secret, "TOP SECRET").unwrap();
        std::os::unix::fs::symlink(&secret, repo.join("big/leak.txt")).unwrap();
        let src = LocalScreenRepoSource::load(&repo).unwrap();

        assert!(matches!(
            src.read_limited("big/leak.txt", MAX),
            ReadOutcome::Missing
        ));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test screen_store_limits_test -- --test-threads=4`
Expected: FAIL to compile — `ReadOutcome` and `read_limited` do not exist.

- [ ] **Step 3: Add `ReadOutcome` and the trait method**

In `src/services/screen_repo_loader.rs`:

```rust
/// The result of a size-capped read. Distinguishes "not there" from "there
/// but refused", so callers can report the difference instead of collapsing
/// both into `None`.
#[derive(Debug)]
pub enum ReadOutcome {
    Found(Vec<u8>),
    Missing,
    TooLarge,
}
```

Add to the `ScreenRepoSource` trait (next to `read_string`, around line 27):

```rust
    /// Read `rel`, refusing anything larger than `max_bytes`.
    ///
    /// The default implementation is correct for embedded sources, whose
    /// contents are already resident in the binary — there is no unbounded
    /// I/O to avoid. Disk-backed sources override it to `stat` first, so an
    /// oversized file is never read into memory at all.
    fn read_limited(&self, rel: &str, max_bytes: usize) -> ReadOutcome {
        match self.read(rel) {
            None => ReadOutcome::Missing,
            Some(b) if b.len() > max_bytes => ReadOutcome::TooLarge,
            Some(b) => ReadOutcome::Found(b),
        }
    }
```

- [ ] **Step 4: Override it in both disk sources**

Add to `impl ScreenRepoSource for GitScreenRepoSource` and `impl ScreenRepoSource for LocalScreenRepoSource`:

```rust
    fn read_limited(&self, rel: &str, max_bytes: usize) -> ReadOutcome {
        read_within_limited(&self.manifest_root, rel, max_bytes)
    }
```

And beside `read_within` in the same file:

```rust
/// `read_within`, but `stat`s the resolved target first so an oversized file
/// is refused without ever being read into memory.
pub(crate) fn read_within_limited(root: &Path, rel: &str, max_bytes: usize) -> ReadOutcome {
    if !is_safe_rel(rel) {
        return ReadOutcome::Missing;
    }
    let Some(canon_root) = std::fs::canonicalize(root).ok() else {
        return ReadOutcome::Missing;
    };
    let Some(canon_target) = std::fs::canonicalize(canon_root.join(rel)).ok() else {
        return ReadOutcome::Missing;
    };
    if !canon_target.starts_with(&canon_root) {
        tracing::warn!(
            root = %canon_root.display(),
            rel,
            "refused screen-repo read escaping the repo root"
        );
        return ReadOutcome::Missing;
    }
    match std::fs::metadata(&canon_target) {
        Ok(m) if m.len() > max_bytes as u64 => ReadOutcome::TooLarge,
        Ok(_) => match std::fs::read(&canon_target) {
            Ok(b) => ReadOutcome::Found(b),
            Err(_) => ReadOutcome::Missing,
        },
        Err(_) => ReadOutcome::Missing,
    }
}
```

- [ ] **Step 5: Run the source-level tests**

Run: `cargo test --test screen_store_limits_test -- --test-threads=4`
Expected: 4 tests PASS.

- [ ] **Step 6: Route `ScreenStore::read_file` through it**

In `src/services/screen_store.rs`, replace the read in `read_file`:

```rust
        let bytes = match src.read_limited(&full_rel, MAX_FILE_BYTES) {
            ReadOutcome::Found(b) => b,
            ReadOutcome::Missing => return Err(StoreError::NotFound),
            ReadOutcome::TooLarge => return Err(StoreError::TooLarge),
        };
```

(Delete the now-dead `if bytes.len() > MAX_FILE_BYTES` check below it.) Add `ReadOutcome` to the `screen_repo_loader` import.

- [ ] **Step 7: Route `validate` through it, and fix the mis-reported UTF-8 case**

Replace `validate`'s `check_file` closure body:

```rust
        let mut check_file = |name: &str, validator: &dyn Fn(&str) -> Result<(), String>| {
            let rel = format!("{screen_path}/{name}");
            let message = match source.read_limited(&rel, MAX_FILE_BYTES) {
                ReadOutcome::Missing => "file not found".to_string(),
                ReadOutcome::TooLarge => format!(
                    "file exceeds the {} MB limit and was not read",
                    MAX_FILE_BYTES / (1024 * 1024)
                ),
                ReadOutcome::Found(bytes) => match String::from_utf8(bytes) {
                    // Previously this surfaced as "file not found", because
                    // `read_string` collapsed a UTF-8 failure into `None`.
                    Err(_) => "file is not valid UTF-8".to_string(),
                    Ok(text) => match validator(&text) {
                        Ok(()) => return,
                        Err(message) => message,
                    },
                },
            };
            issues.push(Issue {
                severity: Severity::Error,
                location: name.to_string(),
                message,
            });
        };
```

- [ ] **Step 8: Add store-level coverage**

Append to `tests/screen_store_limits_test.rs`. A `validate` over a screen whose `script.lua` is oversized must report the cap — not "file not found", which is what the old `read_string` path produced.

First create the shared fixture, `tests/common/store.rs` — Tasks 3 and 4 consume it too, so it lives in `tests/common/` rather than being copied per test binary. (`tests/common/mod.rs` already carries `#![allow(dead_code)]`, so a binary that uses only part of the module compiles cleanly.)

```rust
//! Shared fixture: a `ScreenStore` over a temp writable `local` screen repo.
//!
//! Built through the real `AppState` path so the store and the content
//! pipeline share one `ScreenRepoManager` — the invariant
//! `tests/screen_store_wiring_test.rs` guards. Constructing a `ScreenStore`
//! directly with hand-matched `Arc`s would not exercise that path.

use std::path::Path;
use std::sync::Arc;

use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::create_app_state_with_config;
use byonk::services::screen_store::{ScreenStore, StarterKind};

/// A `ScreenStore` whose `local` handle is a writable repo at `dir/local`,
/// pre-scaffolded with one minimal screen per entry in `screens`.
///
/// Callers needing specific file contents write them directly under
/// `dir/local/<screen>/` afterwards — the disk sources stat and read on every
/// access, so no loader rebuild is needed for an in-place file change.
pub fn build_store(dir: &Path, screens: &[&str]) -> Arc<ScreenStore> {
    let config_path = dir.join("config.yaml");
    let repo_dir = dir.join("local");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("byonk-screens.yaml"),
        "name: local\ndescription: Test fixture.\nauthor: test\nlicense: MIT\n",
    )
    .unwrap();
    std::fs::write(
        &config_path,
        format!(
            "devices:\n  DEFAULT:\n    screen: byonk-builtin/default\n\
             screen_repos:\n  local:\n    path: {}\n",
            repo_dir.display()
        ),
    )
    .unwrap();

    let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path)));
    let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
    let state = create_app_state_with_config(asset_loader, config).expect("create app state");
    let store = state.screen_store.clone();
    for name in screens {
        store
            .create_screen("local", name, StarterKind::Minimal)
            .unwrap_or_else(|e| panic!("scaffold fixture screen {name}: {e:?}"));
    }
    store
}
```

Add `pub mod store;` to `tests/common/mod.rs`.

Then append to `tests/screen_store_limits_test.rs` — note it must gain `mod common;` at the top:

```rust
mod common;

use common::store::build_store;

#[test]
fn test_validate_reports_oversized_file_distinctly() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["big"]);
    // Overwrite the scaffolded script with an oversized one.
    std::fs::write(tmp.path().join("local/big/script.lua"), vec![b'x'; MAX + 1]).unwrap();

    let report = store.validate("local/big");

    assert!(!report.ok, "an oversized script must fail validation");
    let issue = report
        .issues
        .iter()
        .find(|i| i.location == "script.lua")
        .expect("script.lua must be flagged");
    assert!(
        issue.message.contains("exceeds"),
        "must name the size cap, got: {}",
        issue.message
    );
    assert!(
        !issue.message.contains("not found"),
        "an oversized file must not be reported as missing, got: {}",
        issue.message
    );
}

#[test]
fn test_validate_reports_non_utf8_distinctly() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["big"]);
    // Invalid UTF-8: lone continuation bytes.
    std::fs::write(tmp.path().join("local/big/script.lua"), [0x80u8, 0x80, 0x80]).unwrap();

    let report = store.validate("local/big");

    let issue = report
        .issues
        .iter()
        .find(|i| i.location == "script.lua")
        .expect("script.lua must be flagged");
    assert!(
        issue.message.contains("UTF-8"),
        "a non-UTF-8 file must say so rather than 'file not found', got: {}",
        issue.message
    );
}
```

- [ ] **Step 9: Run the full gate**

Run: `make check`
Expected: all green.

- [ ] **Step 10: Commit**

```bash
git add src/services/screen_repo_loader.rs src/services/screen_store.rs tests/screen_store_limits_test.rs
git commit -m "fix: bound screen-file reads by size before loading them"
```

---

## Task 3: Serialize `ScreenStore` mutations

Closes precondition 3. `create_screen` and `rename_screen` have check-then-act windows (`dir.exists()` → scaffold), and two concurrent `create_screen` calls can interleave such that one's failure cleanup (`remove_dir_all(&dir)`) deletes the other's work. `write_file`'s `if_match` read-then-write is the same shape. MCP and (later) the web UI are concurrent callers.

**Files:**
- Modify: `src/services/screen_store.rs`
- Test: `tests/screen_store_concurrency_test.rs` (new)

**Interfaces:**
- Produces: no public API change — `ScreenStore`'s methods keep their signatures. A private `mutation_lock: Mutex<()>` field is added.

- [ ] **Step 1: Write the failing test**

Create `tests/screen_store_concurrency_test.rs`:

```rust
//! Concurrent structural mutations must not corrupt each other. The specific
//! hazard: `create_screen` checks `dir.exists()`, then scaffolds, and on any
//! per-file failure removes the whole dir — so an interleaved pair can have
//! one call's cleanup delete the other call's finished screen.

mod common;

use byonk::services::screen_store::StarterKind;
use common::store::build_store;

#[test]
fn test_concurrent_creates_leave_every_successful_screen_intact() {
    let tmp = tempfile::tempdir().unwrap();
    // No pre-scaffolded screens — the concurrent creates below make them.
    let store = build_store(tmp.path(), &[]);

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let store = store.clone();
            std::thread::spawn(move || {
                store.create_screen("local", &format!("screen{i}"), StarterKind::Minimal)
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Every create targeted a distinct name, so every one must have succeeded.
    for (i, r) in results.iter().enumerate() {
        assert!(r.is_ok(), "create of screen{i} failed: {r:?}");
    }
    // And every one must still be on disk and readable afterwards.
    for i in 0..8 {
        let r = store.read_file(&format!("local/screen{i}"), "meta.yaml");
        assert!(r.is_ok(), "screen{i} missing after concurrent creates");
    }
}

#[test]
fn test_concurrent_creates_of_the_same_name_yield_exactly_one_winner() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &[]);

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || store.create_screen("local", "contended", StarterKind::Minimal))
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let winners = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(winners, 1, "expected exactly one create to win");
    // The winner's screen must be complete, not half-scaffolded by a loser's cleanup.
    for f in ["meta.yaml", "script.lua", "screen.svg"] {
        assert!(
            store.read_file("local/contended", f).is_ok(),
            "{f} missing after contended create"
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test screen_store_concurrency_test -- --test-threads=4`
Expected: `test_concurrent_creates_of_the_same_name_yield_exactly_one_winner` FAILS or flakes — losers' `remove_dir_all` deletes the winner's files.

- [ ] **Step 3: Add the lock field**

In `src/services/screen_store.rs`:

```rust
pub struct ScreenStore {
    manager: Arc<ScreenRepoManager>,
    pipeline: Arc<ContentPipeline>,
    /// Serializes every mutating operation.
    ///
    /// `create_screen`/`rename_screen`/`copy_screen` are check-then-act
    /// (`dir.exists()` → scaffold) and their failure path removes the whole
    /// destination dir — so without this, two interleaved creates can have
    /// one's cleanup delete the other's finished screen. `write_file`'s
    /// `if_match` read-then-write is the same shape. MCP tools and the web
    /// UI are concurrent callers, so the window is reachable.
    ///
    /// Held only across local filesystem work plus `rebuild_loader()`; no
    /// `.await` happens under it. Callers on an async runtime must invoke
    /// `ScreenStore` inside `spawn_blocking` (see `src/mcp/`).
    mutation_lock: Mutex<()>,
}
```

Initialize it in `new()`: `mutation_lock: Mutex::new(())`.

- [ ] **Step 4: Take the lock in every mutating method**

Add as the first statement of `write_file`, `create_screen`, `copy_screen`, `rename_screen`, and `delete_screen`:

```rust
        // Poisoning is not a correctness signal here — the guarded data is
        // the filesystem, not in-memory state a panicking thread could have
        // torn. Recover and proceed.
        let _guard = self.mutation_lock.lock().unwrap_or_else(|e| e.into_inner());
```

Place it *before* any path resolution so the whole check-then-act sequence is covered. `validate` and `render` are read-only and must NOT take it — they would otherwise serialize behind slow renders.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test screen_store_concurrency_test -- --test-threads=4`
Expected: both tests PASS. Run it 5 times to shake out flakes:
`for i in 1 2 3 4 5; do cargo test --test screen_store_concurrency_test -- --test-threads=4 || break; done`

- [ ] **Step 6: Run the full gate**

Run: `make check`

- [ ] **Step 7: Commit**

```bash
git add src/services/screen_store.rs tests/screen_store_concurrency_test.rs
git commit -m "fix: serialize ScreenStore mutations against concurrent callers"
```

---

## Task 4: `ScreenStore::list_screens` and `delete_file`

Spec 1 Component 3 lists both; Plan 1 did not implement them, and the MCP `list_screens` / `delete_screen_file` tools need them. `list_screens` is the one place `writable` is derived, so both the MCP layer and the future web UI get the same answer.

**Files:**
- Modify: `src/services/screen_store.rs`
- Test: `tests/screen_store_listing_test.rs` (new)

**Interfaces:**
- Consumes: `ScreenRepoManager::loader()`, `ScreenRepoLoader::list_all() -> Vec<ResolvedScreen>`, `ScreenRepoSource::writable_root()`, `ScreenRepoSource::screen_files(screen_path)`
- Produces:
  ```rust
  pub struct ScreenListEntry {
      pub screen_ref: String,   // "local/clock"
      pub handle: String,
      pub path: String,
      pub title: String,
      pub description: String,
      pub byonk: String,
      pub writable: bool,
      pub files: Vec<String>,   // screen-relative, e.g. ["meta.yaml", "script.lua", "screen.svg"]
  }
  impl ScreenStore {
      pub fn list_screens(&self) -> Vec<ScreenListEntry>;
      pub fn delete_file(&self, screen_ref: &str, file: &str) -> Result<(), StoreError>;
  }
  ```

- [ ] **Step 1: Write the failing test**

Create `tests/screen_store_listing_test.rs`:

```rust
//! `list_screens` reports writability structurally, and `delete_file`
//! refuses to strip a screen of the three files that define it.

mod common;

use byonk::services::screen_store::StoreError;
use common::store::build_store;

#[test]
fn test_list_screens_marks_builtin_read_only_and_local_writable() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["clock"]);

    let all = store.list_screens();

    let builtin = all
        .iter()
        .find(|e| e.screen_ref == "byonk-builtin/default")
        .expect("builtin default must be listed");
    assert!(!builtin.writable, "byonk-builtin must never be writable");

    let local = all
        .iter()
        .find(|e| e.screen_ref == "local/clock")
        .expect("local/clock must be listed");
    assert!(local.writable, "a local repo screen must be writable");
    assert!(local.files.iter().any(|f| f == "script.lua"));
}

#[test]
fn test_delete_file_removes_a_sibling_asset() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["clock"]);
    store
        .write_file("local/clock", "notes.txt", b"scratch", None)
        .unwrap();

    store.delete_file("local/clock", "notes.txt").unwrap();

    assert!(matches!(
        store.read_file("local/clock", "notes.txt"),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn test_delete_file_refuses_the_three_defining_files() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["clock"]);

    for f in ["meta.yaml", "script.lua", "screen.svg"] {
        let err = store.delete_file("local/clock", f);
        assert!(
            err.is_err(),
            "deleting {f} must be refused — it defines the screen"
        );
    }
    // …and the screen is still intact afterwards.
    assert!(store.read_file("local/clock", "meta.yaml").is_ok());
}

#[test]
fn test_delete_file_on_a_read_only_handle_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &["clock"]);

    match store.delete_file("byonk-builtin/default", "script.lua") {
        Err(StoreError::ReadOnly { copy_hint }) => {
            assert!(copy_hint.contains("copy_screen"));
        }
        other => panic!("expected ReadOnly, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test screen_store_listing_test -- --test-threads=4`
Expected: FAIL to compile — `list_screens` / `delete_file` do not exist.

- [ ] **Step 3: Implement `list_screens`**

```rust
    /// Every screen the loader currently resolves, annotated with whether its
    /// repo is writable. `writable` is read off the source's `writable_root`,
    /// never off the handle's name — a handle called `local` backed by a git
    /// cache is still read-only, and that must be what callers see.
    pub fn list_screens(&self) -> Vec<ScreenListEntry> {
        let loader = self.manager.loader();
        let mut out: Vec<ScreenListEntry> = loader
            .list_all()
            .into_iter()
            .map(|s| ScreenListEntry {
                screen_ref: format!("{}/{}", s.handle, s.path),
                writable: s.source.writable_root().is_some(),
                files: s.source.screen_files(&s.path),
                title: s.meta.title.clone(),
                description: s.meta.description.clone(),
                byonk: s.meta.byonk.clone(),
                handle: s.handle,
                path: s.path,
            })
            .collect();
        // Deterministic order — MCP clients diff these listings between calls.
        out.sort_by(|a, b| a.screen_ref.cmp(&b.screen_ref));
        out
    }
```

Check `ResolvedScreen`'s actual field names in `src/services/screen_repo_loader.rs:67` before writing this and adjust if they differ from `handle`/`path`/`meta`/`source`.

- [ ] **Step 4: Implement `delete_file`**

```rust
    /// Delete one file inside a screen. Refuses the three files that *define*
    /// a screen (`meta.yaml`, `script.lua`, `screen.svg`) — removing one of
    /// those leaves a directory the loader still enumerates but can no longer
    /// resolve, which is a worse state than either keeping the file or
    /// deleting the whole screen. `delete_screen` is the way to remove a
    /// screen.
    pub fn delete_file(&self, screen_ref: &str, file: &str) -> Result<(), StoreError> {
        let _guard = self.mutation_lock.lock().unwrap_or_else(|e| e.into_inner());
        let (handle, screen_path) = Self::split_ref(screen_ref)?;
        let rel = safe_rel(file)?;

        // Writability is checked FIRST, so a read-only handle reports
        // `ReadOnly` (with its copy hint) regardless of which file was
        // named — "you cannot edit this repo" is the more useful answer
        // than "that file is undeletable".
        let base = self.resolve_writable_root(handle, screen_path)?;

        const DEFINING: [&str; 3] = ["meta.yaml", "script.lua", "screen.svg"];
        if DEFINING.contains(&rel.to_string_lossy().as_ref()) {
            return Err(StoreError::Io(format!(
                "'{file}' defines the screen and cannot be deleted; use delete_screen to remove '{screen_ref}'"
            )));
        }

        let target = base.join(screen_path).join(&rel);
        Self::ensure_writable_parent(&base, &target)?;
        match std::fs::remove_file(&target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::NotFound)
            }
            Err(e) => return Err(StoreError::Io(e.to_string())),
        }
        self.manager.rebuild_loader();
        Ok(())
    }
```

Also add `ScreenListEntry` and export it alongside the other public types in `screen_store.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test screen_store_listing_test -- --test-threads=4`
Expected: 4 tests PASS.

- [ ] **Step 6: Run the full gate**

Run: `make check`

- [ ] **Step 7: Commit**

```bash
git add src/services/screen_store.rs tests/screen_store_listing_test.rs
git commit -m "feat: ScreenStore list_screens and delete_file"
```

---

## Task 5: Mount an authenticated `/mcp` endpoint

The transport, the auth gate, and the server identity — no tools yet. This task is where a wrong `rmcp` integration would show up, so it is isolated and independently reviewable.

**Files:**
- Modify: `Cargo.toml`, `src/lib.rs`, `src/server.rs:237-264`
- Create: `src/mcp/mod.rs`, `tests/common/mcp.rs`, `tests/mcp_transport_test.rs`
- Modify: `tests/common/mod.rs` (declare the new helper module)

**Interfaces:**
- Consumes: `AppState` (`src/server.rs:70`), `require_admin(&AppState, &HeaderMap) -> Result<(), ApiError>` (`src/api/admin/mod.rs:17`)
- Produces:
  - `pub struct ByonkMcp { state: AppState, tool_router: ToolRouter<Self> }`, `ByonkMcp::new(AppState)`
  - `pub fn mount(router: Router<AppState>, state: &AppState) -> Router<AppState>` in `src/mcp/mod.rs`
  - `McpTestClient` in `tests/common/mcp.rs` with `initialize()`, `call_tool(name, args) -> serde_json::Value`, `list_tools()`, `raw(method, params) -> TestResponse`

- [ ] **Step 1: Add the dependency**

In `Cargo.toml` under `[dependencies]`:

```toml
# MCP server (Model Context Protocol) — streamable HTTP transport mounted at /mcp.
# Pinned to the 2.x stable line; 3.0 is a prerelease.
rmcp = { version = "2.2", features = ["server", "macros", "transport-streamable-http-server"] }
```

Run `cargo build` once to confirm it resolves and compiles before writing any code.

- [ ] **Step 2: Write the failing test**

Create `tests/common/mcp.rs`:

```rust
//! Minimal MCP-over-HTTP client for tests: JSON-RPC POSTs against the
//! in-process router. byonk runs the transport in stateless + JSON-response
//! mode, so each POST is self-contained and the body is plain JSON — no
//! session header to thread, no SSE frames to unwrap.

use super::app::{TestApp, TestResponse};

pub struct McpTestClient<'a> {
    app: &'a TestApp,
    token: Option<&'a str>,
    next_id: std::cell::Cell<u64>,
}

impl<'a> McpTestClient<'a> {
    pub fn new(app: &'a TestApp, token: Option<&'a str>) -> Self {
        Self {
            app,
            token,
            next_id: std::cell::Cell::new(1),
        }
    }

    /// Raw JSON-RPC POST. rmcp requires BOTH mime types in `Accept` and
    /// `application/json` as `Content-Type`, in every mode.
    pub async fn raw(&self, method: &str, params: serde_json::Value) -> TestResponse {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut headers: Vec<(&str, &str)> =
            vec![("Accept", "application/json, text/event-stream")];
        let bearer;
        if let Some(t) = self.token {
            bearer = format!("Bearer {t}");
            headers.push(("Authorization", &bearer));
        }
        self.app
            .post_json("/mcp", &headers, &body.to_string())
            .await
    }

    /// Perform the MCP handshake and return the server's `InitializeResult`.
    pub async fn initialize(&self) -> serde_json::Value {
        let resp = self
            .raw(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "byonk-test", "version": "0" }
                }),
            )
            .await;
        assert_eq!(resp.status, axum::http::StatusCode::OK, "initialize failed: {}", resp.text());
        let v: serde_json::Value = resp.json();
        v["result"].clone()
    }

    pub async fn list_tools(&self) -> serde_json::Value {
        let resp = self.raw("tools/list", serde_json::json!({})).await;
        let v: serde_json::Value = resp.json();
        v["result"].clone()
    }

    /// Call a tool and return its `result`. Panics with the JSON-RPC error
    /// body when the call fails at the protocol level.
    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> serde_json::Value {
        let resp = self
            .raw(
                "tools/call",
                serde_json::json!({ "name": name, "arguments": args }),
            )
            .await;
        let v: serde_json::Value = resp.json();
        assert!(
            v.get("error").is_none(),
            "tool {name} returned a protocol error: {v}"
        );
        v["result"].clone()
    }
}
```

Add `pub mod mcp;` to `tests/common/mod.rs`.

Create `tests/mcp_transport_test.rs`:

```rust
mod common;

use axum::http::StatusCode;
use common::mcp::McpTestClient;
use common::TestApp;

#[tokio::test]
async fn test_mcp_is_invisible_when_no_admin_token_is_configured() {
    // Matches admin-route behaviour: no token ⇒ 404, not 401. The endpoint
    // must not advertise its own existence.
    let app = TestApp::new();
    let client = McpTestClient::new(&app, None);

    let resp = client.raw("initialize", serde_json::json!({})).await;

    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_mcp_rejects_a_missing_token() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, None);

    let resp = client.raw("initialize", serde_json::json!({})).await;

    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_mcp_rejects_a_wrong_token() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("wrong"));

    let resp = client.raw("initialize", serde_json::json!({})).await;

    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_mcp_handshake_reports_byonk_as_the_server() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));

    let result = client.initialize().await;

    // Must be byonk's own identity — `Implementation::from_build_env()`
    // would report rmcp's crate name and version instead.
    assert_eq!(result["serverInfo"]["name"], "byonk");
    assert_eq!(
        result["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION"),
        "server version must track byonk's Cargo version"
    );
    assert!(
        result["capabilities"]["tools"].is_object(),
        "tools capability must be advertised"
    );
}

#[tokio::test]
async fn test_mcp_requires_the_streaming_accept_header() {
    let app = TestApp::new_admin("secret");

    let resp = app
        .post_json(
            "/mcp",
            &[
                ("Accept", "application/json"),
                ("Authorization", "Bearer secret"),
            ],
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        )
        .await;

    assert_eq!(resp.status, StatusCode::NOT_ACCEPTABLE);
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --test mcp_transport_test -- --test-threads=4`
Expected: FAIL — every request 404s, since `/mcp` does not exist yet (and `test_mcp_handshake_reports_byonk_as_the_server` fails outright).

- [ ] **Step 4: Write `src/mcp/mod.rs`**

```rust
//! MCP (Model Context Protocol) server, mounted at `/mcp`.
//!
//! Lets an LLM author screens against a byonk running anywhere on the LAN —
//! including inside Home Assistant — with no filesystem access. Every tool
//! delegates to `ScreenStore` or the existing admin paths; this module adds
//! no screen logic of its own.
//!
//! Auth is the same Bearer admin token that gates `/api/admin/*`: no token
//! configured ⇒ 404 (invisible), wrong token ⇒ 401.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    Router,
};
use rmcp::{
    handler::server::tool::ToolRouter,
    model::{ServerCapabilities, ServerInfo},
    transport::streamable_http_server::{
        session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    tool_handler, ServerHandler,
};

use crate::api::admin::require_admin;
use crate::error::ApiError;
use crate::server::AppState;

/// The MCP server handler. One instance per request in stateless mode; it
/// only holds `AppState`, which is cheap to clone (all `Arc`s).
#[derive(Clone)]
pub struct ByonkMcp {
    /// `pub`, not `pub(crate)`: no tool reads it until Task 6, and a
    /// crate-private never-read field trips `dead_code` under the
    /// project's `clippy -- -D warnings` gate. A handler struct's state
    /// is legitimately part of this module's public surface.
    pub state: AppState,
    tool_router: ToolRouter<Self>,
}

impl ByonkMcp {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            // Combined from the per-module routers; each is added as its
            // task lands. See `#[tool_router(router = …, vis = "pub")]`.
            tool_router: ToolRouter::new(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ByonkMcp {
    fn get_info(&self) -> ServerInfo {
        // NOT `Implementation::from_build_env()` — that macro expands inside
        // rmcp, so it would report rmcp's crate name and version.
        let mut info = ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        );
        info.server_info = rmcp::model::Implementation {
            name: "byonk".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            title: Some("Byonk screen authoring".to_string()),
            description: Some(
                "Author, validate and render TRMNL e-ink screens on this byonk server."
                    .to_string(),
            ),
            icons: None,
            website_url: None,
        };
        info.with_instructions(
            "Screens are directories addressed as `handle/path` (e.g. `local/clock`), \
             each containing meta.yaml, script.lua and screen.svg. Only repos reported \
             as writable by list_screens can be edited; fork a read-only screen with \
             copy_screen first. After editing, call render_screen to see the result and \
             read its log/data/error fields. Read the byonk://reference/* resources for \
             the Lua and SVG contracts.",
        )
    }
}

/// Mount `/mcp` on the main router, behind the admin-token gate.
pub fn mount(router: Router<AppState>, state: &AppState) -> Router<AppState> {
    let owned = state.clone();
    let service = StreamableHttpService::new(
        move || Ok(ByonkMcp::new(owned.clone())),
        Arc::new(NeverSessionManager::default()),
        StreamableHttpServerConfig {
            // Stateless: byonk's tools are pure request/response, so there is
            // no session state worth keeping. `json_response` then returns
            // plain application/json instead of SSE framing.
            stateful_mode: false,
            json_response: true,
            ..Default::default()
        }
        // rmcp defaults to loopback-only Host validation (DNS-rebinding
        // defence). byonk's whole purpose here is being driven over the LAN
        // at an arbitrary hostname, and the Bearer token already defeats
        // rebinding — a rebound browser request carries no token and 401s.
        .disable_allowed_hosts(),
    );

    let mcp = Router::new()
        .route_service("/", service)
        .layer(middleware::from_fn_with_state(state.clone(), gate));

    router.nest("/mcp", mcp)
}

/// Same semantics as `/api/admin/*`: 404 when admin is disabled, 401 on a
/// missing/wrong token.
async fn gate(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    require_admin(&state, request.headers())?;
    Ok(next.run(request).await)
}
```

- [ ] **Step 5: Wire it up**

In `src/lib.rs`, add `pub mod mcp;` next to the other module declarations.

In `src/server.rs`, `build_router`, insert the mount before `.with_state(state)`:

```rust
pub fn build_router(state: AppState) -> Router {
    let router = Router::new()
        // ... all existing routes unchanged ...
        .nest("/api/admin", crate::api::admin::admin_router());

    // MCP endpoint — gated by the same admin token as /api/admin/*.
    let router = crate::mcp::mount(router, &state);

    router
        .with_state(state)
        .layer(TraceLayer::new_for_http().make_span_with(RequestIdSpan))
        .layer(SetResponseHeaderLayer::overriding(
            CONNECTION,
            axum::http::HeaderValue::from_static("close"),
        ))
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --test mcp_transport_test -- --test-threads=4`
Expected: 5 tests PASS.

If `test_mcp_is_invisible_when_no_admin_token_is_configured` returns 401 instead of 404, `require_admin` is being reached but `ApiError::NotFound` is not mapping to 404 through the middleware — check `ApiError`'s `IntoResponse`.

- [ ] **Step 7: Run the full gate**

Run: `make check`

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/server.rs src/mcp/mod.rs tests/common/mcp.rs tests/common/mod.rs tests/mcp_transport_test.rs
git commit -m "feat: mount an admin-gated MCP endpoint at /mcp"
```

---

## Task 6: Read-context tools

Five tools that let an agent orient itself before touching anything: `list_screens`, `read_screen_file`, `list_screen_repos`, `list_devices`, `get_config`.

**Files:**
- Create: `src/mcp/tools_read.rs`
- Modify: `src/mcp/mod.rs` (declare the module, add its router)
- Create: `tests/mcp_tools_test.rs`

**Interfaces:**
- Consumes: `ScreenStore::list_screens() -> Vec<ScreenListEntry>` and `read_file` (Task 4 / Plan 1); `AppState.screen_repo_manager`; `AppState.registry`; `AppState.config`
- Produces: `pub fn tools_read_router() -> ToolRouter<ByonkMcp>` (module-level, from `#[tool_router(router = tools_read_router, vis = "pub")]`)

- [ ] **Step 1: Write the failing test**

Create `tests/mcp_tools_test.rs`:

```rust
mod common;

use common::mcp::McpTestClient;
use common::TestApp;

/// Every tool call returns `content` (human-readable) and, for tools that
/// declare an output schema, `structuredContent`. Assertions read the
/// structured form.
fn structured(result: &serde_json::Value) -> &serde_json::Value {
    result
        .get("structuredContent")
        .unwrap_or_else(|| panic!("no structuredContent in {result}"))
}

#[tokio::test]
async fn test_list_screens_reports_builtin_as_read_only() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client.call_tool("list_screens", serde_json::json!({})).await;

    let screens = structured(&result)["screens"].as_array().unwrap();
    let builtin = screens
        .iter()
        .find(|s| s["screen_ref"] == "byonk-builtin/default")
        .expect("builtin default must be listed");
    assert_eq!(builtin["writable"], false);
}

#[tokio::test]
async fn test_read_screen_file_returns_content_and_etag() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "read_screen_file",
            serde_json::json!({ "screen_ref": "byonk-builtin/default", "file": "meta.yaml" }),
        )
        .await;

    let s = structured(&result);
    assert!(s["content"].as_str().unwrap().contains("title:"));
    assert_eq!(s["etag"].as_str().unwrap().len(), 64, "blake3 hex etag");
    assert_eq!(s["binary"], false);
}

#[tokio::test]
async fn test_read_screen_file_on_a_missing_screen_is_a_tool_error() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "read_screen_file",
            serde_json::json!({ "screen_ref": "local/nope", "file": "meta.yaml" }),
        )
        .await;

    // A tool-level failure is `isError: true` on the result, not a
    // JSON-RPC error — the agent must be able to read and recover from it.
    assert_eq!(result["isError"], true);
}

#[tokio::test]
async fn test_list_screen_repos_reports_kind() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool("list_screen_repos", serde_json::json!({}))
        .await;

    let repos = structured(&result)["repos"].as_array().unwrap();
    let builtin = repos
        .iter()
        .find(|r| r["handle"] == "byonk-builtin")
        .expect("byonk-builtin must be listed");
    assert_eq!(builtin["kind"], "embedded");
    assert_eq!(builtin["writable"], false);
}

#[tokio::test]
async fn test_get_config_redacts_secrets() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client.call_tool("get_config", serde_json::json!({})).await;

    let text = serde_json::to_string(structured(&result)).unwrap();
    assert!(
        !text.contains("secret"),
        "admin token must never appear in get_config output: {text}"
    );
}

#[tokio::test]
async fn test_list_devices_includes_a_registered_device() {
    let app = TestApp::new_admin("secret");
    app.register_device("11:22:33:44:55:66").await;
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client.call_tool("list_devices", serde_json::json!({})).await;

    let devices = structured(&result)["devices"].as_array().unwrap();
    assert!(devices.iter().any(|d| d["mac"] == "11:22:33:44:55:66"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test mcp_tools_test -- --test-threads=4`
Expected: FAIL — the tools do not exist, so `call_tool` gets a JSON-RPC "tool not found" error.

- [ ] **Step 3: Add a structural `kind()` to `ScreenRepoSource`**

`list_screen_repos` reports whether a repo is embedded, git-fetched or local. That must come from the source's type, never from its handle's name (a handle called `local` backed by a git cache is still read-only). Add to the trait in `src/services/screen_repo_loader.rs`:

```rust
/// What backs a screen repo. Structural, not nominal — callers must never
/// infer this from a handle's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenRepoKind {
    /// Compiled into the binary; unshadowable and uneditable.
    Embedded,
    /// A git-fetched cache; read-only because a refresh would clobber edits.
    Git,
    /// A writable directory on disk.
    Local,
}

impl ScreenRepoKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::Git => "git",
            Self::Local => "local",
        }
    }
}
```

On the trait, next to `writable_root`:

```rust
    /// What backs this source. Defaults to `Embedded` so a future embedded
    /// source needs no override; the two disk sources override it.
    fn kind(&self) -> ScreenRepoKind {
        ScreenRepoKind::Embedded
    }
```

Override in `impl ScreenRepoSource for GitScreenRepoSource` with `ScreenRepoKind::Git` and in `LocalScreenRepoSource` with `ScreenRepoKind::Local`.

- [ ] **Step 4: Write `src/mcp/tools_read.rs`**

```rust
//! Read-context MCP tools: what screens, repos and devices exist, and what a
//! screen's files contain.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
    ErrorData,
};
use serde::{Deserialize, Serialize};

use super::{blocking, ok_json, store_failure, ByonkMcp};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScreenFileArgs {
    /// Screen reference, `handle/path` — e.g. `local/clock`.
    pub screen_ref: String,
    /// File inside the screen directory — e.g. `script.lua`.
    pub file: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ScreenEntry {
    pub screen_ref: String,
    pub handle: String,
    pub title: String,
    pub description: String,
    /// Engine compatibility requirement from `meta.yaml` (a caret range).
    pub byonk: String,
    /// Whether this screen's repo can be written to. Fork a read-only screen
    /// with `copy_screen` before editing it.
    pub writable: bool,
    pub files: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListScreensOutput {
    pub screens: Vec<ScreenEntry>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FileOutput {
    /// UTF-8 contents. Empty when `binary` is true.
    pub content: String,
    /// Pass back as `if_match` on `write_screen_file` for safe edits.
    pub etag: String,
    pub binary: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RepoEntry {
    pub handle: String,
    /// `embedded` | `git` | `local`
    pub kind: String,
    pub name: String,
    pub screen_count: usize,
    pub writable: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListReposOutput {
    pub repos: Vec<RepoEntry>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeviceEntry {
    pub mac: String,
    pub name: Option<String>,
    pub model: Option<String>,
    pub screen: Option<String>,
    pub last_seen: Option<String>,
    pub battery_voltage: Option<f32>,
    pub rssi: Option<i32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListDevicesOutput {
    pub devices: Vec<DeviceEntry>,
}

#[tool_router(router = tools_read_router, vis = "pub")]
impl ByonkMcp {
    /// List every screen this server can resolve, with its repo, title and
    /// whether it is writable.
    #[tool(description = "List every screen on this byonk server, with repo handle, title, \
                          compat requirement, file list and whether it can be edited.")]
    pub async fn list_screens(&self) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let entries = blocking(move || store.list_screens()).await?;
        ok_json(ListScreensOutput {
            screens: entries
                .into_iter()
                .map(|e| ScreenEntry {
                    screen_ref: e.screen_ref,
                    handle: e.handle,
                    title: e.title,
                    description: e.description,
                    byonk: e.byonk,
                    writable: e.writable,
                    files: e.files,
                })
                .collect(),
        })
    }

    /// Read one file inside a screen.
    #[tool(description = "Read one file inside a screen (meta.yaml, script.lua, screen.svg or \
                          any sibling asset). Returns its contents and an etag to pass back \
                          as if_match when writing.")]
    pub async fn read_screen_file(
        &self,
        Parameters(args): Parameters<ScreenFileArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let contents = match blocking(move || store.read_file(&args.screen_ref, &args.file)).await?
        {
            Ok(c) => c,
            Err(e) => return Ok(store_failure(e)),
        };
        ok_json(FileOutput {
            content: if contents.binary {
                String::new()
            } else {
                String::from_utf8_lossy(&contents.bytes).into_owned()
            },
            etag: contents.etag,
            binary: contents.binary,
        })
    }

    /// List the configured screen repositories.
    #[tool(description = "List the screen repositories registered on this server: handle, kind \
                          (embedded/git/local), screen count and writability.")]
    pub async fn list_screen_repos(&self) -> Result<CallToolResult, ErrorData> {
        // Build from the live loader so `kind` and `writable` agree with what
        // the write path will actually enforce.
        let manager = self.state.screen_repo_manager.clone();
        let repos = blocking(move || {
            let loader = manager.loader();
            let mut by_handle: std::collections::BTreeMap<String, RepoEntry> = Default::default();
            for s in loader.list_all() {
                let entry = by_handle.entry(s.handle.clone()).or_insert_with(|| RepoEntry {
                    handle: s.handle.clone(),
                    kind: s.source.kind().as_str().to_string(),
                    name: s.source.manifest().name.clone(),
                    screen_count: 0,
                    writable: s.source.writable_root().is_some(),
                });
                entry.screen_count += 1;
            }
            by_handle.into_values().collect::<Vec<_>>()
        })
        .await?;
        ok_json(ListReposOutput { repos })
    }

    /// List known devices.
    #[tool(description = "List TRMNL devices this server knows about: MAC, model, assigned \
                          screen, last-seen time, battery and signal strength.")]
    pub async fn list_devices(&self) -> Result<CallToolResult, ErrorData> {
        // Mirrors the merge `src/api/admin/read.rs::list_devices` performs
        // (registry telemetry + config mapping). Check that handler for the
        // exact field names and copy them — do not invent new ones.
        let seen = self
            .state
            .registry
            .list_all()
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let config = self.state.config.load();
        let devices: Vec<DeviceEntry> = seen
            .iter()
            .map(|d| {
                let mac = d.device_id.to_string();
                let cfg = config.devices.get(&mac);
                DeviceEntry {
                    screen: cfg.map(|c| c.screen.clone()),
                    name: cfg.and_then(|c| c.name.clone()),
                    model: d.model.clone(),
                    last_seen: d.last_seen.map(|t| t.to_rfc3339()),
                    battery_voltage: d.battery_voltage,
                    rssi: d.rssi,
                    mac,
                }
            })
            .collect();
        ok_json(ListDevicesOutput { devices })
    }

    /// Non-secret global configuration.
    #[tool(description = "Read this server's non-secret global configuration. Tokens and other \
                          credentials are never included.")]
    pub async fn get_config(&self) -> Result<CallToolResult, ErrorData> {
        // Delegate to the same redaction the admin API applies — do not
        // hand-roll a second one that could drift and start leaking.
        ok_json(crate::api::admin::read::redacted_config(&self.state))
    }
}
```

Imports for this file: `use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router, ErrorData};` plus `use super::{blocking, ok_json, store_failure, ByonkMcp};`. `Json` is not used here — every tool builds its result through `ok_json`.

The exact field names on the registry's device record and on `config.devices` values must be checked against `src/api/admin/read.rs:117-190` and adjusted — that handler already does this merge and is the reference.

`redacted_config` may not exist yet as a standalone function: `get_config` in `src/api/admin/read.rs:51` is an axum handler. Extract its redaction body into `pub fn redacted_config(state: &AppState) -> serde_json::Value` and have the handler call it, so both surfaces share one redaction. This is required — do not duplicate the logic.

- [ ] **Step 5: Add the shared helpers to `src/mcp/mod.rs`**

```rust
/// Run a synchronous `ScreenStore` operation off the async runtime.
///
/// `ScreenStore` is entirely blocking — `render` runs Lua (with blocking
/// HTTP), resvg and dithering — so calling it directly from a handler would
/// stall a tokio worker. Same pattern as `src/api/dev.rs:496`.
pub(crate) async fn blocking<T, F>(f: F) -> Result<T, ErrorData>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ErrorData::internal_error(format!("task panicked: {e}"), None))
}

/// Build a successful tool result carrying both a JSON `structured_content`
/// payload and a pretty-printed text block (clients that ignore structured
/// output still show the model something useful).
pub(crate) fn ok_json<T: serde::Serialize>(value: T) -> Result<CallToolResult, ErrorData> {
    let json = serde_json::to_value(&value)
        .map_err(|e| ErrorData::internal_error(format!("serialize result: {e}"), None))?;
    let text = serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string());
    Ok(CallToolResult {
        content: vec![ContentBlock::text(text)],
        structured_content: Some(json),
        is_error: Some(false),
        meta: None,
    })
}

/// Turn a `StoreError` into a **tool-level** error result.
///
/// Deliberately not `Err(ErrorData)`: MCP clients render protocol errors
/// opaquely, so the model would never see the message. These messages are
/// the agent's instructions for recovering — above all `ReadOnly`'s hint
/// naming `copy_screen` — so they must arrive as visible content.
pub(crate) fn store_failure(e: crate::services::screen_store::StoreError) -> CallToolResult {
    use crate::services::screen_store::StoreError as E;
    let message = match e {
        E::ReadOnly { copy_hint } => copy_hint,
        E::NotFound => "no such screen or file".to_string(),
        E::Conflict => "conflict: the file changed on disk since you read it, or the target \
                        already exists — re-read it and retry"
            .to_string(),
        E::Traversal => "path escapes the screen directory".to_string(),
        E::TooLarge => "file exceeds the 5 MB limit".to_string(),
        E::Io(m) => m,
    };
    CallToolResult::error(vec![ContentBlock::text(message)])
}
```

Imports for the above: `use rmcp::model::{CallToolResult, ContentBlock}; use rmcp::ErrorData;`.

Because `store_failure` returns a value rather than an error, the call shape in every fallible tool is:

```rust
        let outcome = blocking(move || store.some_op(..)).await?;   // `?` = protocol fault only
        let value = match outcome {
            Ok(v) => v,
            Err(e) => return Ok(store_failure(e)),
        };
        ok_json(SomeOutput { .. })
```

Declare the module and fold in its router:

```rust
pub mod tools_read;

// in ByonkMcp::new
tool_router: tools_read::tools_read_router(),
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --test mcp_tools_test -- --test-threads=4`
Expected: 6 tests PASS.

- [ ] **Step 7: Run the full gate**

Run: `make check`

- [ ] **Step 8: Commit**

```bash
git add src/services/screen_repo_loader.rs src/mcp/ src/api/admin/read.rs tests/mcp_tools_test.rs
git commit -m "feat: MCP read-context tools (screens, repos, devices, config)"
```

---

## Task 7: Edit tools

Six mutating tools. Every one resolves through `ScreenStore`, so writability is enforced structurally and a read-only target returns the copy hint rather than a bare refusal.

**Files:**
- Create: `src/mcp/tools_edit.rs`
- Modify: `src/mcp/mod.rs`
- Modify: `tests/mcp_tools_test.rs`

**Interfaces:**
- Consumes: `ScreenStore::{write_file, create_screen, copy_screen, rename_screen, delete_screen, delete_file}`; `StarterKind`
- Produces: `pub fn tools_edit_router() -> ToolRouter<ByonkMcp>`

- [ ] **Step 1: Write the failing test**

Append to `tests/mcp_tools_test.rs`. These need a writable `local` repo, so use the fixture that gives `TestApp` a real `SCREENS_DIR` — `TestApp::new_admin_with_screens(token, dir)` (`tests/common/app.rs:247`).

```rust
#[tokio::test]
async fn test_copy_then_edit_a_builtin_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    // Fork the read-only builtin into the writable local repo.
    let copied = client
        .call_tool(
            "copy_screen",
            serde_json::json!({
                "from_ref": "byonk-builtin/default",
                "to_handle": "local",
                "to_name": "mine"
            }),
        )
        .await;
    assert_eq!(structured(&copied)["screen_ref"], "local/mine");

    // Read it, then write it back with its etag.
    let read = client
        .call_tool(
            "read_screen_file",
            serde_json::json!({ "screen_ref": "local/mine", "file": "script.lua" }),
        )
        .await;
    let etag = structured(&read)["etag"].as_str().unwrap().to_string();

    let written = client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/mine",
                "file": "script.lua",
                "content": "return { hello = \"world\" }\n",
                "if_match": etag
            }),
        )
        .await;
    assert_ne!(structured(&written)["etag"], etag, "etag must change");
}

#[tokio::test]
async fn test_write_to_a_read_only_handle_names_copy_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "byonk-builtin/default",
                "file": "script.lua",
                "content": "return {}\n"
            }),
        )
        .await;

    assert_eq!(result["isError"], true);
    let text = serde_json::to_string(&result).unwrap();
    assert!(
        text.contains("copy_screen"),
        "the refusal must tell the agent how to proceed: {text}"
    );
}

#[tokio::test]
async fn test_stale_etag_is_a_conflict() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "conflicted" }),
        )
        .await;

    let result = client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/conflicted",
                "file": "script.lua",
                "content": "return {}\n",
                "if_match": "0".repeat(64)
            }),
        )
        .await;

    assert_eq!(result["isError"], true);
    assert!(serde_json::to_string(&result).unwrap().contains("conflict"));
}

#[tokio::test]
async fn test_create_rename_delete_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "tmp1" }),
        )
        .await;
    client
        .call_tool(
            "rename_screen",
            serde_json::json!({ "screen_ref": "local/tmp1", "new_name": "tmp2" }),
        )
        .await;

    let listed = client.call_tool("list_screens", serde_json::json!({})).await;
    let refs: Vec<String> = structured(&listed)["screens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["screen_ref"].as_str().unwrap().to_string())
        .collect();
    assert!(refs.contains(&"local/tmp2".to_string()));
    assert!(!refs.contains(&"local/tmp1".to_string()));

    client
        .call_tool(
            "delete_screen",
            serde_json::json!({ "screen_ref": "local/tmp2" }),
        )
        .await;

    let after = client.call_tool("list_screens", serde_json::json!({})).await;
    let refs: Vec<String> = structured(&after)["screens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["screen_ref"].as_str().unwrap().to_string())
        .collect();
    assert!(!refs.contains(&"local/tmp2".to_string()));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test mcp_tools_test -- --test-threads=4`
Expected: the four new tests FAIL — tools not found.

- [ ] **Step 3: Write `src/mcp/tools_edit.rs`**

```rust
//! Mutating MCP tools. Every one goes through `ScreenStore`, so a read-only
//! target is refused structurally and the refusal names `copy_screen`.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
    ErrorData,
};
use serde::{Deserialize, Serialize};

use super::{blocking, ok_json, store_failure, ByonkMcp};
use crate::services::screen_store::StarterKind;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteFileArgs {
    /// Screen reference, `handle/path`. Must be in a writable repo.
    pub screen_ref: String,
    /// File inside the screen directory.
    pub file: String,
    /// New UTF-8 contents, written atomically.
    pub content: String,
    /// The etag you last read. Omit to force the write; supply it to be told
    /// (`conflict`) when the file changed underneath you.
    #[serde(default)]
    pub if_match: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct EtagOutput {
    pub etag: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ScreenRefOutput {
    pub screen_ref: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateScreenArgs {
    /// Writable repo handle — usually `local`.
    pub handle: String,
    /// New screen name, e.g. `clock` or `home/clock`.
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CopyScreenArgs {
    /// Source screen, which may be read-only (a builtin or an example).
    pub from_ref: String,
    pub to_handle: String,
    pub to_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameScreenArgs {
    pub screen_ref: String,
    pub new_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScreenRefArgs {
    pub screen_ref: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteFileArgs {
    pub screen_ref: String,
    /// Sibling asset to delete. meta.yaml / script.lua / screen.svg define
    /// the screen and cannot be deleted — use `delete_screen` instead.
    pub file: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct OkOutput {
    pub ok: bool,
}

#[tool_router(router = tools_edit_router, vis = "pub")]
impl ByonkMcp {
    #[tool(description = "Write one file inside a screen, atomically. Pass if_match with the \
                          etag you read to detect concurrent edits. Only writable repos \
                          accept writes; fork a read-only screen with copy_screen first.")]
    pub async fn write_screen_file(
        &self,
        Parameters(a): Parameters<WriteFileArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let outcome = blocking(move || {
            store.write_file(
                &a.screen_ref,
                &a.file,
                a.content.as_bytes(),
                a.if_match.as_deref(),
            )
        })
        .await?;
        match outcome {
            Ok(etag) => ok_json(EtagOutput { etag }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(description = "Scaffold a new screen from the minimal starter (meta.yaml, \
                          script.lua, screen.svg extending the byonk-base-v1 layout).")]
    pub async fn create_screen(
        &self,
        Parameters(a): Parameters<CreateScreenArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let outcome =
            blocking(move || store.create_screen(&a.handle, &a.name, StarterKind::Minimal)).await?;
        match outcome {
            Ok(screen_ref) => ok_json(ScreenRefOutput { screen_ref }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(description = "Fork any screen — including read-only builtins and examples — into a \
                          writable repo, copying every file in its directory. This is how you \
                          customize a screen you cannot edit in place.")]
    pub async fn copy_screen(
        &self,
        Parameters(a): Parameters<CopyScreenArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let outcome =
            blocking(move || store.copy_screen(&a.from_ref, &a.to_handle, &a.to_name)).await?;
        match outcome {
            Ok(screen_ref) => ok_json(ScreenRefOutput { screen_ref }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(description = "Rename a screen within its repo. Devices still pointing at the old \
                          reference will stop resolving — reassign them with assign_screen.")]
    pub async fn rename_screen(
        &self,
        Parameters(a): Parameters<RenameScreenArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let outcome = blocking(move || store.rename_screen(&a.screen_ref, &a.new_name)).await?;
        match outcome {
            Ok(screen_ref) => ok_json(ScreenRefOutput { screen_ref }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(description = "Delete a screen and every file in its directory. Devices pointing at \
                          it will fall back to the builtin default.")]
    pub async fn delete_screen(
        &self,
        Parameters(a): Parameters<ScreenRefArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        match blocking(move || store.delete_screen(&a.screen_ref)).await? {
            Ok(()) => ok_json(OkOutput { ok: true }),
            Err(e) => Ok(store_failure(e)),
        }
    }

    #[tool(description = "Delete one sibling asset from a screen directory. meta.yaml, \
                          script.lua and screen.svg cannot be deleted this way.")]
    pub async fn delete_screen_file(
        &self,
        Parameters(a): Parameters<DeleteFileArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        match blocking(move || store.delete_file(&a.screen_ref, &a.file)).await? {
            Ok(()) => ok_json(OkOutput { ok: true }),
            Err(e) => Ok(store_failure(e)),
        }
    }
}
```

- [ ] **Step 4: Fold in the router**

In `src/mcp/mod.rs`: `pub mod tools_edit;` and

```rust
            tool_router: tools_read::tools_read_router() + tools_edit::tools_edit_router(),
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test mcp_tools_test -- --test-threads=4`
Expected: all tests PASS.

- [ ] **Step 6: Run the full gate**

Run: `make check`

- [ ] **Step 7: Commit**

```bash
git add src/mcp/ tests/mcp_tools_test.rs
git commit -m "feat: MCP edit tools over ScreenStore"
```

---

## Task 8: `render_screen` and `validate_screen`

The tools that close the authoring loop: an agent edits, renders, reads the error line, and fixes it — without a round trip through a human.

**Files:**
- Create: `src/mcp/tools_render.rs`
- Modify: `src/mcp/mod.rs`
- Modify: `tests/mcp_tools_test.rs`

**Interfaces:**
- Consumes: `ScreenStore::render(&str, RenderOpts) -> RenderResult`, `ScreenStore::validate(&str) -> ValidationReport`
- Produces: `pub fn tools_render_router() -> ToolRouter<ByonkMcp>`

- [ ] **Step 1: Write the failing test**

Append to `tests/mcp_tools_test.rs`:

```rust
#[tokio::test]
async fn test_render_screen_returns_an_image_block() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "render_screen",
            serde_json::json!({ "screen_ref": "byonk-builtin/default" }),
        )
        .await;

    let content = result["content"].as_array().unwrap();
    let image = content
        .iter()
        .find(|c| c["type"] == "image")
        .expect("render must return an image content block");
    assert_eq!(image["mimeType"], "image/png");
    // Base64 PNG magic: iVBORw0KGgo
    assert!(image["data"].as_str().unwrap().starts_with("iVBORw0KGgo"));
}

#[tokio::test]
async fn test_render_of_a_broken_script_reports_the_lua_line() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "broken" }),
        )
        .await;
    client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/broken",
                "file": "script.lua",
                // Line 2 indexes a nil value at runtime.
                "content": "local t = nil\nreturn { x = t.y }\n"
            }),
        )
        .await;

    let result = client
        .call_tool(
            "render_screen",
            serde_json::json!({ "screen_ref": "local/broken" }),
        )
        .await;

    let s = structured(&result);
    let error = &s["error"];
    assert!(!error.is_null(), "a broken script must report an error");
    assert_eq!(error["line"], 2, "the Lua error line must be reported");
}

#[tokio::test]
async fn test_render_captures_script_log_output() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "chatty" }),
        )
        .await;
    client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/chatty",
                "file": "script.lua",
                "content": "log_info(\"hello from lua\")\nreturn {}\n"
            }),
        )
        .await;

    let result = client
        .call_tool(
            "render_screen",
            serde_json::json!({ "screen_ref": "local/chatty" }),
        )
        .await;

    let log = serde_json::to_string(&structured(&result)["log"]).unwrap();
    assert!(log.contains("hello from lua"), "log not captured: {log}");
}

#[tokio::test]
async fn test_validate_screen_flags_a_lua_syntax_error() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    client
        .call_tool(
            "create_screen",
            serde_json::json!({ "handle": "local", "name": "syntax" }),
        )
        .await;
    client
        .call_tool(
            "write_screen_file",
            serde_json::json!({
                "screen_ref": "local/syntax",
                "file": "script.lua",
                "content": "return {\n"
            }),
        )
        .await;

    let result = client
        .call_tool(
            "validate_screen",
            serde_json::json!({ "screen_ref": "local/syntax" }),
        )
        .await;

    let s = structured(&result);
    assert_eq!(s["ok"], false);
    let issues = s["issues"].as_array().unwrap();
    assert!(issues.iter().any(|i| i["location"] == "script.lua"));
}

#[tokio::test]
async fn test_validate_of_a_healthy_builtin_passes() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "validate_screen",
            serde_json::json!({ "screen_ref": "byonk-builtin/default" }),
        )
        .await;

    assert_eq!(structured(&result)["ok"], true);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test mcp_tools_test -- --test-threads=4`
Expected: the five new tests FAIL — tools not found.

- [ ] **Step 3: Write `src/mcp/tools_render.rs`**

```rust
//! Render and validate. `render_screen` returns the PNG as an MCP image
//! block *and* the diagnostics an author needs — captured log output, the
//! data table the script returned, and a line-numbered error — so a failing
//! edit is debuggable in one round trip.

use base64::Engine as _;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_router, ErrorData,
};
use serde::{Deserialize, Serialize};

use super::{blocking, ByonkMcp};
use crate::services::screen_store::RenderOpts;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenderArgs {
    /// Screen reference, `handle/path`.
    pub screen_ref: String,
    /// Device model selecting default size and palette. Defaults to `og`
    /// (800x480, 4-grey).
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// Panel profile name from the `panels:` config section.
    #[serde(default)]
    pub panel: Option<String>,
    /// Dither algorithm, e.g. `floyd-steinberg`, `atkinson`.
    #[serde(default)]
    pub dither: Option<String>,
    /// Also return the pre-dither, full-colour PNG for comparison.
    #[serde(default)]
    pub include_raw: bool,
    /// Unix timestamp to render at, for testing time-dependent screens.
    #[serde(default)]
    pub timestamp: Option<i64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RenderDiagnostics {
    /// Captured `log_info`/`log_warn`/`log_error` output from the script.
    pub log: Vec<String>,
    /// The table the Lua script returned.
    pub data: serde_json::Value,
    pub refresh_rate: u32,
    /// Present when the render failed. `line` points into script.lua when
    /// the failure was a Lua error.
    pub error: Option<RenderErrorOut>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RenderErrorOut {
    pub line: Option<u32>,
    pub message: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateArgs {
    pub screen_ref: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ValidateOutput {
    pub ok: bool,
    pub issues: Vec<IssueOut>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct IssueOut {
    /// `error` or `warning`.
    pub severity: String,
    /// The screen-relative file the issue is in.
    pub location: String,
    pub message: String,
}

#[tool_router(router = tools_render_router, vis = "pub")]
impl ByonkMcp {
    #[tool(description = "Render a screen and return the dithered PNG plus diagnostics: the \
                          script's captured log output, the data table it returned, the \
                          refresh rate, and any error with its line number. Use this after \
                          every edit — it is the fastest way to see what a change did.")]
    pub async fn render_screen(
        &self,
        Parameters(a): Parameters<RenderArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let opts = RenderOpts {
            model: a.model.unwrap_or_else(|| "og".to_string()),
            width: a.width,
            height: a.height,
            panel: a.panel,
            dither: a.dither,
            timestamp: a.timestamp,
            include_raw: a.include_raw,
            ..RenderOpts::default()
        };
        let screen_ref = a.screen_ref.clone();
        let result = blocking(move || store.render(&screen_ref, opts)).await?;

        let diagnostics = RenderDiagnostics {
            log: result.log,
            data: result.data,
            refresh_rate: result.refresh_rate,
            error: result.error.as_ref().map(|e| RenderErrorOut {
                line: e.line,
                message: e.message.clone(),
            }),
        };
        let failed = diagnostics.error.is_some();

        let mut content: Vec<ContentBlock> = Vec::new();
        let b64 = base64::engine::general_purpose::STANDARD;
        // A failed render has an empty `png` by contract — emit only the
        // diagnostics so the agent isn't handed a zero-byte image.
        if !result.png.is_empty() {
            content.push(ContentBlock::image(b64.encode(&result.png), "image/png"));
        }
        if let Some(raw) = &result.raw_png {
            content.push(ContentBlock::image(b64.encode(raw), "image/png"));
        }
        content.push(ContentBlock::text(
            serde_json::to_string_pretty(&diagnostics)
                .unwrap_or_else(|e| format!("failed to serialize diagnostics: {e}")),
        ));

        let structured = serde_json::to_value(&diagnostics)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        // Constructed directly — `CallToolResult` has no structured-content
        // builder, and a failed render must be flagged `is_error` while still
        // carrying its diagnostics as visible content.
        Ok(CallToolResult {
            content,
            structured_content: Some(structured),
            is_error: Some(failed),
            meta: None,
        })
    }

    #[tool(description = "Statically check a screen without running it: meta.yaml against its \
                          schema, script.lua compiled (not executed), and screen.svg parsed \
                          with its extends/include chain resolved.")]
    pub async fn validate_screen(
        &self,
        Parameters(a): Parameters<ValidateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use crate::services::screen_store::Severity;
        let store = self.state.screen_store.clone();
        let report = blocking(move || store.validate(&a.screen_ref)).await?;
        // `validate` reports findings rather than failing, so a screen with
        // issues is a *successful* call whose payload says `ok: false`. Do
        // not flag it `is_error` — the agent is meant to read the issues.
        ok_json(ValidateOutput {
            ok: report.ok,
            issues: report
                .issues
                .into_iter()
                .map(|i| IssueOut {
                    severity: match i.severity {
                        Severity::Error => "error".to_string(),
                        Severity::Warning => "warning".to_string(),
                    },
                    location: i.location,
                    message: i.message,
                })
                .collect(),
        })
    }
}
```

Add `base64 = "0.22"` to `[dependencies]` if it is not already present (rmcp depends on it, but do not rely on a transitive dep — declare it).

Update this file's `use super::…` line to `use super::{blocking, ok_json, ByonkMcp};`.

- [ ] **Step 4: Fold in the router**

`pub mod tools_render;` in `src/mcp/mod.rs`, and add `+ tools_render::tools_render_router()`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test mcp_tools_test -- --test-threads=4`
Expected: all tests PASS.

- [ ] **Step 6: Run the full gate**

Run: `make check`

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/mcp/ tests/mcp_tools_test.rs
git commit -m "feat: MCP render_screen and validate_screen with diagnostics"
```

---

## Task 9: `assign_screen`

Assigning a device to a screen is a *device* write, not a global-config write, so it stays allowed in add-on mode — matching `patch_device`'s existing behaviour (`require_writable_global` is not called there).

This task also extracts the device-upsert core out of the axum handler so the MCP tool and the REST handler share one implementation, including screen/param validation and the write lock.

**Files:**
- Modify: `src/api/admin/write.rs:177-244`
- Create: `src/mcp/tools_device.rs`
- Modify: `src/mcp/mod.rs`
- Modify: `tests/mcp_tools_test.rs`

**Interfaces:**
- Produces: `pub async fn apply_device_patch(state: &AppState, key: &str, body: DeviceWrite) -> Result<serde_json::Value, ApiError>` in `src/api/admin/write.rs`; `pub fn tools_device_router() -> ToolRouter<ByonkMcp>`

- [ ] **Step 1: Write the failing test**

Append to `tests/mcp_tools_test.rs`:

```rust
#[tokio::test]
async fn test_assign_screen_updates_the_device_mapping() {
    let tmp = tempfile::tempdir().unwrap();
    // A file-backed config is required — device writes persist to disk.
    let (app, _config_path) = TestApp::new_admin_with_file("secret", tmp.path());
    let mac = "11:22:33:44:55:66";
    app.register_device(mac).await;

    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "byonk-builtin/default" }),
        )
        .await;

    assert_eq!(structured(&result)["screen"], "byonk-builtin/default");

    // And it is visible through list_devices.
    let devices = client.call_tool("list_devices", serde_json::json!({})).await;
    let d = structured(&devices)["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["mac"] == mac)
        .cloned()
        .expect("device must be listed");
    assert_eq!(d["screen"], "byonk-builtin/default");
}

#[tokio::test]
async fn test_assign_screen_rejects_an_unknown_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let (app, _config_path) = TestApp::new_admin_with_file("secret", tmp.path());
    let mac = "11:22:33:44:55:66";
    app.register_device(mac).await;

    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let result = client
        .call_tool(
            "assign_screen",
            serde_json::json!({ "mac": mac, "screen_ref": "local/does-not-exist" }),
        )
        .await;

    assert_eq!(result["isError"], true);
}
```

Check `TestApp::new_admin_with_file`'s return shape at `tests/common/app.rs:211` and match it.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test mcp_tools_test -- --test-threads=4`
Expected: both FAIL — `assign_screen` not found.

- [ ] **Step 3: Extract the shared device-patch core**

In `src/api/admin/write.rs`, move the body of `patch_device` (everything after `require_admin`) into:

```rust
/// Apply a device patch: merge with the existing entry, validate the screen
/// and params, and persist. Shared by `PATCH /api/admin/devices/{key}` and
/// the MCP `assign_screen` tool so both enforce identical rules — screen
/// existence, param schema, the config write lock, and rollback on a failed
/// reload.
///
/// Deliberately does NOT call `require_writable_global`: a device mapping is
/// not global config, so it stays writable in add-on mode.
pub async fn apply_device_patch(
    state: &AppState,
    key: &str,
    body: DeviceWrite,
) -> Result<serde_json::Value, ApiError> {
    let path = require_file_config(state)?;
    let _guard = state.write_lock.lock().await;
    // ... existing patch_device body, with `key` borrowed and `state` by ref ...
}
```

Then `patch_device` becomes:

```rust
pub async fn patch_device(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<DeviceWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    Ok(Json(apply_device_patch(&state, &key, body).await?))
}
```

- [ ] **Step 4: Confirm the refactor changed no behaviour**

Run: `cargo test --test admin_devices_test --test admin_write_test -- --test-threads=4`
Expected: all existing device tests still PASS, unchanged.

- [ ] **Step 5: Write `src/mcp/tools_device.rs`**

```rust
//! Device assignment. A device mapping is not global config, so this stays
//! available when byonk runs as a Home Assistant app — matching
//! `PATCH /api/admin/devices/{key}`.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_router, ErrorData,
};
use serde::{Deserialize, Serialize};

use super::{ok_json, ByonkMcp};
use crate::api::admin::write::{apply_device_patch, DeviceWrite};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AssignScreenArgs {
    /// Device MAC (or its config key), as reported by `list_devices`.
    pub mac: String,
    /// Screen reference to assign, `handle/path`.
    pub screen_ref: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AssignScreenOutput {
    pub key: String,
    pub screen: String,
}

#[tool_router(router = tools_device_router, vis = "pub")]
impl ByonkMcp {
    #[tool(description = "Assign a device to a screen. The screen must exist and the device \
                          must already be known — call list_devices first. Assigning replaces \
                          the device's params with the new screen's defaults.")]
    pub async fn assign_screen(
        &self,
        Parameters(a): Parameters<AssignScreenArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = DeviceWrite {
            key: None,
            screen: Some(a.screen_ref.clone()),
            panel: None,
            dither: None,
            colors: None,
            params: None,
            refresh: None,
            name: None,
        };
        match apply_device_patch(&self.state, &a.mac, body).await {
            Ok(value) => ok_json(AssignScreenOutput {
                key: value["key"].as_str().unwrap_or(&a.mac).to_string(),
                screen: value["screen"].as_str().unwrap_or(&a.screen_ref).to_string(),
            }),
            // Tool-level, not protocol-level: "unknown screen `local/x`" and
            // "device not found" are exactly the messages the agent needs to
            // read and act on.
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(
                e.to_string(),
            )])),
        }
    }
}
```

`e.to_string()` requires `ApiError: Display`. Check `src/error.rs`: if it derives `thiserror::Error` the impl exists; if not, add whichever accessor yields the human-readable reason and use that instead — the message must survive, because it is the agent's only clue.

- [ ] **Step 6: Fold in the router**

`pub mod tools_device;` and `+ tools_device::tools_device_router()`.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test --test mcp_tools_test -- --test-threads=4`
Expected: all PASS.

- [ ] **Step 8: Run the full gate**

Run: `make check`

- [ ] **Step 9: Commit**

```bash
git add src/api/admin/write.rs src/mcp/ tests/mcp_tools_test.rs
git commit -m "feat: MCP assign_screen sharing the admin device-patch core"
```

---

## Task 10: Generate the `meta.yaml` JSON Schema from the parse types

The schema is served as an MCP resource in Task 11. Generating it from the very types that parse `meta.yaml` is what keeps it from drifting into a lie.

**Files:**
- Modify: `src/models/screen_meta.rs`, `src/models/param_schema.rs`
- Test: `tests/screen_meta_schema_test.rs` (new)

**Interfaces:**
- Produces: `pub fn meta_json_schema() -> serde_json::Value` in `src/models/screen_meta.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/screen_meta_schema_test.rs`:

```rust
//! The published meta.yaml schema must describe what the parser actually
//! accepts — it is served to LLM authors as a contract.

use byonk::models::screen_meta::{meta_json_schema, ScreenMeta};

#[test]
fn test_schema_requires_exactly_what_the_parser_requires() {
    let schema = meta_json_schema();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("schema must declare required fields")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // These three are the ones from_yaml rejects a document for missing.
    assert!(required.contains(&"title"));
    assert!(required.contains(&"description"));
    assert!(required.contains(&"byonk"));
    assert!(!required.contains(&"refresh"), "refresh is optional");
    assert!(!required.contains(&"params"), "params is optional");
}

#[test]
fn test_schema_documents_every_optional_top_level_field() {
    let schema = meta_json_schema();
    let props = schema["properties"].as_object().unwrap();
    for f in ["title", "description", "byonk", "refresh", "params"] {
        assert!(props.contains_key(f), "schema is missing property {f}");
    }
}

#[test]
fn test_parser_agrees_with_the_schemas_required_set() {
    // Guard against drift in the other direction: a document with only the
    // required fields must parse.
    let minimal = "title: t\ndescription: d\nbyonk: \"0.17\"\n";
    assert!(ScreenMeta::from_yaml(minimal).is_ok());

    // And dropping any one of them must fail.
    for drop in ["title: t\n", "description: d\n", "byonk: \"0.17\"\n"] {
        let src = minimal.replace(drop, "");
        assert!(
            ScreenMeta::from_yaml(&src).is_err(),
            "parser accepted a document missing a schema-required field: {src}"
        );
    }
}

#[test]
fn test_params_schema_describes_the_field_descriptor() {
    let schema = meta_json_schema();
    let text = serde_json::to_string(&schema).unwrap();
    // The params sub-language is the part an author most needs spelled out.
    for token in ["type", "required", "options", "label"] {
        assert!(
            text.contains(token),
            "params descriptor is missing `{token}` in the published schema"
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test screen_meta_schema_test -- --test-threads=4`
Expected: FAIL to compile — `meta_json_schema` does not exist.

- [ ] **Step 3: Derive `JsonSchema` on the param descriptor types**

In `src/models/param_schema.rs`, add `rmcp::schemars::JsonSchema` to the derives on `ParamType`, `EnumOption`, and `RawField`, and make `RawField` `pub`. Add at the top:

```rust
// The schema derives resolve `schemars::` — use rmcp's re-export so there is
// exactly one schemars version in the tree.
use rmcp::schemars;
```

`RawField::options` is `Option<serde_yaml::Value>`, which schemars cannot describe. Annotate it:

```rust
    /// Enum choices: either a list of strings, or a list of
    /// `{value, label}` maps.
    #[serde(default)]
    #[schemars(with = "Option<Vec<EnumOption>>")]
    options: Option<serde_yaml::Value>,
```

- [ ] **Step 4: Derive it on `RawMeta` and expose the generator**

In `src/models/screen_meta.rs`:

```rust
use rmcp::schemars;

#[derive(Deserialize, schemars::JsonSchema)]
struct RawMeta {
    /// Human-readable screen title.
    title: String,
    /// One-line description of what the screen shows.
    description: String,
    /// Engine compatibility requirement, a caret range like `"0.17"`.
    /// Note this is a RANGE, not a minimum: `"0.15"` excludes 0.17.x.
    byonk: String,
    /// Default refresh interval in seconds.
    #[serde(default)]
    refresh: Option<u32>,
    /// Parameter declarations, keyed by parameter name.
    #[serde(default)]
    #[schemars(with = "std::collections::HashMap<String, crate::models::param_schema::RawField>")]
    params: serde_yaml::Value,
}

/// The JSON Schema for `meta.yaml`, generated from the very type that parses
/// it — so the contract byonk publishes to authors cannot drift away from
/// what byonk actually accepts.
pub fn meta_json_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(RawMeta))
        .expect("meta.yaml schema must serialize")
}
```

Check `schemars` 1.0's `schema_for!` return type — it is a `Schema` that serializes to a JSON object. If `required` lands somewhere other than the top level in the generated document, adjust the test's path rather than hand-editing the schema.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test screen_meta_schema_test -- --test-threads=4`
Expected: 4 tests PASS.

- [ ] **Step 6: Run the full gate**

Run: `make check`

- [ ] **Step 7: Commit**

```bash
git add src/models/screen_meta.rs src/models/param_schema.rs tests/screen_meta_schema_test.rs
git commit -m "feat: generate the meta.yaml JSON Schema from the parse types"
```

---

## Task 11: Authoring-contract resources

The agent learns byonk's rules from the byonk it is editing. The Lua and SVG references are the existing `docs/src/` pages, embedded verbatim — one source of truth for humans and agents.

**Files:**
- Create: `src/mcp/resources.rs`
- Modify: `src/mcp/mod.rs` (`list_resources` / `read_resource`)
- Test: `tests/mcp_resources_test.rs` (new)

**Interfaces:**
- Consumes: `meta_json_schema()` (Task 10); `ScreenStore::list_screens` and `read_file` (Task 4 / Plan 1); `blocking` (Task 6)
- Produces: `pub fn list(state: &AppState) -> Vec<rmcp::model::Resource>` and `pub fn read(uri: &str, state: &AppState) -> Option<Vec<ResourceContents>>` in `src/mcp/resources.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/mcp_resources_test.rs`:

```rust
mod common;

use common::mcp::McpTestClient;
use common::TestApp;

#[tokio::test]
async fn test_resources_list_includes_the_authoring_contracts() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client.raw("resources/list", serde_json::json!({})).await;
    let v: serde_json::Value = resp.json();
    let uris: Vec<String> = v["result"]["resources"]
        .as_array()
        .expect("resources array")
        .iter()
        .map(|r| r["uri"].as_str().unwrap().to_string())
        .collect();

    for expected in [
        "byonk://reference/lua-api",
        "byonk://reference/svg-templates",
        "byonk://reference/authoring",
        "byonk://schema/meta.yaml",
    ] {
        assert!(uris.contains(&expected.to_string()), "missing {expected} in {uris:?}");
    }
}

#[tokio::test]
async fn test_reading_the_lua_reference_returns_the_shipped_doc() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client
        .raw(
            "resources/read",
            serde_json::json!({ "uri": "byonk://reference/lua-api" }),
        )
        .await;
    let v: serde_json::Value = resp.json();
    let text = v["result"]["contents"][0]["text"].as_str().unwrap();

    assert!(text.contains("log_info"), "Lua reference looks wrong");
    assert!(text.len() > 1000, "Lua reference is suspiciously short");
}

#[tokio::test]
async fn test_reading_the_meta_schema_returns_json() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client
        .raw(
            "resources/read",
            serde_json::json!({ "uri": "byonk://schema/meta.yaml" }),
        )
        .await;
    let v: serde_json::Value = resp.json();
    let text = v["result"]["contents"][0]["text"].as_str().unwrap();

    let schema: serde_json::Value = serde_json::from_str(text).expect("must be valid JSON");
    assert!(schema["properties"]["title"].is_object());
}

#[tokio::test]
async fn test_worked_examples_are_listed_and_readable() {
    // The examples repo is seeded on first run, so it needs a real
    // SCREENS_DIR — an embedded-only app has nothing to seed into.
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client.raw("resources/list", serde_json::json!({})).await;
    let v: serde_json::Value = resp.json();
    let examples: Vec<String> = v["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["uri"].as_str())
        .filter(|u| u.starts_with("byonk://examples/"))
        .map(|u| u.to_string())
        .collect();
    assert!(
        !examples.is_empty(),
        "the shipped examples must be exposed as resources"
    );

    let resp = client
        .raw(
            "resources/read",
            serde_json::json!({ "uri": examples[0] }),
        )
        .await;
    let v: serde_json::Value = resp.json();
    let text = v["result"]["contents"][0]["text"].as_str().unwrap();
    // A worked example is only useful if it shows the full triple.
    for section in ["meta.yaml", "script.lua", "screen.svg"] {
        assert!(text.contains(section), "example is missing {section}");
    }
}

#[tokio::test]
async fn test_examples_resource_cannot_read_other_repos() {
    let tmp = tempfile::tempdir().unwrap();
    let app = TestApp::new_admin_with_screens("secret", tmp.path());
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    // `byonk-builtin/default` exists, but not under the examples handle —
    // the prefix must not become a general read primitive.
    let resp = client
        .raw(
            "resources/read",
            serde_json::json!({ "uri": "byonk://examples/../byonk-builtin/default" }),
        )
        .await;
    let v: serde_json::Value = resp.json();

    assert!(v.get("error").is_some(), "must not resolve: {v}");
}

#[tokio::test]
async fn test_reading_an_unknown_resource_is_an_error() {
    let app = TestApp::new_admin("secret");
    let client = McpTestClient::new(&app, Some("secret"));
    client.initialize().await;

    let resp = client
        .raw(
            "resources/read",
            serde_json::json!({ "uri": "byonk://reference/nope" }),
        )
        .await;
    let v: serde_json::Value = resp.json();

    assert!(v.get("error").is_some(), "unknown URI must error: {v}");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test mcp_resources_test -- --test-threads=4`
Expected: FAIL — `resources/list` returns an empty list, `resources/read` is method-not-found.

- [ ] **Step 3: Embed the reference docs**

In `src/assets.rs`, next to the other `RustEmbed` types:

```rust
/// Authoring reference pages, embedded so the MCP server can serve them as
/// contracts to an LLM author. These are the SAME files the mdBook builds —
/// there is deliberately no second copy to drift.
#[derive(RustEmbed)]
#[folder = "docs/src/"]
#[include = "api/lua-api.md"]
#[include = "tutorial/svg-templates.md"]
#[include = "guide/authoring.md"]
pub struct EmbeddedDocs;
```

Confirm the `folder` path is correct relative to `CARGO_MANIFEST_DIR` and that `rust-embed`'s `include-exclude` feature (already enabled) applies.

- [ ] **Step 4: Write `src/mcp/resources.rs`**

```rust
//! MCP resources: the authoring contracts, served from the server the agent
//! is editing. No local scaffolding, no filesystem access needed.

use rmcp::model::{Resource, ResourceContents};

use crate::assets::EmbeddedDocs;

/// (uri, embedded path, name, title, description)
const DOCS: &[(&str, &str, &str, &str)] = &[
    (
        "byonk://reference/lua-api",
        "api/lua-api.md",
        "lua-api",
        "Every global and function byonk injects into a screen's script.lua, \
         and the contract for the table it returns.",
    ),
    (
        "byonk://reference/svg-templates",
        "tutorial/svg-templates.md",
        "svg-templates",
        "How screen.svg works: Tera syntax, the byonk-base-v1 layout library, \
         the blocks it exposes, and the extends/include conventions.",
    ),
    (
        "byonk://reference/authoring",
        "guide/authoring.md",
        "authoring",
        "How screens, screen repos and writability fit together on this server.",
    ),
];

const META_SCHEMA_URI: &str = "byonk://schema/meta.yaml";
/// Worked examples are addressed `byonk://examples/<screen path>` — one
/// resource per shipped example screen.
const EXAMPLES_PREFIX: &str = "byonk://examples/";
/// The handle the examples repo is seeded under (Plan 1, Task 11).
const EXAMPLES_HANDLE: &str = "examples";

pub fn list(state: &AppState) -> Vec<Resource> {
    let mut out: Vec<Resource> = DOCS
        .iter()
        .map(|(uri, _, name, description)| {
            Resource::new(*uri, *name)
                .with_description(*description)
                .with_mime_type("text/markdown")
        })
        .collect();
    out.push(
        Resource::new(META_SCHEMA_URI, "meta-yaml-schema")
            .with_description(
                "JSON Schema for a screen's meta.yaml, generated from the type that \
                 parses it — including the params descriptor sub-language.",
            )
            .with_mime_type("application/json"),
    );

    // One resource per shipped example. These are real, working screens on
    // this very server, so an agent can read a complete meta+lua+svg triple
    // that is known to render here — far better grounding than prose.
    for screen in state.screen_store.list_screens() {
        if screen.handle != EXAMPLES_HANDLE {
            continue;
        }
        out.push(
            Resource::new(
                format!("{EXAMPLES_PREFIX}{}", screen.path),
                format!("example-{}", screen.path.replace('/', "-")),
            )
            .with_title(screen.title.clone())
            .with_description(format!(
                "Worked example — {}. Full source of meta.yaml, script.lua and screen.svg.",
                screen.description
            ))
            .with_mime_type("text/markdown"),
        );
    }
    out
}

pub fn read(uri: &str, state: &AppState) -> Option<Vec<ResourceContents>> {
    if uri == META_SCHEMA_URI {
        let schema = crate::models::screen_meta::meta_json_schema();
        return Some(vec![ResourceContents::TextResourceContents {
            uri: uri.to_string(),
            mime_type: Some("application/json".to_string()),
            text: serde_json::to_string_pretty(&schema).ok()?,
            meta: None,
        }]);
    }

    if let Some(path) = uri.strip_prefix(EXAMPLES_PREFIX) {
        let screen_ref = format!("{EXAMPLES_HANDLE}/{path}");
        // Refuse anything that isn't actually a listed example, so this
        // cannot be turned into a general read primitive for other repos.
        if !state
            .screen_store
            .list_screens()
            .iter()
            .any(|s| s.screen_ref == screen_ref)
        {
            return None;
        }
        let mut text = format!("# Example: {screen_ref}\n");
        for file in ["meta.yaml", "script.lua", "screen.svg"] {
            let body = state
                .screen_store
                .read_file(&screen_ref, file)
                .ok()
                .map(|c| String::from_utf8_lossy(&c.bytes).into_owned())
                .unwrap_or_else(|| "(unreadable)".to_string());
            let lang = match file {
                "meta.yaml" => "yaml",
                "script.lua" => "lua",
                _ => "xml",
            };
            text.push_str(&format!("\n## {file}\n\n```{lang}\n{body}\n```\n"));
        }
        return Some(vec![ResourceContents::TextResourceContents {
            uri: uri.to_string(),
            mime_type: Some("text/markdown".to_string()),
            text,
            meta: None,
        }]);
    }

    let (_, path, _, _) = DOCS.iter().find(|(u, _, _, _)| *u == uri)?;
    let file = EmbeddedDocs::get(path)?;
    Some(vec![ResourceContents::TextResourceContents {
        uri: uri.to_string(),
        mime_type: Some("text/markdown".to_string()),
        text: String::from_utf8(file.data.to_vec()).ok()?,
        meta: None,
    }])
}
```

Both functions call the synchronous `ScreenStore`, so the handler methods in the next step must wrap them in `blocking(...)` like every other `ScreenStore` caller. Add `use crate::server::AppState;` to this file's imports.

- [ ] **Step 5: Implement the handler methods**

In `src/mcp/mod.rs`, inside the `#[tool_handler] impl ServerHandler for ByonkMcp` block:

```rust
    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::model::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, ErrorData> {
        let state = self.state.clone();
        let list = blocking(move || resources::list(&state)).await?;
        // `with_all_items` is the constructor rmcp's `paginated_result!`
        // macro generates; it fills in `next_cursor: None` and `meta: None`.
        Ok(rmcp::model::ListResourcesResult::with_all_items(list))
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::model::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResult, ErrorData> {
        let state = self.state.clone();
        let uri = request.uri.clone();
        let found = blocking(move || resources::read(&uri, &state)).await?;
        match found {
            Some(contents) => Ok(rmcp::model::ReadResourceResult {
                contents,
                meta: None,
            }),
            // A resource read is addressed by URI, not chosen from arguments,
            // so an unknown URI genuinely is a protocol-level fault.
            None => Err(ErrorData::resource_not_found(
                format!("unknown resource: {}", request.uri),
                None,
            )),
        }
    }
```

Declare `pub mod resources;` and add `use rmcp::ErrorData;` to the imports.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --test mcp_resources_test -- --test-threads=4`
Expected: 4 tests PASS.

- [ ] **Step 7: Run the full gate**

Run: `make check && make docs`

- [ ] **Step 8: Commit**

```bash
git add src/assets.rs src/mcp/ tests/mcp_resources_test.rs
git commit -m "feat: serve authoring contracts as MCP resources"
```

---

## Task 12: Documentation and changelog

**Files:**
- Create: `docs/src/guide/mcp.md`
- Modify: `docs/src/SUMMARY.md`, `CHANGES.md`
- Modify: `docs/src/tutorial/first-screen.md:67,187`, `docs/src/api/admin-api.md:537,573` (the parked compat-doc fix)

- [ ] **Step 1: Clear the parked `byonk: "0.15"` documentation defect**

From `docs/HANDOVER.md`'s known-parked list. Four sites still show `byonk: "0.15"`, and `first-screen.md:67` calls it the "minimum engine version" — it is a **caret range**, so `"0.15"` *excludes* 0.17.x. An author following the tutorial hand-writes a screen that reports a false compatibility warning.

Update all four to the current engine's major.minor and correct the "minimum engine version" wording to describe a caret range. Grep for other occurrences before finishing:

```bash
grep -rn 'byonk: "0\.15"' docs/src/
```

- [ ] **Step 2: Write the user guide**

Create `docs/src/guide/mcp.md` covering:
- What the MCP endpoint is and why it exists (author screens from an LLM, over the LAN, no file access, no Samba).
- **Prerequisite: an admin token.** Without one `/mcp` is 404 — the same rule as the admin API. Point at where the token is set (config `admin.token`, `BYONK_ADMIN_TOKEN`, or the HA app's Options).
- Connecting: the URL is `http://<host>:<port>/mcp`, transport is streamable HTTP, auth is `Authorization: Bearer <token>`. Give a concrete `claude mcp add` invocation and the equivalent JSON config block.
- The tool list, grouped as read / edit / render / assign, one line each.
- The resources, and the advice to read `byonk://reference/lua-api` before writing a script.
- The workflow that actually works: `list_screens` → `copy_screen` a builtin or example → edit → `render_screen` → read `log`/`error.line` → repeat → `assign_screen`.
- A security note: the token grants full screen-authoring and device-assignment rights; `/mcp` accepts any `Host`, so do not expose the port to the internet.

Add it to `docs/src/SUMMARY.md` under "Getting Started", after `Screen Authoring`:

```markdown
- [Authoring with an LLM (MCP)](guide/mcp.md)
```

- [ ] **Step 3: Write the changelog entry**

In `CHANGES.md`, under `## Unreleased` → `### New` (user-facing only — no mention of rmcp internals, refactors or test scaffolding):

```markdown
- **Author screens with an LLM over MCP.** Byonk now exposes a Model Context
  Protocol endpoint at `/mcp`, so an assistant like Claude Code can list, read,
  create, edit, validate and render screens on a running byonk — including one
  inside Home Assistant — over the network, with no file access needed. It is
  protected by the same admin token as the admin API, and is invisible (404)
  until you set one. The server also publishes its own authoring references
  (Lua API, SVG templates, the `meta.yaml` schema) so the assistant works from
  this server's rules rather than guesswork.
```

Under `### Fixed`:

```markdown
- Screen repositories no longer follow symbolic links that point outside the
  repository, so a screen repo cannot expose files elsewhere on the server.
```

- [ ] **Step 4: Verify the docs build**

Run: `make docs`
Expected: builds clean, `guide/mcp.md` appears in the rendered summary.

- [ ] **Step 5: Run the full gate one last time**

Run: `make check`
Expected: fmt clean, clippy clean, every test green. Record the test count.

- [ ] **Step 6: Commit**

```bash
git add docs/src/guide/mcp.md docs/src/SUMMARY.md docs/src/tutorial/first-screen.md docs/src/api/admin-api.md CHANGES.md
git commit -m "docs: MCP authoring guide; fix stale byonk compat examples"
```

---

## Post-plan verification (not a task — do this before declaring the branch done)

- [ ] `make check` and `make docs` green on the final commit.
- [ ] Connect a real MCP client (`claude mcp add --transport http byonk http://localhost:3000/mcp --header "Authorization: Bearer <token>"`) against a locally running `byonk serve` and drive the full loop: list → copy → edit → render → assign. A passing integration suite does not prove a real client can negotiate the handshake.
- [ ] Validate on the HA VM per the `ha-vm-from-source-addon-build` memory, reaching `/mcp` from the Mac host over the LAN (this is the case `.disable_allowed_hosts()` exists for — it is exactly what the loopback default would have blocked).
- [ ] Confirm `/mcp` returns 404 on an install with no admin token configured.

## Deferred — explicitly not in this plan

- **REST screen-write routes under `/api/admin/screens/*`.** The spec's architecture diagram shows them beside MCP, but they are Spec 2's (the Svelte UI's) consumer surface. `ScreenStore` already supports everything they will need.
- **The HA app options schema `path:` variant.** Still unsafe to add alone: `apply_to_config` runs *after* config validation, so a `path`+`repo` pair would bypass `ScreenRepoRef::validate`, and `build_disk_sources` checks `path` first so it would silently win. `local` and `examples` auto-register with zero config, so nothing here is blocked by it.
- **Git commit/history tools.** Spec 3.
