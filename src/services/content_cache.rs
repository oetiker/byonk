use std::sync::Arc;
use tokio::sync::RwLock;

use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Cached rendered SVG ready for PNG conversion
#[derive(Clone)]
pub struct CachedContent {
    /// Pre-rendered SVG content (template already applied)
    pub rendered_svg: String,
    /// Hash of the SVG content (used as filename for change detection)
    pub content_hash: String,
    /// Screen name (for logging)
    pub screen_name: String,
    /// When this content was generated
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

impl CachedContent {
    /// Create a new cached content entry from rendered SVG
    pub fn new(rendered_svg: String, screen_name: String) -> Self {
        let content_hash = compute_svg_hash(&rendered_svg);
        Self {
            rendered_svg,
            content_hash,
            screen_name,
            generated_at: chrono::Utc::now(),
        }
    }
}

/// Compute a short hash of the SVG content for use as filename
fn compute_svg_hash(svg: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(svg.as_bytes());
    let result = hasher.finalize();
    // Use first 8 bytes (16 hex chars) for a reasonably short but unique filename
    hex::encode(&result[..8])
}

/// Cache for script output, keyed by device MAC
pub struct ContentCache {
    cache: Arc<RwLock<HashMap<String, CachedContent>>>,
}

impl ContentCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store script output for a device
    pub async fn store(&self, device_mac: &str, content: CachedContent) {
        let mut cache = self.cache.write().await;
        cache.insert(device_mac.to_string(), content);
    }

    /// Retrieve cached content for a device
    pub async fn get(&self, device_mac: &str) -> Option<CachedContent> {
        let cache = self.cache.read().await;
        cache.get(device_mac).cloned()
    }

    /// Remove cached content for a device
    #[allow(dead_code)]
    pub async fn remove(&self, device_mac: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(device_mac);
    }
}

impl Default for ContentCache {
    fn default() -> Self {
        Self::new()
    }
}
