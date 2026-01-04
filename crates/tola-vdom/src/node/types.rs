//! Node sum type - the core VDOM node abstraction
//!
//! `Node<P>` is a sum type that represents any node in the VDOM tree.

use crate::phase::PhaseData;

use super::{Element, Text};

// =============================================================================
// Node<P> - Sum type
// =============================================================================

/// VDOM node sum type parameterized by phase
///
/// # Design Note
///
/// Frames (typst embedded content) are eagerly converted to SVG Elements
/// during the Raw phase conversion (in `convert.rs`). This simplifies the
/// pipeline - all nodes are either Elements or Text at every phase.
#[derive(Debug, Clone)]
pub enum Node<P: PhaseData> {
    Element(Box<Element<P>>),
    Text(Text<P>),
}

impl<P: PhaseData> Node<P> {
    // Generates for each variant (element -> Element, etc.):
    //   - is_xxx(&self) -> bool
    //   - as_xxx(&self) -> Option<&Type<P>>
    //   - as_xxx_mut(&mut self) -> Option<&mut Type<P>>
    impl_enum_accessors!(P; element, text);
}
