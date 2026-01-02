//! Bridge module: connects Typst compilation with VDOM processing.
//!
//! This module provides the unified entry point for compiling Typst files
//! through the VDOM pipeline. It decouples `typst_lib` (pure Typst compilation)
//! from `vdom` (HTML tree processing).
//!
//! # Architecture
//!
//! ```text
//! typst_lib::compile_base  →  typst_html::HtmlDocument
//!                                      │
//!                                      ▼
//! bridge::compile_vdom    →  vdom::compile → VdomResult
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use tola::compiler::bridge;
//! use tola::driver::Production;
//!
//! let result = bridge::compile_vdom(&Production, path, root, "tola-meta", None)?;
//! std::fs::write("output.html", &result.html)?;
//! ```

use std::path::{Path, PathBuf};

use crate::driver::BuildDriver;

// =============================================================================
// Result Types
// =============================================================================

/// Result of VDOM compilation.
///
/// Contains the compiled HTML, optional cached VDOM (for hot reload),
/// extracted metadata, and file dependencies.
pub struct VdomResult {
    /// Generated HTML bytes
    pub html: Vec<u8>,
    /// Indexed VDOM for diff comparison (only in development mode)
    pub indexed_vdom: Option<crate::vdom::Document<crate::vdom::Indexed>>,
    /// Extracted metadata (from label query)
    pub metadata: Option<serde_json::Value>,
    /// Files accessed during compilation (for dependency tracking)
    pub accessed_files: Vec<PathBuf>,
    /// Formatted warnings (e.g., unknown font family)
    pub warnings: Option<String>,
}

impl VdomResult {
    /// Check if this compilation accessed any virtual data files.
    ///
    /// Virtual data files (`/_data/pages.json`, `/_data/tags.json`) contain
    /// dynamically generated content that depends on other pages' metadata.
    /// Pages that access these files are "dynamic" and may need to be
    /// recompiled when other pages change.
    #[inline]
    pub fn uses_virtual_data(&self) -> bool {
        self.accessed_files
            .iter()
            .any(|p| crate::data::is_virtual_data_path(p))
    }
}

// =============================================================================
// Main API
// =============================================================================

/// Compile a Typst file using the VDOM pipeline.
///
/// This is the **unified entry point** for all compilation modes.
/// The `driver` parameter controls:
/// - `emit_ids()`: Whether to output `data-tola-id` attributes (for hot reload)
/// - `cache_vdom()`: Whether to return indexed VDOM (for diffing)
///
/// # Arguments
///
/// * `driver` - Build driver (`Production` or `Development`)
/// * `path` - Path to the Typst source file
/// * `root` - Project root directory
/// * `label_name` - Metadata label to extract (e.g., "tola-meta")
/// * `url_path` - Optional URL path for globally unique StableIds
///
/// # Examples
///
/// ```ignore
/// use tola::compiler::bridge;
/// use tola::driver::{Production, Development};
///
/// // Production build (no VDOM caching)
/// let result = bridge::compile_vdom(&Production, path, root, "tola-meta", None)?;
/// assert!(result.indexed_vdom.is_none());
///
/// // Development build (with VDOM for hot reload)
/// let result = bridge::compile_vdom(&Development, path, root, "tola-meta", Some("/blog/post.html"))?;
/// let vdom = result.indexed_vdom.expect("Development mode returns VDOM");
/// ```
pub fn compile_vdom<D: BuildDriver>(
    driver: &D,
    path: &Path,
    root: &Path,
    label_name: &str,
    url_path: Option<&str>,
) -> anyhow::Result<VdomResult> {
    // Use typst_lib for raw compilation
    let (document, accessed_files, warnings) = crate::typst_lib::compile_html(path, root)?;

    // Use VDOM pipeline for processing
    let output = crate::vdom::compile(&document, label_name, driver, url_path);

    Ok(VdomResult {
        html: output.html,
        indexed_vdom: output.indexed,
        metadata: output.metadata,
        accessed_files,
        warnings,
    })
}
