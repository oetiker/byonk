use crate::assets::AssetLoader;
use serde::Deserialize;
use std::collections::HashMap;

/// Dither tuning values for error_clamp, noise_scale, chroma_clamp, strength.
///
/// Used at every level of the tuning priority chain:
/// panel defaults, device config, script overrides, dev UI overrides.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DitherTuningValues {
    pub error_clamp: Option<f32>,
    pub noise_scale: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub strength: Option<f32>,
}

impl DitherTuningValues {
    /// Merge: self takes priority, other fills gaps.
    pub fn or(&self, other: &DitherTuningValues) -> DitherTuningValues {
        DitherTuningValues {
            error_clamp: self.error_clamp.or(other.error_clamp),
            noise_scale: self.noise_scale.or(other.noise_scale),
            chroma_clamp: self.chroma_clamp.or(other.chroma_clamp),
            strength: self.strength.or(other.strength),
        }
    }

    /// Returns true if all fields are None.
    pub fn is_empty(&self) -> bool {
        self.error_clamp.is_none()
            && self.noise_scale.is_none()
            && self.chroma_clamp.is_none()
            && self.strength.is_none()
    }
}

/// Panel dither configuration with flat defaults and per-algorithm overrides.
///
/// Deserialized from a YAML map where scalar keys (`error_clamp`, `noise_scale`,
/// `chroma_clamp`) become `defaults` and map-valued keys become per-algorithm
/// overrides in `algorithms`.
///
/// ```yaml
/// dither:
///   error_clamp: 0.1         # flat default for all algorithms
///   noise_scale: 5.0
///   floyd-steinberg:          # per-algorithm override
///     error_clamp: 0.08
///     noise_scale: 4.0
/// ```
#[derive(Debug, Clone, Default)]
pub struct PanelDitherConfig {
    pub defaults: DitherTuningValues,
    pub algorithms: HashMap<String, DitherTuningValues>,
}

impl PanelDitherConfig {
    /// Resolve tuning for a specific algorithm.
    /// Returns per-algorithm values (if present) merged with flat defaults.
    pub fn resolve_for_algorithm(&self, algorithm: Option<&str>) -> DitherTuningValues {
        if let Some(algo) = algorithm {
            let normalized = normalize_algorithm_name(algo);
            if let Some(algo_tuning) = self.algorithms.get(&normalized) {
                return algo_tuning.or(&self.defaults);
            }
        }
        self.defaults.clone()
    }
}

impl<'de> Deserialize<'de> for PanelDitherConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};
        use std::fmt;

        struct PanelDitherVisitor;

        impl<'de> Visitor<'de> for PanelDitherVisitor {
            type Value = PanelDitherConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("a map with optional scalar tuning keys and per-algorithm sub-maps")
            }

            fn visit_map<M>(self, mut map: M) -> Result<PanelDitherConfig, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut defaults = DitherTuningValues::default();
                let mut algorithms = HashMap::new();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "error_clamp" => {
                            defaults.error_clamp = Some(map.next_value()?);
                        }
                        "noise_scale" => {
                            defaults.noise_scale = Some(map.next_value()?);
                        }
                        "chroma_clamp" => {
                            defaults.chroma_clamp = Some(map.next_value()?);
                        }
                        "strength" => {
                            defaults.strength = Some(map.next_value()?);
                        }
                        _ => {
                            // Treat as algorithm name with sub-map of tuning values
                            let tuning: DitherTuningValues = map.next_value()?;
                            let normalized = normalize_algorithm_name(&key);
                            algorithms.insert(normalized, tuning);
                        }
                    }
                }

                Ok(PanelDitherConfig {
                    defaults,
                    algorithms,
                })
            }
        }

        deserializer.deserialize_map(PanelDitherVisitor)
    }
}

/// Normalize dither algorithm name to its canonical form.
///
/// Accepts common aliases and returns the canonical hyphenated name
/// used throughout the system.
pub fn normalize_algorithm_name(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "atkinson" => "atkinson".to_string(),
        "atkinson-hybrid" | "atkinson_hybrid" | "atkinsonhybrid" => "atkinson-hybrid".to_string(),
        "floyd-steinberg" | "floyd_steinberg" | "floydsteinberg" => "floyd-steinberg".to_string(),
        "jjn" | "jarvis-judice-ninke" | "jarvis_judice_ninke" => "jarvis-judice-ninke".to_string(),
        "sierra" => "sierra".to_string(),
        "sierra-two-row" | "sierra_two_row" | "sierratworow" => "sierra-two-row".to_string(),
        "sierra-lite" | "sierra_lite" | "sierralite" => "sierra-lite".to_string(),
        "stucki" => "stucki".to_string(),
        "burkes" => "burkes".to_string(),
        other => other.to_string(),
    }
}

