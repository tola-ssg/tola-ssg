//! TagFamily trait and family definitions
//!
//! The family system is the core of TTG architecture, implementing tag classification and type specialization.

use std::fmt::Debug;

// =============================================================================
// FamilyKind enum
// =============================================================================

/// Type-safe family identification result enum
///
/// NOTE: Currently unused but reserved for runtime family identification.
/// Will be used in `convert.rs` for HTML → Raw VDOM conversion.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FamilyKind {
    Svg,
    Link,
    Heading,
    Media,
    Other,
}

#[allow(dead_code)]
impl FamilyKind {
    /// Returns all FamilyKind variants
    pub const fn all() -> &'static [FamilyKind] {
        &[Self::Svg, Self::Link, Self::Heading, Self::Media, Self::Other]
    }

    /// Number of variants
    pub const fn count() -> usize {
        5
    }

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

    /// Create default FamilyExt from runtime FamilyKind
    ///
    /// This bridges runtime FamilyKind → compile-time FamilyExt.
    /// Useful when creating elements from parsed HTML where family
    /// is determined at runtime.
    pub fn into_default_ext<P: super::phase::PhaseData>(self) -> super::node::FamilyExt<P>
    where
        P::ElemExt<SvgFamily>: Default,
        P::ElemExt<LinkFamily>: Default,
        P::ElemExt<HeadingFamily>: Default,
        P::ElemExt<MediaFamily>: Default,
        P::ElemExt<OtherFamily>: Default,
    {
        use super::node::FamilyExt;
        match self {
            Self::Svg => FamilyExt::Svg(P::ElemExt::<SvgFamily>::default()),
            Self::Link => FamilyExt::Link(P::ElemExt::<LinkFamily>::default()),
            Self::Heading => FamilyExt::Heading(P::ElemExt::<HeadingFamily>::default()),
            Self::Media => FamilyExt::Media(P::ElemExt::<MediaFamily>::default()),
            Self::Other => FamilyExt::Other(P::ElemExt::<OtherFamily>::default()),
        }
    }
}

// =============================================================================
// TagFamily trait
// =============================================================================

/// Tag family trait - tags in same family share extension data types
///
/// The true value of GAT: each family can have different data structures at different phases
/// - Indexed phase: collect raw information (href, id, viewbox, etc.)
/// - Processed phase: processing results (resolved_url, anchor_id, optimized, etc.)
pub trait TagFamily: 'static + Send + Sync {
    const NAME: &'static str;
    const KIND: FamilyKind;

    /// Family data at Indexed phase (raw information)
    type IndexedData: Debug + Clone + Default + Send + Sync;

    /// Family data at Processed phase (processing results)
    type ProcessedData: Debug + Clone + Default + Send + Sync;

    /// Unified data transformation interface: IndexedData → ProcessedData
    fn process(indexed: &Self::IndexedData) -> Self::ProcessedData;
}

// =============================================================================
// Family definitions
// =============================================================================

/// SVG family: <svg>, <path>, <circle>, <rect>, <g>, ...
#[allow(dead_code)]
pub struct SvgFamily;

#[allow(dead_code)]
impl SvgFamily {
    /// Check if a tag is an SVG element
    pub fn is_svg_tag(tag: &str) -> bool {
        is_svg_tag(tag)
    }

    /// SVG container elements
    pub const CONTAINER_TAGS: &'static [&'static str] = &[
        "svg", "g", "defs", "symbol", "use", "switch"
    ];

    /// SVG shape elements
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
        SvgProcessedData::default()
    }
}

/// SVG Indexed phase data
#[derive(Debug, Clone, Default)]
pub struct SvgIndexedData {
    pub is_root: bool,
    pub viewbox: Option<String>,
    pub dimensions: Option<(f32, f32)>,
}

impl SvgIndexedData {
    /// Parse viewBox string into (min_x, min_y, width, height)
    /// Example: "0 0 100 200" → Some((0.0, 0.0, 100.0, 200.0))
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

    /// Get width/height from viewBox
    pub fn viewbox_dimensions(&self) -> Option<(f32, f32)> {
        self.parse_viewbox().map(|(_, _, w, h)| (w, h))
    }

    /// Get effective dimensions (prefers explicit dimensions over viewBox)
    pub fn effective_dimensions(&self) -> Option<(f32, f32)> {
        self.dimensions.or_else(|| self.viewbox_dimensions())
    }
}

/// SVG Processed phase data
#[derive(Debug, Clone, Default)]
pub struct SvgProcessedData {
    pub optimized: bool,
    pub original_bytes: usize,
    pub optimized_bytes: usize,
    pub extracted_path: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────

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

/// Link Indexed phase data
#[derive(Debug, Clone, Default)]
pub struct LinkIndexedData {
    pub link_type: LinkType,
    pub original_href: Option<String>,
}

/// Link Processed phase data
#[derive(Debug, Clone, Default)]
pub struct LinkProcessedData {
    pub resolved_url: Option<String>,
    pub is_external: bool,
    pub is_broken: bool,
    pub nofollow: bool,
}

/// Link type
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LinkType {
    #[default]
    None,
    Absolute,   // /path
    Relative,   // ./file
    Fragment,   // #anchor
    External,   // https://...
}

impl LinkType {
    /// Infer link type from href string
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

