//! Shortening of embedded `data:` URIs in MCP render responses.
//!
//! A screen that embeds an image with `image_process` carries the whole
//! picture as a base64 `data:` URI. That URI shows up in three places in a
//! `render_screen` response — the script's `data` table (serialized twice,
//! once as text and once as structured content) and, when asked for, the
//! expanded SVG. At 800x480 that is hundreds of kilobytes of an LLM client's
//! context spent on a string it cannot read anyway.
//!
//! What an author actually needs to know is that a data URI is *there*, what
//! type it is, and how big it is — not its payload. So the default is to
//! replace the payload with a marker naming both.

use std::sync::LazyLock;

use regex::Regex;
use rmcp::schemars;
use serde::Deserialize;

/// How `render_screen` should treat embedded `data:` URIs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DataUriMode {
    /// Replace the base64 payload with a marker naming its media type and
    /// length. The default: it keeps the response readable without hiding
    /// that an image is embedded.
    #[default]
    Shorten,
    /// Leave data URIs exactly as the script produced them.
    Full,
    /// Replace the whole URI, media type included, with a length marker.
    Omit,
}

/// Matches a `data:` URI's payload: the media-type/parameters run up to the
/// comma, then the payload itself.
///
/// The payload stops at the first character that cannot appear in a URI of
/// this shape. Base64 is `[A-Za-z0-9+/=]`; percent-encoded text data URIs add
/// `%` and friends. Stopping at quote, angle bracket, whitespace, backslash
/// and closing paren covers every context one of these is embedded in —
/// a JSON string, an SVG attribute, a CSS `url(...)` — without needing to
/// know which one we are looking at.
static DATA_URI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"data:([a-zA-Z0-9.+/-]*(?:;[a-zA-Z0-9.+=-]*)*),([^"'<>\s\\)]{64,})"#)
        .expect("static data-uri regex must compile")
});

/// Rewrite every `data:` URI in `s` according to `mode`.
///
/// Only payloads of at least 64 characters are touched: a short inline SVG or
/// a tiny placeholder pixel costs nothing to keep, and mangling it would lose
/// information for no saving.
pub fn shorten_in_text(s: &str, mode: DataUriMode) -> String {
    if mode == DataUriMode::Full {
        return s.to_string();
    }
    DATA_URI
        .replace_all(s, |c: &regex::Captures| {
            let media = &c[1];
            let payload_len = c[2].len();
            match mode {
                DataUriMode::Omit => format!("<data uri omitted, {payload_len} chars>"),
                _ => format!("data:{media},<{payload_len} chars elided>"),
            }
        })
        .into_owned()
}

/// Apply `shorten_in_text` to every string in a JSON value, in place.
///
/// Walks arrays and object values recursively; object *keys* are left alone,
/// since a data URI is never a key and rewriting one would silently change
/// the shape of the table the script returned.
pub fn shorten_in_json(v: &mut serde_json::Value, mode: DataUriMode) {
    if mode == DataUriMode::Full {
        return;
    }
    match v {
        serde_json::Value::String(s) => {
            let shortened = shorten_in_text(s, mode);
            if shortened != *s {
                *s = shortened;
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                shorten_in_json(item, mode);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, val) in map.iter_mut() {
                shorten_in_json(val, mode);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 80 base64 chars — over the 64-char floor.
    fn long_payload() -> String {
        "iVBORw0KGgoAAAANSUhEUg".repeat(4)
    }

    #[test]
    fn shorten_replaces_the_payload_but_keeps_the_media_type() {
        let uri = format!("data:image/png;base64,{}", long_payload());
        let out = shorten_in_text(&uri, DataUriMode::Shorten);
        assert!(
            out.starts_with("data:image/png;base64,"),
            "the media type must survive so the author still knows what is embedded: {out}"
        );
        assert!(
            !out.contains("iVBORw0KGgo"),
            "the payload must be gone: {out}"
        );
        assert!(
            out.contains(&format!("{} chars", long_payload().len())),
            "the marker must report the original length: {out}"
        );
    }

    #[test]
    fn omit_drops_the_media_type_too() {
        let uri = format!("data:image/png;base64,{}", long_payload());
        let out = shorten_in_text(&uri, DataUriMode::Omit);
        assert!(!out.contains("image/png"), "{out}");
        assert!(!out.contains("iVBORw0KGgo"), "{out}");
        assert!(out.contains("omitted"), "{out}");
    }

    #[test]
    fn full_is_byte_identical() {
        let uri = format!("data:image/png;base64,{}", long_payload());
        assert_eq!(shorten_in_text(&uri, DataUriMode::Full), uri);
    }

    #[test]
    fn short_payloads_are_left_alone() {
        // Under the 64-char floor: keeping it costs nothing and mangling it
        // would lose a value the author might actually want to read.
        let uri = "data:image/png;base64,iVBORw0KGgo=";
        assert_eq!(shorten_in_text(uri, DataUriMode::Shorten), uri);
    }

    #[test]
    fn a_uri_inside_an_svg_attribute_stops_at_the_quote() {
        let svg = format!(
            r#"<image href="data:image/png;base64,{}" x="0"/><rect/>"#,
            long_payload()
        );
        let out = shorten_in_text(&svg, DataUriMode::Shorten);
        assert!(
            out.ends_with(r#"" x="0"/><rect/>"#),
            "everything after the URI must survive intact: {out}"
        );
        assert!(!out.contains("iVBORw0KGgo"), "{out}");
    }

    #[test]
    fn several_uris_in_one_document_are_all_shortened() {
        let svg = format!(
            r#"<image href="data:image/png;base64,{p}"/><image href="data:image/jpeg;base64,{p}"/>"#,
            p = long_payload()
        );
        let out = shorten_in_text(&svg, DataUriMode::Shorten);
        assert_eq!(
            out.matches("chars elided").count(),
            2,
            "both URIs must be shortened: {out}"
        );
        assert!(out.contains("image/jpeg"), "{out}");
    }

    #[test]
    fn json_walk_reaches_nested_strings_but_not_keys() {
        let payload = long_payload();
        let mut v = serde_json::json!({
            "src": format!("data:image/png;base64,{payload}"),
            "nested": { "deep": [format!("data:image/png;base64,{payload}")] },
            "untouched": "hello",
        });
        shorten_in_json(&mut v, DataUriMode::Shorten);
        assert!(!v["src"].as_str().unwrap().contains("iVBORw0KGgo"));
        assert!(!v["nested"]["deep"][0]
            .as_str()
            .unwrap()
            .contains("iVBORw0KGgo"));
        assert_eq!(v["untouched"], "hello", "unrelated values must not change");
        assert!(
            v.get("src").is_some(),
            "keys must survive the walk untouched"
        );
    }

    #[test]
    fn json_full_mode_changes_nothing() {
        let payload = long_payload();
        let original = serde_json::json!({ "src": format!("data:image/png;base64,{payload}") });
        let mut v = original.clone();
        shorten_in_json(&mut v, DataUriMode::Full);
        assert_eq!(v, original);
    }
}
