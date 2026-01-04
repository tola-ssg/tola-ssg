//! High-level compilation API for Typst to HTML.
//!
//! This module provides convenient functions for batch compilation workflows.
//!
//! # Example
//!
//! ```ignore
//! use typst_batch::{compile_html, get_fonts};
//! use std::path::Path;
//!
//! // Initialize fonts once at startup
//! get_fonts(&[]);
//!
//! // Compile a file to HTML
//! let result = compile_html(Path::new("doc.typ"), Path::new(".")).unwrap();
//! println!("HTML: {} bytes", result.html.len());
//!
//! // With metadata extraction
//! let result = compile_html_with_metadata(
//!     Path::new("doc.typ"),
//!     Path::new("."),
//!     "my-meta",  // label name in typst: #metadata(...) <my-meta>
//! ).unwrap();
//! if let Some(meta) = result.metadata {
//!     println!("Title: {:?}", meta.get("title"));
//! }
//! ```

use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;
use typst::foundations::{Label, Selector};
use typst::introspection::MetadataElem;
use typst::utils::PicoStr;
use typst::Document;
use typst_html::HtmlDocument;

use crate::diagnostic::{filter_html_warnings, format_diagnostics, has_errors};
use crate::file::{get_accessed_files, reset_access_flags};
use crate::world::SystemWorld;

// =============================================================================
// Result Types
// =============================================================================

/// Result of HTML compilation.
#[derive(Debug)]
pub struct HtmlResult {
    /// Compiled HTML content as bytes.
    pub html: Vec<u8>,
    /// Files accessed during compilation (relative to root).
    pub accessed_files: Vec<PathBuf>,
    /// Formatted warnings (if any).
    pub warnings: Option<String>,
}

/// Result of HTML compilation with metadata extraction.
#[derive(Debug)]
pub struct HtmlWithMetadataResult {
    /// Compiled HTML content as bytes.
    pub html: Vec<u8>,
    /// Extracted metadata as JSON (if label found).
    pub metadata: Option<JsonValue>,
    /// Files accessed during compilation (relative to root).
    pub accessed_files: Vec<PathBuf>,
    /// Formatted warnings (if any).
    pub warnings: Option<String>,
}

/// Result of document compilation (without HTML serialization).
#[derive(Debug)]
pub struct DocumentResult {
    /// The compiled HTML document (for further processing).
    pub document: HtmlDocument,
    /// Files accessed during compilation (relative to root).
    pub accessed_files: Vec<PathBuf>,
    /// Formatted warnings (if any).
    pub warnings: Option<String>,
}

/// Result of document compilation with metadata.
#[derive(Debug)]
pub struct DocumentWithMetadataResult {
    /// The compiled HTML document (for further processing).
    pub document: HtmlDocument,
    /// Extracted metadata as JSON (if label found).
    pub metadata: Option<JsonValue>,
    /// Files accessed during compilation (relative to root).
    pub accessed_files: Vec<PathBuf>,
    /// Formatted warnings (if any).
    pub warnings: Option<String>,
}

// =============================================================================
// Compilation Functions
// =============================================================================

/// Compile a Typst file to HTML bytes.
///
/// This is the simplest API for getting HTML output from a Typst file.
///
/// # Arguments
///
/// * `path` - Path to the .typ file to compile
/// * `root` - Project root directory (for resolving imports)
///
/// # Returns
///
/// Returns `HtmlResult` containing the HTML bytes and accessed files.
///
/// # Example
///
/// ```ignore
/// let result = compile_html(Path::new("doc.typ"), Path::new("."))?;
/// std::fs::write("output.html", &result.html)?;
/// ```
pub fn compile_html(path: &Path, root: &Path) -> anyhow::Result<HtmlResult> {
    let (document, accessed_files, warnings) = compile_document_internal(path, root)?;

    let html = typst_html::html(&document)
        .map_err(|e| anyhow::anyhow!("HTML export failed: {e:?}"))?
        .into_bytes();

    Ok(HtmlResult {
        html,
        accessed_files,
        warnings,
    })
}

