# Screen Packages — Plan 2: Distribution — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make registered packages fetchable from git repos — clone/cache/refresh a repo at a pin, serve screens from the cached checkout, and control it all over the admin API — building on Plan 1's `PackageLoader`/registry.

**Architecture:** A new `PackageManager` service owns the package **cache** (keyed by repo+resolved-sha), a per-handle **status store**, and an `Arc<ArcSwap<PackageLoader>>` it rebuilds and hot-swaps after fetches/registry changes. Git work goes through a `git_fetch` module (gix; git2 fallback gated by Task 1) run in `spawn_blocking`. The admin API gains package register/patch/delete/update endpoints and enriched `GET /packages` status; a tokio interval task periodically re-fetches mutable pins. `byonk-builtin` stays embedded and is never fetched.

**Tech Stack:** Rust, tokio (`spawn_blocking` + `interval`), **gix** (new; git2 fallback), arc-swap, chrono, serde_yaml 0.9, yamlpatch/yamlpath (via `config_writer`).

## Global Constraints

- **Pin semantics** (spec §8.2): a full **commit sha** is immutable — fetched once, cached forever, never re-fetched. A **tag or branch** is mutable — re-fetched on demand and on the periodic interval; **never silently mid-serve** (resolve to a sha, then swap the active checkout atomically at a serve boundary).
- **Cache** (spec §8.3): keyed by **repo + resolved sha**; multiple pins of one repo coexist. **Offline:** a fetch failure never takes down an already-cached screen — serve the last good checkout, status `offline`.
- **Auth** (spec §8.4): default is ambient host git credentials (credential helpers, ssh-agent, netrc); an optional per-package `token` overrides. A package `token` is **never** serialized by any GET — only `token_set: bool`.
- **`byonk-builtin`** is embedded, always registered, never fetched, and cannot be deleted.
- **`PackageInfo`** JSON shape (spec §9a.1): `{ handle, repo, pin, pin_kind, resolved_sha, status, last_fetched, error, token_set, screen_count, builtin }`. `pin_kind ∈ {sha, tag, branch, embedded}`. `status ∈ {ready, fetching, error, offline}`.
- Config struct is **`AppConfig`**; `SharedConfig = Arc<ArcSwap<AppConfig>>`. Admin writes persist via `config_writer` + `persist()` under `state.write_lock`. Never `git add -A`. Build/verify: `make check`.

---

### Task 1: `git_fetch` module — gix spike + pin resolution (git2 fallback gate)

