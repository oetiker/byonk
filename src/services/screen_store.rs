//! `ScreenStore` — the sole owner of screen mutation (read/write authored
//! screen files), sitting beside the read-only `AssetLoader`/`ScreenRepoLoader`
//! resolution path. Only handles registered as writable
//! (`LocalScreenRepoSource`, via `ScreenRepoSource::writable_root`) can be
//! written to; everything else (the embedded `byonk-builtin`, git-fetched
//! screen repos) is read-only and returns `StoreError::ReadOnly` with a
//! copy-hint pointing at the caller's next step (Task 7's `copy_screen`).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::models::screen_meta::ScreenMeta;
use crate::models::{normalize_algorithm_name, DisplaySpec, DitherTuningValues};
use crate::services::content_pipeline::{ContentPipeline, DeviceContext};
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

/// How serious a `validate` finding is. Only `Error` is emitted today (by
/// `validate`'s three checks below); `Warning` exists for Task 9's render
/// diagnostics and the MCP layer to use once they have something
/// non-fatal to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// One `validate` finding: `location` is the screen-relative file the issue
/// was found in (e.g. `"script.lua"`), `message` is human-readable detail
/// (e.g. the mlua/tera/serde_yaml error text).
#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    pub location: String,
    pub message: String,
}

/// The result of `ScreenStore::validate`: `ok` is true iff `issues` contains
/// no `Severity::Error` (a `Warning`-only report is still `ok`).
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub ok: bool,
    pub issues: Vec<Issue>,
}

/// Options for `ScreenStore::render`. Mirrors the knobs `/dev/render`
/// (`crate::api::dev::handle_render`) accepts for a screen-name (non-MAC)
/// render: no device lookup, no custom Lua params — an authoring-time
/// preview of the screen as it stands on disk.
///
/// `model` selects the default width/height/palette when `width`/`height`
/// aren't given (`"og"` → 800x480 4-grey, anything else, including `"x"`,
/// follows `/dev/render`'s own model dispatch). `panel` names a profile
/// from the `panels:` config section (colors + measured colors + dither
/// tuning); unresolvable names fall back to the model default, same as
/// `/dev/render`.
#[derive(Debug, Clone)]
pub struct RenderOpts {
    pub model: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub panel: Option<String>,
    pub dither: Option<String>,
    pub error_clamp: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub noise_scale: Option<f32>,
    pub preserve_exact: Option<bool>,
    pub timestamp: Option<i64>,
    /// Also render a pre-dither, full-color PNG alongside the palette-
    /// restricted `png` (see `RenderResult::raw_png`).
    pub include_raw: bool,
}

impl Default for RenderOpts {
    /// `"og"` (800x480, 4-grey), matching `/dev/render`'s own default model
    /// — the common case, and the one every starter screen renders under.
    fn default() -> Self {
        Self {
            model: "og".to_string(),
            width: None,
            height: None,
            panel: None,
            dither: None,
            error_clamp: None,
            chroma_clamp: None,
            noise_scale: None,
            preserve_exact: None,
            timestamp: None,
            include_raw: false,
        }
    }
}

/// A line-numbered rendering failure. Not Lua-specific: a Tera/SVG failure,
/// a dithering failure, or an unresolvable screen ref all populate this
/// (with `line: None` where there's no line to report), rather than a
/// separate error type per failure mode.
#[derive(Debug, Clone)]
pub struct RenderError {
    pub line: Option<u32>,
    pub message: String,
}

/// The result of `ScreenStore::render`: the PNG an author's edit produces,
/// plus the diagnostics they (often an LLM) need to debug it without a
/// separate round trip — captured `log_*` output, the script's returned
/// `data` table, and (on failure) a line-numbered error. `error.is_some()`
/// implies `png` is empty; it is never a panic or a silently-empty PNG.
#[derive(Debug, Clone)]
pub struct RenderResult {
    pub png: Vec<u8>,
    /// Pre-dither, full-color PNG — only populated when `RenderOpts::include_raw`.
    pub raw_png: Option<Vec<u8>>,
    pub log: Vec<String>,
    pub data: serde_json::Value,
    pub refresh_rate: u32,
    pub error: Option<RenderError>,
}

