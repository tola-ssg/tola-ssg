//! Configuration for typst-batch.
//!
//! This module provides runtime configuration for package downloads.
//! Use [`ConfigBuilder`] at application startup to configure the User-Agent string.

use std::sync::OnceLock;

use typst_kit::download::Downloader;
use typst_kit::package::PackageStorage;

/// Global configuration, initialized via [`init`].
static CONFIG: OnceLock<Config> = OnceLock::new();

/// Runtime configuration for typst-batch.
#[derive(Debug, Clone)]
pub struct Config {
    /// User-Agent string for package downloads from the Typst registry.
    /// Example: "my-app/1.0.0"
    pub user_agent: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            user_agent: concat!("typst-batch/", env!("CARGO_PKG_VERSION")).to_string(),
        }
    }
}

/// Configuration builder for fluent API.
#[derive(Debug, Clone, Default)]
pub struct ConfigBuilder {
    user_agent: Option<String>,
}

impl ConfigBuilder {
    /// Create a new configuration builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the User-Agent string for package downloads.
    ///
    /// Default: "typst-batch/{version}"
    ///
    /// # Example
    ///
    /// ```
    /// use typst_batch::config::ConfigBuilder;
    ///
    /// ConfigBuilder::new()
    ///     .user_agent("my-app/1.0.0")
    ///     .init();
    /// ```
    pub fn user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = Some(agent.into());
        self
    }

    /// Build and initialize the global configuration.
    ///
    /// This can only be called once. Subsequent calls are ignored.
    /// Returns `true` if configuration was set, `false` if already initialized.
    pub fn init(self) -> bool {
        let config = Config {
            user_agent: self
                .user_agent
                .unwrap_or_else(|| Config::default().user_agent),
        };
        CONFIG.set(config).is_ok()
    }
}

/// Initialize typst-batch with default configuration.
///
/// This is equivalent to `ConfigBuilder::new().init()`.
pub fn init_default() -> bool {
    ConfigBuilder::new().init()
}

/// Get the current configuration, or default if not initialized.
pub fn get() -> &'static Config {
    CONFIG.get_or_init(Config::default)
}

/// Global shared package storage - one cache for all compilations.
///
/// Uses the configured User-Agent string. If not configured, uses default.
pub static PACKAGE_STORAGE: OnceLock<PackageStorage> = OnceLock::new();

/// Get or initialize the global package storage.
pub fn package_storage() -> &'static PackageStorage {
    PACKAGE_STORAGE.get_or_init(|| {
        let config = get();
        PackageStorage::new(
            None, // Use default cache path
            None, // Use default package path
            Downloader::new(config.user_agent.clone()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.user_agent.starts_with("typst-batch/"));
    }

    #[test]
    fn test_builder() {
        let builder = ConfigBuilder::new().user_agent("test/1.0");
        assert_eq!(builder.user_agent, Some("test/1.0".to_string()));
    }
}
