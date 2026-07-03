//! Orchestrates screen-package fetch/cache/refresh and serves a
//! hot-swappable [`PackageLoader`] snapshot.
//!
//! `PackageManager` is the single owner of "what package handles resolve to
//! what bytes right now": it holds the [`PackageCache`] (fetched checkouts on
//! disk), a per-handle [`PackageStatus`] store (fetch/error/offline state for
//! `GET /packages`), and an [`ArcSwap`]-backed [`PackageLoader`] snapshot that
//! call sites resolve screens against. Fetching a package never blocks
//! readers: `refresh_one`/`refresh_all` build a brand new `PackageLoader` and
//! atomically swap it in; in-flight `resolve()` calls against the old
//! snapshot keep working against the old (still-valid) checkout until they
//! naturally pick up the new one on their next call.
//!
//! This fixes Plan-1 follow-up #6: previously the loader was built once at
//! startup and never rebuilt when `config.packages` changed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use chrono::Utc;

use crate::assets::AssetLoader;
use crate::models::config::PackageRef;
use crate::server::SharedConfig;
use crate::services::git_fetch;
use crate::services::package_cache::PackageCache;
use crate::services::package_loader::{PackageLoader, BUILTIN_HANDLE};
use crate::services::package_status::{PackageState, PackageStatus};

/// Owns package fetch orchestration + the live, hot-swappable [`PackageLoader`].
pub struct PackageManager {
    asset_loader: Arc<AssetLoader>,
    /// Reads `config.packages` fresh on every refresh (config can change
    /// underneath us via the admin API).
    config: SharedConfig,
    cache: PackageCache,
    /// Per-handle fetch state, surfaced via `status_snapshot` for `GET /packages`.
    status: Mutex<HashMap<String, PackageStatus>>,
    /// The live snapshot readers resolve screens against.
    loader: ArcSwap<PackageLoader>,
    /// `PACKAGES_DIR`-style dev packages that are always disk-backed, never fetched.
    extra_disk: HashMap<String, PathBuf>,
}

impl PackageManager {
    /// Build a manager and its initial loader snapshot (embedded builtin +
    /// `extra_disk` only — cache checkouts are added once `rebuild_loader`
    /// runs after a successful fetch).
    pub fn new(
        asset_loader: Arc<AssetLoader>,
        config: SharedConfig,
        cache: PackageCache,
        extra_disk: HashMap<String, PathBuf>,
    ) -> Arc<Self> {
        let initial = PackageLoader::new(asset_loader.clone(), extra_disk.clone());
        Arc::new(Self {
            asset_loader,
            config,
            cache,
            status: Mutex::new(HashMap::new()),
            loader: ArcSwap::from_pointee(initial),
            extra_disk,
        })
    }

    /// Current loader snapshot (cheap; call once per resolve, not once per file read).
    pub fn loader(&self) -> Arc<PackageLoader> {
        self.loader.load_full()
    }

    /// Rebuild the loader from the embedded builtin (added by `PackageLoader::new`
    /// itself) + `extra_disk` + every registered handle whose status has a
    /// `resolved_sha` that's actually present in the cache, then swap it in.
    pub fn rebuild_loader(&self) {
        let mut disk_map = self.extra_disk.clone();

        let config = self.config.load();
        let status = self.lock_status();
        for (handle, pkg_ref) in config.packages.iter() {
            if handle == BUILTIN_HANDLE {
                continue;
            }
            let Some(repo) = pkg_ref.repo.as_deref() else {
                continue;
            };
            let Some(st) = status.get(handle) else {
                continue;
            };
            let Some(sha) = st.resolved_sha.as_deref() else {
                continue;
            };
            if self.cache.has(repo, sha) {
                disk_map.insert(handle.clone(), self.cache.checkout_dir(repo, sha));
            }
        }
        drop(status);

        let fresh = PackageLoader::new(self.asset_loader.clone(), disk_map);
        self.loader.store(Arc::new(fresh));
    }

