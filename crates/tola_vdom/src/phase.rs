//! Phase definitions for TTG VDOM
//!
//! Defines the four compilation phases:
//! - Raw: Direct parsed output, stores source location
//! - Indexed: Elements indexed with family data
//! - Processed: Links resolved, SVGs optimized
//! - Rendered: Final HTML output
//!
//! ## GAT-based Type System
//!
//! The core insight is `ElemExt<F: TagFamily>` - a GAT that allows different
//! data types for different tag families at each phase:
//!
//! ```text
//! Indexed::ElemExt<LinkFamily>  = IndexedElemExt<LinkIndexedData>
//! Indexed::ElemExt<SvgFamily>   = IndexedElemExt<SvgIndexedData>
//! Processed::ElemExt<LinkFamily> = ProcessedElemExt<LinkProcessedData>
//! ```

use std::fmt::Debug;

use crate::family::TagFamily;
use crate::id::StableId;

// =============================================================================
// Phase trait
// =============================================================================

/// Marker trait for VDOM phases.
///
/// Each phase defines the types used for:
/// - Element extension data (family-specific via GAT)
/// - Text extension data
/// - Document extension data (metadata, stats)
pub trait Phase: 'static + Send + Sync + Debug + Clone {
    /// Phase name for debugging.
    const NAME: &'static str;
}

// =============================================================================
// PhaseData trait (GAT-based extension data)
// =============================================================================

/// Associates extension data types with a phase using GATs.
///
/// 🔑 Key insight: `ElemExt<F>` is a GAT that allows different data types
/// for different tag families at each phase.
pub trait PhaseData: Phase {
    /// Document-level extension data (metadata, stats).
    type DocExt: Debug + Clone + Default + Send + Sync;

    /// Element extension data - GAT parameterized by TagFamily.
    /// This is the core of TTG: each family can have different data at each phase.
    type ElemExt<F: TagFamily>: Debug + Clone + Default + Send + Sync;

    /// Text node extension data.
    type TextExt: Debug + Clone + Default + Send + Sync;
}

// =============================================================================
// Raw Phase - Direct parsed output
// =============================================================================

/// Raw phase: Direct output from parser.
///
/// Contains source location metadata but no family-specific element data.
/// Generic over source location type `S` to support different parsers.
#[derive(Debug, Clone, Copy)]
pub struct Raw<S = ()>(std::marker::PhantomData<S>);

impl<S> Default for Raw<S> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<S: 'static + Send + Sync + Debug + Clone> Phase for Raw<S> {
    const NAME: &'static str = "raw";
}

impl<S: 'static + Send + Sync + Debug + Clone> PhaseData for Raw<S> {
    type DocExt = RawDocExt;
    /// Raw phase element extension - stores source location for StableId generation.
    /// The GAT parameter `F` is ignored here because all families start with
    /// the same source-location-only state.
    type ElemExt<F: TagFamily> = RawElemExt<S>;
    type TextExt = RawTextExt<S>;
}

/// Document extension for Raw phase.
#[derive(Debug, Clone, Default)]
pub struct RawDocExt {
    /// Source file path (relative to content directory).
    pub source_path: Option<String>,
    /// Whether this is an index page.
    pub is_index: bool,
}

/// Element extension for Raw phase - stores source location.
#[derive(Debug, Clone)]
pub struct RawElemExt<S = ()> {
    /// Source location for this element (if available).
    /// Used to generate stable IDs that persist across compilations.
    pub source_loc: Option<S>,
}

// Manual Default impl - doesn't require S: Default
impl<S> Default for RawElemExt<S> {
    fn default() -> Self {
        Self { source_loc: None }
    }
}

impl<S> RawElemExt<S> {
    /// Create with source location.
    pub fn with_source(loc: S) -> Self {
        Self { source_loc: Some(loc) }
    }

    /// Create without source location (detached).
    pub fn detached() -> Self {
        Self { source_loc: None }
    }

    /// Check if this element has a source location.
    pub fn has_source(&self) -> bool {
        self.source_loc.is_some()
    }
}

/// Text node extension for Raw phase.
#[derive(Debug, Clone)]
pub struct RawTextExt<S = ()> {
    /// Source location for this text node (if available).
    pub source_loc: Option<S>,
}

// Manual Default impl - doesn't require S: Default
impl<S> Default for RawTextExt<S> {
    fn default() -> Self {
        Self { source_loc: None }
    }
}

impl<S> RawTextExt<S> {
    /// Create with source location.
    pub fn with_source(loc: S) -> Self {
        Self { source_loc: Some(loc) }
    }

    /// Create without source location.
    pub fn detached() -> Self {
        Self { source_loc: None }
    }
}

// =============================================================================
// Indexed Phase - Elements tagged with family data
// =============================================================================

/// Indexed phase: Elements analyzed and assigned to families.
///
/// Each element has been assigned a stable ID and family-specific
/// indexed data (href, src, viewbox, etc.).
#[derive(Debug, Clone, Copy, Default)]
pub struct Indexed;

impl Phase for Indexed {
    const NAME: &'static str = "indexed";
}

