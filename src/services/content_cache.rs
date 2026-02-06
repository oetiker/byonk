use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::RwLock;

/// Default maximum number of entries in the cache
const DEFAULT_MAX_ENTRIES: usize = 100;

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
    /// Display color palette as RGB tuples (if provided by firmware)
    pub colors: Option<Vec<(u8, u8, u8)>>,
    /// Measured/actual display colors (what the panel really shows)
    pub colors_actual: Option<Vec<(u8, u8, u8)>>,
    /// Dither mode ("photo" or "graphics"), None = default (graphics)
    pub dither: Option<String>,
    /// Whether to preserve exact palette matches (None = default true)
    pub preserve_exact: Option<bool>,
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
            colors: None,
            colors_actual: None,
            dither: None,
            preserve_exact: None,
        }
    }

    /// Set the display color palette
    pub fn with_colors(mut self, colors: Option<Vec<(u8, u8, u8)>>) -> Self {
        self.colors = colors;
        self
    }

    /// Set the measured/actual display colors
    pub fn with_colors_actual(mut self, colors_actual: Option<Vec<(u8, u8, u8)>>) -> Self {
        self.colors_actual = colors_actual;
        self
    }

    /// Set the dither mode
    pub fn with_dither(mut self, dither: Option<String>) -> Self {
        self.dither = dither;
        self
    }

    /// Set the preserve_exact flag
    pub fn with_preserve_exact(mut self, preserve_exact: Option<bool>) -> Self {
        self.preserve_exact = preserve_exact;
        self
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

/// Cache for rendered SVG content, keyed by content hash.
///
/// Uses LRU eviction when the cache exceeds `max_entries`. Entries are evicted
/// based on their `generated_at` timestamp (oldest first).
///
/// This cache uses synchronous locking (`std::sync::RwLock`) to allow safe access
/// from both async contexts and `spawn_blocking` tasks without nested runtime issues.
pub struct ContentCache {
    cache: RwLock<HashMap<String, CachedContent>>,
    max_entries: usize,
}

impl ContentCache {
    /// Create a new cache with default max entries (100)
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_ENTRIES)
    }

    /// Create a new cache with specified max entries
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            max_entries,
        }
    }

    /// Store rendered content (keyed by its content hash).
    /// Evicts oldest entries if cache exceeds max_entries.
    ///
    /// This is a synchronous operation safe to call from any context.
    pub fn store(&self, content: CachedContent) {
        let mut cache = self.cache.write().unwrap();

        // Insert the new content
        cache.insert(content.content_hash.clone(), content);

        // Evict oldest entries if we exceed max_entries
        while cache.len() > self.max_entries {
            if let Some(oldest_hash) = self.find_oldest_entry(&cache) {
                tracing::debug!(
                    hash = %oldest_hash,
                    cache_size = cache.len(),
                    max_entries = self.max_entries,
                    "Evicting oldest cache entry"
                );
                cache.remove(&oldest_hash);
            } else {
                break;
            }
        }
    }

    /// Find the hash of the oldest entry by generated_at timestamp
    fn find_oldest_entry(&self, cache: &HashMap<String, CachedContent>) -> Option<String> {
        cache
            .values()
            .min_by_key(|c| c.generated_at)
            .map(|c| c.content_hash.clone())
    }

    /// Retrieve cached content by its hash.
    ///
    /// This is a synchronous operation safe to call from any context.
    pub fn get(&self, content_hash: &str) -> Option<CachedContent> {
        let cache = self.cache.read().unwrap();
        cache.get(content_hash).cloned()
    }

    /// Remove cached content by hash.
    ///
    /// This is a synchronous operation safe to call from any context.
    #[allow(dead_code)]
    pub fn remove(&self, content_hash: &str) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(content_hash);
    }

    /// Get the current number of entries in the cache
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        let cache = self.cache.read().unwrap();
        cache.len()
    }

    /// Check if the cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

    #[test]
    fn test_content_cache_new() {
        let cache = ContentCache::new();
        // Should not panic
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_content_cache_default() {
        let cache = ContentCache::default();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_content_cache_store_and_get() {
        let cache = ContentCache::new();
        let content =
            CachedContent::new("<svg>hello</svg>".to_string(), "test".to_string(), 800, 480);
        let hash = content.content_hash.clone();

        cache.store(content);

        let retrieved = cache.get(&hash);
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.rendered_svg, "<svg>hello</svg>");
        assert_eq!(retrieved.screen_name, "test");
    }

    #[test]
    fn test_content_cache_get_nonexistent() {
        let cache = ContentCache::new();
        let result = cache.get("does_not_exist");
        assert!(result.is_none());
    }

    #[test]
    fn test_content_cache_remove() {
        let cache = ContentCache::new();
        let content = CachedContent::new(
            "<svg>test</svg>".to_string(),
            "screen".to_string(),
            800,
            480,
        );
        let hash = content.content_hash.clone();

        cache.store(content);
        assert!(cache.get(&hash).is_some());

        cache.remove(&hash);
        assert!(cache.get(&hash).is_none());
    }

    #[test]
    fn test_content_cache_overwrite() {
        let cache = ContentCache::new();

        // Store first content
        let content1 =
            CachedContent::new("<svg>v1</svg>".to_string(), "screen".to_string(), 800, 480);
        let hash1 = content1.content_hash.clone();
        cache.store(content1);

        // Store different content with same hash (simulating update - though this shouldn't happen)
        // Actually, same SVG would have same hash, so let's test that storing same content works
        let content2 = CachedContent::new(
            "<svg>v1</svg>".to_string(),
            "updated".to_string(),
            1872,
            1404,
        );
        cache.store(content2);

        let retrieved = cache.get(&hash1).unwrap();
        // Screen name should be updated
        assert_eq!(retrieved.screen_name, "updated");
    }

    #[test]
    fn test_content_cache_multiple_entries() {
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

        cache.store(content1);
        cache.store(content2);
        cache.store(content3);

        assert!(cache.get(&hash1).is_some());
        assert!(cache.get(&hash2).is_some());
        assert!(cache.get(&hash3).is_some());

        // Remove one, others should remain
        cache.remove(&hash2);
        assert!(cache.get(&hash1).is_some());
        assert!(cache.get(&hash2).is_none());
        assert!(cache.get(&hash3).is_some());
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

    #[test]
    fn test_content_cache_with_capacity() {
        let cache = ContentCache::with_capacity(50);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_content_cache_len_and_is_empty() {
        let cache = ContentCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        let content = CachedContent::new(
            "<svg>test</svg>".to_string(),
            "screen".to_string(),
            800,
            480,
        );
        cache.store(content);

        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_content_cache_lru_eviction() {
        // Create a cache with max 3 entries
        let cache = ContentCache::with_capacity(3);

        // Add 3 entries with small delays to ensure different timestamps
        let content1 =
            CachedContent::new("<svg>1</svg>".to_string(), "screen1".to_string(), 800, 480);
        let hash1 = content1.content_hash.clone();
        cache.store(content1);

        let content2 =
            CachedContent::new("<svg>2</svg>".to_string(), "screen2".to_string(), 800, 480);
        let hash2 = content2.content_hash.clone();
        cache.store(content2);

        let content3 =
            CachedContent::new("<svg>3</svg>".to_string(), "screen3".to_string(), 800, 480);
        let hash3 = content3.content_hash.clone();
        cache.store(content3);

        assert_eq!(cache.len(), 3);

        // Adding a 4th entry should evict the oldest (content1)
        let content4 =
            CachedContent::new("<svg>4</svg>".to_string(), "screen4".to_string(), 800, 480);
        let hash4 = content4.content_hash.clone();
        cache.store(content4);

        assert_eq!(cache.len(), 3);
        assert!(
            cache.get(&hash1).is_none(),
            "Oldest entry should be evicted"
        );
        assert!(cache.get(&hash2).is_some());
        assert!(cache.get(&hash3).is_some());
        assert!(cache.get(&hash4).is_some());
    }

    #[test]
    fn test_content_cache_eviction_multiple() {
        // Create a cache with max 2 entries
        let cache = ContentCache::with_capacity(2);

        // Add 5 entries rapidly
        for i in 0..5 {
            let content = CachedContent::new(
                format!("<svg>{}</svg>", i),
                format!("screen{}", i),
                800,
                480,
            );
            cache.store(content);
        }

        // Should only have 2 entries
        assert_eq!(cache.len(), 2);
    }
}
