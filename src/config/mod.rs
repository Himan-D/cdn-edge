//! Configuration management

pub mod settings;

use config::{Config, Environment, File};
use std::path::Path;

pub use settings::Settings;

/// Load configuration from file and environment overrides
pub fn load_config(path: &Path) -> Result<Settings, config::ConfigError> {
    let _run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

    let settings = Config::builder()
        .add_source(File::from(path))
        .add_source(Environment::with_prefix("CDN").separator("__"))
        .build()?;

    settings.try_deserialize()
}
