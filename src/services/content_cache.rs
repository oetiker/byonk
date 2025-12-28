use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Cached content ready to be served
#[derive(Clone)]
pub struct CachedContent {
    /// Rendered PNG bytes
    pub png_bytes: Vec<u8>,
    /// Refresh rate returned by the script
    pub refresh_rate: u32,
    /// When this content was generated
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

/// Cache for rendered content, keyed by device MAC
pub struct ContentCache {
    cache: Arc<RwLock<HashMap<String, CachedContent>>>,
}

impl ContentCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store rendered content for a device
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
