# Screen Store Authoring Core — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give byonk a typed, writable screen-source model and a `ScreenStore` core that can create, edit, validate, and render screens on disk — the browser-free, MCP-free foundation for screen authoring.

**Architecture:** Writability becomes a structural property of each `ScreenRepoSource` (`writable_root()`). The single embedded `byonk-builtin` overlay is split into three layers: the untouched `byonk-base-v1` include library, a minimal embedded `byonk-builtin` repo (`default` + `calibration/*`), and shipped `examples` seeded to a writable `local`/`examples` repo. `ScreenStore` is the sole mutation/validation/render-orchestration owner, sitting beside the read-only `AssetLoader`. A one-time startup migration moves pre-existing user screens from the `byonk-builtin` overlay to the new `local` handle.

**Tech Stack:** Rust, axum 0.8 (upgraded from 0.7), mlua 0.10, Tera, rust-embed, serde/serde_yaml, blake3 (etag hashing).

**Spec:** `docs/superpowers/specs/2026-07-24-screen-store-and-mcp-design.md` (Components 1–4). Component 5 (MCP) is a separate follow-up plan.

## Global Constraints

- **No `git add -A` / `git add .`** — pre-existing untracked local files must never be swept in. Stage by explicit path; verify `git diff --cached` before every commit. (memory `no-git-add-all`)
- **CHANGES.md entries are user-facing only** — describe user-visible changes vs. the last release; keep out CI/tooling/version/dev-process. (memory `changelog-user-facing-only`)
- **Build/verify gates:** `make check` = fmt + `clippy -- -D warnings` + tests. `make docs` must stay clean. Run `make check` green before every commit that touches Rust.
- **`byonk-builtin` handle value is frozen** — device configs reference it; never rename the handle string, only what it embeds.
- **`byonk-base-v1` include library is untouched** — it stays embedded (`EmbeddedBase`), universal, versioned, read-only.
- **Rust toolchain via `rust-toolchain.toml`** (rustup); never add cargo/rust to mise. (memory `rust-toolchain-via-rustup`)
- **Release image is `FROM scratch`** — no `/tmp`, no shell, no git binary; anything on-disk lives under `/data`. (memory `scratch-image-no-tmp`)

---

### Task 1: Upgrade axum 0.7 → 0.8

**Files:**
- Modify: `Cargo.toml:14` (`axum = "0.7"` → `"0.8"`), `Cargo.toml:39` (`utoipa-swagger-ui = { version = "8", … }` → `"9"`)
- Modify: `src/server.rs:219` (`"/api/image/:hash"` → `"/api/image/{hash}"`)
- Modify: `src/main.rs:845` (`"/dev/panel-colors/:panel"` → `"/dev/panel-colors/{panel}"`)
- Possibly modify: any custom `FromRequest`/`FromRequestParts` impls (search below)

**Interfaces:**
- Consumes: nothing (first task).
- Produces: a green tree on axum 0.8; no API/behavior change. All later tasks build on 0.8.

- [ ] **Step 1: Find axum breakage surface**

Run: `grep -rn "async_trait\|FromRequest\|FromRequestParts\|:hash\|:panel\|:mac\|:hash\|Path<" src/ | grep -v "test"`
Expected: the two route strings above, plus any extractor impls. Note each hit for Step 3.

- [ ] **Step 2: Bump versions and route syntax**

Edit `Cargo.toml`: `axum = "0.8"` and `utoipa-swagger-ui = { version = "9", features = ["axum"] }`.
Edit the two route strings to `{param}` brace syntax.

- [ ] **Step 3: Fix extractor fallout**

axum 0.8 removed the `#[async_trait]` requirement on `FromRequest`/`FromRequestParts` and changed `Path` deserialization for single params. For each hit from Step 1: remove `#[async_trait]` and the `async_trait` import where an axum extractor uses it; adjust any `Path<(T,)>` vs `Path<T>` mismatch. If Step 1 found no custom extractors, this step is a no-op — record that.

- [ ] **Step 4: Build and test**

Run: `make check`
Expected: PASS (fmt clean, clippy clean, all existing tests green). If `utoipa-swagger-ui` 9 pulls a conflicting `utoipa` minor, bump `utoipa` to the matching 5.x and re-run.

- [ ] **Step 5: Smoke-test routing**

Run: `cargo test --lib -- server 2>&1 | tail -20` and confirm any router-construction tests pass. If none exist, add a minimal one:

