//! VDOM node types
//!
//! Core tree structure:
//! - `Node<P>`: Sum type for all node variants
//! - `Element<P>`: HTML element with tag, attrs, children, and family extension
//! - `Text<P>`: Text content node
//! - `Frame<P>`: Embedded document (from typst)
//! - `Document<P>`: Root document container
//! - `FamilyExt<P>`: Zero-cost family extension enum (replaces Box<dyn Any>)

use std::fmt::Debug;

use smallvec::SmallVec;

use super::attr::Attrs;
use super::family::{
    HeadingFamily, LinkFamily, MediaFamily, OtherFamily, SvgFamily, TagFamily,
};
use super::phase::PhaseData;

// =============================================================================
// NodeId
// =============================================================================

/// Unique identifier for nodes within a document
///
/// Used for efficient node lookup and tree operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NodeId(pub u32);

impl NodeId {
    /// Create a new NodeId
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw id value
    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

// =============================================================================
// FamilyExt - Zero-cost family extension enum
// =============================================================================

/// Family extension enum - compile-time determined, zero runtime overhead
///
/// Key design: Use enum instead of Box<dyn Any>
/// - ✅ Stack allocated (no heap overhead)
/// - ✅ Size known at compile time
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
    // Generates: family_name() -> &'static str (returns TagFamily::NAME)
    impl_family_match!(family_name, NAME, &'static str, Svg, Link, Heading, Media, Other);

    /// Get the FamilyKind for this extension
    pub fn kind(&self) -> super::family::FamilyKind {
        use super::family::FamilyKind;
        match self {
            Self::Svg(_) => FamilyKind::Svg,
            Self::Link(_) => FamilyKind::Link,
            Self::Heading(_) => FamilyKind::Heading,
            Self::Media(_) => FamilyKind::Media,
            Self::Other(_) => FamilyKind::Other,
        }
    }

    // Generates for each variant (Svg, Link, Heading, Media, Other):
    //   - is_xxx(&self) -> bool
    //   - as_xxx(&self) -> Option<&ElemExt<XxxFamily>>
    //   - as_xxx_mut(&mut self) -> Option<&mut ElemExt<XxxFamily>>
    impl_family_accessors!(Svg, Link, Heading, Media, Other);
}

// NOTE: FamilyExt intentionally does NOT implement Default.
// Rationale: Silently defaulting to `Other` family hides errors.
// Users must explicitly specify the family when creating elements.
// Use Element::svg(), Element::link(), etc. or Element::auto() instead.

// =============================================================================
// FamilyExt phase-specific implementations
// =============================================================================

use super::phase::{Indexed, Processed, Raw};

/// Raw phase: access and set Span for StableId generation
impl FamilyExt<Raw> {
    /// Get the Span from any family variant
    pub fn span(&self) -> Option<typst::syntax::Span> {
        match self {
            Self::Svg(ext) => ext.span,
            Self::Link(ext) => ext.span,
            Self::Heading(ext) => ext.span,
            Self::Media(ext) => ext.span,
            Self::Other(ext) => ext.span,
        }
    }

    /// Set the Span on any family variant
    pub fn set_span(&mut self, span: typst::syntax::Span) {
        match self {
            Self::Svg(ext) => ext.span = Some(span),
            Self::Link(ext) => ext.span = Some(span),
            Self::Heading(ext) => ext.span = Some(span),
            Self::Media(ext) => ext.span = Some(span),
            Self::Other(ext) => ext.span = Some(span),
        }
    }

    /// Check if this element has a valid (non-detached) Span
    pub fn has_span(&self) -> bool {
        self.span().map(|s| !s.is_detached()).unwrap_or(false)
    }
}

/// Indexed phase: access common fields across all families
impl FamilyExt<Indexed> {
    // Generates: node_id(&self) -> NodeId (reads e.node_id from each variant)
    impl_family_field_get!(node_id, node_id, NodeId, Svg, Link, Heading, Media, Other);

