//! Comment-preserving edits to `config.yaml`, built on `yamlpath`/`yamlpatch`.
//!
//! Strategy (avoids yamlpatch's weak spots on sequences/flow lists):
//! - global scalar settings → in-place scalar replace
//! - device / screen repo add/edit/remove → remove the entry's subtree + append a
//!   freshly block-serialized subtree (entries are machine-managed, so no user
//!   comments live inside them). Both `devices:` and `screen_repos:` share the
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

/// Build a `yamlpath::Route` by chaining `with_key` over `path`.
fn route_for<'a>(path: &[&'a str]) -> yamlpath::Route<'a> {
    let mut route = yamlpath::Route::default();
    for key in path {
        route = route.with_key(*key);
    }
    route
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
    let route = route_for(path);

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
    let parent_route = route_for(&path[..path.len() - 1]);
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

/// Replace a scalar value at `path` that must already exist, preserving all
/// surrounding comments and formatting. Unlike [`set_scalar`], this never
/// falls back to adding a new key when `Replace` fails — it returns
/// [`ConfigWriteError::Patch`] instead.
///
/// Intended for callers whose contract is "rewrite an existing value", never
/// "create if missing" — e.g. `screen_migration`, where an `Add` fallback
/// could wrongly materialize a `screen:` key on a device that was inheriting
/// it via a YAML merge key (`<<:`), rather than leaving it alone.
pub fn replace_scalar(
    yaml: &str,
    path: &[&str],
    value: serde_yaml::Value,
) -> Result<String, ConfigWriteError> {
    assert!(!path.is_empty(), "replace_scalar: path must not be empty");

    let doc = document(yaml)?;
    let patch = Patch {
        route: route_for(path),
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

/// Byte range of `path`'s key line plus the body lines that belong to it.
///
/// Ownership rule: every following non-comment line indented deeper than
/// the key is body. Blank lines and comment lines (at any indentation, column
/// 0 included) belong to the entry only when more of its body follows them;
/// a run of them that is followed by a line at or left of the key's
/// indentation (or by the end of the file) belongs to what comes next, so a
/// comment above the next sibling stays with that sibling.
///
/// Scanned by indentation rather than with `Op::Remove`, because tree-sitter
/// attaches a following less-indented comment (e.g. a trailing top-level
/// `# comment`) to the last block node and would delete it too.
///
/// Only meaningful for a key that starts its own line: a key inside a flow
/// mapping shares its parent's line (see [`parent_is_inline`]).
fn path_range(yaml: &str, path: &[&str]) -> Result<(usize, usize), ConfigWriteError> {
    let doc = document(yaml)?;
    let feature = doc
        .query_pretty(&route_for(path))
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let key_byte = feature.location.byte_span.0;

    // Start of the key line.
    let line_start = yaml[..key_byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let key_indent = indent_of(&yaml[line_start..]);

    // `end` only advances past blank and comment lines once a deeper body
    // line after them proves they are inside.
    let mut end = next_line_start(yaml, line_start);
    let mut scan = end;
    while scan < yaml.len() {
        let line_end = next_line_start(yaml, scan);
        let line = &yaml[scan..line_end];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            scan = line_end;
            continue;
        }
        if indent_of(line) <= key_indent {
            break;
        }
        scan = line_end;
        end = line_end;
    }
    Ok((line_start, end))
}

/// The byte range of `<section>.<key>`'s own block; see [`path_range`].
fn block_range(yaml: &str, section: &str, key: &str) -> Result<(usize, usize), ConfigWriteError> {
    path_range(yaml, &[section, key])
}

fn parse(yaml: &str) -> Result<serde_yaml::Value, ConfigWriteError> {
    serde_yaml::from_str(yaml).map_err(|e| ConfigWriteError::Patch(e.to_string()))
}

fn lookup<'a>(doc: &'a serde_yaml::Value, path: &[&str]) -> Option<&'a serde_yaml::Value> {
    path.iter().try_fold(doc, |v, k| v.get(*k))
}

fn is_scalar(v: &serde_yaml::Value) -> bool {
    !matches!(
        v,
        serde_yaml::Value::Mapping(_) | serde_yaml::Value::Sequence(_)
    )
}

