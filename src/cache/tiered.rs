use super::store::{CacheEntry, CacheKey, CacheStats, CacheStore};
use async_trait::async_trait;
use std::sync::Arc;

/// Tiered cache combining in-memory LRU and persistent disk cache
pub struct TieredCache {
    lru: Arc<dyn CacheStore>,
    disk: Option<Arc<dyn CacheStore>>,
}

impl TieredCache {
    pub fn new(lru: Arc<dyn CacheStore>, disk: Option<Arc<dyn CacheStore>>) -> Self {
        Self { lru, disk }
    }
}

#[async_trait]
impl CacheStore for TieredCache {
    async fn get(&self, key: &CacheKey) -> Option<CacheEntry> {
        if let Some(entry) = self.lru.get(key).await {
            return Some(entry);
        }

        if let Some(disk) = &self.disk {
            if let Some(entry) = disk.get(key).await {
                // Populate L1 cache
                self.lru.put(key.clone(), entry.clone()).await;
                return Some(entry);
            }
        }

        None
    }

    async fn put(&self, key: CacheKey, entry: CacheEntry) {
        self.lru.put(key.clone(), entry.clone()).await;
        if let Some(disk) = &self.disk {
            disk.put(key, entry).await;
        }
    }

    async fn delete(&self, key: &CacheKey) {
        self.lru.delete(key).await;
        if let Some(disk) = &self.disk {
            disk.delete(key).await;
        }
    }

    async fn contains(&self, key: &CacheKey) -> bool {
        if self.lru.contains(key).await {
            return true;
        }
        if let Some(disk) = &self.disk {
            if disk.contains(key).await {
                return true;
            }
        }
        false
    }

    async fn stats(&self) -> CacheStats {
        let mut stats = self.lru.stats().await;
        if let Some(disk) = &self.disk {
            let disk_stats = disk.stats().await;
            stats.items += disk_stats.items;
            stats.bytes += disk_stats.bytes;
            stats.hits += disk_stats.hits;
            stats.misses += disk_stats.misses;
        }
        stats
    }
}
