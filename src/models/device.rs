use serde::{Deserialize, Serialize};
use std::fmt;

/// Device identifier (MAC address)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(mac: impl Into<String>) -> Self {
        Self(mac.into())
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// API authentication token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn generate() -> Self {
        use rand::Rng;
        let key: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        Self(key)
    }

    pub fn from_str(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Device model type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceModel {
    /// Original TRMNL: 800x480, max 90KB
    OG,
    /// TRMNL X: 1872x1404, max 750KB
    X,
}

impl DeviceModel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "x" => DeviceModel::X,
            _ => DeviceModel::OG,
        }
    }
}

impl fmt::Display for DeviceModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceModel::OG => write!(f, "og"),
            DeviceModel::X => write!(f, "x"),
        }
    }
}

/// Registered device with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub device_id: DeviceId,
    pub api_key: ApiKey,
    pub friendly_id: String,
    pub model: DeviceModel,
    pub firmware_version: String,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub battery_voltage: Option<f32>,
    pub rssi: Option<i32>,
}

impl Device {
    pub fn new(device_id: DeviceId, model: DeviceModel, fw_version: String) -> Self {
        Self {
            device_id,
            api_key: ApiKey::generate(),
            friendly_id: Self::generate_friendly_id(),
            model,
            firmware_version: fw_version,
            last_seen: chrono::Utc::now(),
            battery_voltage: None,
            rssi: None,
        }
    }

    /// Generate a friendly ID with 48 bits of entropy (12 hex chars)
    /// This provides ~281 trillion combinations, making collisions extremely unlikely
    fn generate_friendly_id() -> String {
        use rand::Rng;
        let high = rand::thread_rng().gen::<u32>();
        let low = rand::thread_rng().gen::<u16>();
        format!("{high:08X}{low:04X}")
    }
}
