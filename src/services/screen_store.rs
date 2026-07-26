//! `ScreenStore` — the sole owner of screen mutation (read/write authored
//! screen files), sitting beside the read-only `AssetLoader`/`ScreenRepoLoader`
//! resolution path. Only handles registered as writable
//! (`LocalScreenRepoSource`, via `ScreenRepoSource::writable_root`) can be
//! written to; everything else (the embedded `byonk-builtin`, git-fetched
//! screen repos) is read-only and returns `StoreError::ReadOnly` with a
//! copy-hint pointing at the caller's next step (Task 7's `copy_screen`).

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::services::content_pipeline::ContentPipeline;
use crate::services::screen_repo_loader::ScreenRepoSource;
use crate::services::screen_repo_manager::ScreenRepoManager;

/// Largest file `read_file`/`write_file` will handle. Guards against
/// accidentally loading e.g. a stray multi-gigabyte asset into memory.
const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;

/// A screen file's bytes plus enough metadata for the authoring UI/API to
/// display it and detect edit conflicts.
pub struct FileContents {
    pub bytes: Vec<u8>,
    pub etag: String,
    pub binary: bool,
}

/// Failure modes for `ScreenStore` reads/writes.
#[derive(Debug)]
pub enum StoreError {
    /// The target handle has no `writable_root` (embedded or git-fetched).
    /// `copy_hint` names the writable handle + how to fork into it.
    ReadOnly { copy_hint: String },
    /// The `screen_ref`/`file` doesn't resolve to anything.
    NotFound,
    /// `if_match` didn't match the file's current etag (including when the
    /// file no longer exists — a stale etag against a deleted file is a
    /// conflict, not a silent create).
    Conflict,
    /// `screen_path` or `file` escapes the screen's directory (`..`,
    /// absolute, empty, or symlink escape).
    Traversal,
    /// `bytes` (write) or the on-disk file (read) exceeds `MAX_FILE_BYTES`.
    TooLarge,
    /// Any other I/O failure, stringified.
    Io(String),
}

/// Content-addressed etag: the blake3 hex digest of the file's bytes.
pub fn etag(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Which starter template `create_screen` scaffolds. One variant today;
/// the enum exists so a future starter (e.g. a data-table layout) can be
/// added without changing `create_screen`'s signature.
#[derive(Debug, Clone, Copy)]
pub enum StarterKind {
    Minimal,
}

impl StarterKind {
    /// The (filename, contents) pairs to scaffold for this starter kind.
    fn files(self) -> [(&'static str, &'static str); 3] {
        match self {
            StarterKind::Minimal => [
                ("meta.yaml", STARTER_META),
                ("script.lua", STARTER_LUA),
                ("screen.svg", STARTER_SVG),
            ],
        }
    }
}

const STARTER_META: &str =
    "title: New Screen\ndescription: A new screen.\nbyonk: \"0.15\"\nrefresh: 300\n";
const STARTER_LUA: &str =
    "-- Return the data table for your screen.\nreturn { data = { message = \"Hello\" } }\n";
const STARTER_SVG: &str = concat!(
    "{% extends \"byonk-base-v1/base.svg\" %}\n",
    "{% block content %}\n",
    "  <text x=\"40\" y=\"80\" font-size=\"48\">{{ data.message }}</text>\n",
    "{% endblock %}\n",
);

/// Reject `..`, absolute, and empty components. Returns a clean relative `PathBuf`.
///
/// Used for both the per-file `file` argument and (via `ScreenStore::split_ref`)
/// the `screen_path` half of a `screen_ref` — anything that becomes part of an
/// on-disk write target must fail closed here, at parse time, rather than
/// relying solely on the later canonicalize guard (which only catches
/// symlink escapes once `..`/absolute components are already excluded).
fn safe_rel(rel: &str) -> Result<PathBuf, StoreError> {
    let p = Path::new(rel);
    if p.is_absolute() {
        return Err(StoreError::Traversal);
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => out.push(s),
            _ => return Err(StoreError::Traversal), // ParentDir/RootDir/CurDir/Prefix
        }
    }
    if out.as_os_str().is_empty() {
        return Err(StoreError::Traversal);
    }
    Ok(out)
}

/// Find the deepest ancestor of `path` (inclusive) that currently exists on
/// disk. A screen's directory usually doesn't exist yet on its first write,
/// so the write-path symlink guard canonicalizes the nearest *real* ancestor
/// instead of `path` itself, and verifies that before creating anything.
///
/// Terminates: called only after the writable root itself has already been
/// canonicalized successfully, and that root is always an ancestor of
/// `path`, so the walk up `parent()` is guaranteed to hit an existing
/// directory (the root, at the latest) before running out of components.
fn deepest_existing_ancestor(path: &Path) -> &Path {
    let mut p = path;
    loop {
        if p.exists() {
            return p;
        }
        match p.parent() {
            Some(parent) => p = parent,
            None => return p,
        }
    }
}

