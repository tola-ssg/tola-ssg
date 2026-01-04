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
//! let result = compile_html(Path::new("doc.typ"), Path::new("."))?;
//! println!("HTML: {} bytes", result.html.len());
//!
//! // Access diagnostics (warnings)
//! for diag in &result.diagnostics {
//!     println!("Warning: {}", diag.message);
//! }
//!
//! // With metadata extraction
//! let result = compile_html_with_metadata(
//!     Path::new("doc.typ"),
//!     Path::new("."),
//!     "my-meta",  // label name in typst: #metadata(...) <my-meta>
//! )?;
//! if let Some(meta) = result.metadata {
//!     println!("Title: {:?}", meta.get("title"));
//! }
//! ```
//!
//! # sys.inputs Support
//!
//! For documents that need `sys.inputs`, use [`compile_html_with_inputs`]:
//!
//! ```ignore
//! let result = compile_html_with_inputs(
//!     Path::new("doc.typ"),
//!     Path::new("."),
//!     [("title", "Hello"), ("author", "Alice")],
//! )?;
//! ```

use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;
use typst::diag::SourceDiagnostic;
use typst::foundations::{Dict, Label, Selector};
use typst::introspection::MetadataElem;
use typst::utils::PicoStr;
use typst::Document;
use typst_html::HtmlDocument;

use crate::diagnostic::{filter_html_warnings, has_errors, CompileError};
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
    /// Compilation diagnostics (warnings only - errors cause Err return).
    ///
    /// Use [`crate::DiagnosticsExt::format`] to format for display.
    pub diagnostics: Vec<SourceDiagnostic>,
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
    /// Compilation diagnostics (warnings only - errors cause Err return).
    pub diagnostics: Vec<SourceDiagnostic>,
}

