//! Reads Home Assistant add-on options from `/data/options.json`.
//!
//! When byonk runs as an HA Supervisor add-on, Supervisor writes the Configuration
//! tab values to `/data/options.json`. This module reads two of them — `admin_token`
//! and `log_level` — and feeds them into byonk's existing mechanisms (the in-memory
//! `AppConfig.admin.token` and the tracing filter). It never writes the file, never
//! generates a token, and never logs one. Outside HA (file absent) it is a no-op.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::models::AppConfig;

/// Subset of the add-on options byonk consumes. Unknown keys are ignored.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AddonOptions {
    #[serde(default)]
    pub admin_token: Option<String>,
    #[serde(default)]
    pub log_level: Option<String>,
}

/// Outcome of attempting to read the options file.
#[derive(Debug)]
pub enum ReadResult {
    /// No options file present (normal for non-add-on runs).
    Missing,
    /// File present and parsed successfully.
    Parsed(AddonOptions),
    /// File present but unreadable or not valid JSON. Carries a human message.
    Malformed(String),
}

/// Default in-container path Supervisor writes options to.
const DEFAULT_OPTIONS_PATH: &str = "/data/options.json";

/// Resolve the options file path. `BYONK_OPTIONS_FILE` overrides the default
/// (used by tests and as an escape hatch).
pub fn options_path() -> PathBuf {
    std::env::var("BYONK_OPTIONS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_OPTIONS_PATH))
}

/// Read and parse the options file at `path`. Never panics.
pub fn read_options(path: &Path) -> ReadResult {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ReadResult::Missing,
        Err(e) => ReadResult::Malformed(format!("cannot read {}: {e}", path.display())),
        Ok(text) => match serde_json::from_str::<AddonOptions>(&text) {
            Ok(opts) => ReadResult::Parsed(opts),
            Err(e) => ReadResult::Malformed(format!("invalid JSON in {}: {e}", path.display())),
        },
    }
}

/// Map a log-level word to byonk's tracing filter string, or `None` if unknown.
fn level_to_filter(level: &str) -> Option<String> {
    let l = level.trim().to_ascii_lowercase();
    match l.as_str() {
        "trace" | "debug" | "info" | "warn" | "error" => Some(format!("byonk={l},tower_http={l}")),
        _ => None,
    }
}

/// Tracing filter implied by the options, if a valid `log_level` is present.
/// `None` when there is no options file, no `log_level`, or an unknown level.
pub fn log_filter(result: &ReadResult) -> Option<String> {
    match result {
        ReadResult::Parsed(opts) => opts.log_level.as_deref().and_then(level_to_filter),
        _ => None,
    }
}

/// Apply add-on options to the in-memory config. Only a non-empty `admin_token`
/// has an effect — it sets `config.admin.token`. byonk never persists this.
pub fn apply_to_config(result: &ReadResult, config: &mut AppConfig) {
    if let ReadResult::Parsed(opts) = result {
        if let Some(token) = opts.admin_token.as_deref() {
            let token = token.trim();
            if !token.is_empty() {
                config.admin.token = Some(token.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetLoader;
    use crate::models::AppConfig;

    fn embedded_config() -> AppConfig {
        let loader = AssetLoader::new(None, None, None);
        AppConfig::load_from_assets(&loader).expect("load embedded config")
    }

    #[test]
    fn missing_file_is_missing() {
        let result = read_options(std::path::Path::new("/no/such/options.json"));
        assert!(matches!(result, ReadResult::Missing));
    }

    #[test]
    fn valid_json_parses_both_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("options.json");
        std::fs::write(&path, r#"{"admin_token":"secret","log_level":"info"}"#).unwrap();
        match read_options(&path) {
            ReadResult::Parsed(opts) => {
                assert_eq!(opts.admin_token.as_deref(), Some("secret"));
                assert_eq!(opts.log_level.as_deref(), Some("info"));
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("options.json");
        std::fs::write(&path, r#"{"port":3000,"log_level":"warn"}"#).unwrap();
        match read_options(&path) {
            ReadResult::Parsed(opts) => {
                assert_eq!(opts.admin_token, None);
                assert_eq!(opts.log_level.as_deref(), Some("warn"));
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_is_malformed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("options.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(matches!(read_options(&path), ReadResult::Malformed(_)));
    }

    #[test]
    fn log_filter_maps_known_level() {
        let r = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: Some("info".to_string()),
        });
        assert_eq!(
            log_filter(&r).as_deref(),
            Some("byonk=info,tower_http=info")
        );
    }

    #[test]
    fn log_filter_none_for_unknown_level_or_missing() {
        let unknown = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: Some("verbose".to_string()),
        });
        assert_eq!(log_filter(&unknown), None);
        assert_eq!(log_filter(&ReadResult::Missing), None);
    }

    #[test]
    fn apply_sets_token_when_present() {
        let r = ReadResult::Parsed(AddonOptions {
            admin_token: Some("secret".to_string()),
            log_level: None,
        });
        let mut config = embedded_config();
        config.admin.token = None;
        apply_to_config(&r, &mut config);
        assert_eq!(config.admin.token.as_deref(), Some("secret"));
    }

    #[test]
    fn apply_ignores_blank_token_and_missing() {
        let mut config = embedded_config();
        config.admin.token = Some("keep".to_string());

        // Blank/whitespace token must not clobber an existing value.
        let blank = ReadResult::Parsed(AddonOptions {
            admin_token: Some("   ".to_string()),
            log_level: None,
        });
        apply_to_config(&blank, &mut config);
        assert_eq!(config.admin.token.as_deref(), Some("keep"));

        // Missing options file leaves config untouched.
        apply_to_config(&ReadResult::Missing, &mut config);
        assert_eq!(config.admin.token.as_deref(), Some("keep"));
    }
}