/// Reference to a named screen package.
///
/// `repo == None` means the embedded byonk-builtin package.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct PackageRef {
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub pin: Option<String>,
    /// Secret token; redacted in read APIs.
    #[serde(default)]
    pub token: Option<String>,
}

/// Application configuration loaded from config.yaml
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    /// Device to screen mappings
    #[serde(default)]
    pub devices: HashMap<String, DeviceConfig>,

    /// Panel profiles with measured display colors
    #[serde(default)]
    pub panels: HashMap<String, PanelConfig>,

    /// Default screen for unknown devices
    #[serde(default = "default_screen")]
    pub default_screen: Option<String>,

    /// Device registration settings
    #[serde(default)]
    pub registration: RegistrationConfig,

    /// Authentication mode advertised to devices via /api/setup
    /// Values: "api_key" (default), "ed25519"
    /// Note: /api/display always accepts both methods regardless of this setting
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,

    /// Admin/management API settings
    #[serde(default)]
    pub admin: AdminConfig,

    /// Screen package registry
    #[serde(default)]
    pub packages: HashMap<String, PackageRef>,
}

/// Panel profile with official and measured display colors
#[derive(Debug, Deserialize, Clone)]
pub struct PanelConfig {
    /// Human-readable display name (e.g. "TRMNL OG (4-grey)")
    pub name: String,
    /// Exact string match against the firmware `Board` header (or the `Model`
    /// header as a fallback, for devices that report panel identity there).
    #[serde(rename = "match")]
    pub match_pattern: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Official colors (comma-separated hex)
    pub colors: String,
    /// Measured/actual colors (comma-separated hex)
    pub colors_actual: Option<String>,
    /// Per-panel dither tuning defaults
    #[serde(default)]
    pub dither: Option<PanelDitherConfig>,
}

fn default_screen() -> Option<String> {
    Some("byonk-builtin/default".to_string())
}

/// Configuration for a specific device
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DeviceConfig {
    /// Which screen to display
    pub screen: String,

    /// Parameters passed to the Lua script
    #[serde(default)]
    pub params: HashMap<String, serde_yaml::Value>,

    /// Optional display color override (comma-separated hex, e.g. "#000000,#FFFFFF,#FF0000")
    pub colors: Option<String>,

    /// Optional dither algorithm override (e.g. "atkinson", "floyd-steinberg")
    pub dither: Option<String>,

    /// Optional panel profile name (references panels section)
    pub panel: Option<String>,

    /// Optional error clamp override for dithering (e.g. 0.08)
    pub error_clamp: Option<f32>,

    /// Optional blue noise jitter scale override (e.g. 0.6)
    pub noise_scale: Option<f32>,

    /// Optional chroma clamp override for dithering
    pub chroma_clamp: Option<f32>,

    /// Optional dither strength override (0.0 = no diffusion, 1.0 = standard)
    pub strength: Option<f32>,

    /// Optional per-device refresh override in seconds (0/absent = use Lua/screen default)
    #[serde(default)]
    pub refresh: Option<u32>,

    /// Optional friendly name (mirrored from Home Assistant; absent = identify by MAC)
    #[serde(default)]
    pub name: Option<String>,
}

/// Admin/management API settings.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AdminConfig {
    /// Bearer token gating `/api/admin/*`. If unset (and `BYONK_ADMIN_TOKEN` is
    /// unset), the admin API is disabled (returns 404).
    #[serde(default)]
    pub token: Option<String>,
}

/// Device registration settings
#[derive(Debug, Deserialize, Clone)]
pub struct RegistrationConfig {
    /// Whether device registration is required
    ///
    /// When enabled, devices not found in config.devices (by MAC or code)
    /// will display the registration screen with their registration code shown.
    /// Default: true
    #[serde(default = "default_registration_enabled")]
    pub enabled: bool,

    /// Custom screen to use for registration (optional)
    ///
    /// If specified, this screen will be used instead of the built-in registration screen.
    /// The screen's Lua script receives `params.code` with the registration code.
    #[serde(default)]
    pub screen: Option<String>,
}

