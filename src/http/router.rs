//! Request routing

use std::collections::HashMap;
use hyper::body::Bytes;
use hyper::{Request, Response};
use http_body_util::Full;

/// Route handler type
pub type RouteHandler = Box<dyn Fn(Request<hyper::body::Incoming>) -> Response<Full<Bytes>> + Send + Sync>;

/// Simple router for HTTP requests
pub struct Router {
    routes: HashMap<String, RouteHandler>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Add a route
    pub fn add_route<F>(&mut self, pattern: &str, handler: F) -> Result<(), String>
    where
        F: Fn(Request<hyper::body::Incoming>) -> Response<Full<Bytes>> + Send + Sync + 'static,
    {
        self.routes.insert(pattern.to_string(), Box::new(handler));
        Ok(())
    }

    /// Route a request to the appropriate handler
    pub fn route(&self, req: &Request<hyper::body::Incoming>) -> Option<&RouteHandler> {
        let path = req.uri().path();

        // Simple exact match for now
        // In production, use a proper router like matchit
        for (pattern, handler) in &self.routes {
            if pattern == path || pattern == "*" {
                return Some(handler);
            }
        }

        None
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}
