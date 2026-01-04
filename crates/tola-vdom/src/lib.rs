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
//! - `transform`: Unified transformation module
//!   - `Transform` trait: The only public API for phase transitions
//!   - `Pipeline`: Fluent chain builder
//!   - `Processor`: Indexed → Processed transformation
//!   - `indexer`: Raw → Indexed (StableId generation, family identification)
//!   - `render`: Processed → HTML (rendering)
//!
//! ## Algorithms
//! - `diff`: VDOM diff algorithm (generates Patches)
//! - `lcs`: Longest Common Subsequence (used by diff)
//! - `id`: StableId (content-hash based identity)
//!
//! ## Conversion (feature-gated)
//! - `convert::typst`: Typst HtmlDocument → Raw VDOM (feature = "typst")
//! - `convert::markdown`: Markdown → Raw VDOM (planned, feature = "markdown")
//! - `convert::html`: HTML string → Raw VDOM (planned, feature = "html-parser")
//!
//! # Usage
//!
//! ```ignore
//! use vdom::{Document, Raw, Indexed, Processed, Transform, Processor};
//! use vdom::transform::Indexer;
//!
//! // Pipeline: Raw → Indexed → Processed → HTML
//! let indexed = raw_doc.pipe(Indexer::new());
//! let processed = indexed.pipe(Processor::new());
//! let html = HtmlRenderer::new().render(processed);
//! ```

// Allow dead code at module level - this is a standalone design that will be
// integrated when convert.rs is implemented
#![allow(dead_code)]

// Allow `::tola_vdom` to work inside the crate itself
extern crate self as tola_vdom;

pub mod attr;
pub mod cache;
pub mod capability;
pub mod convert;
pub mod diff;
pub mod family;
pub mod hash;
pub mod id;
pub mod lcs;
#[macro_use]
pub mod macros;
pub mod node;
pub mod phase;
pub mod span;
pub mod transform;

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
pub use transform::{process_family_ext, IdentityTransform, Pipeline, Processor, Transform};

// Node types
#[allow(unused_imports)]
pub use node::{Document, Element, FamilyExt, HasFamilyData, Node, Stats, Text};

// Phase types
#[allow(unused_imports)]
pub use phase::{
    Indexed, IndexedDocExt, IndexedElemExt, IndexedTextExt, Phase, PhaseData, Processed,
    ProcessedDocExt, ProcessedElemExt, Raw, RawDocExt, RawElemExt, RawTextExt,
    Rendered, RenderedDocExt,
};

// Source span abstraction
#[allow(unused_imports)]
pub use span::SourceSpan;

// Conversion (requires typst feature)
#[cfg(feature = "typst")]
#[allow(unused_imports)]
pub use convert::{from_typst_html, from_typst_html_with_meta};

// Identity
#[allow(unused_imports)]
pub use id::{PageSeed, StableId};

// Diff algorithm
#[allow(unused_imports)]
pub use diff::{diff, DiffResult, DiffStats, Patch};
#[allow(unused_imports)]
pub use lcs::{diff_sequences, Edit, LcsResult, LcsStats};

// Cache
pub use cache::{CacheKey, VdomCache};

// Hash utilities
pub use hash::StableHasher;

// =============================================================================
// High-level API for compilation pipeline integration
// =============================================================================

#[cfg(feature = "typst")]
use transform::{HtmlRenderer, Indexer};

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
/// 2. `Indexer` - Transform Raw → Indexed (assign StableIds, identify families)
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
#[cfg(feature = "typst")]
pub fn compile_to_html(document: &typst_batch::typst_html::HtmlDocument) -> VdomCompileResult {
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