**Files:**
- Modify: `Cargo.toml` (add `gix`)
- Create: `src/services/git_fetch.rs`
- Modify: `src/services/mod.rs` (`pub mod git_fetch;`)
- Test: `src/services/git_fetch.rs` (`#[cfg(test)]`, local-fixture repo)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
  #[serde(rename_all = "snake_case")]
  pub enum PinKind { Sha, Tag, Branch }

  #[derive(Debug, Clone)]
  pub struct FetchOutcome { pub resolved_sha: String, pub pin_kind: PinKind }

  #[derive(Debug, thiserror::Error)]
  pub enum FetchError {
      #[error("git error: {0}")] Git(String),
      #[error("pin `{0}` not found in {1}")] PinNotFound(String, String),
  }

  /// Clone/fetch `repo` at `pin`, materialize a working tree at `dest`
  /// (created fresh), and return the resolved sha + pin kind. `token`, when
  /// present, is used for HTTPS auth; otherwise ambient host git creds apply.
  pub fn fetch(repo: &str, pin: &str, token: Option<&str>, dest: &std::path::Path)
      -> Result<FetchOutcome, FetchError>;

  /// Classify a pin without network access: 40-hex ⇒ Sha, else determined by
  /// fetch (this helper only recognizes an obvious full sha up front).
  pub fn looks_like_sha(pin: &str) -> bool;
  ```

> **gix vs git2 decision gate.** The spec picks gix (pure Rust, lean image) with git2 as the documented fallback. Implement `fetch` with gix. If gix's clone/fetch/checkout or HTTPS/ssh auth proves unworkable within a reasonable spike, STOP and report BLOCKED with the specific gix limitation hit — the controller decides whether to swap this one module to `git2` (the interface above is engine-agnostic, so only this file changes). Do NOT silently ship a half-working fetch.

- [ ] **Step 1: Add the dependency**

Add to `[dependencies]` in `Cargo.toml`:
```toml
gix = { version = "0.66", default-features = false, features = ["blocking-network-client", "worktree-mutation"] }
```
(Pick the current 0.x that builds; enable the feature set needed for blocking clone + checkout + https. If https needs `blocking-http-transport-reqwest` or a tls feature, add it. Confirm `cargo build` succeeds before proceeding.)

- [ ] **Step 2: Write the failing test (local fixture repo — no network)**

Build a real git repo on disk inside the test using gix (init + a commit on `main` + a tag), then clone it via a `file://` / path URL. This keeps the test hermetic.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_sha() {
        assert!(looks_like_sha("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0"));
        assert!(!looks_like_sha("v1.0.0"));
        assert!(!looks_like_sha("main"));
        assert!(!looks_like_sha("a1b2c3d")); // short sha not treated as full sha
    }

    #[test]
    fn test_fetch_branch_from_local_repo() {
        // Arrange: create a source repo with one commit on the default branch and
        // a file `byonk-screens.yaml`; capture its default branch name + head sha.
        let src = make_fixture_repo(); // helper below builds it with gix
        let dest = tempdir_path("byonk_fetch_dest");

        // Act
        let out = fetch(&src.url, &src.branch, None, &dest).expect("fetch branch");

        // Assert: resolved sha matches head, kind is Branch, and the tree is present.
        assert_eq!(out.resolved_sha, src.head_sha);
        assert_eq!(out.pin_kind, PinKind::Branch);
        assert!(dest.join("byonk-screens.yaml").exists());
    }

    #[test]
    fn test_fetch_missing_pin_errors() {
        let src = make_fixture_repo();
        let dest = tempdir_path("byonk_fetch_missing");
        let err = fetch(&src.url, "no-such-ref", None, &dest).unwrap_err();
        assert!(matches!(err, FetchError::PinNotFound(_, _)));
    }
}
```

The implementer writes `make_fixture_repo()` / `tempdir_path()` helpers (init a repo with gix, write a file, commit, read head sha + default branch name). If building a fixture repo purely with gix is impractical, a checked-in tiny bare-repo fixture under `tests/fixtures/` is an acceptable alternative — document which was chosen.

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p byonk git_fetch`
Expected: FAIL (module/functions absent).

- [ ] **Step 4: Implement `looks_like_sha` + `fetch`**