    /// Fetch one handle (always, ignoring any "already fresh" shortcut except
    /// the immutable-sha reuse case), update its status, then rebuild+swap
    /// the loader. No-op for the builtin handle, unknown handles, or handles
    /// with no `repo` (disk-only entries).
    pub fn refresh_one(&self, handle: &str) {
        if handle == BUILTIN_HANDLE {
            return;
        }
        let pkg_ref: PackageRef = match self.config.load().packages.get(handle) {
            Some(p) => p.clone(),
            None => return,
        };
        let Some(repo) = pkg_ref.repo.clone() else {
            return;
        };
        let pin = pkg_ref.pin.clone().unwrap_or_else(|| "main".to_string());
        let token = pkg_ref.token.clone();

        self.update_status(handle, |st| {
            st.state = Some(PackageState::Fetching);
        });

        // Immutable pin whose tree we already have on disk: reuse without a
        // network round-trip. A sha's content can never change underneath us.
        if git_fetch::looks_like_sha(&pin) && self.cache.has(&repo, &pin) {
            self.update_status(handle, |st| {
                st.state = Some(PackageState::Ready);
                st.resolved_sha = Some(pin.clone());
                st.pin_kind = Some(git_fetch::PinKind::Sha);
                st.error = None;
            });
            self.rebuild_loader();
            return;
        }

        // We don't know the resolved sha until after the fetch, so fetch
        // into a scratch checkout dir first, then move it into its final
        // content-addressed home (`checkout_dir(repo, resolved_sha)`) once
        // we know the sha. Scratch lives under the same cache root (keyed
        // off `repo`, so same filesystem as the final destination) to make
        // the final move a plain rename in the common case.
        let scratch = self.cache.checkout_dir(&repo, &scratch_name());
        let outcome = git_fetch::fetch(&repo, &pin, token.as_deref(), &scratch);
        // Always clean up scratch, win or lose.
        let cleanup_scratch = |scratch: &Path| {
            let _ = std::fs::remove_dir_all(scratch);
        };

        match outcome {
            Ok(fetched) => {
                let dest = self.cache.checkout_dir(&repo, &fetched.resolved_sha);
                if self.cache.has(&repo, &fetched.resolved_sha) {
                    // The branch/tag resolved to a sha we already have on disk — the
                    // cached tree is byte-identical (content-addressed). Discard
                    // scratch and mark Ready WITHOUT touching the live checkout dir:
                    // no teardown window, no needless rewrite, and a concurrent
                    // same-handle refresh can't race the move.
                    cleanup_scratch(&scratch);
                    self.update_status(handle, |st| {
                        st.state = Some(PackageState::Ready);
                        st.resolved_sha = Some(fetched.resolved_sha.clone());
                        st.pin_kind = Some(fetched.pin_kind);
                        st.last_fetched = Some(Utc::now());
                        st.error = None;
                    });
                    self.rebuild_loader();
                    return;
                }
                if let Err(e) = move_dir(&scratch, &dest) {
                    cleanup_scratch(&scratch);
                    // A fetch that succeeded but couldn't be installed is
                    // handled the same as a fetch that failed outright: keep
                    // serving a still-cached prior checkout (Offline) rather
                    // than erroring, since the loader isn't rebuilt on this
                    // path and the old checkout keeps serving either way.
                    let state = self.state_after_post_fetch_failure(handle, &repo);
                    self.update_status(handle, |st| {
                        st.state = Some(state);
                        st.error = Some(format!("installing fetched checkout: {e}"));
                    });
                    return;
                }
                self.update_status(handle, |st| {
                    st.state = Some(PackageState::Ready);
                    st.resolved_sha = Some(fetched.resolved_sha.clone());
                    st.pin_kind = Some(fetched.pin_kind);
                    st.last_fetched = Some(Utc::now());
                    st.error = None;
                });
                self.rebuild_loader();
            }
            Err(e) => {
                cleanup_scratch(&scratch);
                let message = e.to_string();
                let state = self.state_after_post_fetch_failure(handle, &repo);
                self.update_status(handle, |st| {
                    st.state = Some(state);
                    st.error = Some(message);
                });
            }
        }
    }

