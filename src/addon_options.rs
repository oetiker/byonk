//! Reads Home Assistant add-on options from `/data/options.json`.
//!
//! When byonk runs as an HA Supervisor add-on, Supervisor writes the Configuration
//! tab values to `/data/options.json`. This module reads two of them — `admin_token`
//! and `log_level` — and feeds them into byonk's existing mechanisms (the in-memory
//! `AppConfig.admin.token` and the tracing filter). It never writes the file, never
//! generates a token, and never logs one. Outside HA (file absent) it is a no-op.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::models::config::ScreenRepoRef;
use crate::models::AppConfig;

/// One screen repo entry as it appears in the add-on options `screen_repos:` list.
/// The handle is a field here (HAOS list rows are flat objects); byonk stores
/// screen repos keyed by handle, so `apply_to_config` folds these into a map.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AddonScreenRepo {
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub pin: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}

/// Subset of the add-on options byonk consumes. Unknown keys are ignored.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AddonOptions {
    #[serde(default)]
    pub admin_token: Option<String>,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub auth_mode: Option<String>,
    #[serde(default)]
    pub screen_repo_refresh_interval: Option<u64>,
    #[serde(default)]
    pub screen_repos: Vec<AddonScreenRepo>,
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

/// The configured `log_level` string when it is present but not a recognized
/// level, so the caller can warn before falling back to the default filter.
/// `None` when the level is absent, blank, valid, or there is no options file.
pub fn invalid_log_level(result: &ReadResult) -> Option<String> {
    if let ReadResult::Parsed(opts) = result {
        if let Some(level) = opts.log_level.as_deref() {
            if !level.trim().is_empty() && level_to_filter(level).is_none() {
                return Some(level.to_string());
            }
        }
    }
    None
}

