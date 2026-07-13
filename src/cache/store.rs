//! Cache storage interface and types

use async_trait::async_trait;
use bytes::Bytes;
use http::HeaderMap;
use std::time::SystemTime;

/// Cached item with metadata
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Response body
    pub body: Bytes,
    /// Response headers
    pub headers: HeaderMap,
    /// Cache control metadata
    pub meta: CacheMetadata,
}

/// Cache metadata for expiration and validation
#[derive(Debug, Clone)]
pub struct CacheMetadata {
    /// When this entry was cached
    pub cached_at: SystemTime,
    /// TTL in seconds (0 = no expiration)
    pub ttl: u64,
    /// ETag for validation
    pub etag: Option<String>,
    /// Last-Modified for validation
    pub last_modified: Option<String>,
}

impl CacheEntry {
    /// Check if this entry is expired
    pub fn is_expired(&self) -> bool {
        if self.meta.ttl == 0 {
            return false;
        }
        let elapsed = SystemTime::now()
            .duration_since(self.meta.cached_at)
            .unwrap_or_default();
        elapsed.as_secs() > self.meta.ttl
    }
}

/// Cache key type
pub type CacheKey = String;

/// Cache storage interface
#[async_trait]
pub trait CacheStore: Send + Sync {
    /// Get a cached item
    async fn get(&self, key: &CacheKey) -> Option<CacheEntry>;

    /// Store an item
    async fn put(&self, key: CacheKey, entry: CacheEntry);

    /// Remove an item
    async fn delete(&self, key: &CacheKey);

    /// Check if key exists
    async fn contains(&self, key: &CacheKey) -> bool;

    /// Get cache statistics
    async fn stats(&self) -> CacheStats;
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total number of items
    pub items: u64,
    /// Total bytes stored
    pub bytes: u64,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Hit rate (0.0 - 1.0)
    pub hit_rate: f64,
}

impl CacheStats {
    pub fn calculate_hit_rate(&mut self) {
        let total = self.hits + self.misses;
        self.hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };
    }
}

/// Generate cache key from request components
pub fn cache_key(method: &str, host: &str, path: &str, query: Option<&str>) -> CacheKey {
    let mut key = format!("{}:{}{}", method, host, path);
    if let Some(q) = query {
        key.push('?');
        key.push_str(q);
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_cache_key_generation() {
        assert_eq!(cache_key("GET", "example.com", "/", None), "GET:example.com/");
        assert_eq!(cache_key("GET", "example.com", "/path", Some("q=1")), "GET:example.com/path?q=1");
    }

    #[test]
    fn test_cache_entry_expiration() {
        let entry = CacheEntry {
            body: Bytes::from(""),
            headers: HeaderMap::new(),
            meta: CacheMetadata {
                cached_at: SystemTime::now() - Duration::from_secs(100),
                ttl: 60,
                etag: None,
                last_modified: None,
            },
        };
        assert!(entry.is_expired());

        let entry_valid = CacheEntry {
            body: Bytes::from(""),
            headers: HeaderMap::new(),
            meta: CacheMetadata {
                cached_at: SystemTime::now() - Duration::from_secs(10),
                ttl: 60,
                etag: None,
                last_modified: None,
            },
        };
        assert!(!entry_valid.is_expired());

        let entry_no_ttl = CacheEntry {
            body: Bytes::from(""),
            headers: HeaderMap::new(),
            meta: CacheMetadata {
                cached_at: SystemTime::now() - Duration::from_secs(1000),
                ttl: 0,
                etag: None,
                last_modified: None,
            },
        };
        assert!(!entry_no_ttl.is_expired());
    }
}
