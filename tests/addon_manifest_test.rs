//! Validates the HA add-on package manifest stays consistent with the design.

use serde_yaml::Value;
use std::path::Path;

fn load_yaml(rel: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[test]
fn repository_yaml_has_required_keys() {
    let repo = load_yaml("repository.yaml");
    assert!(repo.get("name").and_then(Value::as_str).is_some(), "name");
    assert!(repo.get("url").and_then(Value::as_str).is_some(), "url");
}

#[test]
fn addon_config_matches_design() {
    let cfg = load_yaml("homeassistant/byonk/config.yaml");

    assert_eq!(cfg["slug"].as_str(), Some("byonk"));
    assert_eq!(cfg["image"].as_str(), Some("ghcr.io/oetiker/byonk"));

    // version must be a concrete, non-empty image tag
    assert!(
        cfg["version"]
            .as_str()
            .map(|v| !v.is_empty())
            .unwrap_or(false),
        "version must be a non-empty string"
    );

    // version must look like X.Y.Z (a valid image tag / add-on version)
    let ver = cfg["version"].as_str().unwrap_or("");
    assert!(
        ver.split('.').count() == 3 && ver.split('.').all(|p| p.parse::<u32>().is_ok()),
        "version must be semver X.Y.Z, got {ver:?}"
    );

    // arch includes amd64 + aarch64
    let arch: Vec<&str> = cfg["arch"]
        .as_sequence()
        .expect("arch seq")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        arch.contains(&"amd64") && arch.contains(&"aarch64"),
        "arch={arch:?}"
    );

    // ports maps 3000/tcp -> 3000
    assert_eq!(cfg["ports"]["3000/tcp"].as_u64(), Some(3000));

    // editable persistent config
    let map: Vec<&str> = cfg["map"]
        .as_sequence()
        .expect("map seq")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(map.contains(&"addon_config:rw"), "map={map:?}");

    // environment wires byonk's paths + bind
    assert_eq!(
        cfg["environment"]["CONFIG_FILE"].as_str(),
        Some("/config/config.yaml")
    );
    assert_eq!(
        cfg["environment"]["SCREENS_DIR"].as_str(),
        Some("/config/screens")
    );
    assert_eq!(
        cfg["environment"]["FONTS_DIR"].as_str(),
        Some("/config/fonts")
    );
    assert_eq!(
        cfg["environment"]["BIND_ADDR"].as_str(),
        Some("0.0.0.0:3000")
    );
    // Remote screen-package cache lives in the add-on's persistent /data so it
    // survives restarts/rebuilds (unset would fall back to an ephemeral temp dir).
    assert_eq!(
        cfg["environment"]["SCREEN_REPOS_CACHE_DIR"].as_str(),
        Some("/data/packages")
    );

    // schema exposes EXACTLY the add-on-owned options: admin_token + log_level
    // (Phase 2) plus the genuinely-global settings and the package registry
    // (Plan A — add-on-owned global config: auth_mode, screen_repo_refresh_interval,
    // packages), plus the dev UI login (dev_username/dev_password, Task 11 —
    // the optional external login for the /dev/ screen editor). The
    // operational registration_enabled toggle and per-device config are NOT
    // here — they stay live via the admin API / HA integration.
    let schema_keys: std::collections::BTreeSet<&str> = cfg["schema"]
        .as_mapping()
        .expect("schema mapping")
        .keys()
        .filter_map(Value::as_str)
        .collect();
    let expected: std::collections::BTreeSet<&str> = [
        "admin_token",
        "log_level",
        "auth_mode",
        "screen_repo_refresh_interval",
        "screen_repos",
        "dev_username",
        "dev_password",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        schema_keys, expected,
        "add-on schema must expose exactly admin_token, log_level, auth_mode, \
         screen_repo_refresh_interval, screen_repos, dev_username, and dev_password"
    );
}

/// Task 11 (dev-mode-editor spec, section 1 "Add-on manifest"): Home
/// Assistant reaches the screen editor two ways — a no-second-login Ingress
/// sidebar entry (`ingress_port`/`ingress_entry`/`panel_*`, and the
/// `BYONK_INGRESS_ADDR` listener the server binds for it), and an optional
/// external login for people who reach `/dev/` directly. The ingress port
/// must stay off the public `ports:` map — it is reached only through the
/// Supervisor proxy or, per the spec's peer check, from inside the add-on
/// network.
#[test]
fn addon_config_declares_ingress_and_dev_login() {
    let cfg = load_yaml("homeassistant/byonk/config.yaml");
    assert_eq!(cfg["ingress"], true);
    assert_eq!(cfg["ingress_port"], 8099);
    assert_eq!(cfg["ingress_entry"], "/dev/");
    assert_eq!(cfg["panel_admin"], true);
    assert_eq!(cfg["environment"]["BYONK_INGRESS_ADDR"], "0.0.0.0:8099");
    assert_eq!(cfg["schema"]["dev_username"], "str?");
    assert_eq!(cfg["schema"]["dev_password"], "password?");
    assert!(
        cfg["ports"].as_mapping().unwrap().get("8099/tcp").is_none(),
        "ingress port must not be published"
    );
}

#[test]
fn addon_config_declares_integration_install_capabilities() {
    let cfg = load_yaml("homeassistant/byonk/config.yaml");

    // The app writes custom_components/byonk into the HA config dir, which
    // Supervisor mounts at /homeassistant for this mapping.
    let map: Vec<&str> = cfg["map"]
        .as_sequence()
        .expect("map seq")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        map.contains(&"homeassistant_config:rw"),
        "map must grant write access to the HA config dir, got {map:?}"
    );

    // Needed to POST the "restart Home Assistant" notification through
    // /core/api on the Supervisor proxy.
    assert_eq!(cfg["homeassistant_api"].as_bool(), Some(true));

    // Supervisor rejects a discovery message for a service the app does not
    // list here (supervisor/api/discovery.py::set_discovery).
    let discovery: Vec<&str> = cfg["discovery"]
        .as_sequence()
        .expect("discovery seq")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(discovery, vec!["byonk"]);
}
