//! TTG (Trees That Grow) VDOM Core Module
//!
//! Multi-phase type-safe architecture based on GATs:
//!
//! ## Core Modules
//! - `phase`: Phase/PhaseData trait and phase definitions (Raw → Indexed → Processed → Rendered)
//! - `node`: Node/Element/Text/Document types + FamilyExt
//! - `family`: TagFamily trait (SVG, Link, Heading, Media, Other)
//! - `attr`: Attribute system (Attrs type alias)
//!
//! ## Transformation System
//! - `transform`: `Transform` trait + `Pipeline` (unified API)
//!   - `Processor`: Indexed → Processed transformation
//! - `transforms/`: Concrete transform implementations
//!   - `indexer`: Raw → Indexed (StableId generation, family identification)
//!   - `render`: Processed → HTML (rendering)
//!
//! ## Algorithms
//! - `diff`: VDOM diff algorithm (generates Patches)
//! - `lcs`: Longest Common Subsequence (used by diff)
//! - `id`: StableId (content-hash based identity)
//!
//! ## Conversion
//! - `convert`: typst-html → Raw VDOM conversion
//!
//! # Usage
//!
//! ```ignore
//! use vdom::{Document, Raw, Indexed, Processed, Transform, Processor};
//! use vdom::transforms::Indexer;
//!
//! // Pipeline: Raw → Indexed → Processed → HTML
//! let indexed = raw_doc.pipe(Indexer::new());
//! let processed = indexed.pipe(Processor::new());
//! let html = HtmlRenderer::new().render(processed);
//! ```

// Allow dead code at module level - this is a standalone design that will be
// integrated when convert.rs is implemented
#![allow(dead_code)]

pub mod attr;
pub mod convert;
pub mod diff;
pub mod family;
pub mod id;
pub mod lcs;
#[macro_use]
pub mod macros;
pub mod node;
pub mod phase;
pub mod transform;
pub mod transforms;

// =============================================================================
// Re-exports for public API
// =============================================================================

// These exports may appear unused within the crate but are part of the public API

// Family system
#[allow(unused_imports)]
pub use family::{
    FamilyKind, HeadingFamily, HeadingIndexedData, HeadingProcessedData, LinkFamily,
    LinkIndexedData, LinkProcessedData, LinkType, MediaFamily, MediaIndexedData,
    MediaProcessedData, MediaType, OtherFamily, SvgFamily, SvgIndexedData, SvgProcessedData,
    TagFamily,
};

// Transform system (unified API)
#[allow(unused_imports)]
pub use transform::{
    process_family_ext, IdentityTransform, NodeIdGenerator, Pipeline, Processor, Transform,
};

// Node types
#[allow(unused_imports)]
pub use node::{Document, Element, FamilyExt, HasFamilyData, Node, NodeId, Stats, Text};

// Phase types
#[allow(unused_imports)]
pub use phase::{
    Indexed, IndexedDocExt, IndexedElemExt, IndexedTextExt, Phase, PhaseData, Processed,
    ProcessedDocExt, ProcessedElemExt, Raw, RawDocExt, RawElemExt, RawTextExt,
    Rendered, RenderedDocExt,
};

// Conversion
#[allow(unused_imports)]
pub use convert::{from_typst_html, from_typst_html_with_meta};

// Identity
#[allow(unused_imports)]
pub use id::StableId;

// Diff algorithm
#[allow(unused_imports)]
pub use diff::{diff, DiffResult, DiffStats, Patch};
#[allow(unused_imports)]
pub use lcs::{diff_sequences, Edit, LcsResult, LcsStats};

// =============================================================================
// High-level API for compilation pipeline integration
// =============================================================================

use transforms::{HtmlRenderer, Indexer};

/// Result of VDOM compilation
#[derive(Debug)]
pub struct VdomCompileResult {
    /// Generated HTML bytes
    pub html: Vec<u8>,
    /// Processing statistics
    pub stats: ProcessedDocExt,
}

/// Compile a typst HtmlDocument to HTML bytes using the VDOM pipeline.
///
/// This is the main entry point for integrating VDOM with the compilation pipeline.
///
/// # Pipeline
///
/// 1. `from_typst_html()` - Convert typst HtmlDocument to Raw VDOM
/// 2. `Indexer` - Transform Raw → Indexed (assign NodeIds, identify families)
/// 3. `Processor` - Transform Indexed → Processed (prepare for rendering)
/// 4. `HtmlRenderer` - Render Processed → HTML bytes
///
/// # Usage
///
/// ```ignore
/// let doc_result = typst_lib::compile_document(path, root, "tola-meta")?;
/// let result = vdom::compile_to_html(&doc_result.document);
/// fs::write(output_path, &result.html)?;
/// ```
pub fn compile_to_html(document: &typst_html::HtmlDocument) -> VdomCompileResult {
    use transform::Transform;

    // Raw phase: convert from typst
    let raw_doc = from_typst_html(document);

    // Transform through pipeline
    let indexed_doc = Indexer::new().transform(raw_doc);
    let processed_doc = Processor::new().transform(indexed_doc);
    let stats = processed_doc.ext.clone();

    // Render to HTML
    let html = HtmlRenderer::new().render(processed_doc);

    VdomCompileResult { html, stats }
}

/// Compile with extracted metadata.
///
/// Same as `compile_to_html` but also extracts metadata by label.
pub fn compile_to_html_with_meta(
    document: &typst_html::HtmlDocument,
    label_name: &str,
) -> (VdomCompileResult, Option<serde_json::Value>) {
    let result = compile(document, label_name, &crate::driver::Production, None);
    (VdomCompileResult { html: result.html, stats: result.stats }, result.metadata)
}

