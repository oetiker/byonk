//! Comment-preserving edits to `config.yaml`, built on `yamlpath`/`yamlpatch`.
//!
//! Strategy (avoids yamlpatch's weak spots on sequences/flow lists):
//! - global scalar settings → in-place scalar replace
//! - device add/edit/remove → remove the device subtree + append a freshly
//!   block-serialized subtree (device blocks are machine-managed, so no user
//!   comments live inside them).

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

/// Replace a scalar value at `path` (e.g. `["registration","enabled"]`),
/// preserving all surrounding comments and formatting.
pub fn set_scalar(
    yaml: &str,
    path: &[&str],
    value: serde_yaml::Value,
) -> Result<String, ConfigWriteError> {
    let doc = document(yaml)?;

    let mut route = yamlpath::Route::default();
    for key in path {
        route = route.with_key(*key);
    }

    let patch = Patch {
        route,
        operation: Op::Replace(to_patch_value(&value)?),
    };

    let new_doc =
        apply_yaml_patches(&doc, &[patch]).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
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

/// Compute the byte range covering `devices.<key>` *and only* its own block,
/// starting at the device key line and extending over the deeper-indented body
/// lines that belong to it.
///
/// This is done manually instead of via `yamlpatch::Op::Remove` because
/// tree-sitter attaches any following less-indented comment (e.g. a trailing
/// top-level `# comment`) to the last block node, which would make `Op::Remove`
/// delete that comment too. Scanning by indentation keeps such comments intact.
fn device_block_range(yaml: &str, key: &str) -> Result<(usize, usize), ConfigWriteError> {
    let doc = document(yaml)?;
    let route = yamlpath::Route::default().with_key("devices").with_key(key);
    let feature = doc
        .query_pretty(&route)
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let key_byte = feature.location.byte_span.0;

    // Start of the device key line.
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

/// Remove `devices.<key>` entirely. Returns [`ConfigWriteError::NotFound`] if
/// the device does not exist.
pub fn remove_device(yaml: &str, key: &str) -> Result<String, ConfigWriteError> {
    // Confirm presence first for a clean NotFound error.
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let exists = parsed.get("devices").and_then(|d| d.get(key)).is_some();
    if !exists {
        return Err(ConfigWriteError::NotFound(format!("device {key}")));
    }

    let (start, end) = device_block_range(yaml, key)?;
    let mut out = yaml.to_string();
    out.replace_range(start..end, "");
    Ok(out)
}

/// Add a new device or replace an existing one with `block`.
///
/// Editing is implemented as remove-then-add so it is robust against odd
/// existing layouts (flow lists / sequence params) inside the old block.
pub fn upsert_device(
    yaml: &str,
    key: &str,
    block: &serde_yaml::Mapping,
) -> Result<String, ConfigWriteError> {
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let exists = parsed.get("devices").and_then(|d| d.get(key)).is_some();

    // Edit = remove then add (robust against sequence/flow-list params).
    let base = if exists {
        remove_device(yaml, key)?
    } else {
        yaml.to_string()
    };

    // If `devices` already holds at least one entry, `yamlpatch`'s block-mapping
    // addition handles indentation and trailing-comment placement correctly.
    let base_parsed: serde_yaml::Value =
        serde_yaml::from_str(&base).map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let devices_nonempty = base_parsed
        .get("devices")
        .and_then(|d| d.as_mapping())
        .map(|m| !m.is_empty())
        .unwrap_or(false);

    if devices_nonempty {
        let doc = document(&base)?;
        let route = yamlpath::Route::default().with_key("devices");
        let value = to_patch_value(&serde_yaml::Value::Mapping(block.clone()))?;
        let patch = Patch {
            route,
            operation: Op::Add {
                key: key.to_string(),
                value,
            },
        };
        let new_doc = apply_yaml_patches(&doc, &[patch])
            .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
        Ok(render(new_doc))
    } else {
        // `devices:` is empty/null (e.g. after editing the only device).
        // `yamlpatch` can't add into an empty mapping, so insert the block
        // manually right after the `devices:` header line, indented two spaces.
        insert_into_empty_devices(&base, key, block)
    }
}

/// Insert a freshly block-serialized device under an empty `devices:` header,
/// preserving everything else (including trailing comments).
fn insert_into_empty_devices(
    base: &str,
    key: &str,
    block: &serde_yaml::Mapping,
) -> Result<String, ConfigWriteError> {
    let doc = document(base)?;
    let route = yamlpath::Route::default().with_key("devices");
    let feature = doc
        .query_pretty(&route)
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let header_byte = feature.location.byte_span.0;
    let header_line_end = next_line_start(base, header_byte);

    // Serialize `{ key: block }` then indent every line by two spaces so the
    // device key sits one level under `devices:`.
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

    let mut out = base.to_string();
    out.insert_str(header_line_end, &indented);
    Ok(out)
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
}