/// Best-effort line-number extraction from an mlua error message shape like
/// `runtime error: [string "..."]:12: attempt to index a nil value` or
/// `syntax error: [string "..."]:1: unexpected symbol near <eof>` (both as
/// wrapped by `ContentError`'s `Display`, e.g. `"Script error: Lua error:
/// runtime error: [string \"...\"]:12: ..."`). Looks for the first `]:`
/// marker and reads the digits immediately after it; returns `None` when
/// the message doesn't have that shape (a non-Lua render failure, or a
/// Lua message mlua didn't attach a location to).
fn parse_lua_error_line(message: &str) -> Option<u32> {
    let idx = message.find("]:")?;
    let rest = &message[idx + 2..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
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
    /// Used by `validate` (its `TemplateService` for SVG/include
    /// registration checks) and by `render` (resolves screens through its
    /// own `ScreenRepoManager` — see `ScreenStore::new`'s doc comment for
    /// why that manager must be the same `Arc` as `manager` above).
    pipeline: Arc<ContentPipeline>,
}

impl ScreenStore {
    /// `manager` and `pipeline` must share the *same* underlying
    /// `ScreenRepoManager` — `render` resolves screens through
    /// `pipeline`'s manager, while every other method here (`read_file`,
    /// `write_file`, `create_screen`, ...) resolves through `manager`
    /// directly. If the caller ever constructs `pipeline` from a
    /// *different* `ScreenRepoManager` than `manager` (e.g. two separately
    /// loaded managers pointed at the same config), reads/writes and
    /// renders would silently disagree about what a `screen_ref` resolves
    /// to. Nothing in the type system enforces this — the production
    /// wiring that constructs both (Task 13) must pass the same `Arc`.
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

    /// Statically validate one screen: parse `meta.yaml` against its schema,
    /// compile `script.lua` without executing it, and check `screen.svg`
    /// compiles as a Tera template with its `{% extends %}` chain resolving
    /// (`TemplateService::validate_template`). A missing file is an Error
    /// issue located at that filename, not a panic or a silently-ok report.
    ///
    /// A read operation, not a write: works against read-only handles
    /// (embedded `byonk-builtin`, git-fetched repos) too, unlike
    /// `write_file`/the structural ops above.
    pub fn validate(&self, screen_ref: &str) -> ValidationReport {
        let (handle, screen_path) = match Self::split_ref(screen_ref) {
            Ok(v) => v,
            Err(_) => {
                return ValidationReport {
                    ok: false,
                    issues: vec![Issue {
                        severity: Severity::Error,
                        location: screen_ref.to_string(),
                        message: "invalid screen reference".to_string(),
                    }],
                }
            }
        };
        let loader = self.manager.loader();
        let Some(source) = loader.source_for(handle) else {
            return ValidationReport {
                ok: false,
                issues: vec![Issue {
                    severity: Severity::Error,
                    location: screen_ref.to_string(),
                    message: format!("unknown screen repo '{handle}'"),
                }],
            };
        };

        let mut issues = Vec::new();

        // meta.yaml — schema check only.
        match source.read_string(&format!("{screen_path}/meta.yaml")) {
            Some(text) => {
                if let Err(message) = ScreenMeta::from_yaml(&text) {
                    issues.push(Issue {
                        severity: Severity::Error,
                        location: "meta.yaml".to_string(),
                        message,
                    });
                }
            }
            None => issues.push(Issue {
                severity: Severity::Error,
                location: "meta.yaml".to_string(),
                message: "file not found".to_string(),
            }),
        }

        // script.lua — compile without executing.
        match source.read_string(&format!("{screen_path}/script.lua")) {
            Some(text) => {
                if let Err(e) = mlua::Lua::new().load(&text).into_function() {
                    issues.push(Issue {
                        severity: Severity::Error,
                        location: "script.lua".to_string(),
                        message: e.to_string(),
                    });
                }
            }
            None => issues.push(Issue {
                severity: Severity::Error,
                location: "script.lua".to_string(),
                message: "file not found".to_string(),
            }),
        }

        // screen.svg — Tera compile + extends-chain resolution, mirroring
        // the registration phase real rendering runs (see
        // `TemplateService::validate_template`'s doc comment for what this
        // does and doesn't catch).
        match source.read_string(&format!("{screen_path}/screen.svg")) {
            Some(text) => {
                if let Err(e) =
                    self.pipeline
                        .template_service()
                        .validate_template(&text, &source, screen_path)
                {
                    issues.push(Issue {
                        severity: Severity::Error,
                        location: "screen.svg".to_string(),
                        message: e.to_string(),
                    });
                }
            }
            None => issues.push(Issue {
                severity: Severity::Error,
                location: "screen.svg".to_string(),
                message: "file not found".to_string(),
            }),
        }

        let ok = !issues.iter().any(|i| matches!(i.severity, Severity::Error));
        ValidationReport { ok, issues }
    }

    /// Render one screen with authoring diagnostics: runs `script.lua`,
    /// renders `screen.svg` through it, and dithers to PNG — the same
    /// pipeline `/dev/render` uses for a screen-name (non-MAC) preview, with
    /// `RenderOpts` mapped onto the same knobs. Any failure along the way
    /// (an unresolvable ref, a Lua error, a Tera/SVG failure — including a
    /// dangling `{% include %}` target `validate` can't catch, since Tera
    /// resolves includes at render time — or a dithering failure) is
    /// returned in `RenderResult.error`, never a panic or a silently-empty
    /// PNG.
    ///
    /// A read operation, like `validate`: works against read-only handles
    /// (embedded `byonk-builtin`, git-fetched repos) too.
    pub fn render(&self, screen_ref: &str, opts: RenderOpts) -> RenderResult {
        let empty = || RenderResult {
            png: Vec::new(),
            raw_png: None,
            log: Vec::new(),
            data: serde_json::Value::Null,
            refresh_rate: 0,
            error: None,
        };

        // Dimensions + base palette: explicit override, else the model's
        // default — the same option-resolution chain `/dev/render` uses
        // (`crate::api::display::resolve_preview_dimensions`/
        // `resolve_query_palette`), shared so the two previews can't drift.
        let (width, height) =
            crate::api::display::resolve_preview_dimensions(&opts.model, opts.width, opts.height);
        let query_palette: Vec<(u8, u8, u8)> =
            crate::api::display::resolve_query_palette(&opts.model, None);

        let config = self.pipeline.config().load();
        let panel = opts
            .panel
            .as_deref()
            .and_then(|name| config.get_panel(name));
        let panel_colors: Option<String> = panel.map(|p| p.colors.clone());
        let measured_colors: Option<Vec<(u8, u8, u8)>> = panel
            .and_then(|p| p.colors_actual.as_deref())
            .map(crate::api::display::parse_colors_header);

        // The palette the script sees via `device.colors` — panel colors
        // fold in over the model default, same as `/dev/render`'s
        // `default_palette` (there is no device-config layer in an
        // authoring render, so that step of the chain is always absent).
        let ctx_palette: Vec<(u8, u8, u8)> =
            crate::api::display::resolve_ctx_palette(None, panel_colors.as_deref(), &query_palette);

        // Pre-script dither algorithm + tuning, for the device context the
        // script sees via `device.dither.*` — same resolution `/dev/render`
        // does before running the script.
        let pre_script_algo = opts.dither.as_deref().unwrap_or("atkinson");
        let panel_dither_config = panel.and_then(|p| p.dither.clone());
        let pre_panel_tuning = panel_dither_config
            .as_ref()
            .map(|pdc| pdc.resolve_for_algorithm(Some(pre_script_algo)))
            .unwrap_or_default();

        let device_ctx = DeviceContext {
            mac: "dev-simulator".to_string(),
            battery_voltage: Some(4.2),
            rssi: Some(-50),
            model: Some(opts.model.clone()),
            firmware_version: Some("dev".to_string()),
            width: Some(width),
            height: Some(height),
            colors: Some(crate::api::display::colors_to_hex_strings(&ctx_palette)),
            dither_algorithm: Some(pre_script_algo.to_string()),
            dither_error_clamp: pre_panel_tuning.error_clamp,
            dither_noise_scale: pre_panel_tuning.noise_scale,
            dither_chroma_clamp: pre_panel_tuning.chroma_clamp,
            dither_strength: pre_panel_tuning.strength,
            ..Default::default()
        };

        // Caller-owned log sink: passed *into* `run_script_direct` (down
        // through `run_resolved`/`LuaRuntime::run_script`) so a Lua error —
        // which short-circuits before `ScriptResult` is built — still
        // leaves every `log_*` call made before the failure readable here,
        // in the error branch below. See `LuaRuntime::run_script`'s doc
        // comment for why this has to be caller-owned rather than read out
        // of `ScriptResult` after the fact.
        let log_sink: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        // Run the script, capturing logs/data/refresh_rate. `run_script_direct`
        // is the same entry point `/dev/render` uses for a screen-name preview.
        let script_result = match self.pipeline.run_script_direct(
            screen_ref,
            HashMap::new(),
            Some(device_ctx.clone()),
            opts.timestamp,
            Some(&log_sink),
        ) {
            Ok(r) => r,
            Err(message) => {
                let log = log_sink.lock().map(|g| g.clone()).unwrap_or_default();
                return RenderResult {
                    log,
                    error: Some(RenderError {
                        line: parse_lua_error_line(&message),
                        message,
                    }),
                    ..empty()
                };
            }
        };

        let log = script_result.logs.clone();
        let data = script_result.data.clone();
        let refresh_rate = script_result.refresh_rate;

        let svg = match self
            .pipeline
            .render_svg_from_script(&script_result, Some(&device_ctx))
        {
            Ok(s) => s,
            Err(e) => {
                // Tera/SVG failure, not Lua — no line number to report (see
                // `RenderError`'s doc comment / binding resolution #2).
                return RenderResult {
                    log,
                    data,
                    refresh_rate,
                    error: Some(RenderError {
                        line: None,
                        message: e.to_string(),
                    }),
                    ..empty()
                };
            }
        };

        // Explicit render-option dither wins outright over the script's own
        // choice (like a `/dev/render` UI override); otherwise the script's
        // `dither` return value is used. Mirrors `effective_script_dither`/
        // `effective_device_dither` in `handle_render` (there is no device
        // config layer in an authoring render, so that layer is always empty).
        let (effective_script_dither, dither_override): (Option<&str>, Option<&str>) =
            match opts.dither.as_deref() {
                Some(d) => (None, Some(d)),
                None => (script_result.script_dither.as_deref(), None),
            };

        let effective_algo = dither_override.or(effective_script_dither);
        let panel_tuning = panel_dither_config
            .as_ref()
            .map(|pdc| {
                pdc.resolve_for_algorithm(effective_algo.map(normalize_algorithm_name).as_deref())
            })
            .unwrap_or_default();

        let opts_tuning = DitherTuningValues {
            error_clamp: opts.error_clamp,
            noise_scale: opts.noise_scale,
            chroma_clamp: opts.chroma_clamp,
            strength: None,
        };
        let script_tuning = DitherTuningValues {
            error_clamp: script_result.script_error_clamp,
            noise_scale: script_result.script_noise_scale,
            chroma_clamp: script_result.script_chroma_clamp,
            strength: script_result.script_strength,
        };
        let tuning = crate::api::display::resolve_effective_tuning(
            &opts_tuning,
            &script_tuning,
            &DitherTuningValues::default(),
            &panel_tuning,
        );

        let render_params = crate::api::display::resolve_render_params(
            script_result.script_colors.as_deref(),
            effective_script_dither,
            script_result.script_preserve_exact,
            None,
            dither_override,
            panel_colors.as_deref(),
            &query_palette,
            measured_colors.clone(),
            opts.preserve_exact,
            &tuning,
        );

        let display_spec = DisplaySpec::from_dimensions(width, height).unwrap_or(DisplaySpec::OG);
        let use_actual = measured_colors.is_some();
        let (dither_tuning, has_tuning) =
            crate::api::display::resolve_dither_tuning(&render_params);

        let png = match self.pipeline.render_png_from_svg(
            &svg,
            display_spec,
            &render_params.palette,
            measured_colors.as_deref(),
            use_actual,
            render_params.dither.as_deref(),
            render_params.preserve_exact,
            has_tuning.then_some(&dither_tuning),
        ) {
            Ok(bytes) => bytes,
            Err(e) => {
                // Dither/PNG-encode failure, not Lua — no line number (see
                // `render_svg_from_script`'s error branch above).
                return RenderResult {
                    log,
                    data,
                    refresh_rate,
                    error: Some(RenderError {
                        line: None,
                        message: e.to_string(),
                    }),
                    ..empty()
                };
            }
        };

        let raw_png = if opts.include_raw {
            self.pipeline
                .render_raw_png_from_svg(&svg, display_spec)
                .ok()
        } else {
            None
        };

        RenderResult {
            png,
            raw_png,
            log,
            data,
            refresh_rate,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetLoader;
    use crate::models::config::PanelConfig;
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
        test_store_with_local_and_config(AppConfig::default())
    }

    /// Like `test_store_with_local`, but with a caller-supplied `AppConfig`
    /// (e.g. one with `panels` populated, for tests that need `render`'s
    /// panel resolution).
    fn test_store_with_local_and_config(config: AppConfig) -> (ScreenStore, PathBuf) {
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
        let shared_config: SharedConfig = Arc::new(arc_swap::ArcSwap::from(Arc::new(config)));
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

    // --- Task 8: validate -------------------------------------------------

    #[test]
    fn validate_flags_lua_syntax_error() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "bad", StarterKind::Minimal)
            .unwrap();
        store
            .write_file("local/bad", "script.lua", b"return {", None)
            .unwrap(); // unbalanced
        let rep = store.validate("local/bad");
        assert!(!rep.ok);
        assert!(rep.issues.iter().any(|i| i.location.contains("script.lua")));
    }

    #[test]
    fn validate_passes_for_starter() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "ok", StarterKind::Minimal)
            .unwrap();
        assert!(store.validate("local/ok").ok);
    }

    #[test]
    fn validate_flags_missing_files_by_location_not_panic() {
        let (store, repo_root) = test_store_with_local();
        // A screen directory that exists but has none of the three files.
        std::fs::create_dir_all(repo_root.join("empty")).unwrap();
        let rep = store.validate("local/empty");
        assert!(!rep.ok);
        assert!(rep.issues.iter().any(|i| i.location == "meta.yaml"));
        assert!(rep.issues.iter().any(|i| i.location == "script.lua"));
        assert!(rep.issues.iter().any(|i| i.location == "screen.svg"));
        assert!(rep
            .issues
            .iter()
            .all(|i| matches!(i.severity, Severity::Error)));
    }

    #[test]
    fn validate_flags_meta_schema_error() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "badmeta", StarterKind::Minimal)
            .unwrap();
        // Missing required `title` field.
        store
            .write_file(
                "local/badmeta",
                "meta.yaml",
                b"description: d\nbyonk: \"0.15\"\n",
                None,
            )
            .unwrap();
        let rep = store.validate("local/badmeta");
        assert!(!rep.ok);
        assert!(rep.issues.iter().any(|i| i.location == "meta.yaml"));
    }

    #[test]
    fn validate_flags_svg_missing_extends_target() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "badsvg", StarterKind::Minimal)
            .unwrap();
        store
            .write_file(
                "local/badsvg",
                "screen.svg",
                b"{% extends \"byonk-base-v1/does-not-exist.svg\" %}",
                None,
            )
            .unwrap();
        let rep = store.validate("local/badsvg");
        assert!(!rep.ok);
        assert!(rep.issues.iter().any(|i| i.location == "screen.svg"));
    }

    #[test]
    fn validate_works_for_read_only_source() {
        // validate is a read operation — it must work against the embedded
        // (read-only) byonk-builtin handle, not just writable repos.
        let (store, _repo_root) = test_store_with_local();
        let rep = store.validate("byonk-builtin/default");
        assert!(rep.ok, "{:?}", rep.issues);
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

    // --- Task 9: render -----------------------------------------------------

    #[test]
    fn render_returns_png_data_and_log() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "r", StarterKind::Minimal)
            .unwrap();
        store
            .write_file(
                "local/r",
                "script.lua",
                b"log_info(\"hi\")\nreturn { data = { message = \"X\" } }",
                None,
            )
            .unwrap();
        let res = store.render("local/r", RenderOpts::default());
        assert!(res.error.is_none(), "{:?}", res.error);
        assert!(!res.png.is_empty());
        assert_eq!(res.data["message"], "X");
        assert!(res.log.iter().any(|l| l.contains("hi")));
    }

    #[test]
    fn render_reports_lua_error_with_message() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "e", StarterKind::Minimal)
            .unwrap();
        store
            .write_file("local/e", "script.lua", b"error(\"boom\")", None)
            .unwrap();
        let res = store.render("local/e", RenderOpts::default());
        assert!(res.error.as_ref().unwrap().message.contains("boom"));
    }

    #[test]
    fn render_error_still_returns_logs_from_before_the_failure() {
        // Regression for the caller-owned log sink: a script's `log_info`
        // calls before it errors are the single most useful diagnostic for
        // debugging that failure, and must not be lost just because
        // `run_script`'s `?` short-circuits before `ScriptResult` (and its
        // `logs` field) is built. If this reverts to reading logs only out
        // of `ScriptResult` on the `Ok` path, `res.log` goes back to `[]`
        // here.
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "logthenerror", StarterKind::Minimal)
            .unwrap();
        store
            .write_file(
                "local/logthenerror",
                "script.lua",
                b"log_info(\"before the crash\")\nerror(\"boom\")",
                None,
            )
            .unwrap();
        let res = store.render("local/logthenerror", RenderOpts::default());
        assert!(res.error.is_some(), "expected the script error to surface");
        assert!(
            res.log.iter().any(|l| l.contains("before the crash")),
            "expected pre-failure log lines to survive the error, got: {:?}",
            res.log
        );
    }

    #[test]
    fn render_reports_lua_error_with_line_number() {
        // `error("boom")` on line 2 of the script (line 1 is a comment) —
        // the mlua runtime error carries a line number, and `render` must
        // surface it rather than leaving `RenderError.line` unparsed.
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "e2", StarterKind::Minimal)
            .unwrap();
        store
            .write_file(
                "local/e2",
                "script.lua",
                b"-- comment\nerror(\"boom\")",
                None,
            )
            .unwrap();
        let res = store.render("local/e2", RenderOpts::default());
        let err = res.error.unwrap();
        assert_eq!(err.line, Some(2), "message was: {}", err.message);
    }

    #[test]
    fn render_missing_screen_returns_error_not_panic() {
        let (store, _repo_root) = test_store_with_local();
        let res = store.render("local/does-not-exist", RenderOpts::default());
        assert!(res.error.is_some());
        assert!(res.png.is_empty());
    }

    #[test]
    fn render_reports_dangling_include_as_error() {
        // `validate` can't catch this (Tera resolves `{% include %}` at
        // render time, not registration — see `TemplateService::build_tera`'s
        // doc comment); `render` must hit it and surface it as a populated
        // `RenderResult.error`, not a panic or a silently-empty PNG.
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "badinclude", StarterKind::Minimal)
            .unwrap();
        store
            .write_file(
                "local/badinclude",
                "screen.svg",
                b"<svg>{% include \"badinclude/does-not-exist.svg\" %}</svg>",
                None,
            )
            .unwrap();
        let res = store.render("local/badinclude", RenderOpts::default());
        assert!(
            res.error.is_some(),
            "expected a dangling include to fail render"
        );
        assert!(!res.error.as_ref().unwrap().message.is_empty());
        assert!(res.png.is_empty());
    }

    #[test]
    fn render_panel_option_selects_panel_colors_for_device_colors() {
        // Regression for the Important-2 divergence: `render`'s
        // `device_ctx.colors` (what a script sees via `device.colors`) must
        // fold in the resolved panel's `colors`, matching what the panel's
        // `colors` also drives the final dithered PNG toward — not the
        // bare model default. A script that adapts its drawing to
        // `device.colors` (the exact thing `RenderOpts::panel` exists to
        // preview) would otherwise preview against the wrong palette. If
        // this reverts to building `device_ctx.colors` from the bare model
        // palette instead of `resolve_ctx_palette`, the assertion below
        // fails (it'd see the 4-grey model default instead).
        let mut panels = HashMap::new();
        panels.insert(
            "test-panel".to_string(),
            PanelConfig {
                name: "Test Panel".to_string(),
                match_pattern: None,
                width: None,
                height: None,
                colors: "#FF0000,#00FF00,#0000FF".to_string(),
                colors_actual: None,
                dither: None,
            },
        );
        let config = AppConfig {
            panels,
            ..AppConfig::default()
        };
        let (store, _repo_root) = test_store_with_local_and_config(config);
        store
            .create_screen("local", "panelcheck", StarterKind::Minimal)
            .unwrap();
        store
            .write_file(
                "local/panelcheck",
                "script.lua",
                // `message` keeps the starter's `screen.svg` (which
                // references `data.message`) happy; `colors` is what this
                // test actually inspects.
                b"return { data = { message = \"x\", colors = device.colors } }",
                None,
            )
            .unwrap();
        let res = store.render(
            "local/panelcheck",
            RenderOpts {
                panel: Some("test-panel".to_string()),
                ..RenderOpts::default()
            },
        );
        assert!(res.error.is_none(), "{:?}", res.error);
        assert_eq!(
            res.data["colors"],
            serde_json::json!(["#FF0000", "#00FF00", "#0000FF"]),
            "device.colors should be the panel's colors, not the model default"
        );
    }

    #[test]
    fn render_include_raw_produces_pre_dither_png() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "raw", StarterKind::Minimal)
            .unwrap();
        let res = store.render(
            "local/raw",
            RenderOpts {
                include_raw: true,
                ..RenderOpts::default()
            },
        );
        assert!(res.error.is_none(), "{:?}", res.error);
        assert!(!res.png.is_empty());
        let raw = res.raw_png.expect("include_raw should populate raw_png");
        assert!(!raw.is_empty());
        // Must actually be the pre-dither (full-color) PNG, not a second
        // copy of the palette-restricted `png` — this would still pass if
        // `render_raw_png_from_svg` accidentally returned the dithered
        // bytes, so assert the two differ rather than just non-empty.
        assert_ne!(
            raw, res.png,
            "raw_png should be the pre-dither PNG, not the dithered `png`"
        );
    }

    #[test]
    fn render_without_include_raw_leaves_raw_png_none() {
        let (store, _repo_root) = test_store_with_local();
        store
            .create_screen("local", "noraw", StarterKind::Minimal)
            .unwrap();
        let res = store.render("local/noraw", RenderOpts::default());
        assert!(res.error.is_none(), "{:?}", res.error);
        assert!(res.raw_png.is_none());
    }

    #[test]
    fn render_works_for_read_only_source() {
        // render is a read operation, like validate — it must work against
        // the embedded (read-only) byonk-builtin handle too.
        let (store, _repo_root) = test_store_with_local();
        let res = store.render("byonk-builtin/default", RenderOpts::default());
        assert!(res.error.is_none(), "{:?}", res.error);
        assert!(!res.png.is_empty());
    }

    #[test]
    fn parse_lua_error_line_extracts_runtime_error_line() {
        let msg = "Script error: Lua error: runtime error: [string \"...\"]:12: attempt to index a nil value (local 'y')\nstack traceback:\n\t[C]: in metamethod 'index'";
        assert_eq!(parse_lua_error_line(msg), Some(12));
    }

    #[test]
    fn parse_lua_error_line_extracts_syntax_error_line() {
        let msg = "Script error: Lua error: syntax error: [string \"...\"]:1: unexpected symbol near <eof>";
        assert_eq!(parse_lua_error_line(msg), Some(1));
    }

    #[test]
    fn parse_lua_error_line_none_when_unparseable() {
        assert_eq!(parse_lua_error_line("Screen 'x/y' not found"), None);
    }
}
