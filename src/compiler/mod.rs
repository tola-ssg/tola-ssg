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

/// Canonicalize a path, returning original if canonicalization fails.
#[inline]
pub fn canonicalize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Check if destination is up-to-date compared to source and dependencies.
///
/// Uses content-based hashing (blake3) instead of mtime for reliable detection
/// with version control systems like jujutsu that may not update timestamps.
///
/// # Arguments
///
/// * `src` - Source file path
/// * `dst` - Destination/output file path
/// * `deps_hash` - Optional hash of dependencies (templates, config, etc.)
pub fn is_up_to_date(src: &Path, dst: &Path, deps_hash: Option<ContentHash>) -> bool {
    freshness::is_fresh(src, dst, deps_hash)
}
