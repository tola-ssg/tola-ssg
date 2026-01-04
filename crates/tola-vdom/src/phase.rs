//! Phase definitions for TTG VDOM
//!
//! Defines the four compilation phases:
//! - Raw: Direct typst-html output
//! - Indexed: Elements indexed with family data
//! - Processed: Links resolved, SVGs optimized
//! - Rendered: Final HTML output

use std::fmt::Debug;

use super::family::TagFamily;

// =============================================================================
// Phase trait
// =============================================================================

/// Marker trait for VDOM phases
///
/// Each phase defines the types used for:
/// - Element extension data (family-specific at Indexed/Processed)
/// - Text extension data (typically unit type)
/// - Frame extension data (for embedded documents)
/// - Document extension data (metadata, stats)
pub trait Phase: 'static + Send + Sync + Debug + Clone {
    /// Phase name for debugging
    const NAME: &'static str;
}

// =============================================================================
// PhaseData trait (GAT-based extension data)
// =============================================================================

/// Associates extension data types with a phase using GATs
///
/// 🔑 Key insight: ElemExt<F> is a GAT that allows different data types
/// for different tag families at each phase.
///
/// Example:
/// - `Indexed::ElemExt<LinkFamily>` = `IndexedElemExt<LinkIndexedData>`
/// - `Indexed::ElemExt<SvgFamily>` = `IndexedElemExt<SvgIndexedData>`
/// - `Processed::ElemExt<LinkFamily>` = `ProcessedElemExt<LinkProcessedData>`
///
/// Note: FrameExt has been removed. Frames are now eagerly converted to
/// SVG Elements during the Raw phase conversion.
pub trait PhaseData: Phase {
    /// Document-level extension data (metadata, stats)
    type DocExt: Debug + Clone + Default + Send + Sync;

    /// Element extension data - GAT parameterized by TagFamily
    /// This is the core of TTG: each family can have different data at each phase
    type ElemExt<F: TagFamily>: Debug + Clone + Default + Send + Sync;

    /// Text node extension data
    type TextExt: Debug + Clone + Default + Send + Sync;
}

// =============================================================================
// Phase definitions
// =============================================================================

/// Raw phase: Direct output from typst-html
///
/// Contains source file metadata but no family-specific element data.
#[derive(Debug, Clone, Copy)]
pub struct Raw;

impl Phase for Raw {
    const NAME: &'static str = "raw";
}

impl PhaseData for Raw {
    type DocExt = RawDocExt;
    /// Raw phase element extension - stores Span for StableId generation.
    ///
    /// At Raw phase, we capture the Typst Span from HtmlElement. This Span
    /// is then converted to StableId during the Raw → Indexed transformation.
    /// The GAT parameter `F` is ignored here because all families start with
    /// the same Span-only state.
    type ElemExt<F: TagFamily> = RawElemExt;
    /// Raw phase text extension - stores Span for StableId generation.
    type TextExt = RawTextExt;
}

/// Document extension for Raw phase - source file metadata
#[derive(Debug, Clone, Default)]
pub struct RawDocExt {
    /// Source file path (relative to content directory)
    pub source_path: Option<String>,
    /// Whether this is an index page (index.typ)
    pub is_index: bool,
    /// File dependencies (templates, includes)
    pub dependencies: Vec<String>,
    /// Content metadata (title, date, etc.)
    pub content_meta: Option<String>,
}

/// Element extension for Raw phase - stores Typst Span
///
/// The Span is captured during conversion from typst-html and later
/// converted to StableId in the Indexer transform.
#[derive(Debug, Clone, Default)]
pub struct RawElemExt {
    /// Source Span for this element (if available)
    ///
    /// Used to generate stable IDs that persist across compilations.
    /// `None` for elements without a source location (e.g., generated wrappers).
    pub span: Option<crate::span::SourceSpan>,
}

impl RawElemExt {
    /// Create with a span
    pub fn with_span(span: crate::span::SourceSpan) -> Self {
        Self { span: Some(span) }
    }

    /// Create without a span (detached)
    pub fn detached() -> Self {
        Self { span: None }
    }

    /// Check if this element has a valid span
    pub fn has_span(&self) -> bool {
        self.span.map(|s| !s.is_detached()).unwrap_or(false)
    }
}

/// Text node extension for Raw phase - stores Typst Span
///
/// Text nodes also have Spans which are used for StableId generation.
#[derive(Debug, Clone, Default)]
pub struct RawTextExt {
    /// Source Span for this text node (if available)
    pub span: Option<crate::span::SourceSpan>,
}

