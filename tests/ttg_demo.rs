//! TTG (Trees that Grow) Architecture Demo - v8 Improved Version
//!
//! This file contains the complete TTG implementation, improved based on experience from v1-v7.
//! Main changes: Transform trait consumes self + Pipeline builder
//!
//! Core Architecture:
//! - TagFamily trait (family system + GAT, with process() conversion method)
//! - FamilyExt enum (zero-cost family extension, replaces Box<dyn Any>)
//! - FamilyKind enum (type-safe runtime family identification)
//! - Phase/PhaseData trait (phase system + GAT)
//! - HasFamilyData trait (unified family data access)
//! - Transform<P> trait (type-safe phase transitions)
//! - Pipeline (chainable builder)
//! - Frame node handling (deferred SVG rendering)
//!
//! 🔧 v8 Improvements (aggressive rewrite):
//! - Removed Visitor/MutVisitor/StatsVisitor (replaced with closures + query API)
//! - Added doc.pipe(T) chainable transforms
//! - Added doc.find_element(), find_all(), has_element(), has_frames()
//! - LinkProcessor converted to Transform<Indexed>
//! - Transform trait simplified: consumes self, no need for name() method
//!
//! Run: cargo test --test ttg_demo -- --nocapture


use std::fmt::Debug;

// =============================================================================
// Part 1: TagFamily System (Compile-time Type Discrimination)
// =============================================================================

// 🔧 Critique #8: Forward declare FamilyKind so TagFamily can reference it
/// Type-safe enum for family identification results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyKind {
    Svg,
    Link,
    Heading,
    Media,
    Other,
}

/// 🔧 Critique #33: FamilyKind helper methods
impl FamilyKind {
    /// Returns all FamilyKind variants
    pub const fn all() -> &'static [FamilyKind] {
        &[Self::Svg, Self::Link, Self::Heading, Self::Media, Self::Other]
    }

    /// Number of variants
    pub const fn count() -> usize {
        5
    }
}

/// TagFamily trait - Compile-time type discrimination
///
/// Each HTML element family (SVG, Link, Heading, etc.) implements this trait.
/// Uses GATs (IndexedData, ProcessedData) for phase-specific data.
/// 🔧 v7 improvement: Added KIND constant for compile-time ↔ runtime bidirectional mapping
/// 🔧 v8 improvement: Added process() method for phase transitions
///
/// 🔧 Critique #8 improvement: Add KIND associated constant for bidirectional mapping with FamilyKind
pub trait TagFamily: 'static + Send + Sync {
    /// Family name (for debugging and logging)
    const NAME: &'static str;
    /// 🔧 Critique #8: Compile-time family identifier constant
    /// Maps compile-time type → runtime enum
    /// Pairs with FamilyKind::into_default_ext() for reverse mapping
    const KIND: FamilyKind;  // 🔧 New: Type-level FamilyKind

    /// Family-specific data for Indexed phase
    type IndexedData: Debug + Clone + Default + Send + Sync;

    /// Family-specific data for Processed phase
    type ProcessedData: Debug + Clone + Default + Send + Sync;

    /// 🔧 v8 new: Phase transition method
    /// Converts IndexedData to ProcessedData
    fn process(indexed: &Self::IndexedData) -> Self::ProcessedData;
}

// ─────────────────────────────────────────────────────────────────────────────
// Family Definitions
// ─────────────────────────────────────────────────────────────────────────────

/// SVG family: <svg>, <path>, <circle>, <rect>, <g>, ...
pub struct SvgFamily;

/// 🔧 Critique #32: SvgFamily associated methods
impl SvgFamily {
    /// Check if tag is an SVG element
    pub fn is_svg_tag(tag: &str) -> bool {
        is_svg_tag(tag)
    }

    /// All SVG container elements
    pub const CONTAINER_TAGS: &'static [&'static str] = &[
        "svg", "g", "defs", "symbol", "use", "switch"
    ];

    /// All SVG shape elements
    pub const SHAPE_TAGS: &'static [&'static str] = &[
        "path", "circle", "rect", "line", "polyline", "polygon", "ellipse"
    ];
}

impl TagFamily for SvgFamily {
    const NAME: &'static str = "svg";
    const KIND: FamilyKind = FamilyKind::Svg;
    type IndexedData = SvgIndexedData;
    type ProcessedData = SvgProcessedData;