/// Sole owner of screen mutation: reads/writes an individual screen file by
/// `"handle/screen_path"` + file-relative path, refusing writes to any handle
/// that isn't backed by a writable on-disk repo (see
/// `ScreenRepoSource::writable_root`).
pub struct ScreenStore {
    manager: Arc<ScreenRepoManager>,
    /// Unused until Task 9's `render` (renders a screen through the same
    /// pipeline devices use, so authors can preview an edit before publishing
    /// it).
    #[allow(dead_code)]
    pipeline: Arc<ContentPipeline>,
}

impl ScreenStore {
    pub fn new(manager: Arc<ScreenRepoManager>, pipeline: Arc<ContentPipeline>) -> Self {
        Self { manager, pipeline }
    }

    /// Split `"handle/screen_path"`, rejecting an empty handle and validating
    /// `screen_path` itself as a safe relative path (non-empty, no
    /// `..`/absolute components — mirrors `screen_repo_loader::split_ref`'s
    /// non-empty check, but stricter: this half becomes part of an on-disk
    /// write target, so a bare non-empty check isn't enough).
    fn split_ref(screen_ref: &str) -> Result<(&str, &str), StoreError> {
        let (handle, screen_path) = screen_ref.split_once('/').ok_or(StoreError::NotFound)?;
        if handle.is_empty() {
            return Err(StoreError::NotFound);
        }
        safe_rel(screen_path)?;
        Ok((handle, screen_path))
    }

    /// Resolve the writable root directory for `handle`, or a `ReadOnly`
    /// error naming the handle + a hint to copy `handle/screen_path` into a
    /// writable one. Callers join `screen_path`/`file` onto this themselves;
    /// resolved once per call site rather than once per path segment.
    fn resolve_writable_root(
        &self,
        handle: &str,
        screen_path: &str,
    ) -> Result<PathBuf, StoreError> {
        let loader = self.manager.loader();
        let src = loader.source_for(handle).ok_or(StoreError::NotFound)?;
        src.writable_root()
            .map(Path::to_path_buf)
            .ok_or_else(|| StoreError::ReadOnly {
                copy_hint: format!(
                    "'{handle}' is read-only; use copy_screen to fork '{handle}/{screen_path}' into a writable repo (e.g. 'local')"
                ),
            })
    }

    /// Read one file within a screen (`screen_ref` e.g. `"local/clock"`,
    /// `file` e.g. `"script.lua"`).
    pub fn read_file(&self, screen_ref: &str, file: &str) -> Result<FileContents, StoreError> {
        let (handle, screen_path) = Self::split_ref(screen_ref)?;
        let rel = safe_rel(file)?;
        let loader = self.manager.loader();
        let src = loader.source_for(handle).ok_or(StoreError::NotFound)?;
        let full_rel = format!("{screen_path}/{}", rel.to_string_lossy());
        let bytes = src.read(&full_rel).ok_or(StoreError::NotFound)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        let binary = std::str::from_utf8(&bytes).is_err();
        let etag = etag(&bytes);
        Ok(FileContents {
            bytes,
            etag,
            binary,
        })
    }

    /// Write one file within a screen, enforcing optimistic concurrency via
    /// `if_match` (the etag the caller last read; `None` skips the check —
    /// last-write-wins, or "create"). Returns the new etag on success.
    pub fn write_file(
        &self,
        screen_ref: &str,
        file: &str,
        bytes: &[u8],
        if_match: Option<&str>,
    ) -> Result<String, StoreError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        let (handle, screen_path) = Self::split_ref(screen_ref)?;
        let rel = safe_rel(file)?;
        let base = self.resolve_writable_root(handle, screen_path)?;
        let target = base.join(screen_path).join(&rel);

        // Guard + create the target's parent dir BEFORE any filesystem
        // interaction with `target` itself — including the `if_match` read
        // below, not just the eventual write. A symlink planted at any
        // point along `screen_path`/`file` must be caught here, not
        // followed by a stat/read first.
        Self::ensure_writable_parent(&base, &target)?;

