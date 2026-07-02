//! Screen-package registry: resolve a `handle/path` reference to a `ResolvedScreen`.
//!
//! A *package* is a collection of screens plus shared library/part files, described
//! by a `byonk-screens.yaml` manifest at its root. Each screen lives in a directory
//! that contains a `meta.yaml`. This module abstracts *where* a package's bytes come
//! from (`PackageSource`) so screens can be served identically whether they are
//! embedded in the binary (`byonk-builtin`) or dropped on disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::assets::AssetLoader;
use crate::models::package_manifest::PackageManifest;
use crate::models::screen_meta::ScreenMeta;

/// The handle under which the built-in (embedded + `SCREENS_DIR`) package is registered.
pub const BUILTIN_HANDLE: &str = "byonk-builtin";

/// Reads any file within a package by package-root-relative (manifest-`root`-relative)
/// path, using forward slashes. Implementations must be cheap to share across threads.
pub trait PackageSource: Send + Sync {
    /// Read a package file's raw bytes, or `None` if it does not exist.
    fn read(&self, rel: &str) -> Option<Vec<u8>>;

    /// Read a package file as a UTF-8 string, or `None` if missing / not valid UTF-8.
    fn read_string(&self, rel: &str) -> Option<String> {
        self.read(rel).and_then(|b| String::from_utf8(b).ok())
    }

    /// All screen directories in this package (dirs containing a `meta.yaml`),
    /// relative to the manifest `root` (or the package root if no `root`).
    fn screen_paths(&self) -> Vec<String>;

    /// Package-relative paths of every `.svg` file in the package. Used to scope
    /// Tera template registration to one package (screens + shared parts).
    fn svg_files(&self) -> Vec<String>;

    /// The parsed package manifest.
    fn manifest(&self) -> &PackageManifest;
}

/// A fully resolved screen: its parsed metadata plus a handle back to its package
/// so siblings (`lib/*.lua` for `require`, `parts/*.svg` for includes) can be read.
pub struct ResolvedScreen {
    /// Package handle, e.g. `"byonk-builtin"` or `"acme"`.
    pub handle: String,
    /// Screen path within the package, e.g. `"useful/gphoto"`.
    pub path: String,
    /// Parsed `meta.yaml`.
    pub meta: ScreenMeta,
    /// The screen's package source, for reading sibling files.
    pub source: Arc<dyn PackageSource>,
    /// Manifest-root-relative directory of the screen (same as `path`).
    pub screen_dir: String,
}

/// Join a manifest-`root` prefix with a package-relative path.
fn join_rel(prefix: &str, rel: &str) -> String {
    if prefix.is_empty() || prefix == "." {
        rel.to_string()
    } else {
        format!("{}/{}", prefix.trim_end_matches('/'), rel)
    }
}

/// Split `"handle/path"` on the FIRST `/`. The path portion may itself contain `/`.
fn split_ref(screen_ref: &str) -> Option<(&str, &str)> {
    let (handle, path) = screen_ref.split_once('/')?;
    if handle.is_empty() || path.is_empty() {
        None
    } else {
        Some((handle, path))
    }
}

/// A package that lives in an on-disk directory. Files are read with `fs::read`;
/// screens are discovered by walking the manifest root for `meta.yaml` files.
pub struct DiskPackageSource {
    manifest: PackageManifest,
    /// Directory the manifest-relative paths resolve against (`root.join(manifest.root)`).
    manifest_root: PathBuf,
}

