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

/// Embedded screen assets (Lua scripts, SVG templates, and images)
#[derive(RustEmbed)]
#[folder = "screens/"]
#[include = "*.lua"]
#[include = "*.svg"]
#[include = "*.png"]
#[include = "*.jpg"]
#[include = "*.jpeg"]
#[include = "*.gif"]
#[include = "*.webp"]
#[include = "**/*.lua"]
#[include = "**/*.svg"]
#[include = "**/*.png"]
#[include = "**/*.jpg"]
#[include = "**/*.jpeg"]
#[include = "**/*.gif"]
#[include = "**/*.webp"]
struct EmbeddedScreens;

/// Embedded font assets
#[derive(RustEmbed)]
#[folder = "fonts/"]
struct EmbeddedFonts;

/// Embedded default config
#[derive(RustEmbed)]
#[folder = "."]
#[include = "config.yaml"]
struct EmbeddedConfig;

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
    pub fonts_seeded: Vec<String>,
    pub config_seeded: bool,
}

impl SeedReport {
    pub fn is_empty(&self) -> bool {
        self.screens_seeded.is_empty() && self.fonts_seeded.is_empty() && !self.config_seeded
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
        Self {
            screens_dir,
            fonts_dir,
            config_file,
        }
    }

    /// Read a screen asset (Lua script or SVG template)
    ///
    /// If an external path is configured, tries filesystem first, then falls back to embedded.
    /// If no external path is configured, uses embedded only.
    pub fn read_screen(&self, relative_path: &Path) -> io::Result<Cow<'static, [u8]>> {
        // Try external first if path configured
        if let Some(ref dir) = self.screens_dir {
            let full_path = dir.join(relative_path);
            if full_path.exists() {
                tracing::trace!(path = %full_path.display(), "Loading screen from filesystem");
                return Ok(Cow::Owned(fs::read(&full_path)?));
            }
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

    /// Read a screen asset as a UTF-8 string
    pub fn read_screen_string(&self, relative_path: &Path) -> io::Result<String> {
        let bytes = self.read_screen(relative_path)?;
        String::from_utf8(bytes.into_owned())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// List all available screens (merged view of embedded + external)
    pub fn list_screens(&self) -> Vec<String> {
        let mut files: HashSet<String> = EmbeddedScreens::iter().map(|s| s.to_string()).collect();

        if let Some(ref dir) = self.screens_dir {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".lua") || name.ends_with(".svg") {
                            files.insert(name.to_string());
                        }
                    }
                }
            }
        }

        let mut result: Vec<_> = files.into_iter().collect();
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
        EmbeddedConfig::get("config.yaml")
            .map(|f| {
                tracing::trace!("Loading config from embedded assets");
                f.data
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Embedded config.yaml not found")
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
    pub fn seed_if_configured(&self) -> io::Result<SeedReport> {
        let mut report = SeedReport::default();

        // Seed screens
        if let Some(ref dir) = self.screens_dir {
            let should_seed = !dir.exists() || Self::is_empty_dir(dir);
            if should_seed {
                fs::create_dir_all(dir)?;
                for file in EmbeddedScreens::iter() {
                    if let Some(data) = EmbeddedScreens::get(&file) {
                        let path = dir.join(file.as_ref());
                        // Create parent directories for nested assets (e.g., default/background.jpg)
                        if let Some(parent) = path.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::write(&path, &*data.data)?;
                        report.screens_seeded.push(file.to_string());
                    }
                }
                if !report.screens_seeded.is_empty() {
                    tracing::info!(
                        dir = %dir.display(),
                        count = report.screens_seeded.len(),
                        "Seeded screens directory with embedded assets"
                    );
                }
            }
        }

        // Seed fonts
        if let Some(ref dir) = self.fonts_dir {
            let should_seed = !dir.exists() || Self::is_empty_dir(dir);
            if should_seed {
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
            }
        }

        // Seed config
        if let Some(ref path) = self.config_file {
            if !path.exists() {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                if let Some(data) = EmbeddedConfig::get("config.yaml") {
                    fs::write(path, &*data.data)?;
                    report.config_seeded = true;
                    tracing::info!(path = %path.display(), "Seeded config file with embedded default");
                }
            }
        }

        Ok(report)
    }

    /// Extract embedded assets to filesystem (init command)
    ///
    /// Uses the configured paths (or defaults if not set).
    pub fn init(&self, categories: &[AssetCategory], force: bool) -> io::Result<InitReport> {
        let mut report = InitReport::default();

        for category in categories {
            match category {
                AssetCategory::Screens => {
                    let dir = self
                        .screens_dir
                        .clone()
                        .unwrap_or_else(|| PathBuf::from("./screens"));
                    fs::create_dir_all(&dir)?;

                    for file in EmbeddedScreens::iter() {
                        let path = dir.join(file.as_ref());
                        if !force && path.exists() {
                            report.skipped.push(path.display().to_string());
                            continue;
                        }
                        if let Some(data) = EmbeddedScreens::get(&file) {
                            // Create parent directories for nested assets (e.g., default/background.jpg)
                            if let Some(parent) = path.parent() {
                                fs::create_dir_all(parent)?;
                            }
                            fs::write(&path, &*data.data)?;
                            report.written.push(path.display().to_string());
                        }
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
                    if let Some(data) = EmbeddedConfig::get("config.yaml") {
                        fs::write(&path, &*data.data)?;
                        report.written.push(path.display().to_string());
                    }
                }
            }
        }

        Ok(report)
    }

    /// List embedded assets by category (for display)
    pub fn list_embedded(category: AssetCategory) -> Vec<String> {
        match category {
            AssetCategory::Screens => EmbeddedScreens::iter().map(|s| s.to_string()).collect(),
            AssetCategory::Fonts => EmbeddedFonts::iter().map(|s| s.to_string()).collect(),
            AssetCategory::Config => vec!["config.yaml".to_string()],
        }
    }
}
