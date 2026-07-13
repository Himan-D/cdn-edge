//! Persistent disk cache using redb

use super::store::{CacheEntry, CacheKey, CacheStats, CacheStore};
use async_trait::async_trait;
use bytes::Bytes;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Table definition for cache entries
static CACHE_TABLE: TableDefinition<&'static str, &'static [u8]> = TableDefinition::new("cache");

/// Metadata table definition
static META_TABLE: TableDefinition<&'static str, &'static [u8]> = TableDefinition::new("metadata");

/// Serialized cache metadata for disk storage
#[derive(Debug, Serialize, Deserialize, Clone)]
struct DiskMetadata {
    cached_at: u64,
    ttl: u64,
    etag: Option<String>,
    last_modified: Option<String>,
}

/// Disk-based persistent cache
pub struct DiskCache {
    db: Arc<Database>,
    stats: Arc<Mutex<CacheStats>>,
}

impl DiskCache {
    /// Create a new disk cache at the given path
    pub async fn new(path: impl AsRef<Path>, max_size: u64) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Database::create(path)?;
        let txn = db.begin_write()?;

        txn.open_table(CACHE_TABLE)?;
        txn.open_table(META_TABLE)?;
        txn.commit()?;

        Ok(Self {
            db: Arc::new(db),
            stats: Arc::new(Mutex::new(CacheStats {
                bytes: max_size,
                ..Default::default()
            })),
        })
    }

    /// Create or open an existing disk cache
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Database::open(path)?;

        let txn = db.begin_write()?;
        let mut items = 0u64;
        let mut bytes = 0u64;

        {
            let cache_table = txn.open_table(CACHE_TABLE)?;
            let _meta_table = txn.open_table(META_TABLE)?;

            let iter = cache_table.iter()?;
            for entry in iter {
                let (_, value) = entry?;
                items += 1;
                bytes += value.value().len() as u64;
            }
        }

        txn.commit()?;

        Ok(Self {
            db: Arc::new(db),
            stats: Arc::new(Mutex::new(CacheStats {
                items,
                bytes,
                ..Default::default()
            })),
        })
    }
}

#[async_trait]
impl CacheStore for DiskCache {
    async fn get(&self, key: &CacheKey) -> Option<CacheEntry> {
        let txn = self.db.begin_read().ok()?;
        let table = txn.open_table(CACHE_TABLE).ok()?;
        let meta_table = txn.open_table(META_TABLE).ok()?;

        let value = table.get(key.as_str()).ok()??;
        let meta_bytes = meta_table.get(key.as_str()).ok()??;

        // Convert AccessGuard to bytes
        let value_bytes: Vec<u8> = value.value().to_vec();
        let meta_bytes_vec: Vec<u8> = meta_bytes.value().to_vec();

        // Deserialize metadata
        let disk_meta: DiskMetadata = bincode::deserialize(&meta_bytes_vec).ok()?;

        let mut headers = http::HeaderMap::new();
        if let Some(ref etag) = disk_meta.etag {
            headers.insert("etag", http::HeaderValue::from_str(etag).ok()?);
        }
        if let Some(ref last_modified) = disk_meta.last_modified {
            headers.insert("last-modified", http::HeaderValue::from_str(last_modified).ok()?);
        }

        Some(CacheEntry {
            body: Bytes::from(value_bytes),
            headers,
            meta: super::store::CacheMetadata {
                cached_at: std::time::SystemTime::UNIX_EPOCH
                    .checked_add(std::time::Duration::from_secs(disk_meta.cached_at))?,
                ttl: disk_meta.ttl,
                etag: disk_meta.etag,
                last_modified: disk_meta.last_modified,
            },
        })
    }

    async fn put(&self, key: CacheKey, entry: CacheEntry) {
        let disk_meta = DiskMetadata {
            cached_at: entry
                .meta
                .cached_at
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            ttl: entry.meta.ttl,
            etag: entry.meta.etag.clone(),
            last_modified: entry.meta.last_modified.clone(),
        };

        let body_bytes: Vec<u8> = entry.body.to_vec();
        let meta_bytes: Vec<u8> = bincode::serialize(&disk_meta).unwrap_or_default();

        // Store key as owned string for lifetime
        let key_static: &'static str = unsafe { &*(key.as_str() as *const str) };

        if let Ok(txn) = self.db.begin_write() {
            let table = txn.open_table(CACHE_TABLE).ok();
            let meta_table = txn.open_table(META_TABLE).ok();

            if let (Some(mut t), Some(mut mt)) = (table, meta_table) {
                let _ = t.insert(key_static, &body_bytes.as_slice());
                let _ = mt.insert(key_static, &meta_bytes.as_slice());

                let mut stats = self.stats.lock().await;
                stats.items += 1;
                stats.bytes += body_bytes.len() as u64;
            }

            let _ = txn.commit();
        }
    }

    async fn delete(&self, key: &CacheKey) {
        let key_static: &'static str = unsafe { &*(key.as_str() as *const str) };

        if let Ok(txn) = self.db.begin_write() {
            let table = txn.open_table(CACHE_TABLE).ok();
            let meta_table = txn.open_table(META_TABLE).ok();

            if let (Some(mut t), Some(mut mt)) = (table, meta_table) {
                let _ = t.remove(key_static);
                let _ = mt.remove(key_static);

                let mut stats = self.stats.lock().await;
                stats.items = stats.items.saturating_sub(1);
            }

            let _ = txn.commit();
        }
    }

    async fn contains(&self, key: &CacheKey) -> bool {
        let key_static: &'static str = unsafe { &*(key.as_str() as *const str) };

        if let Ok(txn) = self.db.begin_read() {
            if let Ok(table) = txn.open_table(CACHE_TABLE) {
                return table.get(key_static).is_ok();
            }
        }
        false
    }

    async fn stats(&self) -> CacheStats {
        let stats = self.stats.lock().await;
        stats.clone()
    }
}
