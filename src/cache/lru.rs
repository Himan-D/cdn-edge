//! In-memory LRU cache using Moka

use super::store::{CacheEntry, CacheKey, CacheStats, CacheStore};
use async_trait::async_trait;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// In-memory LRU cache
pub struct LruCache {
    cache: Cache<CacheKey, CacheEntry>,
    stats: Arc<RwLock<CacheStats>>,
}

impl LruCache {
    /// Create a new LRU cache with the given maximum size in bytes
    pub fn new(max_size: u64, default_ttl: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_size)
            .time_to_live(default_ttl)
            .build();

        Self {
            cache,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Create a new LRU cache that counts by weight (estimated size)
    pub fn with_weight(max_size: u64, default_ttl: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_size)
            .time_to_live(default_ttl)
            .weigher(|_key, value: &CacheEntry| value.body.len() as u32)
            .build();

        Self {
            cache,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }
}

#[async_trait]
impl CacheStore for LruCache {
    async fn get(&self, key: &CacheKey) -> Option<CacheEntry> {
        let result = self.cache.get(key).await;
        let mut stats = self.stats.write().await;
        if result.is_some() {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }
        stats.calculate_hit_rate();
        result
    }

    async fn put(&self, key: CacheKey, entry: CacheEntry) {
        self.cache.insert(key, entry).await;
        let mut stats = self.stats.write().await;
        stats.items = self.cache.entry_count() as u64;
        stats.bytes = self.cache.weighted_size();
    }

    async fn delete(&self, key: &CacheKey) {
        self.cache.invalidate(key).await;
        let mut stats = self.stats.write().await;
        stats.items = self.cache.entry_count() as u64;
        stats.bytes = self.cache.weighted_size();
    }

    async fn contains(&self, key: &CacheKey) -> bool {
        self.cache.contains_key(key)
    }

    async fn stats(&self) -> CacheStats {
        let stats = self.stats.read().await;
        stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::store::CacheMetadata;
    use bytes::Bytes;
    use http::HeaderMap;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_lru_cache_operations() {
        let cache = LruCache::new(100, Duration::from_secs(60));
        let key = "GET:test.com/path".to_string();
        
        let entry = CacheEntry {
            body: Bytes::from("test body"),
            headers: HeaderMap::new(),
            meta: CacheMetadata {
                cached_at: SystemTime::now(),
                ttl: 60,
                etag: None,
                last_modified: None,
            },
        };

        // Initially empty
        assert!(cache.get(&key).await.is_none());

        // Put an item
        cache.put(key.clone(), entry.clone()).await;

        // Check if exists
        assert!(cache.contains(&key).await);

        // Get the item
        let cached = cache.get(&key).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().body, Bytes::from("test body"));

        // Check stats
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);

        // Delete the item
        cache.delete(&key).await;
        assert!(!cache.contains(&key).await);
    }
}
