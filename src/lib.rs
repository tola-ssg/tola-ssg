//! # typst-batch
//!
//! A Typst → HTML batch compilation library with shared global resources.
//!
//! This crate was created for [tola](https://github.com/tola-ssg/tola-ssg),
//! a Typst-based static site generator. It provides optimized batch compilation
//! by sharing expensive resources across multiple document compilations:
//!
//! - **Fonts**: Loaded once (~100ms saved per compilation)
//! - **Packages**: Downloaded once and cached globally
//! - **File cache**: Fingerprint-based invalidation for incremental builds
//! - **Standard library**: Shared instance with HTML feature enabled
//!
//! ## Note
//!
//! This library is specifically designed for **Typst → HTML** workflows.
//! If you need PDF output or other formats, consider using typst directly
//! or the official typst-cli.
//!
//! ## Quick Start
//!
//! ```ignore
//! use typst_batch::{compile_html, get_fonts};
//! use std::path::Path;
//!
//! // Initialize fonts once at startup
//! get_fonts(&[]);
//!
//! // Compile a single file
//! let result = compile_html(Path::new("doc.typ"), Path::new("."))?;
//! std::fs::write("output.html", &result.html)?;
//!
//! // Compile with metadata extraction
//! // In your .typ file: #metadata((title: "Hello")) <post-meta>
//! let result = compile_html_with_metadata(
//!     Path::new("post.typ"),
//!     Path::new("."),
//!     "post-meta",  // label name
//! )?;
//! println!("Title: {:?}", result.metadata);
//! ```
//!
//! ## High-Level API
//!
//! For most use cases, use the high-level functions:
//!
//! - [`compile_html`]: Compile to HTML bytes
//! - [`compile_html_with_metadata`]: Compile to HTML with metadata extraction
//! - [`compile_document`]: Compile to HtmlDocument (for further processing)
//! - [`query_metadata`]: Extract metadata from a compiled document
//!
//! ## Low-Level API
//!
//! For advanced use cases, access the underlying modules:
//!
//! - [`config`]: Runtime configuration (User-Agent for package downloads)
//! - [`world`]: Typst World implementation
//! - [`font`]: Font discovery and loading
//! - [`mod@file`]: File caching and virtual file support
//! - [`diagnostic`]: Error formatting

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod compile;
pub mod config;
pub mod diagnostic;
pub mod file;
pub mod font;
pub mod library;
pub mod package;
pub mod world;

// =============================================================================
// Prelude - import commonly used items with a single `use`
// =============================================================================

/// Prelude module for convenient imports.
///
/// Import everything commonly needed with:
///
/// ```ignore
/// use typst_batch::prelude::*;
/// ```
///
/// This includes:
/// - Compilation functions: `compile_html`, `compile_document`, etc.
/// - Diagnostics: `DiagnosticsExt`, `CompileError`
/// - VFS: `VirtualFileSystem`, `MapVirtualFS`, `set_virtual_fs`
/// - Fonts: `get_fonts`, `FontOptions`
/// - World: `SystemWorld`
pub mod prelude {
    // Re-export common items from the crate root
    // (avoids duplication - these are already exported at crate level)

    // Compilation
    pub use crate::{
        compile_document, compile_document_with_inputs, compile_document_with_metadata,
        compile_html, compile_html_with_inputs, compile_html_with_inputs_dict,
        compile_html_with_metadata, query_metadata, query_metadata_map, DocumentResult,
        HtmlResult,
    };

    // Diagnostics
    pub use crate::{
        CompileError, DiagnosticFilter, DiagnosticInfo, DiagnosticOptions,
        DiagnosticSummary, DiagnosticsExt, DisplayStyle, SourceLine, TraceInfo,
    };

    // VFS
    pub use crate::{
        file_id, file_id_from_path, set_virtual_fs, virtual_file_id, MapVirtualFS,
        NoVirtualFS, VirtualFileSystem,
    };

    // Fonts
    pub use crate::{get_fonts, init_fonts_with_options, FontOptions};

    // Library
    pub use crate::create_library_with_inputs;

    // World
    pub use crate::SystemWorld;
}

// =============================================================================
// High-Level API (recommended for most use cases)
// =============================================================================

pub use compile::{
    compile_document, compile_document_with_inputs, compile_document_with_metadata,
    compile_html, compile_html_with_inputs, compile_html_with_inputs_dict,
    compile_html_with_metadata, query_metadata, query_metadata_map, DocumentResult,
    DocumentWithMetadataResult, HtmlResult, HtmlWithMetadataResult,
};

// =============================================================================
// Diagnostics
// =============================================================================

pub use diagnostic::{
    // Error type
    CompileError,
    // Options for formatting
    DiagnosticOptions, DisplayStyle,
    // Filtering
    DiagnosticFilter,
    // Summary and extension trait (use .format(), .resolve(), etc.)
    DiagnosticSummary, DiagnosticsExt,
    // Structured data for custom rendering
    DiagnosticInfo, SourceLine, TraceInfo,
};

// =============================================================================
// Infrastructure
// =============================================================================

pub use config::{Config, ConfigBuilder};
pub use file::{
    clear_file_cache, file_id, file_id_from_path, get_accessed_files, is_virtual_path, read_virtual,
    read_with_global_virtual, record_file_access, reset_access_flags, set_virtual_fs,
    virtual_file_id, MapVirtualFS, NoVirtualFS, VirtualFileSystem, EMPTY_ID, GLOBAL_FILE_CACHE,
    STDIN_ID,
};
pub use font::{
    font_count, font_family_count, fonts_initialized, get_fonts, init_fonts_with_options,
    FontOptions,
};
pub use library::{create_library_with_inputs, GLOBAL_LIBRARY};
pub use world::SystemWorld;

// =============================================================================
// Re-export typst crates for advanced use
// =============================================================================

/// Full typst crate for advanced/custom compilation workflows.
pub use typst;

/// typst-html crate for HTML rendering.
pub use typst_html;

/// typst-kit for font/package utilities.
pub use typst_kit;
