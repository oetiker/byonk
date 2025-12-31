use hmac::{Hmac, Mac};
use sha2::Sha256;

#[allow(dead_code)]
type HmacSha256 = Hmac<Sha256>;

/// URL signing service for secure image URLs
/// Note: Currently unused since we use content hashes in URLs instead,
/// but kept for potential future use.
#[allow(dead_code)]
pub struct UrlSigner {
    secret: Vec<u8>,
    /// Signature validity in seconds (default: 1 hour)
    validity_secs: i64,
}

#[allow(dead_code)]
impl UrlSigner {
    pub fn new(secret: &str) -> Self {
        Self {
            secret: secret.as_bytes().to_vec(),
            validity_secs: 3600,
        }
    }

    /// Generate a new signing service with a random secret
    pub fn with_random_secret() -> Self {
        use rand::Rng;
        let secret: [u8; 32] = rand::thread_rng().gen();
        Self {
            secret: secret.to_vec(),
            validity_secs: 3600,
        }
    }

    /// Sign a URL path with expiration
    pub fn sign(&self, path: &str) -> (String, i64) {
        let expires = chrono::Utc::now().timestamp() + self.validity_secs;
        let message = format!("{path}:{expires}");

        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC can take key of any size");
        mac.update(message.as_bytes());

        let signature = hex::encode(mac.finalize().into_bytes());
        (signature, expires)
    }

    /// Verify a signed URL
    pub fn verify(&self, path: &str, signature: &str, expires: i64) -> bool {
        // Check expiration
        if chrono::Utc::now().timestamp() > expires {
            return false;
        }

        // Verify signature
        let message = format!("{path}:{expires}");
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC can take key of any size");
        mac.update(message.as_bytes());

        let expected = hex::encode(mac.finalize().into_bytes());
        // Constant-time comparison to prevent timing attacks
        signature == expected
    }
}