/// Compile for development mode with hot reload support.
///
/// Emits `data-tola-id` attributes on all elements for VDOM diffing.
#[deprecated(since = "0.7.0", note = "Use `compile` with Development driver")]
#[allow(deprecated)]
pub fn compile_to_html_for_dev(
    document: &typst_html::HtmlDocument,
    label_name: &str,
) -> (VdomCompileResult, Option<serde_json::Value>) {
    let result = compile(document, label_name, &crate::driver::Development, None);
    (VdomCompileResult { html: result.html, stats: result.stats }, result.metadata)
}

/// Compile with configurable driver.
///
/// Deprecated: Use `compile` instead.
#[deprecated(since = "0.7.0", note = "Use `compile` instead")]
#[allow(deprecated)]
pub fn compile_to_html_with_driver<D: crate::driver::BuildDriver>(
    document: &typst_html::HtmlDocument,
    label_name: &str,
    driver: &D,
) -> (VdomCompileResult, Option<serde_json::Value>) {
    let result = compile(document, label_name, driver, None);
    (VdomCompileResult { html: result.html, stats: result.stats }, result.metadata)
}

// =============================================================================
// Unified Compilation API
// =============================================================================

/// Unified result of VDOM compilation.
///
/// Contains HTML output and optionally the Indexed VDOM tree (for hot reload).
#[derive(Debug)]
pub struct CompileOutput {
    /// Generated HTML bytes
    pub html: Vec<u8>,
    /// Indexed VDOM for diff comparison (only when driver.cache_vdom() is true)
    pub indexed: Option<Document<Indexed>>,
    /// Processing statistics
    pub stats: ProcessedDocExt,
    /// Extracted metadata (if any)
    pub metadata: Option<serde_json::Value>,
}

/// Compile a typst HtmlDocument using the VDOM pipeline.
///
/// This is the **unified entry point** for all compilation modes.
/// The `driver` parameter controls:
/// - `emit_ids()`: Whether to output `data-tola-id` attributes
/// - `cache_vdom()`: Whether to return indexed VDOM for hot reload
///
/// # Examples
///
/// ```ignore
/// // Production build
/// let result = vdom::compile(&document, "tola-meta", &Production, None);
/// assert!(result.indexed.is_none()); // No VDOM cache needed
///
/// // Development build (hot reload)
/// let result = vdom::compile(&document, "tola-meta", &Development, Some("/blog/post.html"));
/// cache.insert(path, result.indexed.unwrap()); // Cache for diffing
/// ```
pub fn compile<D: crate::driver::BuildDriver>(
    document: &typst_html::HtmlDocument,
    label_name: &str,
    driver: &D,
    page_path: Option<&str>,
) -> CompileOutput {
    use transform::Transform;
    use typst::foundations::{Label, Selector};
    use typst::introspection::MetadataElem;
    use typst::utils::PicoStr;

    // Extract metadata
    let meta = (|| {
        let label = Label::new(PicoStr::intern(label_name))?;
        let introspector = &document.introspector;
        let elem = introspector.query_unique(&Selector::Label(label)).ok()?;
        elem.to_packed::<MetadataElem>()
            .and_then(|meta| serde_json::to_value(&meta.value).ok())
    })();

    // Raw phase: convert from typst
    let raw_doc = from_typst_html(document);

    // Transform to Indexed
    // When page_path is provided, StableIds become globally unique across pages
    let indexer = if let Some(path) = page_path {
        Indexer::new().with_page_seed(path)
    } else {
        Indexer::new()
    };
    let indexed_doc = indexer.transform(raw_doc);

    // Optionally cache for hot reload
    let indexed_for_cache = if driver.cache_vdom() {
        Some(indexed_doc.clone())
    } else {
        None
    };

    // Continue pipeline to get HTML
    let processed_doc = Processor::new().transform(indexed_doc);
    let stats = processed_doc.ext.clone();

    // Render with appropriate config
    let renderer_config = if driver.emit_ids() {
        transforms::render::HtmlRendererConfig::for_dev()
    } else {
        transforms::render::HtmlRendererConfig::for_production()
    };
    let html = HtmlRenderer::with_config(renderer_config).render(processed_doc);

    CompileOutput {
        html,
        indexed: indexed_for_cache,
        stats,
        metadata: meta,
    }
}

// =============================================================================
// Deprecated APIs (for backward compatibility)
// =============================================================================

/// Result of VDOM compilation with Indexed tree for diffing
///
/// Deprecated: Use `CompileOutput` instead.
#[derive(Debug)]
pub struct VdomDevResult {
    /// Generated HTML bytes
    pub html: Vec<u8>,
    /// Indexed VDOM for diff comparison
    pub indexed: Document<Indexed>,
    /// Processing statistics
    pub stats: ProcessedDocExt,
    /// Extracted metadata (if any)
    pub metadata: Option<serde_json::Value>,
}

/// Compile for development with both HTML and Indexed VDOM.
///
/// Deprecated: Use `compile` with Development driver instead.
#[deprecated(since = "0.7.0", note = "Use `compile` with Development driver instead")]
#[allow(deprecated)]
pub fn compile_with_vdom_cache(
    document: &typst_html::HtmlDocument,
    label_name: &str,
) -> VdomDevResult {
    let result = compile(document, label_name, &crate::driver::Development, None);
    VdomDevResult {
        html: result.html,
        indexed: result.indexed.expect("Development driver should cache VDOM"),
        stats: result.stats,
        metadata: result.metadata,
    }
}

/// Deprecated: Use `compile` with Development driver instead.
#[deprecated(since = "0.7.0", note = "Use `compile` with Development driver instead")]
pub fn compile_for_dev(
    document: &typst_html::HtmlDocument,
    label_name: &str,
) -> VdomDevResult {
    #[allow(deprecated)]
    compile_with_vdom_cache(document, label_name)
}