    /// Get the StableId from any family variant
    pub fn stable_id(&self) -> super::id::StableId {
        match self {
            Self::Svg(ext) => ext.stable_id,
            Self::Link(ext) => ext.stable_id,
            Self::Heading(ext) => ext.stable_id,
            Self::Media(ext) => ext.stable_id,
            Self::Other(ext) => ext.stable_id,
        }
    }
}

/// Processed phase: access common fields across all families
impl FamilyExt<Processed> {
    // Generates: is_modified(&self) -> bool (reads e.modified from each variant)
    impl_family_field_get!(is_modified, modified, bool, Svg, Link, Heading, Media, Other);

    // Generates: set_modified(&mut self, value: bool) (sets e.modified on each variant)
    impl_family_field_set!(set_modified, modified, bool, Svg, Link, Heading, Media, Other);

    /// Get the StableId from any family variant (preserved from Indexed phase)
    pub fn stable_id(&self) -> super::id::StableId {
        match self {
            Self::Svg(ext) => ext.stable_id,
            Self::Link(ext) => ext.stable_id,
            Self::Heading(ext) => ext.stable_id,
            Self::Media(ext) => ext.stable_id,
            Self::Other(ext) => ext.stable_id,
        }
    }
}

// =============================================================================
// HasFamilyData trait - unified family data access
// =============================================================================

/// Unified family data access trait
///
/// Allows accessing family-specific data without manual match branches.
///
/// # Example
/// ```ignore
/// use tola::vdom::{Element, Indexed, LinkFamily, HasFamilyData};
///
/// fn process_link(elem: &Element<Indexed>) {
///     if let Some(link_data) = elem.family_data::<LinkFamily>() {
///         println!("href: {:?}", link_data.original_href);
///     }
/// }
/// ```
pub trait HasFamilyData<F: TagFamily> {
    /// The concrete data type for this (Phase, Family) combination
    type Data;

    /// Get immutable reference to family data if this element belongs to family F
    fn family_data(&self) -> Option<&Self::Data>;