/// Compile a Typst file to HTML bytes with metadata extraction.
///
/// Extracts metadata from a labeled metadata element in the document.
/// In your Typst file, use: `#metadata((...)) <label-name>`
///
/// # Arguments
///
/// * `path` - Path to the .typ file to compile
/// * `root` - Project root directory
/// * `label` - The label name to query (without angle brackets)
///
/// # Example
///
/// ```ignore
/// // In your .typ file:
/// // #metadata((title: "My Post", date: "2024-01-01")) <post-meta>
///
/// let result = compile_html_with_metadata(
///     Path::new("post.typ"),
///     Path::new("."),
///     "post-meta",
/// )?;
/// ```
pub fn compile_html_with_metadata(
    path: &Path,
    root: &Path,
    label: &str,
) -> anyhow::Result<HtmlWithMetadataResult> {
    let (document, accessed_files, warnings) = compile_document_internal(path, root)?;

    let metadata = query_metadata(&document, label);

    let html = typst_html::html(&document)
        .map_err(|e| anyhow::anyhow!("HTML export failed: {e:?}"))?
        .into_bytes();

    Ok(HtmlWithMetadataResult {
        html,
        metadata,
        accessed_files,
        warnings,
    })
}

/// Compile a Typst file to HtmlDocument (without serializing to bytes).
///
/// Use this when you need to process the document further (e.g., with tola-vdom).
///
/// # Arguments
///
/// * `path` - Path to the .typ file to compile
/// * `root` - Project root directory
pub fn compile_document(path: &Path, root: &Path) -> anyhow::Result<DocumentResult> {
    let (document, accessed_files, warnings) = compile_document_internal(path, root)?;

    Ok(DocumentResult {
        document,
        accessed_files,
        warnings,
    })
}

/// Compile a Typst file to HtmlDocument with metadata extraction.
///
/// Use this when you need both the document for further processing and metadata.
pub fn compile_document_with_metadata(
    path: &Path,
    root: &Path,
    label: &str,
) -> anyhow::Result<DocumentWithMetadataResult> {
    let (document, accessed_files, warnings) = compile_document_internal(path, root)?;

    let metadata = query_metadata(&document, label);

    Ok(DocumentWithMetadataResult {
        document,
        metadata,
        accessed_files,
        warnings,
    })
}

// =============================================================================
// Metadata Query
// =============================================================================

/// Query metadata from a compiled document by label name.
///
/// In Typst, you can attach metadata to a label like this:
/// ```typst
/// #metadata((title: "Hello", author: "Alice")) <my-meta>
/// ```
///
/// Then query it:
/// ```ignore
/// let meta = query_metadata(&document, "my-meta");
/// // Returns: Some({"title": "Hello", "author": "Alice"})
/// ```
///
/// # Arguments
///
/// * `document` - The compiled HtmlDocument
/// * `label` - The label name (without angle brackets)
///
/// # Returns
///
/// Returns `Some(JsonValue)` if the label exists and contains valid metadata,
/// `None` otherwise.
pub fn query_metadata(document: &HtmlDocument, label: &str) -> Option<JsonValue> {
    let label = Label::new(PicoStr::intern(label))?;
    let introspector = document.introspector();
    let elem = introspector.query_unique(&Selector::Label(label)).ok()?;

    elem.to_packed::<MetadataElem>()
        .and_then(|meta| serde_json::to_value(&meta.value).ok())
}

// =============================================================================
// Internal Helpers
// =============================================================================

/// Core compilation logic.
fn compile_document_internal(
    path: &Path,
    root: &Path,
) -> anyhow::Result<(HtmlDocument, Vec<PathBuf>, Option<String>)> {
    reset_access_flags();

    let world = SystemWorld::new(path, root);
    let result = typst::compile(&world);

    // Check for errors in warnings
    if has_errors(&result.warnings) {
        let formatted = format_diagnostics(&world, &result.warnings);
        anyhow::bail!("Typst compilation warnings:\n{formatted}");
    }

    // Extract document or format errors
    let document = result.output.map_err(|errors| {
        let all_diags: Vec<_> = errors.iter().chain(&result.warnings).cloned().collect();
        let filtered = filter_html_warnings(&all_diags);
        let formatted = format_diagnostics(&world, &filtered);
        anyhow::anyhow!("Typst compilation failed:\n{formatted}")
    })?;

    // Collect accessed files
    let accessed_files = collect_accessed_files(root);

    // Format warnings
    let filtered_warnings = filter_html_warnings(&result.warnings);
    let warnings = if filtered_warnings.is_empty() {
        None
    } else {
        Some(format_diagnostics(&world, &filtered_warnings))
    };

    Ok((document, accessed_files, warnings))
}

/// Collect accessed files relative to root.
fn collect_accessed_files(root: &Path) -> Vec<PathBuf> {
    get_accessed_files()
        .into_iter()
        .filter(|id| id.package().is_none()) // Skip package files
        .filter_map(|id| id.vpath().resolve(root))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_metadata_not_found() {
        // This would require a compiled document, skip for now
    }
}
