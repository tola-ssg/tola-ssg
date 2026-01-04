//! Configuration for typst-batch.
//!
//! This module provides runtime configuration for package downloads and
//! project defaults. Call [`init`] at application startup to configure.

use std::sync::OnceLock;

use typst_kit::download::Downloader;
use typst_kit::package::PackageStorage;

/// Global configuration, initialized via [`init`].
static CONFIG: OnceLock<Config> = OnceLock::new();

/// Runtime configuration for typst-batch.
#[derive(Debug, Clone)]
pub struct Config {
    /// User-Agent string for package downloads.
    /// Example: "my-app/1.0.0"
    pub user_agent: String,

    /// Default project name for generated typst.toml files.
    /// Example: "my-project"
    pub default_project_name: String,

    /// Default entrypoint path for generated typst.toml files.
    /// Example: "main.typ" or "content/index.typ"
    pub default_entrypoint: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            user_agent: concat!("typst-batch/", env!("CARGO_PKG_VERSION")).to_string(),
            default_project_name: "typst-project".to_string(),
            default_entrypoint: "main.typ".to_string(),
        }
    }
}

/// Configuration builder for fluent API.
#[derive(Debug, Clone, Default)]
pub struct ConfigBuilder {
    user_agent: Option<String>,
    default_project_name: Option<String>,
    default_entrypoint: Option<String>,
}

impl ConfigBuilder {
    /// Create a new configuration builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the User-Agent string for package downloads.
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

    /// Set the default project name for generated typst.toml.
    pub fn default_project_name(mut self, name: impl Into<String>) -> Self {
        self.default_project_name = Some(name.into());
        self
    }

    /// Set the default entrypoint for generated typst.toml.
    pub fn default_entrypoint(mut self, path: impl Into<String>) -> Self {
        self.default_entrypoint = Some(path.into());
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
            default_project_name: self
                .default_project_name
                .unwrap_or_else(|| Config::default().default_project_name),
            default_entrypoint: self
                .default_entrypoint
                .unwrap_or_else(|| Config::default().default_entrypoint),
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

/// Generate default typst.toml content based on configuration.
pub fn default_typst_toml() -> Vec<u8> {
    let config = get();
    format!(
        "[package]\nname = \"{}\"\nversion = \"0.0.0\"\nentrypoint = \"{}\"",
        config.default_project_name, config.default_entrypoint
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.user_agent.starts_with("typst-batch/"));
        assert_eq!(config.default_project_name, "typst-project");
        assert_eq!(config.default_entrypoint, "main.typ");
    }

    #[test]
    fn test_builder() {
        let builder = ConfigBuilder::new()
            .user_agent("test/1.0")
            .default_project_name("test-project")
            .default_entrypoint("src/main.typ");

        assert_eq!(builder.user_agent, Some("test/1.0".to_string()));
        assert_eq!(builder.default_project_name, Some("test-project".to_string()));
        assert_eq!(builder.default_entrypoint, Some("src/main.typ".to_string()));
    }
}
