//! VDOM node types
//!
//! Core tree structure:
//! - [`Node`]: Sum type for all node variants
//! - [`Element`]: HTML element with tag, attrs, children, and family extension
//! - [`Text`]: Text content node
//! - [`Document`]: Root document container
//! - [`FamilyExt`]: Zero-cost family extension enum (replaces Box<dyn Any>)
//! - [`HasFamilyData`]: Unified family data access trait
//!
//! # Module Organization
//!
//! ```text
//! node/
//! ├── mod.rs        - This file (re-exports)
//! ├── element.rs    - Element<P> type
//! ├── text.rs       - Text<P> type
//! ├── document.rs   - Document<P>, Stats, ElementIterator
//! ├── types.rs      - Node<P> sum type
//! └── family_ext.rs - FamilyExt<P>, HasFamilyData trait
//! ```

// Submodules
mod document;
mod element;
mod family_ext;
mod text;
mod types;

// Re-export all public types
pub use document::{Document, Stats};
pub use element::Element;
pub use family_ext::{FamilyExt, HasFamilyData};
pub use text::Text;
pub use types::Node;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::{Raw, RawElemExt};

    #[test]
    fn test_node_type_checks() {
        let elem: Element<Raw> = Element::new("div");
        let node = Node::Element(Box::new(elem));

        assert!(node.is_element());
        assert!(!node.is_text());
        assert!(node.as_element().is_some());
    }

    #[test]
    fn test_family_ext_explicit_construction() {
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