    /// Check if link type is None
    pub fn is_none(&self) -> bool { matches!(self, Self::None) }
    /// Check if link is absolute path
    pub fn is_absolute(&self) -> bool { matches!(self, Self::Absolute) }
    /// Check if link is relative path
    pub fn is_relative(&self) -> bool { matches!(self, Self::Relative) }
    /// Check if link is a fragment
    pub fn is_fragment(&self) -> bool { matches!(self, Self::Fragment) }
    /// Check if link is external
    pub fn is_external(&self) -> bool { matches!(self, Self::External) }
    /// Check if link is internal (not external or none)
    pub fn is_internal(&self) -> bool { !matches!(self, Self::External | Self::None) }
}

// ─────────────────────────────────────────────────────────────────────────────

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

/// Heading Indexed phase data
#[derive(Debug, Clone, Default)]
pub struct HeadingIndexedData {
    pub level: u8,
    pub original_id: Option<String>,
}

impl HeadingIndexedData {
    /// Parse heading level from tag name: "h1" → 1
    pub fn level_from_tag(tag: &str) -> u8 {
        tag.chars()
            .last()
            .and_then(|c| c.to_digit(10))
            .unwrap_or(1) as u8
    }

    /// Check heading level
    pub fn is_h1(&self) -> bool { self.level == 1 }
    pub fn is_h2(&self) -> bool { self.level == 2 }
    pub fn is_h3(&self) -> bool { self.level == 3 }
    pub fn is_h4(&self) -> bool { self.level == 4 }
    pub fn is_h5(&self) -> bool { self.level == 5 }
    pub fn is_h6(&self) -> bool { self.level == 6 }

    /// Check if top-level heading (h1 or h2)
    pub fn is_top_level(&self) -> bool { self.level <= 2 }
}

/// Heading Processed phase data
#[derive(Debug, Clone, Default)]
pub struct HeadingProcessedData {
    pub anchor_id: String,
    pub toc_text: String,
    pub in_toc: bool,
}

// ─────────────────────────────────────────────────────────────────────────────

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

/// Media Indexed phase data
#[derive(Debug, Clone, Default)]
pub struct MediaIndexedData {
    pub src: Option<String>,
    pub is_svg_image: bool,
}

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

    /// Check if media is an image type
    pub fn is_image(&self) -> bool {
        matches!(self.media_type(), MediaType::Image | MediaType::Svg)
    }

    /// Check if media is a video type
    pub fn is_video(&self) -> bool {
        matches!(self.media_type(), MediaType::Video)
    }
}

/// Media type enum for type detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Image,
    Svg,
    Video,
    Audio,
    Unknown,
}

/// Media Processed phase data
#[derive(Debug, Clone, Default)]
pub struct MediaProcessedData {
    pub resolved_src: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub lazy_load: bool,
}

// ─────────────────────────────────────────────────────────────────────────────

/// Other family: all other elements
pub struct OtherFamily;

impl TagFamily for OtherFamily {
    const NAME: &'static str = "other";
    const KIND: FamilyKind = FamilyKind::Other;
    type IndexedData = ();
    type ProcessedData = ();

    fn process(_indexed: &Self::IndexedData) -> Self::ProcessedData {}
}

// =============================================================================
// Family identification functions
// =============================================================================

/// Identify family by tag name and attributes
///
/// NOTE: Reserved for `convert.rs` HTML → Raw conversion.
#[allow(dead_code)]
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

/// Check if a tag is an SVG element
#[allow(dead_code)]
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
        // Filter elements
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_family() {
        assert_eq!(identify_family_kind("svg", &[]), FamilyKind::Svg);
        assert_eq!(identify_family_kind("path", &[]), FamilyKind::Svg);
        assert_eq!(identify_family_kind("a", &[]), FamilyKind::Link);
        assert_eq!(identify_family_kind("h1", &[]), FamilyKind::Heading);
        assert_eq!(identify_family_kind("img", &[]), FamilyKind::Media);
        assert_eq!(identify_family_kind("div", &[]), FamilyKind::Other);

        // href attribute makes any element a link family member
        let attrs = vec![("href".into(), "/path".into())];
        assert_eq!(identify_family_kind("div", &attrs), FamilyKind::Link);
    }

    #[test]
    fn test_link_type() {
        assert_eq!(LinkType::from_href("https://example.com"), LinkType::External);
        assert_eq!(LinkType::from_href("/about"), LinkType::Absolute);
        assert_eq!(LinkType::from_href("#section"), LinkType::Fragment);
        assert_eq!(LinkType::from_href("./file"), LinkType::Relative);
    }

    #[test]
    fn test_heading_level() {
        assert_eq!(HeadingIndexedData::level_from_tag("h1"), 1);
        assert_eq!(HeadingIndexedData::level_from_tag("h6"), 6);
    }
}
