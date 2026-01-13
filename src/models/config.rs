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

    /// Get screen config for a device
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
}