    /// Get mutable reference to family data if this element belongs to family F
    fn family_data_mut(&mut self) -> Option<&mut Self::Data>;
}

// Macro to generate HasFamilyData implementations
// Uses paste to auto-generate method names (Svg -> as_svg, as_svg_mut)
// and data types (Indexed + Svg -> SvgIndexedData, Processed + Svg -> SvgProcessedData)
macro_rules! impl_has_family_data {
    // With explicit data type (for OtherFamily which uses () instead of OtherXxxData)
    ($phase:ident, $family:ident, $data_type:ty) => {
        ::paste::paste! {
            impl HasFamilyData<[<$family Family>]> for Element<$phase> {
                type Data = $data_type;

                fn family_data(&self) -> Option<&Self::Data> {
                    self.ext.[<as_ $family:lower>]().map(|e| &e.family_data)
                }

                fn family_data_mut(&mut self) -> Option<&mut Self::Data> {
                    self.ext.[<as_ $family:lower _mut>]().map(|e| &mut e.family_data)
                }
            }
        }
    };
    // Auto-generate data type (XxxFamily + Phase -> XxxPhaseData)
    ($phase:ident, $family:ident) => {
        ::paste::paste! {
            impl_has_family_data!($phase, $family, [<$family $phase Data>]);
        }
    };
}

// Import family data types for impl_has_family_data
use super::family::{
    HeadingIndexedData, HeadingProcessedData, LinkIndexedData, LinkProcessedData,
    MediaIndexedData, MediaProcessedData, SvgIndexedData, SvgProcessedData,
};

// Generates for each (Phase, Family):
//   impl HasFamilyData<XxxFamily> for Element<Phase> {
//     type Data = XxxPhaseData;  // or () for Other
//     fn family_data(&self) -> Option<&Self::Data>
//     fn family_data_mut(&mut self) -> Option<&mut Self::Data>
//   }

// Indexed phase implementations
impl_has_family_data!(Indexed, Svg);         // -> SvgIndexedData
impl_has_family_data!(Indexed, Link);        // -> LinkIndexedData
impl_has_family_data!(Indexed, Heading);     // -> HeadingIndexedData
impl_has_family_data!(Indexed, Media);       // -> MediaIndexedData
impl_has_family_data!(Indexed, Other, ());   // -> Other uses ()

// Processed phase implementations
impl_has_family_data!(Processed, Svg);       // -> SvgProcessedData
impl_has_family_data!(Processed, Link);      // -> LinkProcessedData
impl_has_family_data!(Processed, Heading);   // -> HeadingProcessedData
impl_has_family_data!(Processed, Media);     // -> MediaProcessedData
impl_has_family_data!(Processed, Other, ()); // -> Other uses ()

// =============================================================================
// Node<P> - Sum type
// =============================================================================

/// VDOM node sum type parameterized by phase
///
/// # Frame Variant Semantics by Phase
///
/// The `Frame` variant has different meanings at each phase:
///
/// - **Raw**: Frames exist and represent typst embedded content
/// - **Indexed**: Frames are indexed but not yet expanded
/// - **Processed**: Frames have been converted to Elements (SVG)
///   - `FrameExt = Infallible` means Frame<Processed> cannot be constructed
///   - The Frame variant technically exists but is unreachable
/// - **Rendered**: Same as Processed
///
/// This is a deliberate design choice: using `Infallible` as `FrameExt`
/// prevents construction of `Frame<Processed>` at compile time, while
/// `Folder::fold_frame` returns `Node<To>` to allow Frame → Element conversion.
#[derive(Debug, Clone)]
pub enum Node<P: PhaseData> {
    Element(Box<Element<P>>),
    Text(Text<P>),
    /// Frame variant - only constructable in phases where FrameExt: Default
    ///
    /// At Processed/Rendered phases, this variant exists in the type but
    /// cannot be instantiated because `FrameExt = Infallible`.
    Frame(Box<Frame<P>>),
}

impl<P: PhaseData> Node<P> {
    // Generates for each variant (element -> Element, etc.):
    //   - is_xxx(&self) -> bool
    //   - as_xxx(&self) -> Option<&Type<P>>
    //   - as_xxx_mut(&mut self) -> Option<&mut Type<P>>
    impl_enum_accessors!(P; element, text, frame);
}

// =============================================================================
// Element<P>
// =============================================================================

/// HTML element with children and family-specific extension data
#[derive(Debug, Clone)]
pub struct Element<P: PhaseData> {
    /// HTML tag name
    pub tag: String,
    /// Element attributes
    pub attrs: Attrs,
    /// Child nodes
    pub children: SmallVec<[Node<P>; 8]>,
    /// Family-specific extension data (the core of TTG!)
    pub ext: FamilyExt<P>,
}

impl<P: PhaseData> Element<P> {
    /// Create an Other family element with default extension
    ///
    /// This is a convenience constructor for creating generic elements.
    /// For specific families, use `Element::svg()`, `Element::link()`, etc.
    /// For auto-detection, use `Element::auto()`.
    pub fn new(tag: impl Into<String>) -> Self
    where
        P::ElemExt<OtherFamily>: Default,
    {
        Self {
            tag: tag.into(),
            attrs: Vec::new(),
            children: SmallVec::new(),
            ext: FamilyExt::Other(Default::default()),
        }
    }

