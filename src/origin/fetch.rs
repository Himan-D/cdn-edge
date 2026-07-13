//! Origin fetch client with retry logic

use hyper::{Request, Response};
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use hyper_rustls::HttpsConnector;
use http_body_util::Full;
use hyper::body::Bytes;
use std::time::Duration;
use tracing::{debug, warn};

/// Origin fetch client with retry logic
pub struct OriginClient {
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    max_retries: u32,
    timeout: Duration,
    shield_nodes: Vec<String>,
}

impl OriginClient {
    /// Create a new origin client
    pub fn new(max_retries: u32, timeout: Duration, shield_nodes: Vec<String>) -> Self {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .expect("no native root CA certificates found")
            .https_or_http()
            .enable_http1()
            .build();

        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(90))
            .build(https);

        Self {
            client,
            max_retries,
            timeout,
            shield_nodes,
        }
    }

    /// Fetch a resource from origin with retry logic
    pub async fn fetch(&self, url: &str) -> Result<Response<Incoming>, FetchError> {
        match self.fetch_internal(url).await {
            Ok(res) => Ok(res),
            Err(e) => {
                warn!("Primary origin fetch failed: {}. Trying shield nodes...", e);
                
                // Parse original URL to get path and query
                let uri = url.parse::<hyper::Uri>().map_err(|e| FetchError::RequestBuild(e.to_string()))?;
                let path_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
                
                let mut shields = self.shield_nodes.clone();
                use rand::seq::SliceRandom;
                shields.shuffle(&mut rand::rng());

                for shield in shields {
                    let shield_url = format!("{}{}", shield, path_query);
                    warn!("Trying shield node: {}", shield_url);
                    if let Ok(res) = self.fetch_internal(&shield_url).await {
                        return Ok(res);
                    }
                }
                
                Err(e)
            }
        }
    }

    async fn fetch_internal(&self, url: &str) -> Result<Response<Incoming>, FetchError> {
        let mut retries = 0;

        loop {
            let request = match Request::builder()
                .uri(url)
                .body(Full::<Bytes>::new(Bytes::new()))
            {
                Ok(req) => req,
                Err(e) => {
                    if retries < self.max_retries {
                        retries += 1;
                        warn!("Failed to build request for {}, retry {}/{}", url, retries, self.max_retries);
                        tokio::time::sleep(Duration::from_millis(100 * retries as u64)).await;
                        continue;
                    }
                    return Err(FetchError::RequestBuild(e.to_string()));
                }
            };

            match self.client.request(request).await {
                Ok(response) => {
                    debug!("Successfully fetched from origin: {}", url);
                    return Ok(response);
                }
                Err(e) => {
                    if retries < self.max_retries {
                        retries += 1;
                        warn!(
                            "Failed to fetch from origin {}, retry {}/{}: {}",
                            url, retries, self.max_retries, e
                        );
                        tokio::time::sleep(Duration::from_millis(100 * retries as u64)).await;
                    } else {
                        return Err(FetchError::MaxRetriesExceeded);
                    }
                }
            }
        }
    }

    /// Fetch with conditional request using ETag
    pub async fn fetch_conditional(
        &self,
        url: &str,
        etag: Option<&str>,
        if_modified_since: Option<&str>,
    ) -> Result<Response<Incoming>, FetchError> {
        let mut builder = Request::builder().uri(url);

        if let Some(et) = etag {
            builder = builder.header("If-None-Match", et);
        }

        if let Some(ims) = if_modified_since {
            builder = builder.header("If-Modified-Since", ims);
        }

        let request = builder
            .body(Full::<Bytes>::new(Bytes::new()))
            .map_err(|e| FetchError::RequestBuild(e.to_string()))?;

        self.client
            .request(request)
            .await
            .map_err(|_| FetchError::ConnectionFailed)
    }
}

/// Origin fetch error
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("Failed to build request: {0}")]
    RequestBuild(String),

    #[error("Max retries exceeded")]
    MaxRetriesExceeded,

    #[error("Connection failed")]
    ConnectionFailed,

    #[error("HTTP error: {0}")]
    HttpError(String),
}

impl Clone for OriginClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            max_retries: self.max_retries,
            timeout: self.timeout,
            shield_nodes: self.shield_nodes.clone(),
        }
    }
}