    fn process(_indexed: &Self::IndexedData) -> Self::ProcessedData {
        SvgProcessedData {
            optimized: false,
            original_bytes: 0,
            optimized_bytes: 0,
            extracted_path: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SvgIndexedData {
    pub is_root: bool,
    pub viewbox: Option<String>,
    pub dimensions: Option<(f32, f32)>,
}

/// 🔧 Critique #21: SvgIndexedData convenience methods
impl SvgIndexedData {
    /// Parse viewBox string to (min_x, min_y, width, height)
    /// e.g., "0 0 100 200" → Some((0.0, 0.0, 100.0, 200.0))
    pub fn parse_viewbox(&self) -> Option<(f32, f32, f32, f32)> {
        let vb = self.viewbox.as_ref()?;
        let parts: Vec<f32> = vb.split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if parts.len() == 4 {
            Some((parts[0], parts[1], parts[2], parts[3]))
        } else {
            None
        }
    }

    /// Get width and height from viewBox
    pub fn viewbox_dimensions(&self) -> Option<(f32, f32)> {
        self.parse_viewbox().map(|(_, _, w, h)| (w, h))
    }

    /// Get effective dimensions (prefers dimensions, otherwise infers from viewBox)
    pub fn effective_dimensions(&self) -> Option<(f32, f32)> {
        self.dimensions.or_else(|| self.viewbox_dimensions())
    }
}

/// SVG processing result
#[derive(Debug, Clone, Default)]
pub struct SvgProcessedData {
    pub optimized: bool,
    pub original_bytes: usize,
    pub optimized_bytes: usize,
    pub extracted_path: Option<String>,  // If extracted to external file
}

/// Link family: <a>, any element with href/src attribute
pub struct LinkFamily;
impl TagFamily for LinkFamily {
    const NAME: &'static str = "link";
    const KIND: FamilyKind = FamilyKind::Link;
    type IndexedData = LinkIndexedData;
    type ProcessedData = LinkProcessedData;

    fn process(indexed: &Self::IndexedData) -> Self::ProcessedData {
        LinkProcessedData {
            resolved_url: indexed.original_href.clone(),
            is_external: indexed.link_type == LinkType::External,
            is_broken: false,
            nofollow: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LinkIndexedData {
    pub link_type: LinkType,
    pub original_href: Option<String>,
}

/// Link processing result
#[derive(Debug, Clone, Default)]
pub struct LinkProcessedData {
    pub resolved_url: Option<String>,
    pub is_external: bool,
    pub is_broken: bool,
    pub nofollow: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum LinkType {
    #[default]
    None,
    Absolute,   // /path
    Relative,   // ./file
    Fragment,   // #anchor
    External,   // https://...
}

impl LinkType {
    /// 🔧 Critique #4: Infer link type from href string
    /// Moves classification logic from IndexerFolder to the type itself
    pub fn from_href(href: &str) -> Self {
        if href.starts_with("http://") || href.starts_with("https://") || href.starts_with("//") {
            Self::External
        } else if href.starts_with('/') {
            Self::Absolute
        } else if href.starts_with('#') {
            Self::Fragment
        } else {
            Self::Relative
        }
    }

    /// 🔧 Critique #19: Convenience methods to check link type
    pub fn is_none(&self) -> bool { matches!(self, Self::None) }
    pub fn is_absolute(&self) -> bool { matches!(self, Self::Absolute) }
    pub fn is_relative(&self) -> bool { matches!(self, Self::Relative) }
    pub fn is_fragment(&self) -> bool { matches!(self, Self::Fragment) }
    pub fn is_external(&self) -> bool { matches!(self, Self::External) }

    /// Whether it's an internal link (not external)
    pub fn is_internal(&self) -> bool {
        !matches!(self, Self::External | Self::None)
    }
}

/// Heading family: <h1> - <h6>
pub struct HeadingFamily;
impl TagFamily for HeadingFamily {
    const NAME: &'static str = "heading";
    const KIND: FamilyKind = FamilyKind::Heading;
    type IndexedData = HeadingIndexedData;
    type ProcessedData = HeadingProcessedData;

    fn process(indexed: &Self::IndexedData) -> Self::ProcessedData {
        HeadingProcessedData {
            anchor_id: indexed.original_id.clone().unwrap_or_default(),
            toc_text: String::new(),
            in_toc: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HeadingIndexedData {
    pub level: u8,
    pub original_id: Option<String>,
}

impl HeadingIndexedData {
    /// 🔧 Critique #5: Parse heading level from tag name
    /// "h1" → 1, "h2" → 2, ..., "h6" → 6
    pub fn level_from_tag(tag: &str) -> u8 {
        tag.chars()
            .last()
            .and_then(|c| c.to_digit(10))
            .unwrap_or(1) as u8
    }

    /// 🔧 Critique #20: Heading level convenience methods
    pub fn is_h1(&self) -> bool { self.level == 1 }
    pub fn is_h2(&self) -> bool { self.level == 2 }
    pub fn is_h3(&self) -> bool { self.level == 3 }
    pub fn is_h4(&self) -> bool { self.level == 4 }
    pub fn is_h5(&self) -> bool { self.level == 5 }
    pub fn is_h6(&self) -> bool { self.level == 6 }

    /// Whether it's a top-level heading (h1 or h2)
    pub fn is_top_level(&self) -> bool { self.level <= 2 }
}

/// Heading processing result
#[derive(Debug, Clone, Default)]
pub struct HeadingProcessedData {
    pub anchor_id: String,        // slugified ID
    pub toc_text: String,         // Plain text for table of contents
    pub in_toc: bool,             // Whether included in table of contents
}

/// 🔧 Critique #34: HeadingProcessedData convenience methods
impl HeadingProcessedData {
    /// Whether it has a valid anchor ID
    pub fn has_anchor(&self) -> bool {
        !self.anchor_id.is_empty()
    }

    /// Get anchor link for HTML
    pub fn anchor_href(&self) -> String {
        format!("#{}", self.anchor_id)
    }
}

/// Media family: <img>, <video>, <audio>
pub struct MediaFamily;
impl TagFamily for MediaFamily {
    const NAME: &'static str = "media";
    const KIND: FamilyKind = FamilyKind::Media;
    type IndexedData = MediaIndexedData;
    type ProcessedData = MediaProcessedData;

    fn process(indexed: &Self::IndexedData) -> Self::ProcessedData {
        MediaProcessedData {
            resolved_src: indexed.src.clone(),
            width: None,
            height: None,
            lazy_load: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MediaIndexedData {
    pub src: Option<String>,
    pub is_svg_image: bool,
}

/// 🔧 Critique #25: MediaIndexedData media type detection
impl MediaIndexedData {
    /// Infer media type from file extension
    pub fn media_type(&self) -> MediaType {
        let src = match &self.src {
            Some(s) => s.to_lowercase(),
            None => return MediaType::Unknown,
        };

        if src.ends_with(".svg") {
            MediaType::Svg
        } else if src.ends_with(".png") || src.ends_with(".jpg") || src.ends_with(".jpeg")
            || src.ends_with(".gif") || src.ends_with(".webp") || src.ends_with(".avif") {
            MediaType::Image
        } else if src.ends_with(".mp4") || src.ends_with(".webm") || src.ends_with(".ogg") {
            MediaType::Video
        } else if src.ends_with(".mp3") || src.ends_with(".wav") || src.ends_with(".flac") {
            MediaType::Audio
        } else {
            MediaType::Unknown
        }
    }

    /// Whether it's an image type
    pub fn is_image(&self) -> bool {
        matches!(self.media_type(), MediaType::Image | MediaType::Svg)
    }

    /// Whether it's a video type
    pub fn is_video(&self) -> bool {
        matches!(self.media_type(), MediaType::Video)
    }
}

/// Media type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Image,
    Svg,
    Video,
    Audio,
    Unknown,
}

/// Media processing result
#[derive(Debug, Clone, Default)]
pub struct MediaProcessedData {
    pub resolved_src: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub lazy_load: bool,
}

/// Other family: All other elements
pub struct OtherFamily;
impl TagFamily for OtherFamily {
    const NAME: &'static str = "other";
    const KIND: FamilyKind = FamilyKind::Other;
    type IndexedData = ();
    type ProcessedData = ();

    fn process(_indexed: &Self::IndexedData) -> Self::ProcessedData {
        // 🔧 Critique #12: clippy suggests removing redundant ()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Runtime Family Identification
// ─────────────────────────────────────────────────────────────────────────────

/// 🔧 Type-safe family identification result
impl FamilyKind {
    /// Get family name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Svg => SvgFamily::NAME,
            Self::Link => LinkFamily::NAME,
            Self::Heading => HeadingFamily::NAME,
            Self::Media => MediaFamily::NAME,
            Self::Other => OtherFamily::NAME,
        }
    }

    /// Create default `FamilyExt` from runtime enum
    ///
    /// This bridges runtime FamilyKind → compile-time TagFamily
    pub fn into_default_ext<P: PhaseData>(self) -> FamilyExt<P>
    where
        P::ElemExt<SvgFamily>: Default,
        P::ElemExt<LinkFamily>: Default,
        P::ElemExt<HeadingFamily>: Default,
        P::ElemExt<MediaFamily>: Default,
        P::ElemExt<OtherFamily>: Default,
    {
        match self {
            Self::Svg => FamilyExt::Svg(P::ElemExt::<SvgFamily>::default()),
            Self::Link => FamilyExt::Link(P::ElemExt::<LinkFamily>::default()),
            Self::Heading => FamilyExt::Heading(P::ElemExt::<HeadingFamily>::default()),
            Self::Media => FamilyExt::Media(P::ElemExt::<MediaFamily>::default()),
            Self::Other => FamilyExt::Other(P::ElemExt::<OtherFamily>::default()),
        }
    }
}

/// Identify family by tag name and attributes (type-safe version)
pub fn identify_family_kind(tag: &str, attrs: &[(String, String)]) -> FamilyKind {
    if is_svg_tag(tag) {
        return FamilyKind::Svg;
    }

    match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => FamilyKind::Heading,
        "img" | "video" | "audio" | "source" | "track" | "picture" | "canvas" | "embed" | "object" => FamilyKind::Media,
        "a" => FamilyKind::Link,
        _ if attrs.iter().any(|(k, _)| k == "href" || k == "src") => FamilyKind::Link,
        _ => FamilyKind::Other,
    }
}

/// Identify family by tag name and attributes (string version, for legacy code)
pub fn identify_family(tag: &str, attrs: &[(String, String)]) -> &'static str {
    identify_family_kind(tag, attrs).name()
}

/// Check if tag is an SVG element (complete list)
fn is_svg_tag(tag: &str) -> bool {
    matches!(tag,
        // Container elements
        "svg" | "g" | "defs" | "symbol" | "use" | "switch"
        // Shape elements
        | "path" | "circle" | "rect" | "line" | "polyline" | "polygon" | "ellipse"
        // Text elements
        | "text" | "tspan" | "textPath"
        // Gradient elements
        | "linearGradient" | "radialGradient" | "stop"
        // Clipping and masking
        | "clipPath" | "mask" | "pattern"
        // Filter elements (all fe* prefix)
        | "filter" | "feBlend" | "feColorMatrix" | "feComponentTransfer"
        | "feComposite" | "feConvolveMatrix" | "feDiffuseLighting"
        | "feDisplacementMap" | "feDistantLight" | "feDropShadow"
        | "feFlood" | "feFuncR" | "feFuncG" | "feFuncB" | "feFuncA"
        | "feGaussianBlur" | "feImage" | "feMerge" | "feMergeNode"
        | "feMorphology" | "feOffset" | "fePointLight" | "feSpecularLighting"
        | "feSpotLight" | "feTile" | "feTurbulence"
        // Animation elements
        | "animate" | "animateMotion" | "animateTransform" | "set" | "mpath"
        // Other SVG elements
        | "image" | "foreignObject" | "marker" | "metadata"
        | "view" | "cursor" | "font" | "glyph" | "hkern" | "vkern"
        | "font-face" | "font-face-src" | "font-face-uri" | "font-face-format"
        | "font-face-name" | "missing-glyph"
    )
}

// =============================================================================
// Part 2: Phase System
// =============================================================================

/// Phase trait - Marks a processing stage (zero runtime overhead)
pub trait Phase: Debug + Clone + Send + Sync + 'static {
    const NAME: &'static str;
}

/// PhaseData trait - Phase-specific extension data (specialized by family using GATs)
pub trait PhaseData: Phase {
    type DocExt: Debug + Clone + Default + Send + Sync;
    type ElemExt<F: TagFamily>: Debug + Clone + Default + Send + Sync;
    type TextExt: Debug + Clone + Default + Send + Sync;
    type FrameExt: Debug + Clone + Send + Sync;
}

// =============================================================================
// Part 3: FamilyExt Enum (Zero-Cost Family Extension, Replaces Box<dyn Any>)
// =============================================================================

/// Family extension enum - Compile-time determined, zero runtime overhead
///
/// Key design: Uses an enum to replace Box<dyn Any>
/// - ✅ Stack allocated (no heap overhead)
/// - ✅ Compile-time size known
/// - ✅ Pattern matching (no downcast overhead)
#[derive(Debug, Clone)]
pub enum FamilyExt<P: PhaseData> {
    Svg(P::ElemExt<SvgFamily>),
    Link(P::ElemExt<LinkFamily>),
    Heading(P::ElemExt<HeadingFamily>),
    Media(P::ElemExt<MediaFamily>),
    Other(P::ElemExt<OtherFamily>),
}

impl<P: PhaseData> FamilyExt<P> {
    pub fn family_name(&self) -> &'static str {
        match self {
            Self::Svg(_) => SvgFamily::NAME,
            Self::Link(_) => LinkFamily::NAME,
            Self::Heading(_) => HeadingFamily::NAME,
            Self::Media(_) => MediaFamily::NAME,
            Self::Other(_) => OtherFamily::NAME,
        }
    }

    /// 🔧 Critique #13: Get FamilyKind for current variant
    /// Forms complete bidirectional mapping with `TagFamily::KIND`
    pub fn kind(&self) -> FamilyKind {
        match self {
            Self::Svg(_) => FamilyKind::Svg,
            Self::Link(_) => FamilyKind::Link,
            Self::Heading(_) => FamilyKind::Heading,
            Self::Media(_) => FamilyKind::Media,
            Self::Other(_) => FamilyKind::Other,
        }
    }

    // 🔧 Critique #6: Complete set of is_* methods
    pub fn is_svg(&self) -> bool { matches!(self, Self::Svg(_)) }
    pub fn is_link(&self) -> bool { matches!(self, Self::Link(_)) }
    pub fn is_heading(&self) -> bool { matches!(self, Self::Heading(_)) }
    pub fn is_media(&self) -> bool { matches!(self, Self::Media(_)) }
    pub fn is_other(&self) -> bool { matches!(self, Self::Other(_)) }

    // 🔧 Critique #6: Complete set of as_* methods (ordered by family)
    pub fn as_svg(&self) -> Option<&P::ElemExt<SvgFamily>> {
        match self { Self::Svg(e) => Some(e), _ => None }
    }
    pub fn as_svg_mut(&mut self) -> Option<&mut P::ElemExt<SvgFamily>> {
        match self { Self::Svg(e) => Some(e), _ => None }
    }

    pub fn as_link(&self) -> Option<&P::ElemExt<LinkFamily>> {
        match self { Self::Link(e) => Some(e), _ => None }
    }
    pub fn as_link_mut(&mut self) -> Option<&mut P::ElemExt<LinkFamily>> {
        match self { Self::Link(e) => Some(e), _ => None }
    }

    pub fn as_heading(&self) -> Option<&P::ElemExt<HeadingFamily>> {
        match self { Self::Heading(e) => Some(e), _ => None }
    }
    pub fn as_heading_mut(&mut self) -> Option<&mut P::ElemExt<HeadingFamily>> {
        match self { Self::Heading(e) => Some(e), _ => None }
    }

    pub fn as_media(&self) -> Option<&P::ElemExt<MediaFamily>> {
        match self { Self::Media(e) => Some(e), _ => None }
    }
    pub fn as_media_mut(&mut self) -> Option<&mut P::ElemExt<MediaFamily>> {
        match self { Self::Media(e) => Some(e), _ => None }
    }

    pub fn as_other(&self) -> Option<&P::ElemExt<OtherFamily>> {
        match self { Self::Other(e) => Some(e), _ => None }
    }
    pub fn as_other_mut(&mut self) -> Option<&mut P::ElemExt<OtherFamily>> {
        match self { Self::Other(e) => Some(e), _ => None }
    }

    // 🔧 Critique #8: Removed useless map_preserving_family stub
    // Actual mapping uses map_family_ext! macro
}

impl<P: PhaseData> Default for FamilyExt<P> {
    fn default() -> Self { Self::Other(Default::default()) }
}

// 🔧 Critique #1: Add node_id accessor for Indexed phase
// Avoids writing 5 match branches every time
impl FamilyExt<Indexed> {
    /// Get node_id (field common to all families)
    pub fn node_id(&self) -> NodeId {
        match self {
            Self::Svg(e) => e.node_id,
            Self::Link(e) => e.node_id,
            Self::Heading(e) => e.node_id,
            Self::Media(e) => e.node_id,
            Self::Other(e) => e.node_id,
        }
    }
}

// 🔧 Critique #2: Add modified accessor for Processed phase
impl FamilyExt<Processed> {
    /// Get modified flag (field common to all families)
    pub fn is_modified(&self) -> bool {
        match self {
            Self::Svg(e) => e.modified,
            Self::Link(e) => e.modified,
            Self::Heading(e) => e.modified,
            Self::Media(e) => e.modified,
            Self::Other(e) => e.modified,
        }
    }

    /// Set modified flag
    pub fn set_modified(&mut self, value: bool) {
        match self {
            Self::Svg(e) => e.modified = value,
            Self::Link(e) => e.modified = value,
            Self::Heading(e) => e.modified = value,
            Self::Media(e) => e.modified = value,
            Self::Other(e) => e.modified = value,
        }
    }
}

// =============================================================================
// Part 3.1: FamilyExt Transformation Macros (Eliminate Duplicate Match Code)
// =============================================================================

/// Map FamilyExt to new phase, preserving family information
///
/// Note: This macro is for scenarios where all families have the same ElemExt type
/// (e.g., transformations within Indexed phase).
/// For cross-phase transformations like Indexed → Processed, it's recommended
/// to manually expand match to correctly handle each family's ProcessedData.
///
/// Usage (only for same-phase or unified ElemExt type scenarios):
/// ```ignore
/// let new_ext: FamilyExt<Indexed> = map_family_ext!(old_ext, |old| IndexedElemExt {
///     node_id: old.node_id,
///     family_data: old.family_data.clone(),
/// });
/// ```
#[macro_export]
macro_rules! map_family_ext {
    ($ext:expr, |$family:pat_param| $new_ext:expr) => {
        match $ext {
            FamilyExt::Svg($family) => FamilyExt::Svg($new_ext),
            FamilyExt::Link($family) => FamilyExt::Link($new_ext),
            FamilyExt::Heading($family) => FamilyExt::Heading($new_ext),
            FamilyExt::Media($family) => FamilyExt::Media($new_ext),
            FamilyExt::Other($family) => FamilyExt::Other($new_ext),
        }
    };
}

/// 🔧 Critique #14: Indexed → Processed cross-phase transformation macro
/// Automatically calls `TagFamily::process()` and wraps in `ProcessedElemExt`
///
/// Usage:
/// ```ignore
/// let processed_ext = process_family_ext!(indexed_ext);
/// ```
#[macro_export]
macro_rules! process_family_ext {
    ($ext:expr) => {
        match $ext {
            FamilyExt::Svg(indexed) => FamilyExt::Svg(ProcessedElemExt {
                modified: false,
                family_data: SvgFamily::process(&indexed.family_data),
            }),
            FamilyExt::Link(indexed) => FamilyExt::Link(ProcessedElemExt {
                modified: false,
                family_data: LinkFamily::process(&indexed.family_data),
            }),
            FamilyExt::Heading(indexed) => FamilyExt::Heading(ProcessedElemExt {
                modified: false,
                family_data: HeadingFamily::process(&indexed.family_data),
            }),
            FamilyExt::Media(indexed) => FamilyExt::Media(ProcessedElemExt {
                modified: false,
                family_data: MediaFamily::process(&indexed.family_data),
            }),
            FamilyExt::Other(indexed) => FamilyExt::Other(ProcessedElemExt {
                modified: false,
                family_data: OtherFamily::process(&indexed.family_data),
            }),
        }
    };
}

// =============================================================================
// Part 3.2: Unified Extension Data Accessor Trait (Solve Pattern Matching Interference)
// =============================================================================

/// Unified NodeId access (for Indexed phase)
///
/// Key insight: The HasType pattern mentioned by user is very valuable.
/// We define accessors for each phase, encapsulating FamilyExt details.
pub trait HasNodeId {
    fn node_id(&self) -> NodeId;
}

/// Implement HasNodeId for Element in Indexed phase
/// 🔧 Critique #1 improvement: Delegate to FamilyExt::node_id() to avoid duplicate match
impl HasNodeId for Element<Indexed> {
    fn node_id(&self) -> NodeId {
        self.ext.node_id()
    }
}

// =============================================================================
// Part 3.3: Elegant Family Data Access (v7 Improvement)
// =============================================================================

/// 🔑 v7 improvement: Remove PhaseFamilyData intermediate layer
///
/// Before (over-abstracted):
/// ```ignore
/// pub trait HasFamilyData<P: PhaseData, F: PhaseFamilyData<P>> {
///     fn family_data(&self) -> Option<&F::Data>;  // F::Data indirectly depends on P
/// }
/// // Call: <Element<Indexed> as HasFamilyData<Indexed, LinkFamily>>::family_data(&e)
/// ```
///
/// Now (more elegant):
/// ```ignore
/// pub trait HasFamilyData<F: TagFamily> {
///     type Data;  // Associated type, determined by impl
///     fn family_data(&self) -> Option<&Self::Data>;
/// }
/// // Call: <Element<Indexed> as HasFamilyData<LinkFamily>>::family_data(&e)
/// //       or more concise: elem.family_data::<LinkFamily>()  (requires turbofish)
/// ```
///
/// Key improvements:
/// 1. One less type parameter (P is determined by Self)
/// 2. `Data` is an associated type, specified in impl
/// 3. Removed useless `PhaseFamilyData` intermediate layer
pub trait HasFamilyData<F: TagFamily> {
    /// Family data type (determined by impl)
    /// - `Element<Indexed>` + `LinkFamily` → `LinkIndexedData`
    /// - `Element<Processed>` + `LinkFamily` → `LinkProcessedData`
    type Data;

    fn family_data(&self) -> Option<&Self::Data>;
    fn family_data_mut(&mut self) -> Option<&mut Self::Data>;
}

// ─────────────────────────────────────────────────────────────────────────────
// HasFamilyData Implementation (Use macro to eliminate duplicate code)
// ─────────────────────────────────────────────────────────────────────────────

/// 🔧 Eliminate duplication: Implement HasFamilyData<F> for Element<P>
///
/// Macro generates impl for all (Element<Phase>, Family) combinations
macro_rules! impl_has_family_data {
    // Unified implementation pattern
    ($phase:ty, $family:ty, $as_method:ident, $as_mut_method:ident, $data_type:ty) => {
        impl HasFamilyData<$family> for Element<$phase> {
            type Data = $data_type;

            fn family_data(&self) -> Option<&Self::Data> {
                self.ext.$as_method().map(|e| &e.family_data)
            }
            fn family_data_mut(&mut self) -> Option<&mut Self::Data> {
                self.ext.$as_mut_method().map(|e| &mut e.family_data)
            }
        }
    };
}

// Indexed phase
impl_has_family_data!(Indexed, LinkFamily, as_link, as_link_mut, LinkIndexedData);
impl_has_family_data!(Indexed, HeadingFamily, as_heading, as_heading_mut, HeadingIndexedData);
impl_has_family_data!(Indexed, SvgFamily, as_svg, as_svg_mut, SvgIndexedData);
impl_has_family_data!(Indexed, MediaFamily, as_media, as_media_mut, MediaIndexedData);

// Processed phase
impl_has_family_data!(Processed, LinkFamily, as_link, as_link_mut, LinkProcessedData);
impl_has_family_data!(Processed, HeadingFamily, as_heading, as_heading_mut, HeadingProcessedData);
impl_has_family_data!(Processed, SvgFamily, as_svg, as_svg_mut, SvgProcessedData);
impl_has_family_data!(Processed, MediaFamily, as_media, as_media_mut, MediaProcessedData);

// 🔧 Q3: OtherFamily implementation (data is () but keep for completeness)
impl_has_family_data!(Indexed, OtherFamily, as_other, as_other_mut, ());
impl_has_family_data!(Processed, OtherFamily, as_other, as_other_mut, ());

// =============================================================================
// Part 4: Phase Definitions
// =============================================================================

/// Phase 1: Raw - Converted from typst-html
#[derive(Debug, Clone)]
pub struct Raw;

impl Phase for Raw {
    const NAME: &'static str = "raw";
}

impl PhaseData for Raw {
    type DocExt = RawDocExt;
    type ElemExt<F: TagFamily> = ();  // Raw phase has no element extension
    type TextExt = ();
    type FrameExt = RawFrameExt;
}

#[derive(Debug, Clone, Default)]
pub struct RawDocExt {
    pub source_path: Option<String>,
    pub is_index: bool,
}

/// 🔧 Critique #36: RawDocExt convenience methods
impl RawDocExt {
    /// Whether it has a source file path
    pub fn has_source(&self) -> bool {
        self.source_path.is_some()
    }

    /// Get source file extension
    pub fn source_extension(&self) -> Option<&str> {
        self.source_path.as_ref()?
            .rsplit('.')
            .next()
    }

    /// Get source filename (without path)
    pub fn source_filename(&self) -> Option<&str> {
        self.source_path.as_ref()?
            .rsplit('/')
            .next()
    }
}

/// Frame extension - Keeps reference to original typst Frame
#[derive(Debug, Clone)]
pub struct RawFrameExt {
    /// Simulated typst Frame data (real implementation would reference actual Frame)
    pub frame_id: u32,
    pub estimated_size: (f32, f32),
}

/// 🔧 Critique #37: RawFrameExt convenience methods
impl RawFrameExt {
    /// Estimate area
    pub fn area(&self) -> f32 {
        self.estimated_size.0 * self.estimated_size.1
    }

    /// Aspect ratio
    pub fn aspect_ratio(&self) -> f32 {
        if self.estimated_size.1 > 0.0 {
            self.estimated_size.0 / self.estimated_size.1
        } else {
            1.0
        }
    }

    /// Whether it's landscape (width > height)
    pub fn is_landscape(&self) -> bool {
        self.estimated_size.0 > self.estimated_size.1
    }
}

impl Default for RawFrameExt {
    fn default() -> Self {
        Self { frame_id: 0, estimated_size: (100.0, 100.0) }
    }
}

/// Phase 2: Indexed - Nodes have assigned IDs, family data populated
#[derive(Debug, Clone)]
pub struct Indexed;

impl Phase for Indexed {
    const NAME: &'static str = "indexed";
}

impl PhaseData for Indexed {
    type DocExt = IndexedDocExt;
    type ElemExt<F: TagFamily> = IndexedElemExt<F>;
    type TextExt = NodeId;
    type FrameExt = IndexedFrameExt;
}

#[derive(Debug, Clone, Default)]
pub struct IndexedDocExt {
    pub base: RawDocExt,
    pub node_count: u32,
    pub svg_nodes: Vec<NodeId>,
    pub link_nodes: Vec<NodeId>,
    pub heading_nodes: Vec<NodeId>,
    pub media_nodes: Vec<NodeId>,  // 🔧 Critique #10: Add media_nodes
    pub frame_nodes: Vec<NodeId>,
}

/// 🔧 Critique #28: IndexedDocExt statistics methods
impl IndexedDocExt {
    /// Total indexed node count
    pub fn total_indexed_nodes(&self) -> usize {
        self.svg_nodes.len() + self.link_nodes.len() +
        self.heading_nodes.len() + self.media_nodes.len() + self.frame_nodes.len()
    }

    /// Whether it has SVG nodes
    pub fn has_svg(&self) -> bool { !self.svg_nodes.is_empty() }

    /// Whether it has link nodes
    pub fn has_links(&self) -> bool { !self.link_nodes.is_empty() }

    /// Whether it has heading nodes
    pub fn has_headings(&self) -> bool { !self.heading_nodes.is_empty() }

    /// Whether it has Frame nodes
    pub fn has_frames(&self) -> bool { !self.frame_nodes.is_empty() }
}

/// Element extension with family-specific data (Indexed phase)
pub struct IndexedElemExt<F: TagFamily> {
    pub node_id: NodeId,
    pub family_data: F::IndexedData,  // Uses IndexedData
}

// Manual Debug implementation (because F doesn't need Debug)
impl<F: TagFamily> Debug for IndexedElemExt<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexedElemExt")
            .field("node_id", &self.node_id)
            .field("family_data", &self.family_data)
            .finish()
    }
}

// Manual Clone implementation (because F doesn't need Clone)
impl<F: TagFamily> Clone for IndexedElemExt<F> {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id,
            family_data: self.family_data.clone(),
        }
    }
}

impl<F: TagFamily> Default for IndexedElemExt<F> {
    fn default() -> Self {
        Self {
            node_id: NodeId(0),
            family_data: F::IndexedData::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexedFrameExt {
    pub node_id: NodeId,
    pub frame_id: u32,
    pub estimated_svg_size: usize,
}

impl Default for IndexedFrameExt {
    fn default() -> Self {
        Self { node_id: NodeId(0), frame_id: 0, estimated_svg_size: 0 }
    }
}

/// Phase 3: Processed - All transforms applied, Frames expanded
#[derive(Debug, Clone)]
pub struct Processed;

impl Phase for Processed {
    const NAME: &'static str = "processed";
}

impl PhaseData for Processed {
    type DocExt = ProcessedDocExt;
    type ElemExt<F: TagFamily> = ProcessedElemExt<F>;  // 🔑 True GAT specialization!
    type TextExt = ();
    type FrameExt = std::convert::Infallible;  // Frames all converted to Elements
}

#[derive(Debug, Clone, Default)]
pub struct ProcessedDocExt {
    pub svg_count: usize,
    pub total_svg_bytes: usize,
    pub links_transformed: usize,
    pub frames_expanded: usize,
}

/// 🔧 Critique #22: ProcessedDocExt summary methods
impl ProcessedDocExt {
    /// Average SVG size (bytes)
    pub fn avg_svg_bytes(&self) -> usize {
        if self.svg_count > 0 {
            self.total_svg_bytes / self.svg_count
        } else {
            0
        }
    }

    /// Total processed count
    pub fn total_processed(&self) -> usize {
        self.svg_count + self.links_transformed + self.frames_expanded
    }
}

/// Element extension with family-specific processing results (Processed phase)
/// 🔑 This is the true value of GAT: each family has different processing results!
pub struct ProcessedElemExt<F: TagFamily> {
    pub modified: bool,
    pub family_data: F::ProcessedData,  // Uses ProcessedData
}

impl<F: TagFamily> Debug for ProcessedElemExt<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessedElemExt")
            .field("modified", &self.modified)
            .field("family_data", &self.family_data)
            .finish()
    }
}

impl<F: TagFamily> Clone for ProcessedElemExt<F> {
    fn clone(&self) -> Self {
        Self {
            modified: self.modified,
            family_data: self.family_data.clone(),
        }
    }
}

impl<F: TagFamily> Default for ProcessedElemExt<F> {
    fn default() -> Self {
        Self {
            modified: false,
            family_data: F::ProcessedData::default(),
        }
    }
}

/// Phase 4: Rendered - Final HTML
#[derive(Debug, Clone)]
pub struct Rendered;

impl Phase for Rendered {
    const NAME: &'static str = "rendered";
}

impl PhaseData for Rendered {
    type DocExt = RenderedDocExt;
    type ElemExt<F: TagFamily> = ();
    type TextExt = ();
    type FrameExt = std::convert::Infallible;
}

#[derive(Debug, Clone, Default)]
pub struct RenderedDocExt {
    pub html: String,
    pub asset_count: usize,
}

/// 🔧 Critique #29: RenderedDocExt convenience methods
impl RenderedDocExt {
    /// HTML byte count
    pub fn html_bytes(&self) -> usize {
        self.html.len()
    }

    /// Whether HTML is empty
    pub fn is_empty(&self) -> bool {
        self.html.is_empty()
    }

    /// HTML line count
    pub fn line_count(&self) -> usize {
        self.html.lines().count()
    }
}

// =============================================================================
// Part 5: DOM Structure
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NodeId(pub u32);

/// 🔧 Critique #18: Add Display for NodeId
impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// DOM node
#[derive(Debug, Clone)]
pub enum Node<P: PhaseData> {
    Element(Element<P>),
    Text(Text<P>),
    Frame(Frame<P>),
}

/// 🔧 Critique #15: Node convenience methods
impl<P: PhaseData> Node<P> {
    pub fn is_element(&self) -> bool { matches!(self, Self::Element(_)) }
    pub fn is_text(&self) -> bool { matches!(self, Self::Text(_)) }
    pub fn is_frame(&self) -> bool { matches!(self, Self::Frame(_)) }

    pub fn as_element(&self) -> Option<&Element<P>> {
        match self { Self::Element(e) => Some(e), _ => None }
    }
    pub fn as_element_mut(&mut self) -> Option<&mut Element<P>> {
        match self { Self::Element(e) => Some(e), _ => None }
    }

    pub fn as_text(&self) -> Option<&Text<P>> {
        match self { Self::Text(t) => Some(t), _ => None }
    }
    pub fn as_text_mut(&mut self) -> Option<&mut Text<P>> {
        match self { Self::Text(t) => Some(t), _ => None }
    }

    pub fn as_frame(&self) -> Option<&Frame<P>> {
        match self { Self::Frame(f) => Some(f), _ => None }
    }
    pub fn as_frame_mut(&mut self) -> Option<&mut Frame<P>> {
        match self { Self::Frame(f) => Some(f), _ => None }
    }
}

/// Element node - Uses FamilyExt enum (zero overhead)
#[derive(Debug, Clone)]
pub struct Element<P: PhaseData> {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node<P>>,
    pub ext: FamilyExt<P>,
}

impl<P: PhaseData> Element<P> {
    /// Create SVG family element
    pub fn svg(tag: &str, attrs: Vec<(String, String)>, children: Vec<Node<P>>, ext: P::ElemExt<SvgFamily>) -> Self {
        Self { tag: tag.into(), attrs, children, ext: FamilyExt::Svg(ext) }
    }

    /// Create Link family element
    pub fn link(tag: &str, attrs: Vec<(String, String)>, children: Vec<Node<P>>, ext: P::ElemExt<LinkFamily>) -> Self {
        Self { tag: tag.into(), attrs, children, ext: FamilyExt::Link(ext) }
    }

    /// Create Heading family element
    pub fn heading(tag: &str, attrs: Vec<(String, String)>, children: Vec<Node<P>>, ext: P::ElemExt<HeadingFamily>) -> Self {
        Self { tag: tag.into(), attrs, children, ext: FamilyExt::Heading(ext) }
    }

    /// Create Media family element
    pub fn media(tag: &str, attrs: Vec<(String, String)>, children: Vec<Node<P>>, ext: P::ElemExt<MediaFamily>) -> Self {
        Self { tag: tag.into(), attrs, children, ext: FamilyExt::Media(ext) }
    }

    /// Create Other family element
    pub fn other(tag: &str, attrs: Vec<(String, String)>, children: Vec<Node<P>>, ext: P::ElemExt<OtherFamily>) -> Self {
        Self { tag: tag.into(), attrs, children, ext: FamilyExt::Other(ext) }
    }

    /// Auto-identify family by tag and create element
    pub fn auto(tag: &str, attrs: Vec<(String, String)>, children: Vec<Node<P>>) -> Self
    where
        P::ElemExt<SvgFamily>: Default,
        P::ElemExt<LinkFamily>: Default,
        P::ElemExt<HeadingFamily>: Default,
        P::ElemExt<MediaFamily>: Default,
        P::ElemExt<OtherFamily>: Default,
    {

        let kind = identify_family_kind(tag, &attrs);
        Self {
            tag: tag.into(),
            attrs,
            children,
            ext: kind.into_default_ext(),
        }
    }

    pub fn family(&self) -> &'static str { self.ext.family_name() }

    /// 🔧 Critique #3: Helper method to get attribute value
    /// Eliminates repetitive `.iter().find(|(k, _)| k == "xxx").map(|(_, v)| v.clone())` pattern
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// 🔧 Critique #16: Set attribute value (update if exists, add if not)
    pub fn set_attr(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        if let Some(attr) = self.attrs.iter_mut().find(|(k, _)| k == &name) {
            attr.1 = value;
        } else {
            self.attrs.push((name, value));
        }
    }

    /// 🔧 Critique #16: Remove attribute, return removed value
    pub fn remove_attr(&mut self, name: &str) -> Option<String> {
        if let Some(pos) = self.attrs.iter().position(|(k, _)| k == name) {
            Some(self.attrs.remove(pos).1)
        } else {
            None
        }
    }

    /// 🔧 Critique #16: Check if attribute exists
    pub fn has_attr(&self, name: &str) -> bool {
        self.attrs.iter().any(|(k, _)| k == name)
    }

    /// 🔧 Critique #30: Iterate child elements (filter out Text and Frame)
    pub fn children_elements(&self) -> impl Iterator<Item = &Element<P>> {
        self.children.iter().filter_map(|n| n.as_element())
    }

    /// Mutable iterate child elements
    pub fn children_elements_mut(&mut self) -> impl Iterator<Item = &mut Element<P>> {
        self.children.iter_mut().filter_map(|n| n.as_element_mut())
    }

    /// Child element count
    pub fn element_count(&self) -> usize {
        self.children.iter().filter(|n| n.is_element()).count()
    }

    /// Whether it has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Whether it's a leaf node (no child elements)
    pub fn is_leaf(&self) -> bool {
        self.children.iter().all(|n| !n.is_element())
    }

    /// 🔧 Critique #39: Extract plain text content from all child nodes
    pub fn text_content(&self) -> String {
        let mut result = String::new();
        Self::collect_text(&self.children, &mut result);
        result
    }

    /// Recursively collect text
    fn collect_text(nodes: &[Node<P>], output: &mut String) {
        for node in nodes {
            match node {
                Node::Text(t) => output.push_str(&t.content),
                Node::Element(e) => Self::collect_text(&e.children, output),
                Node::Frame(_) => {} // Frame doesn't contain text
            }
        }
    }
}

/// Text node
#[derive(Debug, Clone)]
pub struct Text<P: PhaseData> {
    pub content: String,
    pub ext: P::TextExt,
}

/// 🔧 Critique #26: Text convenience methods
impl<P: PhaseData> Text<P> {
    /// Create text node with default extension
    pub fn new(content: impl Into<String>) -> Self
    where
        P::TextExt: Default,
    {
        Self {
            content: content.into(),
            ext: P::TextExt::default(),
        }
    }

    /// Whether text is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Text length
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Whether it's whitespace only after trimming
    pub fn is_whitespace_only(&self) -> bool {
        self.content.trim().is_empty()
    }
}

/// Frame node - Typst render frame (diagrams, formulas)
#[derive(Debug, Clone)]
pub struct Frame<P: PhaseData> {
    pub ext: P::FrameExt,
}

/// Document
#[derive(Debug, Clone)]
pub struct Document<P: PhaseData> {
    pub root: Element<P>,
    pub ext: P::DocExt,
}

/// 🔧 Critique #24: Document convenience methods
impl<P: PhaseData> Document<P> {
    /// Get current phase name
    pub fn phase_name(&self) -> &'static str {
        P::NAME
    }

    /// Create new document
    pub fn new(root: Element<P>, ext: P::DocExt) -> Self {
        Self { root, ext }
    }
}

// =============================================================================
// Part 6: Folder trait (Internal phase transition helper, for Indexer/FrameExpander)
// =============================================================================

/// Transform folder - Internal phase transition helper
///
/// Note: External code should use Transform trait + Pipeline, not Folder directly
trait Folder<From: PhaseData, To: PhaseData> {
    fn fold_doc_ext(&mut self, ext: From::DocExt) -> To::DocExt;
    fn fold_element(&mut self, elem: Element<From>) -> Element<To>;
    fn fold_text(&mut self, text: Text<From>) -> Text<To>;
    fn fold_frame(&mut self, frame: Frame<From>) -> Node<To>;

    fn fold_node(&mut self, node: Node<From>) -> Node<To> {
        match node {
            Node::Element(e) => Node::Element(self.fold_element(e)),
            Node::Text(t) => Node::Text(self.fold_text(t)),
            Node::Frame(f) => self.fold_frame(f),
        }
    }

    fn fold_children(&mut self, children: Vec<Node<From>>) -> Vec<Node<To>> {
        children.into_iter().map(|n| self.fold_node(n)).collect()
    }
}

fn fold<From, To, F>(doc: Document<From>, folder: &mut F) -> Document<To>
where
    From: PhaseData,
    To: PhaseData,
    F: Folder<From, To>,
{
    Document {
        root: folder.fold_element(doc.root),
        ext: folder.fold_doc_ext(doc.ext),
    }
}

// =============================================================================
// Part 7: Pipeline + Transform API
// =============================================================================


/// 🔧 Critique #3: Helper function to get attribute value from list
fn get_attr_from_list(attrs: &[(String, String)], name: &str) -> Option<String> {
    attrs.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

// -----------------------------------------------------------------------------
// New Transform trait: Only need to implement one method
// -----------------------------------------------------------------------------

/// Phase transition trait (v8 new version)
///
/// Improvements over old version:
/// - Only need to implement one method `transform`
/// - Consumes self, allowing Transform to hold state
/// - Works with Pipeline
pub trait Transform<From: PhaseData> {
    type To: PhaseData;
    fn transform(self, doc: Document<From>) -> Document<Self::To>;
}


// -----------------------------------------------------------------------------
// Pipeline: Type-safe chaining
// -----------------------------------------------------------------------------

/// Pipeline builder - Type-safe chainable Transform calls
pub struct Pipeline<P: PhaseData> {
    doc: Document<P>,
}

impl<P: PhaseData> Pipeline<P> {
    /// Create new Pipeline
    pub fn new(doc: Document<P>) -> Self {
        Self { doc }
    }

    /// Apply phase transition Transform: P → T::To
    pub fn then<T>(self, transform: T) -> Pipeline<T::To>
    where
        T: Transform<P>,
        T::To: PhaseData,
    {
        Pipeline { doc: transform.transform(self.doc) }
    }

    /// Same-phase in-place modification (closure version)
    pub fn apply<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut Document<P>),
    {
        f(&mut self.doc);
        self
    }

    /// Finish Pipeline, return final document
    pub fn finish(self) -> Document<P> {
        self.doc
    }
}

// -----------------------------------------------------------------------------
// Document helper methods: Support closure-style traversal
// -----------------------------------------------------------------------------

impl<P: PhaseData> Document<P> {
    /// Traverse all elements (read-only)
    pub fn for_each_element<F>(&self, mut f: F)
    where
        F: FnMut(&Element<P>),
    {
        Self::visit_elements_recursive(&self.root, &mut f);
    }

    fn visit_elements_recursive<F>(elem: &Element<P>, f: &mut F)
    where
        F: FnMut(&Element<P>),
    {
        f(elem);
        for child in &elem.children {
            if let Node::Element(e) = child {
                Self::visit_elements_recursive(e, f);
            }
        }
    }

    /// Traverse all elements (mutable)
    pub fn for_each_element_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut Element<P>),
    {
        Self::visit_elements_mut_recursive(&mut self.root, &mut f);
    }

    fn visit_elements_mut_recursive<F>(elem: &mut Element<P>, f: &mut F)
    where
        F: FnMut(&mut Element<P>),
    {
        f(elem);
        for child in &mut elem.children {
            if let Node::Element(e) = child {
                Self::visit_elements_mut_recursive(e, f);
            }
        }
    }

    /// Collect statistics
    pub fn collect_stats(&self) -> Stats {
        let mut stats = Stats::default();
        self.for_each_element(|elem| {
            match elem.ext.kind() {
                FamilyKind::Svg => stats.svg_count += 1,
                FamilyKind::Link => stats.link_count += 1,
                FamilyKind::Heading => stats.heading_count += 1,
                FamilyKind::Media => stats.media_count += 1,
                FamilyKind::Other => {}
            }
        });
        // Count frames separately
        fn count_frames<P: PhaseData>(elem: &Element<P>) -> usize {
            let mut count = 0;
            for child in &elem.children {
                match child {
                    Node::Frame(_) => count += 1,
                    Node::Element(e) => count += count_frames(e),
                    _ => {}
                }
            }
            count
        }
        stats.frame_count = count_frames(&self.root);
        stats
    }

    // =========================================================================
    // Chaining API
    // =========================================================================

    /// Chain transform: `doc.pipe(Indexer).pipe(FrameExpander)`
    pub fn pipe<T: Transform<P>>(self, transform: T) -> Document<T::To> {
        transform.transform(self)
    }

    // =========================================================================
    // Query API (Lazy iterators)
    // =========================================================================

    /// Find first element matching predicate
    pub fn find_element<F>(&self, predicate: F) -> Option<&Element<P>>
    where
        F: Fn(&Element<P>) -> bool,
    {
        Self::find_in_element(&self.root, &predicate)
    }

    fn find_in_element<'a, F>(elem: &'a Element<P>, pred: &F) -> Option<&'a Element<P>>
    where
        F: Fn(&Element<P>) -> bool,
    {
        if pred(elem) {
            return Some(elem);
        }
        for child in &elem.children {
            if let Node::Element(e) = child {
                if let Some(found) = Self::find_in_element(e, pred) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Find first element matching predicate (mutable)
    pub fn find_element_mut<F>(&mut self, predicate: F) -> Option<&mut Element<P>>
    where
        F: Fn(&Element<P>) -> bool + Copy,
    {
        Self::find_in_element_mut(&mut self.root, predicate)
    }

    fn find_in_element_mut<F>(elem: &mut Element<P>, pred: F) -> Option<&mut Element<P>>
    where
        F: Fn(&Element<P>) -> bool + Copy,
    {
        if pred(elem) {
            return Some(elem);
        }
        for child in &mut elem.children {
            if let Node::Element(e) = child {
                if let Some(found) = Self::find_in_element_mut(e, pred) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Collect all elements matching predicate (returns reference vector)
    pub fn find_all<'a, F>(&'a self, predicate: F) -> Vec<&'a Element<P>>
    where
        F: Fn(&Element<P>) -> bool,
    {
        let mut results = Vec::new();
        Self::collect_elements(&self.root, &predicate, &mut results);
        results
    }

    fn collect_elements<'a, F>(elem: &'a Element<P>, pred: &F, results: &mut Vec<&'a Element<P>>)
    where
        F: Fn(&Element<P>) -> bool,
    {
        if pred(elem) {
            results.push(elem);
        }
        for child in &elem.children {
            if let Node::Element(e) = child {
                Self::collect_elements(e, pred, results);
            }
        }
    }


    /// Check if an element matching predicate exists
    pub fn has_element<F>(&self, predicate: F) -> bool
    where
        F: Fn(&Element<P>) -> bool,
    {
        self.find_element(predicate).is_some()
    }

    /// Check if any Frame node exists
    pub fn has_frames(&self) -> bool {
        fn check<P: PhaseData>(elem: &Element<P>) -> bool {
            for child in &elem.children {
                match child {
                    Node::Frame(_) => return true,
                    Node::Element(e) => if check(e) { return true; }
                    _ => {}
                }
            }
            false
        }
        check(&self.root)
    }
}

/// Statistics
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub svg_count: usize,
    pub link_count: usize,
    pub heading_count: usize,
    pub media_count: usize,
    pub frame_count: usize,
}


// =============================================================================
// Part 10: Transform Implementations
// =============================================================================

// ─────────────────────────────────────────────────────────────────────────────
// Indexer: Raw → Indexed (Uses Folder)
// ─────────────────────────────────────────────────────────────────────────────

pub struct Indexer;

impl Transform<Raw> for Indexer {
    type To = Indexed;

    fn transform(self, doc: Document<Raw>) -> Document<Indexed> {
        let mut folder = IndexerFolder::new(doc.ext.clone());
        fold(doc, &mut folder)
    }
}


struct IndexerFolder {
    next_id: u32,
    doc_ext: IndexedDocExt,
}

impl IndexerFolder {
    fn new(base: RawDocExt) -> Self {
        Self {
            next_id: 0,
            doc_ext: IndexedDocExt { base, ..Default::default() },
        }
    }

    fn next_id(&mut self) -> NodeId {
        // 🔧 Fix D9: Add overflow check
        let id = NodeId(self.next_id);
        self.next_id = self.next_id.checked_add(1)
            .expect("NodeId overflow: more than 4 billion nodes");
        id
    }
}

impl Folder<Raw, Indexed> for IndexerFolder {
    fn fold_doc_ext(&mut self, _ext: RawDocExt) -> IndexedDocExt {
        self.doc_ext.node_count = self.next_id;
        std::mem::take(&mut self.doc_ext)
    }

    fn fold_element(&mut self, elem: Element<Raw>) -> Element<Indexed> {
        let node_id = self.next_id();
        let family_kind = identify_family_kind(&elem.tag, &elem.attrs);
        let children = self.fold_children(elem.children);

        // 🔧 Use type-safe FamilyKind matching + Critique #3 helper function
        match family_kind {
            FamilyKind::Svg => {
                self.doc_ext.svg_nodes.push(node_id);
                let viewbox = get_attr_from_list(&elem.attrs, "viewBox");
                Element::svg(&elem.tag, elem.attrs, children, IndexedElemExt {
                    node_id,
                    family_data: SvgIndexedData { is_root: true, viewbox, dimensions: None },
                })
            }
            FamilyKind::Link => {
                self.doc_ext.link_nodes.push(node_id);
                let original_href = get_attr_from_list(&elem.attrs, "href");
                // 🔧 Critique #4: Use LinkType::from_href
                let link_type = original_href.as_ref()
                    .map(|h| LinkType::from_href(h))
                    .unwrap_or_default();
                Element::link(&elem.tag, elem.attrs, children, IndexedElemExt {
                    node_id,
                    family_data: LinkIndexedData { link_type, original_href },
                })
            }
            FamilyKind::Heading => {
                self.doc_ext.heading_nodes.push(node_id);
                // 🔧 Critique #5: Use HeadingIndexedData::level_from_tag
                let level = HeadingIndexedData::level_from_tag(&elem.tag);
                let original_id = get_attr_from_list(&elem.attrs, "id");
                Element::heading(&elem.tag, elem.attrs, children, IndexedElemExt {
                    node_id,
                    family_data: HeadingIndexedData { level, original_id },
                })
            }
            FamilyKind::Media => {
                // 🔧 Critique #10: Record media nodes
                self.doc_ext.media_nodes.push(node_id);
                let src = get_attr_from_list(&elem.attrs, "src");
                let is_svg_image = src.as_ref().map(|s| s.ends_with(".svg")).unwrap_or(false);
                Element::media(&elem.tag, elem.attrs, children, IndexedElemExt {
                    node_id,
                    family_data: MediaIndexedData { src, is_svg_image },
                })
            }
            FamilyKind::Other => {
                Element::other(&elem.tag, elem.attrs, children, IndexedElemExt {
                    node_id,
                    family_data: (),
                })
            }
        }
    }

    fn fold_text(&mut self, text: Text<Raw>) -> Text<Indexed> {
        Text { content: text.content, ext: self.next_id() }
    }

    fn fold_frame(&mut self, frame: Frame<Raw>) -> Node<Indexed> {
        let node_id = self.next_id();
        self.doc_ext.frame_nodes.push(node_id);
        Node::Frame(Frame {
            ext: IndexedFrameExt {
                node_id,
                frame_id: frame.ext.frame_id,
                estimated_svg_size: (frame.ext.estimated_size.0 * frame.ext.estimated_size.1) as usize,
            },
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LinkProcessor: Indexed → Indexed (Same-phase Transform)
// ─────────────────────────────────────────────────────────────────────────────

/// Link processing Transform
pub struct LinkProcessor {
    pub prefix: Option<String>,
}

impl LinkProcessor {
    pub fn new() -> Self {
        Self { prefix: None }
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }
}

impl Default for LinkProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform<Indexed> for LinkProcessor {
    type To = Indexed;

    fn transform(self, mut doc: Document<Indexed>) -> Document<Indexed> {
        if let Some(ref prefix) = self.prefix {
            doc.for_each_element_mut(|elem| {
                if let Some(link_ext) = elem.ext.as_link_mut() {
                    if link_ext.family_data.link_type == LinkType::Absolute {
                        if let Some(href) = elem.attrs.iter_mut().find(|(k, _)| k == "href") {
                            let path = href.1.trim_start_matches('/');
                            href.1 = format!("/{}/{}", prefix, path);
                        }
                    }
                }
            });
        }
        doc
    }
}

/// Convenience function (backwards compatible)
pub fn process_links(doc: &mut Document<Indexed>, prefix: Option<&str>) {
    if let Some(p) = prefix {
        doc.for_each_element_mut(|elem| {
            if let Some(link_ext) = elem.ext.as_link_mut() {
                if link_ext.family_data.link_type == LinkType::Absolute {
                    if let Some(href) = elem.attrs.iter_mut().find(|(k, _)| k == "href") {
                        let path = href.1.trim_start_matches('/');
                        href.1 = format!("/{}/{}", p, path);
                    }
                }
            }
        });
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// FrameExpander: Indexed → Processed（Frame → SVG Element）
// ─────────────────────────────────────────────────────────────────────────────

pub struct FrameExpander;

impl Transform<Indexed> for FrameExpander {
    type To = Processed;

    fn transform(self, doc: Document<Indexed>) -> Document<Processed> {
        let mut folder = FrameExpanderFolder::new();
        fold(doc, &mut folder)
    }
}


struct FrameExpanderFolder {
    svg_count: usize,
    total_svg_bytes: usize,
    frames_expanded: usize,
}

impl FrameExpanderFolder {
    fn new() -> Self {
        Self { svg_count: 0, total_svg_bytes: 0, frames_expanded: 0 }
    }
}

impl Folder<Indexed, Processed> for FrameExpanderFolder {
    fn fold_doc_ext(&mut self, ext: IndexedDocExt) -> ProcessedDocExt {
        ProcessedDocExt {
            svg_count: self.svg_count,
            total_svg_bytes: self.total_svg_bytes,
            links_transformed: ext.link_nodes.len(),
            frames_expanded: self.frames_expanded,
        }
    }

    fn fold_element(&mut self, elem: Element<Indexed>) -> Element<Processed> {
        if elem.ext.is_svg() {
            self.svg_count += 1;
            self.total_svg_bytes += 100; // Simulated SVG byte size
        }

        let children = self.fold_children(elem.children);

        // 🔧 Critique #14 improvement: Use process_family_ext! macro to simplify
        let ext = process_family_ext!(elem.ext);

        Element {
            tag: elem.tag,
            attrs: elem.attrs,
            children,
            ext,
        }
    }

    fn fold_text(&mut self, text: Text<Indexed>) -> Text<Processed> {
        Text { content: text.content, ext: () }
    }

    fn fold_frame(&mut self, frame: Frame<Indexed>) -> Node<Processed> {
        // 🔑 Key: Frame → SVG Element
        // Real implementation would call typst_render::render_svg(frame)
        self.frames_expanded += 1;
        self.svg_count += 1;
        self.total_svg_bytes += frame.ext.estimated_svg_size;

        // Simulated generated SVG element
        Node::Element(Element {
            tag: "svg".into(),
            attrs: vec![
                ("class".into(), "typst-frame".into()),
                ("data-frame-id".into(), frame.ext.frame_id.to_string()),
            ],
            children: vec![
                Node::Text(Text { content: "<!-- rendered from typst frame -->".into(), ext: () }),
            ],
            ext: FamilyExt::Svg(ProcessedElemExt {
                modified: true,  // SVG produced from Frame transform is newly generated
                family_data: SvgProcessedData {
                    optimized: false,
                    original_bytes: frame.ext.estimated_svg_size,  // Original size
                    optimized_bytes: 0,  // Not optimized
                    extracted_path: None,
                },
            }),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HtmlRenderer: Processed → Rendered
// ─────────────────────────────────────────────────────────────────────────────

pub struct HtmlRenderer;

impl Transform<Processed> for HtmlRenderer {
    type To = Rendered;

    fn transform(self, doc: Document<Processed>) -> Document<Rendered> {
        let mut output = String::new();
        render_node(&Node::Element(doc.root), &mut output);

        // 🔧 Fix D3: Rendered phase root is placeholder (HTML serialized to ext.html)
        Document {
            root: Element {
                tag: "html".into(),
                attrs: vec![("data-rendered".into(), "true".into())],
                children: vec![],
                ext: FamilyExt::Other(()),
            },
            ext: RenderedDocExt {
                html: output,
                asset_count: doc.ext.svg_count,
            },
        }
    }
}


/// HTML void elements (cannot have closing tag)
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "param", "source", "track", "wbr",
];

/// 🔧 Critique #7: Consolidate HTML escape logic
/// Escape HTML text content (basic escaping)
fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}

/// Escape HTML attribute value (text escape + quote escape)
fn escape_attr(s: &str) -> String {
    escape_text(s).replace('"', "&quot;")
}

fn render_node(node: &Node<Processed>, output: &mut String) {
    match node {
        Node::Text(t) => output.push_str(&escape_text(&t.content)),
        Node::Element(elem) => {
            // Skip empty tag (placeholder root in Rendered phase)
            if elem.tag.is_empty() {
                for child in &elem.children {
                    render_node(child, output);
                }
                return;
            }

            output.push('<');
            output.push_str(&elem.tag);
            for (k, v) in &elem.attrs {
                output.push(' ');
                output.push_str(k);
                output.push_str("=\"");
                output.push_str(&escape_attr(v));
                output.push('"');
            }

            // Void elements should not have closing tag
            if VOID_ELEMENTS.contains(&elem.tag.as_str()) {
                output.push_str(" />");
            } else {
                output.push('>');
                for child in &elem.children {
                    render_node(child, output);
                }
                output.push_str("</");
                output.push_str(&elem.tag);
                output.push('>');
            }
        }
        Node::Frame(_) => unreachable!("Frames should be expanded before rendering"),
    }
}

// =============================================================================
// Part 8: Tests
// =============================================================================


#[cfg(test)]
mod tests {
    use super::*;

    /// Create test document (contains Frame nodes)
    fn create_test_document() -> Document<Raw> {
        Document {
            root: Element::other("html", vec![], vec![
                Node::Element(Element::other("body", vec![], vec![
                    Node::Text(Text { content: "Hello World".into(), ext: () }),
                    Node::Element(Element::auto("svg", vec![("viewBox".into(), "0 0 100 100".into())], vec![])),
                    Node::Element(Element::auto("a", vec![("href".into(), "/about".into())], vec![
                        Node::Text(Text { content: "About".into(), ext: () }),
                    ])),
                    Node::Element(Element::auto("h1", vec![("id".into(), "title".into())], vec![
                        Node::Text(Text { content: "Title".into(), ext: () }),
                    ])),
                    // Frame node (simulates Typst render frame)
                    Node::Frame(Frame { ext: RawFrameExt { frame_id: 1, estimated_size: (200.0, 150.0) } }),
                    Node::Element(Element::auto("svg", vec![], vec![])),
                    Node::Frame(Frame { ext: RawFrameExt { frame_id: 2, estimated_size: (100.0, 100.0) } }),
                ], ())),
            ], ()),
            ext: RawDocExt { source_path: Some("test.typ".into()), is_index: false },
        }
    }

    #[test]
    fn test_family_identification() {
        assert_eq!(identify_family("svg", &[]), "svg");
        assert_eq!(identify_family("path", &[]), "svg");
        assert_eq!(identify_family("a", &[]), "link");
        assert_eq!(identify_family("h1", &[]), "heading");
        assert_eq!(identify_family("img", &[]), "media");
        assert_eq!(identify_family("div", &[]), "other");

        // href attribute makes any element a link family
        let attrs = vec![("href".into(), "/path".into())];
        assert_eq!(identify_family("div", &attrs), "link");
    }

    /// 🔧 Critique #8 test: FamilyKind ↔ TagFamily bidirectional mapping
    #[test]
    fn test_family_kind_bidirectional_mapping() {
        // Verify TagFamily::KIND constants (compile-time → runtime)
        assert_eq!(SvgFamily::KIND, FamilyKind::Svg);
        assert_eq!(LinkFamily::KIND, FamilyKind::Link);
        assert_eq!(HeadingFamily::KIND, FamilyKind::Heading);
        assert_eq!(MediaFamily::KIND, FamilyKind::Media);
        assert_eq!(OtherFamily::KIND, FamilyKind::Other);

        // Verify FamilyKind::name() matches TagFamily::NAME
        assert_eq!(FamilyKind::Svg.name(), SvgFamily::NAME);
        assert_eq!(FamilyKind::Link.name(), LinkFamily::NAME);
        assert_eq!(FamilyKind::Heading.name(), HeadingFamily::NAME);
        assert_eq!(FamilyKind::Media.name(), MediaFamily::NAME);
        assert_eq!(FamilyKind::Other.name(), OtherFamily::NAME);

        // Verify FamilyKind::into_default_ext() (runtime → compile-time)
        let svg_ext: FamilyExt<Indexed> = FamilyKind::Svg.into_default_ext();
        assert!(svg_ext.is_svg());
        assert_eq!(svg_ext.family_name(), "svg");

        let link_ext: FamilyExt<Indexed> = FamilyKind::Link.into_default_ext();
        assert!(link_ext.is_link());
        assert_eq!(link_ext.family_name(), "link");

        // Verify Element::auto uses new simplified implementation
        let elem = Element::<Indexed>::auto("svg", vec![], vec![]);
        assert!(elem.ext.is_svg());
    }

    #[test]
    fn test_family_ext_zero_cost() {
        // Verify FamilyExt methods
        let ext: FamilyExt<Indexed> = FamilyExt::Svg(IndexedElemExt {
            node_id: NodeId(0),
            family_data: SvgIndexedData { is_root: true, viewbox: Some("0 0 100 100".into()), dimensions: None },
        });

        assert!(ext.is_svg());
        assert!(!ext.is_link());
        assert_eq!(ext.family_name(), "svg");

        // 🔧 Critique #13: Verify kind() method
        assert_eq!(ext.kind(), FamilyKind::Svg);

        // kind() should match TagFamily::KIND
        let link_ext: FamilyExt<Indexed> = FamilyKind::Link.into_default_ext();
        assert_eq!(link_ext.kind(), FamilyKind::Link);
        assert_eq!(link_ext.kind(), LinkFamily::KIND);

        if let Some(svg_ext) = ext.as_svg() {
            assert!(svg_ext.family_data.is_root);
            assert_eq!(svg_ext.family_data.viewbox, Some("0 0 100 100".into()));
        }
    }

    #[test]
    fn test_collect_stats_basic() {
        let doc = create_test_document();
        let stats = doc.collect_stats();

        assert_eq!(stats.svg_count, 2);
        assert_eq!(stats.link_count, 1);
        assert_eq!(stats.heading_count, 1);
        assert_eq!(stats.frame_count, 2);

        println!("Stats: svg={}, link={}, heading={}, frame={}",
            stats.svg_count, stats.link_count, stats.heading_count, stats.frame_count);
    }


    #[test]
    fn test_indexer_with_folder() {
        let doc = create_test_document();
        let indexed = Indexer.transform(doc);

        assert_eq!(indexed.ext.svg_nodes.len(), 2);
        assert_eq!(indexed.ext.link_nodes.len(), 1);
        assert_eq!(indexed.ext.heading_nodes.len(), 1);
        assert_eq!(indexed.ext.frame_nodes.len(), 2);

        // Verify family-specific data
        fn find_heading(elem: &Element<Indexed>) -> Option<&IndexedElemExt<HeadingFamily>> {
            if let Some(h) = elem.ext.as_heading() { return Some(h); }
            for child in &elem.children {
                if let Node::Element(e) = child {
                    if let Some(h) = find_heading(e) { return Some(h); }
                }
            }
            None
        }

        let heading = find_heading(&indexed.root).expect("Should find heading");
        assert_eq!(heading.family_data.level, 1);
        assert_eq!(heading.family_data.original_id, Some("title".into()));
    }

    #[test]
    fn test_link_processor_transform() {
        let doc = create_test_document();

        // Use LinkProcessor Transform
        let indexed = doc
            .pipe(Indexer)
            .pipe(LinkProcessor::new().with_prefix("blog"));

        // Use new find_element() method
        let link = indexed.find_element(|e| e.ext.is_link())
            .expect("Should find link");
        let href = link.get_attr("href").expect("Link should have href");
        assert_eq!(href, "/blog/about");
    }

    #[test]
    fn test_find_element_api() {
        let doc = create_test_document();
        let indexed = doc.pipe(Indexer);

        // Test find_element
        let svg = indexed.find_element(|e| e.ext.is_svg());
        assert!(svg.is_some(), "Should find SVG element");

        // Test has_element
        assert!(indexed.has_element(|e| e.ext.is_heading()));
        assert!(!indexed.has_element(|e| e.tag == "nonexistent"));

        // Test find_all
        let all_svgs = indexed.find_all(|e| e.ext.is_svg());
        assert_eq!(all_svgs.len(), 2, "Should find 2 SVG elements");

        // Test has_frames
        assert!(indexed.has_frames(), "Indexed doc should have frames");

        // After Processed, no frames
        let processed = indexed.pipe(FrameExpander);
        assert!(!processed.has_frames(), "Processed doc should not have frames");
    }



    #[test]
    fn test_frame_expansion() {
        let doc = create_test_document();

        // Use new pipe() chain API
        let processed = doc
            .pipe(Indexer)
            .pipe(FrameExpander);

        // Frames should be expanded to SVG
        assert_eq!(processed.ext.frames_expanded, 2);
        assert_eq!(processed.ext.svg_count, 4); // 2 original SVG + 2 Frame→SVG

        // Use new has_frames() method
        assert!(!processed.has_frames(), "No frames should remain after expansion");
    }

    #[test]
    fn test_full_pipeline() {
        let doc = create_test_document();

        // Use new pipe() chain API + LinkProcessor Transform
        let rendered = doc
            .pipe(Indexer)
            .pipe(LinkProcessor::new().with_prefix("blog"))
            .pipe(FrameExpander)
            .pipe(HtmlRenderer);

        assert!(!rendered.ext.html.is_empty());
        assert!(rendered.ext.html.contains("typst-frame")); // Frame expanded to SVG
        assert!(rendered.ext.html.contains("/blog/about")); // Links processed
        assert_eq!(rendered.ext.asset_count, 4);

        println!("Generated HTML ({} bytes):\n{}", rendered.ext.html.len(), rendered.ext.html);
    }



    #[test]
    fn test_phase_type_safety() {
        // Compile-time type safety verification
        let raw_doc = create_test_document();

        // Raw phase: ElemExt is ()
        let _: () = match &raw_doc.root.ext { FamilyExt::Other(e) => e.clone(), _ => () };

        // Indexed phase: ElemExt is IndexedElemExt<F>
        let indexed_doc = Indexer.transform(raw_doc);
        if let FamilyExt::Other(ext) = &indexed_doc.root.ext {
            let _: NodeId = ext.node_id;
        }

        // Processed phase: ElemExt is ProcessedElemExt
        let processed_doc = FrameExpander.transform(indexed_doc);
        if let FamilyExt::Other(ext) = &processed_doc.root.ext {
            let _: bool = ext.modified;
        }

        // Rendered phase: ElemExt is ()
        let rendered_doc = HtmlRenderer.transform(processed_doc);
        let _: () = match &rendered_doc.root.ext { FamilyExt::Other(e) => e.clone(), _ => () };
    }

    #[test]
    fn test_zero_cost_verification() {
        // Verify FamilyExt is stack allocated, no Box
        use std::mem::size_of;

        // FamilyExt size should be largest variant size + discriminant
        let ext_size = size_of::<FamilyExt<Indexed>>();
        println!("FamilyExt<Indexed> size: {} bytes", ext_size);

        // Compare: Box<dyn Any> is at least 16 bytes (two pointers) + heap allocation
        // FamilyExt should be smaller and no heap allocation
        assert!(ext_size < 128, "FamilyExt should be reasonably sized for stack allocation");
    }

    #[test]
    fn test_has_node_id_accessor() {
        // Verify HasNodeId trait provides unified access
        let doc = create_test_document();
        let indexed = Indexer.transform(doc);

        // Use HasNodeId trait for unified node_id access
        let root_id = indexed.root.node_id();
        assert_eq!(root_id, NodeId(0));

        // Recursively find all element node_ids
        fn collect_node_ids(elem: &Element<Indexed>, ids: &mut Vec<NodeId>) {
            ids.push(elem.node_id()); // Unified access, no need to care about family type
            for child in &elem.children {
                if let Node::Element(e) = child {
                    collect_node_ids(e, ids);
                }
            }
        }

        let mut all_ids = vec![];
        collect_node_ids(&indexed.root, &mut all_ids);
        assert!(all_ids.len() >= 5, "Should have multiple elements with node_ids");
        println!("Collected {} element node_ids: {:?}", all_ids.len(), all_ids);
    }

    #[test]
    fn test_has_family_data_accessor() {
        // Verify HasFamilyData trait provides unified phase-aware family data access
        let doc = create_test_document();
        let indexed = Indexer.transform(doc);

        // Find Link element and access its data using unified trait
        fn find_link_data(elem: &Element<Indexed>) -> Option<LinkType> {
            // 🔑 v7 improvement: Only need to specify F, no need for P
            // HasFamilyData<LinkFamily>'s Data associated type is automatically LinkIndexedData
            if let Some(data) = <Element<Indexed> as HasFamilyData<LinkFamily>>::family_data(elem) {
                return Some(data.link_type.clone());
            }
            for child in &elem.children {
                if let Node::Element(e) = child {
                    if let Some(t) = find_link_data(e) { return Some(t); }
                }
            }
            None
        }

        let link_type = find_link_data(&indexed.root);
        assert_eq!(link_type, Some(LinkType::Absolute));

        // Same trait, different phase, access different data
        let processed = FrameExpander.transform(indexed);

        fn find_link_processed(elem: &Element<Processed>) -> Option<bool> {
            // 🔑 Same trait, Element<Processed>'s Data is automatically LinkProcessedData
            if let Some(data) = <Element<Processed> as HasFamilyData<LinkFamily>>::family_data(elem) {
                return Some(data.is_external);
            }
            for child in &elem.children {
                if let Node::Element(e) = child {
                    if let Some(v) = find_link_processed(e) { return Some(v); }
                }
            }
            None
        }

        let is_external = find_link_processed(&processed.root);
        // Original link is Absolute type (internal link), so is_external should be false
        assert_eq!(is_external, Some(false));
    }

    #[test]
    fn test_map_family_ext_macro() {
        // Verify macro correctly preserves family info in same-phase transforms
        let indexed_ext: FamilyExt<Indexed> = FamilyExt::Link(IndexedElemExt {
            node_id: NodeId(42),
            family_data: LinkIndexedData {
                link_type: LinkType::External,
                original_href: Some("https://example.com".into()),
            },
        });

        // Use macro for same-phase transform (clone family_data)
        let cloned_ext: FamilyExt<Indexed> = map_family_ext!(indexed_ext, |old| IndexedElemExt {
            node_id: old.node_id,
            family_data: old.family_data.clone(),
        });

        // 验证族信息被保留
        assert!(cloned_ext.is_link(), "Family should be preserved as Link");
        assert!(!cloned_ext.is_svg(), "Should not become Svg");
        assert_eq!(cloned_ext.family_name(), "link");

        // Verify data was correctly cloned
        if let FamilyExt::Link(ext) = cloned_ext {
            assert_eq!(ext.node_id, NodeId(42));
            assert_eq!(ext.family_data.original_href, Some("https://example.com".into()));
        }
    }

    /// 🔧 Critique #14: Test process_family_ext! macro
    #[test]
    fn test_process_family_ext_macro() {
        // Create Link element extension in Indexed phase
        let indexed_ext: FamilyExt<Indexed> = FamilyExt::Link(IndexedElemExt {
            node_id: NodeId(100),
            family_data: LinkIndexedData {
                link_type: LinkType::External,
                original_href: Some("https://rust-lang.org".into()),
            },
        });

        // Use process_family_ext! macro to convert to Processed phase
        let processed_ext: FamilyExt<Processed> = process_family_ext!(indexed_ext);

        // Verify family info is preserved
        assert!(processed_ext.is_link());
        assert_eq!(processed_ext.kind(), FamilyKind::Link);

        // Verify process() was called correctly
        if let FamilyExt::Link(ext) = processed_ext {
            assert!(!ext.modified); // Default is false
            assert!(ext.family_data.is_external); // External link
            assert_eq!(ext.family_data.resolved_url, Some("https://rust-lang.org".into()));
        } else {
            panic!("Expected Link family");
        }
    }

    /// 🔧 Critique #35: Test newly added convenience methods
    #[test]
    fn test_convenience_methods() {
        // Test FamilyKind::all()
        assert_eq!(FamilyKind::all().len(), FamilyKind::count());
        assert_eq!(FamilyKind::count(), 5);

        // Test LinkType convenience methods
        assert!(LinkType::External.is_external());
        assert!(LinkType::Absolute.is_internal());
        assert!(!LinkType::External.is_internal());
        assert!(LinkType::Fragment.is_fragment());

        // Test HeadingIndexedData convenience methods
        let h1 = HeadingIndexedData { level: 1, original_id: None };
        let h3 = HeadingIndexedData { level: 3, original_id: None };
        assert!(h1.is_h1());
        assert!(h1.is_top_level());
        assert!(h3.is_h3());
        assert!(!h3.is_top_level());

        // Test SvgIndexedData::parse_viewbox
        let svg_data = SvgIndexedData {
            is_root: true,
            viewbox: Some("0 0 100 200".into()),
            dimensions: None,
        };
        assert_eq!(svg_data.parse_viewbox(), Some((0.0, 0.0, 100.0, 200.0)));
        assert_eq!(svg_data.viewbox_dimensions(), Some((100.0, 200.0)));

        // Test MediaIndexedData::media_type
        let img = MediaIndexedData { src: Some("test.png".into()), is_svg_image: false };
        assert!(img.is_image());
        assert!(!img.is_video());

        // Test Node convenience methods
        let text_node: Node<Raw> = Node::Text(Text { content: "hello".into(), ext: () });
        assert!(text_node.is_text());
        assert!(!text_node.is_element());
        assert!(text_node.as_text().is_some());

        // Test Element convenience methods
        let mut elem = Element::<Raw>::auto("div", vec![("id".into(), "test".into())], vec![]);
        assert!(elem.has_attr("id"));
        assert!(!elem.has_attr("class"));
        elem.set_attr("class", "container");
        assert!(elem.has_attr("class"));
        assert_eq!(elem.get_attr("class"), Some("container"));
        elem.remove_attr("class");
        assert!(!elem.has_attr("class"));

        // Test Text convenience methods
        let text = Text::<Raw>::new("  hello  ");
        assert!(!text.is_empty());
        assert!(!text.is_whitespace_only());
        let whitespace = Text::<Raw>::new("   ");
        assert!(whitespace.is_whitespace_only());

        // Test NodeId Display
        let id = NodeId(42);
        assert_eq!(format!("{}", id), "#42");

        // Test SvgFamily associated methods
        assert!(SvgFamily::is_svg_tag("svg"));
        assert!(SvgFamily::is_svg_tag("path"));
        assert!(!SvgFamily::is_svg_tag("div"));
    }

    /// 🔧 Critique #35: Test Document and DocExt convenience methods
    #[test]
    fn test_doc_ext_methods() {
        let doc = create_test_document();
        let indexed = Indexer.transform(doc);

        // Test Document::phase_name
        assert_eq!(indexed.phase_name(), "indexed");

        // Test IndexedDocExt convenience methods
        assert!(indexed.ext.has_svg());
        assert!(indexed.ext.has_links());
        assert!(indexed.ext.has_headings());
        assert!(indexed.ext.has_frames());
        assert!(indexed.ext.total_indexed_nodes() > 0);

        // Test ProcessedDocExt convenience methods
        let processed = FrameExpander.transform(indexed);
        assert_eq!(processed.phase_name(), "processed");
        assert!(processed.ext.avg_svg_bytes() > 0);
        assert!(processed.ext.total_processed() > 0);

        // Test RenderedDocExt convenience methods
        let rendered = HtmlRenderer.transform(processed);
        assert_eq!(rendered.phase_name(), "rendered");
        assert!(!rendered.ext.is_empty());
        assert!(rendered.ext.html_bytes() > 0);
        assert!(rendered.ext.line_count() > 0);
    }

    /// 🔧 D6: HTML escape security test
    #[test]
    fn test_html_escape_security() {
        use super::*;

        // Test text content escaping
        let xss_text = "<script>alert('xss')</script>";
        let escaped = escape_text(xss_text);
        assert!(!escaped.contains('<'), "< should be escaped");
        assert!(!escaped.contains('>'), "> should be escaped");
        assert!(escaped.contains("&lt;"), "< should become &lt;");
        assert!(escaped.contains("&gt;"), "> should become &gt;");

        // Test attribute value escaping
        let malicious_attr = "\" onclick=\"alert('xss')\" data=\"";
        let escaped_attr = escape_attr(malicious_attr);
        assert!(!escaped_attr.contains('"'), "\" should be escaped");
        assert!(escaped_attr.contains("&quot;"), "\" should become &quot;");

        // Test & symbol escaping
        let ampersand_text = "Tom & Jerry";
        let escaped_amp = escape_text(ampersand_text);
        assert!(escaped_amp.contains("&amp;"), "& should become &amp;");

        // End-to-end test: Build document with malicious content and render
        let evil_doc: Document<Raw> = Document {
            root: Element::other("div", vec![], vec![
                Node::Text(Text { content: xss_text.into(), ext: () }),
                Node::Element(Element::other("a",
                    vec![("href".into(), "javascript:alert('xss')".into())],
                    vec![Node::Text(Text { content: "click me".into(), ext: () })],
                    ())),
            ], ()),
            ext: RawDocExt::default(),
        };

        let indexed = Indexer.transform(evil_doc);
        let processed = FrameExpander.transform(indexed);
        let rendered = HtmlRenderer.transform(processed);

        // Verify script tags are escaped
        assert!(!rendered.ext.html.contains("<script>"), "script tags should be escaped");
        assert!(rendered.ext.html.contains("&lt;script&gt;"), "script should appear as escaped");
    }

    /// 🔧 v8 new: Test Pipeline API
    #[test]
    fn test_pipeline_api() {
        let doc = create_test_document();

        // Use new Pipeline API - chain calls
        let rendered = Pipeline::new(doc)
            .then(Indexer)                    // Raw → Indexed
            .apply(|doc| {                    // Closure-style modification
                doc.for_each_element_mut(|elem| {
                    if let Some(link_ext) = elem.ext.as_link_mut() {
                        if link_ext.family_data.link_type == LinkType::Absolute {
                            if let Some(href) = elem.attrs.iter_mut().find(|(k, _)| k == "href") {
                                let path = href.1.trim_start_matches('/');
                                href.1 = format!("/blog/{}", path);
                            }
                        }
                    }
                });
            })
            .then(FrameExpander)              // Indexed → Processed
            .then(HtmlRenderer)               // Processed → Rendered
            .finish();

        assert!(!rendered.ext.html.is_empty());
        assert!(rendered.ext.html.contains("/blog/about"));  // Links processed
        assert!(rendered.ext.html.contains("typst-frame"));  // Frames expanded
    }

    /// 🔧 v8 new: Test Document::collect_stats
    #[test]
    fn test_collect_stats() {
        let doc = create_test_document();
        let stats = doc.collect_stats();

        assert_eq!(stats.svg_count, 2);
        assert_eq!(stats.link_count, 1);
        assert_eq!(stats.heading_count, 1);
        assert_eq!(stats.frame_count, 2);
    }
}