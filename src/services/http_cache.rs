//! HTTP response cache with LRU eviction for Lua scripts.
//!
//! Provides optional caching for HTTP responses to reduce API calls.
//! Cache entries expire based on their TTL (time-to-live) in seconds.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Default maximum number of entries in the cache
const DEFAULT_MAX_ENTRIES: usize = 100;

/// Cached HTTP response
struct CachedResponse {
    /// The response body
    body: String,
    /// When this entry was cached
    cached_at: Instant,
    /// How long this entry is valid (TTL)
    ttl: Duration,
}

impl CachedResponse {
    fn new(body: String, ttl_secs: u64) -> Self {
        Self {
            body,
            cached_at: Instant::now(),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }
}

/// HTTP response cache with LRU eviction.
///
/// Thread-safe cache for storing HTTP responses. Uses a simple timestamp-based
/// LRU eviction policy when the cache exceeds its maximum capacity.
struct HttpCache {
    cache: HashMap<String, CachedResponse>,
    /// Order of keys by insertion time (oldest first)
    insertion_order: Vec<String>,
    max_entries: usize,
}

impl HttpCache {
    fn new(max_entries: usize) -> Self {
        Self {
            cache: HashMap::new(),
            insertion_order: Vec::new(),
            max_entries,
        }
    }

    /// Get a cached response if it exists and hasn't expired
    fn get(&mut self, key: &str) -> Option<String> {
        // First check if entry exists
        let entry = self.cache.get(key)?;

        // Check if expired
        if entry.is_expired() {
            // Remove expired entry
            self.cache.remove(key);
            self.insertion_order.retain(|k| k != key);
            return None;
        }

        // Move to end of insertion order (mark as recently used)
        self.insertion_order.retain(|k| k != key);
        self.insertion_order.push(key.to_string());

        Some(entry.body.clone())
    }

    /// Store a response in the cache
    fn store(&mut self, key: String, body: String, ttl_secs: u64) {
        // Remove existing entry if present
        if self.cache.contains_key(&key) {
            self.insertion_order.retain(|k| k != &key);
        }

        // Evict oldest entries if we're at capacity
        while self.cache.len() >= self.max_entries && !self.insertion_order.is_empty() {
            let oldest_key = self.insertion_order.remove(0);
            self.cache.remove(&oldest_key);
            tracing::debug!(
                key = %oldest_key,
                cache_size = self.cache.len(),
                "HTTP cache: evicted oldest entry"
            );
        }

        // Insert new entry
        self.cache
            .insert(key.clone(), CachedResponse::new(body, ttl_secs));
        self.insertion_order.push(key);
    }

    /// Get cache statistics
    #[allow(dead_code)]
    fn stats(&self) -> (usize, usize) {
        (self.cache.len(), self.max_entries)
    }
}

/// Global HTTP cache instance
static HTTP_CACHE: OnceLock<Mutex<HttpCache>> = OnceLock::new();

fn get_cache() -> &'static Mutex<HttpCache> {
    HTTP_CACHE.get_or_init(|| Mutex::new(HttpCache::new(DEFAULT_MAX_ENTRIES)))
}

/// Compute a cache key from request parameters.
///
/// The key is a SHA256 hash of the URL, method, headers, and body.
pub fn compute_cache_key(
    url: &str,
    method: &str,
    params: Option<&[(String, String)]>,
    headers: Option<&[(String, String)]>,
    body: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();

    hasher.update(method.as_bytes());
    hasher.update(b"|");
    hasher.update(url.as_bytes());

    // Include sorted params in key
    if let Some(params) = params {
        hasher.update(b"|params:");
        let mut sorted_params: Vec<_> = params.iter().collect();
        sorted_params.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in sorted_params {
            hasher.update(k.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b"&");
        }
    }

    // Include sorted headers in key (only specific headers that affect response)
    if let Some(headers) = headers {
        hasher.update(b"|headers:");
        let mut sorted_headers: Vec<_> = headers.iter().collect();
        sorted_headers.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in sorted_headers {
            hasher.update(k.as_bytes());
            hasher.update(b":");
            hasher.update(v.as_bytes());
            hasher.update(b";");
        }
    }

    // Include body in key
    if let Some(body) = body {
        hasher.update(b"|body:");
        hasher.update(body.as_bytes());
    }

    let result = hasher.finalize();
    // Use first 16 bytes of hash, encoded as 32 hex characters
    hex::encode(&result[..16])
}

