//! Unified freshness detection module.
//!
//! Provides two strategies for detecting file freshness:
//!
//! - **Content-hash (blake3)**: For source files where VCS (jj/git) may not update mtimes
//! - **Mtime**: For tola-generated outputs where timestamps are reliable
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        Freshness Detection                              │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                         │
//! │  ┌────────────────────────────────────┐  ┌────────────────────────────┐ │
//! │  │       Content-Hash (blake3)        │  │         Mtime-based        │ │
//! │  │  ────────────────────────────────  │  │  ────────────────────────  │ │
//! │  │  • Source file detection           │  │  • Generated file checks   │ │
//! │  │  • VCS-friendly (jj/git)           │  │  • HTML → SVG compression  │ │
//! │  │  • .typ → .html freshness          │  │  • Output vs output        │ │
//! │  └────────────────────────────────────┘  └────────────────────────────┘ │
//! │                                                                         │
//! │  ┌──────────────┐     ┌──────────────┐     ┌──────────────────────────┐ │
//! │  │ Source File  │────▶│ ContentHash  │────▶│ FreshnessCache (Global)  │ │
//! │  └──────────────┘     └──────────────┘     └──────────────────────────┘ │
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
//! use crate::freshness::mtime::{is_output_fresh, get_mtime};
//!
//! // Content-hash: source file detection
//! if is_fresh(&source_path, &output_path) {
//!     // Skip rebuild
//! }
//!
//! // Mtime: generated file comparison
//! let html_mtime = get_mtime(&html_path);
//! if is_output_fresh(&svg_path, html_mtime) {
//!     // Skip SVG compression
//! }
//!
//! // Clear cache at start of new build
//! clear_cache();
//! ```

mod cache;
mod hash;
pub mod mtime;

pub use cache::clear_cache;
pub use hash::{build_hash_marker, compute_deps_hash, compute_file_hash, is_fresh, ContentHash};
