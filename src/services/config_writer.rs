//! Comment-preserving edits to `config.yaml`, built on `yamlpath`/`yamlpatch`.
//!
//! Strategy (avoids yamlpatch's weak spots on sequences/flow lists):
//! - global scalar settings → in-place scalar replace
//! - device / package add/edit/remove → remove the entry's subtree + append a
//!   freshly block-serialized subtree (entries are machine-managed, so no user
//!   comments live inside them). Both `devices:` and `packages:` share the
//!   same section-generic helpers.

use yamlpatch::{apply_yaml_patches, Op, Patch};
use yamlpath::Document;

/// Errors produced when rewriting `config.yaml`.
#[derive(Debug)]
pub enum ConfigWriteError {
    /// The requested device/key was not present in the document.
    NotFound(String),
    /// The underlying patch / parse operation failed.
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

/// Build a `yaml_serde::Value` (the value type `yamlpatch` operates on) from a
/// `serde_yaml::Value` by round-tripping through a YAML string. This keeps us
/// decoupled from the exact in-memory representation of either crate.
fn to_patch_value(value: &serde_yaml::Value) -> Result<yaml_serde::Value, ConfigWriteError> {
    let s = serde_yaml::to_string(value).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    yaml_serde::from_str(&s).map_err(|e| ConfigWriteError::Patch(e.to_string()))
}

fn document(yaml: &str) -> Result<Document, ConfigWriteError> {
    Document::new(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))
}

fn render(doc: Document) -> String {
    doc.source().to_string()
}

/// Set a scalar value at `path` (e.g. `["registration","enabled"]`),
/// preserving all surrounding comments and formatting.
///
/// If the key already exists it is replaced in place; if it is absent it is
/// added to its parent mapping (which must exist). This covers the common case
/// where a key is optional in config and may not be present yet.
pub fn set_scalar(
    yaml: &str,
    path: &[&str],
    value: serde_yaml::Value,
) -> Result<String, ConfigWriteError> {
    assert!(!path.is_empty(), "set_scalar: path must not be empty");

    let doc = document(yaml)?;

    // Build the full route to the target key.
    let mut route = yamlpath::Route::default();
    for key in path {
        route = route.with_key(*key);
    }

    // Try Replace first (key already exists). On failure, fall back to Add
    // at the parent route (adds the key to an existing mapping).
    let replace_patch = Patch {
        route: route.clone(),
        operation: Op::Replace(to_patch_value(&value)?),
    };
    if let Ok(new_doc) = apply_yaml_patches(&doc, &[replace_patch]) {
        return Ok(render(new_doc));
    }

    // Key doesn't exist yet — add it to the parent mapping.
    let last_key = path.last().expect("non-empty path");
    let mut parent_route = yamlpath::Route::default();
    for key in &path[..path.len() - 1] {
        parent_route = parent_route.with_key(*key);
    }
    let add_patch = Patch {
        route: parent_route,
        operation: Op::Add {
            key: last_key.to_string(),
            value: to_patch_value(&value)?,
        },
    };
    let new_doc = apply_yaml_patches(&doc, &[add_patch])
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    Ok(render(new_doc))
}

/// Byte offset just past the next `\n` at or after `from` (or end of string).
fn next_line_start(s: &str, from: usize) -> usize {
    match s[from..].find('\n') {
        Some(i) => from + i + 1,
        None => s.len(),
    }
}

/// Count of leading space characters in `line` (its indentation depth).
fn indent_of(line: &str) -> usize {
    line.bytes().take_while(|b| *b == b' ').count()
}