impl RawTextExt {
    /// Create with a span
    pub fn with_span(span: crate::span::SourceSpan) -> Self {
        Self { span: Some(span) }
    }

    /// Create without a span (detached)
    pub fn detached() -> Self {
        Self { span: None }
    }

    /// Check if this text node has a valid span
    pub fn has_span(&self) -> bool {
        self.span.map(|s| !s.is_detached()).unwrap_or(false)
    }
}

impl RawDocExt {
    /// Check if source path is set
    pub fn has_source(&self) -> bool {
        self.source_path.is_some()
    }

    /// Get source file extension
    pub fn source_extension(&self) -> Option<&str> {
        self.source_path.as_ref()?
            .rsplit('.')
            .next()
    }

    /// Get source filename without path
    pub fn source_filename(&self) -> Option<&str> {
        self.source_path.as_ref()?
            .rsplit('/')
            .next()
    }
}

// Note: RawFrameExt has been removed.
// Frames are now eagerly converted to SVG Elements in convert.rs.

/// Indexed phase: Elements tagged with family data
///
/// Each element has been analyzed and assigned to a family,
/// with initial data collected (href, src, viewbox, etc.)
#[derive(Debug, Clone, Copy)]
pub struct Indexed;

impl Phase for Indexed {
    const NAME: &'static str = "indexed";
}

/// Indexed phase element extension - contains family-specific indexed data
pub struct IndexedElemExt<F: TagFamily> {
    /// Stable node identifier for cross-compilation identity
    ///
    /// Used for VDOM diffing and SyncTeX functionality.
    /// Generated from content hash with occurrence-based disambiguation.
    pub stable_id: super::id::StableId,
    /// Family-specific data (via TagFamily::IndexedData)
    pub family_data: F::IndexedData,
}

// Manual trait implementations (F doesn't require Debug/Clone/Default)
impl<F: TagFamily> std::fmt::Debug for IndexedElemExt<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexedElemExt")
            .field("stable_id", &self.stable_id)
            .field("family_data", &self.family_data)
            .finish()
    }
}

impl<F: TagFamily> Clone for IndexedElemExt<F> {
    fn clone(&self) -> Self {
        Self {
            stable_id: self.stable_id,
            family_data: self.family_data.clone(),
        }
    }
}

impl<F: TagFamily> Default for IndexedElemExt<F> {
    fn default() -> Self {
        Self {
            stable_id: super::id::StableId::default(),
            family_data: F::IndexedData::default(),
        }
    }
}

impl PhaseData for Indexed {
    type DocExt = IndexedDocExt;
    type ElemExt<F: TagFamily> = IndexedElemExt<F>;
    type TextExt = IndexedTextExt;
}

/// Text node extension for Indexed phase
///
/// Contains StableId for cross-compilation identity tracking.
#[derive(Debug, Clone, Default)]
pub struct IndexedTextExt {
    /// Stable node identifier for cross-compilation identity
    pub stable_id: super::id::StableId,
}

impl IndexedTextExt {
    /// Create with a stable id
    pub fn new(stable_id: super::id::StableId) -> Self {
        Self { stable_id }
    }

    /// Get the StableId for this text node
    pub fn stable_id(&self) -> super::id::StableId {
        self.stable_id
    }
}

/// Document extension for Indexed phase
#[derive(Debug, Clone, Default)]
pub struct IndexedDocExt {
    /// Base info from Raw phase
    pub base: RawDocExt,
    /// Total node count (elements + text nodes)
    pub node_count: usize,
    /// Element count
    pub element_count: usize,
    /// Text node count
    pub text_count: usize,
    /// SVG element count
    pub svg_count: usize,
    /// Link element count
    pub link_count: usize,
    /// Heading element count
    pub heading_count: usize,
    /// Media element count
    pub media_count: usize,
}

impl IndexedDocExt {
    /// Total indexed family nodes count (svg + link + heading + media)
    pub fn total_family_nodes(&self) -> usize {
        self.svg_count + self.link_count + self.heading_count + self.media_count
    }

    /// Check if has SVG nodes
    pub fn has_svg(&self) -> bool { self.svg_count > 0 }

    /// Check if has link nodes
    pub fn has_links(&self) -> bool { self.link_count > 0 }

    /// Check if has heading nodes
    pub fn has_headings(&self) -> bool { self.heading_count > 0 }

    /// Check if has media nodes
    pub fn has_media(&self) -> bool { self.media_count > 0 }
}

// Note: IndexedFrameExt has been removed.
// Frames are now eagerly converted to SVG Elements in convert.rs.

