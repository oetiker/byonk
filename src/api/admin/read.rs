//! Admin read endpoints.

use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;

use crate::error::ApiError;
use crate::models::compat::{compat_warning, engine_version};
use crate::models::config::{AppConfig, RESERVED_DEFAULT_KEY};
use crate::models::param_schema::ParamField;
use crate::server::AppState;
use crate::services::git_fetch::PinKind;
use crate::services::screen_repo_status::{ScreenRepoState, ScreenRepoStatus};
use crate::services::DeviceRegistry;

use super::require_admin;

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
    let mut value: serde_yaml::Value = serde_yaml::from_str(&text)
        .map_err(|e| ApiError::Internal(format!("parse config: {e}")))?;

    // Strip admin.token from the response.
    if let Some(map) = value.as_mapping_mut() {
        if let Some(admin) = map
            .get_mut(serde_yaml::Value::from("admin"))
            .and_then(|a| a.as_mapping_mut())
        {
            admin.remove(serde_yaml::Value::from("token"));
        }
        // Strip screen repo tokens — `ScreenRepoRef.token` is documented as
        // "Secret token; redacted in read APIs".
        if let Some(screen_repos) = map
            .get_mut(serde_yaml::Value::from("screen_repos"))
            .and_then(|p| p.as_mapping_mut())
        {
            for (_, pkg) in screen_repos.iter_mut() {
                if let Some(pkg_map) = pkg.as_mapping_mut() {
                    pkg_map.remove(serde_yaml::Value::from("token"));
                }
            }
        }
    }

    let json =
        serde_json::to_value(&value).map_err(|e| ApiError::Internal(format!("to json: {e}")))?;
    Ok(Json(json))
}

#[derive(Serialize)]
pub struct AdminDevice {
    /// Config key (MAC or registration code) if configured, else the MAC.
    pub key: String,
    pub mac: String,
    pub registration_code: String,
    pub registered: bool,
    /// `true` for the reserved DEFAULT device (byonk-managed fallback, not a
    /// physical device). The integration presents it specially.
    pub reserved: bool,
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
    pub refresh: Option<u32>,
    pub name: Option<String>,
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
        let reserved = mac == RESERVED_DEFAULT_KEY;
        out.push(AdminDevice {
            key: mac.clone(),
            mac,
            registration_code: code,
            registered,
            reserved,
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
            refresh: dc.and_then(|c| c.refresh),
            name: dc.and_then(|c| c.name.clone()),
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
            reserved: key == RESERVED_DEFAULT_KEY,
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
            refresh: dc.refresh,
            name: dc.name.clone(),
        });
    }

    Ok(Json(out))
}

#[derive(Serialize)]
pub struct ScreenInfo {
    pub r#ref: String,
    pub title: String,
    pub description: String,
    pub params: Vec<ParamField>,
    pub byonk: String,
    pub compat_warning: Option<String>,
}

#[derive(Serialize)]
pub struct ScreenRepoScreens {
    pub handle: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub screens: Vec<ScreenInfo>,
}

#[derive(Serialize)]
pub struct PanelInfo {
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub colors: String,
}

#[derive(Serialize)]
pub struct ScreensResponse {
    pub screen_repos: Vec<ScreenRepoScreens>,
    pub panels: Vec<PanelInfo>,
    pub dither_algorithms: Vec<String>,
}

/// One entry in the screen repo registry listing (`GET /screen-repos`).
#[derive(Serialize)]
pub struct ScreenRepoInfo {
    pub handle: String,
    pub repo: Option<String>,
    pub pin: Option<String>,
    /// `true` for the embedded builtin or any screen repo without a remote repo.
    pub builtin: bool,
    /// Whether an auth token is configured — the token itself is never serialized.
    pub token_set: bool,
    pub screen_count: usize,
    /// `ready` | `fetching` | `error` | `offline`. Never-fetched non-builtin
    /// handles report `error` — they are not currently serving.
    pub status: String,
    /// How `pin` was resolved (`sha`/`tag`/`branch`), or `embedded` for the
    /// builtin screen repo. `None` if never successfully fetched.
    pub pin_kind: Option<PinKind>,
    /// The commit sha the screen repo is currently pinned/fetched at.
    pub resolved_sha: Option<String>,
    /// RFC3339 timestamp of the last successful fetch.
    pub last_fetched: Option<String>,
    /// The last fetch error, if any.
    pub error: Option<String>,
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

    // Group every resolved screen by its screen repo handle. The screen-repo-level
    // metadata (name/description/author/license) comes from that screen repo's
    // manifest, read off the first screen encountered for the handle.
    let engine = engine_version();
    let mut by_handle: std::collections::BTreeMap<String, ScreenRepoScreens> =
        std::collections::BTreeMap::new();

