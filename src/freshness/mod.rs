//! Content-based freshness detection using blake3 hashing.
//!
//! This module provides unified file freshness detection that works reliably
//! with version control systems like jujutsu (jj) where file modification times
//! may not change when switching commits.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        Freshness Detection                               │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                          │
//! │  ┌──────────────┐     ┌──────────────┐     ┌──────────────────────────┐ │
//! │  │ Source File  │────▶│ ContentHash  │────▶│ FreshnessCache (Global)  │ │
//! │  └──────────────┘     └──────────────┘     └──────────────────────────┘ │
//! │                              │                         │                 │
//! │                              ▼                         ▼                 │
//! │                       ┌─────────────┐          ┌─────────────┐          │
//! │                       │ blake3 hash │          │ HashMap<    │          │
//! │                       │ (streaming) │          │   Path,     │          │
//! │                       └─────────────┘          │   Hash      │          │
//! │                                                │ >           │          │
//! │                                                └─────────────┘          │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Key Features
//!
//! - **Content-based**: Detects actual file changes, not just timestamps
//! - **VCS-friendly**: Works with jj, git worktrees, and other tools that preserve mtimes
//! - **Streaming**: Memory-efficient hashing for large files
//! - **Cached**: Avoids redundant hash computations within a build
//!
//! # Usage
//!
//! ```ignore
//! use crate::freshness::{is_fresh, compute_deps_hash, clear_cache};
//!
//! // Check if output is fresh relative to source
//! if is_fresh(&source_path, &output_path) {
//!     // Skip rebuild
//! }
//!
//! // Get combined hash of all dependency files
//! let deps_hash = compute_deps_hash(&config);
//!
//! // Clear cache at start of new build
//! clear_cache();
//! ```

mod cache;
mod hash;

pub use cache::clear_cache;
pub use hash::{build_hash_marker, compute_deps_hash, compute_file_hash, is_fresh, ContentHash};
