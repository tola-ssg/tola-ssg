//! Element type - HTML elements with TTG family extensions.
//!
//! The core building block of the VDOM tree.

use compact_str::CompactString;
use smallvec::SmallVec;

use crate::attr::Attrs;
use crate::family::{HeadingFamily, LinkFamily, MediaFamily, OtherFamily, SvgFamily};
use crate::phase::PhaseData;

use super::family_ext::{FamilyExt, HasFamilyData};
use super::Node;

// =============================================================================
// Element<P>
// =============================================================================

/// HTML element with children and family-specific extension data.
#[derive(Debug, Clone)]
pub struct Element<P: PhaseData> {
    /// HTML tag name.
    pub tag: CompactString,
    /// Element attributes.
    pub attrs: Attrs,
    /// Child nodes.
    pub children: SmallVec<[Node<P>; 8]>,
    /// Family-specific extension data (the core of TTG!).
    pub ext: FamilyExt<P>,
}

impl<P: PhaseData> Element<P> {
    /// Create an Other family element with default extension.
    pub fn new(tag: impl Into<CompactString>) -> Self
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

    /// Create an SVG family element.
    pub fn svg(tag: impl Into<CompactString>) -> Self
    where
        P::ElemExt<SvgFamily>: Default,
    {
        Self {
            tag: tag.into(),
            attrs: Vec::new(),
            children: SmallVec::new(),
            ext: FamilyExt::Svg(Default::default()),
        }
    }

    /// Create a Link family element.
    pub fn link(tag: impl Into<CompactString>) -> Self
    where
        P::ElemExt<LinkFamily>: Default,
    {
        Self {
            tag: tag.into(),
            attrs: Vec::new(),
            children: SmallVec::new(),
            ext: FamilyExt::Link(Default::default()),
        }
    }

    /// Create a Heading family element.
    pub fn heading(tag: impl Into<CompactString>) -> Self
    where
        P::ElemExt<HeadingFamily>: Default,
    {
        Self {
            tag: tag.into(),
            attrs: Vec::new(),
            children: SmallVec::new(),
            ext: FamilyExt::Heading(Default::default()),
        }
    }

    /// Create a Media family element.
    pub fn media(tag: impl Into<CompactString>) -> Self
    where
        P::ElemExt<MediaFamily>: Default,
    {
        Self {
            tag: tag.into(),
            attrs: Vec::new(),
            children: SmallVec::new(),
            ext: FamilyExt::Media(Default::default()),
        }
    }

    /// Create element with specific family extension.
    pub fn with_ext(tag: impl Into<CompactString>, ext: FamilyExt<P>) -> Self {
        Self {
            tag: tag.into(),
            attrs: Vec::new(),
            children: SmallVec::new(),
            ext,
        }
    }

    /// Create element with all fields.
    pub fn with_all(
        tag: impl Into<CompactString>,
        attrs: Attrs,
        children: SmallVec<[Node<P>; 8]>,
        ext: FamilyExt<P>,
    ) -> Self {
        Self {
            tag: tag.into(),
            attrs,
            children,
            ext,
        }
    }

    /// Get tag name as str.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Check if element has given tag name.
    pub fn is_tag(&self, tag: &str) -> bool {
        self.tag.as_str() == tag
    }

    /// Add a child node.
    pub fn push_child(&mut self, child: Node<P>) {
        self.children.push(child);
    }

    /// Get number of children.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Check if element has no children.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    // =========================================================================
    // Attribute methods
    // =========================================================================

    /// Get attribute value by name.
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Set attribute value (update if exists, add if not).
    pub fn set_attr(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        if let Some(attr) = self.attrs.iter_mut().find(|(k, _)| k == &name) {
            attr.1 = value;
        } else {
            self.attrs.push((name, value));
        }
    }

    /// Remove attribute by name, returning the old value if it existed.
    pub fn remove_attr(&mut self, name: &str) -> Option<String> {
        if let Some(pos) = self.attrs.iter().position(|(k, _)| k == name) {
            let (_, value) = self.attrs.remove(pos);
            Some(value)
        } else {
            None
        }
    }

    /// Check if element has a specific attribute.
    pub fn has_attr(&self, name: &str) -> bool {
        self.attrs.iter().any(|(k, _)| k == name)
    }
}

impl<P: PhaseData> HasFamilyData<P> for Element<P> {
    fn family_ext(&self) -> &FamilyExt<P> {
        &self.ext
    }

    fn family_ext_mut(&mut self) -> &mut FamilyExt<P> {
        &mut self.ext
    }
}
