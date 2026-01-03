//! Node enum combining all node types.

use crate::phase::PhaseData;
use super::{Element, Text};

/// Union type for all DOM nodes.
///
/// # Design Note
///
/// Element is boxed to break the recursive type cycle:
/// `Node` contains `Element`, which contains `SmallVec<[Node; 8]>`.
/// Without `Box`, the compiler cannot determine the size of `Node`.
#[derive(Debug, Clone)]
pub enum Node<P: PhaseData> {
    /// Element node (boxed to break recursion).
    Element(Box<Element<P>>),
    /// Text node.
    Text(Text<P>),
}

impl<P: PhaseData> Node<P> {
    /// Check if this is an element node.
    pub fn is_element(&self) -> bool {
        matches!(self, Node::Element(_))
    }

    /// Check if this is a text node.
    pub fn is_text(&self) -> bool {
        matches!(self, Node::Text(_))
    }

    /// Get as element reference if this is an element.
    pub fn as_element(&self) -> Option<&Element<P>> {
        match self {
            Node::Element(e) => Some(e),
            _ => None,
        }
    }

    /// Get as mutable element reference if this is an element.
    pub fn as_element_mut(&mut self) -> Option<&mut Element<P>> {
        match self {
            Node::Element(e) => Some(e),
            _ => None,
        }
    }

    /// Get as text reference if this is a text node.
    pub fn as_text(&self) -> Option<&Text<P>> {
        match self {
            Node::Text(t) => Some(t),
            _ => None,
        }
    }

    /// Get as mutable text reference if this is a text node.
    pub fn as_text_mut(&mut self) -> Option<&mut Text<P>> {
        match self {
            Node::Text(t) => Some(t),
            _ => None,
        }
    }
}

impl<P: PhaseData> From<Element<P>> for Node<P> {
    fn from(elem: Element<P>) -> Self {
        Node::Element(Box::new(elem))
    }
}

impl<P: PhaseData> From<Box<Element<P>>> for Node<P> {
    fn from(elem: Box<Element<P>>) -> Self {
        Node::Element(elem)
    }
}

impl<P: PhaseData> From<Text<P>> for Node<P> {
    fn from(text: Text<P>) -> Self {
        Node::Text(text)
    }
}
