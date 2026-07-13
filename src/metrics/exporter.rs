//! Prometheus metrics exporter

use hyper::{Request, Response, StatusCode};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use prometheus::{
    self, Encoder, Histogram, IntCounter, IntCounterVec, IntGauge, Registry, TextEncoder,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::TcpListener;
use tracing::{error, info};
use hyper::body::{Bytes, Incoming};
use http_body_util::Full;

/// CDN metrics
pub struct Metrics {
    registry: Registry,

    // Request metrics
    requests_total: IntCounterVec,
    request_duration: Histogram,

    // Cache metrics
    cache_hits: IntCounter,
    cache_misses: IntCounter,
    cache_items: IntGauge,
    cache_bytes: IntGauge,

    // Origin metrics
    origin_requests_total: IntCounterVec,
    origin_errors_total: IntCounterVec,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        // Request metrics
        let requests_total = IntCounterVec::new(
            prometheus::Opts::new("cdn_requests_total", "Total requests"),
            &["method", "status"],
        )
        .unwrap();

        let request_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "cdn_request_duration_seconds",
                "Request duration in seconds",
            )
            .buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
        )
        .unwrap();

        // Cache metrics
        let cache_hits = IntCounter::new("cdn_cache_hits_total", "Total cache hits").unwrap();
        let cache_misses =
            IntCounter::new("cdn_cache_misses_total", "Total cache misses").unwrap();
        let cache_items = IntGauge::new("cdn_cache_items", "Current cache items").unwrap();
        let cache_bytes = IntGauge::new("cdn_cache_bytes", "Current cache bytes").unwrap();

        // Origin metrics
        let origin_requests_total = IntCounterVec::new(
            prometheus::Opts::new("cdn_origin_requests_total", "Total origin requests"),
            &["origin"],
        )
        .unwrap();

        let origin_errors_total = IntCounterVec::new(
            prometheus::Opts::new("cdn_origin_errors_total", "Total origin errors"),
            &["origin", "error_type"],
        )
        .unwrap();

        // Register all metrics
        registry.register(Box::new(requests_total.clone())).unwrap();
        registry
            .register(Box::new(request_duration.clone()))
            .unwrap();
        registry.register(Box::new(cache_hits.clone())).unwrap();
        registry.register(Box::new(cache_misses.clone())).unwrap();
        registry.register(Box::new(cache_items.clone())).unwrap();
        registry.register(Box::new(cache_bytes.clone())).unwrap();
        registry
            .register(Box::new(origin_requests_total.clone()))
            .unwrap();
        registry
            .register(Box::new(origin_errors_total.clone()))
            .unwrap();

        Self {
            registry,
            requests_total,
            request_duration,
            cache_hits,
            cache_misses,
            cache_items,
            cache_bytes,
            origin_requests_total,
            origin_errors_total,
        }
    }

    pub fn inc_requests(&self, method: &str, status: u16) {
        self.requests_total
            .with_label_values(&[method, &status.to_string()])
            .inc();
    }

    pub fn observe_request_duration(&self, duration: f64) {
        self.request_duration.observe(duration);
    }

    pub fn inc_cache_hits(&self) {
        self.cache_hits.inc();
    }

    pub fn inc_cache_misses(&self) {
        self.cache_misses.inc();
    }

    pub fn set_cache_items(&self, count: u64) {
        self.cache_items.set(count as i64);
    }

    pub fn set_cache_bytes(&self, bytes: u64) {
        self.cache_bytes.set(bytes as i64);
    }

    pub fn inc_origin_requests(&self, origin: &str) {
        self.origin_requests_total
            .with_label_values(&[origin])
            .inc();
    }

    pub fn inc_origin_errors(&self, origin: &str, error_type: &str) {
        self.origin_errors_total
            .with_label_values(&[origin, error_type])
            .inc();
    }

    pub fn encode(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer).unwrap_or_default())
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple metrics handler
async fn handle_metrics(metrics: Arc<RwLock<Metrics>>, _req: Request<Incoming>) -> Response<Full<Bytes>> {
    let m = metrics.read().await;
    match m.encode() {
        Ok(text) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Full::new(Bytes::from(text)))
            .unwrap(),
        Err(e) => {
            error!("Failed to encode metrics: {}", e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::from("Failed to encode metrics")))
                .unwrap()
        }
    }
}

/// Metrics exporter server
pub struct MetricsExporter {
    metrics: Arc<RwLock<Metrics>>,
    address: String,
}

impl MetricsExporter {
    pub fn new(address: String) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Metrics::new())),
            address,
        }
    }

    pub fn metrics(&self) -> Arc<RwLock<Metrics>> {
        Arc::clone(&self.metrics)
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(&self.address).await?;
        info!("Metrics server listening on {}", self.address);

        while let Ok((socket, _addr)) = listener.accept().await {
            let metrics = Arc::clone(&self.metrics);
            tokio::spawn(async move {
                let service = service_fn(move |req| {
                    let metrics = metrics.clone();
                    async move { Ok::<_, hyper::Error>(handle_metrics(metrics, req).await) }
                });

                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(socket), service)
                    .await;
            });
        }

        Ok(())
    }
}
