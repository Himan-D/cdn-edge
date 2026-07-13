//! Cache layer with LRU eviction and persistent storage

pub mod disk;
pub mod lru;
pub mod store;
pub mod tiered;

pub use disk::DiskCache;
pub use lru::LruCache;
pub use store::{CacheEntry, CacheStore};
pub use tiered::TieredCache;
