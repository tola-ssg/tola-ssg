//! Configuration error types.

use std::path::PathBuf;
use thiserror::Error;

/// Configuration-related errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error when reading `{0}`")]
    Io(PathBuf, #[source] std::io::Error),

    #[error("Config file parsing error")]
    Toml(#[from] toml::de::Error),

    #[error("Config validation error: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn test_config_error_display() {
        let io_err = ConfigError::Io(
            PathBuf::from("test.toml"),
            Error::new(ErrorKind::NotFound, "file not found"),
        );
        let display = format!("{io_err}");
        assert!(display.contains("IO error"));
        assert!(display.contains("test.toml"));

        let validation_err = ConfigError::Validation("Test validation error".to_string());
        let display = format!("{validation_err}");
        assert!(display.contains("Test validation error"));
    }
}
