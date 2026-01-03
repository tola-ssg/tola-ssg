//! Compilation and asset processing for static site generation.
//!
//! This module orchestrates the build pipeline:
//!
//! - **pages**: Compile `.typ` files to HTML
//! - **meta**: Extract and process page metadata
//! - **assets**: Copy and optimize static assets
//! - **watch**: Incremental builds on file changes
//! - **deps**: Dependency tracking for precise rebuilds
//!
//! # Build Flow
//!
//! ```text
//! collect_pages() ──► compile_pages() ──► process_asset()
//!       │                   │                  │
//!       ▼                   ▼                  ▼
//!   PageMeta[]         HTML files        Asset files
//! ```

pub mod assets;
pub mod bridge;
pub mod deps;
pub mod meta;
pub mod pages;

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::freshness::{self, ContentHash};

// ============================================================================
// Public API
// ============================================================================

pub use assets::process_asset;
pub use assets::process_rel_asset;
pub use pages::drain_warnings;

// ============================================================================
// Shared utilities
// ============================================================================

/// Files to ignore during directory traversal
const IGNORED_FILES: &[&str] = &[".DS_Store"];

/// Collect all files from a directory recursively.
pub fn collect_all_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let name = e.file_name().to_str().unwrap_or_default();
            !IGNORED_FILES.contains(&name)
        })
        .map(walkdir::DirEntry::into_path)
        .collect()
}

