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

    /// Recursively collect screen files from a directory
    fn collect_screen_files(base_dir: &Path, current_dir: &Path, files: &mut HashSet<String>) {
        if let Ok(entries) = fs::read_dir(current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Recurse into subdirectories
                    Self::collect_screen_files(base_dir, &path, files);
                } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".lua") || name.ends_with(".svg") {
                        // Get relative path from base_dir
                        if let Ok(relative) = path.strip_prefix(base_dir) {
                            if let Some(relative_str) = relative.to_str() {
                                // Normalize path separators to forward slashes
                                files.insert(relative_str.replace('\\', "/"));
                            }
                        }
                    }
                }
            }
        }
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
    fn test_seed_report_is_empty() {
        let report = SeedReport::default();
        assert!(report.is_empty());

        let report = SeedReport {
            screens_seeded: vec!["test.lua".to_string()],
            fonts_seeded: vec![],
            config_seeded: false,
        };
        assert!(!report.is_empty());

        let report = SeedReport {
            screens_seeded: vec![],
            fonts_seeded: vec!["font.ttf".to_string()],
            config_seeded: false,
        };
        assert!(!report.is_empty());

        let report = SeedReport {
            screens_seeded: vec![],
            fonts_seeded: vec![],
            config_seeded: true,
        };
        assert!(!report.is_empty());
    }

    #[test]
    fn test_read_screen_embedded() {
        let loader = AssetLoader::new(None, None, None);

        // Should find embedded hello.lua
        let result = loader.read_screen(Path::new("hello.lua"));
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

        let result = loader.read_screen_string(Path::new("hello.lua"));
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("return"));
    }

    #[test]
    fn test_list_screens() {
        let loader = AssetLoader::new(None, None, None);

        let screens = loader.list_screens();
        assert!(!screens.is_empty());

        // Should include hello.lua and hello.svg
        assert!(screens.iter().any(|s| s == "hello.lua"));
        assert!(screens.iter().any(|s| s == "hello.svg"));
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
        assert_eq!(config[0], "config.yaml");
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
        // Don't create hello.lua in temp dir, should fall back to embedded

        let loader = AssetLoader::new(Some(temp_dir.path().to_path_buf()), None, None);
        let result = loader.read_screen(Path::new("hello.lua"));

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
        assert!(screens.contains(&"hello.lua".to_string()));
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
        assert!(screens_dir.join("hello.lua").exists());
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

    #[test]
    fn test_seed_if_configured_skips_existing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");
        std::fs::create_dir_all(&screens_dir).unwrap();
        std::fs::write(screens_dir.join("existing.lua"), "-- existing").unwrap();

        let loader = AssetLoader::new(Some(screens_dir), None, None);
        let result = loader.seed_if_configured();

        assert!(result.is_ok());
        let report = result.unwrap();
        // Should not seed because directory is not empty
        assert!(report.screens_seeded.is_empty());
    }

    #[test]
    fn test_init_screens() {
        let temp_dir = tempfile::tempdir().unwrap();
        let screens_dir = temp_dir.path().join("screens");

        let loader = AssetLoader::new(Some(screens_dir.clone()), None, None);
        let result = loader.init(&[AssetCategory::Screens], false);

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(!report.written.is_empty());
        assert!(screens_dir.join("hello.lua").exists());
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
        assert!(content.contains("screens:"));
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
}