    /// Fetch every registered non-builtin package that needs it, then
    /// rebuild+swap the loader once. Skipping is keyed on **pin shape**, not
    /// run-time state: a sha pin is immutable, so an already-cached sha is
    /// left alone (`!force`); a tag/branch pin is mutable and must always get
    /// a real `refresh_one` call so it re-checks upstream for moves, no
    /// matter how recently it was last fetched. `force` skips nothing — every
    /// non-builtin handle gets a real `refresh_one` call (which itself
    /// already short-circuits a cached sha without a network round-trip, so
    /// this doesn't cause needless re-fetching of immutable pins).
    pub fn refresh_all(&self, force: bool) {
        let handles: Vec<String> = self
            .config
            .load()
            .packages
            .iter()
            .filter(|(handle, pkg_ref)| handle.as_str() != BUILTIN_HANDLE && pkg_ref.repo.is_some())
            .map(|(handle, _)| handle.clone())
            .collect();

        for handle in handles {
            if !force && self.pin_is_immutable_and_cached(&handle) {
                continue;
            }
            self.refresh_one(&handle);
        }
        self.rebuild_loader();
    }

    /// Snapshot of per-handle status for `GET /packages`.
    pub fn status_snapshot(&self) -> HashMap<String, PackageStatus> {
        self.lock_status().clone()
    }

    /// Remove `handle`'s fetch status entry, e.g. after `delete_package` so a
    /// later re-registration of the same handle doesn't briefly surface the
    /// stale `resolved_sha`/`Ready` state from the deleted package.
    pub fn forget_status(&self, handle: &str) {
        self.lock_status().remove(handle);
    }

    /// True only when `handle`'s configured pin is a sha (immutable) *and*
    /// that sha is already cached on disk. A tag/branch pin always returns
    /// false, regardless of run-time `PackageStatus`, so `refresh_all(false)`
    /// can't skip it forever once it's first fetched.
    fn pin_is_immutable_and_cached(&self, handle: &str) -> bool {
        let config = self.config.load();
        let Some(pkg_ref) = config.packages.get(handle) else {
            return false;
        };
        let Some(repo) = pkg_ref.repo.as_deref() else {
            return false;
        };
        let pin = pkg_ref.pin.as_deref().unwrap_or("main");
        git_fetch::looks_like_sha(pin) && self.cache.has(repo, pin)
    }

    /// Whether `handle`'s last-known-good checkout for `repo` is still
    /// present in the cache — the shared decision for both post-fetch
    /// failure paths in `refresh_one` (fetch itself failing, and a
    /// successful fetch failing to install): `Offline` when a prior checkout
    /// is still cached (keep serving it), `Error` when there's nothing to
    /// fall back to.
    fn state_after_post_fetch_failure(&self, handle: &str, repo: &str) -> PackageState {
        let prior_sha = self
            .lock_status()
            .get(handle)
            .and_then(|s| s.resolved_sha.clone());
        let still_cached = prior_sha
            .as_deref()
            .is_some_and(|sha| self.cache.has(repo, sha));
        if still_cached {
            PackageState::Offline
        } else {
            PackageState::Error
        }
    }

    fn lock_status(&self) -> std::sync::MutexGuard<'_, HashMap<String, PackageStatus>> {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn update_status(&self, handle: &str, f: impl FnOnce(&mut PackageStatus)) {
        let mut guard = self.lock_status();
        let entry = guard.entry(handle.to_string()).or_default();
        f(entry);
    }
}

/// A short-lived, process+time+random-unique name for a scratch checkout
/// dir. The random component (matching `git_fetch::scratch_dir` and the test
/// helper `tempdir_path`) guards against two same-nanosecond scratch dirs
/// colliding.
fn scratch_name() -> String {
    format!(
        "_tmp-{}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        rand::random::<u64>()
    )
}