/// Compute the byte range covering `<section>.<key>` *and only* its own block,
/// starting at the key line and extending over the deeper-indented body
/// lines that belong to it.
///
/// This is done manually instead of via `yamlpatch::Op::Remove` because
/// tree-sitter attaches any following less-indented comment (e.g. a trailing
/// top-level `# comment`) to the last block node, which would make `Op::Remove`
/// delete that comment too. Scanning by indentation keeps such comments intact.
fn block_range(yaml: &str, section: &str, key: &str) -> Result<(usize, usize), ConfigWriteError> {
    let doc = document(yaml)?;
    let route = yamlpath::Route::default().with_key(section).with_key(key);
    let feature = doc
        .query_pretty(&route)
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let key_byte = feature.location.byte_span.0;

    // Start of the key line.
    let line_start = yaml[..key_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let key_indent = indent_of(&yaml[line_start..]);

    // Walk forward, consuming lines that are more deeply indented than the key.
    let mut end = next_line_start(yaml, line_start);
    while end < yaml.len() {
        let line_end = next_line_start(yaml, end);
        let line = &yaml[end..line_end];
        if line.trim().is_empty() || indent_of(line) <= key_indent {
            break;
        }
        end = line_end;
    }

    Ok((line_start, end))
}

/// Remove `<section>.<key>` entirely. Returns [`ConfigWriteError::NotFound`] if
/// the entry does not exist. `label` is the singular noun used in the
/// [`ConfigWriteError::NotFound`] message (e.g. `"device"`, `"package"`).
fn remove_from_section(
    yaml: &str,
    section: &str,
    label: &str,
    key: &str,
) -> Result<String, ConfigWriteError> {
    // Confirm presence first for a clean NotFound error.
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let exists = parsed.get(section).and_then(|d| d.get(key)).is_some();
    if !exists {
        return Err(ConfigWriteError::NotFound(format!("{label} {key}")));
    }

    let (start, end) = block_range(yaml, section, key)?;
    let mut out = yaml.to_string();
    out.replace_range(start..end, "");
    Ok(out)
}

/// Add a new entry or replace an existing one with `block` under `section`
/// (e.g. `devices` or `packages`). `label` is the singular noun used in the
/// [`ConfigWriteError::NotFound`] message.
///
/// Editing is implemented as remove-then-add so it is robust against odd
/// existing layouts (flow lists / sequence params) inside the old block.
fn upsert_in_section(
    yaml: &str,
    section: &str,
    label: &str,
    key: &str,
    block: &serde_yaml::Mapping,
) -> Result<String, ConfigWriteError> {
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let exists = parsed.get(section).and_then(|d| d.get(key)).is_some();

    // Edit = remove then add (robust against sequence/flow-list params).
    let base = if exists {
        remove_from_section(yaml, section, label, key)?
    } else {
        yaml.to_string()
    };

    let base_parsed: serde_yaml::Value =
        serde_yaml::from_str(&base).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;

    // Serialize the entry as a block and insert it manually. We do NOT use
    // `yamlpatch`'s Op::Add here: it mis-indents nested sub-maps (e.g. `params`),
    // writing the sub-keys as siblings of `params:` instead of children.
    // serde_yaml serializes nested maps correctly, and a uniform two-space indent
    // preserves that nesting.
    let indented = serialize_indented(key, block)?;

    match base_parsed.get(section) {
        // Section present with at least one entry: insert right after the header.
        Some(v) if v.as_mapping().map(|m| !m.is_empty()).unwrap_or(false) => {
            insert_after_header(&base, section, &indented)
        }
        // Section present but empty (`section:` null, or `section: {}`).
        Some(_) => insert_into_empty_section(&base, section, &indented),
        // Section absent entirely: append a brand-new section.
        None => Ok(insert_new_section(&base, section, &indented)),
    }
}

/// Serialize `{ key: block }` and indent every line two spaces so the key
/// sits one level under the section header, with nested sub-maps (e.g.
/// `params`) nested correctly.
fn serialize_indented(key: &str, block: &serde_yaml::Mapping) -> Result<String, ConfigWriteError> {
    let mut outer = serde_yaml::Mapping::new();
    outer.insert(
        serde_yaml::Value::from(key),
        serde_yaml::Value::Mapping(block.clone()),
    );
    let serialized = serde_yaml::to_string(&serde_yaml::Value::Mapping(outer))
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let mut indented = String::new();
    for line in serialized.lines() {
        if line.is_empty() {
            indented.push('\n');
        } else {
            indented.push_str("  ");
            indented.push_str(line);
            indented.push('\n');
        }
    }
    Ok(indented)
}

/// Byte offset just after the `<section>:` header line (searching
/// `\n<section>:`, with a fallback for a document that starts with
/// `<section>:`).
fn header_bounds(base: &str, section: &str) -> Result<(usize, usize), ConfigWriteError> {
    let needle = format!("\n{section}:");
    let prefix = format!("{section}:");
    let after_colon = base
        .find(&needle)
        .map(|i| i + needle.len())
        .or_else(|| {
            if base.starts_with(&prefix) {
                Some(prefix.len())
            } else {
                None
            }
        })
        .ok_or_else(|| ConfigWriteError::Patch(format!("`{section}:` not found in config")))?;
    Ok((after_colon, next_line_start(base, after_colon)))
}

/// Insert a serialized block immediately after the `<section>:` header line,
/// among existing block-form entries. The new entry becomes the first one;
/// existing entries (and any trailing comments) follow unchanged.
fn insert_after_header(
    base: &str,
    section: &str,
    indented: &str,
) -> Result<String, ConfigWriteError> {
    let (_, line_end) = header_bounds(base, section)?;
    let mut out = base.to_string();
    out.insert_str(line_end, indented);
    Ok(out)
}

/// Insert a serialized block under an empty `<section>:` header, preserving
/// everything else (including trailing comments).
///
/// Handles two forms of an empty section:
/// - `<section>:` — null / empty block (e.g. after the last entry was removed)
/// - `<section>: {}` — empty flow mapping (e.g. the shipped default config)
///
/// In both cases we replace whatever follows `<section>:` on that line with
/// `\n<indented block>`, producing a valid block mapping.
fn insert_into_empty_section(
    base: &str,
    section: &str,
    indented: &str,
) -> Result<String, ConfigWriteError> {
    // Confirm the section key is present (clean error if malformed/missing).
    let doc = document(base)?;
    let route = yamlpath::Route::default().with_key(section);
    doc.query_pretty(&route)
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;

    let (after_colon, header_line_end) = header_bounds(base, section)?;
    let mut out = base.to_string();
    out.replace_range(after_colon..header_line_end, &format!("\n{indented}"));
    Ok(out)
}

/// Append a brand-new `<section>:` header (with the given entry already
/// nested under it) to the end of the document. Used when the section is
/// entirely absent from the config.
fn insert_new_section(base: &str, section: &str, indented: &str) -> String {
    let mut out = base.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(section);
    out.push_str(":\n");
    out.push_str(indented);
    out
}

/// Remove `devices.<key>` entirely. Returns [`ConfigWriteError::NotFound`] if
/// the device does not exist.
pub fn remove_device(yaml: &str, key: &str) -> Result<String, ConfigWriteError> {
    remove_from_section(yaml, "devices", "device", key)
}

/// Add a new device or replace an existing one with `block`.
pub fn upsert_device(
    yaml: &str,
    key: &str,
    block: &serde_yaml::Mapping,
) -> Result<String, ConfigWriteError> {
    upsert_in_section(yaml, "devices", "device", key, block)
}

/// Remove `packages.<handle>` entirely. Returns [`ConfigWriteError::NotFound`]
/// if the package does not exist.
pub fn remove_package(yaml: &str, handle: &str) -> Result<String, ConfigWriteError> {
    remove_from_section(yaml, "packages", "package", handle)
}

/// Add a new package or replace an existing one with `block`.
pub fn upsert_package(
    yaml: &str,
    handle: &str,
    block: &serde_yaml::Mapping,
) -> Result<String, ConfigWriteError> {
    upsert_in_section(yaml, "packages", "package", handle, block)
}

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
        assert_eq!(
            v["devices"]["CC:DD"]["screen"],
            serde_yaml::Value::from("hello")
        );
        assert!(out.contains("# top comment"));
        // Trailing top-level comment must survive AND stay after the device
        // content (not relocated into the middle of the devices map).
        assert!(out.contains("# trailing comment"));
        assert!(
            out.find("# trailing comment").unwrap() > out.rfind("screen:").unwrap(),
            "trailing comment was relocated above device content:\n{out}"
        );
    }

    #[test]
    fn test_upsert_edits_existing_device() {
        let mut block = serde_yaml::Mapping::new();
        block.insert("screen".into(), "graytest".into());
        let out = upsert_device(SAMPLE, "AA:BB", &block).unwrap();
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            v["devices"]["AA:BB"]["screen"],
            serde_yaml::Value::from("graytest")
        );
        assert!(out.contains("# top comment"));
        // Trailing top-level comment must survive AND stay after the device
        // content (not relocated into the middle of the devices map).
        assert!(out.contains("# trailing comment"));
        assert!(
            out.find("# trailing comment").unwrap() > out.rfind("screen:").unwrap(),
            "trailing comment was relocated above device content:\n{out}"
        );
    }

    #[test]
    fn test_remove_one_of_several_devices_keeps_siblings_and_comments() {
        let multi = "\
# top comment
devices:
  \"AA:BB\":
    screen: transit
  \"CC:DD\":   # keep me
    screen: clock
# trailing comment
";
        let out = remove_device(multi, "AA:BB").unwrap();
        assert!(!out.contains("screen: transit"));
        assert!(out.contains("\"CC:DD\""));
        assert!(out.contains("screen: clock"));
        assert!(out.contains("# keep me"));
        assert!(out.contains("# top comment"));
        assert!(out.contains("# trailing comment"));
        // Still valid YAML with the sibling intact.
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert!(v["devices"]["AA:BB"].is_null());
        assert_eq!(
            v["devices"]["CC:DD"]["screen"],
            serde_yaml::Value::from("clock")
        );
    }

    #[test]
    fn test_remove_missing_device_errors() {
        assert!(matches!(
            remove_device(SAMPLE, "ZZ:ZZ"),
            Err(ConfigWriteError::NotFound(_))
        ));
    }

    #[test]
    fn test_upsert_into_empty_flow_mapping() {
        // `devices: {}` is the shipped default config format; ensure a device
        // can be added to it without producing invalid YAML.
        let yaml = "\
# top comment
registration:
  enabled: true
auth_mode: api_key
# Devices are owned by Home Assistant; none ship by default.
devices: {}
";
        let mut block = serde_yaml::Mapping::new();
        block.insert("screen".into(), "hello".into());
        let out = upsert_device(yaml, "CC:DD:EE:FF:00:11", &block).unwrap();
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            v["devices"]["CC:DD:EE:FF:00:11"]["screen"],
            serde_yaml::Value::from("hello")
        );
        // Comments must survive.
        assert!(out.contains("# top comment"));
        assert!(out.contains("# Devices are owned by Home Assistant"));
    }

    #[test]
    fn test_upsert_package_adds_and_updates() {
        let yaml = "auth_mode: api_key\n";
        let mut block = serde_yaml::Mapping::new();
        block.insert("repo".into(), "github.com/acme/x".into());
        block.insert("pin".into(), "v1.0.0".into());
        let out = upsert_package(yaml, "weather", &block).unwrap();
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            v["packages"]["weather"]["repo"],
            serde_yaml::Value::from("github.com/acme/x")
        );

        // update in place
        let mut b2 = serde_yaml::Mapping::new();
        b2.insert("repo".into(), "github.com/acme/x".into());
        b2.insert("pin".into(), "v2.0.0".into());
        let out2 = upsert_package(&out, "weather", &b2).unwrap();
        let v2: serde_yaml::Value = serde_yaml::from_str(&out2).unwrap();
        assert_eq!(
            v2["packages"]["weather"]["pin"],
            serde_yaml::Value::from("v2.0.0")
        );
    }

    #[test]
    fn test_remove_package() {
        let yaml = "packages:\n  weather:\n    repo: github.com/acme/x\n    pin: v1\n";
        let out = remove_package(yaml, "weather").unwrap();
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert!(v
            .get("packages")
            .map(|p| p.get("weather").is_none())
            .unwrap_or(true));
    }

    #[test]
    fn test_remove_missing_package_is_notfound() {
        assert!(matches!(
            remove_package("packages: {}\n", "nope"),
            Err(ConfigWriteError::NotFound(_))
        ));
    }

    #[test]
    fn test_upsert_device_with_params_submap_populated() {
        let yaml = "\
devices:
  \"AA:BB\":
    screen: transit
  \"CC:DD\":
    screen: clock
";
        let mut block = serde_yaml::Mapping::new();
        block.insert("screen".into(), "transit".into());
        let mut pm = serde_yaml::Mapping::new();
        pm.insert("station".into(), "Olten".into());
        block.insert("params".into(), serde_yaml::Value::Mapping(pm));
        let out = upsert_device(yaml, "AA:BB", &block).unwrap();
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            v["devices"]["AA:BB"]["params"]["station"],
            serde_yaml::Value::from("Olten"),
            "params sub-map mis-nested:\n{out}"
        );
    }
}
