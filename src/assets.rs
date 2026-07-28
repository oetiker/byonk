//! Asset loading with embedded fallbacks
//!
//! This module provides a unified interface for loading assets (screens, fonts, config)
//! with the following behavior:
//!
//! - If an env var is NOT set: use embedded assets only (no filesystem access)
//! - If an env var IS set and path is empty/missing: seed with embedded assets, then use filesystem
//! - If an env var IS set and path has files: use filesystem with embedded fallback

use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Embedded screen assets (Lua scripts, SVG templates, and images) — the
/// minimal, read-only `byonk-builtin` repo (`default` + `calibration/*` only).
#[derive(RustEmbed)]
#[folder = "screens/builtin/"]
#[include = "*.lua"]
#[include = "*.svg"]
#[include = "*.yaml"]
#[include = "*.png"]
#[include = "*.jpg"]
#[include = "*.jpeg"]
#[include = "*.gif"]
#[include = "*.webp"]
#[include = "**/*.lua"]
#[include = "**/*.svg"]
#[include = "**/*.yaml"]
#[include = "**/*.png"]
#[include = "**/*.jpg"]
#[include = "**/*.jpeg"]
#[include = "**/*.gif"]
#[include = "**/*.webp"]
struct EmbeddedScreens;

/// Embedded example screen assets (`screens/examples/`), shipped separately
/// from `byonk-builtin`. Not yet registered as a screen repo — that is
/// `Task 11`'s job (seeding `examples` to disk as an editable local repo).
#[derive(RustEmbed)]
#[folder = "screens/examples/"]
#[include = "*.lua"]
#[include = "*.svg"]
#[include = "*.yaml"]
#[include = "*.png"]
#[include = "*.jpg"]
#[include = "*.jpeg"]
#[include = "*.gif"]
#[include = "*.webp"]
#[include = "**/*.lua"]
#[include = "**/*.svg"]
#[include = "**/*.yaml"]
#[include = "**/*.png"]
#[include = "**/*.jpg"]
#[include = "**/*.jpeg"]
#[include = "**/*.gif"]
#[include = "**/*.webp"]
struct EmbeddedExamples;

/// Embedded font assets
#[derive(RustEmbed)]
#[folder = "fonts/"]
struct EmbeddedFonts;

/// Embedded default config
#[derive(RustEmbed)]
#[folder = "."]
#[include = "default-config.yaml"]
struct EmbeddedConfig;

/// Embedded byonk-base std assets (versioned: v1/, v2/, ...).
#[derive(RustEmbed)]
#[folder = "byonk-base/"]
#[include = "**/*.svg"]
#[include = "**/*.lua"]
struct EmbeddedBase;

/// The `local` handle's manifest, seeded into an empty/missing `SCREENS_DIR`
/// (see `AssetLoader::seed_if_configured`) and written by `byonk init
/// --screens` (see `AssetLoader::init`) — the one manifest content both
/// paths must agree on, kept in exactly one place so they can't drift.
const LOCAL_MANIFEST: &str =
    "name: local\ndescription: Your own screens.\nauthor: you\nlicense: UNLICENSED\n";

/// Asset category for selective operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetCategory {
    Screens,
    Fonts,
    Config,
}

/// Report of seeding operations
#[derive(Debug, Default)]
pub struct SeedReport {
    pub screens_seeded: Vec<String>,
    pub examples_seeded: Vec<String>,
    pub fonts_seeded: Vec<String>,
    pub config_seeded: bool,
}

impl SeedReport {
    pub fn is_empty(&self) -> bool {
        self.screens_seeded.is_empty()
            && self.examples_seeded.is_empty()
            && self.fonts_seeded.is_empty()
            && !self.config_seeded
    }
}

/// Report of init (extraction) operations
#[derive(Debug, Default)]
pub struct InitReport {
    pub written: Vec<String>,
    pub skipped: Vec<String>,
}

/// Asset loader with merge behavior and optional filesystem override
pub struct AssetLoader {
    /// External screens directory (from SCREENS_DIR env var)
    screens_dir: Option<PathBuf>,
    /// Directory the shipped `examples` set is seeded into, derived from
    /// `screens_dir` as `<SCREENS_DIR>/../examples`. `None` whenever
    /// `screens_dir` is `None` — there is no fallback location, and nothing
    /// is ever written outside a configured directory.
    examples_dir: Option<PathBuf>,
    /// External fonts directory (from FONTS_DIR env var)
    fonts_dir: Option<PathBuf>,
    /// External config file path (from CONFIG_FILE env var)
    config_file: Option<PathBuf>,
}

impl AssetLoader {
    /// Create a new asset loader
    ///
    /// Paths should be `Some` only if the corresponding env var was set.
    /// If `None`, embedded assets are used exclusively.
    pub fn new(
        screens_dir: Option<PathBuf>,
        fonts_dir: Option<PathBuf>,
        config_file: Option<PathBuf>,
    ) -> Self {
        let examples_dir = screens_dir
            .as_ref()
            .map(|dir| dir.join("..").join("examples"));
        Self {
            screens_dir,
            examples_dir,
            fonts_dir,
            config_file,
        }
    }

    /// Override the derived examples directory (`<SCREENS_DIR>/../examples`)
    /// with an explicit path — e.g. from the `EXAMPLES_DIR` env var, read
    /// alongside `SCREENS_DIR`/`FONTS_DIR` at every `AssetLoader::new` call
    /// site in `main.rs` that starts a server. `None` leaves whatever `new`
    /// already computed untouched: this only ever redirects an
    /// already-derived location (or leaves it `None`, when `screens_dir`
    /// wasn't configured either) — it never invents a location `new`
    /// wouldn't have. Unlike `examples_dir`'s derivation, this is
    /// independent of `screens_dir`: setting `EXAMPLES_DIR` without
    /// `SCREENS_DIR` is honored, matching how `FONTS_DIR`/`CONFIG_FILE` are
    /// already independent of `SCREENS_DIR`.
    pub fn with_examples_dir_override(mut self, examples_dir: Option<PathBuf>) -> Self {
        if let Some(dir) = examples_dir {
            self.examples_dir = Some(dir);
        }
        self
    }

