//! HTTP server placeholder - QUIC/HTTP3 requires complex setup

use crate::cache::CacheStore;
use crate::config::Settings;
use crate::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, error};
use hyper_util::server::conn::auto::Builder;
use hyper_util::rt::TokioIo;
use hyper::service::service_fn;
use crate::http::handler::Handler;

/// HTTP server (placeholder for full HTTP/3 implementation)
pub struct HttpServer {
    local_addr: SocketAddr,
    shutdown: Arc<tokio::sync::Notify>,
    handler: Arc<Handler>,
}

impl HttpServer {
    /// Create a new HTTP server
    pub async fn new(
        settings: &Settings,
        cache: Arc<dyn CacheStore>,
    ) -> Result<Self> {
        let local_addr: SocketAddr = settings.server.address.parse()?;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        let handler = Arc::new(Handler::new(settings, cache));

        info!("HTTP server configured for {}", local_addr);

        Ok(Self {
            local_addr,
            shutdown,
            handler,
        })
    }

    /// Start serving requests
    pub async fn serve(&self) -> Result<()> {
        let socket = socket2::Socket::new(
            socket2::Domain::for_address(self.local_addr),
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )?;
        socket.set_reuse_port(true)?;
        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;
        socket.bind(&self.local_addr.into())?;
        socket.listen(1024)?;
        let listener = tokio::net::TcpListener::from_std(socket.into())?;
        info!("HTTP server running on {}", self.local_addr);

        loop {
            tokio::select! {
                _ = self.shutdown.notified() => {
                    info!("Shutting down HTTP server");
                    break;
                }
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let io = TokioIo::new(stream);
                            let handler_clone = Arc::clone(&self.handler);
                            
                            tokio::spawn(async move {
                                let service = service_fn(move |req| {
                                    let handler = Arc::clone(&handler_clone);
                                    async move {
                                        Ok::<_, std::convert::Infallible>(handler.handle(req).await)
                                    }
                                });
                                
                                if let Err(e) = Builder::new(hyper_util::rt::TokioExecutor::new())
                                    .serve_connection(io, service)
                                    .await
                                {
                                    error!("Error serving connection: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Accept failed: {}", e);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Gracefully shutdown the server
    pub fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }

    /// Get the local address the server is bound to
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}
