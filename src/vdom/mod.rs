//! TTG (Trees That Grow) VDOM Core Module
//!
//! Multi-phase type-safe architecture based on GATs:
//! - `family`: TagFamily trait and family definitions
//! - `attr`: Attribute system (Attrs type alias)
//! - `phase`: Phase/PhaseData trait and phase definitions
//! - `node`: Node/Element/Text/Frame/Document types + FamilyExt
//! - `folder`: Folder trait (low-level phase transformation)
//! - `transform`: Transform trait + Pipeline (high-level pipeline API)
//! - `macros`: FamilyExt transformation macros
//! - `convert`: typst-html → Raw conversion
//!
//! # Status
//!
//! This module is a complete TTG architecture implementation but is not yet
//! integrated into the main compilation pipeline. The `convert.rs` module
//! (TODO) will bridge typst-html output to Raw VDOM.
//!
//! Until integration is complete, most code is marked `#[allow(dead_code)]`.

// Allow dead code at module level - this is a standalone design that will be
// integrated when convert.rs is implemented
#![allow(dead_code)]

pub mod attr;
pub mod convert;
pub mod family;
pub mod folder;
pub mod id;
#[macro_use]
pub mod macros;
pub mod node;
pub mod phase;
pub mod transform;
pub mod transforms;

// Re-exports for convenience (allow unused until module is integrated)
#[allow(unused_imports)]
pub use family::{
    FamilyKind, HeadingFamily, HeadingIndexedData, HeadingProcessedData, LinkFamily,
    LinkIndexedData, LinkProcessedData, LinkType, MediaFamily, MediaIndexedData,
    MediaProcessedData, MediaType, OtherFamily, SvgFamily, SvgIndexedData, SvgProcessedData,
    TagFamily,
};
#[allow(unused_imports)]
pub use folder::{fold, Folder, ProcessFolder, process_family_ext};
#[allow(unused_imports)]
pub use node::{Document, Element, FamilyExt, Frame, HasFamilyData, Node, NodeId, Stats, Text};
#[allow(unused_imports)]
pub use phase::{
    Indexed, IndexedDocExt, IndexedElemExt, IndexedFrameExt, IndexedTextExt, Phase, PhaseData, Processed,
    ProcessedDocExt, ProcessedElemExt, Raw, RawDocExt, RawElemExt, RawFrameExt, RawTextExt,
    Rendered, RenderedDocExt,
};
#[allow(unused_imports)]
pub use transform::{IdentityTransform, Pipeline, Transform};
#[allow(unused_imports)]
pub use convert::{from_typst_html, from_typst_html_with_meta};
#[allow(unused_imports)]
pub use id::StableId;

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
/// 3. `ProcessFolder` - Transform Indexed → Processed (prepare for rendering)
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
    let mut process_folder = ProcessFolder::new();
    let processed_doc = fold(indexed_doc, &mut process_folder);
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
    compile_to_html_with_options(document, label_name, false)
}

/// Compile for development mode with hot reload support.
///
/// Emits `data-tola-id` attributes on all elements for VDOM diffing.
pub fn compile_to_html_for_dev(
    document: &typst_html::HtmlDocument,
    label_name: &str,
) -> (VdomCompileResult, Option<serde_json::Value>) {
    compile_to_html_with_options(document, label_name, true)
}

/// Internal: compile with configurable options.
fn compile_to_html_with_options(
    document: &typst_html::HtmlDocument,
    label_name: &str,
    emit_stable_ids: bool,
) -> (VdomCompileResult, Option<serde_json::Value>) {
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

    // Transform through pipeline
    let indexed_doc = Indexer::new().transform(raw_doc);
    let mut process_folder = ProcessFolder::new();
    let processed_doc = fold(indexed_doc, &mut process_folder);
    let stats = processed_doc.ext.clone();

    // Render to HTML with appropriate config
    let renderer_config = if emit_stable_ids {
        transforms::render::HtmlRendererConfig::for_dev()
    } else {
        transforms::render::HtmlRendererConfig::for_production()
    };
    let html = HtmlRenderer::with_config(renderer_config).render(processed_doc);

    (VdomCompileResult { html, stats }, meta)
}

// =============================================================================
// Hot Reload Support: VDOM Diff API
// =============================================================================

/// Result of VDOM compilation with Indexed tree for diffing
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
/// This function is optimized for hot reload: it returns the Indexed VDOM
/// which can be cached and compared with the next compilation to generate
/// minimal patches.
///
/// # Usage
///
/// ```ignore
/// // First compilation
/// let result = vdom::compile_for_dev(&document, "tola-meta");
/// cache.insert(path, result.indexed.clone());
///
/// // On file change
/// let new_result = vdom::compile_for_dev(&new_document, "tola-meta");
/// let patches = hotreload::diff_indexed_documents(&cache[path], &new_result.indexed);
/// ```
pub fn compile_for_dev(
    document: &typst_html::HtmlDocument,
    label_name: &str,
) -> VdomDevResult {
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
    let indexed_doc = Indexer::new().transform(raw_doc);

    // Clone for return before consuming
    let indexed_for_cache = indexed_doc.clone();

    // Continue pipeline to get HTML
    let mut process_folder = ProcessFolder::new();
    let processed_doc = fold(indexed_doc, &mut process_folder);
    let stats = processed_doc.ext.clone();

    // Render with stable IDs for hot reload
    let renderer_config = transforms::render::HtmlRendererConfig::for_dev();
    let html = HtmlRenderer::with_config(renderer_config).render(processed_doc);

    VdomDevResult {
        html,
        indexed: indexed_for_cache,
        metadata: meta,
        stats,
    }
}
