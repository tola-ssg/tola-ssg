//! VDOM compilation utilities for tola
//!
//! This module provides the high-level compile API that bridges
//! typst-html documents with the VDOM pipeline, including driver-aware
//! configuration.

use serde_json::Value as JsonValue;
use typst_batch::typst_html;

use crate::driver::BuildDriver;
use tola_vdom::{
    transform::Transform,
    transforms::{render::HtmlRendererConfig, HtmlRenderer, Indexer},
    Document, Indexed, PageSeed, ProcessedDocExt, Processor,
};

/// Unified result of VDOM compilation.
///
/// Contains HTML output and optionally the Indexed VDOM tree (for hot reload).
#[derive(Debug)]
pub struct CompileOutput {
    /// Generated HTML bytes
    pub html: Vec<u8>,
    /// Indexed VDOM for diff comparison (only when driver.cache_vdom() is true)
    pub indexed: Option<Document<Indexed>>,
    /// Processing statistics (reserved for future use)
    #[allow(dead_code)]
    pub stats: ProcessedDocExt,
    /// Extracted metadata (if any)
    pub metadata: Option<JsonValue>,
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
/// let result = vdom_compile::compile(&document, "tola-meta", &Production, None);
/// assert!(result.indexed.is_none()); // No VDOM cache needed
///
/// // Development build (hot reload)
/// let result = vdom_compile::compile(&document, "tola-meta", &Development, Some("/blog/post.html"));
/// cache.insert(path, result.indexed.unwrap()); // Cache for diffing
/// ```
pub fn compile<D: BuildDriver>(
    document: &typst_html::HtmlDocument,
    label_name: &str,
    driver: &D,
    page_path: Option<&str>,
) -> CompileOutput {
    use typst_batch::typst::foundations::{Label, Selector};
    use typst_batch::typst::introspection::MetadataElem;
    use typst_batch::typst::utils::PicoStr;

    // Extract metadata
    let meta = (|| {
        let label = Label::new(PicoStr::intern(label_name))?;
        let introspector = &document.introspector;
        let elem = introspector.query_unique(&Selector::Label(label)).ok()?;
        elem.to_packed::<MetadataElem>()
            .and_then(|meta| serde_json::to_value(&meta.value).ok())
    })();

    // Raw phase: convert from typst
    let raw_doc = tola_vdom::from_typst_html(document);

    // Transform to Indexed
    // When page_path is provided, StableIds become globally unique across pages
    let indexer = if let Some(path) = page_path {
        Indexer::new().with_page_seed(PageSeed::from_path(path))
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
        HtmlRendererConfig::for_dev()
    } else {
        HtmlRendererConfig::for_production()
    };
    let html = HtmlRenderer::with_config(renderer_config).render(processed_doc);

    CompileOutput {
        html,
        indexed: indexed_for_cache,
        stats,
        metadata: meta,
    }
}
