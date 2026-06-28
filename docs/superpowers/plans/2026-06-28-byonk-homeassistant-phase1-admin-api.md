# Byonk HA Phase 1 — Admin/Management API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a token-gated `/api/admin/*` HTTP API to byonk that exposes device
telemetry and supports full read-write management of device→screen mappings and global
settings, backed by per-screen `@params` schemas, comment-preserving config writes, and
config hot-reload — so the future Home Assistant integration has a clean contract to build on.

**Architecture:** New admin handlers live under `src/api/admin/`. Config becomes a
hot-swappable `Arc<ArcSwap<AppConfig>>` shared by `AppState` and `ContentPipeline`. Writes
go through a `config_writer` module that patches `config.yaml` text surgically with
`yamlpath`/`yamlpatch` (preserving comments), then reloads the in-memory config. Screen
parameter schemas are parsed (not executed) from a `@params` block in each screen's `.lua`.

**Tech Stack:** Rust, axum 0.7, serde/serde_yaml/serde_json, tokio, `arc-swap`,
`yamlpath`, `yamlpatch`, chrono.

## Global Constraints

- Rust edition 2021; workspace builds with `make check` (fmt + clippy + tests) clean.
- Follow existing patterns: handlers in `src/api/`, errors via `ApiError`, tests in
  `tests/` using `tests/common::TestApp`.
- Error JSON shape is fixed: `{ "status": <u16>, "error": <string> }` (see `src/error.rs`).
- Admin auth: `Authorization: Bearer <token>`. Token source = env `BYONK_ADMIN_TOKEN`,
  else `admin.token` in config. **No token configured ⇒ every `/api/admin/*` returns 404.**
  Wrong/missing token (when configured) ⇒ 401. Use constant-time comparison.
- Writes require a file-backed config (a real `config.yaml` path). Embedded-only ⇒ 409.
- Config writes MUST preserve comments/formatting outside the touched region.
- Config changes take effect with no restart (hot-reload).
- TDD: write the failing test first; commit after each green task.
- Every code change updates `CHANGES.md` (Unreleased) in its task where user-visible.
- Commit message footer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

## File structure

| File | Responsibility |
|---|---|
| `src/error.rs` (modify) | Add `Unauthorized`, `BadRequest(String)`, `Conflict(String)` variants + status mapping |
| `src/models/config.rs` (modify) | Add `AdminConfig { token }` + `admin` field on `AppConfig` |
| `src/models/param_schema.rs` (create) | `@params` types, textual extractor, parser, value validator |
| `src/services/device_registry.rs` (modify) | `list_all()` on trait + impl |
| `src/services/config_writer.rs` (create) | Comment-preserving YAML patch ops + reload helper |
| `src/server.rs` (modify) | `SharedConfig` type; `AppState` gains `asset_loader`, `admin_token`; arc-swap wiring; mount admin router |
| `src/assets.rs` (modify) | `config_path()` getter |
| `src/services/content_pipeline.rs` (modify) | Hold `SharedConfig`, `.load_full()` per use |
| `src/api/admin/mod.rs` (create) | Auth guard, router, shared DTOs |
| `src/api/admin/read.rs` (create) | `GET devices/pending/config/screens` |
| `src/api/admin/write.rs` (create) | `POST/PATCH/DELETE devices`, `PATCH settings` |
| `src/api/mod.rs` (modify) | `pub mod admin;` |
| `screens/*.lua` (modify) | Add `@params` headers |
| `tests/admin_*_test.rs` (create) | Integration tests per endpoint group |
| `tests/common/app.rs` (modify) | Admin test constructors + `patch_json`/`delete` helpers |
| `docs/src/...` + `CHANGES.md` (modify) | Docs + changelog |

---

## Task 1: Foundation — dependencies, error variants, admin config field

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/error.rs`
- Modify: `src/models/config.rs`
- Test: inline `#[cfg(test)]` in `src/error.rs` and `src/models/config.rs`

**Interfaces:**
- Produces: `ApiError::Unauthorized`, `ApiError::BadRequest(String)`, `ApiError::Conflict(String)`.
- Produces: `AppConfig.admin: AdminConfig` where `pub struct AdminConfig { pub token: Option<String> }`.
- Produces: new deps `arc-swap`, `yamlpath`, `yamlpatch` available to the crate.

- [ ] **Step 1: Add dependencies**

Add to `Cargo.toml` under `[dependencies]` (after the existing utility deps):

```toml
# Config hot-reload + comment-preserving YAML edits
arc-swap = "1"
yamlpath = "1"
yamlpatch = "1"
```

Run `cargo fetch` to pull them.

- [ ] **Step 2: Write failing tests for new error variants**

Add to the `#[cfg(test)] mod tests` block in `src/error.rs`:

```rust
#[test]
fn test_api_error_unauthorized_status() {
    let resp = ApiError::Unauthorized.into_response();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_api_error_bad_request_status() {
    let resp = ApiError::BadRequest("nope".into()).into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_api_error_conflict_status() {
    let resp = ApiError::Conflict("dup".into()).into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib error::tests`
Expected: FAIL — `no variant named Unauthorized` etc.

- [ ] **Step 4: Implement the variants**

In `src/error.rs`, add to the `ApiError` enum:

```rust
    #[error("Unauthorized")]
    Unauthorized,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),
```

And add to the `match &self` in `IntoResponse for ApiError`:

```rust
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib error::tests`
Expected: PASS.

- [ ] **Step 6: Write failing test for admin config field**

Add to the `#[cfg(test)] mod tests` block in `src/models/config.rs`:

```rust
#[test]
fn test_admin_token_parses_and_defaults() {
    let yaml = "admin:\n  token: secret123\nscreens: {}\n";
    let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.admin.token.as_deref(), Some("secret123"));

    let cfg2: AppConfig = serde_yaml::from_str("screens: {}\n").unwrap();
    assert_eq!(cfg2.admin.token, None);
}
```

- [ ] **Step 7: Run test to verify it fails**

Run: `cargo test --lib config::tests::test_admin_token_parses_and_defaults`
Expected: FAIL — `no field admin`.

- [ ] **Step 8: Implement AdminConfig**

In `src/models/config.rs`, add the struct near `RegistrationConfig`:

```rust
/// Admin/management API settings.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AdminConfig {
    /// Bearer token gating `/api/admin/*`. If unset (and `BYONK_ADMIN_TOKEN` is
    /// unset), the admin API is disabled (returns 404).
    #[serde(default)]
    pub token: Option<String>,
}
```

Add the field to `AppConfig`:

```rust
    /// Admin/management API settings
    #[serde(default)]
    pub admin: AdminConfig,
```

Add `admin: AdminConfig::default(),` to the `AppConfig` literal in
`impl Default for AppConfig` (the test `config.rs` block around the existing default).
Export it in `src/models/mod.rs`:

```rust
pub use config::{
    normalize_algorithm_name, AdminConfig, AppConfig, DeviceConfig, DitherTuningValues,
    PanelDitherConfig, RegistrationConfig, ScreenConfig,
};
```

- [ ] **Step 9: Run test + clippy**

Run: `cargo test --lib config::tests::test_admin_token_parses_and_defaults && cargo clippy --all-targets`
Expected: PASS, no clippy errors.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml Cargo.lock src/error.rs src/models/config.rs src/models/mod.rs
git commit -m "feat(admin): add deps, error variants, and admin config field

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Registry — list all devices

**Files:**
- Modify: `src/services/device_registry.rs`
- Test: inline `#[cfg(test)]` in same file

**Interfaces:**
- Produces: `DeviceRegistry::list_all(&self) -> Result<Vec<Device>, ApiError>` (trait method)
  implemented by `InMemoryRegistry`.

- [ ] **Step 1: Write failing test**

Add to the `#[cfg(test)] mod tests` in `src/services/device_registry.rs`:

```rust
#[tokio::test]
async fn test_list_all_returns_all_devices() {
    let registry = InMemoryRegistry::new();
    let d1 = Device::new(DeviceId::new("AA:AA:AA:AA:AA:AA"), DeviceModel::OG, "1".into());
    let d2 = Device::new(DeviceId::new("BB:BB:BB:BB:BB:BB"), DeviceModel::X, "2".into());
    registry.upsert(d1).await.unwrap();
    registry.upsert(d2).await.unwrap();

    let all = registry.list_all().await.unwrap();
    assert_eq!(all.len(), 2);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib device_registry::tests::test_list_all_returns_all_devices`
Expected: FAIL — `no method named list_all`.

- [ ] **Step 3: Implement**

In the `pub trait DeviceRegistry` add:

```rust
    /// List all known devices.
    async fn list_all(&self) -> Result<Vec<Device>, ApiError>;
```

In `impl DeviceRegistry for InMemoryRegistry` add:

```rust
    async fn list_all(&self) -> Result<Vec<Device>, ApiError> {
        let devices = self.devices.read().await;
        Ok(devices.values().cloned().collect())
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --lib device_registry::tests::test_list_all_returns_all_devices`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/device_registry.rs
git commit -m "feat(admin): add DeviceRegistry::list_all

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Param schema — types, extractor, parser

**Files:**
- Create: `src/models/param_schema.rs`
- Modify: `src/models/mod.rs`
- Test: inline `#[cfg(test)]` in `src/models/param_schema.rs`

**Interfaces:**
- Produces:
  - `pub enum ParamType { String, Int, Float, Bool, Enum, Color, Url }` (serde snake_case, `Default = String`)
  - `pub struct EnumOption { pub value: String, pub label: String }`
  - `pub struct ParamField { pub name: String, pub param_type: ParamType, pub required: bool, pub default: Option<serde_json::Value>, pub label: Option<String>, pub description: Option<String>, pub min: Option<f64>, pub max: Option<f64>, pub step: Option<f64>, pub unit: Option<String>, pub mode: Option<String>, pub options: Vec<EnumOption>, pub sensitive: bool, pub multiline: bool, pub hidden: bool, pub advanced: bool }`
  - `pub struct ParamSchema { pub fields: Vec<ParamField> }`
  - `pub fn extract_params_block(lua_source: &str) -> Option<String>`
  - `pub fn parse_schema(yaml: &str) -> Result<ParamSchema, String>`
  - `pub fn schema_for_script(lua_source: &str) -> Result<Option<ParamSchema>, String>` (None when no `@params` block; `Err` when present but malformed)

- [ ] **Step 1: Write failing tests**

Create `src/models/param_schema.rs` with tests at the bottom:

```rust
//! Per-screen `@params` schema: types, textual extraction from `.lua`, and parsing.
//!
//! The schema is declared inside a Lua block comment at the top of a screen script:
//! ```lua
//! --[[ @params
//! station:
//!   type: string
//!   required: true
//! ]]
//! ```
//! It is parsed as YAML — never executed.

use serde::{Deserialize, Serialize};

// (implementation added in later steps)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_block_present() {
        let lua = "--[[ @params\nstation:\n  type: string\n]]\nlocal x = 1\n";
        let block = extract_params_block(lua).unwrap();
        assert!(block.contains("station:"));
        assert!(block.contains("type: string"));
        assert!(!block.contains("local x"));
    }

    #[test]
    fn test_extract_block_absent() {
        assert!(extract_params_block("local x = 1\n").is_none());
    }

    #[test]
    fn test_parse_minimal_field() {
        let schema = parse_schema("station:\n  type: string\n  required: true\n").unwrap();
        assert_eq!(schema.fields.len(), 1);
        let f = &schema.fields[0];
        assert_eq!(f.name, "station");
        assert_eq!(f.param_type, ParamType::String);
        assert!(f.required);
    }

    #[test]
    fn test_parse_preserves_order() {
        let schema = parse_schema("b:\n  type: int\na:\n  type: string\n").unwrap();
        assert_eq!(schema.fields[0].name, "b");
        assert_eq!(schema.fields[1].name, "a");
    }

    #[test]
    fn test_parse_enum_options_objects_and_bare() {
        let obj = parse_schema(
            "k:\n  type: enum\n  options:\n    - {value: a, label: Apple}\n    - {value: b, label: Banana}\n",
        )
        .unwrap();
        assert_eq!(obj.fields[0].options.len(), 2);
        assert_eq!(obj.fields[0].options[0].label, "Apple");

        let bare = parse_schema("k:\n  type: enum\n  options: [a, b]\n").unwrap();
        assert_eq!(bare.fields[0].options[0].value, "a");
        assert_eq!(bare.fields[0].options[0].label, "a"); // label defaults to value
    }

    #[test]
    fn test_parse_enum_without_options_is_error() {
        assert!(parse_schema("k:\n  type: enum\n").is_err());
    }

    #[test]
    fn test_parse_unknown_type_is_error() {
        assert!(parse_schema("k:\n  type: banana\n").is_err());
    }

    #[test]
    fn test_schema_for_script_none_when_no_block() {
        assert!(schema_for_script("local x = 1\n").unwrap().is_none());
    }

    #[test]
    fn test_schema_for_script_err_when_malformed() {
        let lua = "--[[ @params\nk:\n  type: banana\n]]\n";
        assert!(schema_for_script(lua).is_err());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib param_schema`
Expected: FAIL — module not declared / functions missing.

- [ ] **Step 3: Declare the module**

In `src/models/mod.rs` add `pub mod param_schema;` and extend the re-export:

```rust
pub use param_schema::{
    extract_params_block, parse_schema, schema_for_script, EnumOption, ParamField, ParamSchema,
    ParamType,
};
```

- [ ] **Step 4: Implement types**

Add above the `#[cfg(test)]` block in `src/models/param_schema.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    #[default]
    String,
    Int,
    Float,
    Bool,
    Enum,
    Color,
    Url,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ParamField {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: ParamType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<EnumOption>,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub advanced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct ParamSchema {
    pub fields: Vec<ParamField>,
}
```

- [ ] **Step 5: Implement extractor**

```rust
/// Extract the YAML text inside a `--[[ @params ... ]]` block. Returns `None`
/// if no `@params` marker is present.
pub fn extract_params_block(lua_source: &str) -> Option<String> {
    let marker = lua_source.find("@params")?;
    // Start after the rest of the marker line.
    let after_marker = &lua_source[marker + "@params".len()..];
    let body_start = after_marker.find('\n').map(|i| i + 1).unwrap_or(0);
    let body = &after_marker[body_start..];
    let end = body.find("]]")?;
    Some(body[..end].to_string())
}
```

- [ ] **Step 6: Implement parser**

```rust
/// Raw descriptor as written in YAML (without the `name`, which is the map key).
#[derive(Deserialize)]
struct RawField {
    #[serde(rename = "type")]
    param_type: ParamType,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<serde_json::Value>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    options: Option<serde_yaml::Value>,
    #[serde(default)]
    sensitive: bool,
    #[serde(default)]
    multiline: bool,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    advanced: bool,
}

fn parse_options(raw: serde_yaml::Value) -> Result<Vec<EnumOption>, String> {
    let seq = raw
        .as_sequence()
        .ok_or_else(|| "enum `options` must be a list".to_string())?;
    let mut out = Vec::new();
    for item in seq {
        if let Some(s) = item.as_str() {
            out.push(EnumOption { value: s.to_string(), label: s.to_string() });
        } else if let Some(map) = item.as_mapping() {
            let value = map
                .get(serde_yaml::Value::from("value"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "enum option object needs a string `value`".to_string())?
                .to_string();
            let label = map
                .get(serde_yaml::Value::from("label"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.clone());
            out.push(EnumOption { value, label });
        } else {
            return Err("enum option must be a scalar or {value,label} map".to_string());
        }
    }
    Ok(out)
}

/// Parse a `@params` YAML body into a schema. Preserves field order.
pub fn parse_schema(yaml: &str) -> Result<ParamSchema, String> {
    // Empty body ⇒ empty schema (screen takes no params).
    if yaml.trim().is_empty() {
        return Ok(ParamSchema::default());
    }
    let mapping: serde_yaml::Mapping =
        serde_yaml::from_str(yaml).map_err(|e| format!("invalid @params YAML: {e}"))?;

    let mut fields = Vec::new();
    for (k, v) in mapping {
        let name = k
            .as_str()
            .ok_or_else(|| "param keys must be strings".to_string())?
            .to_string();
        let raw: RawField = serde_yaml::from_value(v)
            .map_err(|e| format!("param `{name}`: {e}"))?;

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

/// Extract + parse a screen's schema. `Ok(None)` when there is no `@params`
/// block; `Err` when a block is present but malformed.
pub fn schema_for_script(lua_source: &str) -> Result<Option<ParamSchema>, String> {
    match extract_params_block(lua_source) {
        None => Ok(None),
        Some(body) => parse_schema(&body).map(Some),
    }
}
```

> Note: `serde_yaml::from_value` deserializing `ParamType` requires the unknown-type case
> to error — it does, because `ParamType` has no catch-all variant. `serde_yaml::Mapping`
> preserves insertion order, giving stable field order.

- [ ] **Step 7: Run tests**

Run: `cargo test --lib param_schema && cargo clippy --all-targets`
Expected: PASS, clean.

- [ ] **Step 8: Commit**

```bash
git add src/models/param_schema.rs src/models/mod.rs
git commit -m "feat(admin): add @params screen schema types, extractor, and parser

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Param value validation

**Files:**
- Modify: `src/models/param_schema.rs`
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: `ParamSchema`, `ParamType` (Task 3).
- Produces: `pub fn validate_params(schema: &ParamSchema, params: &std::collections::HashMap<String, serde_yaml::Value>) -> Result<(), Vec<String>>`

- [ ] **Step 1: Write failing tests**

Add to the test module in `src/models/param_schema.rs`:

```rust
use std::collections::HashMap;

fn params(pairs: &[(&str, serde_yaml::Value)]) -> HashMap<String, serde_yaml::Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

#[test]
fn test_validate_missing_required() {
    let schema = parse_schema("station:\n  type: string\n  required: true\n").unwrap();
    let errs = validate_params(&schema, &params(&[])).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("station")));
}

#[test]
fn test_validate_type_mismatch() {
    let schema = parse_schema("limit:\n  type: int\n").unwrap();
    let errs = validate_params(&schema, &params(&[("limit", "abc".into())])).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("limit")));
}

#[test]
fn test_validate_min_max() {
    let schema = parse_schema("limit:\n  type: int\n  min: 1\n  max: 30\n").unwrap();
    assert!(validate_params(&schema, &params(&[("limit", 50i64.into())])).is_err());
    assert!(validate_params(&schema, &params(&[("limit", 8i64.into())])).is_ok());
}

#[test]
fn test_validate_enum_membership() {
    let schema = parse_schema("k:\n  type: enum\n  options: [a, b]\n").unwrap();
    assert!(validate_params(&schema, &params(&[("k", "c".into())])).is_err());
    assert!(validate_params(&schema, &params(&[("k", "a".into())])).is_ok());
}

#[test]
fn test_validate_ok_when_optional_absent() {
    let schema = parse_schema("station:\n  type: string\n").unwrap();
    assert!(validate_params(&schema, &params(&[])).is_ok());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib param_schema::tests::test_validate`
Expected: FAIL — `validate_params` not found.

- [ ] **Step 3: Implement**

Add to `src/models/param_schema.rs` (above tests):

```rust
use std::collections::HashMap;

/// Validate a params map against a schema. Returns all problems found (not just
/// the first). Params not described by the schema are allowed (ignored).
pub fn validate_params(
    schema: &ParamSchema,
    params: &HashMap<String, serde_yaml::Value>,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    for field in &schema.fields {
        match params.get(&field.name) {
            None => {
                if field.required {
                    errors.push(format!("missing required param `{}`", field.name));
                }
            }
            Some(value) => {
                if let Err(e) = check_value(field, value) {
                    errors.push(e);
                }
            }
        }
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

fn check_value(field: &ParamField, value: &serde_yaml::Value) -> Result<(), String> {
    let name = &field.name;
    match field.param_type {
        ParamType::String | ParamType::Color | ParamType::Url => {
            if !value.is_string() {
                return Err(format!("param `{name}` must be a string"));
            }
        }
        ParamType::Bool => {
            if !value.is_bool() {
                return Err(format!("param `{name}` must be a boolean"));
            }
        }
        ParamType::Int => {
            let n = value
                .as_i64()
                .ok_or_else(|| format!("param `{name}` must be an integer"))?;
            check_range(field, n as f64)?;
        }
        ParamType::Float => {
            let n = value
                .as_f64()
                .ok_or_else(|| format!("param `{name}` must be a number"))?;
            check_range(field, n)?;
        }
        ParamType::Enum => {
            let s = value
                .as_str()
                .ok_or_else(|| format!("param `{name}` must be one of the enum values"))?;
            if !field.options.iter().any(|o| o.value == s) {
                return Err(format!("param `{name}` value `{s}` is not an allowed option"));
            }
        }
    }
    Ok(())
}

fn check_range(field: &ParamField, n: f64) -> Result<(), String> {
    if let Some(min) = field.min {
        if n < min {
            return Err(format!("param `{}` must be >= {min}", field.name));
        }
    }
    if let Some(max) = field.max {
        if n > max {
            return Err(format!("param `{}` must be <= {max}", field.name));
        }
    }
    Ok(())
}
```

Export it in `src/models/mod.rs` (extend the `param_schema` re-export with `validate_params`).

- [ ] **Step 4: Run tests**

Run: `cargo test --lib param_schema && cargo clippy --all-targets`
Expected: PASS, clean.

- [ ] **Step 5: Commit**

```bash
git add src/models/param_schema.rs src/models/mod.rs
git commit -m "feat(admin): validate device params against screen schema

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Hot-reload — SharedConfig (arc-swap) + AppState wiring

This is the riskiest refactor: it touches every `state.config` consumer. After it, the
full suite must still pass.

**Files:**
- Modify: `src/server.rs`
- Modify: `src/services/content_pipeline.rs`
- Modify: `src/assets.rs`
- Test: inline + run full suite

**Interfaces:**
- Produces: `pub type SharedConfig = std::sync::Arc<arc_swap::ArcSwap<AppConfig>>;` (in `server.rs`, re-exported).
- Produces: `AppState { pub config: SharedConfig, pub asset_loader: Arc<AssetLoader>, pub admin_token: Option<String>, .. }` (existing fields retained).
- Produces: `AssetLoader::config_path(&self) -> Option<&std::path::Path>`.
- Produces: `ContentPipeline::new(config: SharedConfig, asset_loader, renderer)` (signature change).
- Produces: `pub fn reload_config(state: &AppState) -> anyhow::Result<()>` (in `server.rs`).

- [ ] **Step 1: Add `config_path` getter to AssetLoader (test first)**

Add a test to `src/assets.rs` `#[cfg(test)]`:

```rust
#[test]
fn test_config_path_getter() {
    let loader = AssetLoader::new(None, None, Some(PathBuf::from("/tmp/x.yaml")));
    assert_eq!(loader.config_path(), Some(std::path::Path::new("/tmp/x.yaml")));
    let embedded = AssetLoader::new(None, None, None);
    assert_eq!(embedded.config_path(), None);
}
```

Run `cargo test --lib assets::tests::test_config_path_getter` → FAIL.

Implement on `impl AssetLoader`:

```rust
    /// Path to the external config file, if one is configured.
    pub fn config_path(&self) -> Option<&std::path::Path> {
        self.config_file.as_deref()
    }
```

Run the test → PASS.

- [ ] **Step 2: Define `SharedConfig` and update `AppState`**

In `src/server.rs`, add near the top:

```rust
use arc_swap::ArcSwap;

/// Hot-swappable application config shared by the server and the content pipeline.
pub type SharedConfig = std::sync::Arc<ArcSwap<AppConfig>>;
```

Change `AppState`:

```rust
#[derive(Clone)]
pub struct AppState {
    pub config: SharedConfig,
    pub asset_loader: Arc<AssetLoader>,
    pub admin_token: Option<String>,
    /// Serializes admin config writes so concurrent requests can't interleave file patches.
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    pub registry: Arc<InMemoryRegistry>,
    pub renderer: Arc<RenderService>,
    pub content_pipeline: Arc<ContentPipeline>,
    pub content_cache: Arc<ContentCache>,
    pub dev_overrides: DevOverrides,
}
```

- [ ] **Step 3: Update the state constructors**

Rewrite `create_app_state` and `create_app_state_with_overrides` in `src/server.rs`:

```rust
pub fn create_app_state(asset_loader: Arc<AssetLoader>) -> anyhow::Result<AppState> {
    let config = AppConfig::load_from_assets(&asset_loader)?;
    create_app_state_with_config(asset_loader, Arc::new(config))
}

pub fn create_app_state_with_config(
    asset_loader: Arc<AssetLoader>,
    config: Arc<AppConfig>,
) -> anyhow::Result<AppState> {
    create_app_state_with_overrides(asset_loader, config, DevOverrides::default())
}

pub fn create_app_state_with_overrides(
    asset_loader: Arc<AssetLoader>,
    config: Arc<AppConfig>,
    dev_overrides: DevOverrides,
) -> anyhow::Result<AppState> {
    let admin_token = std::env::var("BYONK_ADMIN_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| config.admin.token.clone());

    let shared_config: SharedConfig = Arc::new(ArcSwap::from(config));

    let registry = Arc::new(InMemoryRegistry::new());
    let renderer = Arc::new(RenderService::new(&asset_loader)?);
    let content_pipeline = Arc::new(
        ContentPipeline::new(shared_config.clone(), asset_loader.clone(), renderer.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create content pipeline: {e}"))?,
    );
    let content_cache = Arc::new(ContentCache::new());

    Ok(AppState {
        config: shared_config,
        asset_loader,
        admin_token,
        write_lock: Arc::new(tokio::sync::Mutex::new(())),
        registry,
        renderer,
        content_pipeline,
        content_cache,
        dev_overrides,
    })
}
```

> The existing `create_app_state_with_config` signature (taking `Arc<AppConfig>`) is kept so
> `TestApp::new_without_registration` and other callers compile unchanged.

- [ ] **Step 4: Update the wrapper handlers to load the current config**

In `src/server.rs`, the wrapper handlers currently pass `state.config` into the inner TRMNL
handlers that expect `State<Arc<AppConfig>>`. Replace each `axum::extract::State(state.config)`
with `axum::extract::State(state.config.load_full())`. `load_full()` returns `Arc<AppConfig>`,
so the inner handlers (`handle_setup`, `handle_display`) stay unchanged.

- [ ] **Step 5: Update `ContentPipeline` to hold `SharedConfig`**

In `src/services/content_pipeline.rs`:

- Change the struct field `config: Arc<AppConfig>` → `config: crate::server::SharedConfig`.
- Change `ContentPipeline::new` first parameter to `config: crate::server::SharedConfig`.
- At the start of every method that reads `self.config` (the methods around lines 138, 165,
  182), take a snapshot: `let config = self.config.load();` and replace `self.config.` with
  `config.`. `load()` returns a `Guard` that derefs to `AppConfig`.

For example, the body that did:

```rust
self.config.screens.get(screen_name).cloned().or_else(|| { ... })
```

becomes:

```rust
let config = self.config.load();
config.screens.get(screen_name).cloned().or_else(|| { ... })
```

- [ ] **Step 6: Update `src/api/dev.rs` consumers**

`DevState.config` is `Arc<AppConfig>` and is built elsewhere (dev server). Where `DevState`
is constructed (in `src/main.rs run_dev_server`), it already has an `Arc<AppConfig>`; keep
`DevState.config` as `Arc<AppConfig>` (a point-in-time snapshot is fine for dev mode). No
change needed in `dev.rs` itself. Confirm `cargo build` still resolves dev mode.

- [ ] **Step 7: Add the `reload_config` helper**

In `src/server.rs`:

```rust
/// Reparse the config file via the asset loader and atomically swap it in.
pub fn reload_config(state: &AppState) -> anyhow::Result<()> {
    let fresh = AppConfig::load_from_assets(&state.asset_loader)?;
    state.config.store(Arc::new(fresh));
    Ok(())
}
```

- [ ] **Step 8: Fix any remaining call sites + run the FULL suite**

Known direct call site: `src/main.rs` `run_render_command` (~line 201) builds
`ContentPipeline::new(config.clone(), ...)` with an `Arc<AppConfig>`. Wrap it for the new
signature:

```rust
    let shared: byonk::server::SharedConfig = std::sync::Arc::new(arc_swap::ArcSwap::from(config.clone()));
    let content_pipeline = ContentPipeline::new(shared, asset_loader, renderer.clone())
```

(`config` is still used later in that function as `Arc<AppConfig>`, so keep it.) Then:

Run: `cargo build --all-targets 2>&1 | head -40`
Fix every remaining compile error (mechanical: snapshot `load()`/`load_full()` where an
`&AppConfig` or `Arc<AppConfig>` is needed). Then:

Run: `cargo test`
Expected: the entire existing suite PASSES (no behavior change yet).

- [ ] **Step 9: Add a hot-reload unit test**

Add to `src/server.rs` `#[cfg(test)] mod tests` (create the module if absent):

```rust
#[cfg(test)]
mod reload_tests {
    use super::*;

    #[test]
    fn test_config_swap_is_visible() {
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let state = create_app_state(loader).unwrap();
        assert!(state.config.load().screens.contains_key("default"));

        // Swap in a config with a sentinel screen and confirm the snapshot updates.
        let mut cfg = (**state.config.load()).clone();
        cfg.default_screen = Some("sentinel".to_string());
        state.config.store(Arc::new(cfg));
        assert_eq!(state.config.load().default_screen.as_deref(), Some("sentinel"));
    }
}
```

Run: `cargo test --lib server::reload_tests` → PASS.

- [ ] **Step 10: Commit**

```bash
git add src/server.rs src/services/content_pipeline.rs src/assets.rs
git commit -m "refactor(admin): make config hot-swappable via arc-swap

AppState.config becomes Arc<ArcSwap<AppConfig>>, shared with the content
pipeline; adds asset_loader + admin_token to AppState and a reload_config
helper. No behavior change to existing endpoints.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Admin auth guard + router + `GET /api/admin/devices`

**Files:**
- Create: `src/api/admin/mod.rs`
- Create: `src/api/admin/read.rs`
- Modify: `src/api/mod.rs`
- Modify: `src/server.rs` (mount router)
- Modify: `tests/common/app.rs` (admin test constructor + helpers)
- Test: create `tests/admin_devices_test.rs`

**Interfaces:**
- Consumes: `AppState`, `SharedConfig`, `DeviceRegistry::list_all` (Task 2, 5).
- Produces: `pub fn admin_router() -> axum::Router<AppState>` (in `src/api/admin/mod.rs`).
- Produces: auth guard `require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError>`.
- Produces: `AdminDevice` DTO (serialized JSON for device list).

- [ ] **Step 1: Add the admin test harness helpers**

In `tests/common/app.rs`, add constructors and request helpers:

```rust
impl TestApp {
    /// Admin app whose config is EMBEDDED only (writes will return 409),
    /// with the given admin token enabled.
    pub fn new_admin(token: &str) -> Self {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let mut config = AppConfig::load_from_assets(&asset_loader).expect("load config");
        config.admin.token = Some(token.to_string());
        let state = create_app_state_with_config(asset_loader, Arc::new(config))
            .expect("create state");
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);
        Self { router, registry, content_cache }
    }

    /// Admin app backed by a real config FILE seeded from the embedded default
    /// (writes succeed). Returns (app, config_path). `dir` must outlive the app.
    pub fn new_admin_with_file(token: &str, dir: &std::path::Path) -> (Self, std::path::PathBuf) {
        let config_path = dir.join("config.yaml");
        let embedded = AssetLoader::new(None, None, None);
        let yaml = embedded.read_config_string().expect("read embedded config");
        std::fs::write(&config_path, format!("admin:\n  token: {token}\n{yaml}"))
            .expect("write config file");

        let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path.clone())));
        let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
        let state = create_app_state_with_config(asset_loader, Arc::new(config))
            .expect("create state");
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);
        (Self { router, registry, content_cache }, config_path)
    }

    pub async fn patch_json(&self, path: &str, headers: &[(&str, &str)], body: &str) -> TestResponse {
        let mut builder = Request::patch(path).header("Content-Type", "application/json");
        for (n, v) in headers { builder = builder.header(*n, *v); }
        self.request(builder.body(Body::from(body.to_string())).unwrap()).await
    }

    pub async fn delete(&self, path: &str, headers: &[(&str, &str)]) -> TestResponse {
        let mut builder = Request::delete(path);
        for (n, v) in headers { builder = builder.header(*n, *v); }
        self.request(builder.body(Body::empty()).unwrap()).await
    }
}
```

(Add `use byonk::server::create_app_state_with_config;` etc. as needed — already imported.)

- [ ] **Step 2: Write failing integration tests**

Create `tests/admin_devices_test.rs`:

```rust
//! Tests for GET /api/admin/devices and admin auth.

mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn test_admin_disabled_returns_404() {
    // Default TestApp has no admin token configured.
    let app = TestApp::new();
    let resp = app.get_with_headers("/api/admin/devices", &[("Authorization", "Bearer x")]).await;
    assert_eq!(resp.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_wrong_token_returns_401() {
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/devices", &[("Authorization", "Bearer nope")]).await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_missing_token_returns_401() {
    let app = TestApp::new_admin("secret");
    let resp = app.get("/api/admin/devices").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_devices_lists_seen_device() {
    let app = TestApp::new_admin("secret");
    // Make a device appear in the registry via the normal setup flow.
    app.register_device("AA:BB:CC:DD:EE:FF").await;

    let resp = app
        .get_with_headers("/api/admin/devices", &[("Authorization", "Bearer secret")])
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let json: serde_json::Value = resp.json();
    let arr = json.as_array().expect("array");
    assert!(arr.iter().any(|d| d["mac"] == "AA:BB:CC:DD:EE:FF"));
}
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test --test admin_devices_test`
Expected: FAIL — routes don't exist (currently 404 for all, so the 401 tests fail).

- [ ] **Step 4: Implement the auth guard + DTOs in `src/api/admin/mod.rs`**

```rust
//! Admin/management API (`/api/admin/*`), gated by a bearer token.

pub mod read;

use axum::{http::HeaderMap, routing::get, Router};

use crate::error::ApiError;
use crate::server::AppState;

/// Enforce admin auth. Returns 404 when admin is disabled (no token configured),
/// 401 when the token is missing or wrong.
pub fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.admin_token.as_deref() else {
        return Err(ApiError::NotFound); // admin disabled ⇒ invisible
    };
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match provided {
        Some(tok) if constant_time_eq(tok.as_bytes(), expected.as_bytes()) => Ok(()),
        _ => Err(ApiError::Unauthorized),
    }
}

/// Constant-time byte comparison (avoids token-length/timing leaks).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// All admin routes, to be nested under `/api/admin`.
pub fn admin_router() -> Router<AppState> {
    Router::new().route("/devices", get(read::list_devices))
    // read.rs/write.rs add more routes in later tasks
}
```

> `require_admin` is the single auth entry point; every handler calls it first.
> The unused `State`/`Json`/`State` imports in the `use` block above can be trimmed to just
> what `admin_router` needs (`axum::{routing::get, Router}`); add `post`/`patch`/`delete`
> in Tasks 10–11. `ApiError::NotFound` already maps to 404, so the disabled case is correct.

- [ ] **Step 5: Implement `GET /devices` in `src/api/admin/read.rs`**

```rust
//! Admin read endpoints.

use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;

use crate::error::ApiError;
use crate::server::AppState;
use crate::services::DeviceRegistry;

use super::require_admin;

#[derive(Serialize)]
pub struct AdminDevice {
    /// Config key (MAC or registration code) if configured, else the MAC.
    pub key: String,
    pub mac: String,
    pub registration_code: String,
    pub registered: bool,
    // telemetry (None when never seen)
    pub model: Option<String>,
    pub firmware_version: Option<String>,
    pub last_seen: Option<String>,
    pub battery_voltage: Option<f32>,
    pub rssi: Option<i32>,
    // resolved active config (None when no mapping)
    pub screen: Option<String>,
    pub dither: Option<String>,
    pub panel: Option<String>,
    pub colors: Option<String>,
    pub params: serde_json::Value,
}

pub async fn list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AdminDevice>>, ApiError> {
    require_admin(&state, &headers)?;

    let config = state.config.load();
    let seen = state.registry.list_all().await?;

    let mut out: Vec<AdminDevice> = Vec::new();

    // 1) Every device the registry has seen, merged with its config mapping.
    for d in &seen {
        let mac = d.device_id.to_string();
        let code = d.api_key.registration_code();
        let dc = config
            .get_device_config(&mac)
            .or_else(|| config.get_device_config_for_code(&code));
        let registered = config.is_device_registered(&mac, Some(&code));
        out.push(AdminDevice {
            key: mac.clone(),
            mac,
            registration_code: code,
            registered,
            model: Some(d.model.to_string()),
            firmware_version: Some(d.firmware_version.clone()),
            last_seen: Some(d.last_seen.to_rfc3339()),
            battery_voltage: d.battery_voltage,
            rssi: d.rssi,
            screen: dc.map(|c| c.screen.clone()),
            dither: dc.and_then(|c| c.dither.clone()),
            panel: dc.and_then(|c| c.panel.clone()),
            colors: dc.and_then(|c| c.colors.clone()),
            params: dc
                .map(|c| serde_json::to_value(&c.params).unwrap_or(serde_json::Value::Null))
                .unwrap_or(serde_json::Value::Null),
        });
    }

    // 2) Configured devices that have NOT been seen yet (telemetry = None).
    let seen_macs: std::collections::HashSet<String> =
        seen.iter().map(|d| d.device_id.to_string()).collect();
    for (key, dc) in &config.devices {
        // Skip if this config entry corresponds to a seen device (by MAC key).
        if seen_macs.contains(key) {
            continue;
        }
        out.push(AdminDevice {
            key: key.clone(),
            mac: key.clone(),
            registration_code: String::new(),
            registered: true,
            model: None,
            firmware_version: None,
            last_seen: None,
            battery_voltage: None,
            rssi: None,
            screen: Some(dc.screen.clone()),
            dither: dc.dither.clone(),
            panel: dc.panel.clone(),
            colors: dc.colors.clone(),
            params: serde_json::to_value(&dc.params).unwrap_or(serde_json::Value::Null),
        });
    }

    Ok(Json(out))
}
```

- [ ] **Step 6: Wire up the module and mount the router**

In `src/api/mod.rs` add: `pub mod admin;`

In `src/server.rs` `build_router`, before `.with_state(state)`, nest the admin router:

```rust
        .nest("/api/admin", crate::api::admin::admin_router())
```

- [ ] **Step 7: Run the tests**

Run: `cargo test --test admin_devices_test`
Expected: PASS (all four).

- [ ] **Step 8: Run full suite + clippy**

Run: `make check`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add src/api/admin/ src/api/mod.rs src/server.rs tests/common/app.rs tests/admin_devices_test.rs
git commit -m "feat(admin): bearer-gated GET /api/admin/devices

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: `GET /api/admin/pending` and `GET /api/admin/config`

**Files:**
- Modify: `src/api/admin/read.rs`, `src/api/admin/mod.rs`
- Test: create `tests/admin_read_test.rs`

**Interfaces:**
- Consumes: `AppState`, `require_admin`, `AssetLoader::read_config_string`.
- Produces: handlers `pending(...)`, `get_config(...)`; routes `/pending`, `/config`.

- [ ] **Step 1: Write failing tests**

Create `tests/admin_read_test.rs`:

```rust
mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn test_pending_lists_unregistered_seen_device() {
    let app = TestApp::new_admin("secret");
    // A freshly-set-up device with no config mapping is "pending".
    app.register_device("11:22:33:44:55:66").await;

    let resp = app.get_with_headers("/api/admin/pending", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let arr: serde_json::Value = resp.json();
    let list = arr.as_array().unwrap();
    assert!(list.iter().any(|d| d["mac"] == "11:22:33:44:55:66"));
    assert!(list[0]["registration_code"].as_str().unwrap().len() == 10);
}

#[tokio::test]
async fn test_config_returns_json_without_token() {
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/config", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let json: serde_json::Value = resp.json();
    assert!(json["screens"].is_object());
    // admin.token must be stripped.
    assert!(json["admin"]["token"].is_null());
}

#[tokio::test]
async fn test_config_requires_auth() {
    let app = TestApp::new_admin("secret");
    let resp = app.get("/api/admin/config").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test admin_read_test`
Expected: FAIL — routes not found (401-disabled path returns 404 since routes absent).

- [ ] **Step 3: Implement handlers**

Add to `src/api/admin/read.rs`:

```rust
#[derive(Serialize)]
pub struct PendingDevice {
    pub mac: String,
    pub registration_code: String,
    pub model: String,
    pub firmware_version: String,
    pub last_seen: String,
}

pub async fn pending(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PendingDevice>>, ApiError> {
    require_admin(&state, &headers)?;

    let config = state.config.load();
    let mut out = Vec::new();
    for d in state.registry.list_all().await? {
        let mac = d.device_id.to_string();
        let code = d.api_key.registration_code();
        if config.is_device_registered(&mac, Some(&code)) {
            continue;
        }
        out.push(PendingDevice {
            mac,
            registration_code: code,
            model: d.model.to_string(),
            firmware_version: d.firmware_version.clone(),
            last_seen: d.last_seen.to_rfc3339(),
        });
    }
    Ok(Json(out))
}

pub async fn get_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;

    let text = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(format!("read config: {e}")))?;
    let mut value: serde_yaml::Value =
        serde_yaml::from_str(&text).map_err(|e| ApiError::Internal(format!("parse config: {e}")))?;

    // Strip admin.token from the response.
    if let Some(map) = value.as_mapping_mut() {
        if let Some(admin) = map
            .get_mut(serde_yaml::Value::from("admin"))
            .and_then(|a| a.as_mapping_mut())
        {
            admin.remove(serde_yaml::Value::from("token"));
        }
    }

    let json = serde_json::to_value(&value)
        .map_err(|e| ApiError::Internal(format!("to json: {e}")))?;
    Ok(Json(json))
}
```

- [ ] **Step 4: Add routes**

In `src/api/admin/mod.rs` `admin_router`:

```rust
    Router::new()
        .route("/devices", get(read::list_devices))
        .route("/pending", get(read::pending))
        .route("/config", get(read::get_config))
```

- [ ] **Step 5: Run tests + full suite**

Run: `cargo test --test admin_read_test && make check`
Expected: PASS, clean.

- [ ] **Step 6: Commit**

```bash
git add src/api/admin/ tests/admin_read_test.rs
git commit -m "feat(admin): GET /api/admin/pending and /config

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: `GET /api/admin/screens` (param schemas + enumerations)

**Files:**
- Modify: `src/api/admin/read.rs`, `src/api/admin/mod.rs`
- Test: create `tests/admin_screens_test.rs`

**Interfaces:**
- Consumes: `AppState`, `schema_for_script` (Task 3), `AssetLoader::read_screen_string`.
- Produces: handler `screens(...)`; route `/screens`. Response shape:
  `{ "screens": [{name, params:[ParamField], schema_error:Option<String>}], "panels": [{name,width,height,colors}], "dither_algorithms": [String] }`.

- [ ] **Step 1: Write failing tests**

Create `tests/admin_screens_test.rs`:

```rust
mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn test_screens_lists_screens_and_enums() {
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/screens", &[AUTH]).await;
    assert_eq!(resp.status, StatusCode::OK);
    let json: serde_json::Value = resp.json();

    let screens = json["screens"].as_array().unwrap();
    assert!(screens.iter().any(|s| s["name"] == "transit"));
    assert!(json["panels"].as_array().unwrap().iter().any(|p| p["name"] == "trmnl_og"));
    assert!(json["dither_algorithms"].as_array().unwrap().iter().any(|d| d == "atkinson"));
}

#[tokio::test]
async fn test_transit_has_station_param_after_headers_added() {
    // After Task 12 adds @params headers, transit exposes a `station` param.
    let app = TestApp::new_admin("secret");
    let resp = app.get_with_headers("/api/admin/screens", &[AUTH]).await;
    let json: serde_json::Value = resp.json();
    let transit = json["screens"]
        .as_array().unwrap()
        .iter()
        .find(|s| s["name"] == "transit")
        .unwrap();
    let params = transit["params"].as_array().unwrap();
    assert!(params.iter().any(|p| p["name"] == "station"));
}
```

> The second test depends on Task 12 (the `@params` header for `transit`). It is written
> now but will pass only after Task 12. If executing strictly task-by-task, mark it
> `#[ignore]` here and remove the attribute in Task 12.

- [ ] **Step 2: Run to verify the first test fails**

Run: `cargo test --test admin_screens_test test_screens_lists`
Expected: FAIL — route not found.

- [ ] **Step 3: Implement**

Add to `src/api/admin/read.rs`:

```rust
use crate::models::param_schema::{schema_for_script, ParamField};

#[derive(Serialize)]
pub struct ScreenInfo {
    pub name: String,
    pub params: Vec<ParamField>,
    pub schema_error: Option<String>,
}

#[derive(Serialize)]
pub struct PanelInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub colors: String,
}

#[derive(Serialize)]
pub struct ScreensResponse {
    pub screens: Vec<ScreenInfo>,
    pub panels: Vec<PanelInfo>,
    pub dither_algorithms: Vec<String>,
}

/// Canonical dither algorithm names byonk understands.
const DITHER_ALGORITHMS: &[&str] = &[
    "floyd-steinberg",
    "atkinson",
    "atkinson-hybrid",
    "jarvis-judice-ninke",
    "sierra",
    "sierra-two-row",
    "sierra-lite",
    "sierra-light",
    "stucki",
    "burkes",
];

pub async fn screens(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ScreensResponse>, ApiError> {
    require_admin(&state, &headers)?;
    let config = state.config.load();

    let mut screens = Vec::new();
    for (name, sc) in &config.screens {
        let (params, schema_error) = match state.asset_loader.read_screen_string(&sc.script) {
            Err(e) => (Vec::new(), Some(format!("cannot read script: {e}"))),
            Ok(src) => match schema_for_script(&src) {
                Ok(Some(schema)) => (schema.fields, None),
                Ok(None) => (Vec::new(), None),
                Err(e) => {
                    tracing::warn!(screen = %name, error = %e, "invalid @params schema");
                    (Vec::new(), Some(e))
                }
            },
        };
        screens.push(ScreenInfo { name: name.clone(), params, schema_error });
    }

    let panels = config
        .panels
        .iter()
        .map(|(name, p)| PanelInfo {
            name: name.clone(),
            width: p.width,
            height: p.height,
            colors: p.colors.clone(),
        })
        .collect();

    Ok(Json(ScreensResponse {
        screens,
        panels,
        dither_algorithms: DITHER_ALGORITHMS.iter().map(|s| s.to_string()).collect(),
    }))
}
```

> Confirm `PanelConfig` field names (`width`, `height`, `colors`) by checking
> `src/models/config.rs` around line 182; adjust if the struct differs.

- [ ] **Step 4: Add route + run**

In `admin_router`: `.route("/screens", get(read::screens))`.

Run: `cargo test --test admin_screens_test test_screens_lists && make check`
Expected: PASS, clean.

- [ ] **Step 5: Commit**

```bash
git add src/api/admin/ tests/admin_screens_test.rs
git commit -m "feat(admin): GET /api/admin/screens with param schemas + enums

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Comment-preserving config writer (yamlpath/yamlpatch adapter)

This task binds the one external API (`yamlpatch`). **Implement it against the installed
crate first** — the public contract and tests below are fixed; the body uses
`yamlpatch`/`yamlpath`, whose exact `Value`/`Route` construction must be confirmed via
`cargo doc -p yamlpatch -p yamlpath`.

**Files:**
- Create: `src/services/config_writer.rs`
- Modify: `src/services/mod.rs`
- Test: inline `#[cfg(test)]` in `src/services/config_writer.rs`

**Interfaces:**
- Produces (the stable contract other tasks call):
  - `pub fn set_scalar(yaml: &str, path: &[&str], value: serde_yaml::Value) -> Result<String, ConfigWriteError>`
  - `pub fn upsert_device(yaml: &str, key: &str, block: &serde_yaml::Mapping) -> Result<String, ConfigWriteError>`
  - `pub fn remove_device(yaml: &str, key: &str) -> Result<String, ConfigWriteError>`
  - `pub enum ConfigWriteError { NotFound(String), Patch(String) }` (impl `Display`)
- Each function takes the current YAML text and returns new YAML text with comments outside
  the touched region preserved.

- [ ] **Step 1: Write failing tests (these define the contract)**

Create `src/services/config_writer.rs`:

```rust
//! Comment-preserving edits to `config.yaml`, built on `yamlpath`/`yamlpatch`.
//!
//! Strategy (avoids yamlpatch's weak spots on sequences/flow lists):
//! - global scalar settings → in-place scalar replace
//! - device add/edit/remove → remove the device subtree + append a freshly
//!   block-serialized subtree (device blocks are machine-managed, so no user
//!   comments live inside them).

// implementation added below

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# top comment
registration:
  enabled: true   # inline comment
auth_mode: api_key
devices:
  \"AA:BB\":
    screen: transit
    params:
      station: Olten
# trailing comment
";

    #[test]
    fn test_set_scalar_preserves_comments() {
        let out = set_scalar(SAMPLE, &["registration", "enabled"], false.into()).unwrap();
        assert!(out.contains("# top comment"));
        assert!(out.contains("# trailing comment"));
        assert!(out.contains("enabled: false"));
    }

    #[test]
    fn test_remove_device_keeps_other_comments() {
        let out = remove_device(SAMPLE, "AA:BB").unwrap();
        assert!(!out.contains("station: Olten"));
        assert!(out.contains("# top comment"));
        assert!(out.contains("# trailing comment"));
    }

    #[test]
    fn test_upsert_adds_new_device() {
        let mut block = serde_yaml::Mapping::new();
        block.insert("screen".into(), "hello".into());
        let out = upsert_device(SAMPLE, "CC:DD", &block).unwrap();
        // Re-parse and confirm the new device exists with the right screen.
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(v["devices"]["CC:DD"]["screen"], serde_yaml::Value::from("hello"));
        assert!(out.contains("# top comment"));
    }

    #[test]
    fn test_upsert_edits_existing_device() {
        let mut block = serde_yaml::Mapping::new();
        block.insert("screen".into(), "graytest".into());
        let out = upsert_device(SAMPLE, "AA:BB", &block).unwrap();
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(v["devices"]["AA:BB"]["screen"], serde_yaml::Value::from("graytest"));
        assert!(out.contains("# top comment"));
    }

    #[test]
    fn test_remove_missing_device_errors() {
        assert!(matches!(remove_device(SAMPLE, "ZZ:ZZ"), Err(ConfigWriteError::NotFound(_))));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib config_writer`
Expected: FAIL — functions not defined.

- [ ] **Step 3: Implement the error type + helpers**

```rust
use yamlpatch::{apply_yaml_patches, Op, Patch};
use yamlpath::Document;

#[derive(Debug)]
pub enum ConfigWriteError {
    NotFound(String),
    Patch(String),
}

impl std::fmt::Display for ConfigWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigWriteError::NotFound(s) => write!(f, "not found: {s}"),
            ConfigWriteError::Patch(s) => write!(f, "patch failed: {s}"),
        }
    }
}
impl std::error::Error for ConfigWriteError {}

/// Build a `yamlpatch` value from a serde_yaml value by round-tripping through
/// a YAML string (keeps us decoupled from yamlpatch's internal value type).
///
/// CONFIRM: the exact constructor for `Op::Replace`/`Op::Add` values against the
/// installed `yamlpatch` version (`cargo doc -p yamlpatch`). The crate exposes a
/// `yaml_serde::Value`; build it by parsing `serde_yaml::to_string(value)`.
fn to_patch_value(value: &serde_yaml::Value) -> Result<yamlpatch::yaml_serde::Value, ConfigWriteError> {
    let s = serde_yaml::to_string(value).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    yamlpatch::yaml_serde::from_str(&s).map_err(|e| ConfigWriteError::Patch(e.to_string()))
}

fn render(doc: Document) -> String {
    doc.source().to_string()
}

fn document(yaml: &str) -> Result<Document, ConfigWriteError> {
    Document::new(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))
}
```

- [ ] **Step 4: Implement `set_scalar`**

```rust
/// Replace a scalar value at `path` (e.g. `["registration","enabled"]`).
pub fn set_scalar(
    yaml: &str,
    path: &[&str],
    value: serde_yaml::Value,
) -> Result<String, ConfigWriteError> {
    let doc = document(yaml)?;
    // Build a route to the scalar. CONFIRM exact route-builder API; using with_key.
    let mut route = yamlpath::Route::root();
    for key in path {
        route = route.with_key(*key);
    }
    let patch = Patch {
        route,
        operation: Op::Replace(to_patch_value(&value)?),
    };
    let new_doc = apply_yaml_patches(&doc, &[patch])
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    Ok(render(new_doc))
}
```

- [ ] **Step 5: Implement `remove_device` and `upsert_device`**

```rust
/// Remove `devices.<key>` entirely.
pub fn remove_device(yaml: &str, key: &str) -> Result<String, ConfigWriteError> {
    // Confirm presence first for a clean NotFound error.
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let exists = parsed
        .get("devices")
        .and_then(|d| d.get(key))
        .is_some();
    if !exists {
        return Err(ConfigWriteError::NotFound(format!("device {key}")));
    }

    let doc = document(yaml)?;
    let route = yamlpath::Route::root().with_key("devices").with_key(key);
    let patch = Patch { route, operation: Op::Remove };
    let new_doc = apply_yaml_patches(&doc, &[patch])
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    Ok(render(new_doc))
}

/// Add a new device or replace an existing one with `block`.
pub fn upsert_device(
    yaml: &str,
    key: &str,
    block: &serde_yaml::Mapping,
) -> Result<String, ConfigWriteError> {
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let exists = parsed.get("devices").and_then(|d| d.get(key)).is_some();

    // Edit = remove then add (robust against sequence/flow-list params).
    let base = if exists { remove_device(yaml, key)? } else { yaml.to_string() };

    let doc = document(&base)?;
    let route = yamlpath::Route::root().with_key("devices");
    let value = to_patch_value(&serde_yaml::Value::Mapping(block.clone()))?;
    let patch = Patch {
        route,
        operation: Op::Add { key: key.to_string(), value },
    };
    let new_doc = apply_yaml_patches(&doc, &[patch])
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    Ok(render(new_doc))
}
```

> If the installed `yamlpath` exposes a different route builder (e.g. the `route!` macro or
> `Route::from(...)`), adapt the three `Route::root().with_key(..)` chains accordingly. The
> tests in Step 1 are the source of truth — make them green without changing their
> assertions.

- [ ] **Step 6: Declare module + run tests**

In `src/services/mod.rs` add `pub mod config_writer;` and
`pub use config_writer::{remove_device, set_scalar, upsert_device, ConfigWriteError};`.

Run: `cargo test --lib config_writer`
Expected: PASS (all five). If a route/value API mismatch appears, fix the adapter internals
only.

- [ ] **Step 7: Commit**

```bash
git add src/services/config_writer.rs src/services/mod.rs
git commit -m "feat(admin): comment-preserving config writer (yamlpath/yamlpatch)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: Device write endpoints (`POST`/`PATCH`/`DELETE /api/admin/devices`)

**Files:**
- Create: `src/api/admin/write.rs`
- Modify: `src/api/admin/mod.rs`
- Test: create `tests/admin_write_test.rs`

**Interfaces:**
- Consumes: `config_writer` (Task 9), `validate_params`/`schema_for_script` (Task 3/4),
  `reload_config` (Task 5), `AssetLoader::config_path`/`read_config_string`.
- Produces: handlers `add_device`, `patch_device`, `delete_device`; routes
  `POST /devices`, `PATCH /devices/:key`, `DELETE /devices/:key`.
- Request body shape (POST): `{ "key": String, "screen": String, "panel"?: String, "dither"?: String, "colors"?: String, "params"?: object }`.
  PATCH body: same fields, all optional, no `key`.

- [ ] **Step 1: Write failing tests**

Create `tests/admin_write_test.rs`:

```rust
mod common;

use axum::http::StatusCode;
use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn test_write_on_embedded_config_returns_409() {
    let app = TestApp::new_admin("secret"); // embedded-only
    let body = r#"{"key":"CC:DD:EE:FF:00:11","screen":"hello"}"#;
    let resp = app.post_json("/api/admin/devices", &[AUTH], body).await;
    assert_eq!(resp.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_add_device_persists_and_hot_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let body = r#"{"key":"CC:DD:EE:FF:00:11","screen":"hello"}"#;
    let resp = app.post_json("/api/admin/devices", &[AUTH], body).await;
    assert_eq!(resp.status, StatusCode::OK);

    // File updated + comment preserved.
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains("CC:DD:EE:FF:00:11"));

    // Hot-reload: GET /devices shows it without restart.
    let listed = app.get_with_headers("/api/admin/devices", &[AUTH]).await;
    let json: serde_json::Value = listed.json();
    assert!(json.as_array().unwrap().iter().any(|d| d["mac"] == "CC:DD:EE:FF:00:11"));
}

#[tokio::test]
async fn test_add_unknown_screen_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    let body = r#"{"key":"CC:DD:EE:FF:00:11","screen":"does-not-exist"}"#;
    let resp = app.post_json("/api/admin/devices", &[AUTH], body).await;
    assert_eq!(resp.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_patch_then_delete_device() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = TestApp::new_admin_with_file("secret", dir.path());
    app.post_json("/api/admin/devices", &[AUTH], r#"{"key":"CC:DD","screen":"hello"}"#).await;

    let patch = app.patch_json("/api/admin/devices/CC:DD", &[AUTH], r#"{"screen":"graytest"}"#).await;
    assert_eq!(patch.status, StatusCode::OK);

    let del = app.delete("/api/admin/devices/CC:DD", &[AUTH]).await;
    assert_eq!(del.status, StatusCode::OK);
    let del_again = app.delete("/api/admin/devices/CC:DD", &[AUTH]).await;
    assert_eq!(del_again.status, StatusCode::NOT_FOUND);
}
```

Add `tempfile = "3"` to `[dev-dependencies]` in `Cargo.toml` if not already present
(check first).

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test admin_write_test`
Expected: FAIL — routes not found.

- [ ] **Step 3: Implement the write handlers**

Create `src/api/admin/write.rs`:

```rust
//! Admin write endpoints: device mappings + global settings.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use std::collections::HashMap;

use crate::error::ApiError;
use crate::models::param_schema::{schema_for_script, validate_params};
use crate::server::{reload_config, AppState};
use crate::services::config_writer;

use super::require_admin;

#[derive(Deserialize)]
pub struct DeviceWrite {
    pub key: Option<String>, // required for POST, ignored for PATCH (taken from URL)
    pub screen: Option<String>,
    pub panel: Option<String>,
    pub dither: Option<String>,
    pub colors: Option<String>,
    pub params: Option<HashMap<String, serde_yaml::Value>>,
}

/// Guard: writes require a file-backed config.
fn require_file_config(state: &AppState) -> Result<std::path::PathBuf, ApiError> {
    state
        .asset_loader
        .config_path()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| ApiError::Conflict("config is embedded/read-only; set CONFIG_FILE".into()))
}

/// Validate the screen exists and (if it has a schema) the params pass it.
fn validate_screen_and_params(
    state: &AppState,
    screen: &str,
    params: &HashMap<String, serde_yaml::Value>,
) -> Result<(), ApiError> {
    let config = state.config.load();
    let sc = config
        .screens
        .get(screen)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown screen `{screen}`")))?;
    if let Ok(src) = state.asset_loader.read_screen_string(&sc.script) {
        if let Ok(Some(schema)) = schema_for_script(&src) {
            if let Err(errs) = validate_params(&schema, params) {
                return Err(ApiError::BadRequest(errs.join("; ")));
            }
        }
    }
    Ok(())
}

/// Build the YAML mapping for a device block from the provided fields.
fn device_block(w: &DeviceWrite, screen: &str) -> serde_yaml::Mapping {
    let mut m = serde_yaml::Mapping::new();
    m.insert("screen".into(), screen.into());
    if let Some(p) = &w.panel { m.insert("panel".into(), p.as_str().into()); }
    if let Some(d) = &w.dither { m.insert("dither".into(), d.as_str().into()); }
    if let Some(c) = &w.colors { m.insert("colors".into(), c.as_str().into()); }
    if let Some(params) = &w.params {
        let mut pm = serde_yaml::Mapping::new();
        for (k, v) in params {
            pm.insert(k.as_str().into(), v.clone());
        }
        m.insert("params".into(), serde_yaml::Value::Mapping(pm));
    }
    m
}

fn persist(state: &AppState, path: &std::path::Path, new_yaml: String) -> Result<(), ApiError> {
    // Snapshot current contents so we can roll back if the new text fails to reparse.
    let previous = std::fs::read_to_string(path).ok();

    // Atomic write: temp file in same dir, then rename.
    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(&tmp, &new_yaml)
        .map_err(|e| ApiError::Internal(format!("write temp: {e}")))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| ApiError::Internal(format!("rename: {e}")))?;

    // Reload into the live config; on failure, roll the file back.
    if let Err(e) = reload_config(state) {
        if let Some(prev) = previous {
            let _ = std::fs::write(path, prev);
        }
        return Err(ApiError::Internal(format!("reload failed, rolled back: {e}")));
    }
    Ok(())
}
```

> **Write serialization:** each mutating handler below acquires the write lock for the whole
> read-modify-write-reload cycle. Add this line as the first statement after
> `require_file_config(&state)?` in `add_device`, `patch_device`, `delete_device`, and
> (Task 11) `patch_settings`:
>
> ```rust
>     let _guard = state.write_lock.lock().await;
> ```

pub async fn add_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DeviceWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;
    let key = body
        .key
        .clone()
        .ok_or_else(|| ApiError::BadRequest("`key` is required".into()))?;
    let screen = body
        .screen
        .clone()
        .ok_or_else(|| ApiError::BadRequest("`screen` is required".into()))?;

    // Reject duplicate.
    if state.config.load().devices.contains_key(&key) {
        return Err(ApiError::Conflict(format!("device `{key}` already exists")));
    }

    let empty = HashMap::new();
    validate_screen_and_params(&state, &screen, body.params.as_ref().unwrap_or(&empty))?;

    let block = device_block(&body, &screen);
    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = config_writer::upsert_device(&yaml, &key, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(&state, &path, new_yaml)?;

    Ok(Json(serde_json::json!({ "key": key, "screen": screen })))
}

pub async fn patch_device(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(body): Json<DeviceWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    // Must already exist.
    let existing = {
        let config = state.config.load();
        config
            .devices
            .get(&key)
            .cloned()
            .ok_or_else(|| ApiError::NotFound)?
    };

    // Merge: start from existing, override provided fields.
    let screen = body.screen.clone().unwrap_or(existing.screen.clone());
    let merged = DeviceWrite {
        key: Some(key.clone()),
        screen: Some(screen.clone()),
        panel: body.panel.clone().or(existing.panel.clone()),
        dither: body.dither.clone().or(existing.dither.clone()),
        colors: body.colors.clone().or(existing.colors.clone()),
        params: Some(body.params.clone().unwrap_or(existing.params.clone())),
    };

    let empty = HashMap::new();
    validate_screen_and_params(&state, &screen, merged.params.as_ref().unwrap_or(&empty))?;

    let block = device_block(&merged, &screen);
    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = config_writer::upsert_device(&yaml, &key, &block)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    persist(&state, &path, new_yaml)?;

    Ok(Json(serde_json::json!({ "key": key, "screen": screen })))
}

pub async fn delete_device(
    State(state): State<AppState>,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let new_yaml = match config_writer::remove_device(&yaml, &key) {
        Ok(y) => y,
        Err(config_writer::ConfigWriteError::NotFound(_)) => return Err(ApiError::NotFound),
        Err(e) => return Err(ApiError::Internal(e.to_string())),
    };
    persist(&state, &path, new_yaml)?;

    Ok(Json(serde_json::json!({ "deleted": key })))
}
```

- [ ] **Step 4: Wire routes**

In `src/api/admin/mod.rs`: add `pub mod write;`, import `post`, `patch`, `delete` from
`axum::routing`, and extend `admin_router`:

```rust
        .route("/devices", post(write::add_device))
        .route("/devices/:key", patch(write::patch_device).delete(write::delete_device))
```

(Combine with the existing `.route("/devices", get(read::list_devices))` using
`get(read::list_devices).post(write::add_device)` on the same path.)

- [ ] **Step 5: Run tests + full suite**

Run: `cargo test --test admin_write_test && make check`
Expected: PASS, clean.

- [ ] **Step 6: Commit**

```bash
git add src/api/admin/ tests/admin_write_test.rs Cargo.toml Cargo.lock
git commit -m "feat(admin): device write endpoints with validation + hot-reload

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: Global settings — `PATCH /api/admin/settings`

**Files:**
- Modify: `src/api/admin/write.rs`, `src/api/admin/mod.rs`
- Test: add to `tests/admin_write_test.rs`

**Interfaces:**
- Consumes: `config_writer::set_scalar`, `reload_config`.
- Produces: handler `patch_settings`; route `PATCH /settings`. Body:
  `{ "registration_enabled"?: bool, "auth_mode"?: String, "default_screen"?: String }`.

- [ ] **Step 1: Write failing test**

Add to `tests/admin_write_test.rs`:

```rust
#[tokio::test]
async fn test_patch_settings_toggles_registration() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = TestApp::new_admin_with_file("secret", dir.path());

    let resp = app
        .patch_json("/api/admin/settings", &[AUTH], r#"{"registration_enabled":false}"#)
        .await;
    assert_eq!(resp.status, StatusCode::OK);

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains("enabled: false"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --test admin_write_test test_patch_settings`
Expected: FAIL — route not found.

- [ ] **Step 3: Implement**

Add to `src/api/admin/write.rs`:

```rust
#[derive(Deserialize)]
pub struct SettingsWrite {
    pub registration_enabled: Option<bool>,
    pub auth_mode: Option<String>,
    pub default_screen: Option<String>,
}

pub async fn patch_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SettingsWrite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &headers)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let mut yaml = state
        .asset_loader
        .read_config_string()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if let Some(enabled) = body.registration_enabled {
        yaml = config_writer::set_scalar(&yaml, &["registration", "enabled"], enabled.into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    if let Some(mode) = &body.auth_mode {
        if mode != "api_key" && mode != "ed25519" {
            return Err(ApiError::BadRequest("auth_mode must be api_key or ed25519".into()));
        }
        yaml = config_writer::set_scalar(&yaml, &["auth_mode"], mode.as_str().into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    if let Some(screen) = &body.default_screen {
        if !state.config.load().screens.contains_key(screen) {
            return Err(ApiError::BadRequest(format!("unknown screen `{screen}`")));
        }
        yaml = config_writer::set_scalar(&yaml, &["default_screen"], screen.as_str().into())
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    persist(&state, &path, yaml)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

> `registration.enabled` is the documented path. If the seeded config omits the
> `registration:` block, `set_scalar` will return a patch error — the default config in this
> repo DOES include `registration:\n  enabled: true`, so this is fine. If a future config
> lacks it, the handler returns 500; acceptable for Phase 1 (documented limitation).

- [ ] **Step 4: Add route**

In `admin_router`: `.route("/settings", patch(write::patch_settings))`.

- [ ] **Step 5: Run tests + full suite**

Run: `cargo test --test admin_write_test && make check`
Expected: PASS, clean.

- [ ] **Step 6: Commit**

```bash
git add src/api/admin/ tests/admin_write_test.rs
git commit -m "feat(admin): PATCH /api/admin/settings for global settings

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 12: Add `@params` headers to all repo screens

**Files:**
- Modify: `screens/transit.lua`, `screens/gphoto.lua`, `screens/floerli.lua`,
  `screens/fontdemo-bitmap.lua` (add `@params`); leave `default`, `graytest`, `hello`,
  `hintdemo`, `calibrator`, `mandelbrot`, `fontdemo-terminus` without a block (no params).
- Test: create `tests/screen_schemas_test.rs`

**Interfaces:**
- Consumes: `byonk::models::param_schema::schema_for_script`, `byonk::assets::AssetLoader`.

- [ ] **Step 1: Write a failing test that all bundled screens parse cleanly**

Create `tests/screen_schemas_test.rs`:

```rust
//! Every bundled screen's @params block must parse without error, and known
//! screens expose their documented params.

use byonk::assets::AssetLoader;
use byonk::models::param_schema::schema_for_script;
use std::path::Path;

fn schema(script: &str) -> Option<Vec<String>> {
    let loader = AssetLoader::new(None, None, None);
    let src = loader.read_screen_string(Path::new(script)).unwrap();
    schema_for_script(&src)
        .expect("schema must parse")
        .map(|s| s.fields.iter().map(|f| f.name.clone()).collect())
}

#[test]
fn test_transit_params() {
    let names = schema("transit.lua").unwrap();
    assert!(names.contains(&"station".to_string()));
    assert!(names.contains(&"limit".to_string()));
}

#[test]
fn test_gphoto_params() {
    let names = schema("gphoto.lua").unwrap();
    assert!(names.contains(&"album_url".to_string()));
}

#[test]
fn test_fontdemo_bitmap_is_enum() {
    let loader = AssetLoader::new(None, None, None);
    let src = loader.read_screen_string(Path::new("fontdemo-bitmap.lua")).unwrap();
    let schema = schema_for_script(&src).unwrap().unwrap();
    let f = schema.fields.iter().find(|f| f.name == "font_prefix").unwrap();
    assert!(!f.options.is_empty());
}

#[test]
fn test_no_param_screens_have_no_schema_or_empty() {
    for s in ["default.lua", "graytest.lua", "hello.lua", "mandelbrot.lua"] {
        let loader = AssetLoader::new(None, None, None);
        let src = loader.read_screen_string(Path::new(s)).unwrap();
        // Either no block, or a block that parses to zero fields.
        let parsed = schema_for_script(&src).expect("must parse");
        if let Some(p) = parsed {
            assert!(p.fields.is_empty());
        }
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --test screen_schemas_test`
Expected: FAIL — params not declared yet.

- [ ] **Step 3: Add the `@params` header to `screens/transit.lua`**

Insert at the very top of the file (before existing code):

```lua
--[[ @params
station:
  type: string
  label: "Stop name"
  default: "Olten, Südwest"
  description: "Stop name as used by the transport API"
limit:
  type: int
  label: "Departures"
  default: 8
  min: 1
  max: 30
  mode: box
  description: "Number of departures to show"
]]
```

- [ ] **Step 4: Add `@params` to `screens/gphoto.lua`**

```lua
--[[ @params
album_url:
  type: url
  label: "Album URL"
  required: true
  description: "Shared Google Photos album link"
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
]]
```

- [ ] **Step 5: Add `@params` to `screens/floerli.lua`**

```lua
--[[ @params
room:
  type: string
  label: "Room name"
  default: "Rosa"
test_timestamp:
  type: int
  label: "Test timestamp"
  hidden: true
  description: "Debug: override current time (unix seconds)"
]]
```

- [ ] **Step 6: Add `@params` to `screens/fontdemo-bitmap.lua`**

```lua
--[[ @params
font_prefix:
  type: enum
  label: "Font family"
  default: "X11Helv"
  options:
    - {value: X11Helv, label: "X11 Helvetica"}
    - {value: X11LuSans, label: "X11 Lucida Sans"}
    - {value: X11LuType, label: "X11 Lucida Typewriter"}
    - {value: X11Term, label: "X11 Terminal"}
    - {value: X11Misc, label: "X11 Misc"}
]]
```

- [ ] **Step 7: If Task 8's second test was `#[ignore]`d, remove the attribute now**

Edit `tests/admin_screens_test.rs` to un-ignore `test_transit_has_station_param_after_headers_added`.

- [ ] **Step 8: Run tests + full suite**

Run: `cargo test --test screen_schemas_test --test admin_screens_test && make check`
Expected: PASS, clean.

- [ ] **Step 9: Commit**

```bash
git add screens/transit.lua screens/gphoto.lua screens/floerli.lua screens/fontdemo-bitmap.lua tests/screen_schemas_test.rs tests/admin_screens_test.rs
git commit -m "feat(admin): declare @params schemas for bundled screens

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 13: Documentation + changelog

**Files:**
- Create: `docs/src/api/admin-api.md`
- Modify: `docs/src/SUMMARY.md`
- Modify: `CHANGES.md`

**Interfaces:** none (docs only).

- [ ] **Step 1: Write the admin API doc page**

Create `docs/src/api/admin-api.md` documenting: enabling the API (`BYONK_ADMIN_TOKEN` or
`admin.token`), the 404-when-disabled behavior, `Authorization: Bearer` usage, every
endpoint from Tasks 6–11 with a request/response example, the `@params` schema format
(all fields from Task 3, with the `transit` example), and that writes preserve comments and
hot-reload. Keep examples consistent with the handlers' actual JSON shapes.

- [ ] **Step 2: Link it into the book**

Add to `docs/src/SUMMARY.md` under the appropriate API section:

```markdown
  - [Admin API](api/admin-api.md)
```

- [ ] **Step 3: Build the docs**

Run: `make docs`
Expected: builds without warnings about the new page.

- [ ] **Step 4: Update CHANGES.md**

Under the `## [Unreleased]` section in `CHANGES.md`, add:

```markdown
### Added
- Admin/management API (`/api/admin/*`), gated by a bearer token
  (`BYONK_ADMIN_TOKEN` env or `admin.token` in config; disabled = returns 404):
  read device telemetry, pending/unregistered devices, effective config, and
  screen param schemas; create/update/delete device mappings and update global
  settings.
- Per-screen parameter schemas via a parsed (not executed) `@params` header in
  each screen's `.lua`, with UI hints (label, min/max/step/unit/mode, enum
  options, sensitive/multiline/hidden/advanced). Bundled screens now declare
  their params.
- Config hot-reload: admin writes update `config.yaml` in place (preserving
  comments via yamlpath/yamlpatch) and take effect without a restart.
```

- [ ] **Step 5: Commit**

```bash
git add docs/src/api/admin-api.md docs/src/SUMMARY.md CHANGES.md
git commit -m "docs(admin): document the admin API, @params schema, and changelog

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Final verification

- [ ] Run `make check` — fmt, clippy, and the entire test suite pass.
- [ ] Run `make docs` — documentation builds.
- [ ] Manually confirm acceptance criteria from the spec (§12):
  1. No token ⇒ `/api/admin/*` returns 404 (covered by `test_admin_disabled_returns_404`).
  2. Token set ⇒ endpoints work with Bearer; bad/missing token rejected (covered).
  3. Device screen change served on next `/api/display` without restart — verify by hand:
     start the server with a file config + token, `PATCH` a device's screen, then request
     `/api/display` for that device and confirm the new screen renders.
  4. Comments preserved after writes (covered by `config_writer` + write tests).
  5. `/api/admin/screens` returns param schemas; validation enforced on writes (covered).
  6. `make check` passes; `CHANGES.md` + docs updated (this task).
