//! `[serve]` section configuration.
//!
//! Contains development server settings.

use super::defaults;
use educe::Educe;
use serde::{Deserialize, Serialize};

/// `[serve]` section in tola.toml - development server settings.
///
/// # Example
/// ```toml
/// [serve]
/// interface = "0.0.0.0"  # Listen on all interfaces
/// port = 3000
/// watch = true           # Auto-rebuild on file changes
/// ```
#[derive(Debug, Clone, Educe, Serialize, Deserialize)]
#[educe(Default)]
#[serde(deny_unknown_fields)]
pub struct ServeConfig {
    /// Network interface to bind.
    /// - `127.0.0.1` (default): localhost only
    /// - `0.0.0.0`: all interfaces (LAN accessible)
    #[serde(default = "defaults::serve::interface")]
    #[educe(Default = defaults::serve::interface())]
    pub interface: String,

    /// HTTP port number (default: 5277).
    #[serde(default = "defaults::serve::port")]
    #[educe(Default = defaults::serve::port())]
    pub port: u16,

    /// Enable file watcher for live reload on changes.
    #[serde(default = "defaults::r#true")]
    #[educe(Default = true)]
    pub watch: bool,
}

#[cfg(test)]
mod tests {
    use super::super::SiteConfig;

    #[test]
    fn test_serve_config() {
        let config = r#"
            [base]
            title = "Test"
            description = "Test blog"

            [serve]
            interface = "0.0.0.0"
            port = 8080
            watch = false
        "#;
        let config: SiteConfig = toml::from_str(config).unwrap();

        assert_eq!(config.serve.interface, "0.0.0.0");
        assert_eq!(config.serve.port, 8080);
        assert!(!config.serve.watch);
    }

    #[test]
    fn test_serve_config_defaults() {
        let config = r#"
            [base]
            title = "Test"
            description = "Test blog"
        "#;
        let config: SiteConfig = toml::from_str(config).unwrap();

        assert_eq!(config.serve.interface, "127.0.0.1");
        assert_eq!(config.serve.port, 5277);
        assert!(config.serve.watch);
    }

    #[test]
    fn test_unknown_field_rejection() {
        let config = r#"
            [base]
            title = "Test"
            description = "Test blog"

            [serve]
            unknown_field = "should_fail"
        "#;
        let result: Result<SiteConfig, _> = toml::from_str(config);

        assert!(result.is_err());
    }

    #[test]
    fn test_serve_config_interface_variants() {
        // Test IPv4 any
        let config = r#"
            [base]
            title = "Test"
            description = "Test"
            [serve]
            interface = "0.0.0.0"
        "#;
        let config: SiteConfig = toml::from_str(config).unwrap();
        assert_eq!(config.serve.interface, "0.0.0.0");

        // Test IPv6 localhost
        let config = r#"
            [base]
            title = "Test"
            description = "Test"
            [serve]
            interface = "::1"
        "#;
        let config: SiteConfig = toml::from_str(config).unwrap();
        assert_eq!(config.serve.interface, "::1");
    }

    #[test]
    fn test_serve_config_port_range() {
        // Test minimum port
        let config = r#"
            [base]
            title = "Test"
            description = "Test"
            [serve]
            port = 1
        "#;
        let config: SiteConfig = toml::from_str(config).unwrap();
        assert_eq!(config.serve.port, 1);

        // Test maximum port
        let config = r#"
            [base]
            title = "Test"
            description = "Test"
            [serve]
            port = 65535
        "#;
        let config: SiteConfig = toml::from_str(config).unwrap();
        assert_eq!(config.serve.port, 65535);
    }

    #[test]
    fn test_serve_config_watch_disabled() {
        let config = r#"
            [base]
            title = "Test"
            description = "Test"
            [serve]
            watch = false
        "#;
        let config: SiteConfig = toml::from_str(config).unwrap();
        assert!(!config.serve.watch);
    }

    #[test]
    fn test_serve_config_partial_override() {
        let config = r#"
            [base]
            title = "Test"
            description = "Test"
            [serve]
            port = 3000
        "#;
        let config: SiteConfig = toml::from_str(config).unwrap();

        // port is overridden
        assert_eq!(config.serve.port, 3000);
        // interface uses default
        assert_eq!(config.serve.interface, "127.0.0.1");
        // watch uses default
        assert!(config.serve.watch);
    }
}
