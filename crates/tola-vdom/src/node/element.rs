//! Element type - HTML elements with TTG family extensions
//!
//! The core building block of the VDOM tree.

use smallvec::SmallVec;

use crate::attr::Attrs;
use crate::family::{
    HeadingFamily, LinkFamily, MediaFamily, OtherFamily, SvgFamily,
};
use crate::phase::{PhaseData, Raw};

use super::{FamilyExt, Node};

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
        use crate::family::identify_family_kind;
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
    /// Auto-detect family and capture SourceSpan for StableId generation
    ///
    /// This is the primary constructor for Raw phase elements during
    /// typst-html conversion. The Span is stored in the element extension
    /// and later converted to StableId during indexing.
    ///
    /// # Arguments
    ///
    /// * `tag` - HTML tag name
    /// * `attrs` - Attributes for family detection (not stored)
    /// * `span` - SourceSpan from the source element
    pub fn auto_with_span(
        tag: impl Into<String>,
        attrs: &[(String, String)],
        span: crate::span::SourceSpan,
    ) -> Self {
        use crate::family::identify_family_kind;
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_basics() {
        let elem: Element<Raw> = Element::new("div");
        assert_eq!(elem.tag, "div");
        assert!(elem.is_empty());
        assert_eq!(elem.child_count(), 0);
    }
}