impl DiskPackageSource {
    /// Load a disk package rooted at `root`. Returns `Err` (skip) if the
    /// `byonk-screens.yaml` manifest is missing or invalid.
    pub fn load(root: &Path) -> Result<DiskPackageSource, String> {
        let manifest_path = root.join("byonk-screens.yaml");
        let src = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("cannot read {}: {e}", manifest_path.display()))?;
        let manifest = PackageManifest::from_yaml(&src)?;
        let manifest_root = match manifest.root.as_deref() {
            Some(r) if !r.is_empty() && r != "." => root.join(r),
            _ => root.to_path_buf(),
        };
        Ok(DiskPackageSource {
            manifest,
            manifest_root,
        })
    }

    /// Recursively collect directories (relative to `base`) that contain a `meta.yaml`.
    fn walk_screens(base: &Path, current: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.join("meta.yaml").is_file() {
                    if let Ok(rel) = path.strip_prefix(base) {
                        if let Some(s) = rel.to_str() {
                            out.push(s.replace('\\', "/"));
                        }
                    }
                }
                Self::walk_screens(base, &path, out);
            }
        }
    }

    /// Recursively collect files (relative to `base`) whose extension is `ext`.
    fn walk_ext(base: &Path, current: &Path, ext: &str, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::walk_ext(base, &path, ext, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
                if let Ok(rel) = path.strip_prefix(base) {
                    if let Some(s) = rel.to_str() {
                        out.push(s.replace('\\', "/"));
                    }
                }
            }
        }
    }
}

impl PackageSource for DiskPackageSource {
    fn read(&self, rel: &str) -> Option<Vec<u8>> {
        std::fs::read(self.manifest_root.join(rel)).ok()
    }

    fn screen_paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        Self::walk_screens(&self.manifest_root, &self.manifest_root, &mut out);
        out.sort();
        out
    }

    fn svg_files(&self) -> Vec<String> {
        let mut out = Vec::new();
        Self::walk_ext(&self.manifest_root, &self.manifest_root, "svg", &mut out);
        out.sort();
        out
    }

    fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }
}

/// The built-in package, backed by the embedded `screens/` tree (optionally
/// overlaid by `SCREENS_DIR`) via `AssetLoader`.
pub struct EmbeddedBuiltinSource {
    loader: Arc<AssetLoader>,
    manifest: PackageManifest,
    /// Manifest-`root` prefix within the screens tree (empty when no `root`).
    root_prefix: String,
}

impl EmbeddedBuiltinSource {
    /// Load the built-in package. Returns `Err` (skip) if `byonk-screens.yaml`
    /// is missing or invalid.
    pub fn load(loader: Arc<AssetLoader>) -> Result<EmbeddedBuiltinSource, String> {
        let src = loader
            .read_screen_string(Path::new("byonk-screens.yaml"))
            .map_err(|e| format!("cannot read embedded byonk-screens.yaml: {e}"))?;
        let manifest = PackageManifest::from_yaml(&src)?;
        let root_prefix = match manifest.root.as_deref() {
            Some(r) if !r.is_empty() && r != "." => r.trim_end_matches('/').to_string(),
            _ => String::new(),
        };
        Ok(EmbeddedBuiltinSource {
            loader,
            manifest,
            root_prefix,
        })
    }
}

impl PackageSource for EmbeddedBuiltinSource {
    fn read(&self, rel: &str) -> Option<Vec<u8>> {
        let full = join_rel(&self.root_prefix, rel);
        self.loader
            .read_screen(Path::new(&full))
            .ok()
            .map(|b| b.into_owned())
    }

    fn screen_paths(&self) -> Vec<String> {
        let prefix = if self.root_prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", self.root_prefix)
        };
        let mut out: Vec<String> = self
            .loader
            .list_screens()
            .into_iter()
            .filter_map(|entry| {
                // Keep only entries under the root prefix that are `meta.yaml` files.
                let under = entry.strip_prefix(&prefix)?;
                let dir = under.strip_suffix("meta.yaml")?;
                // `dir` ends with '/' (e.g. "weather/forecast/"); trim it. A bare
                // "meta.yaml" at the root yields an empty dir which we skip.
                let dir = dir.trim_end_matches('/');
                if dir.is_empty() {
                    None
                } else {
                    Some(dir.to_string())
                }
            })
            .collect();
        out.sort();
        out
    }

    fn svg_files(&self) -> Vec<String> {
        let prefix = if self.root_prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", self.root_prefix)
        };
        let mut out: Vec<String> = self
            .loader
            .list_screens()
            .into_iter()
            .filter_map(|entry| {
                let under = entry.strip_prefix(&prefix)?;
                if under.ends_with(".svg") {
                    Some(under.to_string())
                } else {
                    None
                }
            })
            .collect();
        out.sort();
        out
    }

    fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }
}

