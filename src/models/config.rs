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
    /// Load configuration from a YAML file
    pub fn load(path: &std::path::Path) -> Result<Self, ConfigError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| ConfigError::Io(path.to_path_buf(), e))?;

        serde_yaml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Load configuration or return defaults
    pub fn load_or_default(path: &std::path::Path) -> Self {
        match Self::load(path) {
            Ok(config) => {
                tracing::info!(
                    screens = config.screens.len(),
                    devices = config.devices.len(),
                    "Loaded configuration"
                );
                config
            }
            Err(e) => {
                tracing::warn!(%e, "Failed to load config, using defaults");
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

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file {0}: {1}")]
    Io(PathBuf, std::io::Error),

    #[error("Failed to parse config: {0}")]
    Parse(String),
}
