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
    /// `if_match` didn't match the file's current etag.
    Conflict,
    /// `file` escapes the screen's directory (`..`, absolute, symlink escape).
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

    fn split_ref(screen_ref: &str) -> Result<(&str, &str), StoreError> {
        screen_ref.split_once('/').ok_or(StoreError::NotFound)
    }

    /// Resolve the writable root for a screen_ref's handle, or a `ReadOnly`
    /// error naming the handle + a hint to copy into a writable one.
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
        let dir = self.writable_dir(handle, screen_path)?;
        let target = dir.join(&rel);

        // canonicalize-then-verify-prefix guard (defends against symlink escape)
        let base = self.writable_dir(handle, "")?; // repo root
        let parent = target.parent().ok_or_else(|| {
            StoreError::Io(format!(
                "cannot determine parent directory of {}",
                target.display()
            ))
        })?;
        std::fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
        if let Ok(canon_base) = base.canonicalize() {
            let canon_parent = parent
                .canonicalize()
                .map_err(|e| StoreError::Io(e.to_string()))?;
            if !canon_parent.starts_with(&canon_base) {
                return Err(StoreError::Traversal);
            }
        }
        // else: repo root doesn't exist yet (first write ever) — nothing to
        // canonicalize against; `create_dir_all` above already staked out
        // `parent` under the (uncanonicalized) writable root computed by
        // `writable_dir`, which already rejects traversal via `safe_rel`.

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
    /// read-only embedded `byonk-builtin` (always present).
    fn test_store_with_local() -> ScreenStore {
        let dir = tempdir_path("screen_store_local");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("byonk-screens.yaml"),
            "name: local\ndescription: d\nauthor: a\nlicense: MIT\n",
        )
        .unwrap();
        let manager = test_manager_with_screens_dir(&dir);

        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let renderer = Arc::new(RenderService::new(&asset_loader).unwrap());
        let shared_config: SharedConfig =
            Arc::new(arc_swap::ArcSwap::from(Arc::new(AppConfig::default())));
        let pipeline = Arc::new(
            ContentPipeline::new(shared_config, asset_loader, renderer, manager.clone()).unwrap(),
        );
        ScreenStore::new(manager, pipeline)
    }

    #[test]
    fn write_rejects_read_only_handle() {
        let store = test_store_with_local(); // helper: a manager whose 'local' is writable, builtin is not
        let err = store
            .write_file("byonk-builtin/default", "script.lua", b"x", None)
            .unwrap_err();
        assert!(matches!(err, StoreError::ReadOnly { .. }));
    }

    #[test]
    fn write_then_read_roundtrips_with_etag() {
        let store = test_store_with_local();
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
        let store = test_store_with_local();
        let err = store
            .write_file("local/clock", "../../etc/passwd", b"x", None)
            .unwrap_err();
        assert!(matches!(err, StoreError::Traversal));
    }
}