    /// Create a new element with specific family extension
    pub fn with_ext(tag: impl Into<String>, ext: FamilyExt<P>) -> Self
    where
        P::ElemExt<OtherFamily>: Default,
    {
        Self {
            tag: tag.into(),
            attrs: Vec::new(),
            children: SmallVec::new(),
            ext,
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Family-specific constructors
    // ─────────────────────────────────────────────────────────────────────────

    /// Create an SVG family element
    pub fn svg(tag: impl Into<String>, ext: P::ElemExt<SvgFamily>) -> Self {
        Self::with_ext(tag, FamilyExt::Svg(ext))
    }

    /// Create a Link family element
    pub fn link(tag: impl Into<String>, ext: P::ElemExt<LinkFamily>) -> Self {
        Self::with_ext(tag, FamilyExt::Link(ext))
    }

    /// Create a Heading family element
    pub fn heading(tag: impl Into<String>, ext: P::ElemExt<HeadingFamily>) -> Self {
        Self::with_ext(tag, FamilyExt::Heading(ext))
    }

    /// Create a Media family element
    pub fn media(tag: impl Into<String>, ext: P::ElemExt<MediaFamily>) -> Self {
        Self::with_ext(tag, FamilyExt::Media(ext))
    }

    /// Create an Other family element (same as `new()`)
    pub fn other(tag: impl Into<String>, ext: P::ElemExt<OtherFamily>) -> Self {
        Self::with_ext(tag, FamilyExt::Other(ext))
    }

    /// Auto-detect family from tag name and attributes
    ///
    /// Uses `identify_family_kind()` to determine the appropriate family,
    /// then creates the element with default extension data.
    /// Note: attrs are used for detection only, not stored (use builder pattern if needed)
    pub fn auto(tag: impl Into<String>, attrs: &[(String, String)]) -> Self
    where
        P::ElemExt<SvgFamily>: Default,
        P::ElemExt<LinkFamily>: Default,
        P::ElemExt<HeadingFamily>: Default,
        P::ElemExt<MediaFamily>: Default,
        P::ElemExt<OtherFamily>: Default,
    {
        use super::family::identify_family_kind;
        let tag = tag.into();
        let kind = identify_family_kind(&tag, attrs);
        Self {
            tag,
            attrs: Vec::new(),
            children: SmallVec::new(),
            ext: kind.into_default_ext(),
        }
    }
}

// =============================================================================
// Element<Raw> specific methods
// =============================================================================

impl Element<Raw> {
    /// Auto-detect family and capture Typst Span for StableId generation
    ///
    /// This is the primary constructor for Raw phase elements during
    /// typst-html conversion. The Span is stored in the element extension
    /// and later converted to StableId during indexing.
    ///
    /// # Arguments
    ///
    /// * `tag` - HTML tag name
    /// * `attrs` - Attributes for family detection (not stored)
    /// * `span` - Typst Span from the source element
    pub fn auto_with_span(
        tag: impl Into<String>,
        attrs: &[(String, String)],
        span: typst::syntax::Span,
    ) -> Self {
        use super::family::identify_family_kind;
        let tag = tag.into();
        let kind = identify_family_kind(&tag, attrs);
        let mut ext = kind.into_default_ext::<Raw>();
        ext.set_span(span);
        Self {
            tag,
            attrs: Vec::new(),
            children: SmallVec::new(),
            ext,
        }
    }
}

impl<P: PhaseData> Element<P> {
    // ─────────────────────────────────────────────────────────────────────────
    // Attribute access
    // ─────────────────────────────────────────────────────────────────────────

    /// Get attribute value by name
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Set attribute value (update if exists, add if not)
    pub fn set_attr(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        if let Some(attr) = self.attrs.iter_mut().find(|(k, _)| k == &name) {
            attr.1 = value;
        } else {
            self.attrs.push((name, value));
        }
    }

    /// Remove attribute by name, returning the old value if it existed
    pub fn remove_attr(&mut self, name: &str) -> Option<String> {
        if let Some(pos) = self.attrs.iter().position(|(k, _)| k == name) {
            let (_, value) = self.attrs.remove(pos);
            Some(value)
        } else {
            None
        }
    }

    /// Check if attribute exists
    pub fn has_attr(&self, name: &str) -> bool {
        self.attrs.iter().any(|(k, _)| k == name)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Other helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Get family name string
    pub fn family(&self) -> &'static str {
        self.ext.family_name()
    }

    /// Check if element has no children
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Check if element has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Check if element is a leaf (no child elements, may have text)
    pub fn is_leaf(&self) -> bool {
        self.children.iter().all(|n| !n.is_element())
    }

    /// Number of direct children (all node types)
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Number of direct child elements (excludes Text and Frame)
    pub fn element_count(&self) -> usize {
        self.children.iter().filter(|n| n.is_element()).count()
    }

    /// Iterate over child element references
    pub fn children_elements(&self) -> impl Iterator<Item = &Element<P>> {
        self.children.iter().filter_map(|n| n.as_element())
    }

    /// Iterate over child element mutable references
    pub fn children_elements_mut(&mut self) -> impl Iterator<Item = &mut Element<P>> {
        self.children.iter_mut().filter_map(|n| n.as_element_mut())
    }

    /// Get text content of this element (concatenated from all text nodes)
    pub fn text_content(&self) -> String {
        let mut result = String::new();
        self.collect_text(&mut result);
        result
    }

    fn collect_text(&self, buf: &mut String) {
        for child in &self.children {
            match child {
                Node::Text(t) => buf.push_str(&t.content),
                Node::Element(e) => e.collect_text(buf),
                Node::Frame(_) => {}
            }
        }
    }
}

// =============================================================================
// Text<P>
// =============================================================================

/// Text content node
#[derive(Debug, Clone)]
pub struct Text<P: PhaseData> {
    /// Text content
    pub content: String,
    /// Phase-specific extension data
    pub ext: P::TextExt,
}

impl<P: PhaseData> Text<P> {
    /// Create a new text node
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ext: P::TextExt::default(),
        }
    }