/// Does `path` carry its value on the key line (`dither: {}`, `x: 1`,
/// `[a, b]`) rather than as an indented block (or nothing, `dither:`)?
/// Asked of the parse tree, so a quoted key with a colon in it (`"AA:BB":`)
/// or a trailing comment is not mistaken for an inline value.
fn inline_value(yaml: &str, path: &[&str]) -> Result<bool, ConfigWriteError> {
    let doc = document(yaml)?;
    let value = doc
        .query_exact(&route_for(path))
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    Ok(match value {
        None => false,
        Some(v) => !matches!(
            v.kind(),
            yamlpath::FeatureKind::BlockMapping | yamlpath::FeatureKind::BlockSequence
        ),
    })
}

/// Is `path`'s parent a flow mapping (`og: {name: OG, …}`)? Then `path`'s key
/// shares the parent's line, and the parent is rewritten as a block instead
/// of editing the key's "own" lines, which it does not have.
fn parent_is_inline(yaml: &str, path: &[&str]) -> Result<bool, ConfigWriteError> {
    Ok(path.len() >= 2 && inline_value(yaml, &path[..path.len() - 1])?)
}

/// `{key: value}` as block YAML, every line indented by `indent` spaces.
/// Written as text because `yamlpatch`'s `Op::Add` mis-indents nested maps.
fn block_text(
    key: &str,
    value: &serde_yaml::Value,
    indent: usize,
) -> Result<String, ConfigWriteError> {
    let mut m = serde_yaml::Mapping::new();
    m.insert(key.into(), value.clone());
    let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(m))
        .map_err(|e| ConfigWriteError::Patch(e.to_string()))?;
    let pad = " ".repeat(indent);
    Ok(text.lines().map(|l| format!("{pad}{l}\n")).collect())
}

/// Remove `path` and its body lines. An absent path leaves the text
/// unchanged: removing what is not there is not an error for a whole-rung
/// write.
pub fn remove_path(yaml: &str, path: &[&str]) -> Result<String, ConfigWriteError> {
    let doc = parse(yaml)?;
    if lookup(&doc, path).is_none() {
        return Ok(yaml.to_string());
    }
    if parent_is_inline(yaml, path)? {
        let parent = &path[..path.len() - 1];
        let mut rest = lookup(&doc, parent)
            .and_then(|v| v.as_mapping().cloned())
            .unwrap_or_default();
        rest.remove(path[path.len() - 1]);
        return set_path(yaml, parent, &serde_yaml::Value::Mapping(rest));
    }
    let (start, end) = path_range(yaml, path)?;
    let mut out = yaml.to_string();
    out.replace_range(start..end, "");
    Ok(out)
}

/// Set `path` to `value`, rewriting only the lines that key owns.
///
/// - an equal value: nothing is written (quoting and comments stay);
/// - an existing scalar replaced by a scalar: in place, comment kept;
/// - an existing key otherwise: its lines are replaced by a block at the
///   same place and indentation;
/// - an existing key inside a flow mapping: the parent is rewritten as a
///   block holding the new value (the key has no lines of its own);
/// - an absent key: appended as the parent's last child, at the indentation
///   the parent's children already use (parent indent + 2 when it has none).
///   An inline parent (`dither: {}`) is first rewritten as a block.
///
/// The parent must exist ([`ConfigWriteError::NotFound`] otherwise); a
/// top-level key's parent is the document, which always does.
pub fn set_path(
    yaml: &str,
    path: &[&str],
    value: &serde_yaml::Value,
) -> Result<String, ConfigWriteError> {
    assert!(!path.is_empty(), "set_path: path must not be empty");
    let doc = parse(yaml)?;
    let (parent, key) = path.split_at(path.len() - 1);
    let key = key[0];

    if let Some(existing) = lookup(&doc, path) {
        if existing == value {
            return Ok(yaml.to_string());
        }
        if parent_is_inline(yaml, path)? {
            let mut merged = lookup(&doc, parent)
                .and_then(|v| v.as_mapping().cloned())
                .unwrap_or_default();
            merged.insert(key.into(), value.clone());
            return set_path(yaml, parent, &serde_yaml::Value::Mapping(merged));
        }
        if is_scalar(existing) && is_scalar(value) && !existing.is_null() {
            return replace_scalar(yaml, path, value.clone());
        }
        let (start, end) = path_range(yaml, path)?;
        let text = block_text(key, value, indent_of(&yaml[start..]))?;
        let mut out = yaml.to_string();
        out.replace_range(start..end, &text);
        return Ok(out);
    }

    if parent.is_empty() {
        // A new top-level key: the document's last line.
        let mut out = yaml.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&block_text(key, value, 0)?);
        return Ok(out);
    }
    let parent_value =
        lookup(&doc, parent).ok_or_else(|| ConfigWriteError::NotFound(parent.join(".")))?;
    if inline_value(yaml, parent)? {
        let mut merged = parent_value.as_mapping().cloned().unwrap_or_default();
        merged.insert(key.into(), value.clone());
        return set_path(yaml, parent, &serde_yaml::Value::Mapping(merged));
    }

    let (start, end) = path_range(yaml, parent)?;
    let parent_indent = indent_of(&yaml[start..]);
    let child_indent = yaml[next_line_start(yaml, start)..end]
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(indent_of)
        .unwrap_or(parent_indent + 2);
    let mut text = block_text(key, value, child_indent)?;
    if !yaml[..end].ends_with('\n') {
        text.insert(0, '\n');
    }
    let mut out = yaml.to_string();
    out.insert_str(end, &text);
    Ok(out)
}

