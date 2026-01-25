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

/// Byonk API key prefix
pub const BYONK_KEY_PREFIX: &str = "BNK";

/// Characters used for registration codes (excludes ambiguous I, L, O)
const CODE_CHARS: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ";

/// API authentication token
///
/// Byonk-managed keys have the format: `BNK` + 10 uppercase letters (registration code)
/// Example: `BNKABCDEFGHJK` (13 chars total)
///
/// The 10-letter code:
/// - Uses unambiguous uppercase letters (excludes I, L, O)
/// - Displays as 2 rows of 5 characters on the device screen
/// - Can be used in config.devices as an alternative to MAC address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey(String);

impl ApiKey {
    /// Generate a new Byonk API key with 10-character registration code
    ///
    /// Format: BNK + 10 uppercase letters = 13 total
    /// The 10 letters provide ~41 trillion combinations (23^10)
    pub fn generate() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Generate 10-letter uppercase registration code (easy to read, no ambiguous chars)
        let code: String = (0..10)
            .map(|_| CODE_CHARS[rng.gen_range(0..CODE_CHARS.len())] as char)
            .collect();

        Self(format!("{BYONK_KEY_PREFIX}{code}"))
    }

    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a Byonk-managed API key (BNK prefix + 10 letters)
    pub fn is_byonk_key(&self) -> bool {
        self.0.starts_with(BYONK_KEY_PREFIX) && self.0.len() == 13
    }

    /// Extract the 10-letter registration code from a Byonk key
    ///
    /// Returns None if this is not a valid Byonk key
    pub fn registration_code(&self) -> Option<&str> {
        if self.is_byonk_key() {
            Some(&self.0[3..13])
        } else {
            None
        }
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
    pub fn parse(s: &str) -> Self {
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
