//! # typst-batch
//!
//! A Typst compilation library with shared global resources for batch processing.
//!
//! This crate provides a [`World`](typst::World) implementation optimized for
//! compiling multiple Typst documents efficiently by sharing:
//!
//! - **Fonts**: Loaded once and shared across all compilations
//! - **Packages**: Downloaded once and cached globally
//! - **File cache**: Fingerprint-based invalidation for incremental builds
//! - **Standard library**: Shared with HTML feature enabled
//!
//! ## Quick Start
//!
//! ```ignore
//! use typst_batch::{SystemWorld, get_fonts};
//! use std::path::Path;
//!
//! // Initialize fonts (once at startup)
//! let fonts = get_fonts(&[Path::new("assets/fonts")]);
//!
//! // Create a world for compilation
//! let world = SystemWorld::new(
//!     Path::new("content/index.typ"),
//!     Path::new("."),
//! );
//!
//! // Compile with typst
//! let result = typst::compile(&world);
//! ```
//!
//! ## Modules
//!
//! - [`config`]: Runtime configuration (User-Agent for package downloads)
//! - [`world`]: Typst World implementation
//! - [`font`]: Font discovery and loading
//! - [`package`]: Package resolution
//! - [`library`]: Typst standard library
//! - [`file`]: File caching and virtual file support
//! - [`diagnostic`]: Error formatting

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
pub mod diagnostic;
pub mod file;
pub mod font;
pub mod library;
pub mod package;
pub mod world;

// Re-export main types
pub use config::{Config, ConfigBuilder};
pub use diagnostic::{filter_html_warnings, format_diagnostics, has_errors};
pub use file::{
    clear_file_cache, get_accessed_files, is_virtual_path, read_virtual, read_with_global_virtual,
    record_file_access, reset_access_flags, set_virtual_provider, VirtualDataProvider, EMPTY_ID,
    GLOBAL_FILE_CACHE, STDIN_ID,
};
pub use font::get_fonts;
pub use library::GLOBAL_LIBRARY;
pub use world::SystemWorld;

// Re-export typst types for convenience
pub use typst;
pub use typst_html;
pub use typst_kit;
pub use typst_svg;
