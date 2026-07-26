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
        let parent = target.parent().ok_or_else(|| {
            StoreError::Io(format!(
                "cannot determine parent directory of {}",
                target.display()
            ))
        })?;

        // canonicalize-then-verify-prefix guard (defends against symlink
        // escape): verify BEFORE touching the filesystem — never
        // `create_dir_all` first and check after. `base` must already exist
        // (`LocalScreenRepoSource::load` had to read it to load the
        // manifest before this handle was ever registered as writable), so
        // if it fails to canonicalize now, the writable root was deleted or
        // replaced out from under a stale loader snapshot; treat that as a
        // hard failure rather than silently skipping the guard.
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

        // Atomic tmp+rename in the same dir. The tmp name is unique per
        // write (pid + random suffix appended, not substituted via
        // `with_extension`) so concurrent writes to sibling files that share
        // a stem-less extension swap (e.g. `script.lua` and `script.svg`)
        // never stage through the same path, and an aborted write never
        // leaves a fixed, collidable `*.byonk-tmp` name behind.
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
        std::fs::rename(&tmp, &target).map_err(|e| StoreError::Io(e.to_string()))?;
        self.manager.rebuild_loader();
        Ok(etag(bytes))
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