/// Apply add-on options to the in-memory config.
///
/// When an options file was successfully parsed (byonk is running as an HA
/// add-on), the `admin_token` option is **authoritative**: a non-empty value
/// sets `config.admin.token`; a blank or absent value **clears** it so the admin
/// API stays dormant — the add-on option is the single source of truth. An
/// explicit `BYONK_ADMIN_TOKEN` env var still wins (resolved before
/// `config.admin.token` in `server.rs`). When there is no options file
/// (`Missing`) or it could not be parsed (`Malformed`), the config is left
/// untouched so non-add-on runs keep their Phase 1 behavior. byonk never
/// persists the token.
pub fn apply_to_config(result: &ReadResult, config: &mut AppConfig) {
    if let ReadResult::Parsed(opts) = result {
        // admin_token stays authoritative (non-empty sets, blank/absent clears).
        config.admin.token = opts
            .admin_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string);

        // The remaining global settings override config only when present in
        // options.json; absent keys leave the config value untouched.
        if let Some(mode) = opts.auth_mode.as_deref() {
            let mode = mode.trim();
            if !mode.is_empty() {
                config.auth_mode = mode.to_string();
            }
        }
        if let Some(interval) = opts.screen_repo_refresh_interval {
            config.screen_repo_refresh_interval = interval;
        }

        // In add-on mode the screen repo registry is taken authoritatively from
        // options.json: it always replaces config.screen_repos, so an empty list
        // clears any pre-existing registry.
        config.screen_repos = opts
            .screen_repos
            .iter()
            .filter(|p| !p.handle.trim().is_empty())
            .map(|p| {
                (
                    p.handle.trim().to_string(),
                    ScreenRepoRef {
                        repo: p.repo.clone(),
                        pin: p.pin.clone(),
                        token: p.token.clone(),
                    },
                )
            })
            .collect();
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
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
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
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
        });
        assert_eq!(log_filter(&unknown), None);
        assert_eq!(log_filter(&ReadResult::Missing), None);
    }

    #[test]
    fn apply_sets_token_when_present() {
        let r = ReadResult::Parsed(AddonOptions {
            admin_token: Some("secret".to_string()),
            log_level: None,
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
        });
        let mut config = embedded_config();
        config.admin.token = None;
        apply_to_config(&r, &mut config);
        assert_eq!(config.admin.token.as_deref(), Some("secret"));
    }

    #[test]
    fn apply_blank_or_absent_token_clears_existing() {
        // In add-on mode (Parsed) the option is authoritative: a blank or absent
        // admin_token must clear any pre-existing config token so the admin API
        // stays dormant — the add-on option is the single source of truth.
        let blank = ReadResult::Parsed(AddonOptions {
            admin_token: Some("   ".to_string()),
            log_level: None,
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
        });
        let mut config = embedded_config();
        config.admin.token = Some("stale".to_string());
        apply_to_config(&blank, &mut config);
        assert_eq!(
            config.admin.token, None,
            "blank option must clear the token"
        );

        let absent = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: Some("info".to_string()),
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
        });
        let mut config = embedded_config();
        config.admin.token = Some("stale".to_string());
        apply_to_config(&absent, &mut config);
        assert_eq!(
            config.admin.token, None,
            "absent option must clear the token"
        );
    }

    #[test]
    fn apply_missing_or_malformed_leaves_token_untouched() {
        // Non-add-on runs (no options file) and unreadable files must keep the
        // Phase 1 config token rather than disabling the admin API.
        let mut config = embedded_config();
        config.admin.token = Some("keep".to_string());

        apply_to_config(&ReadResult::Missing, &mut config);
        assert_eq!(config.admin.token.as_deref(), Some("keep"));

        apply_to_config(&ReadResult::Malformed("bad json".to_string()), &mut config);
        assert_eq!(config.admin.token.as_deref(), Some("keep"));
    }

    #[test]
    fn invalid_log_level_reports_only_present_unknown() {
        let unknown = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: Some("verbose".to_string()),
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
        });
        assert_eq!(invalid_log_level(&unknown).as_deref(), Some("verbose"));

        let valid = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: Some("info".to_string()),
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
        });
        assert_eq!(invalid_log_level(&valid), None);

        let blank = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: Some("  ".to_string()),
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
        });
        assert_eq!(invalid_log_level(&blank), None);

        let absent = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: None,
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
        });
        assert_eq!(invalid_log_level(&absent), None);
        assert_eq!(invalid_log_level(&ReadResult::Missing), None);
    }

    #[test]
    fn parses_settings_and_packages_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("options.json");
        std::fs::write(
            &path,
            r#"{
                "admin_token":"secret",
                "auth_mode":"ed25519",
                "screen_repo_refresh_interval":900,
                "screen_repos":[
                    {"handle":"disttest","repo":"https://example.com/x.git","pin":"main","token":"gh_x"},
                    {"handle":"nopin","repo":"https://example.com/y.git"}
                ]
            }"#,
        )
        .unwrap();
        match read_options(&path) {
            ReadResult::Parsed(opts) => {
                assert_eq!(opts.auth_mode.as_deref(), Some("ed25519"));
                assert_eq!(opts.screen_repo_refresh_interval, Some(900));
                assert_eq!(opts.screen_repos.len(), 2);
                assert_eq!(opts.screen_repos[0].handle, "disttest");
                assert_eq!(opts.screen_repos[0].pin.as_deref(), Some("main"));
                assert_eq!(opts.screen_repos[1].pin, None);
                assert_eq!(opts.screen_repos[1].token, None);
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    fn apply_overrides_settings_and_builds_package_map() {
        let r = ReadResult::Parsed(AddonOptions {
            admin_token: Some("t".to_string()),
            log_level: None,
            auth_mode: Some("ed25519".to_string()),
            screen_repo_refresh_interval: Some(600),
            screen_repos: vec![AddonScreenRepo {
                handle: "disttest".to_string(),
                repo: Some("https://example.com/x.git".to_string()),
                pin: Some("main".to_string()),
                token: Some("gh_x".to_string()),
            }],
        });
        let mut config = embedded_config();
        apply_to_config(&r, &mut config);
        assert_eq!(config.auth_mode, "ed25519");
        assert_eq!(config.screen_repo_refresh_interval, 600);
        let pkg = config
            .screen_repos
            .get("disttest")
            .expect("screen repo present");
        assert_eq!(pkg.repo.as_deref(), Some("https://example.com/x.git"));
        assert_eq!(pkg.pin.as_deref(), Some("main"));
        assert_eq!(pkg.token.as_deref(), Some("gh_x"));
    }

    #[test]
    fn apply_preserves_absent_settings_but_clears_packages() {
        // A parsed options file that omits the new keys must not clobber config
        // defaults (only admin_token and screen repos are authoritative-on-absence
        // / authoritative-replace; auth_mode and interval preserve-on-absent).
        let r = ReadResult::Parsed(AddonOptions {
            admin_token: None,
            log_level: None,
            auth_mode: None,
            screen_repo_refresh_interval: None,
            screen_repos: vec![],
        });
        let mut config = embedded_config();
        config.auth_mode = "api_key".to_string();
        config.screen_repo_refresh_interval = 42;
        config.screen_repos.insert(
            "stale".to_string(),
            ScreenRepoRef {
                repo: Some("https://example.com/stale.git".to_string()),
                pin: None,
                token: None,
            },
        );
        apply_to_config(&r, &mut config);
        assert_eq!(
            config.auth_mode, "api_key",
            "absent auth_mode keeps config value"
        );
        assert_eq!(
            config.screen_repo_refresh_interval, 42,
            "absent interval keeps config value"
        );
        assert!(
            config.screen_repos.is_empty(),
            "empty screen repos list in Parsed options must clear a pre-existing registry"
        );
    }
}