/// Make the mapping at `path` equal `target`, key by key: keys the target
/// lacks are removed, changed values are set, nested mappings are synced
/// recursively, and equal values are left exactly as written. An absent,
/// null, inline or to-be-empty mapping is written whole by [`set_path`].
pub fn sync_mapping(
    yaml: &str,
    path: &[&str],
    target: &serde_yaml::Mapping,
) -> Result<String, ConfigWriteError> {
    let doc = parse(yaml)?;
    let existing = match lookup(&doc, path) {
        Some(serde_yaml::Value::Mapping(m)) if !target.is_empty() && !inline_value(yaml, path)? => {
            m.clone()
        }
        _ => return set_path(yaml, path, &serde_yaml::Value::Mapping(target.clone())),
    };
    let key_of = |k: &serde_yaml::Value| -> Result<String, ConfigWriteError> {
        k.as_str()
            .map(str::to_string)
            .ok_or_else(|| ConfigWriteError::Patch(format!("non-string key {k:?}")))
    };
    let mut out = yaml.to_string();
    // Removals first, so an insert never lands next to a line about to go.
    for k in existing.keys() {
        if !target.contains_key(k) {
            let k = key_of(k)?;
            let child: Vec<&str> = path.iter().copied().chain([k.as_str()]).collect();
            out = remove_path(&out, &child)?;
        }
    }
    for (k, v) in target {
        let ks = key_of(k)?;
        let child: Vec<&str> = path.iter().copied().chain([ks.as_str()]).collect();
        match (existing.get(k), v) {
            (Some(old), new) if old == new => {}
            (Some(serde_yaml::Value::Mapping(_)), serde_yaml::Value::Mapping(m)) => {
                out = sync_mapping(&out, &child, m)?;
            }
            (_, new) => out = set_path(&out, &child, new)?,
        }
    }
    Ok(out)
}

/// Remove `<section>.<key>` entirely. Returns [`ConfigWriteError::NotFound`] if
/// the entry does not exist. `label` is the singular noun used in the
/// [`ConfigWriteError::NotFound`] message (e.g. `"device"`, `"screen repo"`).
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
/// (e.g. `devices` or `screen repos`). `label` is the singular noun used in the
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

/// Remove `screen_repos.<handle>` entirely. Returns [`ConfigWriteError::NotFound`]
/// if the screen repo does not exist.
pub fn remove_screen_repo(yaml: &str, handle: &str) -> Result<String, ConfigWriteError> {
    remove_from_section(yaml, "screen_repos", "screen repo", handle)
}

