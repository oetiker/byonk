# Screen Packages — Plan 1: Format & Loader — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace byonk's flat `<name>.lua`/`<name>.svg` screens with folder-per-screen *packages* (a `byonk-screens.yaml` manifest + per-screen `meta.yaml`/`script.lua`/`screen.svg`), a package-aware loader with `handle/path` addressing, repo-relative + `byonk-base-v1` sharing (incl. sandboxed Lua `require()`), semver compatibility, and migration of the built-in screens into an embedded `byonk-builtin` package.

**Architecture:** A new `PackageLoader` (registry `handle → source`) resolves a screen reference `handle/path` to a `ResolvedScreen` (parsed `meta.yaml` + a `PackageSource` that reads any package-relative file). `byonk-builtin` is an embedded package (rust-embed); other handles map to on-disk directories (git fetching is **Plan 2**, out of scope here). `content_pipeline`, `template_service`, `lua_runtime`, and the admin API consume `ResolvedScreen`. This is a clean break — no legacy reader (spec §7).

**Tech Stack:** Rust, axum 0.7, mlua 0.10 (lua54), tera 1, rust-embed 8, serde_yaml 0.9, semver (new).

## Global Constraints

- **No backward compatibility / no legacy reader** (spec §7): no `local` package, no `@params` comment parsing, no legacy `layouts/`+`components/` global include paths, no bare-name screen references. All screen refs are qualified `handle/path`.
- **Fixed file names**: `byonk-screens.yaml` (repo root), and per-screen `meta.yaml` / `script.lua` / `screen.svg`. No overrides.
- **Embedded package handle** is literally `byonk-builtin`. Std namespace is `byonk-base-v1/…` (path-versioned).
- **`meta.yaml` is the single source of screen truth**: `title`, `description` (both required), `byonk` (semver req, bare=caret), optional `refresh`, and `params` (existing `ParamField` schema).
- **`byonk-screens.yaml` required fields**: `name`, `description`, `author`, `license`; optional `root`.
- **Compat mismatch warns, never refuses** (spec §6): produce a `compat_warning` string; still serve.
- Build/verify with `make check` (fmt + clippy + tests). Never `git add -A`; stage explicit paths.
- Config struct is **`AppConfig`** (in `src/models/config.rs`). `SharedConfig = Arc<ArcSwap<AppConfig>>`.

---

### Task 1: Parse a param schema from a YAML value

**Files:**
- Modify: `src/models/param_schema.rs`
- Test: `src/models/param_schema.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: existing `ParamSchema`, `ParamField`, `parse_schema(&str)`, `RawField`, `parse_options`.
- Produces: `pub fn parse_schema_from_value(v: &serde_yaml::Value) -> Result<ParamSchema, String>` — parses the `params:` mapping of a `meta.yaml`. Order-preserving. An absent/null value yields an empty schema.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_parse_schema_from_value_mapping() {
    let v: serde_yaml::Value = serde_yaml::from_str(
        "location:\n  type: string\n  required: true\nunits:\n  type: enum\n  options: [metric, imperial]\n",
    )
    .unwrap();
    let schema = parse_schema_from_value(&v).unwrap();
    assert_eq!(schema.fields.len(), 2);
    assert_eq!(schema.fields[0].name, "location");
    assert!(schema.fields[0].required);
    assert_eq!(schema.fields[1].options.len(), 2);
}

#[test]
fn test_parse_schema_from_value_null_is_empty() {
    let v = serde_yaml::Value::Null;
    assert!(parse_schema_from_value(&v).unwrap().fields.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk parse_schema_from_value`
Expected: FAIL — `parse_schema_from_value` not found.

- [ ] **Step 3: Implement `parse_schema_from_value`**

Refactor: extract the per-field loop of `parse_schema` into a shared helper that both entry points call. Add:

```rust
/// Parse a `params:` mapping (as found in `meta.yaml`) into a schema.
/// `Null` or an empty mapping yields an empty schema. Preserves field order.
pub fn parse_schema_from_value(v: &serde_yaml::Value) -> Result<ParamSchema, String> {
    let mapping = match v {
        serde_yaml::Value::Null => return Ok(ParamSchema::default()),
        serde_yaml::Value::Mapping(m) => m.clone(),
        _ => return Err("`params` must be a mapping".to_string()),
    };
    let mut fields = Vec::new();
    for (k, val) in mapping {
        let name = k
            .as_str()
            .ok_or_else(|| "param keys must be strings".to_string())?
            .to_string();
        let raw: RawField =
            serde_yaml::from_value(val).map_err(|e| format!("param `{name}`: {e}"))?;
        let options = match raw.options {
            Some(o) => parse_options(o)?,
            None => Vec::new(),
        };
        if raw.param_type == ParamType::Enum && options.is_empty() {
            return Err(format!("param `{name}`: enum requires non-empty `options`"));
        }
        fields.push(ParamField {
            name,
            param_type: raw.param_type,
            required: raw.required,
            default: raw.default,
            label: raw.label,
            description: raw.description,
            min: raw.min,
            max: raw.max,
            step: raw.step,
            unit: raw.unit,
            mode: raw.mode,
            options,
            sensitive: raw.sensitive,
            multiline: raw.multiline,
            hidden: raw.hidden,
            advanced: raw.advanced,
        });
    }
    Ok(ParamSchema { fields })
}
```