    /// Read a screen asset from the embedded tree ONLY, bypassing the
    /// `SCREENS_DIR` filesystem overlay entirely (unlike `read_screen`).
    ///
    /// Used for reads whose identity must never be shadowed by a file the
    /// user happens to have at the same relative path under `SCREENS_DIR` —
    /// currently just `byonk-builtin`'s own `byonk-screens.yaml`
    /// (`EmbeddedBuiltinSource::load`): the built-in repo's
    /// name/description/author/license/`root:` must always come from the
    /// embedded tree, never from whatever manifest the `local` repo happens
    /// to have at `SCREENS_DIR/byonk-screens.yaml` (same relative path, same
    /// overlay `read_screen` would otherwise prefer).
    pub fn read_screen_embedded_only(
        &self,
        relative_path: &Path,
    ) -> io::Result<Cow<'static, [u8]>> {
        let path_str = relative_path.to_string_lossy();
        EmbeddedScreens::get(&path_str)
            .map(|f| {
                tracing::trace!(path = %path_str, "Loading screen from embedded assets (overlay bypassed)");
                f.data
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Screen not found: {path_str}"),
                )
            })
    }

    /// Resolve `relative_path` against the `SCREENS_DIR` overlay, if one is
    /// configured: canonicalize-and-prefix-check, then return the
    /// canonicalized target if it exists and stays inside `SCREENS_DIR`.
    ///
    /// `SCREENS_DIR` is user-writable (Samba share, HA `/config/screens`), so
    /// a symlink planted there must not become a read primitive for the
    /// whole filesystem. This is the same guard `screen_repo_loader::read_within`
    /// applies to git/local repos. Shared by `read_screen` and
    /// `read_screen_capped` so the check exists in exactly one place.
    ///
    /// Returns `None` when there's no overlay configured, the file isn't
    /// there, or it resolves outside `SCREENS_DIR` (logged as a warning) —
    /// every case where the caller should fall through to the embedded copy.
    fn resolve_screens_dir_overlay(&self, relative_path: &Path) -> Option<PathBuf> {
        let dir = self.screens_dir.as_ref()?;
        let full_path = dir.join(relative_path);
        if !full_path.exists() {
            return None;
        }
        // Read/stat the CANONICALIZED path, not `full_path` — checking one
        // path and using another re-follows the symlink and leaves a swap
        // window open on exactly the user-writable directory this guard
        // defends. `read_within` does the same.
        let checked = std::fs::canonicalize(dir)
            .ok()
            .zip(std::fs::canonicalize(&full_path).ok())
            .filter(|(root, target)| target.starts_with(root))
            .map(|(_, target)| target);
        if checked.is_none() {
            tracing::warn!(
                path = %full_path.display(),
                "refused SCREENS_DIR read escaping the screens directory"
            );
        }
        checked
    }

    /// Read a screen asset (Lua script or SVG template)
    ///
    /// If an external path is configured, tries filesystem first, then falls back to embedded.
    /// If no external path is configured, uses embedded only.
    pub fn read_screen(&self, relative_path: &Path) -> io::Result<Cow<'static, [u8]>> {
        if let Some(target) = self.resolve_screens_dir_overlay(relative_path) {
            tracing::trace!(path = %target.display(), "Loading screen from filesystem");
            return Ok(Cow::Owned(fs::read(&target)?));
        }

        // Fall back to embedded
        let path_str = relative_path.to_string_lossy();
        EmbeddedScreens::get(&path_str)
            .map(|f| {
                tracing::trace!(path = %path_str, "Loading screen from embedded assets");
                f.data
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Screen not found: {path_str}"),
                )
            })
    }

    /// `read_screen`, but refuses anything on disk larger than `max_bytes`
    /// without reading it into memory first.
    ///
    /// `SCREENS_DIR` is user-writable (Samba, HA `/config/screens`), so —
    /// unlike the embedded fallback, whose bytes are already resident in the
    /// binary — the overlay branch is unbounded I/O unless it `stat`s before
    /// it reads. `Ok(None)` means "present in the overlay but over the cap",
    /// distinct from `Err` ("not found anywhere"), so callers can report the
    /// difference instead of collapsing both into one failure. The embedded
    /// fallback has no size check of its own here — same reasoning as
    /// `ScreenRepoSource::read_limited`'s default impl, it's already in
    /// memory either way; callers that need the embedded branch capped too
    /// (as `EmbeddedBuiltinSource::read_limited` does) check the returned
    /// length themselves.
    pub fn read_screen_capped(
        &self,
        relative_path: &Path,
        max_bytes: usize,
    ) -> io::Result<Option<Cow<'static, [u8]>>> {
        if let Some(target) = self.resolve_screens_dir_overlay(relative_path) {
            let meta = fs::metadata(&target)?;
            if meta.len() > max_bytes as u64 {
                return Ok(None);
            }
            tracing::trace!(path = %target.display(), "Loading screen from filesystem");
            return Ok(Some(Cow::Owned(fs::read(&target)?)));
        }

        // Fall back to embedded
        let path_str = relative_path.to_string_lossy();
        EmbeddedScreens::get(&path_str)
            .map(|f| {
                tracing::trace!(path = %path_str, "Loading screen from embedded assets");
                Some(f.data)
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Screen not found: {path_str}"),
                )
            })
    }

    /// Read a screen asset as a UTF-8 string
    pub fn read_screen_string(&self, relative_path: &Path) -> io::Result<String> {
        let bytes = self.read_screen(relative_path)?;
        String::from_utf8(bytes.into_owned())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// List all available screens (merged view of embedded + external)
    ///
    /// Returns paths relative to screens dir, including subdirectories like
    /// "layouts/base.svg" and "components/header.svg".
    pub fn list_screens(&self) -> Vec<String> {
        let mut files: HashSet<String> = EmbeddedScreens::iter().map(|s| s.to_string()).collect();

        if let Some(ref dir) = self.screens_dir {
            Self::collect_screen_files(dir, dir, &mut files);
        }

        let mut result: Vec<_> = files.into_iter().collect();
        result.sort();
        result
    }

    /// Recursively collect screen files from a directory.
    ///
    /// Paths are normalized to use forward slashes regardless of platform,
    /// matching the embedded asset paths from rust-embed and Tera template
    /// references (e.g., `{% extends "layouts/base.svg" %}`).
    fn collect_screen_files(base_dir: &Path, current_dir: &Path, files: &mut HashSet<String>) {
        if let Ok(entries) = fs::read_dir(current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Recurse into subdirectories
                    Self::collect_screen_files(base_dir, &path, files);
                } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".lua") || name.ends_with(".svg") || name.ends_with(".yaml") {
                        // Get relative path from base_dir
                        if let Ok(relative) = path.strip_prefix(base_dir) {
                            if let Some(relative_str) = relative.to_str() {
                                // Normalize to forward slashes for cross-platform consistency
                                files.insert(relative_str.replace('\\', "/"));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Read an embedded example screen asset by path relative to
    /// `screens/examples/` (e.g. `"hello/script.lua"`).
    ///
    /// Mirrors `read_screen`, but examples have no filesystem overlay (unlike
    /// `screens_dir`) — they are embedded-only until Task 11 seeds them to
    /// disk as the `examples` local screen repo.
    pub fn read_example(&self, relative_path: &Path) -> io::Result<Cow<'static, [u8]>> {
        let path_str = relative_path.to_string_lossy();
        EmbeddedExamples::get(&path_str)
            .map(|f| {
                tracing::trace!(path = %path_str, "Loading example from embedded assets");
                f.data
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Example not found: {path_str}"),
                )
            })
    }

    /// List all embedded example screen assets (paths relative to
    /// `screens/examples/`). Mirrors `list_screens`.
    pub fn list_examples(&self) -> Vec<String> {
        let mut result: Vec<String> = EmbeddedExamples::iter().map(|s| s.to_string()).collect();
        result.sort();
        result
    }

    /// Get all font data (for loading into fontdb)
    ///
    /// Returns a merged list: external fonts override embedded fonts with the same name.
    pub fn get_fonts(&self) -> Vec<(String, Cow<'static, [u8]>)> {
        let mut fonts = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // External fonts first (they take priority)
        if let Some(ref dir) = self.fonts_dir {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if matches!(ext.to_str(), Some("ttf" | "otf" | "woff" | "woff2")) {
                            if let Ok(data) = fs::read(&path) {
                                let name = entry.file_name().to_string_lossy().to_string();
                                tracing::trace!(font = %name, "Loading font from filesystem");
                                seen.insert(name.clone());
                                fonts.push((name, Cow::Owned(data)));
                            }
                        }
                    }
                }
            }
        }

        // Embedded fonts (if not overridden)
        for file in EmbeddedFonts::iter() {
            let name = file.to_string();
            if !seen.contains(&name) {
                if let Some(data) = EmbeddedFonts::get(&name) {
                    tracing::trace!(font = %name, "Loading font from embedded assets");
                    fonts.push((name, data.data));
                }
            }
        }

        fonts
    }

    /// Path to the external config file, if one is configured.
    pub fn config_path(&self) -> Option<&std::path::Path> {
        self.config_file.as_deref()
    }

    /// Path to the external `SCREENS_DIR`, if one is configured. Used to
    /// auto-register it as the writable `local` screen repo handle.
    pub fn screens_dir(&self) -> Option<&std::path::Path> {
        self.screens_dir.as_deref()
    }

    /// Path the shipped `examples` set is seeded into (`<SCREENS_DIR>/../examples`),
    /// if `SCREENS_DIR` is configured. Used to auto-register it as the writable
    /// `examples` screen repo handle, mirroring `screens_dir()`/`local`.
    pub fn examples_dir(&self) -> Option<&std::path::Path> {
        self.examples_dir.as_deref()
    }

    /// Read the config file
    ///
    /// If an external path is configured and exists, uses that.
    /// Otherwise falls back to embedded config.
    pub fn read_config(&self) -> io::Result<Cow<'static, [u8]>> {
        // Try external first
        if let Some(ref path) = self.config_file {
            if path.exists() {
                tracing::trace!(path = %path.display(), "Loading config from filesystem");
                return Ok(Cow::Owned(fs::read(path)?));
            }
        }

        // Fall back to embedded
        EmbeddedConfig::get("default-config.yaml")
            .map(|f| {
                tracing::trace!("Loading config from embedded assets");
                f.data
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Embedded default-config.yaml not found",
                )
            })
    }

    /// Read config as a UTF-8 string
    pub fn read_config_string(&self) -> io::Result<String> {
        let bytes = self.read_config()?;
        String::from_utf8(bytes.into_owned())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Check if a directory exists and is empty (ignoring .gitkeep)
    fn is_empty_dir(path: &Path) -> bool {
        if !path.exists() || !path.is_dir() {
            return false;
        }
        path.read_dir()
            .map(|mut entries| {
                entries.all(|e| {
                    e.map(|entry| entry.file_name() == ".gitkeep")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }

    /// Seed empty/missing directories with embedded assets
    ///
    /// Only operates on paths that were configured (env var was set).
    /// Creates directories if they don't exist.
    ///
    /// Each of the four sections below (local manifest, examples, fonts,
    /// config) is independently fallible: a failure in one is logged and
    /// seeding moves on to the next, rather than aborting the rest via `?`.
    /// This matters most for the examples section — the decorative shipped
    /// examples set lives one level *above* the only directory the operator
    /// actually configured (`<SCREENS_DIR>/../examples`), so it can be
    /// unwritable/unmounted (e.g. a Docker rootfs) even when `SCREENS_DIR`
    /// itself is fine. A failed examples seed must never prevent fonts or
    /// config seeding (a 409 on every admin write otherwise, since an
    /// unseeded `CONFIG_FILE` can't be found — see
    /// `test_write_on_embedded_config_returns_409`). `seed_if_configured`
    /// itself now always returns `Ok`: every fallible step already caught
    /// and logged its own error, so there is nothing left to propagate.
    pub fn seed_if_configured(&self) -> io::Result<SeedReport> {
        let mut report = SeedReport::default();

        if let Err(e) = self.seed_local_manifest(&mut report) {
            tracing::warn!(
                error = %e,
                "Failed to seed local screens directory; the 'local' screen repo handle may not register"
            );
        }

        if let Err(e) = self.seed_examples(&mut report) {
            tracing::warn!(
                error = %e,
                "Failed to seed examples directory; the 'examples' screen repo handle may not register (fonts/config seeding unaffected)"
            );
        }

        if let Err(e) = self.seed_fonts(&mut report) {
            tracing::warn!(error = %e, "Failed to seed fonts directory");
        }

        if let Err(e) = self.seed_config(&mut report) {
            tracing::warn!(error = %e, "Failed to seed config file");
        }

        Ok(report)
    }

    /// Seed `SCREENS_DIR` with a `local` manifest.
    ///
    /// `SCREENS_DIR` is the user's own writable `local` repo, not a copy of
    /// byonk's built-in screens (those stay embedded-only, read-only, under
    /// the `byonk-builtin` handle).
    ///
    /// Gate: **the manifest file itself doesn't exist yet** — not "the
    /// directory is empty". A missing manifest is a broken state (no
    /// `local` handle can register at all), not a user preference, unlike a
    /// deleted screen (which is left alone). This also covers the upgrade
    /// case: an existing non-empty `SCREENS_DIR` from an older byonk (or one
    /// a user seeded content into before ever getting a manifest) gets a
    /// `local` manifest retroactively. An existing manifest — including one
    /// the user has since edited — is never overwritten, on this or any
    /// later call.
    fn seed_local_manifest(&self, report: &mut SeedReport) -> io::Result<()> {
        let Some(ref dir) = self.screens_dir else {
            return Ok(());
        };
        fs::create_dir_all(dir)?;
        let manifest_path = dir.join("byonk-screens.yaml");
        if !manifest_path.exists() {
            fs::write(&manifest_path, LOCAL_MANIFEST)?;
            report.screens_seeded.push("byonk-screens.yaml".to_string());
            tracing::info!(
                dir = %dir.display(),
                "Seeded local screens directory with a byonk-screens.yaml manifest"
            );
        }
        Ok(())
    }

    /// Seed the examples directory with the shipped `examples` set (Task
    /// 10's `EmbeddedExamples`, including its own `byonk-screens.yaml`
    /// manifest), so it lands as an editable local screen repo under the
    /// `examples` handle. Only when an examples dir is configured (there is
    /// no fallback location — never write outside a configured directory)
    /// and it is empty/missing (once-only, idempotent: a user's edits or
    /// deletions inside an already-seeded `examples` directory are never
    /// touched again).
    ///
    /// A failure part-way through the copy loop cleans up whatever was
    /// partially written (rather than leaving a permanently half-seeded,
    /// non-empty-but-manifest-less directory that `is_empty_dir` would
    /// never retry), so the next start attempts a full reseed from scratch.
    fn seed_examples(&self, report: &mut SeedReport) -> io::Result<()> {
        let Some(ref dir) = self.examples_dir else {
            return Ok(());
        };
        let should_seed = !dir.exists() || Self::is_empty_dir(dir);
        if !should_seed {
            return Ok(());
        }
        fs::create_dir_all(dir)?;
        if let Err(e) = Self::write_examples(dir, report) {
            report.examples_seeded.clear();
            let _ = fs::remove_dir_all(dir);
            return Err(e);
        }
        if !report.examples_seeded.is_empty() {
            tracing::info!(
                dir = %dir.display(),
                count = report.examples_seeded.len(),
                "Seeded examples directory with embedded assets"
            );
        }
        Ok(())
    }

    /// The examples copy loop itself, factored out so `seed_examples` can
    /// wrap it with cleanup-on-error.
    fn write_examples(dir: &Path, report: &mut SeedReport) -> io::Result<()> {
        for file in EmbeddedExamples::iter() {
            if let Some(data) = EmbeddedExamples::get(&file) {
                let path = dir.join(file.as_ref());
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, &*data.data)?;
                report.examples_seeded.push(file.to_string());
            }
        }
        Ok(())
    }

    /// Seed `FONTS_DIR` with embedded fonts. Gate unchanged (empty/missing
    /// directory), per the controller ruling in Task 11's review: "leave the
    /// fonts/config seeding gates alone."
    fn seed_fonts(&self, report: &mut SeedReport) -> io::Result<()> {
        let Some(ref dir) = self.fonts_dir else {
            return Ok(());
        };
        let should_seed = !dir.exists() || Self::is_empty_dir(dir);
        if !should_seed {
            return Ok(());
        }
        fs::create_dir_all(dir)?;
        for file in EmbeddedFonts::iter() {
            if let Some(data) = EmbeddedFonts::get(&file) {
                let path = dir.join(file.as_ref());
                fs::write(&path, &*data.data)?;
                report.fonts_seeded.push(file.to_string());
            }
        }
        if !report.fonts_seeded.is_empty() {
            tracing::info!(
                dir = %dir.display(),
                count = report.fonts_seeded.len(),
                "Seeded fonts directory with embedded assets"
            );
        }
        Ok(())
    }

    /// Seed `CONFIG_FILE` with the embedded default config. Gate unchanged
    /// (file doesn't exist), per the controller ruling in Task 11's review.
    fn seed_config(&self, report: &mut SeedReport) -> io::Result<()> {
        let Some(ref path) = self.config_file else {
            return Ok(());
        };
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(data) = EmbeddedConfig::get("default-config.yaml") {
            fs::write(path, &*data.data)?;
            report.config_seeded = true;
            tracing::info!(path = %path.display(), "Seeded config file with embedded default");
        }
        Ok(())
    }

    /// Extract embedded assets to filesystem (init command)
    ///
    /// Uses the configured paths (or defaults if not set).
    pub fn init(&self, categories: &[AssetCategory], force: bool) -> io::Result<InitReport> {
        let mut report = InitReport::default();

        for category in categories {
            match category {
                AssetCategory::Screens => {
                    // `byonk-builtin` is embedded-only and read-only — it is
                    // never copied into a user's screens directory (`Task
                    // 11` retired that for `seed_if_configured`; this is the
                    // matching fix for `init --screens`, its last surviving
                    // copy-in path). In the new layering `SCREENS_DIR` is
                    // the writable `local` repo, so `init --screens`
                    // initializes it the same way `seed_if_configured` does:
                    // write only the `byonk-screens.yaml` manifest.
                    let dir = self
                        .screens_dir
                        .clone()
                        .unwrap_or_else(|| PathBuf::from("./screens"));
                    fs::create_dir_all(&dir)?;

                    let manifest_path = dir.join("byonk-screens.yaml");
                    if !force && manifest_path.exists() {
                        report.skipped.push(manifest_path.display().to_string());
                    } else {
                        fs::write(&manifest_path, LOCAL_MANIFEST)?;
                        report.written.push(manifest_path.display().to_string());
                    }
                }
                AssetCategory::Fonts => {
                    let dir = self
                        .fonts_dir
                        .clone()
                        .unwrap_or_else(|| PathBuf::from("./fonts"));
                    fs::create_dir_all(&dir)?;

                    for file in EmbeddedFonts::iter() {
                        let path = dir.join(file.as_ref());
                        if !force && path.exists() {
                            report.skipped.push(path.display().to_string());
                            continue;
                        }
                        if let Some(data) = EmbeddedFonts::get(&file) {
                            fs::write(&path, &*data.data)?;
                            report.written.push(path.display().to_string());
                        }
                    }
                }
                AssetCategory::Config => {
                    let path = self
                        .config_file
                        .clone()
                        .unwrap_or_else(|| PathBuf::from("./config.yaml"));

                    if !force && path.exists() {
                        report.skipped.push(path.display().to_string());
                        continue;
                    }
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    if let Some(data) = EmbeddedConfig::get("default-config.yaml") {
                        fs::write(&path, &*data.data)?;
                        report.written.push(path.display().to_string());
                    }
                }
            }
        }

        Ok(report)
    }

    /// Read a byonk-base asset by version-relative path, e.g. "v1/hinting.svg".
    pub fn read_base(&self, rel: &str) -> Option<Cow<'static, [u8]>> {
        EmbeddedBase::get(rel).map(|f| f.data)
    }

    pub fn read_base_string(&self, rel: &str) -> Option<String> {
        self.read_base(rel)
            .and_then(|b| String::from_utf8(b.into_owned()).ok())
    }

    /// List embedded base asset paths (e.g. ["v1/base.svg", ...]).
    pub fn list_base(&self) -> Vec<String> {
        EmbeddedBase::iter().map(|s| s.to_string()).collect()
    }

    /// List embedded assets by category (for display)
    pub fn list_embedded(category: AssetCategory) -> Vec<String> {
        match category {
            AssetCategory::Screens => EmbeddedScreens::iter().map(|s| s.to_string()).collect(),
            AssetCategory::Fonts => EmbeddedFonts::iter().map(|s| s.to_string()).collect(),
            AssetCategory::Config => vec!["default-config.yaml".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_loader_new() {
        let _loader = AssetLoader::new(None, None, None);
        // Should not panic - if we get here, construction succeeded

        let loader = AssetLoader::new(
            Some(PathBuf::from("/tmp/screens")),
            Some(PathBuf::from("/tmp/fonts")),
            Some(PathBuf::from("/tmp/config.yaml")),
        );
        // Verify paths are stored correctly
        assert!(loader.screens_dir.is_some());
        assert!(loader.fonts_dir.is_some());
        assert!(loader.config_file.is_some());
    }

    #[test]
    fn test_config_path_getter() {
        let loader = AssetLoader::new(None, None, Some(PathBuf::from("/tmp/x.yaml")));
        assert_eq!(
            loader.config_path(),
            Some(std::path::Path::new("/tmp/x.yaml"))
        );
        let embedded = AssetLoader::new(None, None, None);
        assert_eq!(embedded.config_path(), None);
    }

    #[test]
    fn test_seed_report_is_empty() {
        let report = SeedReport::default();
        assert!(report.is_empty());

        let report = SeedReport {
            screens_seeded: vec!["test.lua".to_string()],
            examples_seeded: vec![],
            fonts_seeded: vec![],
            config_seeded: false,
        };
        assert!(!report.is_empty());

        let report = SeedReport {
            screens_seeded: vec![],
            examples_seeded: vec!["hello/script.lua".to_string()],
            fonts_seeded: vec![],
            config_seeded: false,
        };
        assert!(!report.is_empty());

        let report = SeedReport {
            screens_seeded: vec![],
            examples_seeded: vec![],
            fonts_seeded: vec!["font.ttf".to_string()],
            config_seeded: false,
        };
        assert!(!report.is_empty());

        let report = SeedReport {
            screens_seeded: vec![],
            examples_seeded: vec![],
            fonts_seeded: vec![],
            config_seeded: true,
        };
        assert!(!report.is_empty());
    }

    #[test]
    fn test_read_screen_embedded() {
        let loader = AssetLoader::new(None, None, None);

        // Should find embedded default/script.lua (byonk-builtin's default screen)
        let result = loader.read_screen(Path::new("default/script.lua"));
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_read_screen_not_found() {
        let loader = AssetLoader::new(None, None, None);

        let result = loader.read_screen(Path::new("nonexistent.lua"));
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_read_screen_string() {
        let loader = AssetLoader::new(None, None, None);

        let result = loader.read_screen_string(Path::new("default/script.lua"));
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("return"));
    }

    #[test]
    fn test_list_screens() {
        let loader = AssetLoader::new(None, None, None);

        let screens = loader.list_screens();
        assert!(!screens.is_empty());

        // Should include the minimal byonk-builtin repo's default screen files
        assert!(screens.iter().any(|s| s == "default/script.lua"));
        assert!(screens.iter().any(|s| s == "default/screen.svg"));
    }

    #[test]
    fn test_list_examples_contains_moved_screens() {
        let loader = AssetLoader::new(None, None, None);
        let examples = loader.list_examples();
        assert!(!examples.is_empty());
        for expected in [
            "hello/script.lua",
            "mandelbrot/script.lua",
            "webscrape/script.lua",
            "gphoto/script.lua",
            "swiss-departure-board/script.lua",
            "demo/font/bitmap/script.lua",
        ] {
            assert!(
                examples.iter().any(|s| s == expected),
                "expected examples embed to contain {expected}, got {examples:?}"
            );
        }
        // The examples embed must not include the byonk-builtin screens.
        assert!(!examples.iter().any(|s| s == "default/script.lua"));
    }

    #[test]
    fn test_read_example() {
        let loader = AssetLoader::new(None, None, None);
        let result = loader.read_example(Path::new("hello/script.lua"));
        assert!(result.is_ok());
        assert!(loader.read_example(Path::new("nonexistent.lua")).is_err());
    }

    #[test]
    fn test_get_fonts() {
        let loader = AssetLoader::new(None, None, None);

        let fonts = loader.get_fonts();
        // Should have at least one embedded font
        assert!(!fonts.is_empty());

        // All fonts should have data
        for (name, data) in &fonts {
            assert!(!name.is_empty());
            assert!(!data.is_empty());
        }
    }

    #[test]
    fn test_read_config() {
        let loader = AssetLoader::new(None, None, None);

        let result = loader.read_config();
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_read_config_string() {
        let loader = AssetLoader::new(None, None, None);

        let result = loader.read_config_string();
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("screens"));
    }

    #[test]
    fn test_list_embedded_screens() {
        let screens = AssetLoader::list_embedded(AssetCategory::Screens);
        assert!(!screens.is_empty());
        assert!(screens.iter().any(|s| s.ends_with(".lua")));
    }

    #[test]
    fn test_list_embedded_fonts() {
        let fonts = AssetLoader::list_embedded(AssetCategory::Fonts);
        assert!(!fonts.is_empty());
    }

    #[test]
    fn test_list_embedded_config() {
        let config = AssetLoader::list_embedded(AssetCategory::Config);
        assert_eq!(config.len(), 1);
        assert_eq!(config[0], "default-config.yaml");
    }

    #[test]
    fn test_asset_category_equality() {
        assert_eq!(AssetCategory::Screens, AssetCategory::Screens);
        assert_ne!(AssetCategory::Screens, AssetCategory::Fonts);
        assert_ne!(AssetCategory::Fonts, AssetCategory::Config);
    }

    #[test]
    fn test_is_empty_dir_nonexistent() {
        assert!(!AssetLoader::is_empty_dir(Path::new("/nonexistent/path")));
    }

    #[test]
    fn test_init_report_default() {
        let report = InitReport::default();
        assert!(report.written.is_empty());
        assert!(report.skipped.is_empty());
    }

    #[test]
    fn test_is_empty_dir_with_empty_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(AssetLoader::is_empty_dir(temp_dir.path()));
    }

    #[test]
    fn test_is_empty_dir_with_gitkeep() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join(".gitkeep"), "").unwrap();
        assert!(AssetLoader::is_empty_dir(temp_dir.path()));
    }

    #[test]
    fn test_is_empty_dir_with_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();
        assert!(!AssetLoader::is_empty_dir(temp_dir.path()));
    }

    #[test]
    fn test_read_base_asset() {
        let loader = AssetLoader::new(None, None, None);
        assert!(loader.read_base_string("v1/hinting.svg").is_some());
        assert!(loader.list_base().iter().any(|p| p == "v1/base.svg"));
        assert!(loader.read_base_string("v1/does-not-exist.svg").is_none());
    }

    #[test]
    fn test_is_empty_dir_with_file_not_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("file.txt");
        std::fs::write(&file_path, "content").unwrap();
        assert!(!AssetLoader::is_empty_dir(&file_path));
    }

    #[test]
    fn test_read_screen_from_filesystem() {
        let temp_dir = tempfile::tempdir().unwrap();
        let script_content = r#"return { data = { test = true }, refresh_rate = 60 }"#;
        std::fs::write(temp_dir.path().join("custom.lua"), script_content).unwrap();

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let result = loader.read_screen(Path::new("custom.lua"));

        assert!(result.is_ok());
        let content = String::from_utf8(result.unwrap().into_owned()).unwrap();
        assert!(content.contains("test = true"));
    }

    #[test]
    fn test_read_screen_filesystem_fallback_to_embedded() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Don't create the screen in temp dir, should fall back to embedded

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let result = loader.read_screen(Path::new("default/script.lua"));

        assert!(result.is_ok());
    }

    #[test]
    fn test_list_screens_with_external_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("custom.lua"), "-- custom").unwrap();
        std::fs::write(temp_dir.path().join("custom.svg"), "<svg/>").unwrap();
        std::fs::write(temp_dir.path().join("readme.txt"), "ignored").unwrap();

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let screens = loader.list_screens();

        assert!(screens.contains(&"custom.lua".to_string()));
        assert!(screens.contains(&"custom.svg".to_string()));
        assert!(!screens.contains(&"readme.txt".to_string()));
        // Also includes embedded
        assert!(screens.contains(&"default/script.lua".to_string()));
    }

    #[test]
    fn test_get_fonts_from_filesystem() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Create a fake font file
        std::fs::write(temp_dir.path().join("custom.ttf"), b"fake font data").unwrap();

        let loader = AssetLoader::new(None, Some(temp_dir.path().to_path_buf()), None);
        let fonts = loader.get_fonts();

        // Should include both custom and embedded fonts
        assert!(fonts.iter().any(|(name, _)| name == "custom.ttf"));
    }

    #[test]
    fn test_read_config_from_filesystem() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yaml");
        let config_content = "screens:\n  test:\n    script: test.lua\n";
        std::fs::write(&config_path, config_content).unwrap();

        let loader = AssetLoader::new(None, None, Some(config_path));
        let result = loader.read_config_string();

        assert!(result.is_ok());
        assert!(result.unwrap().contains("test:"));
    }

    #[test]
    fn test_read_config_fallback_to_embedded() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("nonexistent.yaml");
        // Don't create the file

        let loader = AssetLoader::new(None, None, Some(config_path));
        let result = loader.read_config();

        // Should fall back to embedded config
        assert!(result.is_ok());
    }

    #[test]
    fn test_seed_if_configured_screens() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");
        // Don't create the directory - seed should create it

        let loader = AssetLoader::new(Some(screens_dir.clone()), None, None);
        let result = loader.seed_if_configured();

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(!report.screens_seeded.is_empty());
        assert!(screens_dir.exists());
        assert!(screens_dir.join("byonk-screens.yaml").exists());
    }

    /// Brief Task-11 Step-1 test, verbatim: an empty `SCREENS_DIR` gets only a
    /// `byonk-screens.yaml` manifest — no built-in screen copies land in it.
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

    #[test]
    fn test_seed_if_configured_writes_local_manifest_content() {
        let dir = tempfile::tempdir().unwrap();
        let loader = AssetLoader::new(Some(dir.path().into()), None, None);
        loader.seed_if_configured().unwrap();
        let manifest = std::fs::read_to_string(dir.path().join("byonk-screens.yaml")).unwrap();
        let parsed = crate::models::screen_repo_manifest::ScreenRepoManifest::from_yaml(&manifest)
            .expect("seeded local manifest must be valid");
        assert_eq!(parsed.name, "local");
    }

    #[test]
    fn test_seed_if_configured_examples() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");

        let loader = AssetLoader::new(Some(screens_dir), None, None);
        let report = loader.seed_if_configured().unwrap();

        assert!(!report.examples_seeded.is_empty());
        let examples_dir = loader.examples_dir().expect("examples_dir configured");
        assert!(examples_dir.join("byonk-screens.yaml").exists());
        assert!(examples_dir.join("hello/script.lua").exists());
        assert!(examples_dir.join("gphoto/meta.yaml").exists());
    }

    #[test]
    fn test_with_examples_dir_override_wins_over_derived_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");
        let explicit_examples_dir = temp_dir.path().join("my-examples");

        let derived_default = screens_dir.join("..").join("examples");
        let loader = AssetLoader::new(Some(screens_dir), None, None)
            .with_examples_dir_override(Some(explicit_examples_dir.clone()));

        assert_eq!(loader.examples_dir(), Some(explicit_examples_dir.as_path()));
        assert_ne!(loader.examples_dir(), Some(derived_default.as_path()));

        let report = loader.seed_if_configured().unwrap();
        assert!(!report.examples_seeded.is_empty());
        assert!(explicit_examples_dir.join("byonk-screens.yaml").exists());
        assert!(!derived_default.exists());
    }

    #[test]
    fn test_with_examples_dir_override_works_without_screens_dir() {
        // EXAMPLES_DIR is an independent knob, like FONTS_DIR/CONFIG_FILE:
        // it doesn't require SCREENS_DIR to also be set.
        let temp_dir = tempfile::tempdir().unwrap();
        let explicit_examples_dir = temp_dir.path().join("my-examples");

        let loader = AssetLoader::new(None, None, None)
            .with_examples_dir_override(Some(explicit_examples_dir.clone()));
        assert_eq!(loader.examples_dir(), Some(explicit_examples_dir.as_path()));

        let report = loader.seed_if_configured().unwrap();
        assert!(!report.examples_seeded.is_empty());
        assert!(explicit_examples_dir.join("byonk-screens.yaml").exists());
    }

    #[test]
    fn test_with_examples_dir_override_none_leaves_derived_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");
        let derived_default = screens_dir.join("..").join("examples");

        let loader =
            AssetLoader::new(Some(screens_dir), None, None).with_examples_dir_override(None);
        assert_eq!(loader.examples_dir(), Some(derived_default.as_path()));
    }

    #[test]
    fn test_seed_if_configured_no_examples_dir_without_screens_dir() {
        // No SCREENS_DIR configured -> no examples dir, nothing seeded, no
        // fallback location invented.
        let loader = AssetLoader::new(None, None, None);
        assert!(loader.examples_dir().is_none());
        let report = loader.seed_if_configured().unwrap();
        assert!(report.examples_seeded.is_empty());
    }

    #[test]
    fn test_seed_if_configured_examples_is_idempotent_and_never_clobbers_edits() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");
        let loader = AssetLoader::new(Some(screens_dir), None, None);

        loader.seed_if_configured().unwrap();
        let examples_dir = loader.examples_dir().unwrap().to_path_buf();

        // User edits a seeded example, and deletes another one entirely.
        std::fs::write(examples_dir.join("hello/script.lua"), "-- user edit\n").unwrap();
        std::fs::remove_dir_all(examples_dir.join("mandelbrot")).unwrap();

        // A second seed pass must change nothing: it must not resurrect the
        // deleted screen and must not overwrite the user's edit.
        let second = loader.seed_if_configured().unwrap();
        assert!(
            second.examples_seeded.is_empty(),
            "second seed pass on a non-empty examples dir must seed nothing"
        );
        assert!(!examples_dir.join("mandelbrot").exists());
        let edited = std::fs::read_to_string(examples_dir.join("hello/script.lua")).unwrap();
        assert_eq!(edited, "-- user edit\n");
    }

    /// Minor 5: a mid-loop failure while seeding examples must clean up the
    /// partial write rather than leave a permanently half-seeded,
    /// manifest-less directory that `is_empty_dir`'s gate would never retry.
    /// Forces a write failure by pre-creating an empty, write-protected
    /// examples dir (so the "empty/missing" seed gate still fires, but every
    /// `fs::write`/nested `create_dir_all` inside it hits `PermissionDenied`).
    #[test]
    #[cfg(unix)]
    fn test_seed_if_configured_examples_cleans_up_after_mid_loop_failure_and_retries_next_start() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");
        let loader = AssetLoader::new(Some(screens_dir), None, None);
        let examples_dir = loader.examples_dir().unwrap().to_path_buf();

        std::fs::create_dir_all(&examples_dir).unwrap();
        std::fs::set_permissions(&examples_dir, std::fs::Permissions::from_mode(0o500)).unwrap();

        // First attempt: every write into the read-only dir fails.
        let first = loader.seed_if_configured().unwrap();
        assert!(
            first.examples_seeded.is_empty(),
            "a mid-loop failure must not report partial success"
        );
        assert!(
            !examples_dir.exists(),
            "the half-seeded dir must be cleaned up so the next start retries from scratch"
        );

        // Second attempt (dir gone, permissions no longer an issue): a full,
        // successful reseed from scratch.
        let second = loader.seed_if_configured().unwrap();
        assert!(
            !second.examples_seeded.is_empty(),
            "a subsequent start must retry and succeed once the obstruction is gone"
        );
        assert!(examples_dir.join("byonk-screens.yaml").exists());
    }

    #[test]
    fn test_seed_if_configured_fonts() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fonts_dir = temp_dir.path().join("fonts");

        let loader = AssetLoader::new(None, Some(fonts_dir.clone()), None);
        let result = loader.seed_if_configured();

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(!report.fonts_seeded.is_empty());
        assert!(fonts_dir.exists());
    }

    #[test]
    fn test_seed_if_configured_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yaml");

        let loader = AssetLoader::new(None, None, Some(config_path.clone()));
        let result = loader.seed_if_configured();

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.config_seeded);
        assert!(config_path.exists());
    }

    /// Controller ruling (Task 11 findings, Minor 7), scenario (i): the
    /// local-manifest seed is gated on "the manifest file doesn't exist",
    /// not "the directory is empty" — so a non-empty `SCREENS_DIR` that
    /// merely lacks a manifest (the upgrade case: an older byonk's
    /// `SCREENS_DIR`, or a stray file like `.DS_Store`, see Minor 7) still
    /// gets one. Non-vacuous: under the old is_empty_dir-based gate this
    /// directory would never seed a manifest at all (`is_empty_dir` is
    /// false the moment ANY real file is present), so `screens_seeded`
    /// would stay empty and the manifest would never be written — this
    /// assertion would fail against that gate.
    #[test]
    fn test_seed_if_configured_screens_writes_manifest_into_nonempty_dir_without_one() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");
        std::fs::create_dir_all(&screens_dir).unwrap();
        std::fs::write(screens_dir.join("existing.lua"), "-- existing").unwrap();

        let loader = AssetLoader::new(Some(screens_dir.clone()), None, None);
        let result = loader.seed_if_configured();

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(
            !report.screens_seeded.is_empty(),
            "a non-empty SCREENS_DIR without a manifest must still get one (upgrade case)"
        );
        let manifest = std::fs::read_to_string(screens_dir.join("byonk-screens.yaml")).unwrap();
        let parsed = crate::models::screen_repo_manifest::ScreenRepoManifest::from_yaml(&manifest)
            .expect("seeded local manifest must be valid");
        assert_eq!(parsed.name, "local");
        // Pre-existing content is left alone.
        assert!(screens_dir.join("existing.lua").exists());
    }

    /// Controller ruling (Task 11 findings, Minor 7), scenario (ii) + Minor
    /// 6 coverage gap: once a `local` manifest exists — including one the
    /// user has since hand-edited — it survives every later
    /// `seed_if_configured()` call, verbatim, never overwritten.
    #[test]
    fn test_seed_if_configured_screens_never_overwrites_existing_manifest_across_multiple_runs() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");
        let loader = AssetLoader::new(Some(screens_dir.clone()), None, None);

        // First run seeds the manifest into a fresh, empty dir.
        let first = loader.seed_if_configured().unwrap();
        assert!(!first.screens_seeded.is_empty());

        // User hand-edits the seeded manifest.
        let edited = "name: local\ndescription: My stuff.\nauthor: me\nlicense: UNLICENSED\n";
        std::fs::write(screens_dir.join("byonk-screens.yaml"), edited).unwrap();

        // Two more runs must never touch it.
        for _ in 0..2 {
            let report = loader.seed_if_configured().unwrap();
            assert!(
                report.screens_seeded.is_empty(),
                "an existing local manifest must never be reseeded"
            );
            let current = std::fs::read_to_string(screens_dir.join("byonk-screens.yaml")).unwrap();
            assert_eq!(
                current, edited,
                "user's manifest edit must survive verbatim"
            );
        }
    }

    #[test]
    fn test_init_screens_writes_local_manifest_not_builtin_copies() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");

        let loader = AssetLoader::new(Some(screens_dir.clone()), None, None);
        let result = loader.init(&[AssetCategory::Screens], false);

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(!report.written.is_empty());
        let manifest = std::fs::read_to_string(screens_dir.join("byonk-screens.yaml")).unwrap();
        let parsed = crate::models::screen_repo_manifest::ScreenRepoManifest::from_yaml(&manifest)
            .expect("init --screens must write a valid local manifest");
        assert_eq!(parsed.name, "local");
        // `byonk-builtin` is embedded-only, read-only, and frozen — `init
        // --screens` must never copy it into the user's writable repo.
        assert!(!screens_dir.join("default").exists());
        assert!(!screens_dir.join("calibration").exists());
    }

    #[test]
    fn test_init_screens_does_not_overwrite_existing_manifest_without_force() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");
        std::fs::create_dir_all(&screens_dir).unwrap();
        std::fs::write(
            screens_dir.join("byonk-screens.yaml"),
            "name: local\ndescription: mine\nauthor: me\nlicense: UNLICENSED\n",
        )
        .unwrap();

        let loader = AssetLoader::new(Some(screens_dir.clone()), None, None);
        let report = loader.init(&[AssetCategory::Screens], false).unwrap();

        assert!(report.written.is_empty());
        assert!(!report.skipped.is_empty());
        let content = std::fs::read_to_string(screens_dir.join("byonk-screens.yaml")).unwrap();
        assert!(content.contains("author: me"));
    }

    #[test]
    fn test_init_fonts() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fonts_dir = temp_dir.path().join("fonts");

        let loader = AssetLoader::new(None, Some(fonts_dir.clone()), None);
        let result = loader.init(&[AssetCategory::Fonts], false);

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(!report.written.is_empty());
    }

    #[test]
    fn test_init_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yaml");

        let loader = AssetLoader::new(None, None, Some(config_path.clone()));
        let result = loader.init(&[AssetCategory::Config], false);

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.written.iter().any(|p| p.contains("config.yaml")));
        assert!(config_path.exists());
    }

    #[test]
    fn test_init_skips_existing_without_force() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yaml");
        std::fs::write(&config_path, "existing: true").unwrap();

        let loader = AssetLoader::new(None, None, Some(config_path.clone()));
        let result = loader.init(&[AssetCategory::Config], false);

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.skipped.iter().any(|p| p.contains("config.yaml")));
        assert!(report.written.is_empty());

        // Content should be unchanged
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("existing: true"));
    }

    #[test]
    fn test_init_overwrites_with_force() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.yaml");
        std::fs::write(&config_path, "existing: true").unwrap();

        let loader = AssetLoader::new(None, None, Some(config_path.clone()));
        let result = loader.init(&[AssetCategory::Config], true);

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.written.iter().any(|p| p.contains("config.yaml")));

        // Content should be overwritten with embedded
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("DEFAULT:"));
    }

    #[test]
    fn test_init_uses_default_paths_when_not_configured() {
        // This test verifies that init uses default paths when no path is configured
        // We don't actually run this to avoid creating files in the current directory
        let loader = AssetLoader::new(None, None, None);
        // Just verify the loader was created - actual init would use ./screens, ./fonts, ./config.yaml
        drop(loader);
    }

    #[test]
    fn test_read_screen_string_invalid_utf8() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Write invalid UTF-8 bytes
        std::fs::write(temp_dir.path().join("binary.lua"), [0xFF, 0xFE, 0x00, 0x01]).unwrap();

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let result = loader.read_screen_string(Path::new("binary.lua"));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_list_screens_recursive() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create nested directory structure
        let layouts_dir = temp_dir.path().join("layouts");
        let components_dir = temp_dir.path().join("components");
        std::fs::create_dir(&layouts_dir).unwrap();
        std::fs::create_dir(&components_dir).unwrap();

        // Create files at various levels
        std::fs::write(temp_dir.path().join("top.svg"), "<svg/>").unwrap();
        std::fs::write(layouts_dir.join("base.svg"), "<svg/>").unwrap();
        std::fs::write(components_dir.join("header.svg"), "<svg/>").unwrap();
        std::fs::write(components_dir.join("footer.svg"), "<svg/>").unwrap();

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let screens = loader.list_screens();

        // Should find all files including nested ones
        assert!(screens.contains(&"top.svg".to_string()));
        assert!(screens.contains(&"layouts/base.svg".to_string()));
        assert!(screens.contains(&"components/header.svg".to_string()));
        assert!(screens.contains(&"components/footer.svg".to_string()));
    }

    #[test]
    fn test_list_screens_recursive_deeply_nested() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create deeply nested structure
        let deep_dir = temp_dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep_dir).unwrap();
        std::fs::write(deep_dir.join("deep.svg"), "<svg/>").unwrap();

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let screens = loader.list_screens();

        assert!(screens.contains(&"a/b/c/deep.svg".to_string()));
    }

    #[test]
    fn test_list_screens_recursive_ignores_non_svg_lua() {
        let temp_dir = tempfile::tempdir().unwrap();

        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();

        // Create various file types
        std::fs::write(subdir.join("valid.svg"), "<svg/>").unwrap();
        std::fs::write(subdir.join("valid.lua"), "return {}").unwrap();
        std::fs::write(subdir.join("invalid.txt"), "text").unwrap();
        std::fs::write(subdir.join("invalid.png"), "image").unwrap();

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let screens = loader.list_screens();

        assert!(screens.contains(&"subdir/valid.svg".to_string()));
        assert!(screens.contains(&"subdir/valid.lua".to_string()));
        assert!(!screens.contains(&"subdir/invalid.txt".to_string()));
        // Note: png files are handled differently - they're for images, not templates
    }

    #[test]
    fn test_embedded_default_ships_only_reserved_default_device() {
        // AssetLoader::new(screens_dir, fonts_dir, config_path) — all None = embedded-only.
        let loader = AssetLoader::new(None, None, None);
        let text = loader.read_config_string().expect("read embedded config");
        let cfg: serde_yaml::Value = serde_yaml::from_str(&text).expect("parse embedded config");
        let devices = cfg.get("devices").expect("devices key present");
        let map = devices.as_mapping().expect("devices is a mapping");
        assert_eq!(
            map.len(),
            1,
            "embedded default config must ship exactly the reserved DEFAULT device"
        );
        assert!(
            map.contains_key(serde_yaml::Value::String("DEFAULT".to_string())),
            "embedded default config's only device must be the reserved DEFAULT key"
        );
    }

    #[test]
    fn test_read_screen_from_subdirectory() {
        let temp_dir = tempfile::tempdir().unwrap();

        let layouts_dir = temp_dir.path().join("layouts");
        std::fs::create_dir(&layouts_dir).unwrap();
        std::fs::write(layouts_dir.join("base.svg"), "<svg>test</svg>").unwrap();

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let result = loader.read_screen_string(Path::new("layouts/base.svg"));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "<svg>test</svg>");
    }

    #[test]
    fn test_collect_screen_files_empty_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let empty_dir = temp_dir.path().join("empty");
        std::fs::create_dir(&empty_dir).unwrap();

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let screens = loader.list_screens();

        // Should not contain any files from empty subdirectory
        assert!(!screens.iter().any(|s| s.starts_with("empty/")));
    }
}