/// Result of document compilation (without HTML serialization).
#[derive(Debug)]
pub struct DocumentResult {
    /// The compiled HTML document (for further processing).
    pub document: HtmlDocument,
    /// Files accessed during compilation (relative to root).
    pub accessed_files: Vec<PathBuf>,
    /// Compilation diagnostics (warnings only - errors cause Err return).
    pub diagnostics: Vec<SourceDiagnostic>,
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
    /// Compilation diagnostics (warnings only - errors cause Err return).
    pub diagnostics: Vec<SourceDiagnostic>,
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
pub fn compile_html(path: &Path, root: &Path) -> Result<HtmlResult, CompileError> {
    let (document, accessed_files, diagnostics) = compile_document_internal(path, root)?;

    let html = typst_html::html(&document)
        .map_err(|e| CompileError::html_export(format!("{e:?}")))?
        .into_bytes();

    Ok(HtmlResult {
        html,
        accessed_files,
        diagnostics,
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
) -> Result<HtmlWithMetadataResult, CompileError> {
    let (document, accessed_files, diagnostics) = compile_document_internal(path, root)?;

    let metadata = query_metadata(&document, label);

    let html = typst_html::html(&document)
        .map_err(|e| CompileError::html_export(format!("{e:?}")))?
        .into_bytes();

    Ok(HtmlWithMetadataResult {
        html,
        metadata,
        accessed_files,
        diagnostics,
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
pub fn compile_document(path: &Path, root: &Path) -> Result<DocumentResult, CompileError> {
    let (document, accessed_files, diagnostics) = compile_document_internal(path, root)?;

    Ok(DocumentResult {
        document,
        accessed_files,
        diagnostics,
    })
}

/// Compile a Typst file to HtmlDocument with metadata extraction.
///
/// Use this when you need both the document for further processing and metadata.
pub fn compile_document_with_metadata(
    path: &Path,
    root: &Path,
    label: &str,
) -> Result<DocumentWithMetadataResult, CompileError> {
    let (document, accessed_files, diagnostics) = compile_document_internal(path, root)?;

    let metadata = query_metadata(&document, label);

    Ok(DocumentWithMetadataResult {
        document,
        metadata,
        accessed_files,
        diagnostics,
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

/// Query multiple metadata labels from a compiled document.
///
/// This is useful when you have multiple metadata elements in a document.
///
/// # Example
///
/// ```typst
/// #metadata((title: "My Post")) <post-meta>
/// #metadata((author: "Alice", bio: "...")) <author-meta>
/// ```
///
/// ```ignore
/// let meta = query_metadata_map(&document, &["post-meta", "author-meta"]);
/// // Returns: {"post-meta": {"title": "My Post"}, "author-meta": {"author": "Alice", ...}}
/// ```
///
/// # Arguments
///
/// * `document` - The compiled HtmlDocument
/// * `labels` - Slice of label names to query
///
/// # Returns
///
/// Returns a map from label name to metadata value. Labels not found are omitted.
pub fn query_metadata_map<'a>(
    document: &HtmlDocument,
    labels: impl IntoIterator<Item = &'a str>,
) -> serde_json::Map<String, JsonValue> {
    let mut result = serde_json::Map::new();

    for label in labels {
        if let Some(value) = query_metadata(document, label) {
            result.insert(label.to_string(), value);
        }
    }

    result
}

// =============================================================================
// Compilation with sys.inputs
// =============================================================================

/// Compile a Typst file to HTML bytes with custom `sys.inputs`.
///
/// This allows passing document-specific data accessible via `sys.inputs`
/// in the Typst document.
///
/// # Arguments
///
/// * `path` - Path to the .typ file to compile
/// * `root` - Project root directory
/// * `inputs` - Key-value pairs accessible as `sys.inputs`
///
/// # Example
///
/// ```ignore
/// let result = compile_html_with_inputs(
///     Path::new("doc.typ"),
///     Path::new("."),
///     [("title", "Hello"), ("author", "Alice")],
/// )?;
/// ```
///
/// In your Typst document:
/// ```typst
/// #let title = sys.inputs.at("title", default: "Untitled")
/// = #title
/// ```
///
/// # Performance Note
///
/// This creates a new library instance per compilation. For batch compilation
/// without inputs, use [`compile_html`] which shares the global library.
pub fn compile_html_with_inputs<I, K, V>(
    path: &Path,
    root: &Path,
    inputs: I,
) -> Result<HtmlResult, CompileError>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<typst::foundations::Str>,
    V: typst::foundations::IntoValue,
{
    let (document, accessed_files, diagnostics) =
        compile_document_internal_with_inputs(path, root, inputs)?;

    let html = typst_html::html(&document)
        .map_err(|e| CompileError::html_export(format!("{e:?}")))?
        .into_bytes();

    Ok(HtmlResult {
        html,
        accessed_files,
        diagnostics,
    })
}

/// Compile a Typst file to HTML bytes with custom `sys.inputs` from a `Dict`.
///
/// This is useful when you already have a pre-built `Dict` of inputs.
pub fn compile_html_with_inputs_dict(
    path: &Path,
    root: &Path,
    inputs: Dict,
) -> Result<HtmlResult, CompileError> {
    let (document, accessed_files, diagnostics) =
        compile_document_internal_with_inputs_dict(path, root, inputs)?;

    let html = typst_html::html(&document)
        .map_err(|e| CompileError::html_export(format!("{e:?}")))?
        .into_bytes();

    Ok(HtmlResult {
        html,
        accessed_files,
        diagnostics,
    })
}

/// Compile to HtmlDocument with custom `sys.inputs`.
pub fn compile_document_with_inputs<I, K, V>(
    path: &Path,
    root: &Path,
    inputs: I,
) -> Result<DocumentResult, CompileError>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<typst::foundations::Str>,
    V: typst::foundations::IntoValue,
{
    let (document, accessed_files, diagnostics) =
        compile_document_internal_with_inputs(path, root, inputs)?;

    Ok(DocumentResult {
        document,
        accessed_files,
        diagnostics,
    })
}

// =============================================================================
// Internal Helpers
// =============================================================================

/// Core compilation logic.
fn compile_document_internal(
    path: &Path,
    root: &Path,
) -> Result<(HtmlDocument, Vec<PathBuf>, Vec<SourceDiagnostic>), CompileError> {
    let world = SystemWorld::new(path, root);
    compile_with_world(&world)
}

/// Core compilation logic with custom inputs.
fn compile_document_internal_with_inputs<I, K, V>(
    path: &Path,
    root: &Path,
    inputs: I,
) -> Result<(HtmlDocument, Vec<PathBuf>, Vec<SourceDiagnostic>), CompileError>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<typst::foundations::Str>,
    V: typst::foundations::IntoValue,
{
    let world = SystemWorld::new(path, root).with_inputs(inputs);
    compile_with_world(&world)
}

/// Core compilation logic with Dict inputs.
fn compile_document_internal_with_inputs_dict(
    path: &Path,
    root: &Path,
    inputs: Dict,
) -> Result<(HtmlDocument, Vec<PathBuf>, Vec<SourceDiagnostic>), CompileError> {
    let world = SystemWorld::new(path, root).with_inputs_dict(inputs);
    compile_with_world(&world)
}

/// Shared compilation logic.
fn compile_with_world(
    world: &SystemWorld,
) -> Result<(HtmlDocument, Vec<PathBuf>, Vec<SourceDiagnostic>), CompileError> {
    reset_access_flags();

    let result = typst::compile(world);

    // Check for errors in warnings (shouldn't happen, but handle it)
    if has_errors(&result.warnings) {
        return Err(CompileError::compilation(world, result.warnings.to_vec()));
    }

    // Extract document or return errors
    let document = result.output.map_err(|errors| {
        let all_diags: Vec<_> = errors.iter().chain(&result.warnings).cloned().collect();
        let filtered = filter_html_warnings(&all_diags);
        CompileError::compilation(world, filtered)
    })?;

    // Collect accessed files
    let accessed_files = collect_accessed_files(world.root());

    // Filter HTML development warnings
    let diagnostics = filter_html_warnings(&result.warnings);

    Ok((document, accessed_files, diagnostics))
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
    #[test]
    fn test_query_metadata_not_found() {
        // This would require a compiled document, skip for now
    }
}
