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
    /// Target display width for PNG rendering
    pub width: u32,
    /// Target display height for PNG rendering
    pub height: u32,
}

impl CachedContent {
    /// Create a new cached content entry from rendered SVG
    pub fn new(rendered_svg: String, screen_name: String, width: u32, height: u32) -> Self {
        let content_hash = compute_svg_hash(&rendered_svg);
        Self {
            rendered_svg,
            content_hash,
            screen_name,
            generated_at: chrono::Utc::now(),
            width,
            height,
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

/// Cache for rendered SVG content, keyed by content hash
pub struct ContentCache {
    cache: Arc<RwLock<HashMap<String, CachedContent>>>,
}

impl ContentCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store rendered content (keyed by its content hash)
    pub async fn store(&self, content: CachedContent) {
        let mut cache = self.cache.write().await;
        cache.insert(content.content_hash.clone(), content);
    }

    /// Retrieve cached content by its hash
    pub async fn get(&self, content_hash: &str) -> Option<CachedContent> {
        let cache = self.cache.read().await;
        cache.get(content_hash).cloned()
    }

    /// Remove cached content by hash
    #[allow(dead_code)]
    pub async fn remove(&self, content_hash: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(content_hash);
    }
}

impl Default for ContentCache {
    fn default() -> Self {
        Self::new()
    }
}
