use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::DeviceContext;

/// Cached script output ready for rendering
#[derive(Clone)]
pub struct CachedContent {
    /// Data returned by the Lua script
    pub script_data: serde_json::Value,
    /// Device context (battery, rssi) at time of script execution
    pub device_context: Option<DeviceContext>,
    /// Screen name (for finding the template)
    pub screen_name: String,
    /// Template path
    pub template_path: std::path::PathBuf,
    /// Config params for this device
    pub params: HashMap<String, serde_yaml::Value>,
    /// When this content was generated
    pub generated_at: chrono::DateTime<chrono::Utc>,
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