/// Add a new screen repo or replace an existing one with `block`.
pub fn upsert_screen_repo(
    yaml: &str,
    handle: &str,
    block: &serde_yaml::Mapping,
) -> Result<String, ConfigWriteError> {
    upsert_in_section(yaml, "screen_repos", "screen repo", handle, block)
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
    fn test_replace_scalar_preserves_comments() {
        let out = replace_scalar(SAMPLE, &["registration", "enabled"], false.into()).unwrap();
        assert!(out.contains("# top comment"));
        assert!(out.contains("# trailing comment"));
        assert!(out.contains("enabled: false"));
    }

    #[test]
    fn test_replace_scalar_nested_device_field() {
        let out = replace_scalar(SAMPLE, &["devices", "AA:BB", "screen"], "clock".into()).unwrap();
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            v["devices"]["AA:BB"]["screen"],
            serde_yaml::Value::from("clock")
        );
        // The sibling `params` sub-map and comments must survive untouched.
        assert_eq!(
            v["devices"]["AA:BB"]["params"]["station"],
            serde_yaml::Value::from("Olten")
        );
        assert!(out.contains("# top comment"));
        assert!(out.contains("# trailing comment"));
    }

    #[test]
    fn test_replace_scalar_missing_key_errors_never_adds() {
        // Unlike `set_scalar`, a missing key must error, not be materialized —
        // contrast with `set_scalar` on the identical input/path, which does add it.
        let err = replace_scalar(SAMPLE, &["registration", "does_not_exist"], true.into())
            .expect_err("replace_scalar must not add a missing key");
        assert!(matches!(err, ConfigWriteError::Patch(_)));

        let added = set_scalar(SAMPLE, &["registration", "does_not_exist"], true.into()).unwrap();
        assert!(added.contains("does_not_exist"));
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
        let out = upsert_screen_repo(yaml, "weather", &block).unwrap();
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            v["screen_repos"]["weather"]["repo"],
            serde_yaml::Value::from("github.com/acme/x")
        );

        // update in place
        let mut b2 = serde_yaml::Mapping::new();
        b2.insert("repo".into(), "github.com/acme/x".into());
        b2.insert("pin".into(), "v2.0.0".into());
        let out2 = upsert_screen_repo(&out, "weather", &b2).unwrap();
        let v2: serde_yaml::Value = serde_yaml::from_str(&out2).unwrap();
        assert_eq!(
            v2["screen_repos"]["weather"]["pin"],
            serde_yaml::Value::from("v2.0.0")
        );
    }

    #[test]
    fn test_remove_package() {
        let yaml = "screen_repos:\n  weather:\n    repo: github.com/acme/x\n    pin: v1\n";
        let out = remove_screen_repo(yaml, "weather").unwrap();
        let v: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert!(v
            .get("screen_repos")
            .map(|p| p.get("weather").is_none())
            .unwrap_or(true));
    }

    #[test]
    fn test_remove_missing_package_is_notfound() {
        assert!(matches!(
            remove_screen_repo("screen_repos: {}\n", "nope"),
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

    const PANELS: &str = "\
panels:
    # first panel
    e1004:
        name: E1004   # inline
        colors: '#000000,#FFFFFF'
    og:
        name: OG
        dither:
            max_error: 0.5   # tuned by hand
            strength: 1.0
";

    #[test]
    fn set_path_inserts_a_nested_mapping_at_the_parents_child_indent() {
        let mut m = serde_yaml::Mapping::new();
        m.insert("strength".into(), 0.8.into());
        let out = set_path(
            PANELS,
            &["panels", "e1004", "dither"],
            &serde_yaml::Value::Mapping(m),
        )
        .unwrap();
        assert!(
            out.contains("        dither:\n          strength: 0.8\n"),
            "{out}"
        );
        assert!(
            out.contains("# first panel")
                && out.contains("# inline")
                && out.contains("# tuned by hand")
        );
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            doc["panels"]["e1004"]["dither"]["strength"].as_f64(),
            Some(0.8)
        );
        assert_eq!(doc["panels"]["e1004"]["name"], "E1004");
    }

    #[test]
    fn sync_mapping_touches_only_the_keys_that_change() {
        let mut target = serde_yaml::Mapping::new();
        target.insert("max_error".into(), 0.5.into()); // unchanged: its line and comment stay
        let mut gamut = serde_yaml::Mapping::new();
        gamut.insert("knee".into(), 0.6.into());
        // A new nested block.
        target.insert("gamut".into(), serde_yaml::Value::Mapping(gamut));
        // `strength` absent from the target: removed.
        let out = sync_mapping(PANELS, &["panels", "og", "dither"], &target).unwrap();
        assert!(out.contains("max_error: 0.5   # tuned by hand"), "{out}");
        assert!(!out.contains("strength"), "{out}");
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            doc["panels"]["og"]["dither"]["gamut"]["knee"].as_f64(),
            Some(0.6)
        );
        assert_eq!(doc["panels"]["og"]["name"], "OG");
    }

    #[test]
    fn set_path_rewrites_an_inline_flow_parent_as_a_block() {
        let yaml = "panels:\n  og:\n    name: OG\n    dither: {}\n";
        let out = set_path(yaml, &["panels", "og", "dither", "strength"], &0.8.into()).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            doc["panels"]["og"]["dither"]["strength"].as_f64(),
            Some(0.8)
        );
        assert_eq!(doc["panels"]["og"]["name"], "OG");
    }

    #[test]
    fn remove_path_of_an_absent_key_changes_nothing() {
        assert_eq!(
            remove_path(PANELS, &["panels", "e1004", "dither"]).unwrap(),
            PANELS
        );
    }

    #[test]
    fn set_path_replaces_a_scalar_in_place_keeping_its_comment() {
        let out = set_path(
            PANELS,
            &["panels", "og", "dither", "max_error"],
            &0.7.into(),
        )
        .unwrap();
        assert!(out.contains("max_error: 0.7   # tuned by hand"), "{out}");
        assert_eq!(
            out.replace("max_error: 0.7", "max_error: 0.5"),
            PANELS,
            "only the value changed"
        );
    }

    #[test]
    fn set_path_with_an_equal_value_leaves_the_text_alone() {
        let quoted = "panels:\n  og:\n    colors: \"#000000,#FFFFFF\"  # dq\n";
        let out = set_path(
            quoted,
            &["panels", "og", "colors"],
            &"#000000,#FFFFFF".into(),
        )
        .unwrap();
        assert_eq!(out, quoted);
    }

    #[test]
    fn set_path_appends_to_the_last_block_of_a_file_without_a_final_newline() {
        let yaml = "panels:\n  og:\n    name: OG\n    dither:\n      max_error: 0.5";
        let out = set_path(yaml, &["panels", "og", "dither", "strength"], &0.8.into()).unwrap();
        assert!(
            out.ends_with("      max_error: 0.5\n      strength: 0.8\n"),
            "{out}"
        );
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            doc["panels"]["og"]["dither"]["max_error"].as_f64(),
            Some(0.5)
        );
        assert_eq!(
            doc["panels"]["og"]["dither"]["strength"].as_f64(),
            Some(0.8)
        );
    }

    #[test]
    fn set_path_into_an_empty_block_parent_indents_one_level_deeper() {
        let yaml =
            "panels:\n  og:\n    name: OG\n    dither:   # nothing yet\n  next:\n    name: N\n";
        let out = set_path(yaml, &["panels", "og", "dither", "strength"], &0.8.into()).unwrap();
        assert_eq!(
            out,
            "panels:\n  og:\n    name: OG\n    dither:   # nothing yet\n      strength: 0.8\n  next:\n    name: N\n"
        );
    }

    /// A blank line inside a block is part of it: removing the block must not
    /// leave its tail behind, orphaned under the wrong parent.
    #[test]
    fn remove_path_takes_a_block_with_a_blank_line_inside_it_whole() {
        let yaml = "\
panels:
  og:
    name: OG
    dither:
      max_error: 0.5

      atkinson:
        noise_scale: 2.0

  next:   # sibling
    name: N
";
        let out = remove_path(yaml, &["panels", "og", "dither"]).unwrap();
        assert_eq!(
            out,
            "panels:\n  og:\n    name: OG\n\n  next:   # sibling\n    name: N\n"
        );
    }

    /// Appending a child lands after the block's last body line, not in the
    /// middle of it at a blank line.
    #[test]
    fn set_path_appends_after_a_blank_line_inside_the_parent() {
        let yaml = "\
panels:
  og:
    name: OG

    colors: '#000000,#FFFFFF'
  next:
    name: N
";
        let out = set_path(
            yaml,
            &["panels", "og", "colors_actual"],
            &"#111111,#EEEEEE".into(),
        )
        .unwrap();
        assert!(
            out.contains(
                "    colors: '#000000,#FFFFFF'\n    colors_actual: '#111111,#EEEEEE'\n  next:"
            ),
            "{out}"
        );
    }

    /// A colon inside a quoted key is not an inline value: the block under it
    /// must be extended in place, not rewritten (which would drop comments).
    #[test]
    fn set_path_under_a_quoted_key_with_a_colon_keeps_the_block() {
        let yaml = "\
devices:
  \"AA:BB\":
    screen: transit   # keep me
";
        let out = set_path(yaml, &["devices", "AA:BB", "panel"], &"og".into()).unwrap();
        assert_eq!(
            out,
            "devices:\n  \"AA:BB\":\n    screen: transit   # keep me\n    panel: og\n"
        );
    }

    #[test]
    fn set_path_under_an_inline_top_level_section_rewrites_it_as_a_block() {
        let yaml = "# head\npanels: {}   # none yet\nauth_mode: api_key\n";
        let out = set_path(yaml, &["panels", "og"], &"x".into()).unwrap();
        assert_eq!(out, "# head\npanels:\n  og: x\nauth_mode: api_key\n");
    }

    #[test]
    fn set_path_of_an_absent_top_level_key_appends_it() {
        let out = set_path("a: 1\n# tail\n", &["b"], &"x".into()).unwrap();
        assert_eq!(out, "a: 1\n# tail\nb: x\n");
    }

    #[test]
    fn set_path_with_a_missing_parent_is_not_found() {
        assert!(matches!(
            set_path(PANELS, &["panels", "nope", "dither"], &1.into()),
            Err(ConfigWriteError::NotFound(_))
        ));
    }

    const FLOW_PANEL: &str = "\
panels:
  og: {name: OG, colors: '#000000,#FFFFFF', colors_actual: '#111111,#EEEEEE'}
  e1004:   # sibling
    name: E
