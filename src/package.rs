//! Global shared package storage.
//!
//! Package downloads and caching are shared across all compilations to avoid
//! redundant network requests and disk I/O.
//!
//! # Package System Overview
//!
//! Typst packages are referenced in documents like:
//! ```typst
//! #import "@preview/cetz:0.3.0": canvas, draw
//! ```
//!
//! When a package is imported:
//! 1. Check if it exists in the local cache
//! 2. If not, download from the Typst package registry
//! 3. Extract and cache for future use
//!
//! # Cache Location
//!
//! Packages are cached at platform-specific locations:
//! - Linux: `~/.cache/typst/packages`
//! - macOS: `~/Library/Caches/typst/packages`
//! - Windows: `%LOCALAPPDATA%\typst\packages`
//!
//! # Thread Safety
//!
//! `PackageStorage` is thread-safe and can be shared across compilations.
//! Downloads are coordinated to prevent duplicate requests.
//!
//! # Configuration
//!
//! Use [`crate::config::ConfigBuilder`] to configure the User-Agent string
//! before first package download.

pub use crate::config::{package_storage, PACKAGE_STORAGE};

/// Backwards-compatible alias for global package storage.
#[deprecated(since = "0.2.0", note = "Use package_storage() instead")]
pub static GLOBAL_PACKAGE_STORAGE: std::sync::LazyLock<&'static typst_kit::package::PackageStorage> =
    std::sync::LazyLock::new(package_storage);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_initialized() {
        let _storage = package_storage();
    }

    #[test]
    fn test_storage_is_shared() {
        let storage1 = package_storage();
        let storage2 = package_storage();
        assert!(std::ptr::eq(storage1, storage2), "Storage should be shared");
    }
}
