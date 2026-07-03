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
        cfg["environment"]["PACKAGES_CACHE_DIR"].as_str(),
        Some("/data/packages")
    );

    // schema exposes EXACTLY the two intended options — nothing the Phase 3
    // integration owns (registration_enabled, auth_mode, default_screen, ...).
    let schema_keys: std::collections::BTreeSet<&str> = cfg["schema"]
        .as_mapping()
        .expect("schema mapping")
        .keys()
        .filter_map(Value::as_str)
        .collect();
    let expected: std::collections::BTreeSet<&str> =
        ["admin_token", "log_level"].into_iter().collect();
    assert_eq!(
        schema_keys, expected,
        "add-on schema must expose exactly admin_token + log_level"
    );
}