    /// Check if text content is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get text length in bytes
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if text is only whitespace
    pub fn is_whitespace(&self) -> bool {
        self.content.trim().is_empty()
    }

    /// Get trimmed content
    pub fn trimmed(&self) -> &str {
        self.content.trim()
    }
}

// =============================================================================
// Frame<P>
// =============================================================================

/// Embedded typst frame (rendered as inline SVG)
///
/// Represents content from typst's `html.frame()` that needs special handling.
/// Unlike Element, Frame is an atomic unit - it has no children in the VDOM sense.
/// The frame content is stored in `FrameExt` and rendered to SVG during processing.
///
/// # Phase behavior
///
/// - **Raw/Indexed**: `FrameExt` contains typst frame data (layout, points, etc.)
/// - **Processed/Rendered**: Cannot be constructed (`FrameExt = Infallible`)
///   - Frame is transformed to an SVG Element during Indexed → Processed
#[derive(Debug, Clone)]
pub struct Frame<P: PhaseData> {
    /// Phase-specific extension data (contains all frame content)
    pub ext: P::FrameExt,
}

impl<P: PhaseData> Frame<P> {
    /// Create a new frame with extension data
    pub fn new(ext: P::FrameExt) -> Self {
        Self { ext }
    }
}

// Note: Frame does NOT implement Default because:
// 1. An empty frame is meaningless
// 2. At Processed phase, FrameExt = Infallible (cannot default)
// Use Frame::new(ext) to construct frames explicitly.

// =============================================================================
// Document<P>
// =============================================================================

/// Root document container
#[derive(Debug, Clone)]
pub struct Document<P: PhaseData> {
    /// Root element (typically <html> or a wrapper)
    pub root: Element<P>,
    /// Document-level extension data (metadata, stats)
    pub ext: P::DocExt,
}

impl<P: PhaseData> Document<P> {
    /// Create a new document with a root element
    pub fn new(root: Element<P>) -> Self {
        Self {
            root,
            ext: P::DocExt::default(),
        }
    }

    /// Get the phase name for debugging
    pub fn phase_name(&self) -> &'static str {
        P::NAME
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Query API (v8)
    // ─────────────────────────────────────────────────────────────────────────

    /// Find first element matching predicate (depth-first search)
    pub fn find_element<F>(&self, predicate: F) -> Option<&Element<P>>
    where
        F: Fn(&Element<P>) -> bool,
    {
        Self::find_in_element(&self.root, &predicate)
    }