- `looks_like_sha`: `pin.len() == 40 && pin.chars().all(|c| c.is_ascii_hexdigit())`.
- `fetch`: with gix — remove `dest` if it exists, then clone the repo shallowly at/around `pin`, resolve `pin` to a commit sha (try in order: the literal as an oid if `looks_like_sha`; a tag ref `refs/tags/<pin>`; a branch ref `refs/heads/<pin>` / remote-tracking), determine `PinKind` from which resolution matched, and check out the resolved tree into `dest`. Map any gix error to `FetchError::Git`, and an unresolvable pin to `FetchError::PinNotFound(pin, repo)`. Pass `token` as HTTPS auth (username `x-access-token` / token as password is the GitHub convention) when the URL is https and a token is given.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p byonk git_fetch`
Expected: PASS. If a network-only path can't be unit-tested, add an `#[ignore]`d integration test against a known public repo and note it.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/services/git_fetch.rs src/services/mod.rs
git commit -m "feat(dist): git_fetch module (gix) — clone/resolve pin to sha"
```

---

### Task 2: Package cache

**Files:**
- Create: `src/services/package_cache.rs`
- Modify: `src/services/mod.rs` (`pub mod package_cache;`)
- Test: `src/services/package_cache.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `FetchOutcome`/`fetch` (Task 1).
- Produces:
  ```rust
  pub struct PackageCache { root: PathBuf }
  impl PackageCache {
      pub fn new(root: PathBuf) -> Self;
      /// Directory a given repo+sha checkout lives at: root/<repo_hash>/<sha>.
      pub fn checkout_dir(&self, repo: &str, sha: &str) -> PathBuf;
      /// True if that checkout already exists on disk (and has a manifest).
      pub fn has(&self, repo: &str, sha: &str) -> bool;
  }
  /// Stable filesystem-safe key for a repo URL (hex sha256, truncated).
  fn repo_key(repo: &str) -> String;
  ```

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_checkout_dir_is_stable_and_scoped() {
        let c = PackageCache::new(std::path::PathBuf::from("/tmp/byonk-cache"));
        let a = c.checkout_dir("github.com/acme/x", "deadbeef");
        let b = c.checkout_dir("github.com/acme/x", "deadbeef");
        let d = c.checkout_dir("github.com/acme/y", "deadbeef");
        assert_eq!(a, b);            // stable
        assert_ne!(a, d);            // different repo ⇒ different dir
        assert!(a.starts_with("/tmp/byonk-cache"));
        assert!(a.ends_with("deadbeef"));
    }
    #[test]
    fn test_has_false_when_absent() {
        let c = PackageCache::new(std::env::temp_dir().join("byonk-cache-none"));
        assert!(!c.has("github.com/acme/x", "deadbeef"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk package_cache`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
use std::path::PathBuf;
use sha2::{Digest, Sha256};

pub struct PackageCache { root: PathBuf }

fn repo_key(repo: &str) -> String {
    let mut h = Sha256::new();
    h.update(repo.as_bytes());
    hex::encode(&h.finalize()[..8]) // 16 hex chars — enough to avoid collisions
}

impl PackageCache {
    pub fn new(root: PathBuf) -> Self { Self { root } }
    pub fn checkout_dir(&self, repo: &str, sha: &str) -> PathBuf {
        self.root.join(repo_key(repo)).join(sha)
    }
    pub fn has(&self, repo: &str, sha: &str) -> bool {
        self.checkout_dir(repo, sha).join("byonk-screens.yaml").exists()
    }
}
```
(`sha2`/`hex` are already dependencies.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk package_cache`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/package_cache.rs src/services/mod.rs
git commit -m "feat(dist): package cache keyed by repo+sha"
```

---

### Task 3: Package status types

**Files:**
- Create: `src/services/package_status.rs`
- Modify: `src/services/mod.rs` (`pub mod package_status;`)
- Test: `src/services/package_status.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `PinKind` (Task 1).
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
  #[serde(rename_all = "snake_case")]
  pub enum PackageState { Ready, Fetching, Error, Offline }

  #[derive(Debug, Clone, Default)]
  pub struct PackageStatus {
      pub state: Option<PackageState>,      // None until first fetch attempt
      pub resolved_sha: Option<String>,
      pub last_fetched: Option<chrono::DateTime<chrono::Utc>>,
      pub error: Option<String>,
      pub pin_kind: Option<crate::services::git_fetch::PinKind>,
  }
  ```

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_state_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&PackageState::Offline).unwrap(), "\"offline\"");
        assert_eq!(serde_json::to_string(&PackageState::Fetching).unwrap(), "\"fetching\"");
    }
    #[test]
    fn test_default_status_is_empty() {
        let s = PackageStatus::default();
        assert!(s.state.is_none() && s.resolved_sha.is_none() && s.error.is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk package_status`
Expected: FAIL.

- [ ] **Step 3: Implement** the two types exactly as in the Interfaces block.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk package_status`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/package_status.rs src/services/mod.rs
git commit -m "feat(dist): package status types"
```

---

### Task 4: `PackageManager` — orchestration, status, hot-swappable loader

**Files:**
- Create: `src/services/package_manager.rs`
- Modify: `src/services/mod.rs` (`pub mod package_manager;`)
- Modify: `src/server.rs` (`build_package_loader` reused; see Task 5 for AppState wiring)
- Test: `src/services/package_manager.rs` (`#[cfg(test)]`, using on-disk fixture packages — no network)

**Interfaces:**
- Consumes: `PackageCache` (Task 2), `PackageStatus`/`PackageState` (Task 3), `git_fetch::{fetch, FetchError, PinKind}` (Task 1), `PackageLoader::new` + `BUILTIN_HANDLE` (Plan 1), `AppConfig`/`PackageRef`/`SharedConfig`.
- Produces:
  ```rust
  pub struct PackageManager {
      asset_loader: Arc<AssetLoader>,
      config: SharedConfig,                                  // reads config.packages
      cache: PackageCache,
      status: Mutex<HashMap<String, PackageStatus>>,         // std::sync::Mutex (sync)
      loader: ArcSwap<PackageLoader>,                        // the live snapshot
      extra_disk: HashMap<String, PathBuf>,                 // PACKAGES_DIR dev packages
  }
  impl PackageManager {
      pub fn new(asset_loader: Arc<AssetLoader>, config: SharedConfig,
                 cache: PackageCache, extra_disk: HashMap<String, PathBuf>) -> Arc<Self>;
      /// Current loader snapshot (cheap; use per-resolve).
      pub fn loader(&self) -> Arc<PackageLoader>;
      /// Fetch every registered non-builtin package that needs it, then rebuild+swap the loader.
      /// `force` re-fetches mutable pins even if a checkout exists.
      pub fn refresh_all(&self, force: bool);
      /// Fetch one handle (force), update status, rebuild+swap. No-op for builtin/unknown.
      pub fn refresh_one(&self, handle: &str);
      /// Rebuild the loader from the embedded builtin + every ready cache checkout + extra_disk, then swap.
      pub fn rebuild_loader(&self);
      /// Snapshot of per-handle status for GET /packages.
      pub fn status_snapshot(&self) -> HashMap<String, PackageStatus>;
  }
  ```

- [ ] **Step 1: Write the failing test (on-disk fixture package, no git)**

Point `extra_disk` at a temp package dir (reusing the Plan-1 `DiskPackageSource` fixture pattern), and assert the manager builds a loader that resolves it. Fetch/refresh against a real repo is covered by an `#[ignore]`d integration test; the unit test exercises loader rebuild + status snapshot.

```rust
#[test]
fn test_manager_serves_extra_disk_package_and_reports_builtin() {
    // build a temp package dir "acme" with weather/forecast/{meta,script,screen}
    // config with packages: { acme: {} } (repo none — treated as disk-provided here)
    let mgr = PackageManager::new(loader, shared_config, cache, hashmap!{"acme" => tmp});
    mgr.rebuild_loader();
    assert!(mgr.loader().resolve("acme/weather/forecast").is_some());
    assert!(mgr.loader().handles().iter().any(|h| h == "byonk-builtin"));
    // status snapshot has no fetch state for a disk-only package (never fetched)
    assert!(mgr.status_snapshot().get("acme").map(|s| s.state).flatten().is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk package_manager`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `new`: builds an initial loader via `PackageLoader::new(asset_loader.clone(), disk_map)` where `disk_map` starts as `extra_disk` (cache checkouts get added by `rebuild_loader` once fetched); wraps in `ArcSwap`; returns `Arc<Self>`.
- `loader`: `self.loader.load_full()`.
- `rebuild_loader`: compute `disk_map`: start from `extra_disk`; for each registered handle with `repo.is_some()` whose status has a `resolved_sha` and the cache `has(repo, sha)`, add `handle -> cache.checkout_dir(repo, sha)`. Build a fresh `PackageLoader::new(asset_loader, disk_map)` and `self.loader.store(Arc::new(...))`. (byonk-builtin is always added by `PackageLoader::new` itself.)
- `refresh_one(handle)`: read `config.packages[handle]`; skip if builtin or `repo` is None. Set status `Fetching`. Determine target: if `looks_like_sha(pin)` and `cache.has(repo, pin)` → reuse (sha immutable), set Ready with resolved_sha=pin. Else call `git_fetch::fetch(repo, pin, token, cache.checkout_dir(repo, <tmp>))` — but sha isn't known before fetch; fetch into a temp dir, get `resolved_sha`, then move/checkout into `cache.checkout_dir(repo, resolved_sha)` (or fetch directly resolving sha first; implementer picks). On success: status `Ready { resolved_sha, pin_kind, last_fetched=now }` (pass `now` in — see note), then `rebuild_loader()`. On `FetchError`: if a prior `resolved_sha` exists in cache → status `Offline` (keep serving old), else `Error { error }`. Never panic.
- `refresh_all(force)`: iterate registered non-builtin handles, `refresh_one` each (respecting immutable-sha skip unless `force`), then a single `rebuild_loader`.
- **Time:** `chrono::Utc::now()` — this is allowed in normal code (only workflow scripts forbid it). Use it directly for `last_fetched`.
- **Blocking:** `git_fetch::fetch` is blocking; `PackageManager` methods are sync and MUST be called from `spawn_blocking` at the call sites (Task 6/7), not from an async context directly.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk package_manager`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/package_manager.rs src/services/mod.rs
git commit -m "feat(dist): PackageManager — fetch orchestration + hot-swap loader"
```

---

### Task 5: Wire `PackageManager` into `AppState` (hot-swappable resolution)

**Files:**
- Modify: `src/server.rs` (`AppState`, `create_app_state_with_overrides`, `build_package_loader` → cache/manager)
- Modify: `src/services/content_pipeline.rs` (resolve via manager, not a captured `Arc<PackageLoader>`)
- Modify: `src/main.rs` (`PACKAGES_CACHE_DIR` env)
- Test: `src/server.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `PackageManager` (Task 4).
- Produces: `AppState.package_manager: Arc<PackageManager>` **replaces** `package_loader: Arc<PackageLoader>`. All resolution goes through `state.package_manager.loader()`. `ContentPipeline` holds `Arc<PackageManager>` and calls `.loader().resolve(...)` per request.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_appstate_has_package_manager_resolving_builtin() {
    let loader = Arc::new(AssetLoader::new(None, None, None));
    let cfg = AppConfig::default();
    let state = create_app_state_with_config(loader, cfg).unwrap();
    assert!(state.package_manager.loader().resolve("byonk-builtin/default").is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk appstate_has_package_manager`
Expected: FAIL (field renamed/missing).

- [ ] **Step 3: Implement**

- In `create_app_state_with_overrides`: build the cache root from `PACKAGES_CACHE_DIR` env (fallback: `std::env::temp_dir().join("byonk-packages")`); build `extra_disk` from `PACKAGES_DIR` via the existing `collect_disk_packages`; `let package_manager = PackageManager::new(asset_loader.clone(), shared_config.clone(), PackageCache::new(cache_root), extra_disk);` then `package_manager.rebuild_loader();`.
- `AppState`: replace the `package_loader` field with `pub package_manager: Arc<PackageManager>`.
- `ContentPipeline::new(config, asset_loader, renderer, package_manager)` — change the 4th param type to `Arc<PackageManager>`; store it; replace `self.package_loader.resolve(...)` with `self.package_manager.loader().resolve(...)`. Update the `screens()`/`packages()` admin handlers and any other `state.package_loader` users to `state.package_manager.loader()` / `state.package_manager`.
- Keep `build_package_loader` only if still used; otherwise remove it (its logic moves into `PackageManager`/`rebuild_loader`). Update `collect_disk_packages` callers accordingly.
- `reload_config`: after `state.config.store(...)`, call `state.package_manager.rebuild_loader()` so a config-file edit to `packages:` takes effect (fetch still triggered by endpoints/refresh).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk` (fix all `state.package_loader` references crate-wide)
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/server.rs src/services/content_pipeline.rs src/main.rs
git commit -m "feat(dist): AppState.package_manager with hot-swappable loader"
```

---

### Task 6: `config_writer` — `upsert_package` / `remove_package`

**Files:**
- Modify: `src/services/config_writer.rs`
- Test: `src/services/config_writer.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces (mirroring the existing `upsert_device`/`remove_device`, keyed to `packages:`):
  ```rust
  pub fn upsert_package(yaml: &str, handle: &str, block: &serde_yaml::Mapping)
      -> Result<String, ConfigWriteError>;
  pub fn remove_package(yaml: &str, handle: &str) -> Result<String, ConfigWriteError>;
  ```

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_upsert_package_adds_and_updates() {
    let yaml = "auth_mode: api_key\n";
    let mut block = serde_yaml::Mapping::new();
    block.insert("repo".into(), "github.com/acme/x".into());
    block.insert("pin".into(), "v1.0.0".into());
    let out = upsert_package(yaml, "weather", &block).unwrap();
    let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
    assert_eq!(v["packages"]["weather"]["repo"], serde_yaml::Value::from("github.com/acme/x"));

    // update in place
    let mut b2 = serde_yaml::Mapping::new();
    b2.insert("repo".into(), "github.com/acme/x".into());
    b2.insert("pin".into(), "v2.0.0".into());
    let out2 = upsert_package(&out, "weather", &b2).unwrap();
    let v2: serde_yaml::Value = serde_yaml::from_str(&out2).unwrap();
    assert_eq!(v2["packages"]["weather"]["pin"], serde_yaml::Value::from("v2.0.0"));
}

#[test]
fn test_remove_package() {
    let yaml = "packages:\n  weather:\n    repo: github.com/acme/x\n    pin: v1\n";
    let out = remove_package(yaml, "weather").unwrap();
    let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
    assert!(v.get("packages").map(|p| p.get("weather").is_none()).unwrap_or(true));
}

#[test]
fn test_remove_missing_package_is_notfound() {
    assert!(matches!(remove_package("packages: {}\n", "nope"),
                     Err(ConfigWriteError::NotFound(_))));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk config_writer`
Expected: FAIL.

- [ ] **Step 3: Implement** — copy the private device helpers (`device_block_range`, `serialize_indented_device`, `insert_into_empty_*`, header insertion) into `packages:`-keyed variants, or generalize them with a `section: &str` parameter (preferred, DRY — but keep the existing device fns' behavior identical). Handle: `packages:` absent (create it), present-but-empty, present-with-entries; comment-preserving via `yamlpatch`, same as devices.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk config_writer`
Expected: PASS (device tests still green).

- [ ] **Step 5: Commit**

```bash
git add src/services/config_writer.rs
git commit -m "feat(dist): config_writer upsert_package/remove_package"
```

---

### Task 7: `package_refresh_interval` setting + periodic refresh task

**Files:**
- Modify: `src/models/config.rs` (add field)
- Modify: `src/api/admin/write.rs` (`SettingsWrite` + `patch_settings`)
- Modify: `src/main.rs` (spawn the interval task in `run_server`)
- Test: `src/models/config.rs` + `src/api/admin/write.rs` tests

**Interfaces:**
- Produces: `AppConfig.package_refresh_interval: u64` (seconds; `#[serde(default)]` ⇒ 0 = disabled). `SettingsWrite.package_refresh_interval: Option<u64>`.

- [ ] **Step 1: Write the failing test**

```rust
// config.rs
#[test]
fn test_package_refresh_interval_defaults_zero() {
    let c: AppConfig = serde_yaml::from_str("auth_mode: api_key\n").unwrap();
    assert_eq!(c.package_refresh_interval, 0);
}
```
(Plus a `patch_settings` test asserting a PATCH with `package_refresh_interval: 3600` writes `["package_refresh_interval"]` — follow the existing `patch_settings` test style.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk package_refresh_interval`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `AppConfig`: add `#[serde(default)] pub package_refresh_interval: u64,` and `package_refresh_interval: 0` in `impl Default`.
- `SettingsWrite`: add `pub(crate) package_refresh_interval: Option<u64>,`; in `patch_settings`, when present, `config_writer::set_scalar(&yaml, &["package_refresh_interval"], (n as u64).into())`.
- `run_server` (main.rs): after `state` is built, read `state.config.load().package_refresh_interval`; if `> 0`, spawn:
  ```rust
  let mgr = state.package_manager.clone();
  let cfg = state.config.clone();
  tokio::spawn(async move {
      loop {
          let secs = cfg.load().package_refresh_interval;
          if secs == 0 { tokio::time::sleep(std::time::Duration::from_secs(60)).await; continue; }
          tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
          let m = mgr.clone();
          // blocking git work off the async executor:
          let _ = tokio::task::spawn_blocking(move || m.refresh_all(false)).await;
      }
  });
  ```
  (Re-reading the interval each loop lets a settings change take effect without restart.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk package_refresh_interval config`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/models/config.rs src/api/admin/write.rs src/main.rs
git commit -m "feat(dist): package_refresh_interval setting + periodic refresh task"
```

---

### Task 8: Package write endpoints (register / patch / delete / update)

**Files:**
- Modify: `src/api/admin/write.rs` (new handlers)
- Modify: `src/api/admin/mod.rs` (routes)
- Test: `tests/admin_packages_test.rs` (new integration test)

**Interfaces:**
- Consumes: `config_writer::{upsert_package, remove_package}` (Task 6), `PackageManager` (Task 4/5), `persist`/`require_file_config`/`write_lock` (existing).
- Produces routes:
  ```
  POST   /api/admin/packages            register { handle, repo, pin, token? }
  PATCH  /api/admin/packages/:handle    update  { repo?, pin?, token? }
  DELETE /api/admin/packages/:handle
  POST   /api/admin/packages/:handle/update
  POST   /api/admin/packages/update
  ```
  DTO: `#[derive(Deserialize)] struct PackageWrite { handle: Option<String>, repo: Option<String>, pin: Option<String>, token: Option<String> }`.

- [ ] **Step 1: Write the failing test** (integration, using the `TestApp` harness with a file-backed config; a real git fetch is NOT asserted — assert config mutation + status transitions)

```rust
// register rejects the builtin handle and duplicates; writes the registry entry.
// delete rejects byonk-builtin; rejects a handle a device references; removes otherwise.
// After POST, GET /packages shows the new handle with status "fetching" or "error"
// (no network in test ⇒ error/offline is acceptable; assert the handle appears and token is redacted).
```
(Write concrete assertions with the harness: `app.post_json("/api/admin/packages", json!({"handle":"weather","repo":"github.com/x/y","pin":"v1","token":"secret"}))` → 200/202; then `GET /api/admin/config` body does NOT contain `"secret"`; `DELETE /api/admin/packages/byonk-builtin` → 4xx.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk --test admin_packages_test`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `add_package` (POST): `require_admin` → `require_file_config` → guard `handle != BUILTIN_HANDLE` and `!config.packages.contains_key(handle)` (else `ApiError::Conflict`) → `let _g = write_lock.lock().await` → build a `serde_yaml::Mapping` block (`repo`, `pin`, and `token` if given) → `upsert_package` → `persist` → `let mgr = state.package_manager.clone(); let h = handle.clone(); tokio::task::spawn_blocking(move || mgr.refresh_one(&h));` (async fetch) → return the handle's current `PackageInfo` (status likely `fetching`).
- `patch_package` (PATCH): merge provided fields into the existing block (preserve untouched fields incl. an unspecified `token`), `upsert_package`, `persist`; if `repo`/`pin` changed → spawn `refresh_one`.
- `delete_package` (DELETE): reject `byonk-builtin` (`ApiError`), reject if any `config.devices[*].screen` starts with `"<handle>/"` (dangling reference → `ApiError::Conflict` naming the device), else `remove_package` + `persist` + `state.package_manager.rebuild_loader()`.
- `update_package` (POST :handle/update): `spawn_blocking(refresh_one)`; return status.
- `update_all_packages` (POST /update): `spawn_blocking(|| refresh_all(true))`; return 200.
- Token redaction: never echo `token`; responses carry `PackageInfo` (built like `read::packages()`), which exposes only `token_set`.
- Routes in `mod.rs`:
  ```rust
  .route("/packages", get(read::packages).post(write::add_package))
  .route("/packages/:handle", patch(write::patch_package).delete(write::delete_package))
  .route("/packages/:handle/update", post(write::update_package))
  .route("/packages/update", post(write::update_all_packages))
  ```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk --test admin_packages_test` then `make check`
Expected: PASS / green.

- [ ] **Step 5: Commit**

```bash
git add src/api/admin/write.rs src/api/admin/mod.rs tests/admin_packages_test.rs
git commit -m "feat(dist): package register/patch/delete/update admin endpoints"
```

---

### Task 9: Enrich `GET /packages` from the status store

**Files:**
- Modify: `src/api/admin/read.rs` (`PackageInfo` + `packages()`)
- Test: `src/api/admin/read.rs` or `tests/admin_packages_test.rs`

**Interfaces:**
- Consumes: `PackageManager::status_snapshot()` (Task 4), `PinKind` (Task 1).
- Produces: `PackageInfo` gains `pin_kind: Option<PinKind>`, `resolved_sha: Option<String>`, `status: PackageState`-as-string, `last_fetched: Option<String>` (RFC3339), `error: Option<String>` — matching the spec §9a.1 shape. `builtin` handle reports `pin_kind: "embedded"`-equivalent (serialize builtin's kind as `"embedded"`, status `ready`).

- [ ] **Step 1: Write the failing test**

```rust
// GET /packages: byonk-builtin entry has builtin=true, status "ready", pin_kind "embedded",
// token_set=false; a registered handle with no successful fetch shows status "error"/"offline"
// or "fetching" and resolved_sha=null. Token never present.
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk --test admin_packages_test`
Expected: FAIL.

- [ ] **Step 3: Implement** — in `packages()`, pull `let statuses = state.package_manager.status_snapshot();` and for each handle fill the new fields: builtin ⇒ `{ status: "ready", pin_kind: "embedded", resolved_sha: null, ... }`; others ⇒ from `statuses.get(handle)` (`state` → string via serde, defaulting to `"error"`/absent when never fetched — pick `"fetching"` if a fetch is in flight, else the stored state, else `null`/`"error"`). Serialize `last_fetched` as `dt.to_rfc3339()`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `make check`
Expected: green.

- [ ] **Step 5: Commit**

```bash
git add src/api/admin/read.rs
git commit -m "feat(dist): GET /packages reports fetch status/sha/pin_kind"
```

---

### Task 10: Docs, CHANGES, and deploy tooling

**Files:**
- Modify: `docs/src/api/admin-api.md` (document the package endpoints + status fields)
- Modify: `CHANGES.md` (Unreleased)
- Modify: `tools/ha-vm/*` if a cache dir must be set for the add-on (`PACKAGES_CACHE_DIR` → persistent `/data`)

- [ ] **Step 1: Document the endpoints** — add `POST/PATCH/DELETE /api/admin/packages`, `/packages/:handle/update`, `/packages/update`, and the enriched `GET /packages` fields (`pin_kind`, `resolved_sha`, `status`, `last_fetched`, `error`, `token_set`) to `docs/src/api/admin-api.md`. Note pin semantics (sha immutable; tag/branch refreshable) and `package_refresh_interval`.

- [ ] **Step 2: CHANGES.md** — add an Unreleased bullet: git-backed package distribution (fetch/cache/refresh, package management API, periodic refresh).

- [ ] **Step 3: Add-on cache dir** — if the add-on should persist the cache, set `PACKAGES_CACHE_DIR=/data/packages` in the add-on config/run script (and document it). Verify `make docs`/`mdbook build` is green.

- [ ] **Step 4: Commit**

```bash
git add docs/src/api/admin-api.md CHANGES.md tools/ha-vm/
git commit -m "docs(dist): document package distribution API + cache dir"
```

---

## Self-Review

**Spec coverage** (spec §8 + §9a.1):
- §8.1 gix fetch → Task 1 (git2 fallback gate). §8.2 pin semantics (sha immutable / tag-branch refetch) → Tasks 1 (pin_kind) + 4 (immutable-skip, force). §8.3 cache keyed by repo+sha + offline → Tasks 2 + 4 (Offline state). §8.4 auth (host + token) → Task 1 (token) + redaction Tasks 8/9.
- §9a.1 endpoints: `GET` (enriched) → Task 9; `POST/PATCH/DELETE/:handle/update/update` → Task 8; token redaction → Tasks 8/9; dangling-ref reject on delete → Task 8.
- Periodic refresh + `package_refresh_interval` → Task 7. Hot-swap / rebuild-on-reload (follow-up #6) → Tasks 4/5.

**Placeholder scan:** the one intentionally non-code-complete area is `git_fetch::fetch` (Task 1), because gix's exact API is validated during the spike and may fall back to git2 — the interface, tests, and approach are fully specified; the gix call sequence is the implementer's spike output. Everything else has concrete code.

**Type consistency:** `PinKind`, `FetchOutcome`, `FetchError` (Task 1); `PackageCache` (Task 2); `PackageState`/`PackageStatus` (Task 3); `PackageManager` (Task 4) used consistently in Tasks 5–9. `AppState.package_manager` replaces `package_loader` everywhere (Task 5). `PackageInfo` fields defined in Plan 1, extended in Task 9.

**Risks / open items:**
- **gix API maturity** — Task 1 is the gate; git2 fallback is a localized swap.
- **Cache dir persistence in the add-on** — Task 10 sets `PACKAGES_CACHE_DIR`; without it the cache is a temp dir re-fetched after restart (functional, just not persistent).
- **Fetch-then-move to sha dir** — fetching before the sha is known means a temp checkout then a rename into `cache.checkout_dir(repo, sha)`; the implementer picks fetch-into-temp-then-rename vs resolve-sha-first. Task 4 notes both.
