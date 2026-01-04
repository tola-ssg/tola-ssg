//! # typst-batch
//!
//! A Typst → HTML batch compilation library with shared global resources.
//!
//! This crate was created for [tola](https://github.com/aspect-rs/tola-ssg),
//! a Typst-based static site generator. It provides optimized batch compilation
//! by sharing expensive resources across multiple document compilations:
//!
//! - **Fonts**: Loaded once (~100ms saved per compilation)
//! - **Packages**: Downloaded once and cached globally
//! - **File cache**: Fingerprint-based invalidation for incremental builds
//! - **Standard library**: Shared instance with HTML feature enabled
//!
//! ## ⚠️ Scope Note
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
//! - [`file`]: File caching and virtual file support
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
// High-Level API (recommended for most use cases)
// =============================================================================

pub use compile::{
    compile_document, compile_document_with_metadata, compile_html, compile_html_with_metadata,
    query_metadata, query_metadata_map, DocumentResult, DocumentWithMetadataResult, HtmlResult,
    HtmlWithMetadataResult,
};

// =============================================================================
// Diagnostics
// =============================================================================

pub use diagnostic::{
    // Error type
    CompileError,
    // Options for formatting
    DiagnosticOptions, DisplayStyle,
    // Re-exported from typst for convenience
    DiagnosticSeverity, SourceDiagnostic,
    // Summary
    DiagnosticSummary,
    // Formatting functions
    filter_html_warnings, format_diagnostics, format_diagnostics_with_options,
    // Utilities
    count_diagnostics, has_errors,
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
pub use library::GLOBAL_LIBRARY;
pub use world::SystemWorld;

// =============================================================================
// Core typst types for VFS and file handling
// =============================================================================

/// File identifier used throughout Typst's file system.
///
/// `FileId` uniquely identifies files within a compilation. It combines:
/// - An optional `PackageSpec` for package files
/// - A `VirtualPath` for the file's location within the project/package
///
/// Use [`FileId::new`] to create IDs for your files, or [`FileId::new_fake`]
/// for dynamically generated content.
///
/// # Example
///
/// ```ignore
/// use typst_batch::{FileId, VirtualPath};
///
/// // Create a FileId for a project file
/// let id = FileId::new(None, VirtualPath::new("/content/post.typ"));
///
/// // Create a fake FileId for virtual content
/// let virtual_id = FileId::new_fake(VirtualPath::new("<generated>"));
/// ```
pub use typst::syntax::FileId;

/// Virtual path within a project or package.
///
/// Virtual paths always start with `/` and represent paths relative to
/// a project or package root. They're platform-independent (always use `/`).
///
/// # Example
///
/// ```ignore
/// use typst_batch::VirtualPath;
/// use std::path::Path;
///
/// // Create from a string path
/// let vpath = VirtualPath::new("/content/post.typ");
///
/// // Create from a real path relative to root
/// let vpath = VirtualPath::within_root(
///     Path::new("/project/content/post.typ"),
///     Path::new("/project"),
/// );
///
/// // Resolve back to filesystem path
/// let real_path = vpath.resolve(Path::new("/project"));
/// ```
pub use typst::syntax::VirtualPath;

/// Parsed Typst source file.
///
/// Contains the source text along with its parsed AST. Used for diagnostics
/// and incremental compilation.
pub use typst::syntax::Source;

/// Result type for file operations in Typst.
pub use typst::diag::FileResult;

/// Error type for file operations in Typst.
pub use typst::diag::FileError;

/// Package specification for Typst packages.
///
/// Used to identify packages in the format `@namespace/name:version`.
pub use typst::syntax::package::PackageSpec;

/// Raw bytes container used for binary files.
pub use typst::foundations::Bytes;

// =============================================================================
// Font types
// =============================================================================

/// Font metadata and lookup index.
///
/// The `FontBook` indexes all available fonts and provides lookup by family
/// name, variant, and other properties. Use this to query available fonts.
///
/// # Example
///
/// ```ignore
/// use typst_batch::get_fonts;
///
/// let fonts = get_fonts(&[]);
/// let book = &fonts.1;
///
/// // List all font families
/// for family in book.families() {
///     println!("Font family: {}", family);
/// }
/// ```
pub use typst::text::FontBook;

/// Information about a single font face.
///
/// Contains family name, variant (weight, style, stretch), and other metadata.
pub use typst::text::FontInfo;

/// Loaded font face ready for rendering.
pub use typst::text::Font;

// =============================================================================
// Re-export typst crates for advanced use
// =============================================================================

/// Full typst crate for advanced/custom compilation workflows.
pub use typst;

/// typst-html crate for HTML rendering.
pub use typst_html;

/// typst-kit for font/package utilities.
pub use typst_kit;

/// typst-svg for SVG rendering (used internally for math/graphics).
pub use typst_svg;
