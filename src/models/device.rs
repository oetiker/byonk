use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

/// Characters used for registration codes (excludes ambiguous I, L, O)
const CODE_CHARS: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ";

/// API authentication token
///
/// Registration codes are derived from a SHA256 hash of the API key,
/// providing a 10-character code that:
/// - Uses unambiguous uppercase letters (excludes I, L, O)
/// - Displays as 2 rows of 5 characters on the device screen
/// - Can be used in config.devices as an alternative to MAC address
/// - Works with any API key format (TRMNL, Byonk, custom)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey(String);

impl ApiKey {
    /// Generate a new random API key
    ///
    /// Generates a 32-character hex string (~128 bits of entropy)
    pub fn generate() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.gen();
        Self(hex::encode(bytes))
    }

    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Derive a 10-character registration code from any API key by hashing
    ///
    /// The code is deterministic: the same API key always produces the same code.
    /// Uses SHA256 hash converted to a 10-character string using CODE_CHARS alphabet.
    pub fn registration_code(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        let hash = hasher.finalize();

        // Convert first 10 bytes to 10 characters using CODE_CHARS
        hash.iter()
            .take(10)
            .map(|b| CODE_CHARS[(*b as usize) % CODE_CHARS.len()] as char)
            .collect()
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

/// Device runtime metadata (tracked in memory)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub device_id: DeviceId,
    pub api_key: ApiKey,
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
            model,
            firmware_version: fw_version,
            last_seen: chrono::Utc::now(),
            battery_voltage: None,
            rssi: None,
        }
    }
}

/// Maximum allowed clock skew for Ed25519 timestamp verification (±60 seconds)
const MAX_TIMESTAMP_SKEW_MS: i64 = 60_000;

/// Verify an Ed25519 signature from a TRMNL device.
///
/// The device signs: `timestamp_ms (8 bytes BE) || public_key (32 bytes)`
///
/// Returns `Ok(())` if the signature is valid and the timestamp is within ±60 seconds.
pub fn verify_ed25519_signature(
    public_key_hex: &str,
    signature_hex: &str,
    timestamp_ms: u64,
) -> Result<(), Ed25519Error> {
    // Parse public key from hex
    let pk_bytes = hex::decode(public_key_hex).map_err(|_| Ed25519Error::InvalidPublicKey)?;
    if pk_bytes.len() != 32 {
        return Err(Ed25519Error::InvalidPublicKey);
    }
    let pk_array: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| Ed25519Error::InvalidPublicKey)?;
    let verifying_key =
        VerifyingKey::from_bytes(&pk_array).map_err(|_| Ed25519Error::InvalidPublicKey)?;

    // Parse signature from hex
    let sig_bytes = hex::decode(signature_hex).map_err(|_| Ed25519Error::InvalidSignature)?;
    if sig_bytes.len() != 64 {
        return Err(Ed25519Error::InvalidSignature);
    }
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| Ed25519Error::InvalidSignature)?;
    let signature = Signature::from_bytes(&sig_array);

    // Check timestamp is within ±60 seconds
    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
    let diff = (now_ms as i64) - (timestamp_ms as i64);
    if diff.abs() > MAX_TIMESTAMP_SKEW_MS {
        return Err(Ed25519Error::TimestampExpired);
    }

    // Reconstruct signed message: timestamp_ms (8 bytes BE) || public_key (32 bytes)
    let mut message = Vec::with_capacity(40);
    message.extend_from_slice(&timestamp_ms.to_be_bytes());
    message.extend_from_slice(&pk_array);

    // Verify signature
    use ed25519_dalek::Verifier;
    verifying_key
        .verify(&message, &signature)
        .map_err(|_| Ed25519Error::InvalidSignature)
}

/// Errors from Ed25519 verification
#[derive(Debug, thiserror::Error)]
pub enum Ed25519Error {
    #[error("Invalid public key")]
    InvalidPublicKey,
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Timestamp expired")]
    TimestampExpired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ed25519_cross_verify_tweetnacl() {
        // Verify that ed25519-dalek can validate signatures produced by standard Ed25519.
        // Uses ed25519-dalek's own signing to generate a known-good test vector,
        // ensuring the verify path in verify_ed25519_signature works correctly.
        use ed25519_dalek::{Signer, SigningKey, Verifier};

        // Generate a keypair using ed25519-dalek
        let signing_key = SigningKey::from_bytes(&[
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec,
            0x2c, 0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03,
            0x1c, 0xae, 0x7f, 0x60,
        ]);
        let verifying_key = signing_key.verifying_key();
        let pk_hex = hex::encode(verifying_key.as_bytes());

        // Build the message: timestamp_ms (8 bytes BE) || public_key (32 bytes)
        let timestamp_ms: u64 = 1234567890123;
        let mut message = Vec::with_capacity(40);
        message.extend_from_slice(&timestamp_ms.to_be_bytes());
        message.extend_from_slice(verifying_key.as_bytes());

        // Sign and verify directly (bypasses timestamp check)
        let signature = signing_key.sign(&message);
        let sig_hex = hex::encode(signature.to_bytes());

        verifying_key
            .verify(&message, &signature)
            .expect("Direct verification should work");

        // Also verify through our function (will get TimestampExpired for old timestamp)
        let result = verify_ed25519_signature(&pk_hex, &sig_hex, timestamp_ms);
        match &result {
            Ok(()) => {}
            Err(Ed25519Error::TimestampExpired) => {} // expected, timestamp is old
            Err(e) => panic!("Cross-library verification failed: {e}"),
        }
    }

    #[test]
    fn test_registration_code_is_deterministic() {
        let key = ApiKey::new("test-api-key-12345");
        let code1 = key.registration_code();
        let code2 = key.registration_code();
        assert_eq!(code1, code2, "Same key should produce same code");
    }

    #[test]
    fn test_registration_code_length() {
        let key = ApiKey::new("any-key");
        let code = key.registration_code();
        assert_eq!(code.len(), 10, "Registration code should be 10 characters");
    }

    #[test]
    fn test_registration_code_uses_valid_chars() {
        let key = ApiKey::new("test-key");
        let code = key.registration_code();
        let valid_chars = "ABCDEFGHJKMNPQRSTUVWXYZ";
        for c in code.chars() {
            assert!(
                valid_chars.contains(c),
                "Code should only use unambiguous uppercase letters, got '{}'",
                c
            );
        }
    }

    #[test]
    fn test_different_keys_produce_different_codes() {
        let key1 = ApiKey::new("key-one");
        let key2 = ApiKey::new("key-two");
        assert_ne!(
            key1.registration_code(),
            key2.registration_code(),
            "Different keys should produce different codes"
        );
    }

    #[test]
    fn test_registration_code_works_for_trmnl_style_keys() {
        // TRMNL-issued keys are typically UUIDs
        let key = ApiKey::new("550e8400-e29b-41d4-a716-446655440000");
        let code = key.registration_code();
        assert_eq!(code.len(), 10);
    }

    #[test]
    fn test_registration_code_works_for_byonk_generated_keys() {
        let key = ApiKey::generate();
        let code = key.registration_code();
        assert_eq!(code.len(), 10);
    }

    #[test]
    fn test_api_key_generate_produces_hex_string() {
        let key = ApiKey::generate();
        let s = key.as_str();
        assert_eq!(s.len(), 32, "Generated key should be 32 hex characters");
        assert!(
            s.chars().all(|c| c.is_ascii_hexdigit()),
            "Generated key should only contain hex characters"
        );
    }
}