    for screen in state.screen_repo_manager.loader().list_all() {
        let entry = by_handle.entry(screen.handle.clone()).or_insert_with(|| {
            let m = screen.source.manifest();
            ScreenRepoScreens {
                handle: screen.handle.clone(),
                name: m.name.clone(),
                description: m.description.clone(),
                author: m.author.clone(),
                license: m.license.clone(),
                screens: Vec::new(),
            }
        });
        entry.screens.push(ScreenInfo {
            r#ref: format!("{}/{}", screen.handle, screen.path),
            title: screen.meta.title.clone(),
            description: screen.meta.description.clone(),
            params: screen.meta.params.fields.clone(),
            byonk: screen.meta.byonk.clone(),
            compat_warning: compat_warning(engine, &screen.meta.byonk),
        });
    }

    // Deterministic order: screens within each screen repo sorted by ref.
    let screen_repos: Vec<ScreenRepoScreens> = by_handle
        .into_values()
        .map(|mut p| {
            p.screens.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
            p
        })
        .collect();

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
        screen_repos,
        panels,
        dither_algorithms: DITHER_ALGORITHMS.iter().map(|s| s.to_string()).collect(),
    }))
}

/// Build the `ScreenRepoInfo` for a single handle from the live config and its
/// fetch status (from `ScreenRepoManager::status_snapshot()`).
///
/// Shared by `screen_repos()` (the read listing) and the screen repo write handlers
/// (`add`/`patch`/`update`), so both surfaces stay in sync.
///
/// Status mapping:
/// - builtin: always `ready` / `pin_kind: embedded`, no sha/error/timestamp
///   (there is nothing to fetch).
/// - non-builtin with a status entry: `status` mirrors the entry's `state`
///   (defaulting to `"error"` if `state` is `None`); `pin_kind`/
///   `resolved_sha`/`error` copied through; `last_fetched` rendered RFC3339.
/// - non-builtin with no status entry (never fetched — e.g. right after
///   registration, before the first refresh runs): `status: "error"`, all
///   other fields `None`. It is not currently serving, so `"error"` is the
///   honest default.
pub(crate) fn build_screen_repo_info(
    config: &AppConfig,
    status: Option<&ScreenRepoStatus>,
    screen_count: usize,
    handle: String,
) -> ScreenRepoInfo {
    let pkg = config.screen_repos.get(&handle);
    let repo = pkg.and_then(|p| p.repo.clone());
    let builtin = handle == crate::services::screen_repo_loader::BUILTIN_HANDLE || repo.is_none();

    let (status_str, pin_kind, resolved_sha, last_fetched, error) = if builtin {
        (
            "ready".to_string(),
            Some(PinKind::Embedded),
            None,
            None,
            None,
        )
    } else {
        match status {
            Some(s) => {
                let state = s.state.unwrap_or(ScreenRepoState::Error);
                let status_str = match state {
                    ScreenRepoState::Ready => "ready",
                    ScreenRepoState::Fetching => "fetching",
                    ScreenRepoState::Error => "error",
                    ScreenRepoState::Offline => "offline",
                }
                .to_string();
                (
                    status_str,
                    s.pin_kind,
                    s.resolved_sha.clone(),
                    s.last_fetched.map(|dt| dt.to_rfc3339()),
                    s.error.clone(),
                )
            }
            None => ("error".to_string(), None, None, None, None),
        }
    };

    ScreenRepoInfo {
        repo,
        pin: pkg.and_then(|p| p.pin.clone()),
        builtin,
        token_set: pkg.map(|p| p.token.is_some()).unwrap_or(false),
        screen_count,
        status: status_str,
        pin_kind,
        resolved_sha,
        last_fetched,
        error,
        handle,
    }
}

#[cfg(test)]
mod build_package_info_tests {
    use super::*;
    use crate::models::config::ScreenRepoRef;

    #[test]
    fn builtin_handle_is_always_ready_and_embedded() {
        let config = AppConfig::default();
        let info = build_screen_repo_info(
            &config,
            None,
            3,
            crate::services::screen_repo_loader::BUILTIN_HANDLE.to_string(),
        );
        assert!(info.builtin);
        assert_eq!(info.status, "ready");
        assert_eq!(info.pin_kind, Some(PinKind::Embedded));
        assert_eq!(info.resolved_sha, None);
        assert_eq!(info.last_fetched, None);
        assert_eq!(info.error, None);
    }

    #[test]
    fn non_builtin_with_error_status_reports_it() {
        let mut config = AppConfig::default();
        config.screen_repos.insert(
            "weather".to_string(),
            ScreenRepoRef {
                repo: Some("github.com/x/y".to_string()),
                pin: Some("main".to_string()),
                token: None,
            },
        );
        let status = ScreenRepoStatus {
            state: Some(ScreenRepoState::Error),
            resolved_sha: Some("deadbeef".to_string()),
            last_fetched: None,
            error: Some("network unreachable".to_string()),
            pin_kind: Some(PinKind::Branch),
        };
        let info = build_screen_repo_info(&config, Some(&status), 0, "weather".to_string());
        assert!(!info.builtin);
        assert_eq!(info.status, "error");
        assert_eq!(info.pin_kind, Some(PinKind::Branch));
        assert_eq!(info.resolved_sha, Some("deadbeef".to_string()));
        assert_eq!(info.error, Some("network unreachable".to_string()));
    }

