//! File category classification for watch mode and incremental builds.
//!
//! This module provides utilities to categorize files by their role in the build process,
//! determining whether changes require full rebuilds or incremental updates.
//!
//! # File Categories
//!
//! | Category   | Rebuild Strategy      | Example Files                |
//! |------------|-----------------------|------------------------------|
//! | Content    | Incremental (single)  | `content/*.typ`              |
//! | Asset      | Incremental (single)  | `assets/images/*`, `*.css`   |
//! | Config     | Full rebuild          | `tola.toml`                  |
//! | Template   | Full rebuild          | `templates/*.typ`            |
//! | Utils      | Full rebuild          | `utils/*.typ`                |
//! | Unknown    | Ignored               | Files outside watched dirs   |
//!
//! # Incremental Build Logic
//!
//! A content file needs rebuilding when:
//! 1. Source `.typ` is newer than output `.html`, OR
//! 2. Any dependency (config/templates/utils) is newer than output

use crate::config::SiteConfig;
use std::path::{Path, PathBuf};

/// Category of a changed file, used to determine rebuild strategy in watch mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCategory {
    /// Content file (.typ) - can be rebuilt individually
    Content,
    /// Asset file - can be copied individually
    Asset,
    /// Site configuration (tola.toml) - requires full site rebuild
    Config,
    /// Dependency file (templates, utils, etc.) - requires rebuilding dependent content
    Deps,
    /// File outside watched directories
    Unknown,
}

impl FileCategory {
    /// Get the short name for this category (used in logs)
    pub const fn name(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Asset => "assets",
            Self::Config => "config",
            Self::Deps => "deps",
            Self::Unknown => "unknown",
        }
    }

    /// Get the directory paths for this category from config.
    /// Returns Vec to support multiple deps directories.
    pub fn paths(self, config: &SiteConfig) -> Vec<PathBuf> {
        match self {
            Self::Content => vec![config.build.content.clone()],
            Self::Asset => vec![config.build.assets.clone()],
            Self::Config => vec![config.config_path.clone()],
            Self::Deps => config.build.deps.clone(),
            Self::Unknown => vec![],
        }
    }

    /// Returns true if this category represents a directory (vs a single file)
    pub const fn is_directory(self) -> bool {
        matches!(self, Self::Content | Self::Asset | Self::Deps)
    }
}

/// Categorize a file path to determine how changes should be handled.
///
/// Used by the file watcher to decide between incremental and full rebuilds:
/// - `Content`/`Asset`: Process only the changed file
/// - `Config`/`Deps`: Trigger rebuild of dependent content files
/// - `Unknown`: Ignored
pub fn categorize_path(path: &Path, config: &SiteConfig) -> FileCategory {
    let path = normalize_path(path);

    if path == config.config_path {
        FileCategory::Config
    } else if config.build.deps.iter().any(|dep| path.starts_with(dep)) {
        FileCategory::Deps
    } else if path.starts_with(&config.build.content) {
        FileCategory::Content
    } else if path.starts_with(&config.build.assets) {
        FileCategory::Asset
    } else {
        FileCategory::Unknown
    }
}

// normalize_path is now in crate::utils::path
use super::path::normalize_path;


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ========================================================================
    // FileCategory Tests
    // ========================================================================

    #[test]
    fn test_category_name() {
        assert_eq!(FileCategory::Content.name(), "content");
        assert_eq!(FileCategory::Asset.name(), "assets");
        assert_eq!(FileCategory::Config.name(), "config");
        assert_eq!(FileCategory::Deps.name(), "deps");
        assert_eq!(FileCategory::Unknown.name(), "unknown");
    }

    #[test]
    fn test_is_directory() {
        // Directory-based categories
        assert!(FileCategory::Content.is_directory());
        assert!(FileCategory::Asset.is_directory());
        assert!(FileCategory::Deps.is_directory());

        // Single file or unknown
        assert!(!FileCategory::Config.is_directory());
        assert!(!FileCategory::Unknown.is_directory());
    }

    // ========================================================================
    // normalize_path Tests
    // ========================================================================

    #[test]
    fn test_normalize_path_absolute() {
        let path = Path::new("/absolute/path/file.txt");
        let normalized = normalize_path(path);
        assert!(normalized.is_absolute());
    }

    #[test]
    fn test_normalize_path_relative() {
        let path = Path::new("relative/path/file.txt");
        let normalized = normalize_path(path);
        // Should be converted to absolute (joined with cwd)
        assert!(normalized.is_absolute());
    }
}