fn default_registration_enabled() -> bool {
    true
}

fn default_auth_mode() -> String {
    "api_key".to_string()
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            screen: None,
        }
    }
}

impl AppConfig {
    /// Load configuration from AssetLoader (embedded or external).
    ///
    /// Returns an error if the config file cannot be read or contains
    /// invalid YAML. The error message includes the line and column
    /// of the first problem found.
    pub fn load_from_assets(loader: &AssetLoader) -> anyhow::Result<Self> {
        let content = loader
            .read_config_string()
            .map_err(|e| anyhow::anyhow!("Failed to read config: {e}"))?;

        let config: Self = serde_yaml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Invalid config.yaml: {e}"))?;

        tracing::info!(devices = config.devices.len(), "Loaded configuration");

        Ok(config)
    }

    /// Check if a device is registered (by MAC or by registration code)
    ///
    /// This only checks whether a device config entry exists. Screen resolution
    /// happens separately via the package loader (`handle/path` refs), so a
    /// registered device does not require any embedded screen definition.
    pub fn is_device_registered(&self, mac: &str, code: Option<&str>) -> bool {
        // Check by MAC first
        if self.get_device_config(mac).is_some() {
            return true;
        }
        // Check by registration code
        if let Some(code) = code {
            if self.get_device_config_for_code(code).is_some() {
                return true;
            }
        }
        false
    }

    /// Get device config by MAC address (without requiring screen to exist in config)
    pub fn get_device_config(&self, device_mac: &str) -> Option<&DeviceConfig> {
        self.devices.get(device_mac).or_else(|| {
            let normalized = device_mac.to_uppercase();
            if normalized != device_mac {
                self.devices.get(&normalized)
            } else {
                None
            }
        })
    }

    /// Get device config by registration code (without requiring screen to exist in config)
    ///
    /// Accepts both formats: `ABCDEFGHJK` or `ABCDE-FGHJK` (hyphenated)
    pub fn get_device_config_for_code(&self, code: &str) -> Option<&DeviceConfig> {
        let normalized = code.to_uppercase().replace('-', "");
        if normalized.len() == 10 {
            let hyphenated = format!("{}-{}", &normalized[..5], &normalized[5..]);
            if let Some(dc) = self.devices.get(&hyphenated) {
                return Some(dc);
            }
        }
        self.devices.get(&normalized)
    }

    /// Get a panel config by name
    pub fn get_panel(&self, name: &str) -> Option<&PanelConfig> {
        self.panels.get(name)
    }

    /// Find a panel by matching its match_pattern against a board string
    pub fn find_panel_for_board(&self, board: &str) -> Option<(&str, &PanelConfig)> {
        self.panels
            .iter()
            .find(|(_, panel)| {
                panel
                    .match_pattern
                    .as_deref()
                    .is_some_and(|pat| pat == board)
            })
            .map(|(name, panel)| (name.as_str(), panel))
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            devices: HashMap::new(),
            panels: HashMap::new(),
            default_screen: default_screen(),
            registration: RegistrationConfig::default(),
            auth_mode: default_auth_mode(),
            admin: AdminConfig::default(),
            packages: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admin_token_parses_and_defaults() {
        let yaml = "admin:\n  token: secret123\n";
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.admin.token.as_deref(), Some("secret123"));

        let cfg2: AppConfig = serde_yaml::from_str("registration:\n  enabled: true\n").unwrap();
        assert_eq!(cfg2.admin.token, None);
    }

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();