    fn find_in_element<'a, F>(elem: &'a Element<P>, predicate: &F) -> Option<&'a Element<P>>
    where
        F: Fn(&Element<P>) -> bool,
    {
        if predicate(elem) {
            return Some(elem);
        }
        for child in &elem.children {
            if let Some(child_elem) = child.as_element() {
                if let Some(found) = Self::find_in_element(child_elem, predicate) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Find all elements matching predicate
    pub fn find_all<F>(&self, predicate: F) -> Vec<&Element<P>>
    where
        F: Fn(&Element<P>) -> bool,
    {
        let mut results = Vec::new();
        Self::collect_elements(&self.root, &predicate, &mut results);
        results
    }

    fn collect_elements<'a, F>(elem: &'a Element<P>, predicate: &F, results: &mut Vec<&'a Element<P>>)
    where
        F: Fn(&Element<P>) -> bool,
    {
        if predicate(elem) {
            results.push(elem);
        }
        for child in &elem.children {
            if let Some(child_elem) = child.as_element() {
                Self::collect_elements(child_elem, predicate, results);
            }
        }
    }

    /// Check if any element matches predicate
    pub fn has_element<F>(&self, predicate: F) -> bool
    where
        F: Fn(&Element<P>) -> bool,
    {
        self.find_element(predicate).is_some()
    }

    /// Check if document contains any Frame nodes
    pub fn has_frames(&self) -> bool {
        Self::check_frames(&self.root)
    }

    fn check_frames(elem: &Element<P>) -> bool {
        for child in &elem.children {
            match child {
                Node::Frame(_) => return true,
                Node::Element(e) => {
                    if Self::check_frames(e) {
                        return true;
                    }
                }
                Node::Text(_) => {}
            }
        }
        false
    }

    /// Count total elements in document
    pub fn element_count(&self) -> usize {
        Self::count_elements(&self.root)
    }

    fn count_elements(elem: &Element<P>) -> usize {
        let mut count = 1; // this element
        for child in &elem.children {
            if let Some(child_elem) = child.as_element() {
                count += Self::count_elements(child_elem);
            }
        }
        count
    }

    /// Iterate over all elements (depth-first)
    pub fn iter_elements(&self) -> impl Iterator<Item = &Element<P>> {
        ElementIterator::new(&self.root)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Mutable query API
    // ─────────────────────────────────────────────────────────────────────────

    /// Find first element matching predicate (mutable)
    pub fn find_element_mut<F>(&mut self, predicate: F) -> Option<&mut Element<P>>
    where
        F: Fn(&Element<P>) -> bool + Copy,
    {
        Self::find_in_element_mut(&mut self.root, predicate)
    }

    fn find_in_element_mut<F>(elem: &mut Element<P>, predicate: F) -> Option<&mut Element<P>>
    where
        F: Fn(&Element<P>) -> bool + Copy,
    {
        if predicate(elem) {
            return Some(elem);
        }
        for child in &mut elem.children {
            if let Some(child_elem) = child.as_element_mut() {
                if let Some(found) = Self::find_in_element_mut(child_elem, predicate) {
                    return Some(found);
                }
            }
        }
        None
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Closure-based traversal API
    // ─────────────────────────────────────────────────────────────────────────

    /// Visit all elements with a closure (read-only)
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
            if let Some(child_elem) = child.as_element() {
                Self::visit_elements_recursive(child_elem, f);
            }
        }
    }

    /// Visit all elements with a closure (mutable)
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
            if let Some(child_elem) = child.as_element_mut() {
                Self::visit_elements_mut_recursive(child_elem, f);
            }
        }
    }
}

// =============================================================================
// ElementIterator - depth-first element traversal
// =============================================================================

/// Depth-first iterator over elements
pub struct ElementIterator<'a, P: PhaseData> {
    stack: Vec<&'a Element<P>>,
}

impl<'a, P: PhaseData> ElementIterator<'a, P> {
    fn new(root: &'a Element<P>) -> Self {
        Self { stack: vec![root] }
    }
}

