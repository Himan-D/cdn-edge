//! CDN Edge Server
//!
//! High-performance edge CDN server built with Rust, supporting HTTP/3,
//! caching, origin fetching, and edge computing capabilities.

pub mod cache;
pub mod config;
pub mod http;
pub mod metrics;
pub mod origin;

use thiserror::Error;

/// CDN Edge error types
#[derive(Error, Debug)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Origin fetch error: {0}")]
    Origin(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("Address parse error: {0}")]
    AddrParse(#[from] std::net::AddrParseError),
}

pub type Result<T> = std::result::Result<T, Error>;