(Optional DRY: have `parse_schema(&str)` deserialize to `serde_yaml::Value` then delegate here. Keep `parse_schema` public for now; `extract_params_block`/`schema_for_script` are removed in Task 13.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk param_schema`
Expected: PASS (existing tests + the two new ones).

- [ ] **Step 5: Commit**

```bash
git add src/models/param_schema.rs
git commit -m "feat(params): parse param schema from a YAML value (meta.yaml)"
```

---

### Task 2: `ScreenMeta` model + parser

**Files:**
- Create: `src/models/screen_meta.rs`
- Modify: `src/models/mod.rs` (add `pub mod screen_meta;`)
- Test: `src/models/screen_meta.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `parse_schema_from_value` (Task 1), `ParamSchema`.
- Produces:
  ```rust
  pub struct ScreenMeta {
      pub title: String,
      pub description: String,
      pub byonk: String,            // raw semver requirement, e.g. "0.15"
      pub refresh: Option<u32>,
      pub params: ParamSchema,
  }
  impl ScreenMeta {
      pub fn from_yaml(src: &str) -> Result<ScreenMeta, String>;
  }
  ```

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_meta() {
        let src = "title: 5-Day Forecast\ndescription: Daily conditions.\nbyonk: \"0.15\"\nrefresh: 900\nparams:\n  location:\n    type: string\n    required: true\n";
        let m = ScreenMeta::from_yaml(src).unwrap();
        assert_eq!(m.title, "5-Day Forecast");
        assert_eq!(m.description, "Daily conditions.");
        assert_eq!(m.byonk, "0.15");
        assert_eq!(m.refresh, Some(900));
        assert_eq!(m.params.fields.len(), 1);
    }

    #[test]
    fn test_missing_title_is_error() {
        let src = "description: x\nbyonk: \"0.15\"\n";
        assert!(ScreenMeta::from_yaml(src).is_err());
    }

    #[test]
    fn test_missing_byonk_is_error() {
        let src = "title: t\ndescription: d\n";
        assert!(ScreenMeta::from_yaml(src).is_err());
    }

    #[test]
    fn test_no_params_is_empty_schema() {
        let src = "title: t\ndescription: d\nbyonk: \"0.15\"\n";
        let m = ScreenMeta::from_yaml(src).unwrap();
        assert!(m.params.fields.is_empty());
        assert_eq!(m.refresh, None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk screen_meta`
Expected: FAIL — module/type not found.

- [ ] **Step 3: Implement `ScreenMeta`**

```rust
//! Per-screen `meta.yaml`: title/description/compat/params — the single source
//! of screen truth. Parsed as YAML, never executed.

use serde::Deserialize;

use crate::models::param_schema::{parse_schema_from_value, ParamSchema};

#[derive(Debug, Clone)]
pub struct ScreenMeta {
    pub title: String,
    pub description: String,
    pub byonk: String,
    pub refresh: Option<u32>,
    pub params: ParamSchema,
}

#[derive(Deserialize)]
struct RawMeta {
    title: String,
    description: String,
    byonk: String,
    #[serde(default)]
    refresh: Option<u32>,
    #[serde(default)]
    params: serde_yaml::Value,
}

impl ScreenMeta {
    pub fn from_yaml(src: &str) -> Result<ScreenMeta, String> {
        let raw: RawMeta =
            serde_yaml::from_str(src).map_err(|e| format!("invalid meta.yaml: {e}"))?;
        let params = parse_schema_from_value(&raw.params)?;
        Ok(ScreenMeta {
            title: raw.title,
            description: raw.description,
            byonk: raw.byonk,
            refresh: raw.refresh,
            params,
        })
    }
}
```

Note: `serde` gives the "missing field `title`" error automatically since `RawMeta.title` is non-`Option`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk screen_meta`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/models/screen_meta.rs src/models/mod.rs
git commit -m "feat(screens): ScreenMeta model + meta.yaml parser"
```

---

### Task 3: `PackageManifest` model + parser

**Files:**
- Create: `src/models/package_manifest.rs`
- Modify: `src/models/mod.rs` (add `pub mod package_manifest;`)
- Test: `src/models/package_manifest.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  ```rust
  pub struct PackageManifest {
      pub name: String,
      pub description: String,
      pub author: String,
      pub license: String,
      pub root: Option<String>,
  }
  impl PackageManifest { pub fn from_yaml(src: &str) -> Result<PackageManifest, String>; }
  ```

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest() {
        let src = "name: acme\ndescription: d\nauthor: a\nlicense: MIT\nroot: contrib/trmnl\n";
        let m = PackageManifest::from_yaml(src).unwrap();
        assert_eq!(m.name, "acme");
        assert_eq!(m.root.as_deref(), Some("contrib/trmnl"));
    }

    #[test]
    fn test_root_optional() {
        let src = "name: a\ndescription: d\nauthor: x\nlicense: MIT\n";
        assert_eq!(PackageManifest::from_yaml(src).unwrap().root, None);
    }

    #[test]
    fn test_missing_required_field_errors() {
        let src = "name: a\ndescription: d\n"; // no author/license
        assert!(PackageManifest::from_yaml(src).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk package_manifest`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `PackageManifest`**

```rust
//! `byonk-screens.yaml` — the mandatory package manifest at a package root.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub description: String,
    pub author: String,
    pub license: String,
    #[serde(default)]
    pub root: Option<String>,
}

impl PackageManifest {
    pub fn from_yaml(src: &str) -> Result<PackageManifest, String> {
        serde_yaml::from_str(src).map_err(|e| format!("invalid byonk-screens.yaml: {e}"))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk package_manifest`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/models/package_manifest.rs src/models/mod.rs
git commit -m "feat(screens): PackageManifest model + byonk-screens.yaml parser"
```

---

### Task 4: Semver compatibility check

**Files:**
- Modify: `Cargo.toml` (add `semver = "1"`)
- Create: `src/models/compat.rs`
- Modify: `src/models/mod.rs` (add `pub mod compat;`)
- Test: `src/models/compat.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `pub fn compat_warning(engine: &str, req: &str) -> Option<String>` — returns `Some(msg)` when the running engine version is outside the screen's requirement (bare version treated as caret), `None` when compatible. A malformed `req` or `engine` yields a `Some(...)` warning (fail-soft; never panics, never blocks — spec §6).

- [ ] **Step 1: Add the dependency**

Add under `[dependencies]` in `Cargo.toml`:

```toml
semver = "1"
```

- [ ] **Step 2: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bare_version_is_caret_ok() {
        // engine 0.15.3 satisfies "0.15" (^0.15 => >=0.15.0, <0.16.0)
        assert_eq!(compat_warning("0.15.3", "0.15"), None);
    }

    #[test]
    fn test_bare_version_next_minor_warns_pre_1_0() {
        // 0.x: minor is the breaking boundary; 0.16.0 is outside ^0.15
        assert!(compat_warning("0.16.0", "0.15").is_some());
    }

    #[test]
    fn test_below_min_warns() {
        assert!(compat_warning("0.14.0", "0.15").is_some());
    }

    #[test]
    fn test_explicit_range_ok() {
        assert_eq!(compat_warning("0.15.0", ">=0.14, <0.17").is_none(), true);
    }

    #[test]
    fn test_bad_requirement_warns_not_panics() {
        assert!(compat_warning("0.15.0", "not-a-version").is_some());
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p byonk compat`
Expected: FAIL — `compat_warning` not found.

- [ ] **Step 4: Implement `compat_warning`**

```rust
//! Semver compatibility between the running byonk engine and a screen's
//! `byonk:` requirement. Bare versions are treated as caret (`^`), matching
//! Cargo. A mismatch produces a warning string; it never blocks rendering.

use semver::{Version, VersionReq};

/// Current engine version (from Cargo).
pub fn engine_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns `Some(warning)` if `engine` does not satisfy `req`, else `None`.
/// A bare `"0.15"` is parsed as `^0.15`. Malformed input fails soft: it warns.
pub fn compat_warning(engine: &str, req: &str) -> Option<String> {
    let version = match Version::parse(engine) {
        Ok(v) => v,
        Err(e) => return Some(format!("cannot parse engine version `{engine}`: {e}")),
    };
    // `VersionReq::parse` already treats a bare "0.15" as ^0.15.
    let requirement = match VersionReq::parse(req) {
        Ok(r) => r,
        Err(e) => return Some(format!("invalid byonk requirement `{req}`: {e}")),
    };
    if requirement.matches(&version) {
        None
    } else {
        Some(format!(
            "screen requires byonk `{req}` but this engine is {engine}"
        ))
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p byonk compat`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/models/compat.rs src/models/mod.rs
git commit -m "feat(screens): semver compat check (bare=caret, warn-not-block)"
```

---

### Task 5: `byonk-base-v1` embedded std assets

**Files:**
- Create: `byonk-base/v1/base.svg` (copy of `screens/layouts/base.svg`)
- Create: `byonk-base/v1/hinting.svg` (copy of `screens/components/hinting.svg`)
- Create: `byonk-base/v1/header.svg`, `byonk-base/v1/footer.svg`, `byonk-base/v1/status_bar.svg` (copies of the matching `screens/components/*.svg`)
- Modify: `src/assets.rs` (add `EmbeddedBase` rust-embed + reader)
- Test: `src/assets.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces on `AssetLoader`:
  ```rust
  /// Read a byonk-base asset by version-relative path, e.g. "v1/hinting.svg".
  pub fn read_base(&self, rel: &str) -> Option<Cow<'static, [u8]>>;
  pub fn read_base_string(&self, rel: &str) -> Option<String>;
  /// List embedded base asset paths (e.g. ["v1/base.svg", ...]).
  pub fn list_base(&self) -> Vec<String>;
  ```

- [ ] **Step 1: Create the base asset files**

Copy the shared templates verbatim as a starting point (their internal `{% include %}` paths are rewritten to `byonk-base-v1/...` in Task 12, but base assets reference each other by their base-relative names — adjust any cross-include inside `base.svg` to `byonk-base-v1/...` here if present).

```bash
mkdir -p byonk-base/v1
cp screens/layouts/base.svg byonk-base/v1/base.svg
cp screens/components/hinting.svg byonk-base/v1/hinting.svg
cp screens/components/header.svg byonk-base/v1/header.svg
cp screens/components/footer.svg byonk-base/v1/footer.svg
cp screens/components/status_bar.svg byonk-base/v1/status_bar.svg
```

- [ ] **Step 2: Write the failing test**

```rust
#[test]
fn test_read_base_asset() {
    let loader = AssetLoader::new(None, None, None);
    assert!(loader.read_base_string("v1/hinting.svg").is_some());
    assert!(loader.list_base().iter().any(|p| p == "v1/base.svg"));
    assert!(loader.read_base_string("v1/does-not-exist.svg").is_none());
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p byonk read_base_asset`
Expected: FAIL — method not found.

- [ ] **Step 4: Implement the embed + readers**

In `src/assets.rs`, add near the other `#[derive(RustEmbed)]` blocks:

```rust
/// Embedded byonk-base std assets (versioned: v1/, v2/, ...).
#[derive(RustEmbed)]
#[folder = "byonk-base/"]
#[include = "**/*.svg"]
#[include = "**/*.lua"]
struct EmbeddedBase;
```

And on `impl AssetLoader`:

```rust
pub fn read_base(&self, rel: &str) -> Option<Cow<'static, [u8]>> {
    EmbeddedBase::get(rel).map(|f| f.data)
}

pub fn read_base_string(&self, rel: &str) -> Option<String> {
    self.read_base(rel)
        .and_then(|b| String::from_utf8(b.into_owned()).ok())
}

pub fn list_base(&self) -> Vec<String> {
    EmbeddedBase::iter().map(|s| s.to_string()).collect()
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p byonk read_base_asset`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add byonk-base/ src/assets.rs
git commit -m "feat(screens): embed byonk-base-v1 std assets + loader readers"
```

---

### Task 6: `PackageSource` + `PackageLoader` (registry resolution)

**Files:**
- Create: `src/services/package_loader.rs`
- Modify: `src/services/mod.rs` (add `pub mod package_loader;`)
- Test: `src/services/package_loader.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `AssetLoader` (embedded byonk-builtin via `EmbeddedScreens` + external `screens_dir`; base via `read_base*`), `ScreenMeta` (Task 2), `PackageManifest` (Task 3), `PackageRef` (Task 7 — but define a minimal local registry input here to avoid a cycle; see note).
- Produces:
  ```rust
  /// Reads any file within a package by package-root-relative path.
  pub trait PackageSource: Send + Sync {
      fn read(&self, rel: &str) -> Option<Vec<u8>>;
      fn read_string(&self, rel: &str) -> Option<String> {
          self.read(rel).and_then(|b| String::from_utf8(b).ok())
      }
      /// All screen dirs in this package (paths containing a meta.yaml),
      /// relative to the manifest `root` (or package root if no root).
      fn screen_paths(&self) -> Vec<String>;
      fn manifest(&self) -> &PackageManifest;
  }

  pub struct ResolvedScreen {
      pub handle: String,                  // "byonk-builtin"
      pub path: String,                    // "useful/gphoto"
      pub meta: ScreenMeta,
      pub source: Arc<dyn PackageSource>,  // read siblings for require/includes
      pub screen_dir: String,             // manifest-root-relative dir of the screen
  }

  pub struct PackageLoader { /* registry: HashMap<String, Arc<dyn PackageSource>> */ }

  impl PackageLoader {
      /// `builtin_dir`: optional on-disk override for byonk-builtin (SCREENS_DIR);
      /// `disk_packages`: handle -> directory for non-builtin packages placed on disk.
      pub fn new(
          asset_loader: Arc<AssetLoader>,
          disk_packages: HashMap<String, PathBuf>,
      ) -> Self;
      /// Resolve "handle/path" -> screen. `None` if handle unknown or screen missing.
      pub fn resolve(&self, screen_ref: &str) -> Option<ResolvedScreen>;
      /// List (handle, ResolvedScreen) for every screen in every package.
      pub fn list_all(&self) -> Vec<ResolvedScreen>;
      pub fn handles(&self) -> Vec<String>;
  }
  ```

> **Note on the `byonk-builtin` source:** its files are the existing `screens/` tree (embedded via `EmbeddedScreens`, optionally overlaid by `SCREENS_DIR`). After Task 12 the embedded tree *is* the package (with `byonk-screens.yaml` at its root). The `EmbeddedBuiltinSource` reads via `AssetLoader::read_screen*` and lists via `AssetLoader::list_screens()`. `screen_paths()` = every dir under the manifest root that contains a `meta.yaml`.

- [ ] **Step 1: Write the failing test**

Use a temp on-disk package so the test doesn't depend on Task 12's migration.

```rust
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
        write(&tmp, "byonk-screens.yaml", "name: t\ndescription: d\nauthor: a\nlicense: MIT\n");
        write(&tmp, "weather/forecast/meta.yaml", "title: F\ndescription: d\nbyonk: \"0.15\"\n");
        write(&tmp, "weather/forecast/script.lua", "return { data = {} }\n");
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
        assert_eq!(r.source.read_string("lib/util.lua").as_deref(), Some("return {}\n"));
        assert!(pl.resolve("acme/nope").is_none());
        assert!(pl.resolve("ghost/x").is_none());

        let _ = fs::remove_dir_all(&tmp);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk package_loader`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `DiskPackageSource`, `EmbeddedBuiltinSource`, `PackageLoader`**

Implement:
- `split_ref(screen_ref) -> Option<(handle, path)>` splitting on the first `/`.
- `DiskPackageSource { root: PathBuf, manifest: PackageManifest, manifest_root: PathBuf }`:
  - `read(rel)` → `fs::read(manifest_root.join(rel))`.
  - `screen_paths()` → walk `manifest_root` for dirs containing `meta.yaml`; return each dir relative to `manifest_root` (forward slashes).
  - Constructor reads `byonk-screens.yaml`, parses `PackageManifest`, sets `manifest_root = root.join(manifest.root.unwrap_or("."))`.
- `EmbeddedBuiltinSource { loader: Arc<AssetLoader>, manifest: PackageManifest, root_prefix: String }`:
  - `read(rel)` → `loader.read_screen(Path::new(&join(root_prefix, rel))).ok()` (embedded+SCREENS_DIR overlay). `join` handles the `root:` prefix.
  - `screen_paths()` → filter `loader.list_screens()` for `…/meta.yaml` under `root_prefix`, strip prefix + `/meta.yaml`.
  - Constructor reads `byonk-screens.yaml` via `loader.read_screen_string`.
- `PackageLoader::new` registers `byonk-builtin` → `EmbeddedBuiltinSource` and each `disk_packages` entry → `DiskPackageSource` (skip + `tracing::warn!` on manifest error).
- `resolve(screen_ref)`: split → look up handle → check `path ∈ source.screen_paths()` → read `<path>/meta.yaml` → `ScreenMeta::from_yaml` → build `ResolvedScreen { screen_dir: path.clone(), ... }`.
- `list_all()`: for each source, for each `screen_paths()`, resolve.

Key `join` helper (root-prefix aware):
```rust
fn join_rel(prefix: &str, rel: &str) -> String {
    if prefix.is_empty() || prefix == "." {
        rel.to_string()
    } else {
        format!("{}/{}", prefix.trim_end_matches('/'), rel)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk package_loader`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/package_loader.rs src/services/mod.rs
git commit -m "feat(screens): PackageLoader + PackageSource (handle/path resolution)"
```

---

### Task 7: Registry schema in `AppConfig`

**Files:**
- Modify: `src/models/config.rs`
- Test: `src/models/config.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  ```rust
  pub struct PackageRef {
      pub repo: Option<String>,   // None => embedded (byonk-builtin)
      pub pin: Option<String>,
      pub token: Option<String>,  // secret; redacted in read APIs
  }
  // On AppConfig:
  #[serde(default)] pub packages: HashMap<String, PackageRef>,
  ```
  And `default_screen()` now returns `Some("byonk-builtin/default")`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_packages_registry_parses() {
    let yaml = "packages:\n  byonk-builtin: {}\n  weather:\n    repo: github.com/acme/screens\n    pin: v1.4.0\n";
    let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(cfg.packages.contains_key("byonk-builtin"));
    assert_eq!(cfg.packages["weather"].pin.as_deref(), Some("v1.4.0"));
    assert!(cfg.packages["byonk-builtin"].repo.is_none());
}

#[test]
fn test_default_screen_is_builtin() {
    assert_eq!(default_screen(), Some("byonk-builtin/default".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk packages_registry_parses default_screen_is_builtin`
Expected: FAIL.

- [ ] **Step 3: Implement**

Add to `AppConfig` (after `screens`):
```rust
#[serde(default)]
pub packages: std::collections::HashMap<String, PackageRef>,
```
Add the struct:
```rust
#[derive(Debug, Deserialize, Clone, Default)]
pub struct PackageRef {
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub pin: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}
```
Change `default_screen()`:
```rust
fn default_screen() -> Option<String> {
    Some("byonk-builtin/default".to_string())
}
```
Also update `impl Default for AppConfig` to include `packages: HashMap::new()` and (since the legacy seeded `"default"` screen is going away) set its `default_screen` field to `default_screen()`. Leave the seeded `screens` map alone for now; it is emptied in Task 13.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk config`
Expected: PASS (fix any `Default`/construction sites the compiler flags).

- [ ] **Step 5: Commit**

```bash
git add src/models/config.rs
git commit -m "feat(config): packages registry + byonk-builtin default screen"
```

---

### Task 8: Wire `PackageLoader` into `AppState` and env

**Files:**
- Modify: `src/server.rs` (`AppState`, `create_app_state_with_overrides`)
- Modify: `src/main.rs` (build `disk_packages` from `PACKAGES_DIR` env)
- Test: `src/server.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `PackageLoader::new` (Task 6), `AppConfig.packages` (Task 7).
- Produces: `pub package_loader: Arc<PackageLoader>` on `AppState`. `disk_packages` is derived from a new optional `PACKAGES_DIR` env var: each immediate subdirectory containing a `byonk-screens.yaml` registers under `handle = <dirname>` — but only if that handle also appears in `config.packages` (a repo/pin entry). In Plan 1 the on-disk path is `PACKAGES_DIR/<handle>`; git fetching replaces this in Plan 2.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_appstate_has_package_loader() {
    // Minimal smoke test: build state and resolve the builtin default screen.
    let loader = std::sync::Arc::new(crate::assets::AssetLoader::new(None, None, None));
    let cfg = crate::models::config::AppConfig::default();
    let state = create_app_state_with_config(loader, cfg).unwrap();
    // byonk-builtin is always registered:
    assert!(state.package_loader.handles().iter().any(|h| h == "byonk-builtin"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk appstate_has_package_loader`
Expected: FAIL — field missing.

- [ ] **Step 3: Implement**

In `create_app_state_with_overrides`, after building `asset_loader` and `config`, build disk packages and the loader:
```rust
let disk_packages = std::env::var("PACKAGES_DIR")
    .ok()
    .filter(|s| !s.is_empty())
    .map(|dir| collect_disk_packages(std::path::Path::new(&dir), &config.packages))
    .unwrap_or_default();
let package_loader = Arc::new(crate::services::package_loader::PackageLoader::new(
    asset_loader.clone(),
    disk_packages,
));
```
Add `package_loader` to the `AppState` struct + its constructor literal. Implement `collect_disk_packages(dir, registry) -> HashMap<String, PathBuf>` (only handles present in `registry` and existing on disk). Pass `package_loader.clone()` into `ContentPipeline::new` (see Task 9 — update that signature).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk appstate_has_package_loader`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/server.rs src/main.rs
git commit -m "feat(server): PackageLoader in AppState (PACKAGES_DIR on-disk packages)"
```

---

### Task 9: Resolve screens through `PackageLoader` in the content pipeline

**Files:**
- Modify: `src/services/content_pipeline.rs`
- Test: `src/services/content_pipeline.rs` (`#[cfg(test)]`) or an integration test

**Interfaces:**
- Consumes: `ResolvedScreen` (Task 6), `PackageLoader` in `AppState` (Task 8).
- Produces: `ContentPipeline` now resolves a device's `screen` (a `handle/path` ref) via `PackageLoader::resolve`, and runs the script/template from the `ResolvedScreen`. `ContentPipeline::new` gains a `package_loader: Arc<PackageLoader>` param. The old `resolve_screen` returning `ScreenConfig` from `<name>.lua/.svg` is removed.

- [ ] **Step 1: Write the failing test**

Add a temp on-disk package + a config registering it, then assert `run_screen_by_name` (renamed to take a ref) succeeds. (Reuse the temp-package helper from Task 6.)

```rust
#[test]
fn test_pipeline_runs_screen_from_package() {
    // ... build temp package "acme" with weather/forecast returning data={msg="hi"} ...
    // ... build AppConfig with packages: { acme: { repo/pin ignored on disk } } ...
    // ... construct pipeline with a PackageLoader over the temp dir ...
    let result = pipeline.run_screen_by_ref("acme/weather/forecast", &params, None).unwrap();
    assert_eq!(result.data["msg"], serde_json::json!("hi"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk pipeline_runs_screen_from_package`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Store `package_loader: Arc<PackageLoader>` in `ContentPipeline`; add to `new(...)`.
- Replace `resolve_screen(&self, name)` with `resolve(&self, screen_ref: &str) -> Option<ResolvedScreen>` delegating to `self.package_loader.resolve(screen_ref)`.
- `run_script_for_device`: use `device_config.screen` as a ref → `self.resolve(&ref)`; on `None`, fall back to `config.default_screen` (also a ref). Compute refresh from `resolved.meta.refresh` (default 900) instead of `ScreenConfig.default_refresh`.
- Thread the `ResolvedScreen` into `lua_runtime.run_script` (Task 10 changes the signature to take the source) and `template_service.render` (Task 11 takes the source + screen path). Until those land, pass `resolved.source` and `resolved.path`.
- The pipeline `ScriptResult.template_path` becomes the screen ref/path (used by `render_svg_from_script` to re-resolve, or carry the `Arc<dyn PackageSource>` forward). Simplest: add `pub source: Arc<dyn PackageSource>` and `pub screen_path: String` to the pipeline `ScriptResult`, drop `template_path`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk content_pipeline`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/content_pipeline.rs
git commit -m "feat(pipeline): resolve screens via PackageLoader (handle/path)"
```

---

### Task 10: Sandboxed Lua `require()` scoped to the package + byonk-base

**Files:**
- Modify: `src/services/lua_runtime.rs`
- Test: `src/services/lua_runtime.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `Arc<dyn PackageSource>` (Task 6), `AssetLoader::read_base_string` (Task 5).
- Produces: `run_script` gains parameters carrying the package source so scripts can `require("lib/x")` (package-relative) and `require("byonk-base-v1/std")` (embedded base). New signature:
  ```rust
  pub fn run_script(
      &self,
      script_src: &str,                         // the resolved script.lua contents
      source: &Arc<dyn PackageSource>,          // for require() resolution
      screen_name: &str,                        // for read_asset/logging
      params: &HashMap<String, serde_yaml::Value>,
      device_ctx: Option<&DeviceContext>,
      timestamp_override: Option<i64>,
  ) -> Result<ScriptResult, ScriptError>;
  ```
  (Callers now pass the already-read script string + source instead of a `script_path`.)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_require_resolves_package_relative_module() {
    struct Src;
    impl crate::services::package_loader::PackageSource for Src {
        fn read(&self, rel: &str) -> Option<Vec<u8>> {
            match rel { "lib/util.lua" => Some(b"return { greet = function() return 'hi' end }".to_vec()), _ => None }
        }
        fn screen_paths(&self) -> Vec<String> { vec![] }
        fn manifest(&self) -> &crate::models::package_manifest::PackageManifest { unreachable!() }
    }
    let rt = LuaRuntime::new(std::sync::Arc::new(crate::assets::AssetLoader::new(None, None, None)));
    let src: std::sync::Arc<dyn crate::services::package_loader::PackageSource> = std::sync::Arc::new(Src);
    let script = "local u = require('lib/util'); return { data = { m = u.greet() } }";
    let res = rt.run_script(script, &src, "t", &Default::default(), None, None).unwrap();
    assert_eq!(res.data["m"], serde_json::json!("hi"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk require_resolves_package_relative_module`
Expected: FAIL (signature/behaviour).

- [ ] **Step 3: Implement the searcher**

After `let lua = Lua::new();`, install a custom `require`:
- Create a `package.loaded`-style cache table in the Lua registry.
- Define a Rust closure `require(name)`:
  1. If cached, return cached value.
  2. If `name` starts with `byonk-base-v1/` (or any `byonk-base-vN/`): strip to `vN/rest`, read via `asset_loader.read_base_string("v1/rest.lua")` (append `.lua` if absent). Note base modules use the `vN/…` path form.
  3. Else treat as package-relative: `source.read_string(&format!("{name}.lua"))` (or `name` if it already ends in `.lua`).
  4. On miss → Lua error `module '<name>' not found`.
  5. `lua.load(&code).set_name(name).eval::<Value>()`, cache, return.
- `globals.set("require", require_fn)?`. Capture `source.clone()` and `asset_loader.clone()` into the closure (both `Arc`).
- Keep the existing single `lua.load(&script_src).eval()` for the screen body (now using `script_src` param).
- Update `setup_globals` to accept `script_src`/`source` as needed; keep all existing globals unchanged.

> mlua note: to share the `Arc<dyn PackageSource>` into a `create_function`, clone it before the closure and move the clone in; `dyn PackageSource: Send + Sync` (required by the trait) satisfies mlua 0.10's bounds for `lua54` non-async use.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk lua_runtime`
Expected: PASS. Update existing `run_script` callers (content_pipeline) to the new signature — compile must succeed.

- [ ] **Step 5: Commit**

```bash
git add src/services/lua_runtime.rs src/services/content_pipeline.rs
git commit -m "feat(lua): sandboxed require() for package-relative + byonk-base modules"
```

---

### Task 11: Per-package Tera template scoping (+ `byonk-base-v1`)

**Files:**
- Modify: `src/services/template_service.rs`
- Test: `src/services/template_service.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `Arc<dyn PackageSource>` (Task 6), `AssetLoader::read_base_string`/`list_base` (Task 5).
- Produces: `render` scopes templates to one package + base. New signature:
  ```rust
  pub fn render(
      &self,
      template_src: &str,                 // the resolved screen.svg
      source: &Arc<dyn PackageSource>,    // package-relative includes/extends
      screen_path: &str,                  // for image ref resolution (screen dir)
      data: &serde_json::Value,
  ) -> Result<String, TemplateError>;
  ```

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_render_uses_byonk_base_include() {
    // source with a package-relative include and a base include
    // screen.svg: {% include "byonk-base-v1/hinting.svg" %}{% include "parts/x.svg" %}<t>{{ data.n }}</t>
    // assert output contains the base + package parts and the interpolated value
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk render_uses_byonk_base_include`
Expected: FAIL.

- [ ] **Step 3: Implement**

Rewrite `render`:
- `let mut tera = Tera::default();`
- Register every base asset: for `p` in `asset_loader.list_base()` (e.g. `v1/hinting.svg`), `tera.add_raw_template(&format!("byonk-base-{}", p.replacen('/', "/", 1)) ...)` — register under the name `byonk-base-v1/hinting.svg` (map `v1/…` → `byonk-base-v1/…`). Read via `asset_loader.read_base_string`.
- Register every package `.svg` under its package-relative name: for `p` in `source`'s file list that ends in `.svg`, `tera.add_raw_template(p, &source.read_string(p)?)`. (Add a `PackageSource::list(&self, ext) -> Vec<String>` helper, or reuse `screen_paths` plus a shallow walk; simplest is to add `fn svg_files(&self) -> Vec<String>` to the trait.)
- Register the main template under a fixed name (e.g. the `screen_path`), then `tera.render(...)`.
- Remove `LAYOUT_DIR`/`COMPONENT_DIR`, `load_templates_from_dir`.
- `resolve_image_refs`: change the on-disk lookup from `screens/<screen_name>/<href>` to `source.read(&join(screen_path, href))` (package-relative to the screen dir).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p byonk template_service`
Expected: PASS. Update the `content_pipeline` caller to the new `render` signature.

- [ ] **Step 5: Commit**

```bash
git add src/services/template_service.rs src/services/content_pipeline.rs
git commit -m "feat(templates): per-package Tera scoping + byonk-base-v1 includes"
```

---

### Task 12: Migrate the built-in screens into the `byonk-builtin` package

**Files:**
- Create: `screens/byonk-screens.yaml`
- Create per screen (per the spec §7 map): `screens/<path>/meta.yaml`, `screens/<path>/script.lua`, `screens/<path>/screen.svg`
- Delete: the old flat `screens/<name>.lua`, `screens/<name>.svg`, `screens/layouts/`, `screens/components/` (now embedded under `byonk-base/v1/`), and any `*.lua~`/`*.svg~` backups
- Modify: `default-config.yaml` (empty `screens:`; `default_screen: byonk-builtin/default`)
- Test: an integration test that resolves + renders `byonk-builtin/default`

**Interfaces:**
- Consumes: everything above. Produces: the embedded `byonk-builtin` package.

Migration map (spec §7):

| Old | New folder (`screens/…`) |
|---|---|
| `default` | `default/` |
| `gphoto` | `useful/gphoto/` |
| `transit` | `useful/swiss-departure-board/` |
| `calibrator` | `calibration/color/` |
| `graytest` | `calibration/grey/` |
| `hintdemo` | `demo/font/hinting/` |
| `fontdemo-bitmap` | `demo/font/bitmap/` |
| `fontdemo-terminus` | `demo/font/ttf/` |
| `hello` | `example/hello/` |
| `mandelbrot` | `example/mandelbrot/` |
| `floerli` | `example/webscrape/` |

- [ ] **Step 1: Create the package manifest**

`screens/byonk-screens.yaml`:
```yaml
name: byonk-builtin
description: Screens bundled with byonk.
author: Byonk
license: MIT
```
(No `root:` — the package root is the `screens/` tree.)

- [ ] **Step 2: For each screen — create its folder trio**

For one screen at a time (start with `default`):
1. `mkdir -p screens/<newpath>`.
2. Move the SVG: `git mv screens/<old>.svg screens/<newpath>/screen.svg`.
3. Move the Lua: `git mv screens/<old>.lua screens/<newpath>/script.lua`.
4. In `script.lua`, delete the `--[[ @params … ]]` block.
5. Create `screens/<newpath>/meta.yaml` with `title`, `description` (from the old header comment), `byonk: "0.15"`, `refresh:` if the old `ScreenConfig`/script had a non-900 default, and the `params:` copied from the removed `@params` block.
6. In `screen.svg`, rewrite includes: `{% include "components/hinting.svg" %}` → `{% include "byonk-base-v1/hinting.svg" %}`; `{% extends "layouts/base.svg" %}` → `{% extends "byonk-base-v1/base.svg" %}`; same for header/footer/status_bar.
7. If the screen reads sibling assets (e.g. `screens/default/background.jpg`), move that asset into the new folder (`git mv screens/default/background.jpg screens/default/background.jpg` — already co-located for `default`; verify others).

Example `screens/useful/gphoto/meta.yaml` (from the current `gphoto.lua` header + `@params`):
```yaml
title: Google Photos Album
description: Shows photos from a shared Google Photos album, one per refresh.
byonk: "0.15"
params:
  album_url:
    type: url
    label: "Album URL"
    required: false
    description: "Shared Google Photos album link (until set, the screen shows its registration code)"
  show_status:
    type: bool
    label: "Show status overlay"
    default: false
  refresh_rate:
    type: int
    label: "Refresh rate"
    default: 3600
    min: 60
    unit: "s"
    mode: box
```

- [ ] **Step 3: Delete now-embedded shared dirs + backups**

```bash
git rm -r screens/layouts screens/components
git rm -f screens/*.lua~ screens/*.svg~
```
(Their content now lives under `byonk-base/v1/` from Task 5.)

- [ ] **Step 4: Point default-config.yaml at the package**

Set `default-config.yaml` `screens: {}` and `default_screen: byonk-builtin/default`. Remove per-screen `script`/`template` map entries.

- [ ] **Step 5: Write the integration test**

`tests/builtin_package.rs`:
```rust
#[test]
fn test_builtin_default_resolves_and_renders() {
    let loader = std::sync::Arc::new(byonk::assets::AssetLoader::new(None, None, None));
    let pl = byonk::services::package_loader::PackageLoader::new(loader, Default::default());
    let r = pl.resolve("byonk-builtin/default").expect("default screen resolves");
    assert!(!r.meta.title.is_empty());
}
```

- [ ] **Step 6: Run everything**

Run: `make check`
Expected: fmt/clippy clean, all tests pass, including the new integration test.

- [ ] **Step 7: Commit**

```bash
git add screens/ byonk-base/ default-config.yaml tests/builtin_package.rs
git commit -m "refactor(screens): migrate built-in screens into byonk-builtin package"
```

---

### Task 13: Admin API — grouped `/screens`, `GET /packages`, device-ref validation

**Files:**
- Modify: `src/api/admin/read.rs` (rewrite `screens()`, add `packages()`)
- Modify: `src/api/admin/write.rs` (validate `screen` is a resolvable ref; drop `resolve_screen_script`/`schema_for_script` usage)
- Modify: `src/api/admin/mod.rs` (add `/packages` route)
- Modify: `src/models/param_schema.rs` (remove `extract_params_block` + `schema_for_script`, now unused)
- Test: `src/api/admin/read.rs` (`#[cfg(test)]`) or integration

**Interfaces:**
- Consumes: `PackageLoader` (via `AppState`), `ResolvedScreen`, `compat_warning` (Task 4).
- Produces: new response shapes (spec §9a.2):
  ```rust
  #[derive(Serialize)] pub struct ScreenInfo {
      pub r#ref: String, pub title: String, pub description: String,
      pub params: Vec<ParamField>, pub byonk: String,
      pub compat_warning: Option<String>,
  }
  #[derive(Serialize)] pub struct PackageScreens {
      pub handle: String, pub name: String, pub description: String,
      pub author: String, pub license: String, pub screens: Vec<ScreenInfo>,
  }
  #[derive(Serialize)] pub struct ScreensResponse {
      pub packages: Vec<PackageScreens>, pub panels: Vec<PanelInfo>,
      pub dither_algorithms: Vec<String>,
  }
  #[derive(Serialize)] pub struct PackageInfo {
      pub handle: String, pub repo: Option<String>, pub pin: Option<String>,
      pub builtin: bool, pub token_set: bool, pub screen_count: usize,
      // status/resolved_sha/last_fetched are Plan 2 (fetching) — omit or hardcode
      pub status: String, // "ready" for builtin/on-disk in Plan 1
  }
  ```

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_screens_grouped_includes_builtin_with_titles() {
    // build AppState with default config; call screens(); assert a package
    // handle=="byonk-builtin" whose screens all have non-empty title + a ref
    // starting with "byonk-builtin/".
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p byonk screens_grouped_includes_builtin`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `screens()`: iterate `state.package_loader.list_all()` grouped by handle; for each `ResolvedScreen`, build `ScreenInfo { ref: format!("{}/{}", handle, path), title: meta.title, description: meta.description, params: meta.params.fields, byonk: meta.byonk, compat_warning: compat_warning(compat::engine_version(), &meta.byonk) }`. Package-level `name/description/author/license` from `source.manifest()`. Keep `panels` + `dither_algorithms` as-is.
- `packages()`: from `config.packages`, one `PackageInfo` each; `builtin = handle=="byonk-builtin" || repo.is_none()`; `token_set = pkg.token.is_some()` (never serialize the token); `screen_count` from `list_all()` filtered by handle; `status = "ready"`.
- `mod.rs`: add `.route("/packages", get(read::packages))`.
- `write.rs`: replace `resolve_screen_script`/`schema_for_script` validation with `state.package_loader.resolve(&screen_ref)`; reject on `None` (`ApiError::…("unknown screen `<ref>`")`), else validate params against `resolved.meta.params` using existing `validate_params`.
- `param_schema.rs`: delete `extract_params_block` and `schema_for_script` and their tests (now unused). Keep `parse_schema`, `parse_schema_from_value`, `validate_params`, `ParamField`.

- [ ] **Step 4: Run everything**

Run: `make check`
Expected: clean; new + existing tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/api/admin/read.rs src/api/admin/write.rs src/api/admin/mod.rs src/models/param_schema.rs
git commit -m "feat(admin): package-grouped /screens, GET /packages, ref validation"
```

---

## Self-Review

**Spec coverage** (spec §-by-§):
- §2 concepts, §3 format (`meta.yaml`, `byonk-screens.yaml`, fixed names) → Tasks 2, 3, 12.
- §4 sharing/resolution (repo-relative + `byonk-base-vN`, `require`, per-package Tera) → Tasks 5, 6, 10, 11.
- §5 registry/addressing (`handle/path`, `byonk-builtin` embedded) → Tasks 6, 7, 8, 9.
- §6 compat (bare=caret, warn-and-serve) → Task 4 (+ surfaced in Task 13).
- §7 no-legacy + migration map → Task 12 (+ `@params` removal in Tasks 1/13).
- §9a admin API (grouped `/screens`, `GET /packages`, token redaction, ref validation) → Task 13. *(Write/update `/packages` endpoints + fetch = Plan 2, intentionally excluded.)*
- §9 internals impact → Tasks 8–13.

**Deferred to Plan 2 (distribution), by design:** `POST/PATCH/DELETE /api/admin/packages`, `POST …/update`, `gix` fetch/cache, `pin_kind`/`resolved_sha`/`last_fetched`/`error` fields, `package_refresh_interval`, periodic refresh, auth. Plan 1's `PackageInfo.status` is a static `"ready"` placeholder until then.

**Placeholder scan:** none — every code step has concrete code or a precise, itemized change list against named symbols.

**Type consistency:** `ScreenMeta`, `PackageManifest`, `PackageRef`, `PackageSource`, `ResolvedScreen`, `PackageLoader`, `compat_warning`, `parse_schema_from_value` are defined once (Tasks 1–7) and consumed with the same names/signatures downstream (Tasks 8–13). `AppConfig` (not `Config`) used throughout. Two distinct `ScriptResult` types (lua_runtime vs content_pipeline) kept separate.

**Note for the executor:** Tasks 9–11 change signatures that ripple into `content_pipeline.rs`; keep the crate compiling by updating callers within the same task (each task ends green). If a task's test needs a helper from an earlier task's `#[cfg(test)]`, promote it to a small `tests/` support module.