";

    fn panel(out: &str, id: &str) -> serde_yaml::Value {
        let doc: serde_yaml::Value = serde_yaml::from_str(out).unwrap();
        doc["panels"][id].clone()
    }

    /// A key inside a flow mapping shares its line with the parent: removing
    /// it must not remove the parent's line.
    #[test]
    fn remove_path_inside_a_flow_panel_keeps_the_panel() {
        let out = remove_path(FLOW_PANEL, &["panels", "og", "colors_actual"]).unwrap();
        let og = panel(&out, "og");
        assert_eq!(og["name"], "OG", "{out}");
        assert_eq!(og["colors"], "#000000,#FFFFFF", "{out}");
        assert!(og.get("colors_actual").is_none(), "{out}");
        assert!(out.contains("  e1004:   # sibling\n    name: E\n"), "{out}");
    }

    #[test]
    fn set_path_replaces_a_scalar_inside_a_flow_panel() {
        let out = set_path(
            FLOW_PANEL,
            &["panels", "og", "colors_actual"],
            &"#222222,#DDDDDD".into(),
        )
        .unwrap();
        let og = panel(&out, "og");
        assert_eq!(og["name"], "OG", "{out}");
        assert_eq!(og["colors"], "#000000,#FFFFFF", "{out}");
        assert_eq!(og["colors_actual"], "#222222,#DDDDDD", "{out}");
        assert!(out.contains("  e1004:   # sibling\n    name: E\n"), "{out}");
    }

    #[test]
    fn sync_mapping_adds_a_block_to_a_flow_panel() {
        let mut t = serde_yaml::Mapping::new();
        t.insert("strength".into(), 0.8.into());
        let out = sync_mapping(FLOW_PANEL, &["panels", "og", "dither"], &t).unwrap();
        let og = panel(&out, "og");
        assert_eq!(og["name"], "OG", "{out}");
        assert_eq!(og["colors_actual"], "#111111,#EEEEEE", "{out}");
        assert_eq!(og["dither"]["strength"].as_f64(), Some(0.8), "{out}");
        assert_eq!(panel(&out, "e1004")["name"], "E");
    }

    #[test]
    fn remove_path_inside_a_flow_dither_keeps_its_siblings() {
        let yaml = "panels:\n  og:\n    name: OG\n    dither: {max_error: 0.5, strength: 1.0}\n  next:\n    name: N\n";
        let out = remove_path(yaml, &["panels", "og", "dither", "strength"]).unwrap();
        let og = panel(&out, "og");
        assert_eq!(og["name"], "OG", "{out}");
        assert_eq!(og["dither"]["max_error"].as_f64(), Some(0.5), "{out}");
        assert!(og["dither"].get("strength").is_none(), "{out}");
    }

    /// A comment left of the block's indentation does not end the block when
    /// block content follows it: dropping `gamut` must take its tail along
    /// rather than re-nesting `max_compression` under `atkinson`.
    #[test]
    fn a_column_zero_comment_inside_a_block_does_not_end_it() {
        let yaml = "\
panels:
  og:
    name: OG
    dither:
      atkinson:
        noise_scale: 2.0
      gamut:
        knee: 0.6
#        amount: 0.2
        max_compression: 0.3
  next:
    name: N
";
        let mut atk = serde_yaml::Mapping::new();
        atk.insert("noise_scale".into(), 2.0.into());
        let mut t = serde_yaml::Mapping::new();
        t.insert("atkinson".into(), serde_yaml::Value::Mapping(atk));
        let out = sync_mapping(yaml, &["panels", "og", "dither"], &t).unwrap();
        assert_eq!(
            out,
            "panels:\n  og:\n    name: OG\n    dither:\n      atkinson:\n        noise_scale: 2.0\n  next:\n    name: N\n"
        );
        let removed = remove_path(yaml, &["panels", "og", "dither"]).unwrap();
        assert_eq!(
            removed,
            "panels:\n  og:\n    name: OG\n  next:\n    name: N\n"
        );
    }

    // Ownership rule for blank and comment lines after an entry's body
    // (`path_range`): they belong to the entry only when more of its body
    // follows them; otherwise they belong to whatever comes next.

    #[test]
    fn remove_device_takes_a_blank_line_inside_the_entry_along() {
        let yaml =
            "devices:\n  AA:\n    screen: x\n\n    params:\n      a: 1\n  BB:\n    screen: y\n";
        let out = remove_device(yaml, "AA").unwrap();
        assert_eq!(out, "devices:\n  BB:\n    screen: y\n");
    }

    #[test]
    fn remove_device_leaves_a_comment_after_a_blank_line_to_the_next_entry() {
        let yaml =
            "devices:\n  AA:\n    screen: x\n\n    # BB is the kitchen\n  BB:\n    screen: y\n";
        let out = remove_device(yaml, "AA").unwrap();
        assert_eq!(
            out,
            "devices:\n\n    # BB is the kitchen\n  BB:\n    screen: y\n"
        );
        let mut block = serde_yaml::Mapping::new();
        block.insert("screen".into(), "z".into());
        let up = upsert_device(yaml, "AA", &block).unwrap();
        assert!(up.contains("# BB is the kitchen\n  BB:"), "{up}");
    }

    #[test]
    fn remove_device_leaves_a_trailing_comment_without_body_after_it() {
        let yaml = "devices:\n  AA:\n    screen: x\n    # about BB\n  BB:\n    screen: y\n";
        let out = remove_device(yaml, "AA").unwrap();
        assert_eq!(out, "devices:\n    # about BB\n  BB:\n    screen: y\n");
    }

    #[test]
    fn sync_mapping_with_an_empty_target_leaves_an_empty_mapping_not_null() {
        let out = sync_mapping(
            PANELS,
            &["panels", "og", "dither"],
            &serde_yaml::Mapping::new(),
        )
        .unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(
            doc["panels"]["og"]["dither"],
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
            "{out}"
        );
    }

    #[test]
    fn sync_mapping_recurses_into_an_existing_nested_mapping() {
        let yaml = "\
og:
  dither:
    atkinson:   # per-algorithm
      noise_scale: 2.0   # by eye
      strength: 1.0
";
        let mut atk = serde_yaml::Mapping::new();
        atk.insert("noise_scale".into(), 2.0.into());
        atk.insert("max_error".into(), 0.4.into());
        let mut target = serde_yaml::Mapping::new();
        target.insert("atkinson".into(), serde_yaml::Value::Mapping(atk));
        let out = sync_mapping(yaml, &["og", "dither"], &target).unwrap();
        assert_eq!(
            out,
            "og:\n  dither:\n    atkinson:   # per-algorithm\n      noise_scale: 2.0   # by eye\n      max_error: 0.4\n"
        );
    }
}
