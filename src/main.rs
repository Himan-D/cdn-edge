//! CDN Edge Server - Main entry point

use cdn_edge::{
    cache::{DiskCache, LruCache},
    config::{self, Settings},
    metrics::MetricsExporter,
    tls::CertManager,
    Result,
};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "cdn-edge")]
#[command(about = "CDN Edge Server", long_about = None)]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config/edge.toml")]
    config: PathBuf,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize rustls crypto provider
    rustls::crypto::ring::default_provider().install_default().ok();

    // Initialize logging
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&args.log_level));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting CDN Edge Server...");

    // Load configuration
    let settings = config::load_config(&args.config)
        .map_err(|e| cdn_edge::Error::Config(e.to_string()))?;
    info!("Configuration loaded from: {}", args.config.display());

    // Initialize metrics
    let metrics_exporter = MetricsExporter::new(settings.server.metrics_address.clone());
    let _metrics = metrics_exporter.metrics();

    info!("Metrics configured on {}", settings.server.metrics_address);

    // Initialize cache
    let memory_size = Settings::parse_size(&settings.cache.memory_max_size)
        .expect("Invalid memory cache size");
    let default_ttl = Duration::from_secs(settings.cache.default_ttl);

    info!(
        "Initializing LRU cache with size={} bytes, ttl={:?}",
        memory_size, default_ttl
    );

    let lru_cache = LruCache::with_weight(memory_size, default_ttl);
    let lru_cache_arc: Arc<dyn cdn_edge::cache::CacheStore> = Arc::new(lru_cache);
    
    let mut disk_cache_arc: Option<Arc<dyn cdn_edge::cache::CacheStore>> = None;

    // Optionally initialize disk cache
    if !settings.cache.disk_path.is_empty() {
        info!("Initializing disk cache at: {}", settings.cache.disk_path);
        let disk_max_size = Settings::parse_size(&settings.cache.disk_max_size)
            .unwrap_or(1024 * 1024 * 1024); // default 1GB
        
        let disk_cache_result = match DiskCache::open(&settings.cache.disk_path).await {
            Ok(cache) => Ok(cache),
            Err(_) => DiskCache::new(&settings.cache.disk_path, disk_max_size).await,
        };
        
        match disk_cache_result {
            Ok(disk_cache) => {
                info!("Disk cache loaded successfully");
                disk_cache_arc = Some(Arc::new(disk_cache));
            }
            Err(e) => {
                error!("Failed to load disk cache: {}", e);
            }
        }
    }
    
    let cache: Arc<dyn cdn_edge::cache::CacheStore> = Arc::new(cdn_edge::cache::TieredCache::new(lru_cache_arc, disk_cache_arc));

    // Load TLS certificates
    info!("Loading TLS certificates from: {}", settings.tls.cert_path);
    let _cert_manager = match CertManager::load(&settings.tls.cert_path, &settings.tls.key_path) {
        Ok(manager) => {
            info!("TLS certificates loaded successfully");
            Some(manager)
        }
        Err(e) => {
            error!("Failed to load TLS certificates: {}", e);
            None
        }
    };

    // Start HTTP/3 server
    info!("Starting HTTP/3 server on: {}", settings.server.address);

    let http_server = cdn_edge::http::HttpServer::new(&settings, Arc::clone(&cache)).await?;
    info!("HTTP/3 server started successfully");

    // Start metrics server (in background)
    tokio::spawn(async move {
        if let Err(e) = metrics_exporter.start().await {
            error!("Metrics server error: {}", e);
        }
    });

    // Start control plane heartbeat
    let control_plane_endpoint = settings.control_plane.api_endpoint.clone();
    let edge_location = settings.control_plane.location.clone();
    let heartbeat_interval = settings.heartbeat_interval();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(heartbeat_interval);
        
        // Simple client for heartbeat using hyper
        let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .build(hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .expect("no native root CA certificates found")
                .https_or_http()
                .enable_http1()
                .build());
                
        loop {
            interval.tick().await;
            info!(
                "Sending heartbeat to {} from edge {}",
                control_plane_endpoint, edge_location
            );
            
            let req = hyper::Request::builder()
                .method(hyper::Method::POST)
                .uri(&control_plane_endpoint)
                .header("Content-Type", "application/json")
                .body(http_body_util::Full::new(hyper::body::Bytes::from(
                    format!(r#"{{"location": "{}"}}"#, edge_location)
                )))
                .unwrap();
                
            match client.request(req).await {
                Ok(res) => tracing::debug!("Heartbeat successful, status: {}", res.status()),
                Err(e) => error!("Failed to send heartbeat: {}", e),
            }
        }
    });

    // Run the server
    info!("CDN Edge Server running");
    http_server.serve().await?;

    Ok(())
}