/// Resolves screen references against a registry of packages keyed by handle.
pub struct PackageLoader {
    registry: HashMap<String, Arc<dyn PackageSource>>,
}

impl PackageLoader {
    /// Build a loader. The `byonk-builtin` handle is always registered (backed by
    /// the embedded tree + `SCREENS_DIR` overlay). Each `disk_packages` entry maps
    /// a handle to a package root directory. Packages whose manifest is
    /// missing/invalid are skipped with a warning.
    pub fn new(asset_loader: Arc<AssetLoader>, disk_packages: HashMap<String, PathBuf>) -> Self {
        let mut registry: HashMap<String, Arc<dyn PackageSource>> = HashMap::new();

        match EmbeddedBuiltinSource::load(asset_loader) {
            Ok(src) => {
                registry.insert(BUILTIN_HANDLE.to_string(), Arc::new(src));
            }
            Err(e) => {
                tracing::warn!(error = %e, "skipping byonk-builtin package: unreadable manifest");
            }
        }

        for (handle, root) in disk_packages {
            match DiskPackageSource::load(&root) {
                Ok(src) => {
                    registry.insert(handle, Arc::new(src));
                }
                Err(e) => {
                    tracing::warn!(handle = %handle, root = %root.display(), error = %e, "skipping disk package: invalid manifest");
                }
            }
        }

        PackageLoader { registry }
    }

    /// Resolve `"handle/path"` to a screen. `None` if the handle is unknown, the
    /// screen dir has no `meta.yaml`, or the `meta.yaml` fails to parse.
    pub fn resolve(&self, screen_ref: &str) -> Option<ResolvedScreen> {
        let (handle, path) = split_ref(screen_ref)?;
        let source = self.registry.get(handle)?;
        if !source.screen_paths().iter().any(|p| p == path) {
            return None;
        }
        let meta_rel = format!("{path}/meta.yaml");
        let meta_src = source.read_string(&meta_rel)?;
        let meta = ScreenMeta::from_yaml(&meta_src).ok()?;
        Some(ResolvedScreen {
            handle: handle.to_string(),
            path: path.to_string(),
            meta,
            source: Arc::clone(source),
            screen_dir: path.to_string(),
        })
    }

    /// Resolve every screen in every registered package.
    pub fn list_all(&self) -> Vec<ResolvedScreen> {
        let mut out = Vec::new();
        for (handle, source) in &self.registry {
            for path in source.screen_paths() {
                if let Some(r) = self.resolve(&format!("{handle}/{path}")) {
                    out.push(r);
                }
            }
        }
        out
    }

    /// All registered package handles.
    pub fn handles(&self) -> Vec<String> {
        let mut h: Vec<String> = self.registry.keys().cloned().collect();
        h.sort();
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    fn write(dir: &std::path::Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    #[test]
    fn test_resolve_disk_package() {
        let tmp = std::env::temp_dir().join(format!("byonk_pkg_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
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
        write(&tmp, "lib/util.lua", "return {}\n");

        let loader = std::sync::Arc::new(crate::assets::AssetLoader::new(None, None, None));
        let mut disk = HashMap::new();
        disk.insert("acme".to_string(), tmp.clone());
        let pl = PackageLoader::new(loader, disk);

        let r = pl.resolve("acme/weather/forecast").expect("resolve");
        assert_eq!(r.handle, "acme");
        assert_eq!(r.path, "weather/forecast");
        assert_eq!(r.meta.title, "F");
        assert_eq!(
            r.source.read_string("lib/util.lua").as_deref(),
            Some("return {}\n")
        );
        assert!(pl.resolve("acme/nope").is_none());
        assert!(pl.resolve("ghost/x").is_none());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_builtin_always_registered() {
        let loader = std::sync::Arc::new(crate::assets::AssetLoader::new(None, None, None));
        let pl = PackageLoader::new(loader, HashMap::new());
        assert!(pl.handles().contains(&"byonk-builtin".to_string()));
    }
}
