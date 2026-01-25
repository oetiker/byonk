use crate::assets::AssetLoader;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Application configuration loaded from config.yaml
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    /// Screen definitions
    #[serde(default)]
    pub screens: HashMap<String, ScreenConfig>,

    /// Device to screen mappings
    #[serde(default)]
    pub devices: HashMap<String, DeviceConfig>,

    /// Default screen for unknown devices
    #[serde(default = "default_screen")]
    pub default_screen: Option<String>,

    /// Device registration settings
    #[serde(default)]
    pub registration: RegistrationConfig,
}

fn default_screen() -> Option<String> {
    Some("default".to_string())
}

/// Configuration for a screen (script + template)
#[derive(Debug, Deserialize, Clone)]
pub struct ScreenConfig {
    /// Path to Lua script (relative to screens/ directory)
    pub script: PathBuf,

    /// Path to SVG template (relative to screens/ directory)
    pub template: PathBuf,

    /// Default refresh rate in seconds (if script doesn't specify)
    #[serde(default = "default_refresh")]
    pub default_refresh: u32,
}

fn default_refresh() -> u32 {
    900 // 15 minutes
}

/// Configuration for a specific device
#[derive(Debug, Deserialize, Clone)]
pub struct DeviceConfig {
    /// Which screen to display
    pub screen: String,

    /// Parameters passed to the Lua script
    #[serde(default)]
    pub params: HashMap<String, serde_yaml::Value>,
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

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            screen: None,
        }
    }
}