        assert_eq!(
            config.default_screen,
            Some("byonk-builtin/default".to_string())
        );
        assert!(config.devices.is_empty());
        assert!(config.packages.is_empty());
    }

    #[test]
    fn test_default_screen_function() {
        // Test that default_screen function returns Some("byonk-builtin/default")
        assert_eq!(default_screen(), Some("byonk-builtin/default".to_string()));
    }

    #[test]
    fn test_deserialize_config() {
        let yaml = r#"
devices:
  "AA:BB:CC:DD:EE:FF":
    screen: byonk-builtin/example/hello
    params:
      name: "Test User"
default_screen: byonk-builtin/example/hello
"#;

        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(
            config.default_screen,
            Some("byonk-builtin/example/hello".to_string())
        );
        assert!(config.devices.contains_key("AA:BB:CC:DD:EE:FF"));

        let device = config.devices.get("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(device.screen, "byonk-builtin/example/hello");
    }

    #[test]
    fn test_registration_config_default() {
        let reg = RegistrationConfig::default();
        assert!(reg.enabled); // Registration is enabled by default
        assert!(reg.screen.is_none());
    }

    #[test]
    fn test_deserialize_config_with_registration() {
        let yaml = r#"
registration:
  enabled: true
"#;

        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.registration.enabled);
        assert!(config.registration.screen.is_none());
    }

    #[test]
    fn test_deserialize_config_with_custom_registration_screen() {
        let yaml = r#"
registration:
  enabled: true
  screen: byonk-builtin/example/hello
"#;

        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.registration.enabled);
        assert_eq!(
            config.registration.screen,
            Some("byonk-builtin/example/hello".to_string())
        );
    }

    #[test]
    fn test_is_device_registered() {
        let mut config = AppConfig::default();

        // Register by MAC
        config.devices.insert(
            "AA:BB:CC:DD:EE:FF".to_string(),
            DeviceConfig {
                screen: "test".to_string(),
                ..Default::default()
            },
        );

        // Register by code
        config.devices.insert(
            "XYZABCDEFG".to_string(),
            DeviceConfig {
                screen: "test".to_string(),
                ..Default::default()
            },
        );

        // Should find by MAC
        assert!(config.is_device_registered("AA:BB:CC:DD:EE:FF", None));
        // Should find by code
        assert!(config.is_device_registered("00:00:00:00:00:00", Some("XYZABCDEFG")));
        // Should not find unknown
        assert!(!config.is_device_registered("00:00:00:00:00:00", Some("UNKNOWNCODE")));
    }

    #[test]
    fn test_dither_tuning_values_or() {
        let a = DitherTuningValues {
            error_clamp: Some(0.1),
            noise_scale: None,
            chroma_clamp: Some(2.0),
            strength: None,
        };
        let b = DitherTuningValues {
            error_clamp: Some(0.2),
            noise_scale: Some(5.0),
            chroma_clamp: None,
            strength: Some(0.8),
        };
        let merged = a.or(&b);
        assert_eq!(merged.error_clamp, Some(0.1)); // a wins
        assert_eq!(merged.noise_scale, Some(5.0)); // b fills gap
        assert_eq!(merged.chroma_clamp, Some(2.0)); // a wins
        assert_eq!(merged.strength, Some(0.8)); // b fills gap
    }

    #[test]
    fn test_dither_tuning_values_is_empty() {
        assert!(DitherTuningValues::default().is_empty());
        assert!(!DitherTuningValues {
            error_clamp: Some(0.1),
            ..Default::default()
        }
        .is_empty());
    }

    #[test]
    fn test_panel_dither_config_flat_only() {
        let yaml = r#"
error_clamp: 0.1
noise_scale: 5.0
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.defaults.error_clamp, Some(0.1));
        assert_eq!(config.defaults.noise_scale, Some(5.0));
        assert_eq!(config.defaults.chroma_clamp, None);
        assert!(config.algorithms.is_empty());
    }

    #[test]
    fn test_panel_dither_config_per_algorithm_only() {
        let yaml = r#"
floyd-steinberg:
  error_clamp: 0.08
  noise_scale: 4.0
atkinson:
  error_clamp: 0.12
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.defaults.is_empty());
        assert_eq!(config.algorithms.len(), 2);
        let fs = config.algorithms.get("floyd-steinberg").unwrap();
        assert_eq!(fs.error_clamp, Some(0.08));
        assert_eq!(fs.noise_scale, Some(4.0));
        let atk = config.algorithms.get("atkinson").unwrap();
        assert_eq!(atk.error_clamp, Some(0.12));
    }

    #[test]
    fn test_panel_dither_config_mixed() {
        let yaml = r#"
error_clamp: 0.1
noise_scale: 5.0
floyd-steinberg:
  error_clamp: 0.08
  noise_scale: 4.0
atkinson:
  error_clamp: 0.12
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.defaults.error_clamp, Some(0.1));
        assert_eq!(config.defaults.noise_scale, Some(5.0));
        assert_eq!(config.algorithms.len(), 2);
    }

    #[test]
    fn test_panel_dither_config_resolve_for_algorithm() {
        let yaml = r#"
error_clamp: 0.1
noise_scale: 5.0
floyd-steinberg:
  error_clamp: 0.08
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();

        // Algorithm match: per-algo error_clamp, default noise_scale
        let resolved = config.resolve_for_algorithm(Some("floyd-steinberg"));
        assert_eq!(resolved.error_clamp, Some(0.08));
        assert_eq!(resolved.noise_scale, Some(5.0)); // from defaults

        // Algorithm miss: falls back to defaults
        let resolved = config.resolve_for_algorithm(Some("sierra"));
        assert_eq!(resolved.error_clamp, Some(0.1));
        assert_eq!(resolved.noise_scale, Some(5.0));

        // None algorithm: falls back to defaults
        let resolved = config.resolve_for_algorithm(None);
        assert_eq!(resolved.error_clamp, Some(0.1));
    }

    #[test]
    fn test_normalize_algorithm_name() {
        // Canonical names
        assert_eq!(normalize_algorithm_name("atkinson"), "atkinson");
        assert_eq!(
            normalize_algorithm_name("atkinson-hybrid"),
            "atkinson-hybrid"
        );
        assert_eq!(
            normalize_algorithm_name("floyd-steinberg"),
            "floyd-steinberg"
        );
        assert_eq!(
            normalize_algorithm_name("jarvis-judice-ninke"),
            "jarvis-judice-ninke"
        );
        assert_eq!(normalize_algorithm_name("sierra"), "sierra");
        assert_eq!(normalize_algorithm_name("sierra-two-row"), "sierra-two-row");
        assert_eq!(normalize_algorithm_name("sierra-lite"), "sierra-lite");
        assert_eq!(normalize_algorithm_name("stucki"), "stucki");
        assert_eq!(normalize_algorithm_name("burkes"), "burkes");

        // Aliases
        assert_eq!(normalize_algorithm_name("jjn"), "jarvis-judice-ninke");

        // Case insensitive
        assert_eq!(
            normalize_algorithm_name("Floyd-Steinberg"),
            "floyd-steinberg"
        );
        assert_eq!(normalize_algorithm_name("ATKINSON"), "atkinson");

        // Underscore variants
        assert_eq!(
            normalize_algorithm_name("floyd_steinberg"),
            "floyd-steinberg"
        );
        assert_eq!(
            normalize_algorithm_name("atkinson_hybrid"),
            "atkinson-hybrid"
        );
        assert_eq!(normalize_algorithm_name("sierra_two_row"), "sierra-two-row");
    }

    #[test]
    fn test_panel_dither_config_normalizes_aliases() {
        let yaml = r#"
atkinson:
  error_clamp: 0.12
jjn:
  error_clamp: 0.05
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.algorithms.contains_key("atkinson"));
        assert!(config.algorithms.contains_key("jarvis-judice-ninke"));

        let resolved = config.resolve_for_algorithm(Some("atkinson"));
        assert_eq!(resolved.error_clamp, Some(0.12));

        let resolved = config.resolve_for_algorithm(Some("jarvis-judice-ninke"));
        assert_eq!(resolved.error_clamp, Some(0.05));
    }

    #[test]
    fn test_panel_config_with_dither() {
        let yaml = r##"
name: "Test Panel"
colors: "#000000,#FFFFFF"
dither:
  error_clamp: 0.1
  floyd-steinberg:
    error_clamp: 0.08
"##;
        let config: PanelConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.dither.is_some());
        let dither = config.dither.unwrap();
        assert_eq!(dither.defaults.error_clamp, Some(0.1));
        assert!(dither.algorithms.contains_key("floyd-steinberg"));
    }

    #[test]
    fn test_panel_config_without_dither() {
        let yaml = r##"
name: "Test Panel"
colors: "#000000,#FFFFFF"
"##;
        let config: PanelConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.dither.is_none());
    }

    #[test]
    fn test_packages_registry_parses() {
        let yaml = "packages:\n  byonk-builtin: {}\n  weather:\n    repo: github.com/acme/screens\n    pin: v1.4.0\n";
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.packages.contains_key("byonk-builtin"));
        assert_eq!(cfg.packages["weather"].pin.as_deref(), Some("v1.4.0"));
        assert!(cfg.packages["byonk-builtin"].repo.is_none());
    }

    #[test]
    fn test_default_screen_is_builtin() {
        assert_eq!(default_screen(), Some("byonk-builtin/default".to_string()));
    }
}