/// Processed phase: All transformations applied
///
/// - Links resolved and validated
/// - SVGs optimized
/// - Headings assigned anchor IDs
/// - Media dimensions calculated
#[derive(Debug, Clone, Copy)]
pub struct Processed;

impl Phase for Processed {
    const NAME: &'static str = "processed";
}

/// Processed phase element extension - contains family-specific processed data
pub struct ProcessedElemExt<F: TagFamily> {
    /// Stable node identifier (preserved from Indexed phase)
    ///
    /// Used for VDOM diffing and hot reload targeting.
    pub stable_id: super::id::StableId,
    /// Whether this element was modified during processing
    pub modified: bool,
    /// Family-specific processed data (via TagFamily::ProcessedData)
    pub family_data: F::ProcessedData,
}

// Manual trait implementations
impl<F: TagFamily> std::fmt::Debug for ProcessedElemExt<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessedElemExt")
            .field("stable_id", &self.stable_id)
            .field("modified", &self.modified)
            .field("family_data", &self.family_data)
            .finish()
    }
}

impl<F: TagFamily> Clone for ProcessedElemExt<F> {
    fn clone(&self) -> Self {
        Self {
            stable_id: self.stable_id,
            modified: self.modified,
            family_data: self.family_data.clone(),
        }
    }
}

impl<F: TagFamily> Default for ProcessedElemExt<F> {
    fn default() -> Self {
        Self {
            stable_id: super::id::StableId::default(),
            modified: false,
            family_data: F::ProcessedData::default(),
        }
    }
}

impl PhaseData for Processed {
    type DocExt = ProcessedDocExt;
    type ElemExt<F: TagFamily> = ProcessedElemExt<F>;
    type TextExt = ();
}

/// Document extension for Processed phase
#[derive(Debug, Clone, Default)]
pub struct ProcessedDocExt {
    pub svg_count: usize,
    pub total_svg_bytes: usize,
    pub links_resolved: usize,
    pub headings_anchored: usize,
}

impl ProcessedDocExt {
    /// Average SVG size in bytes
    pub fn avg_svg_bytes(&self) -> usize {
        if self.svg_count > 0 {
            self.total_svg_bytes / self.svg_count
        } else {
            0
        }
    }

    /// Total processing count
    pub fn total_processed(&self) -> usize {
        self.svg_count + self.links_resolved + self.headings_anchored
    }
}

/// Rendered phase: Final HTML output
///
/// # Semantic Notes
///
/// At Rendered phase, the document has been serialized to HTML.
/// The tree structure is retained for debugging/introspection purposes,
/// but the actual output is in `RenderedDocExt::html`.
///
/// ## Why keep the tree?
///
/// 1. **Debugging**: Compare tree structure with output HTML
/// 2. **Introspection**: Count elements, verify transformations
/// 3. **Future**: Potential incremental re-rendering
///
/// ## Element data at Rendered phase
///
/// `ElemExt<F> = ()` because:
/// - All family-specific processing is complete at Processed phase
/// - Rendered elements only need tag/attrs/children for serialization
/// - Keeping data would be wasteful since HTML is already generated
#[derive(Debug, Clone, Copy)]
pub struct Rendered;

impl Phase for Rendered {
    const NAME: &'static str = "rendered";
}

impl PhaseData for Rendered {
    type DocExt = RenderedDocExt;
    /// Element extensions are `()` at Rendered phase.
    ///
    /// All family-specific data was used during Processed → Rendered transformation.
    /// The tree is retained for debugging, but element data is no longer needed.
    type ElemExt<F: TagFamily> = ();
    type TextExt = ();
}

/// Document extension for Rendered phase - final output data
#[derive(Debug, Clone, Default)]
pub struct RenderedDocExt {
    /// Final HTML string
    pub html: String,
    /// Number of assets referenced
    pub asset_count: usize,
    /// Total output size in bytes
    pub output_bytes: usize,
}

impl RenderedDocExt {
    /// HTML byte length
    pub fn html_bytes(&self) -> usize {
        self.html.len()
    }

    /// Check if HTML is empty
    pub fn is_empty(&self) -> bool {
        self.html.is_empty()
    }

    /// Count lines in HTML
    pub fn line_count(&self) -> usize {
        self.html.lines().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_names() {
        assert_eq!(Raw::NAME, "raw");
        assert_eq!(Indexed::NAME, "indexed");
        assert_eq!(Processed::NAME, "processed");
        assert_eq!(Rendered::NAME, "rendered");
    }
}