    #[test]
    fn non_builtin_never_fetched_defaults_to_error() {
        let mut config = AppConfig::default();
        config.screen_repos.insert(
            "weather".to_string(),
            ScreenRepoRef {
                repo: Some("github.com/x/y".to_string()),
                pin: Some("main".to_string()),
                token: None,
            },
        );
        let info = build_screen_repo_info(&config, None, 0, "weather".to_string());
        assert!(!info.builtin);
        assert_eq!(info.status, "error");
        assert_eq!(info.pin_kind, None);
        assert_eq!(info.resolved_sha, None);
        assert_eq!(info.last_fetched, None);
        assert_eq!(info.error, None);
    }

    /// A `ScreenRepoRef` for a non-builtin handle, so `build_screen_repo_info`
    /// exercises the status-mapping branch rather than the builtin shortcut.
    fn config_with_weather() -> AppConfig {
        let mut config = AppConfig::default();
        config.screen_repos.insert(
            "weather".to_string(),
            ScreenRepoRef {
                repo: Some("github.com/x/y".to_string()),
                pin: Some("main".to_string()),
                token: None,
            },
        );
        config
    }

    #[test]
    fn entry_present_with_none_state_defaults_to_error() {
        // The docstring's "defaulting to error if state is None" path — an
        // entry exists but never reached a terminal state. Most likely to
        // regress silently, so pin it down.
        let config = config_with_weather();
        let status = ScreenRepoStatus {
            state: None,
            resolved_sha: Some("abc123".to_string()),
            last_fetched: None,
            error: None,
            pin_kind: Some(PinKind::Tag),
        };
        let info = build_screen_repo_info(&config, Some(&status), 0, "weather".to_string());
        assert_eq!(info.status, "error");
        // Other fields still copy through from the entry.
        assert_eq!(info.pin_kind, Some(PinKind::Tag));
        assert_eq!(info.resolved_sha, Some("abc123".to_string()));
    }

    #[test]
    fn state_serializes_to_snake_case_status_string() {
        let config = config_with_weather();
        for (state, expected) in [
            (ScreenRepoState::Ready, "ready"),
            (ScreenRepoState::Fetching, "fetching"),
            (ScreenRepoState::Offline, "offline"),
        ] {
            let status = ScreenRepoStatus {
                state: Some(state),
                ..Default::default()
            };
            let info = build_screen_repo_info(&config, Some(&status), 0, "weather".to_string());
            assert_eq!(info.status, expected, "state {state:?}");
        }
    }

    #[test]
    fn last_fetched_renders_as_rfc3339() {
        use chrono::{DateTime, Utc};

        let dt: DateTime<Utc> = DateTime::parse_from_rfc3339("2026-07-03T12:34:56+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let config = config_with_weather();
        let status = ScreenRepoStatus {
            state: Some(ScreenRepoState::Ready),
            last_fetched: Some(dt),
            ..Default::default()
        };
        let info = build_screen_repo_info(&config, Some(&status), 0, "weather".to_string());

        let rendered = info.last_fetched.expect("last_fetched present");
        // Equals the canonical RFC3339 rendering...
        assert_eq!(rendered, dt.to_rfc3339());
        // ...and parses back to the same instant.
        let reparsed = DateTime::parse_from_rfc3339(&rendered)
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(reparsed, dt);
    }
}

/// List the registered screen repos. `byonk-builtin` is always present (it is
/// registered by the screen repo loader even without a `screen_repos:` config entry); any
/// additional entries come from `config.screen_repos`. The screen repo `token` is never
/// serialized — only whether one is set (`token_set`).
pub async fn screen_repos(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ScreenRepoInfo>>, ApiError> {
    require_admin(&state, &headers)?;
    let config = state.config.load();

    // Screen counts per handle, from the source of truth (the screen repo loader).
    let loader = state.screen_repo_manager.loader();
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for screen in loader.list_all() {
        *counts.entry(screen.handle).or_insert(0) += 1;
    }

    // Union of loader-registered handles and configured screen repo handles, so the
    // always-registered builtin appears and config-only screen repos are not dropped.
    let mut handles: std::collections::BTreeSet<String> = loader.handles().into_iter().collect();
    handles.extend(config.screen_repos.keys().cloned());

    let statuses = state.screen_repo_manager.status_snapshot();
    let out = handles
        .into_iter()
        .map(|handle| {
            let count = counts.get(&handle).copied().unwrap_or(0);
            build_screen_repo_info(&config, statuses.get(&handle), count, handle)
        })
        .collect();

    Ok(Json(out))
}