impl PhaseData for Indexed {
    type DocExt = IndexedDocExt;
    type ElemExt<F: TagFamily> = IndexedElemExt<F>;
    type TextExt = IndexedTextExt;
}

/// Indexed phase element extension - contains family-specific indexed data.
pub struct IndexedElemExt<F: TagFamily> {
    /// Stable node identifier for cross-compilation identity.
    pub stable_id: StableId,
    /// Family-specific data (via TagFamily::IndexedData).
    pub family_data: F::IndexedData,
}

// Manual trait implementations (F doesn't require these traits)
impl<F: TagFamily> Debug for IndexedElemExt<F> {
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
            stable_id: StableId::default(),
            family_data: F::IndexedData::default(),
        }
    }
}

/// Text node extension for Indexed phase.
#[derive(Debug, Clone, Default)]
pub struct IndexedTextExt {
    /// Stable node identifier.
    pub stable_id: StableId,
}

/// Document extension for Indexed phase.
#[derive(Debug, Clone, Default)]
pub struct IndexedDocExt {
    /// Base info from Raw phase.
    pub source_path: Option<String>,
    /// Total element count.
    pub element_count: usize,
    /// Total text node count.
    pub text_count: usize,
    /// SVG element count.
    pub svg_count: usize,
    /// Link element count.
    pub link_count: usize,
    /// Heading element count.
    pub heading_count: usize,
    /// Media element count.
    pub media_count: usize,
}

// =============================================================================
// Processed Phase - All transformations applied
// =============================================================================

/// Processed phase: All transformations applied.
///
/// - Links resolved and validated
/// - SVGs optimized
/// - Headings have anchors generated
#[derive(Debug, Clone, Copy, Default)]
pub struct Processed;

impl Phase for Processed {
    const NAME: &'static str = "processed";
}

impl PhaseData for Processed {
    type DocExt = ProcessedDocExt;
    type ElemExt<F: TagFamily> = ProcessedElemExt<F>;
    type TextExt = ProcessedTextExt;
}

/// Processed phase element extension.
pub struct ProcessedElemExt<F: TagFamily> {
    /// Stable node identifier (preserved from Indexed).
    pub stable_id: StableId,
    /// Family-specific processed data.
    pub family_data: F::ProcessedData,
}

impl<F: TagFamily> Debug for ProcessedElemExt<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessedElemExt")
            .field("stable_id", &self.stable_id)
            .field("family_data", &self.family_data)
            .finish()
    }
}

impl<F: TagFamily> Clone for ProcessedElemExt<F> {
    fn clone(&self) -> Self {
        Self {
            stable_id: self.stable_id,
            family_data: self.family_data.clone(),
        }
    }
}

impl<F: TagFamily> Default for ProcessedElemExt<F> {
    fn default() -> Self {
        Self {
            stable_id: StableId::default(),
            family_data: F::ProcessedData::default(),
        }
    }
}

/// Text node extension for Processed phase.
#[derive(Debug, Clone, Default)]
pub struct ProcessedTextExt {
    /// Stable node identifier.
    pub stable_id: StableId,
}

/// Document extension for Processed phase.
#[derive(Debug, Clone, Default)]
pub struct ProcessedDocExt {
    /// Source path.
    pub source_path: Option<String>,
    /// Document title (extracted from h1 or metadata).
    pub title: Option<String>,
    /// All heading anchors for TOC generation.
    pub headings: Vec<HeadingInfo>,
    /// All links for link checking.
    pub links: Vec<LinkInfo>,
}

/// Heading information for TOC.
#[derive(Debug, Clone)]
pub struct HeadingInfo {
    /// Heading level (1-6).
    pub level: u8,
    /// Heading text content.
    pub text: String,
    /// Generated anchor ID.
    pub anchor: String,
    /// StableId of the heading element.
    pub id: StableId,
}

/// Link information for link checking.
#[derive(Debug, Clone)]
pub struct LinkInfo {
    /// Link href.
    pub href: String,
    /// Whether link is internal.
    pub is_internal: bool,
    /// StableId of the link element.
    pub id: StableId,
}

// =============================================================================
// Rendered Phase - Final HTML output
// =============================================================================

/// Rendered phase: Final HTML-ready state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rendered;

impl Phase for Rendered {
    const NAME: &'static str = "rendered";
}

impl PhaseData for Rendered {
    type DocExt = RenderedDocExt;
    type ElemExt<F: TagFamily> = RenderedElemExt;
    type TextExt = RenderedTextExt;
}

/// Rendered phase element extension (minimal - just ID for diffing).
#[derive(Debug, Clone, Default)]
pub struct RenderedElemExt {
    /// Stable ID for diffing.
    pub stable_id: StableId,
}

/// Rendered phase text extension.
#[derive(Debug, Clone, Default)]
pub struct RenderedTextExt {
    /// Stable ID for diffing.
    pub stable_id: StableId,
}

/// Rendered phase document extension.
#[derive(Debug, Clone, Default)]
pub struct RenderedDocExt {
    /// Final HTML content (if pre-rendered).
    pub html: Option<String>,
}
