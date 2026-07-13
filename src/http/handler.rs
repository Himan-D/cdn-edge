//! Request handlers

use crate::cache::CacheStore;
use crate::cache::store::cache_key;
use http::Request;
use hyper::body::Incoming;
use hyper::Response;
use hyper::body::Bytes;
use std::sync::Arc;

use moka::sync::Cache as MokaCache;

use std::sync::atomic::{AtomicU32, Ordering};
use crate::config::Settings;
use crate::origin::OriginClient;
use http_body_util::BodyExt;

/// HTTP request handler
pub struct Handler {
    cache: Arc<dyn CacheStore>,
    rate_limiter: MokaCache<String, Arc<AtomicU32>>,
    max_requests: u32,
    origin_client: OriginClient,
    flight_locks: MokaCache<String, Arc<tokio::sync::Mutex<()>>>,
}

impl Handler {
    pub fn new(settings: &Settings, cache: Arc<dyn CacheStore>) -> Self {
        let rate_limiter = MokaCache::builder()
            .time_to_live(settings.rate_limit_window())
            .build();
            
        let origin_client = OriginClient::new(
            settings.origin.max_retries,
            settings.origin_timeout(),
            settings.origin.shield_nodes.clone(),
        );

        let flight_locks = MokaCache::builder()
            .time_to_idle(std::time::Duration::from_secs(60))
            .build();

        Self { 
            cache, 
            rate_limiter, 
            max_requests: settings.security.rate_limit_requests as u32,
            origin_client,
            flight_locks,
        }
    }

    /// Handle an incoming request
    pub async fn handle(&self, req: Request<Incoming>) -> Response<http_body_util::Full<Bytes>> {
        // Simple rate limiting by host/IP
        let client_id = req.headers().get("x-forwarded-for")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
            
        let count_arc = self.rate_limiter.get_with(client_id, || Arc::new(AtomicU32::new(0)));
        let count = count_arc.fetch_add(1, Ordering::Relaxed) + 1;

        if count > self.max_requests {
            return Response::builder()
                .status(hyper::StatusCode::TOO_MANY_REQUESTS)
                .body(http_body_util::Full::new(Bytes::from("Too Many Requests")))
                .unwrap();
        }

        let method = req.method().as_str();
        let uri = req.uri();
        let host = uri
            .host()
            .unwrap_or_else(|| uri.authority().map(|a| a.host()).unwrap_or("localhost"));
        let path = uri.path();
        let query = uri.query();

        let key = cache_key(method, host, path, query);

        // Try cache first
        if let Some(entry) = self.cache.get(&key).await {
            if !entry.is_expired() {
                tracing::debug!("Cache HIT for {}", key);
                return self.build_response_from_cache(entry);
            }
        }

        tracing::debug!("Cache MISS for {}", key);
        
        let lock = self.flight_locks.get_with(key.clone(), || Arc::new(tokio::sync::Mutex::new(())));
        let _guard = lock.lock().await;
        
        // Double check cache in case another request filled it while we were waiting
        if let Some(entry) = self.cache.get(&key).await {
            if !entry.is_expired() {
                tracing::debug!("Cache HIT (coalesced) for {}", key);
                return self.build_response_from_cache(entry);
            }
        }

        let url = format!("http://{}{}", host, uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/"));

        // Fetch from origin
        match self.fetch_from_origin(&url, &key).await {
            Ok(response) => response,
            Err(e) => {
                tracing::error!("Failed to fetch from origin: {}", e);
                Response::builder()
                    .status(hyper::StatusCode::BAD_GATEWAY)
                    .body(http_body_util::Full::new(Bytes::from("Bad Gateway")))
                    .unwrap()
            }
        }
    }

    fn build_response_from_cache(
        &self,
        entry: crate::cache::CacheEntry,
    ) -> Response<http_body_util::Full<Bytes>> {
        let mut builder = Response::builder();

        // Add cached headers
        for (name, value) in entry.headers.iter() {
            builder = builder.header(name, value);
        }

        // Add cache status header
        builder = builder.header("X-Cache", "HIT");

        builder
            .body(http_body_util::Full::new(entry.body))
            .unwrap()
    }

    async fn fetch_from_origin(
        &self,
        url: &str,
        key: &str,
    ) -> Result<Response<http_body_util::Full<Bytes>>, Box<dyn std::error::Error>> {
        let mut res = self.origin_client.fetch(url).await?;
        
        let body_bytes = res.body_mut().collect().await?.to_bytes();
        
        let mut headers = http::HeaderMap::new();
        for (k, v) in res.headers() {
            headers.insert(k.clone(), v.clone());
        }

        let entry = crate::cache::CacheEntry {
            body: body_bytes.clone(),
            headers: headers.clone(),
            meta: crate::cache::store::CacheMetadata {
                cached_at: std::time::SystemTime::now(),
                ttl: 3600,
                etag: headers.get("etag").and_then(|v| v.to_str().ok()).map(|s| s.to_string()),
                last_modified: headers.get("last-modified").and_then(|v| v.to_str().ok()).map(|s| s.to_string()),
            },
        };
        
        self.cache.put(key.to_string(), entry).await;

        let mut builder = Response::builder()
            .status(res.status())
            .header("X-Cache", "MISS");
            
        for (k, v) in headers.iter() {
            builder = builder.header(k, v);
        }

        Ok(builder.body(http_body_util::Full::new(body_bytes)).unwrap())
    }
}