impl AppConfig {
    /// Load configuration from AssetLoader (embedded or external)
    pub fn load_from_assets(loader: &AssetLoader) -> Self {
        match loader.read_config_string() {
            Ok(content) => match serde_yaml::from_str(&content) {
                Ok(config) => {
                    let config: Self = config;
                    tracing::info!(
                        screens = config.screens.len(),
                        devices = config.devices.len(),
                        "Loaded configuration"
                    );
                    config
                }
                Err(e) => {
                    tracing::warn!(%e, "Failed to parse config, using defaults");
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!(%e, "Failed to read config, using defaults");
                Self::default()
            }
        }
    }

    /// Get screen config for a device by MAC address
    pub fn get_screen_for_device(
        &self,
        device_mac: &str,
    ) -> Option<(&ScreenConfig, &DeviceConfig)> {
        // Normalize MAC address (uppercase, colon-separated)
        let normalized = device_mac.to_uppercase();

        if let Some(device_config) = self.devices.get(&normalized) {
            if let Some(screen_config) = self.screens.get(&device_config.screen) {
                return Some((screen_config, device_config));
            }
        }

        None
    }

    /// Get screen config for a device by registration code
    ///
    /// Registration codes are 10-letter uppercase strings that can be used
    /// in config.devices as an alternative to MAC addresses.
    /// Accepts both formats: `ABCDEFGHJK` or `ABCDE-FGHJK` (hyphenated)
    pub fn get_screen_for_code(&self, code: &str) -> Option<(&ScreenConfig, &DeviceConfig)> {
        // Normalize code: uppercase and remove hyphens
        let normalized = code.to_uppercase().replace('-', "");

        // Try hyphenated format first (ABCDE-FGHJK) - this is the preferred config format
        if normalized.len() == 10 {
            let hyphenated = format!("{}-{}", &normalized[..5], &normalized[5..]);
            if let Some(device_config) = self.devices.get(&hyphenated) {
                if let Some(screen_config) = self.screens.get(&device_config.screen) {
                    return Some((screen_config, device_config));
                }
            }
        }

        // Try without hyphen
        if let Some(device_config) = self.devices.get(&normalized) {
            if let Some(screen_config) = self.screens.get(&device_config.screen) {
                return Some((screen_config, device_config));
            }
        }

        None
    }

    /// Check if a device is registered (by MAC or by registration code)
    pub fn is_device_registered(&self, mac: &str, code: Option<&str>) -> bool {
        // Check by MAC first
        if self.get_screen_for_device(mac).is_some() {
            return true;
        }
        // Check by registration code
        if let Some(code) = code {
            if self.get_screen_for_code(code).is_some() {
                return true;
            }
        }
        false
    }

    /// Get the default screen config
    pub fn get_default_screen(&self) -> Option<&ScreenConfig> {
        self.default_screen
            .as_ref()
            .and_then(|name| self.screens.get(name))
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut screens = HashMap::new();
        screens.insert(
            "default".to_string(),
            ScreenConfig {
                script: PathBuf::from("default.lua"),
                template: PathBuf::from("default.svg"),
                default_refresh: 900,
            },
        );

        Self {
            screens,
            devices: HashMap::new(),
            default_screen: Some("default".to_string()),
            registration: RegistrationConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();

        assert_eq!(config.default_screen, Some("default".to_string()));
        assert!(config.screens.contains_key("default"));
        assert!(config.devices.is_empty());

        let default_screen = config.screens.get("default").unwrap();
        assert_eq!(default_screen.script, PathBuf::from("default.lua"));
        assert_eq!(default_screen.template, PathBuf::from("default.svg"));
        assert_eq!(default_screen.default_refresh, 900);
    }

    #[test]
    fn test_get_screen_for_device_found() {
        let mut config = AppConfig::default();

        // Add a custom screen
        config.screens.insert(
            "custom".to_string(),
            ScreenConfig {
                script: PathBuf::from("custom.lua"),
                template: PathBuf::from("custom.svg"),
                default_refresh: 300,
            },
        );

        // Map a device to that screen
        config.devices.insert(
            "AA:BB:CC:DD:EE:FF".to_string(),
            DeviceConfig {
                screen: "custom".to_string(),
                params: HashMap::new(),
            },
        );

        let result = config.get_screen_for_device("AA:BB:CC:DD:EE:FF");
        assert!(result.is_some());

        let (screen_config, device_config) = result.unwrap();
        assert_eq!(screen_config.script, PathBuf::from("custom.lua"));
        assert_eq!(device_config.screen, "custom");
    }

    #[test]
    fn test_get_screen_for_device_normalizes_mac() {
        let mut config = AppConfig::default();

        config.screens.insert(
            "test".to_string(),
            ScreenConfig {
                script: PathBuf::from("test.lua"),
                template: PathBuf::from("test.svg"),
                default_refresh: 600,
            },
        );

        config.devices.insert(
            "AA:BB:CC:DD:EE:FF".to_string(),
            DeviceConfig {
                screen: "test".to_string(),
                params: HashMap::new(),
            },
        );

        // Should match with lowercase input
        let result = config.get_screen_for_device("aa:bb:cc:dd:ee:ff");
        assert!(result.is_some());
    }

    #[test]
    fn test_get_screen_for_device_not_found() {
        let config = AppConfig::default();

        let result = config.get_screen_for_device("UNKNOWN:MAC:ADDRESS");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_screen_for_device_missing_screen() {
        let mut config = AppConfig::default();

        // Map device to non-existent screen
        config.devices.insert(
            "AA:BB:CC:DD:EE:FF".to_string(),
            DeviceConfig {
                screen: "nonexistent".to_string(),
                params: HashMap::new(),
            },
        );

        let result = config.get_screen_for_device("AA:BB:CC:DD:EE:FF");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_default_screen() {
        let config = AppConfig::default();

        let result = config.get_default_screen();
        assert!(result.is_some());

        let screen = result.unwrap();
        assert_eq!(screen.script, PathBuf::from("default.lua"));
    }

    #[test]
    fn test_get_default_screen_missing() {
        let mut config = AppConfig::default();
        config.screens.clear();

        let result = config.get_default_screen();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_default_screen_none_configured() {
        let config = AppConfig {
            default_screen: None,
            ..Default::default()
        };

        let result = config.get_default_screen();
        assert!(result.is_none());
    }

    #[test]
    fn test_screen_config_default_refresh() {
        // Test that default_refresh function returns 900
        assert_eq!(default_refresh(), 900);
    }

    #[test]
    fn test_default_screen_function() {
        // Test that default_screen function returns Some("default")
        assert_eq!(default_screen(), Some("default".to_string()));
    }

    #[test]
    fn test_deserialize_config() {
        let yaml = r#"
screens:
  hello:
    script: hello.lua
    template: hello.svg
    default_refresh: 60
devices:
  "AA:BB:CC:DD:EE:FF":
    screen: hello
    params:
      name: "Test User"
default_screen: hello
"#;

        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.default_screen, Some("hello".to_string()));
        assert!(config.screens.contains_key("hello"));
        assert!(config.devices.contains_key("AA:BB:CC:DD:EE:FF"));

        let hello = config.screens.get("hello").unwrap();
        assert_eq!(hello.default_refresh, 60);

        let device = config.devices.get("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(device.screen, "hello");
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
screens:
  hello:
    script: hello.lua
    template: hello.svg
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
screens:
  hello:
    script: hello.lua
    template: hello.svg
  my_registration:
    script: registration.lua
    template: registration.svg
registration:
  enabled: true
  screen: my_registration
"#;

        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.registration.enabled);
        assert_eq!(
            config.registration.screen,
            Some("my_registration".to_string())
        );
        assert!(config.screens.contains_key("my_registration"));
    }

    #[test]
    fn test_get_screen_for_code() {
        let mut config = AppConfig::default();

        config.screens.insert(
            "custom".to_string(),
            ScreenConfig {
                script: PathBuf::from("custom.lua"),
                template: PathBuf::from("custom.svg"),
                default_refresh: 300,
            },
        );

        // Map a registration code to that screen (hyphenated format)
        config.devices.insert(
            "ABCDE-FGHJK".to_string(),
            DeviceConfig {
                screen: "custom".to_string(),
                params: HashMap::new(),
            },
        );

        // Should find with hyphenated format
        let result = config.get_screen_for_code("ABCDE-FGHJK");
        assert!(result.is_some());

        // Should find with non-hyphenated format
        let result = config.get_screen_for_code("ABCDEFGHJK");
        assert!(result.is_some());

        // Should also work case-insensitively
        let result = config.get_screen_for_code("abcde-fghjk");
        assert!(result.is_some());
    }

    #[test]
    fn test_is_device_registered() {
        let mut config = AppConfig::default();

        config.screens.insert(
            "test".to_string(),
            ScreenConfig {
                script: PathBuf::from("test.lua"),
                template: PathBuf::from("test.svg"),
                default_refresh: 600,
            },
        );

        // Register by MAC
        config.devices.insert(
            "AA:BB:CC:DD:EE:FF".to_string(),
            DeviceConfig {
                screen: "test".to_string(),
                params: HashMap::new(),
            },
        );

        // Register by code
        config.devices.insert(
            "XYZABCDEFG".to_string(),
            DeviceConfig {
                screen: "test".to_string(),
                params: HashMap::new(),
            },
        );

        // Should find by MAC
        assert!(config.is_device_registered("AA:BB:CC:DD:EE:FF", None));
        // Should find by code
        assert!(config.is_device_registered("00:00:00:00:00:00", Some("XYZABCDEFG")));
        // Should not find unknown
        assert!(!config.is_device_registered("00:00:00:00:00:00", Some("UNKNOWNCODE")));
    }
}