        if let Some(expected) = if_match {
            match std::fs::read(&target) {
                Ok(cur) if etag(&cur) != expected => return Err(StoreError::Conflict),
                Ok(_) => {}
                // A stale etag presented against a file that no longer
                // exists (e.g. deleted concurrently) is a conflict, not a
                // licence to resurrect it under the caller's assumed
                // starting content.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(StoreError::Conflict)
                }
                Err(e) => return Err(StoreError::Io(e.to_string())),
            }
        }

        Self::atomic_write(&target, bytes)?;
        self.manager.rebuild_loader();
        Ok(etag(bytes))
    }

    /// Canonicalize `base` (the writable root) and verify that the nearest
    /// existing ancestor of `target`'s parent resolves under it — the
    /// write-path's verify-before-mutate symlink-escape guard, factored out
    /// so every operation that writes a file into a writable repo runs it.
    /// Verify BEFORE touching the filesystem: never `create_dir_all` first
    /// and check after. `base` must already exist (`LocalScreenRepoSource::load`
    /// had to read it to load the manifest before this handle was ever
    /// registered as writable), so if it fails to canonicalize now, the
    /// writable root was deleted or replaced out from under a stale loader
    /// snapshot; treat that as a hard failure rather than silently skipping
    /// the guard.
    fn ensure_writable_parent(base: &Path, target: &Path) -> Result<(), StoreError> {
        let parent = target.parent().ok_or_else(|| {
            StoreError::Io(format!(
                "cannot determine parent directory of {}",
                target.display()
            ))
        })?;

        let canon_base = base.canonicalize().map_err(|e| {
            StoreError::Io(format!(
                "writable root {} is unavailable: {e}",
                base.display()
            ))
        })?;
        let existing_ancestor = deepest_existing_ancestor(parent);
        let canon_existing = existing_ancestor
            .canonicalize()
            .map_err(|e| StoreError::Io(e.to_string()))?;
        if !canon_existing.starts_with(&canon_base) {
            return Err(StoreError::Traversal);
        }
        std::fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(())
    }

    /// Atomic tmp+rename write of `bytes` to `target` (whose parent dir must
    /// already exist and have been guard-verified by `ensure_writable_parent`).
    /// The tmp name is unique per write (pid + random suffix appended, not
    /// substituted via `with_extension`) so concurrent writes to sibling
    /// files that share a stem-less extension swap (e.g. `script.lua` and
    /// `script.svg`) never stage through the same path, and an aborted write
    /// never leaves a fixed, collidable `*.byonk-tmp` name behind.
    fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), StoreError> {
        let file_name = target
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| StoreError::Io(format!("invalid file name in {}", target.display())))?;
        let tmp = target.with_file_name(format!(
            "{file_name}.byonk-tmp-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::write(&tmp, bytes).map_err(|e| StoreError::Io(e.to_string()))?;
        std::fs::rename(&tmp, target).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(())
    }

    /// Guarded write of one file into a writable repo: `ensure_writable_parent`
    /// then `atomic_write`. Shared by `create_screen` and `copy_screen`, which
    /// (unlike `write_file`) never need an `if_match` check interleaved
    /// between the two. An associated fn (not a method) — it never touches
    /// `self`.
    fn put(base: &Path, target: &Path, bytes: &[u8]) -> Result<(), StoreError> {
        Self::ensure_writable_parent(base, target)?;
        Self::atomic_write(target, bytes)
    }

    /// Scaffold a new screen (`handle/name`) from `template`'s starter files.
    /// Rejects an already-existing destination as `Conflict`.
    pub fn create_screen(
        &self,
        handle: &str,
        name: &str,
        template: StarterKind,
    ) -> Result<String, StoreError> {
        let screen_path = safe_rel(name)?;
        let base = self.resolve_writable_root(handle, name)?;
        let dir = base.join(&screen_path);
        if dir.exists() {
            return Err(StoreError::Conflict);
        }

        for (rel, contents) in template.files() {
            if let Err(e) = Self::put(&base, &dir.join(rel), contents.as_bytes()) {
                // Best-effort cleanup: leaving a half-scaffolded dir behind
                // would make `dir.exists()` above trip `Conflict` on every
                // retry, with no way back in short of an out-of-band
                // delete. `dir` was only ever joined onto `base` (never
                // escaped it — every `put` call canonicalize-verifies its
                // own target), so removing it here can't touch anything
                // outside the writable root.
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
        }
        self.manager.rebuild_loader();
        Ok(format!("{handle}/{name}"))
    }

    /// Fork a screen (built-in, git-fetched, or another writable repo) into a
    /// writable destination, carrying over every file under the source
    /// screen's directory (not just `meta.yaml`/`script.lua`/`screen.svg`).
    /// Rejects an already-existing destination as `Conflict`.
    pub fn copy_screen(
        &self,
        from_ref: &str,
        to_handle: &str,
        to_name: &str,
    ) -> Result<String, StoreError> {
        let (from_handle, from_path) = Self::split_ref(from_ref)?;
        let to_path = safe_rel(to_name)?;

        let loader = self.manager.loader();
        let from_source = loader.source_for(from_handle).ok_or(StoreError::NotFound)?;
        if !from_source.screen_paths().iter().any(|p| p == from_path) {
            return Err(StoreError::NotFound);
        }

        let to_base = self.resolve_writable_root(to_handle, to_name)?;
        let to_dir = to_base.join(&to_path);
        if to_dir.exists() {
            return Err(StoreError::Conflict);
        }

        if let Err(e) = Self::copy_screen_files(from_source.as_ref(), from_path, &to_base, &to_dir)
        {
            // Best-effort cleanup, same reasoning as `create_screen`: a
            // failure partway through (a bad file, a `put` error) must not
            // leave a half-copied screen dir that permanently trips the
            // `Conflict` check above on retry, and that `rebuild_loader`
            // (below — skipped on this path) would otherwise never even
            // pick up correctly.
            let _ = std::fs::remove_dir_all(&to_dir);
            return Err(e);
        }

        self.manager.rebuild_loader();
        Ok(format!("{to_handle}/{to_name}"))
    }

    /// Copy every file `from_source.screen_files(from_path)` lists into
    /// `to_dir` (a subdirectory of `to_base`). An associated fn (not a
    /// method): takes the source directly so it can be exercised in tests
    /// against a stub `ScreenRepoSource`, independent of `ScreenRepoManager`
    /// plumbing.
    fn copy_screen_files(
        from_source: &dyn ScreenRepoSource,
        from_path: &str,
        to_base: &Path,
        to_dir: &Path,
    ) -> Result<(), StoreError> {
        for rel in from_source.screen_files(from_path) {
            let bytes = from_source.read(&rel).ok_or_else(|| {
                StoreError::Io(format!("source file {rel} listed but unreadable"))
            })?;
            if bytes.len() > MAX_FILE_BYTES {
                return Err(StoreError::TooLarge);
            }
            let suffix = rel
                .strip_prefix(from_path)
                .and_then(|s| s.strip_prefix('/'))
                .ok_or_else(|| {
                    StoreError::Io(format!(
                        "source file {rel} is not under screen path {from_path}"
                    ))
                })?;
            // `screen_files` is a public trait method with no default
            // impl — a future (or buggy) `ScreenRepoSource` could report an
            // entry outside its own screen dir (`..`), and this suffix
            // becomes part of an on-disk write target, so it must pass
            // through the exact same guard every other write-target
            // fragment does. Fail closed here rather than trusting the
            // source.
            let suffix = safe_rel(suffix)?;
            Self::put(to_base, &to_dir.join(&suffix), &bytes)?;
        }
        Ok(())
    }

    /// Rename a screen within its own writable repo (no cross-handle moves).
    /// Rejects an already-existing destination as `Conflict`.
    pub fn rename_screen(&self, screen_ref: &str, new_name: &str) -> Result<String, StoreError> {
        let (handle, screen_path) = Self::split_ref(screen_ref)?;
        let new_path = safe_rel(new_name)?;
        let base = self.resolve_writable_root(handle, screen_path)?;
        let old_dir = base.join(screen_path);
        let new_dir = base.join(&new_path);

        let canon_base = base.canonicalize().map_err(|e| {
            StoreError::Io(format!(
                "writable root {} is unavailable: {e}",
                base.display()
            ))
        })?;
        let canon_old = old_dir.canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound
            } else {
                StoreError::Io(e.to_string())
            }
        })?;
        if !canon_old.starts_with(&canon_base) {
            return Err(StoreError::Traversal);
        }
        if !canon_old.is_dir() {
            return Err(StoreError::NotFound);
        }

        if new_dir.exists() {
            return Err(StoreError::Conflict);
        }
        // Verify the destination's nearest existing ancestor resolves under
        // `base` too — verify BEFORE creating anything, same as a write —
        // so a symlinked intermediate segment in `new_name` can't redirect
        // the eventual `rename` outside the writable root.
        let new_parent = new_dir.parent().unwrap_or(&new_dir);
        let canon_new_parent = deepest_existing_ancestor(new_parent)
            .canonicalize()
            .map_err(|e| StoreError::Io(e.to_string()))?;
        if !canon_new_parent.starts_with(&canon_base) {
            return Err(StoreError::Traversal);
        }
        std::fs::create_dir_all(new_parent).map_err(|e| StoreError::Io(e.to_string()))?;

        std::fs::rename(&old_dir, &new_dir).map_err(|e| StoreError::Io(e.to_string()))?;
        self.manager.rebuild_loader();
        Ok(format!("{handle}/{new_name}"))
    }

    /// Delete a screen from its writable repo.
    pub fn delete_screen(&self, screen_ref: &str) -> Result<(), StoreError> {
        let (handle, screen_path) = Self::split_ref(screen_ref)?;
        let base = self.resolve_writable_root(handle, screen_path)?;
        let dir = base.join(screen_path);

        let canon_base = base.canonicalize().map_err(|e| {
            StoreError::Io(format!(
                "writable root {} is unavailable: {e}",
                base.display()
            ))
        })?;
        let canon_dir = dir.canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound
            } else {
                StoreError::Io(e.to_string())
            }
        })?;
        if !canon_dir.starts_with(&canon_base) {
            return Err(StoreError::Traversal);
        }
        // Defense in depth: `screen_path` is non-empty and lexically
        // `..`-free (enforced by `safe_rel` inside `split_ref`), so `dir`
        // can never lexically equal `base` — but a symlink could still
        // resolve it back to the repo root. Assert that explicitly too:
        // `remove_dir_all` on an unverified path is the highest-consequence
        // call in this module.
        if canon_dir == canon_base {
            return Err(StoreError::Traversal);
        }
        if !canon_dir.is_dir() {
            return Err(StoreError::NotFound);
        }

        std::fs::remove_dir_all(&dir).map_err(|e| StoreError::Io(e.to_string()))?;
        self.manager.rebuild_loader();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetLoader;
    use crate::models::AppConfig;
    use crate::server::SharedConfig;
    use crate::services::screen_repo_manager::tests::test_manager_with_screens_dir;
    use crate::services::RenderService;

    fn tempdir_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "byonk-{prefix}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ))
    }

    /// A manager whose `local` handle is a writable temp-dir repo, plus the
    /// read-only embedded `byonk-builtin` (always present). The repo root is
    /// nested two levels under a private, unique outer directory
    /// (`outer/inner/local_repo`) so tests can compute an unambiguous
    /// "would-be escape" target for a traversing `screen_path`
    /// (`../../marker` from `repo_root`) purely from the returned path,
    /// without ever probing real system directories.
    fn test_store_with_local() -> (ScreenStore, PathBuf) {
        let outer = tempdir_path("screen_store_local");
        let repo_root = outer.join("inner").join("local_repo");
        std::fs::create_dir_all(&repo_root).unwrap();
        std::fs::write(
            repo_root.join("byonk-screens.yaml"),
            "name: local\ndescription: d\nauthor: a\nlicense: MIT\n",
        )
        .unwrap();
        let manager = test_manager_with_screens_dir(&repo_root);

        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let renderer = Arc::new(RenderService::new(&asset_loader).unwrap());
        let shared_config: SharedConfig =
            Arc::new(arc_swap::ArcSwap::from(Arc::new(AppConfig::default())));
        let pipeline = Arc::new(
            ContentPipeline::new(shared_config, asset_loader, renderer, manager.clone()).unwrap(),
        );
        (ScreenStore::new(manager, pipeline), repo_root)
    }

    #[test]
    fn write_rejects_read_only_handle() {
        let (store, _repo_root) = test_store_with_local(); // helper: a manager whose 'local' is writable, builtin is not
        let err = store
            .write_file("byonk-builtin/default", "script.lua", b"x", None)
            .unwrap_err();
        assert!(matches!(err, StoreError::ReadOnly { .. }));
    }

    #[test]
    fn write_then_read_roundtrips_with_etag() {
        let (store, _repo_root) = test_store_with_local();
        store
            .write_file("local/clock", "script.lua", b"return {}", None)
            .unwrap();
        let f = store.read_file("local/clock", "script.lua").unwrap();
        assert_eq!(f.bytes, b"return {}");
        let e = f.etag.clone();
        // stale write is rejected
        let err = store
            .write_file("local/clock", "script.lua", b"new", Some("deadbeef"))
            .unwrap_err();
        assert!(matches!(err, StoreError::Conflict));
        // matching etag succeeds
        store
            .write_file("local/clock", "script.lua", b"new", Some(&e))
            .unwrap();
    }

    #[test]
    fn write_rejects_path_traversal() {
        let (store, _repo_root) = test_store_with_local();
        let err = store
            .write_file("local/clock", "../../etc/passwd", b"x", None)
            .unwrap_err();
        assert!(matches!(err, StoreError::Traversal));
    }

    #[test]
    fn write_rejects_traversal_in_screen_path_without_creating_escape_dir() {
        let (store, repo_root) = test_store_with_local();
        // `repo_root` is `outer/inner/local_repo`; `../../marker` from there
        // lexically resolves to `outer/marker` — compute that concretely so
        // we can assert nothing was ever created there.
        let outer = repo_root.parent().unwrap().parent().unwrap().to_path_buf();

        let err = store
            .write_file("local/../../marker", "x.txt", b"pwn", None)
            .unwrap_err();
        assert!(matches!(err, StoreError::Traversal));
        assert!(
            !outer.join("marker").exists(),
            "a traversing screen_path must be rejected before any directory is created"
        );
    }

    #[test]
    #[cfg(unix)]
    fn write_rejects_symlink_escape() {
        let (store, repo_root) = test_store_with_local();
        let outside = tempdir_path("screen_store_outside");
        std::fs::create_dir_all(&outside).unwrap();
        // Plant a symlink inside the writable root that resolves outside it.
        std::os::unix::fs::symlink(&outside, repo_root.join("evil")).unwrap();

        let err = store
            .write_file("local/evil", "x.txt", b"pwn", None)
            .unwrap_err();
        assert!(matches!(err, StoreError::Traversal));
        assert!(
            !outside.join("x.txt").exists(),
            "a symlink escape must be rejected before the file is written"
        );
    }

    #[test]
    fn write_with_if_match_against_deleted_file_is_conflict() {
        let (store, repo_root) = test_store_with_local();
        store
            .write_file("local/clock", "script.lua", b"return {}", None)
            .unwrap();
        let f = store.read_file("local/clock", "script.lua").unwrap();

        // Simulate the file having been deleted out from under the store
        // (e.g. a concurrent delete_screen, once Task 7 lands).
        std::fs::remove_file(repo_root.join("clock/script.lua")).unwrap();

        let err = store
            .write_file("local/clock", "script.lua", b"resurrected", Some(&f.etag))
            .unwrap_err();
        assert!(matches!(err, StoreError::Conflict));
    }

    #[test]
    fn write_rejects_too_large_payload() {
        let (store, _repo_root) = test_store_with_local();
        let huge = vec![0u8; MAX_FILE_BYTES + 1];
        let err = store
            .write_file("local/clock", "script.lua", &huge, None)
            .unwrap_err();
        assert!(matches!(err, StoreError::TooLarge));
    }

    #[test]
    fn read_file_flags_binary_vs_text_correctly() {
        let (store, _repo_root) = test_store_with_local();
        store
            .write_file(
                "local/clock",
                "logo.png",
                &[0xFF, 0xD8, 0xFF, 0x00, 0x01],
                None,
            )
            .unwrap();
        let f = store.read_file("local/clock", "logo.png").unwrap();
        assert!(f.binary);

        store
            .write_file("local/clock", "script.lua", b"return {}", None)
            .unwrap();
        let f = store.read_file("local/clock", "script.lua").unwrap();
        assert!(!f.binary);
    }

    // --- Task 7: create / copy / rename / delete -------------------------

    #[test]
    fn create_scaffolds_three_files_extending_base() {
        let (store, _repo_root) = test_store_with_local();
        let r = store
            .create_screen("local", "clock", StarterKind::Minimal)
            .unwrap();
        assert_eq!(r, "local/clock");
        let svg = store.read_file("local/clock", "screen.svg").unwrap();
        let svg_s = String::from_utf8(svg.bytes).unwrap();
        assert!(
            svg_s.contains("byonk-base-v1/base.svg"),
            "starter must extend the base library"
        );
        store.read_file("local/clock", "meta.yaml").unwrap();
        store.read_file("local/clock", "script.lua").unwrap();
    }

    #[test]
    fn create_rejects_existing_destination() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "dup", StarterKind::Minimal)
            .unwrap();
        let err = store
            .create_screen("local", "dup", StarterKind::Minimal)
            .unwrap_err();
        assert!(matches!(err, StoreError::Conflict));
    }

    #[test]
    fn copy_forks_read_only_screen_into_local() {
        let (store, _repo_root) = test_store_with_local();
        // byonk-builtin/default exists (embedded); copy it into local
        let r = store
            .copy_screen("byonk-builtin/default", "local", "my-default")
            .unwrap();
        assert_eq!(r, "local/my-default");
        store.read_file("local/my-default", "meta.yaml").unwrap();
        // The embedded `default` screen also ships a non-triple asset
        // (background.jpg); copy_screen must carry it over too, not just
        // the meta/script/svg triple.
        let bg = store
            .read_file("local/my-default", "background.jpg")
            .unwrap();
        assert!(bg.binary);
    }

    #[test]
    fn copy_rejects_existing_destination() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "dup", StarterKind::Minimal)
            .unwrap();
        let err = store
            .copy_screen("byonk-builtin/default", "local", "dup")
            .unwrap_err();
        assert!(matches!(err, StoreError::Conflict));
    }

    #[test]
    fn copy_preserves_non_triple_files() {
        let (store, _repo_root) = test_store_with_local();
        store
            .write_file("local/photo", "meta.yaml", b"title: P\n", None)
            .unwrap();
        store
            .write_file("local/photo", "script.lua", b"return {}", None)
            .unwrap();
        store
            .write_file("local/photo", "screen.svg", b"<svg/>", None)
            .unwrap();
        store
            .write_file("local/photo", "assets/logo.png", &[0xFF, 0xD8, 0xFF], None)
            .unwrap();

        let r = store.copy_screen("local/photo", "local", "photo2").unwrap();
        assert_eq!(r, "local/photo2");
        let logo = store.read_file("local/photo2", "assets/logo.png").unwrap();
        assert_eq!(logo.bytes, vec![0xFF, 0xD8, 0xFF]);
    }

    #[test]
    fn rename_and_delete_roundtrip() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "a", StarterKind::Minimal)
            .unwrap();
        store.rename_screen("local/a", "b").unwrap();
        assert!(store.read_file("local/a", "meta.yaml").is_err());
        store.read_file("local/b", "meta.yaml").unwrap();
        store.delete_screen("local/b").unwrap();
        assert!(store.read_file("local/b", "meta.yaml").is_err());
    }

    #[test]
    fn rename_rejects_existing_destination() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "a", StarterKind::Minimal)
            .unwrap();
        store
            .create_screen("local", "b", StarterKind::Minimal)
            .unwrap();
        let err = store.rename_screen("local/a", "b").unwrap_err();
        assert!(matches!(err, StoreError::Conflict));
    }

    #[test]
    fn rename_and_delete_reject_read_only_handle() {
        let (store, _repo_root) = test_store_with_local();
        let err = store
            .rename_screen("byonk-builtin/default", "x")
            .unwrap_err();
        assert!(matches!(err, StoreError::ReadOnly { .. }));
        let err = store.delete_screen("byonk-builtin/default").unwrap_err();
        assert!(matches!(err, StoreError::ReadOnly { .. }));
    }

    #[test]
    fn structural_ops_reject_traversal_before_any_mutation() {
        let (store, repo_root) = test_store_with_local();
        let outer = repo_root.parent().unwrap().parent().unwrap().to_path_buf();

        let err = store
            .create_screen("local", "../../marker", StarterKind::Minimal)
            .unwrap_err();
        assert!(matches!(err, StoreError::Traversal));

        let err = store
            .copy_screen("byonk-builtin/default", "local", "../../marker")
            .unwrap_err();
        assert!(matches!(err, StoreError::Traversal));

        store
            .create_screen("local", "a", StarterKind::Minimal)
            .unwrap();
        let err = store.rename_screen("local/a", "../../marker").unwrap_err();
        assert!(matches!(err, StoreError::Traversal));

        let err = store.delete_screen("local/../../marker").unwrap_err();
        assert!(matches!(err, StoreError::Traversal));

        assert!(
            !outer.join("marker").exists(),
            "a traversing ref must be rejected before any directory is created"
        );
    }

    #[test]
    #[cfg(unix)]
    fn delete_refuses_to_remove_repo_root_via_symlink() {
        let (store, repo_root) = test_store_with_local();
        // Plant a symlink inside the repo that resolves back to the repo
        // root itself — the highest-consequence possible target for
        // `delete_screen`'s `remove_dir_all`.
        std::os::unix::fs::symlink(&repo_root, repo_root.join("selfloop")).unwrap();

        let err = store.delete_screen("local/selfloop").unwrap_err();
        assert!(matches!(err, StoreError::Traversal));
        assert!(repo_root.exists(), "repo root must not be deleted");
        assert!(
            repo_root.join("byonk-screens.yaml").exists(),
            "repo root contents must survive"
        );
    }

    #[test]
    #[cfg(unix)]
    fn rename_rejects_symlink_escape_on_source() {
        // Reaches `rename_screen`'s `!canon_old.starts_with(&canon_base)`
        // branch specifically: `screen_path` itself ("evil") is lexically
        // clean (passes `safe_rel`), but resolves via symlink to outside
        // the writable root.
        let (store, repo_root) = test_store_with_local();
        let outside = tempdir_path("screen_store_rename_outside_src");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, repo_root.join("evil")).unwrap();

        let err = store.rename_screen("local/evil", "renamed").unwrap_err();
        assert!(matches!(err, StoreError::Traversal));
        assert!(!repo_root.join("renamed").exists());
        assert!(!outside.join("renamed").exists());
    }

    #[test]
    #[cfg(unix)]
    fn rename_rejects_symlink_escape_on_destination_ancestor() {
        // Reaches the destination-side guard (the
        // `deepest_existing_ancestor(new_parent)` check): the source ("a")
        // is legitimate, but the new name's leading segment ("linked") is a
        // symlink resolving outside the writable root, even though the
        // full destination path doesn't exist yet.
        let (store, repo_root) = test_store_with_local();
        store
            .create_screen("local", "a", StarterKind::Minimal)
            .unwrap();

        let outside = tempdir_path("screen_store_rename_outside_dst");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, repo_root.join("linked")).unwrap();

        let err = store
            .rename_screen("local/a", "linked/newname")
            .unwrap_err();
        assert!(matches!(err, StoreError::Traversal));
        assert!(!outside.join("newname").exists());
        // The source must be untouched — the rename must never have executed.
        store.read_file("local/a", "meta.yaml").unwrap();
    }

    /// A `ScreenRepoSource` stub whose `screen_files` reports an entry
    /// outside its own screen dir (`..`) — simulating a future/buggy
    /// `ScreenRepoSource` impl, since `screen_files` has no default and
    /// every implementor decides its own contents. `read` deliberately
    /// does NOT reject the traversing path itself (unlike every real
    /// source's `is_safe_rel` check), so this isolates
    /// `copy_screen_files`'s own `safe_rel(suffix)` guard as the thing
    /// that must catch it.
    struct TraversalStubSource;
    impl ScreenRepoSource for TraversalStubSource {
        fn read(&self, rel: &str) -> Option<Vec<u8>> {
            match rel {
                "s/../../evil.txt" => Some(b"pwn".to_vec()),
                _ => None,
            }
        }
        fn screen_paths(&self) -> Vec<String> {
            vec!["s".to_string()]
        }
        fn svg_files(&self) -> Vec<String> {
            vec![]
        }
        fn manifest(&self) -> &crate::models::screen_repo_manifest::ScreenRepoManifest {
            unreachable!("manifest() not used by copy_screen_files")
        }
        fn screen_files(&self, _screen_path: &str) -> Vec<String> {
            vec!["s/../../evil.txt".to_string()]
        }
    }

    #[test]
    fn copy_screen_files_rejects_traversing_source_entry() {
        let (_store, repo_root) = test_store_with_local();
        let to_base = repo_root.clone();
        let to_dir = to_base.join("dest");
        let src = TraversalStubSource;

        let err = ScreenStore::copy_screen_files(&src, "s", &to_base, &to_dir).unwrap_err();
        assert!(matches!(err, StoreError::Traversal));

        let outer = repo_root.parent().unwrap().parent().unwrap();
        assert!(
            !outer.join("evil.txt").exists(),
            "a traversing screen_files entry must be rejected before any write"
        );
        assert!(!to_dir.exists());
    }

    #[test]
    fn copy_carries_screens_dir_overlay_assets_missed_by_list_screens_extension_filter() {
        // `AssetLoader::list_screens()`'s overlay branch
        // (`collect_screen_files`) only picks up `.lua`/`.svg`/`.yaml`
        // files. Embedded assets are unfiltered (rust-embed includes image
        // globs), which is why `copy_forks_read_only_screen_into_local`'s
        // `background.jpg` assertion above passes even without this test —
        // but a screen living under `SCREENS_DIR` (the HA add-on's primary
        // layout, also visible under the `byonk-builtin` handle) must not
        // silently lose its non-lua/svg/yaml assets when copied.
        let screens_dir = tempdir_path("screen_store_overlay_screens_dir");
        std::fs::create_dir_all(screens_dir.join("myscreen")).unwrap();
        std::fs::write(
            screens_dir.join("myscreen/meta.yaml"),
            "title: M\ndescription: d\nbyonk: \"0.15\"\n",
        )
        .unwrap();
        std::fs::write(
            screens_dir.join("myscreen/script.lua"),
            "return { data = {} }\n",
        )
        .unwrap();
        std::fs::write(screens_dir.join("myscreen/screen.svg"), "<svg/>\n").unwrap();
        std::fs::write(
            screens_dir.join("myscreen/photo.png"),
            [0x89, b'P', b'N', b'G'],
        )
        .unwrap();

        let local_repo = tempdir_path("screen_store_overlay_local_repo");
        std::fs::create_dir_all(&local_repo).unwrap();
        std::fs::write(
            local_repo.join("byonk-screens.yaml"),
            "name: local\ndescription: d\nauthor: a\nlicense: MIT\n",
        )
        .unwrap();

        let asset_loader = Arc::new(AssetLoader::new(Some(screens_dir.clone()), None, None));
        let mut screen_repos = std::collections::HashMap::new();
        screen_repos.insert(
            "local".to_string(),
            crate::models::config::ScreenRepoRef {
                path: Some(local_repo.display().to_string()),
                ..Default::default()
            },
        );
        let shared_config: SharedConfig = Arc::new(arc_swap::ArcSwap::from(Arc::new(AppConfig {
            screen_repos,
            ..AppConfig::default()
        })));
        let cache = crate::services::screen_repo_cache::ScreenRepoCache::new(tempdir_path(
            "screen_store_overlay_cache",
        ));
        let manager = crate::services::screen_repo_manager::ScreenRepoManager::new(
            asset_loader.clone(),
            shared_config.clone(),
            cache,
            std::collections::HashMap::new(),
            None,
        );
        let renderer = Arc::new(RenderService::new(&asset_loader).unwrap());
        let pipeline = Arc::new(
            ContentPipeline::new(shared_config, asset_loader, renderer, manager.clone()).unwrap(),
        );
        let store = ScreenStore::new(manager, pipeline);

        let r = store
            .copy_screen("byonk-builtin/myscreen", "local", "myscreen2")
            .unwrap();
        assert_eq!(r, "local/myscreen2");
        let photo = store.read_file("local/myscreen2", "photo.png").unwrap();
        assert!(photo.binary);
    }

    #[test]
    fn write_then_read_roundtrips_when_manifest_has_nondefault_root() {
        let outer = tempdir_path("screen_store_nondefault_root");
        std::fs::create_dir_all(outer.join("contrib/trmnl")).unwrap();
        std::fs::write(
            outer.join("byonk-screens.yaml"),
            "name: local\ndescription: d\nauthor: a\nlicense: MIT\nroot: contrib/trmnl\n",
        )
        .unwrap();
        let manager = test_manager_with_screens_dir(&outer);

        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let renderer = Arc::new(RenderService::new(&asset_loader).unwrap());
        let shared_config: SharedConfig =
            Arc::new(arc_swap::ArcSwap::from(Arc::new(AppConfig::default())));
        let pipeline = Arc::new(
            ContentPipeline::new(shared_config, asset_loader, renderer, manager.clone()).unwrap(),
        );
        let store = ScreenStore::new(manager, pipeline);

        store
            .write_file("local/clock", "script.lua", b"return {}", None)
            .unwrap();
        let f = store.read_file("local/clock", "script.lua").unwrap();
        assert_eq!(f.bytes, b"return {}");

        // The write must land under the manifest root (`contrib/trmnl`), not
        // the bare repo root — otherwise `read_file` (which resolves through
        // `manifest_root`) could never see what `write_file` just wrote.
        assert!(outer.join("contrib/trmnl/clock/script.lua").exists());
        assert!(!outer.join("clock/script.lua").exists());
    }
}
