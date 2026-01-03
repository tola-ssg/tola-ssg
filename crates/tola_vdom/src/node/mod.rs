//! DOM node types for the VDOM.
//!
//! This module provides the core node types:
//! - [`Document`]: Root document container
//! - [`Element`]: HTML/XML element with tag, attributes, children
//! - [`Text`]: Text content node
//! - [`Node`]: Enum combining all node types
//! - [`FamilyExt`]: TTG family extension wrapper

mod document;
mod element;
mod family_ext;
mod text;
mod types;

pub use document::Document;
pub use element::Element;
pub use family_ext::{FamilyExt, HasFamilyData};
pub use text::Text;
pub use types::Node;

/// Statistics collected during document traversal.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    /// Total element count.
    pub elements: usize,
    /// Total text node count.
    pub texts: usize,
    /// Maximum depth reached.
    pub max_depth: usize,
}
