//! # tola_typst
//!
//! Typst integration for the tola static site generator.
//!
//! This crate provides:
//! - [`SystemWorld`] implementation of Typst's `World` trait
//! - Global font discovery and management
//! - Global package resolution
//! - File caching with fingerprint-based invalidation
//! - Diagnostic formatting for compilation errors
//!
//! ## Modules
//!
//! - [`world`]: Typst World implementation
//! - [`font`]: Font discovery and loading
//! - [`package`]: Package resolution
//! - [`library`]: Typst standard library
//! - [`file`]: File caching and access
//! - [`diagnostic`]: Error formatting
//!
//! ## Usage
//!
//! ```ignore
//! use tola_typst::{SystemWorld, get_fonts, GLOBAL_LIBRARY};
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

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod diagnostic;
pub mod file;
pub mod font;
pub mod library;
pub mod package;
pub mod world;

// Re-export main types
pub use diagnostic::{filter_html_warnings, format_diagnostics, has_errors};
pub use file::{
    clear_file_cache, get_accessed_files, is_virtual_path, read_virtual, read_with_global_virtual,
    record_file_access, reset_access_flags, set_virtual_provider, VirtualDataProvider, EMPTY_ID,
    GLOBAL_FILE_CACHE, STDIN_ID,
};
pub use font::get_fonts;
pub use library::GLOBAL_LIBRARY;
pub use package::GLOBAL_PACKAGE_STORAGE;
pub use world::{SystemWorld, TolaWorld};

// Re-export typst types for convenience
pub use typst;
pub use typst_html;
pub use typst_kit;
pub use typst_svg;
