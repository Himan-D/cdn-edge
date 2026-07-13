//! Connection pool for origin servers

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

/// Connection pool for managing concurrent origin connections
pub struct ConnectionPool {
    pools: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
    max_connections: usize,
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new(max_connections: usize) -> Self {
        Self {
            pools: Arc::new(Mutex::new(HashMap::new())),
            max_connections,
        }
    }

    /// Acquire a connection slot for the given origin
    pub async fn acquire(&self, origin: &str) -> ConnectionGuard {
        let mut pools = self.pools.lock().await;

        let semaphore = pools
            .entry(origin.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.max_connections)))
            .clone();

        // Release the lock before acquiring permit
        drop(pools);

        // Acquire permit
        let permit = semaphore.acquire_owned().await.unwrap();

        ConnectionGuard {
            _permit: Some(permit),
            origin: origin.to_string(),
        }
    }

    /// Get current pool statistics
    pub async fn stats(&self) -> PoolStats {
        let pools = self.pools.lock().await;
        PoolStats {
            origins: pools.len(),
            max_connections_per_origin: self.max_connections,
        }
    }
}

/// RAII guard that releases the connection when dropped
pub struct ConnectionGuard {
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
    origin: String,
}

impl ConnectionGuard {
    pub fn origin(&self) -> &str {
        &self.origin
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        // Permit is automatically released when dropped
    }
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub origins: usize,
    pub max_connections_per_origin: usize,
}