```rust
#[test]
fn image_route_uses_brace_syntax() {
    // build_router must accept the {hash} path without panicking
    let state = crate::server::test_state();
    let _ = crate::server::build_router(state); // panics on bad route syntax
}
```
(Use the existing test-state constructor; if none, reuse the pattern in `write.rs:562` `state_with_addon_mode`.)

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/server.rs src/main.rs
# add any extractor files touched in Step 3, by explicit path
git commit -m "chore: upgrade axum 0.7 -> 0.8 (prereq for rmcp/MCP)"
```

---

### Task 2: Add `writable_root()` to `ScreenRepoSource`; rename disk source

**Files:**
- Modify: `src/services/screen_repo_loader.rs` (trait + `EmbeddedBuiltinSource` + `DiskScreenRepoSource`)
- Test: `src/services/screen_repo_loader.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Consumes: axum 0.8 tree (Task 1).
- Produces:
  - `ScreenRepoSource::writable_root(&self) -> Option<&std::path::Path>` (default `None`).
  - `EmbeddedBuiltinSource::writable_root() == None`.
  - `GitScreenRepoSource` (renamed from `DiskScreenRepoSource`), `writable_root() == None`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn embedded_and_git_sources_are_read_only() {
    let loader = std::sync::Arc::new(AssetLoader::new(None, None, None));
    let src = EmbeddedBuiltinSource::load(loader).unwrap();
    assert!(src.writable_root().is_none(), "embedded builtin must be read-only");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib embedded_and_git_sources_are_read_only`
Expected: FAIL — `no method named writable_root`.

- [ ] **Step 3: Add the trait method and impls**

In the `ScreenRepoSource` trait, add (with a default so existing impls compile):

```rust
/// On-disk directory this source may be written to, or `None` if read-only
/// (embedded, or a git cache that a refresh would clobber).
fn writable_root(&self) -> Option<&std::path::Path> { None }
```

`EmbeddedBuiltinSource` inherits the `None` default (leave it). Rename `DiskScreenRepoSource` → `GitScreenRepoSource` throughout this file (struct, `impl`, `load`, doc comments) and keep its default `writable_root() == None`. Update the one call site in `screen_repo_loader.rs::new` (`DiskScreenRepoSource::load` → `GitScreenRepoSource::load`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib embedded_and_git_sources_are_read_only && grep -rn "DiskScreenRepoSource" src/`
Expected: test PASS; grep returns nothing (rename complete).

- [ ] **Step 5: Commit**

```bash
git add src/services/screen_repo_loader.rs
git commit -m "refactor: add writable_root() to ScreenRepoSource; rename DiskScreenRepoSource -> GitScreenRepoSource"
```

---

### Task 3: `LocalScreenRepoSource` — a writable on-disk source

**Files:**
- Modify: `src/services/screen_repo_loader.rs` (new struct near `GitScreenRepoSource`)
- Test: `src/services/screen_repo_loader.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `ScreenRepoSource` trait w/ `writable_root` (Task 2), `ScreenRepoManifest` (`src/models/screen_repo_manifest.rs`).
- Produces:
  - `LocalScreenRepoSource::load(root: &Path) -> Result<LocalScreenRepoSource, String>`
  - `LocalScreenRepoSource::writable_root() -> Some(&Path)` (the root)
  - Same read behavior as `GitScreenRepoSource` (both read the same on-disk layout).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn local_source_is_writable_and_reads_disk() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("byonk-screens.yaml"),
        "name: local\ndescription: d\nauthor: a\nlicense: MIT\n").unwrap();
    std::fs::create_dir_all(dir.path().join("clock")).unwrap();
    std::fs::write(dir.path().join("clock/meta.yaml"),
        "title: Clock\ndescription: d\nbyonk: \"0.15\"\n").unwrap();
    let src = LocalScreenRepoSource::load(dir.path()).unwrap();
    assert_eq!(src.writable_root(), Some(dir.path()));
    assert!(src.screen_paths().iter().any(|p| p == "clock"));
}
```
(If `tempfile` is not yet a dev-dependency, add `tempfile` under `[dev-dependencies]` in `Cargo.toml` in this step.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib local_source_is_writable_and_reads_disk`
Expected: FAIL — `LocalScreenRepoSource` unresolved.

- [ ] **Step 3: Implement `LocalScreenRepoSource`**

Model it on `GitScreenRepoSource`: hold `root: PathBuf` + parsed `ScreenRepoManifest`, implement `read` (read file under `root.join(manifest_root).join(rel)`), `screen_paths` (walk for dirs containing `meta.yaml`), `svg_files`, `manifest`, and:

```rust
fn writable_root(&self) -> Option<&std::path::Path> { Some(&self.root) }
```

Reuse any shared disk-walk helper `GitScreenRepoSource` already uses; if the walk logic is duplicated, extract a private free function `fn walk_screen_paths(root: &Path) -> Vec<String>` and call it from both. `load` returns `Err(String)` on missing/invalid `byonk-screens.yaml`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib local_source_is_writable_and_reads_disk`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/screen_repo_loader.rs Cargo.toml Cargo.lock
git commit -m "feat: LocalScreenRepoSource — writable on-disk screen source"
```

---

### Task 4: Config `path:` variant + validation

**Files:**
- Modify: `src/models/config.rs` (`ScreenRepoRef`, add `path`; add mutual-exclusion validation)
- Test: `src/models/config.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `ScreenRepoRef { repo: Option<String>, path: Option<String>, pin: Option<String>, token: Option<String> }`; a validation error when both `repo` and `path` are set on one entry, or when a user entry uses the reserved `byonk-builtin` handle with a `path`/`repo` conflict.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn screen_repo_path_variant_parses() {
    let yaml = "screen_repos:\n  local:\n    path: /config/screens\n";
    let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.screen_repos["local"].path.as_deref(), Some("/config/screens"));
}

#[test]
fn screen_repo_rejects_repo_and_path_together() {
    let r = ScreenRepoRef {
        repo: Some("github.com/a/b".into()),
        path: Some("/x".into()),
        pin: None, token: None,
    };
    assert!(r.validate("weather").is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib screen_repo_path_variant_parses screen_repo_rejects_repo_and_path_together`
Expected: FAIL — no `path` field / no `validate` method.

- [ ] **Step 3: Add the field and validation**

Add `#[serde(default)] pub path: Option<String>,` to `ScreenRepoRef` (after `repo`). Add:

```rust
impl ScreenRepoRef {
    /// Reject nonsensical combinations. `handle` is only for the error message.
    pub fn validate(&self, handle: &str) -> Result<(), String> {
        if self.repo.is_some() && self.path.is_some() {
            return Err(format!(
                "screen repo '{handle}': set either 'repo' or 'path', not both"
            ));
        }
        Ok(())
    }
}
```

Call `validate` for every entry wherever `AppConfig` is validated on load (find the existing config-validation site; if none, add a loop in the loader that returns the first error). Update the two existing `ScreenRepoRef { … }` struct literals in this file's tests to include `path: None`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib screen_repo`
Expected: PASS (new + existing screen_repo tests).

- [ ] **Step 5: Commit**

```bash
git add src/models/config.rs
git commit -m "feat: screen_repos 'path:' variant (writable local repo) + mutual-exclusion validation"
```

---

### Task 5: Register local repos in the loader/manager

**Files:**
- Modify: `src/services/screen_repo_loader.rs` (`ScreenRepoLoader::new` — accept typed disk sources)
- Modify: `src/services/screen_repo_manager.rs` (`new`, `rebuild_loader` — build local sources from `path:` entries + auto-register `SCREENS_DIR` as `local`)
- Modify: `src/main.rs` (`run_server`, `run_dev_server` — pass `SCREENS_DIR` for auto-registration)
- Test: `src/services/screen_repo_manager.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `LocalScreenRepoSource` (Task 3), `GitScreenRepoSource` (Task 2), `ScreenRepoRef.path` (Task 4).
- Produces: a loader whose registry contains, per handle, the correct source **kind**; `path:` entries and the auto-registered `SCREENS_DIR` resolve to `LocalScreenRepoSource` (writable); `repo:` caches to `GitScreenRepoSource`; `byonk-builtin` embedded.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn scree_dir_auto_registers_as_writable_local_handle() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("byonk-screens.yaml"),
        "name: local\ndescription: d\nauthor: a\nlicense: MIT\n").unwrap();
    let mgr = test_manager_with_screens_dir(dir.path()); // helper defined in Step 3
    let loader = mgr.loader();
    let src = loader.source_for("local").expect("local handle present");
    assert!(src.writable_root().is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib scree_dir_auto_registers_as_writable_local_handle`
Expected: FAIL — helper / `source_for` / `local` handle missing.

- [ ] **Step 3: Thread typed local sources through**

- Add `ScreenRepoLoader::source_for(&self, handle: &str) -> Option<Arc<dyn ScreenRepoSource>>` (a registry `get().cloned()`), for tests and later `ScreenStore`.
- Change `ScreenRepoManager::new` to accept the `SCREENS_DIR` path (an added `screens_dir: Option<PathBuf>` arg) and, when set and config has no explicit `local` entry, register it as a `LocalScreenRepoSource` under handle `local`.
- In `rebuild_loader`, iterate `config.screen_repos`: for entries with `path: Some(p)`, build a `LocalScreenRepoSource::load(p)` (skip with a `warn!` on error) and insert under the handle — **do not** `continue` past them (today's code skips `repo`-less entries; that skip must now only apply to entries with neither `repo` nor `path`). Keep the existing git-cache logic for `repo:` entries.
- `ScreenRepoLoader::new` signature stays `(asset_loader, disk_packages)` but callers now pass a map that includes local roots; if a kind distinction is needed at insert time, pass `HashMap<String, DiskSource>` where `enum DiskSource { Git(PathBuf), Local(PathBuf) }` and construct the right source. Add `test_manager_with_screens_dir` test helper in the `#[cfg(test)]` module.
- Update `main.rs` `ScreenRepoManager::new(...)` call sites (2) to pass `screens_dir.clone()`.

- [ ] **Step 4: Run test + full check**

Run: `cargo test --lib scree_dir_auto_registers_as_writable_local_handle && make check`
Expected: PASS + green.

- [ ] **Step 5: Commit**

```bash
git add src/services/screen_repo_loader.rs src/services/screen_repo_manager.rs src/main.rs
git commit -m "feat: register 'path:' + SCREENS_DIR local repos as writable sources in the loader"
```

---

### Task 6: `ScreenStore` — read/write/etag with path safety

**Files:**
- Create: `src/services/screen_store.rs`
- Modify: `src/services/mod.rs` (export `ScreenStore`)
- Modify: `Cargo.toml` (add `blake3` dependency)
- Test: `src/services/screen_store.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `Arc<ScreenRepoManager>` (for `loader()`/`rebuild_loader()`), `LocalScreenRepoSource::writable_root` (Task 5).
- Produces:
  - `ScreenStore::new(manager: Arc<ScreenRepoManager>, pipeline: Arc<ContentPipeline>) -> ScreenStore`
  - `read_file(&self, screen_ref: &str, file: &str) -> Result<FileContents, StoreError>` where `FileContents { bytes: Vec<u8>, etag: String, binary: bool }`
  - `write_file(&self, screen_ref: &str, file: &str, bytes: &[u8], if_match: Option<&str>) -> Result<String, StoreError>` (returns new etag)
  - `enum StoreError { ReadOnly{copy_hint:String}, NotFound, Conflict, Traversal, TooLarge, Io(String) }`
  - `fn etag(bytes: &[u8]) -> String` (blake3 hex)

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn write_rejects_read_only_handle() {
    let store = test_store_with_local(); // helper: a manager whose 'local' is writable, builtin is not
    let err = store.write_file("byonk-builtin/default", "script.lua", b"x", None).unwrap_err();
    assert!(matches!(err, StoreError::ReadOnly { .. }));
}

#[test]
fn write_then_read_roundtrips_with_etag() {
    let store = test_store_with_local();
    store.write_file("local/clock", "script.lua", b"return {}", None).unwrap();
    let f = store.read_file("local/clock", "script.lua").unwrap();
    assert_eq!(f.bytes, b"return {}");
    let e = f.etag.clone();
    // stale write is rejected
    let err = store.write_file("local/clock", "script.lua", b"new", Some("deadbeef")).unwrap_err();
    assert!(matches!(err, StoreError::Conflict));
    // matching etag succeeds
    store.write_file("local/clock", "script.lua", b"new", Some(&e)).unwrap();
}

#[test]
fn write_rejects_path_traversal() {
    let store = test_store_with_local();
    let err = store.write_file("local/clock", "../../etc/passwd", b"x", None).unwrap_err();
    assert!(matches!(err, StoreError::Traversal));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib screen_store`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement `ScreenStore` read/write**

Create `src/services/screen_store.rs`:

```rust
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use crate::services::screen_repo_manager::ScreenRepoManager;

const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;

pub struct FileContents { pub bytes: Vec<u8>, pub etag: String, pub binary: bool }

#[derive(Debug)]
pub enum StoreError {
    ReadOnly { copy_hint: String },
    NotFound,
    Conflict,
    Traversal,
    TooLarge,
    Io(String),
}

pub struct ScreenStore {
    manager: Arc<ScreenRepoManager>,
    pipeline: Arc<crate::services::ContentPipeline>,
}

pub fn etag(bytes: &[u8]) -> String { blake3::hash(bytes).to_hex().to_string() }

/// Reject `..`, absolute, and empty components. Returns a clean relative PathBuf.
fn safe_rel(rel: &str) -> Result<PathBuf, StoreError> {
    let p = Path::new(rel);
    if p.is_absolute() { return Err(StoreError::Traversal); }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => out.push(s),
            _ => return Err(StoreError::Traversal), // ParentDir/RootDir/CurDir/Prefix
        }
    }
    if out.as_os_str().is_empty() { return Err(StoreError::Traversal); }
    Ok(out)
}

impl ScreenStore {
    pub fn new(manager: Arc<ScreenRepoManager>, pipeline: Arc<crate::services::ContentPipeline>) -> Self {
        Self { manager, pipeline }
    }

    fn split_ref(screen_ref: &str) -> Result<(&str, &str), StoreError> {
        screen_ref.split_once('/').ok_or(StoreError::NotFound)
    }

    /// Resolve the writable root for a screen_ref's handle, or a ReadOnly error.
    fn writable_dir(&self, handle: &str, screen_path: &str) -> Result<PathBuf, StoreError> {
        let loader = self.manager.loader();
        let src = loader.source_for(handle).ok_or(StoreError::NotFound)?;
        let root = src.writable_root().ok_or_else(|| StoreError::ReadOnly {
            copy_hint: format!(
                "'{handle}' is read-only; use copy_screen to fork '{handle}/{screen_path}' into a writable repo (e.g. 'local')"
            ),
        })?;
        Ok(root.join(screen_path))
    }

    pub fn read_file(&self, screen_ref: &str, file: &str) -> Result<FileContents, StoreError> {
        let (handle, screen_path) = Self::split_ref(screen_ref)?;
        let rel = safe_rel(file)?;
        let loader = self.manager.loader();
        let src = loader.source_for(handle).ok_or(StoreError::NotFound)?;
        let full_rel = format!("{screen_path}/{}", rel.to_string_lossy());
        let bytes = src.read(&full_rel).ok_or(StoreError::NotFound)?;
        if bytes.len() > MAX_FILE_BYTES { return Err(StoreError::TooLarge); }
        let binary = std::str::from_utf8(&bytes).is_err();
        let etag = etag(&bytes);
        Ok(FileContents { bytes, etag, binary })
    }

    pub fn write_file(&self, screen_ref: &str, file: &str, bytes: &[u8], if_match: Option<&str>)
        -> Result<String, StoreError>
    {
        if bytes.len() > MAX_FILE_BYTES { return Err(StoreError::TooLarge); }
        let (handle, screen_path) = Self::split_ref(screen_ref)?;
        let rel = safe_rel(file)?;
        let dir = self.writable_dir(handle, screen_path)?;
        let target = dir.join(&rel);

        // canonicalize-then-verify-prefix guard (defends against symlink escape)
        let base = self.writable_dir(handle, "")?; // repo root
        if let Ok(canon_base) = base.canonicalize() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
                let canon_parent = parent.canonicalize().map_err(|e| StoreError::Io(e.to_string()))?;
                if !canon_parent.starts_with(&canon_base) { return Err(StoreError::Traversal); }
            }
        } else {
            std::fs::create_dir_all(target.parent().unwrap())
                .map_err(|e| StoreError::Io(e.to_string()))?;
        }

        if let Some(expected) = if_match {
            match std::fs::read(&target) {
                Ok(cur) if etag(&cur) != expected => return Err(StoreError::Conflict),
                _ => {}
            }
        }

        // atomic tmp+rename in the same dir
        let tmp = target.with_extension("byonk-tmp");
        std::fs::write(&tmp, bytes).map_err(|e| StoreError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &target).map_err(|e| StoreError::Io(e.to_string()))?;
        self.manager.rebuild_loader();
        Ok(etag(bytes))
    }
}
```

Add `blake3 = "1"` to `[dependencies]` in `Cargo.toml`. Add a `test_store_with_local()` helper in the `#[cfg(test)]` module that builds a manager with a temp `local` repo (reuse Task 5's helper) and a real `ContentPipeline`. Export `pub mod screen_store;` and `pub use screen_store::ScreenStore;` in `src/services/mod.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib screen_store`
Expected: PASS (all three).

- [ ] **Step 5: Commit**

```bash
git add src/services/screen_store.rs src/services/mod.rs Cargo.toml Cargo.lock
git commit -m "feat: ScreenStore read/write with etag concurrency + path-traversal safety"
```

---

### Task 7: `ScreenStore` — create / copy / rename / delete

**Files:**
- Modify: `src/services/screen_store.rs`
- Modify: `src/assets.rs` (embed a starter template dir, or add a `STARTER_*` const) — see Step 3
- Test: `src/services/screen_store.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: Task 6's `ScreenStore`, `writable_dir`, `safe_rel`.
- Produces:
  - `create_screen(&self, handle, name, template: StarterKind) -> Result<String, StoreError>` (returns `handle/name` ref)
  - `copy_screen(&self, from_ref, to_handle, to_name) -> Result<String, StoreError>`
  - `rename_screen(&self, screen_ref, new_name) -> Result<String, StoreError>`
  - `delete_screen(&self, screen_ref) -> Result<(), StoreError>`
  - `enum StarterKind { Minimal }` (extensible later)

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn create_scaffolds_three_files_extending_base() {
    let store = test_store_with_local();
    let r = store.create_screen("local", "clock", StarterKind::Minimal).unwrap();
    assert_eq!(r, "local/clock");
    let svg = store.read_file("local/clock", "screen.svg").unwrap();
    let svg_s = String::from_utf8(svg.bytes).unwrap();
    assert!(svg_s.contains("byonk-base-v1/base.svg"), "starter must extend the base library");
    store.read_file("local/clock", "meta.yaml").unwrap();
    store.read_file("local/clock", "script.lua").unwrap();
}

#[test]
fn copy_forks_read_only_screen_into_local() {
    let store = test_store_with_local();
    // byonk-builtin/default exists (embedded); copy it into local
    let r = store.copy_screen("byonk-builtin/default", "local", "my-default").unwrap();
    assert_eq!(r, "local/my-default");
    store.read_file("local/my-default", "meta.yaml").unwrap();
}

#[test]
fn rename_and_delete_roundtrip() {
    let store = test_store_with_local();
    store.create_screen("local", "a", StarterKind::Minimal).unwrap();
    store.rename_screen("local/a", "b").unwrap();
    assert!(store.read_file("local/a", "meta.yaml").is_err());
    store.read_file("local/b", "meta.yaml").unwrap();
    store.delete_screen("local/b").unwrap();
    assert!(store.read_file("local/b", "meta.yaml").is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib screen_store`
Expected: FAIL — methods missing.

- [ ] **Step 3: Implement the structural operations**

Add the starter as three embedded string consts in `screen_store.rs` (simplest; no new embed root):

```rust
pub enum StarterKind { Minimal }

const STARTER_META: &str = "title: New Screen\ndescription: A new screen.\nbyonk: \"0.15\"\nrefresh: 300\n";
const STARTER_LUA: &str = "-- Return the data table for your screen.\nreturn { data = { message = \"Hello\" } }\n";
const STARTER_SVG: &str = concat!(
    "{% extends \"byonk-base-v1/base.svg\" %}\n",
    "{% block content %}\n",
    "  <text x=\"40\" y=\"80\" font-size=\"48\">{{ data.message }}</text>\n",
    "{% endblock %}\n",
);
```

Implement, all writing through the writable-dir guard (reuse `write_file`'s directory logic — refactor its atomic-write + guard into a private `fn put(&self, dir: &Path, base: &Path, rel: &Path, bytes: &[u8])` and call it from both `write_file` and here):

- `create_screen`: reject if the target dir already exists (`StoreError::Conflict`); write the three starter files; `rebuild_loader()`.
- `copy_screen`: read every file of the source screen (enumerate via the source's `screen_paths`/sibling reads — read `meta.yaml`, `script.lua`, `screen.svg`, and any files under the screen dir that the source lists) and write them into the destination writable dir; `rebuild_loader()`.
- `rename_screen`: `std::fs::rename` the screen dir within the same writable root (reject if destination exists); `rebuild_loader()`.
- `delete_screen`: `std::fs::remove_dir_all` the screen dir under the writable root only; `rebuild_loader()`.

For `copy_screen`'s file enumeration, add a helper on the source or walk the source dir; for embedded sources use `source.screen_paths()` + known filenames plus any `svg_files()` under the screen path.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib screen_store && make check`
Expected: PASS + green.

- [ ] **Step 5: Commit**

```bash
git add src/services/screen_store.rs
git commit -m "feat: ScreenStore create/copy/rename/delete screens"
```

---

### Task 8: `ScreenStore` — validate

**Files:**
- Modify: `src/services/screen_store.rs`
- Test: `src/services/screen_store.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `ScreenMeta::from_yaml` (`src/models/screen_meta.rs`), mlua (compile-only), `TemplateService` resolution (`src/services/template_service.rs`).
- Produces:
  - `validate(&self, screen_ref) -> ValidationReport`
  - `struct ValidationReport { ok: bool, issues: Vec<Issue> }`, `struct Issue { severity: Severity, location: String, message: String }`, `enum Severity { Error, Warning }`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn validate_flags_lua_syntax_error() {
    let store = test_store_with_local();
    store.create_screen("local", "bad", StarterKind::Minimal).unwrap();
    store.write_file("local/bad", "script.lua", b"return {", None).unwrap(); // unbalanced
    let rep = store.validate("local/bad");
    assert!(!rep.ok);
    assert!(rep.issues.iter().any(|i| i.location.contains("script.lua")));
}

#[test]
fn validate_passes_for_starter() {
    let store = test_store_with_local();
    store.create_screen("local", "ok", StarterKind::Minimal).unwrap();
    assert!(store.validate("local/ok").ok);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib validate_`
Expected: FAIL — `validate` missing.

- [ ] **Step 3: Implement `validate`**

```rust
pub enum Severity { Error, Warning }
pub struct Issue { pub severity: Severity, pub location: String, pub message: String }
pub struct ValidationReport { pub ok: bool, pub issues: Vec<Issue> }
```

Logic:
- Read `meta.yaml`; `ScreenMeta::from_yaml` — on `Err`, push an Error issue at `meta.yaml`.
- Read `script.lua`; compile without executing: `mlua::Lua::new().load(src).into_function()` — on `Err`, push an Error at `script.lua` with the mlua message (includes line).
- Read `screen.svg`; parse as XML (reuse whatever `template_service` uses; a `roxmltree::Document::parse` or a Tera one-off compile). Attempt to resolve `{% extends/include %}` targets against the base library + the screen's own repo — reuse `TemplateService` construction for this screen and catch registration errors as Error issues at `screen.svg`.
- `ok = issues has no Error`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib validate_`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/screen_store.rs
git commit -m "feat: ScreenStore validate (meta schema + Lua compile + SVG/include resolution)"
```

---

### Task 9: `ScreenStore` — render with diagnostics

**Files:**
- Modify: `src/services/screen_store.rs`
- Modify: `src/services/lua_runtime.rs` (capture `log_*` per-run) — see Step 3
- Test: `src/services/screen_store.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `ContentPipeline` render path (`run_script_direct`, `render_svg_from_script`, `render_png_from_svg` in `content_pipeline.rs`).
- Produces:
  - `render(&self, screen_ref, opts: RenderOpts) -> RenderResult`
  - `struct RenderOpts { model: String, width: Option<u32>, height: Option<u32>, panel: Option<String>, dither: Option<String>, error_clamp: Option<f32>, chroma_clamp: Option<f32>, noise_scale: Option<f32>, preserve_exact: Option<bool>, timestamp: Option<i64>, include_raw: bool }` (with `Default`)
  - `struct RenderResult { png: Vec<u8>, raw_png: Option<Vec<u8>>, log: Vec<String>, data: serde_json::Value, refresh_rate: u32, error: Option<RenderError> }`
  - `struct RenderError { line: Option<u32>, message: String }`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn render_returns_png_data_and_log() {
    let store = test_store_with_local();
    store.create_screen("local", "r", StarterKind::Minimal).unwrap();
    store.write_file("local/r", "script.lua",
        b"log_info(\"hi\")\nreturn { data = { message = \"X\" } }", None).unwrap();
    let res = store.render("local/r", RenderOpts::default());
    assert!(res.error.is_none(), "{:?}", res.error);
    assert!(!res.png.is_empty());
    assert_eq!(res.data["message"], "X");
    assert!(res.log.iter().any(|l| l.contains("hi")));
}

#[test]
fn render_reports_lua_error_with_message() {
    let store = test_store_with_local();
    store.create_screen("local", "e", StarterKind::Minimal).unwrap();
    store.write_file("local/e", "script.lua", b"error(\"boom\")", None).unwrap();
    let res = store.render("local/e", RenderOpts::default());
    assert!(res.error.as_ref().unwrap().message.contains("boom"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib render_returns_png_data_and_log render_reports_lua_error_with_message`
Expected: FAIL — `render`/`RenderOpts` missing.

- [ ] **Step 3: Capture Lua logs + implement render**

In `lua_runtime.rs`, make the `log_info/log_warn/log_error` hooks (lines ~869–885) append to a shared `Arc<Mutex<Vec<String>>>` sink that `run_script` exposes on `ScriptResult` (add a `logs: Vec<String>` field to the runtime's `ScriptResult`), in addition to their current tracing calls. Thread the sink through `run_script`.

In `screen_store.rs`, implement `render` by orchestrating the existing pipeline: resolve the screen, run the script (capturing `logs`, `data`, `refresh_rate`, and any `ScriptError` → `RenderError { line: parse from mlua msg, message }`), render SVG, dither to PNG, and (if `opts.include_raw`) also produce the pre-dither PNG. Reuse `ContentPipeline`'s existing render helpers; do not duplicate dithering. Map `RenderOpts` onto the same knobs `/dev/render` passes today.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib render_ && make check`
Expected: PASS + green.

- [ ] **Step 5: Commit**

```bash
git add src/services/screen_store.rs src/services/lua_runtime.rs
git commit -m "feat: ScreenStore render with diagnostics (png + raw + log + data + lua error line)"
```

---

### Task 10: Split embedded screens into builtin + examples

**Files:**
- Move: `screens/` reorganized into `screens/builtin/` and `screens/examples/`
- Modify: `src/assets.rs` (embed roots) and `src/services/screen_repo_loader.rs` (`EmbeddedBuiltinSource` reads the builtin subset)
- Modify: any path references to moved screens (grep in Step 1)
- Test: `src/services/screen_repo_loader.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `byonk-builtin` embeds only `default` + `calibration/color` + `calibration/grey`. A new embedded `EmbeddedExamples` (rust-embed of `screens/examples/`) holding `hello`, `mandelbrot`, `webscrape`, `gphoto`, `swiss-departure-board`, `demo/font`, plus an `examples` `byonk-screens.yaml`.

- [ ] **Step 1: Inventory references before moving**

Run: `grep -rn "screens/example\|screens/useful\|screens/demo\|screens/calibration\|screens/default\|byonk-builtin/" src/ docs/ tests/ config*.yaml default-config.yaml`
Expected: a list of references to update. Note the hard-reference `byonk-builtin/default` in `content_pipeline.rs` — that path is preserved (default stays builtin).

- [ ] **Step 2: Reorganize the tree**

```bash
mkdir -p screens/builtin screens/examples
git mv screens/default screens/builtin/default
git mv screens/calibration screens/builtin/calibration
git mv screens/example/hello screens/examples/hello
git mv screens/example/mandelbrot screens/examples/mandelbrot
git mv screens/example/webscrape screens/examples/webscrape
git mv screens/useful/gphoto screens/examples/gphoto
git mv screens/useful/swiss-departure-board screens/examples/swiss-departure-board
git mv screens/demo screens/examples/demo
```
Write `screens/builtin/byonk-screens.yaml` (`name: byonk-builtin`, description/author/license from the current top-level `screens/byonk-screens.yaml`) and `screens/examples/byonk-screens.yaml` (`name: examples`). Remove the now-stale top-level `screens/byonk-screens.yaml` if the embed root moves (see Step 3).

- [ ] **Step 3: Point the embed roots at the subdirs**

In `src/assets.rs`: change `EmbeddedScreens` `#[folder = "screens/"]` → `#[folder = "screens/builtin/"]`, and add:

```rust
#[derive(RustEmbed)]
#[folder = "screens/examples/"]
struct EmbeddedExamples;
```
Add `AssetLoader` accessors mirroring the screens ones for examples (`list_examples`, `read_example`). Ensure `EmbeddedBuiltinSource::load` still finds `byonk-screens.yaml` + screen dirs under the new root (it reads via `EmbeddedScreens`, which now points at `screens/builtin/`). Update every reference found in Step 1 (docs/tests/config) so screen refs resolve: builtins stay `byonk-builtin/default`, `byonk-builtin/calibration/*`; examples will resolve under the seeded `examples` handle (Task 11), so update example refs in configs/docs to `examples/hello` etc.

- [ ] **Step 4: Test + full check**

```rust
#[test]
fn builtin_embeds_only_default_and_calibration() {
    let loader = std::sync::Arc::new(AssetLoader::new(None, None, None));
    let src = EmbeddedBuiltinSource::load(loader).unwrap();
    let paths = src.screen_paths();
    assert!(paths.iter().any(|p| p == "default"));
    assert!(paths.iter().any(|p| p.starts_with("calibration/")));
    assert!(!paths.iter().any(|p| p == "hello" || p == "mandelbrot"), "examples must not be builtin");
}
```
Run: `cargo test --lib builtin_embeds_only_default_and_calibration && make check`
Expected: PASS + green.

- [ ] **Step 5: Commit**

```bash
git add -A screens/  # ONLY the screens/ subtree (git mv already staged); verify with: git status --short screens/
git add src/assets.rs src/services/screen_repo_loader.rs
# add updated docs/tests/config by explicit path from Step 1
git status  # CONFIRM no unrelated untracked files are staged (memory: no-git-add-all)
git commit -m "feat: split embedded screens into minimal byonk-builtin + shipped examples"
```
(Note: `git add -A screens/` is scoped to the reorganized subtree and is safe because `git mv` renames are already tracked; still verify `git status` before committing.)

---

### Task 11: Seed examples + local manifest; retire screen-copy seeding

**Files:**
- Modify: `src/assets.rs` (`seed_if_configured` — stop copying builtin screens; seed `local` manifest + examples repo)
- Modify: `src/main.rs` (wire an examples path; declare default `examples` repo)
- Test: `src/assets.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `EmbeddedExamples` (Task 10), `path:` config (Task 4).
- Produces: on first run, an empty `SCREENS_DIR` gets only a `byonk-screens.yaml` (`name: local`); an `examples` directory (default `<SCREENS_DIR>/../examples`, or an env/config-set path) gets the full examples set + its manifest, once.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn seeds_local_manifest_not_screen_copies() {
    let dir = tempfile::tempdir().unwrap();
    let loader = AssetLoader::new(Some(dir.path().into()), None, None);
    loader.seed_if_configured().unwrap();
    assert!(dir.path().join("byonk-screens.yaml").exists());
    // no builtin screen copies:
    assert!(!dir.path().join("default").exists());
    assert!(!dir.path().join("calibration").exists());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib seeds_local_manifest_not_screen_copies`
Expected: FAIL — current code copies screens / writes no manifest.

- [ ] **Step 3: Rewrite the screens seeding branch**

In `seed_if_configured`, replace the "Seed screens" block: into an empty `screens_dir`, write only a `byonk-screens.yaml` with `name: local`. Add an examples-seeding branch: given an examples dir (new `examples_dir: Option<PathBuf>` on `AssetLoader`, defaulted in `main.rs` to `<SCREENS_DIR>/../examples`), when empty, write every `EmbeddedExamples` file plus an `examples` `byonk-screens.yaml`. Fonts/config seeding unchanged. In `main.rs`, register the examples dir as a default `screen_repos.examples.path` entry if not already present in config.

- [ ] **Step 4: Run test + check**

Run: `cargo test --lib seeds_local_manifest_not_screen_copies && make check`
Expected: PASS + green.

- [ ] **Step 5: Commit**

```bash
git add src/assets.rs src/main.rs
git commit -m "feat: seed 'local' manifest + shipped 'examples' repo; stop copying builtin screens"
```

---

### Task 12: One-time migration of pre-existing installs

**Files:**
- Create: `src/services/screen_migration.rs`
- Modify: `src/services/mod.rs` (export), `src/main.rs` (`run_server` — call migration after asset load, before serving)
- Modify: `src/services/config_writer.rs` (reuse device-ref rewrite if a helper exists; else add one)
- Test: `src/services/screen_migration.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `AssetLoader` (SCREENS_DIR path), `config_writer` device-ref rewriting.
- Produces: `migrate_builtin_overlay_to_local(screens_dir: &Path, config_path: Option<&Path>) -> MigrationReport` — idempotent; rewrites `SCREENS_DIR/byonk-screens.yaml` `name: byonk-builtin` → `local`, and device refs `byonk-builtin/<x>` → `local/<x>` only where `<x>` exists as a screen in `SCREENS_DIR`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn migration_rewrites_manifest_and_user_screen_refs_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("byonk-screens.yaml"), "name: byonk-builtin\n").unwrap();
    std::fs::create_dir_all(dir.path().join("myclock")).unwrap();
    std::fs::write(dir.path().join("myclock/meta.yaml"), "title: C\ndescription: d\nbyonk: \"0.15\"\n").unwrap();
    let cfg = dir.path().join("config.yaml");
    std::fs::write(&cfg,
        "devices:\n  AA:BB:\n    screen: byonk-builtin/myclock\n  CC:DD:\n    screen: byonk-builtin/default\n").unwrap();
    let rep = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
    let manifest = std::fs::read_to_string(dir.path().join("byonk-screens.yaml")).unwrap();
    assert!(manifest.contains("name: local"));
    let out = std::fs::read_to_string(&cfg).unwrap();
    assert!(out.contains("local/myclock"), "user screen ref migrated");
    assert!(out.contains("byonk-builtin/default"), "genuine builtin ref untouched");
    assert!(rep.refs_rewritten == 1);
    // idempotent
    let rep2 = migrate_builtin_overlay_to_local(dir.path(), Some(&cfg));
    assert_eq!(rep2.refs_rewritten, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib migration_rewrites`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement the migration**

Create `screen_migration.rs`:
- If `SCREENS_DIR/byonk-screens.yaml` doesn't exist or already says `name: local`, and no matching refs remain, return an empty report (idempotent).
- Rewrite the manifest `name:` to `local` if it says `byonk-builtin`.
- Enumerate screen dir names present in `SCREENS_DIR` (dirs with `meta.yaml`). For each device `screen: byonk-builtin/<x>` in the config where `<x>` (the path after the handle) matches a present screen dir, rewrite to `local/<x>` via `config_writer` (reuse an existing rewrite helper if present; else edit the YAML string in place preserving formatting). `default` and `calibration/*` are not present in `SCREENS_DIR` (they're embedded), so their refs are left alone.
- Return `MigrationReport { manifest_rewritten: bool, refs_rewritten: usize }`; log an INFO summary.

Call `migrate_builtin_overlay_to_local` once in `run_server` after `seed_if_configured` and before building state; log the report.

- [ ] **Step 4: Run test + check**

Run: `cargo test --lib migration && make check`
Expected: PASS + green.

- [ ] **Step 5: Commit**

```bash
git add src/services/screen_migration.rs src/services/mod.rs src/main.rs src/services/config_writer.rs
git commit -m "feat: one-time migration of byonk-builtin overlay screens to 'local' handle"
```

---

### Task 13: Wire `ScreenStore` into `AppState`; docs + CHANGES

**Files:**
- Modify: `src/server.rs` (add `screen_store: Arc<ScreenStore>` to `AppState`; construct it)
- Modify: `docs/src/guide/configuration.md`, `docs/src/guide/` screen-repos page, `docs/src/SUMMARY.md` (new authoring page stub)
- Create: `docs/src/guide/authoring.md` (screen-source model: builtin vs examples vs local; `path:` config; fork-to-edit)
- Modify: `CHANGES.md` (Unreleased)

**Interfaces:**
- Consumes: `ScreenStore::new` (Task 6–9).
- Produces: `AppState.screen_store` available for the MCP plan (spec 1 Component 5) and the UI plan to consume.

- [ ] **Step 1: Add `screen_store` to `AppState`**

Add the field to the `AppState` struct in `server.rs`; construct `ScreenStore::new(screen_repo_manager.clone(), content_pipeline.clone())` in `create_app_state_with_config` (and the `_with_overrides` variant). No route yet — it's consumed by later plans.

- [ ] **Step 2: Build check**

Run: `make check`
Expected: PASS (nothing consumes the field yet; add `#[allow(dead_code)]` on the field if clippy complains, with a comment that the MCP/UI plans consume it).

- [ ] **Step 3: Write docs**

Add `docs/src/guide/authoring.md` explaining the three source layers (base library / builtin / examples), writable local repos, the `path:` config variant, and the fork-to-edit flow (`copy_screen`). Update the screen-repos + configuration pages for `path:`. Add the page to `SUMMARY.md`. Do **not** remove `/dev` docs (spec 2 handles that).

- [ ] **Step 4: Update CHANGES.md**

Add to the Unreleased section (user-facing only):
```
### Added
- Writable local screen repositories via `screen_repos: { <name>: { path: … } }`, so your own screens live in their own handle (`local`) instead of shadowing the built-ins.
- Shipped example screens (hello, mandelbrot, webscrape, gphoto, swiss-departure-board, font demo) now install as an editable `examples` repository.

### Changed
- Built-in screens are now a minimal, read-only set (default + calibration); your own screens are no longer mixed into the `byonk-builtin` handle. Existing installs are migrated automatically on first start.
```

- [ ] **Step 5: Verify docs build + commit**

Run: `make docs`
Expected: clean (only the known harmless mermaid version warning).
```bash
git add src/server.rs docs/src/guide/authoring.md docs/src/guide/configuration.md docs/src/SUMMARY.md CHANGES.md
# add the screen-repos guide page by its explicit path
git commit -m "feat: wire ScreenStore into AppState; document the screen-source model"
```

---

## Self-Review

**Spec coverage** (spec Components 1–4):
- Component 1 (axum bump) → Task 1. ✓
- Component 2 (typed sources + `path:` config) → Tasks 2–5. ✓
- Component 3 (`ScreenStore` read/write/validate/render) → Tasks 6–9. ✓
- Component 4 (builtin/examples split + seeding + migration) → Tasks 10–12. ✓
- Wiring + docs + CHANGES → Task 13. ✓
- Component 5 (MCP) → **out of scope** (separate plan), as agreed.

**Type consistency:** `writable_root()` (Tasks 2–6), `source_for()` (Tasks 5–6), `StoreError`/`FileContents`/`etag` (Tasks 6–9), `StarterKind::Minimal` (Tasks 7–9), `RenderOpts`/`RenderResult`/`RenderError` (Task 9), `migrate_builtin_overlay_to_local` + `MigrationReport` (Task 12) — names used consistently across tasks.

**Placeholder scan:** No TBD/TODO; every code step shows real code. Two spots intentionally say "reuse the existing helper if present, else add one" (config validation site in Task 4, device-ref rewrite in Task 12) — these are conditional on codebase inspection the implementer must do; both give the fallback explicitly.

**Notes for the executor:**
- The examples-dir default location (`<SCREENS_DIR>/../examples`) and whether the add-on sets it explicitly is the one spec-flagged open question; resolve against the add-on `/config` layout when doing Task 11, and confirm the add-on options schema gains the `path:` shape (memory `ha-vm-addon-manifest-sync-gap` for the VM manifest-sync gotcha).
- After the full plan, run the HA add-on options-schema check on the VM before claiming add-on parity.
