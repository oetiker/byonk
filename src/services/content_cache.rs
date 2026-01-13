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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cached_content_new() {
        let content = CachedContent::new(
            "<svg></svg>".to_string(),
            "test_screen".to_string(),
            800,
            480,
        );

        assert_eq!(content.rendered_svg, "<svg></svg>");
        assert_eq!(content.screen_name, "test_screen");
        assert_eq!(content.width, 800);
        assert_eq!(content.height, 480);
        assert!(!content.content_hash.is_empty());
    }

    #[test]
    fn test_cached_content_hash_consistency() {
        let svg = "<svg><text>Hello</text></svg>".to_string();

        let content1 = CachedContent::new(svg.clone(), "screen1".to_string(), 800, 480);
        let content2 = CachedContent::new(svg.clone(), "screen2".to_string(), 1872, 1404);

        // Same SVG content should produce same hash regardless of screen name or dimensions
        assert_eq!(content1.content_hash, content2.content_hash);
    }

    #[test]
    fn test_cached_content_hash_differs_for_different_content() {
        let content1 =
            CachedContent::new("<svg>A</svg>".to_string(), "screen".to_string(), 800, 480);
        let content2 =
            CachedContent::new("<svg>B</svg>".to_string(), "screen".to_string(), 800, 480);

        assert_ne!(content1.content_hash, content2.content_hash);
    }

    #[test]
    fn test_compute_svg_hash_deterministic() {
        let svg = "<svg><rect/></svg>";
        let hash1 = compute_svg_hash(svg);
        let hash2 = compute_svg_hash(svg);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_svg_hash_length() {
        let hash = compute_svg_hash("<svg></svg>");
        // 8 bytes = 16 hex characters
        assert_eq!(hash.len(), 16);
    }

    #[test]
    fn test_compute_svg_hash_is_hex() {
        let hash = compute_svg_hash("<svg>test</svg>");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn test_content_cache_new() {
        let cache = ContentCache::new();
        // Should not panic
        assert!(cache.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_content_cache_default() {
        let cache = ContentCache::default();
        assert!(cache.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_content_cache_store_and_get() {
        let cache = ContentCache::new();
        let content =
            CachedContent::new("<svg>hello</svg>".to_string(), "test".to_string(), 800, 480);
        let hash = content.content_hash.clone();

        cache.store(content).await;

        let retrieved = cache.get(&hash).await;
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.rendered_svg, "<svg>hello</svg>");
        assert_eq!(retrieved.screen_name, "test");
    }

    #[tokio::test]
    async fn test_content_cache_get_nonexistent() {
        let cache = ContentCache::new();
        let result = cache.get("does_not_exist").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_content_cache_remove() {
        let cache = ContentCache::new();
        let content = CachedContent::new(
            "<svg>test</svg>".to_string(),
            "screen".to_string(),
            800,
            480,
        );
        let hash = content.content_hash.clone();

        cache.store(content).await;
        assert!(cache.get(&hash).await.is_some());

        cache.remove(&hash).await;
        assert!(cache.get(&hash).await.is_none());
    }

    #[tokio::test]
    async fn test_content_cache_overwrite() {
        let cache = ContentCache::new();

        // Store first content
        let content1 =
            CachedContent::new("<svg>v1</svg>".to_string(), "screen".to_string(), 800, 480);
        let hash1 = content1.content_hash.clone();
        cache.store(content1).await;

        // Store different content with same hash (simulating update - though this shouldn't happen)
        // Actually, same SVG would have same hash, so let's test that storing same content works
        let content2 = CachedContent::new(
            "<svg>v1</svg>".to_string(),
            "updated".to_string(),
            1872,
            1404,
        );
        cache.store(content2).await;

        let retrieved = cache.get(&hash1).await.unwrap();
        // Screen name should be updated
        assert_eq!(retrieved.screen_name, "updated");
    }

    #[tokio::test]
    async fn test_content_cache_multiple_entries() {
        let cache = ContentCache::new();

        let content1 =
            CachedContent::new("<svg>A</svg>".to_string(), "screen_a".to_string(), 800, 480);
        let content2 =
            CachedContent::new("<svg>B</svg>".to_string(), "screen_b".to_string(), 800, 480);
        let content3 = CachedContent::new(
            "<svg>C</svg>".to_string(),
            "screen_c".to_string(),
            1872,
            1404,
        );

        let hash1 = content1.content_hash.clone();
        let hash2 = content2.content_hash.clone();
        let hash3 = content3.content_hash.clone();

        cache.store(content1).await;
        cache.store(content2).await;
        cache.store(content3).await;

        assert!(cache.get(&hash1).await.is_some());
        assert!(cache.get(&hash2).await.is_some());
        assert!(cache.get(&hash3).await.is_some());

        // Remove one, others should remain
        cache.remove(&hash2).await;
        assert!(cache.get(&hash1).await.is_some());
        assert!(cache.get(&hash2).await.is_none());
        assert!(cache.get(&hash3).await.is_some());
    }

    #[test]
    fn test_cached_content_clone() {
        let content = CachedContent::new(
            "<svg>test</svg>".to_string(),
            "screen".to_string(),
            800,
            480,
        );
        let cloned = content.clone();

        assert_eq!(cloned.rendered_svg, content.rendered_svg);
        assert_eq!(cloned.content_hash, content.content_hash);
        assert_eq!(cloned.screen_name, content.screen_name);
        assert_eq!(cloned.width, content.width);
        assert_eq!(cloned.height, content.height);
    }
}