/// Move a directory tree from `from` to `to`. Tries a plain rename first
/// (the common case, since `from`/`to` share a cache root); falls back to a
/// recursive copy + remove if the rename fails (e.g. a cross-filesystem
/// cache root), so a fetch never silently loses its result.
fn move_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        std::fs::remove_dir_all(to)?;
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir_recursive(from, to)?;
    std::fs::remove_dir_all(from)?;
    Ok(())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::config::AppConfig;
    use std::fs;
    use std::process::Command;

    fn write(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    fn tempdir_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "byonk-{prefix}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ))
    }

    fn shared_config(packages: HashMap<String, PackageRef>) -> SharedConfig {
        let mut config = AppConfig::default();
        config.packages = packages;
        Arc::new(ArcSwap::from_pointee(config))
    }

    fn asset_loader() -> Arc<AssetLoader> {
        Arc::new(AssetLoader::new(None, None, None))
    }

    /// Build a fixture package dir with one screen (`weather/forecast`) plus
    /// a root `byonk-screens.yaml`, mirroring the Plan-1 `PackageLoader`
    /// fixture pattern (see `package_loader.rs` tests).
    fn make_disk_package(name_prefix: &str) -> PathBuf {
        let tmp = tempdir_path(name_prefix);
        write(
            &tmp,
            "byonk-screens.yaml",
            "name: t\ndescription: d\nauthor: a\nlicense: MIT\n",
        );
        write(
            &tmp,
            "weather/forecast/meta.yaml",
            "title: F\ndescription: d\nbyonk: \"0.15\"\n",
        );
        write(
            &tmp,
            "weather/forecast/script.lua",
            "return { data = {} }\n",
        );
        write(&tmp, "weather/forecast/screen.svg", "<svg/>\n");
        tmp
    }

    #[test]
    fn test_manager_serves_extra_disk_package_and_reports_builtin() {
        let tmp = make_disk_package("pkgmgr_disk");

        let mut packages = HashMap::new();
        packages.insert("acme".to_string(), PackageRef::default());
        let config = shared_config(packages);

        let cache = PackageCache::new(tempdir_path("pkgmgr_cache"));
        let mut extra_disk = HashMap::new();
        extra_disk.insert("acme".to_string(), tmp.clone());

        let mgr = PackageManager::new(asset_loader(), config, cache, extra_disk);
        mgr.rebuild_loader();

        assert!(mgr.loader().resolve("acme/weather/forecast").is_some());
        assert!(mgr.loader().handles().iter().any(|h| h == "byonk-builtin"));
        // A disk-only package (no `repo`) never gets fetch state.
        assert!(mgr
            .status_snapshot()
            .get("acme")
            .and_then(|s| s.state)
            .is_none());

        let _ = fs::remove_dir_all(&tmp);
    }

    /// A source fixture git repo built on disk with the system `git` binary
    /// (a valid screen package committed on its default branch), used as a
    /// hermetic `refresh_one`/`refresh_all` source via a plain filesystem
    /// path — no network involved.
    struct FixtureRepo {
        url: String,
        branch: String,
        head_sha: String,
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=byonk-test",
                "-c",
                "user.email=t@example.com",
            ])
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn make_fixture_repo() -> FixtureRepo {
        let dir = tempdir_path("pkgmgr_git_src");
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        write(
            &dir,
            "byonk-screens.yaml",
            "name: t\ndescription: d\nauthor: a\nlicense: MIT\n",
        );
        write(
            &dir,
            "weather/forecast/meta.yaml",
            "title: F\ndescription: d\nbyonk: \"0.15\"\n",
        );
        write(
            &dir,
            "weather/forecast/script.lua",
            "return { data = {} }\n",
        );
        write(&dir, "weather/forecast/screen.svg", "<svg/>\n");
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "initial"]);

        let head_sha = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("rev-parse")
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        let branch = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .expect("branch name")
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();

        FixtureRepo {
            url: dir.to_string_lossy().to_string(),
            branch,
            head_sha,
        }
    }

    #[test]
    fn test_refresh_one_fetches_and_serves_from_local_repo() {
        let src = make_fixture_repo();
        let mut packages = HashMap::new();
        packages.insert(
            "weather".to_string(),
            PackageRef {
                repo: Some(src.url.clone()),
                pin: Some(src.branch.clone()),
                token: None,
            },
        );
        let config = shared_config(packages);
        let cache = PackageCache::new(tempdir_path("pkgmgr_cache_fetch"));

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.refresh_one("weather");

        let status = mgr.status_snapshot();
        let st = status.get("weather").expect("status recorded");
        assert_eq!(st.state, Some(PackageState::Ready));
        assert_eq!(st.resolved_sha.as_deref(), Some(src.head_sha.as_str()));
        assert!(st.last_fetched.is_some());

        assert!(mgr.loader().resolve("weather/weather/forecast").is_some());

        let _ = fs::remove_dir_all(&src.url);
    }

    #[test]
    fn test_refresh_one_does_not_tear_down_live_checkout_when_branch_resolves_to_cached_sha() {
        // Fix: a branch/tag pin that resolves to a sha we already have on
        // disk must NOT go through `move_dir` (remove_dir_all + rename) on
        // the live checkout dir — that would open a window where a
        // concurrent reader sees a vanished file, and risks losing the
        // checkout entirely if the rename half fails. Prove it by planting a
        // sentinel file in the live checkout dir and confirming a second
        // `refresh_one` (branch unchanged, same resolved sha) leaves it
        // untouched.
        let src = make_fixture_repo();
        let mut packages = HashMap::new();
        packages.insert(
            "weather".to_string(),
            PackageRef {
                repo: Some(src.url.clone()),
                pin: Some(src.branch.clone()),
                token: None,
            },
        );
        let config = shared_config(packages);
        let cache_root = tempdir_path("pkgmgr_cache_no_teardown");
        let cache = PackageCache::new(cache_root.clone());

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.refresh_one("weather");
        let sha = mgr
            .status_snapshot()
            .get("weather")
            .and_then(|s| s.resolved_sha.clone())
            .expect("resolved after first refresh_one");
        assert_eq!(sha, src.head_sha);

        let probe_cache = PackageCache::new(cache_root);
        let dest = probe_cache.checkout_dir(&src.url, &sha);
        let sentinel = dest.join("_sentinel");
        fs::write(&sentinel, b"x").unwrap();
        assert!(sentinel.exists(), "sentinel written into live checkout");

        // Branch hasn't moved, so this resolves to the same sha again.
        mgr.refresh_one("weather");
        let status = mgr.status_snapshot();
        let st = status.get("weather").unwrap();
        assert_eq!(st.state, Some(PackageState::Ready));
        assert_eq!(st.resolved_sha.as_deref(), Some(src.head_sha.as_str()));
        assert!(
            sentinel.exists(),
            "live checkout dir was torn down by a refresh that resolved to an already-cached sha"
        );

        let _ = fs::remove_dir_all(&src.url);
    }

    #[test]
    fn test_refresh_one_error_when_repo_missing_and_never_cached() {
        let mut packages = HashMap::new();
        packages.insert(
            "ghost".to_string(),
            PackageRef {
                repo: Some("/no/such/repo/on/disk".to_string()),
                pin: Some("main".to_string()),
                token: None,
            },
        );
        let config = shared_config(packages);
        let cache = PackageCache::new(tempdir_path("pkgmgr_cache_err"));

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.refresh_one("ghost");

        let status = mgr.status_snapshot();
        let st = status.get("ghost").expect("status recorded");
        assert_eq!(st.state, Some(PackageState::Error));
        assert!(st.error.is_some());
        assert!(st.resolved_sha.is_none());
    }

    #[test]
    fn test_refresh_one_goes_offline_when_fetch_fails_but_prior_checkout_cached() {
        let src = make_fixture_repo();
        let mut packages = HashMap::new();
        packages.insert(
            "weather".to_string(),
            PackageRef {
                repo: Some(src.url.clone()),
                pin: Some(src.branch.clone()),
                token: None,
            },
        );
        let config = shared_config(packages);
        let cache = PackageCache::new(tempdir_path("pkgmgr_cache_offline"));

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.refresh_one("weather");
        assert_eq!(
            mgr.status_snapshot().get("weather").unwrap().state,
            Some(PackageState::Ready)
        );

        // Repo host goes away (deleted/unreachable) — same `repo` string
        // (so the cache key is unchanged), but the fetch itself now fails.
        // Its cached checkout from the successful fetch above is still on
        // disk under that same cache key.
        let _ = fs::remove_dir_all(&src.url);

        mgr.refresh_one("weather");
        let status = mgr.status_snapshot();
        let st = status.get("weather").unwrap();
        assert_eq!(st.state, Some(PackageState::Offline));
        // Old resolved sha is preserved so the loader keeps serving it.
        assert_eq!(st.resolved_sha.as_deref(), Some(src.head_sha.as_str()));
        assert!(mgr.loader().resolve("weather/weather/forecast").is_some());

        let _ = fs::remove_dir_all(&src.url);
    }

    #[test]
    fn test_refresh_one_noop_for_builtin_and_disk_only_handles() {
        let mut packages = HashMap::new();
        packages.insert(BUILTIN_HANDLE.to_string(), PackageRef::default());
        packages.insert("acme".to_string(), PackageRef::default()); // no repo
        let config = shared_config(packages);
        let cache = PackageCache::new(tempdir_path("pkgmgr_cache_noop"));

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.refresh_one(BUILTIN_HANDLE);
        mgr.refresh_one("acme");
        mgr.refresh_one("unknown-handle");

        assert!(mgr.status_snapshot().is_empty());
    }

    #[test]
    fn test_refresh_all_skips_already_ready_unless_forced() {
        let src = make_fixture_repo();
        let mut packages = HashMap::new();
        packages.insert(
            "weather".to_string(),
            PackageRef {
                repo: Some(src.url.clone()),
                pin: Some(src.branch.clone()),
                token: None,
            },
        );
        let config = shared_config(packages);
        let cache = PackageCache::new(tempdir_path("pkgmgr_cache_refresh_all"));

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.refresh_all(false);
        assert_eq!(
            mgr.status_snapshot().get("weather").unwrap().state,
            Some(PackageState::Ready)
        );

        // Re-running is idempotent either way: a branch pin is re-fetched
        // every time (`force` or not — see
        // `test_refresh_all_refetches_mutable_branch_pin_but_not_cached_sha_pin`
        // for proof it's a *real* re-fetch, not a skip), and since upstream
        // hasn't moved it still resolves to the same sha and stays `Ready`.
        mgr.refresh_all(false);
        mgr.refresh_all(true);
        assert_eq!(
            mgr.status_snapshot().get("weather").unwrap().state,
            Some(PackageState::Ready)
        );

        let _ = fs::remove_dir_all(&src.url);
    }

    #[test]
    fn test_refresh_all_refetches_mutable_branch_pin_but_not_cached_sha_pin() {
        // A branch pin must be re-fetched by every `refresh_all(false)` call
        // so it picks up upstream moves — it must never be skipped just
        // because the handle is already `Ready` with a cached checkout.
        let src = make_fixture_repo();
        let mut packages = HashMap::new();
        packages.insert(
            "weather".to_string(),
            PackageRef {
                repo: Some(src.url.clone()),
                pin: Some(src.branch.clone()),
                token: None,
            },
        );
        let config = shared_config(packages);
        let cache = PackageCache::new(tempdir_path("pkgmgr_cache_refresh_all_branch"));

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.refresh_all(false);
        let first_sha = mgr
            .status_snapshot()
            .get("weather")
            .and_then(|s| s.resolved_sha.clone())
            .expect("resolved after first refresh_all");
        assert_eq!(first_sha, src.head_sha);

        // A new commit lands on the same branch upstream (v1 -> v2 content).
        write(
            Path::new(&src.url),
            "weather/forecast/script.lua",
            "return { data = { v = 2 } }\n",
        );
        git(Path::new(&src.url), &["add", "-A"]);
        git(Path::new(&src.url), &["commit", "-q", "-m", "v2"]);
        let second_sha = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&src.url)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("rev-parse")
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        assert_ne!(first_sha, second_sha);

        // A plain refresh_all(false) on a *branch* pin must re-fetch and
        // pick up the new commit.
        mgr.refresh_all(false);
        let status = mgr.status_snapshot();
        let st = status.get("weather").unwrap();
        assert_eq!(st.state, Some(PackageState::Ready));
        assert_eq!(
            st.resolved_sha.as_deref(),
            Some(second_sha.as_str()),
            "branch pin must be re-fetched by refresh_all(false), not skipped"
        );

        let _ = fs::remove_dir_all(&src.url);
    }

    #[test]
    fn test_refresh_all_does_not_refetch_already_cached_sha_pin() {
        // A sha pin is immutable: once cached, `refresh_all(false)` must
        // leave it alone, even if upstream becomes unreachable.
        let src = make_fixture_repo();
        let mut packages = HashMap::new();
        packages.insert(
            "weather".to_string(),
            PackageRef {
                repo: Some(src.url.clone()),
                pin: Some(src.head_sha.clone()),
                token: None,
            },
        );
        let config = shared_config(packages);
        let cache = PackageCache::new(tempdir_path("pkgmgr_cache_refresh_all_sha"));

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.refresh_all(false);
        assert_eq!(
            mgr.status_snapshot().get("weather").unwrap().state,
            Some(PackageState::Ready)
        );
        assert_eq!(
            mgr.status_snapshot()
                .get("weather")
                .unwrap()
                .resolved_sha
                .as_deref(),
            Some(src.head_sha.as_str())
        );

        // Upstream disappears entirely; if the sha pin were re-fetched this
        // would flip to Offline/Error.
        let _ = fs::remove_dir_all(&src.url);

        mgr.refresh_all(false);
        let status = mgr.status_snapshot();
        let st = status.get("weather").unwrap();
        assert_eq!(st.state, Some(PackageState::Ready));
        assert!(st.error.is_none());
        assert_eq!(st.resolved_sha.as_deref(), Some(src.head_sha.as_str()));
    }

    #[test]
    fn test_refresh_one_install_failure_after_fetch_honors_offline_when_prior_cached() {
        // A `move_dir` failure right after a *successful* git fetch (e.g. a
        // filesystem hiccup installing the new checkout) must fall back to
        // `Offline` when a prior good checkout is still cached — the same
        // rule the sibling `git_fetch::fetch` `Err` branch already applies —
        // not unconditionally `Error`.
        let src = make_fixture_repo();
        let mut packages = HashMap::new();
        packages.insert(
            "weather".to_string(),
            PackageRef {
                repo: Some(src.url.clone()),
                pin: Some(src.branch.clone()),
                token: None,
            },
        );
        let config = shared_config(packages);
        let cache_root = tempdir_path("pkgmgr_cache_install_fail");
        let cache = PackageCache::new(cache_root.clone());

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.refresh_one("weather");
        let sha1 = mgr
            .status_snapshot()
            .get("weather")
            .and_then(|s| s.resolved_sha.clone())
            .expect("resolved after first refresh_one");
        assert_eq!(sha1, src.head_sha);

        // A new commit lands upstream, so the next fetch resolves to a new
        // sha — pre-sabotage *that* destination by creating a plain file
        // where `move_dir` expects to install a directory, so the git fetch
        // itself succeeds but the install (`move_dir`) fails.
        write(
            Path::new(&src.url),
            "weather/forecast/script.lua",
            "return { data = { v = 2 } }\n",
        );
        git(Path::new(&src.url), &["add", "-A"]);
        git(Path::new(&src.url), &["commit", "-q", "-m", "v2"]);
        let sha2 = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&src.url)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("rev-parse")
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();

        let probe_cache = PackageCache::new(cache_root);
        let dest = probe_cache.checkout_dir(&src.url, &sha2);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, b"not a directory, so move_dir's install fails").unwrap();

        mgr.refresh_one("weather");
        let status = mgr.status_snapshot();
        let st = status.get("weather").unwrap();
        assert_eq!(
            st.state,
            Some(PackageState::Offline),
            "install failure with a prior cached checkout must report Offline, not Error"
        );
        assert!(st.error.is_some());
        assert_eq!(
            st.resolved_sha.as_deref(),
            Some(sha1.as_str()),
            "prior resolved sha is kept so the loader keeps serving it"
        );

        let _ = fs::remove_dir_all(&src.url);
    }

    #[test]
    fn test_forget_status_clears_entry() {
        let mut packages = HashMap::new();
        packages.insert("acme".to_string(), PackageRef::default());
        let config = shared_config(packages);
        let cache = PackageCache::new(tempdir_path("pkgmgr_cache_forget"));

        let mgr = PackageManager::new(asset_loader(), config, cache, HashMap::new());
        mgr.update_status("acme", |st| {
            st.state = Some(PackageState::Ready);
            st.resolved_sha = Some("deadbeef".to_string());
        });
        assert!(mgr.status_snapshot().contains_key("acme"));

        mgr.forget_status("acme");
        assert!(!mgr.status_snapshot().contains_key("acme"));
    }
}