/// Try to get a cached response for the given request.
///
/// Returns `Some(body)` if a valid cached response exists, `None` otherwise.
pub fn get_cached(cache_key: &str) -> Option<String> {
    let mut cache = get_cache().lock().unwrap();
    let result = cache.get(cache_key);
    if result.is_some() {
        tracing::debug!(cache_key = %cache_key, "HTTP cache hit");
    }
    result
}

/// Store a response in the cache.
pub fn store_cached(cache_key: String, body: String, ttl_secs: u64) {
    tracing::debug!(
        cache_key = %cache_key,
        ttl_secs = ttl_secs,
        "HTTP cache: stored response"
    );
    let mut cache = get_cache().lock().unwrap();
    cache.store(cache_key, body, ttl_secs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_deterministic() {
        let key1 = compute_cache_key("https://example.com", "GET", None, None, None);
        let key2 = compute_cache_key("https://example.com", "GET", None, None, None);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_differs_by_url() {
        let key1 = compute_cache_key("https://example.com/a", "GET", None, None, None);
        let key2 = compute_cache_key("https://example.com/b", "GET", None, None, None);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_cache_key_differs_by_method() {
        let key1 = compute_cache_key("https://example.com", "GET", None, None, None);
        let key2 = compute_cache_key("https://example.com", "POST", None, None, None);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_cache_key_differs_by_params() {
        let params1 = vec![("a".to_string(), "1".to_string())];
        let params2 = vec![("a".to_string(), "2".to_string())];
        let key1 = compute_cache_key("https://example.com", "GET", Some(&params1), None, None);
        let key2 = compute_cache_key("https://example.com", "GET", Some(&params2), None, None);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_cache_key_params_order_independent() {
        let params1 = vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
        ];
        let params2 = vec![
            ("b".to_string(), "2".to_string()),
            ("a".to_string(), "1".to_string()),
        ];
        let key1 = compute_cache_key("https://example.com", "GET", Some(&params1), None, None);
        let key2 = compute_cache_key("https://example.com", "GET", Some(&params2), None, None);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_differs_by_body() {
        let key1 = compute_cache_key("https://example.com", "POST", None, None, Some("body1"));
        let key2 = compute_cache_key("https://example.com", "POST", None, None, Some("body2"));
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_http_cache_store_and_get() {
        let mut cache = HttpCache::new(10);
        cache.store("key1".to_string(), "response1".to_string(), 60);

        let result = cache.get("key1");
        assert_eq!(result, Some("response1".to_string()));
    }

    #[test]
    fn test_http_cache_miss() {
        let mut cache = HttpCache::new(10);
        let result = cache.get("nonexistent");
        assert_eq!(result, None);
    }

    #[test]
    fn test_http_cache_lru_eviction() {
        let mut cache = HttpCache::new(3);

        cache.store("key1".to_string(), "response1".to_string(), 60);
        cache.store("key2".to_string(), "response2".to_string(), 60);
        cache.store("key3".to_string(), "response3".to_string(), 60);

        // Access key1 to make it recently used
        cache.get("key1");

        // Adding key4 should evict key2 (oldest unused)
        cache.store("key4".to_string(), "response4".to_string(), 60);

        assert!(cache.get("key1").is_some());
        assert!(cache.get("key2").is_none()); // Evicted
        assert!(cache.get("key3").is_some());
        assert!(cache.get("key4").is_some());
    }

    #[test]
    fn test_http_cache_expiration() {
        let mut cache = HttpCache::new(10);

        // Store with 0 second TTL (immediately expired)
        cache.store("key1".to_string(), "response1".to_string(), 0);

        // Should be expired immediately
        std::thread::sleep(std::time::Duration::from_millis(10));
        let result = cache.get("key1");
        assert_eq!(result, None);
    }

    #[test]
    fn test_http_cache_update_existing() {
        let mut cache = HttpCache::new(10);

        cache.store("key1".to_string(), "response1".to_string(), 60);
        cache.store("key1".to_string(), "response2".to_string(), 60);

        let result = cache.get("key1");
        assert_eq!(result, Some("response2".to_string()));

        // Should not have duplicate keys
        assert_eq!(
            cache
                .insertion_order
                .iter()
                .filter(|k| *k == "key1")
                .count(),
            1
        );
    }
}