impl<'a, P: PhaseData> Iterator for ElementIterator<'a, P> {
    type Item = &'a Element<P>;

    fn next(&mut self) -> Option<Self::Item> {
        let elem = self.stack.pop()?;
        // Push children in reverse order so they're visited left-to-right
        for child in elem.children.iter().rev() {
            if let Some(child_elem) = child.as_element() {
                self.stack.push(child_elem);
            }
        }
        Some(elem)
    }
}

// =============================================================================
// Stats - document statistics
// =============================================================================

/// Document statistics collected from traversal
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    pub svg_count: usize,
    pub link_count: usize,
    pub heading_count: usize,
    pub media_count: usize,
    pub frame_count: usize,
    pub text_count: usize,
    pub element_count: usize,
}

impl Stats {
    /// Check if document has any SVG elements
    pub fn has_svg(&self) -> bool {
        self.svg_count > 0
    }

    /// Check if document has any links
    pub fn has_links(&self) -> bool {
        self.link_count > 0
    }

    /// Check if document has any headings
    pub fn has_headings(&self) -> bool {
        self.heading_count > 0
    }

    /// Check if document has any frames
    pub fn has_frames(&self) -> bool {
        self.frame_count > 0
    }

    /// Total family-specific elements (svg + link + heading + media)
    pub fn family_element_count(&self) -> usize {
        self.svg_count + self.link_count + self.heading_count + self.media_count
    }
}

impl<P: PhaseData> Document<P> {
    /// Collect statistics about the document
    pub fn collect_stats(&self) -> Stats {
        let mut stats = Stats::default();
        Self::collect_stats_recursive(&self.root, &mut stats);
        stats
    }

    fn collect_stats_recursive(elem: &Element<P>, stats: &mut Stats) {
        stats.element_count += 1;

        // Count by family
        match elem.ext.kind() {
            super::family::FamilyKind::Svg => stats.svg_count += 1,
            super::family::FamilyKind::Link => stats.link_count += 1,
            super::family::FamilyKind::Heading => stats.heading_count += 1,
            super::family::FamilyKind::Media => stats.media_count += 1,
            super::family::FamilyKind::Other => {}
        }

        // Recurse into children
        for child in &elem.children {
            match child {
                Node::Element(e) => Self::collect_stats_recursive(e, stats),
                Node::Text(_) => stats.text_count += 1,
                Node::Frame(_) => stats.frame_count += 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdom::phase::Raw;

    #[test]
    fn test_node_id_display() {
        let id = NodeId::new(42);
        assert_eq!(format!("{}", id), "#42");
    }

    #[test]
    fn test_element_basics() {
        let elem: Element<Raw> = Element::new("div");
        assert_eq!(elem.tag, "div");
        assert!(elem.is_empty());
        assert_eq!(elem.child_count(), 0);
    }

    #[test]
    fn test_text_node() {
        let text: Text<Raw> = Text::new("  hello world  ");
        assert!(!text.is_empty());
        assert!(!text.is_whitespace());
        assert_eq!(text.trimmed(), "hello world");
    }

    #[test]
    fn test_node_type_checks() {
        let elem: Element<Raw> = Element::new("div");
        let node = Node::Element(Box::new(elem));

        assert!(node.is_element());
        assert!(!node.is_text());
        assert!(!node.is_frame());
        assert!(node.as_element().is_some());
    }

    #[test]
    fn test_family_ext_explicit_construction() {
        use crate::vdom::phase::RawElemExt;

        // FamilyExt does NOT implement Default - must construct explicitly
        let ext: FamilyExt<Raw> = FamilyExt::Other(RawElemExt::detached());
        assert!(ext.is_other());
        assert_eq!(ext.family_name(), "other");

        // SVG family
        let svg_ext: FamilyExt<Raw> = FamilyExt::Svg(RawElemExt::detached());
        assert!(svg_ext.is_svg());
        assert_eq!(svg_ext.family_name(), "svg");
    }
}
