//! Folder trait for VDOM phase transformation
//!
//! Folders transform trees from one phase to another,
//! enabling type-safe phase transitions with full control
//! over extension data transformation.
//!
//! # Design Notes
//!
//! `Folder` is an internal trait defining tree traversal protocol.
//! Users should interact with the `Transform` trait from `transform.rs`.
//!
//! Concrete implementations (like `ProcessFolder`) implement both:
//! - `Folder<From, To>` for the traversal mechanics
//! - `Transform<From>` for the public API

use super::node::{Document, Element, Frame, Node, NodeId, Text};
use super::phase::PhaseData;
use super::transform::Transform;

// =============================================================================
// Folder trait
// =============================================================================

/// Low-level transformer for phase transitions
///
/// Unlike Visitor (read-only), Folder produces a new tree
/// potentially in a different phase.
///
/// Key design: `fold_frame` returns `Node<To>`, not `Frame<To>`.
/// This allows Frame → Element conversion (e.g., typst frame → SVG element).
pub trait Folder<From: PhaseData, To: PhaseData> {
    /// Transform document extension data
    fn fold_doc_ext(&mut self, ext: From::DocExt) -> To::DocExt;

    /// Transform an element
    fn fold_element(&mut self, elem: Element<From>) -> Element<To>;

    /// Transform a text node
    fn fold_text(&mut self, text: Text<From>) -> Text<To>;

    /// Transform a frame - returns Node to allow Frame → Element conversion
    fn fold_frame(&mut self, frame: Frame<From>) -> Node<To>;

    /// Transform a node (dispatches to specific fold methods)
    fn fold_node(&mut self, node: Node<From>) -> Node<To> {
        match node {
            Node::Element(elem) => Node::Element(Box::new(self.fold_element(*elem))),
            Node::Text(text) => Node::Text(self.fold_text(text)),
            Node::Frame(frame) => self.fold_frame(*frame),
        }
    }

    /// Transform children nodes
    fn fold_children(&mut self, children: impl IntoIterator<Item = Node<From>>) -> Vec<Node<To>> {
        children.into_iter().map(|n| self.fold_node(n)).collect()
    }
}

/// Apply a folder to transform a document
pub fn fold<From, To, F>(doc: Document<From>, folder: &mut F) -> Document<To>
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
// NodeIdGenerator
// =============================================================================

/// Generator for unique node IDs
///
/// Used during tree construction and transformation.
pub struct NodeIdGenerator {
    next: u32,
}

impl NodeIdGenerator {
    pub fn new() -> Self {
        Self { next: 0 }
    }

    pub fn next(&mut self) -> NodeId {
        let id = NodeId::new(self.next);
        self.next += 1;
        id
    }

    pub fn current(&self) -> u32 {
        self.next
    }
}

impl Default for NodeIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Type-safe Indexed → Processed transformation
// =============================================================================

use super::family::{
    HeadingFamily, LinkFamily, MediaFamily, OtherFamily, SvgFamily, TagFamily,
};
use super::node::FamilyExt;
use super::phase::{Indexed, IndexedElemExt, Processed, ProcessedDocExt, ProcessedElemExt};
use smallvec::SmallVec;

/// Transform FamilyExt from Indexed to Processed phase
///
/// This function demonstrates proper GAT utilization:
/// - Takes `FamilyExt<Indexed>` (each variant has `IndexedElemExt<F>`)
/// - Returns `FamilyExt<Processed>` (each variant has `ProcessedElemExt<F>`)
/// - Calls `TagFamily::process()` for type-safe data transformation
///
/// # Type Safety via GAT
/// The transformation is type-safe because:
/// 1. `IndexedElemExt<F>` contains `F::IndexedData`
/// 2. `ProcessedElemExt<F>` contains `F::ProcessedData`
/// 3. `TagFamily::process()` transforms `&F::IndexedData -> F::ProcessedData`
///
/// The compiler ensures each family's data is transformed correctly.
pub fn process_family_ext(ext: FamilyExt<Indexed>) -> FamilyExt<Processed> {
    match ext {
        FamilyExt::Svg(e) => FamilyExt::Svg(process_elem_ext::<SvgFamily>(e)),
        FamilyExt::Link(e) => FamilyExt::Link(process_elem_ext::<LinkFamily>(e)),
        FamilyExt::Heading(e) => FamilyExt::Heading(process_elem_ext::<HeadingFamily>(e)),
        FamilyExt::Media(e) => FamilyExt::Media(process_elem_ext::<MediaFamily>(e)),
        FamilyExt::Other(e) => FamilyExt::Other(process_elem_ext::<OtherFamily>(e)),
    }
}

/// Transform a single IndexedElemExt<F> to ProcessedElemExt<F>
///
/// Uses `TagFamily::process()` to transform family-specific data.
/// Preserves `stable_id` for hot reload targeting.
fn process_elem_ext<F: TagFamily>(indexed: IndexedElemExt<F>) -> ProcessedElemExt<F> {
    ProcessedElemExt {
        stable_id: indexed.stable_id,
        modified: false,
        family_data: F::process(&indexed.family_data),
    }
}

// =============================================================================
// ProcessFolder - Concrete Indexed → Processed Folder implementation
// =============================================================================

/// Default Folder implementation for Indexed → Processed transformation
///
/// This is a concrete implementation that uses `process_family_ext` and
/// `TagFamily::process()` for type-safe phase transformation.
///
/// # Usage
/// ```ignore
/// let indexed_doc: Document<Indexed> = ...;
/// let mut folder = ProcessFolder::new();
/// let processed_doc = fold(indexed_doc, &mut folder);
/// ```
pub struct ProcessFolder {
    stats: ProcessedDocExt,
}

impl ProcessFolder {
    pub fn new() -> Self {
        Self {
            stats: ProcessedDocExt::default(),
        }
    }

    /// Get processing statistics
    pub fn stats(&self) -> &ProcessedDocExt {
        &self.stats
    }
}

impl Default for ProcessFolder {
    fn default() -> Self {
        Self::new()
    }
}

impl Folder<Indexed, Processed> for ProcessFolder {
    fn fold_doc_ext(&mut self, _ext: super::phase::IndexedDocExt) -> ProcessedDocExt {
        // Return accumulated stats
        std::mem::take(&mut self.stats)
    }

    fn fold_element(&mut self, elem: Element<Indexed>) -> Element<Processed> {
        // Track statistics based on family
        match &elem.ext {
            FamilyExt::Svg(_) => self.stats.svg_count += 1,
            FamilyExt::Link(_) => self.stats.links_resolved += 1,
            FamilyExt::Heading(_) => self.stats.headings_anchored += 1,
            _ => {}
        }

        // Transform element using type-safe process_family_ext
        Element {
            tag: elem.tag,
            attrs: elem.attrs,
            children: SmallVec::from_vec(self.fold_children(elem.children)),
            ext: process_family_ext(elem.ext),
        }
    }

    fn fold_text(&mut self, text: Text<Indexed>) -> Text<Processed> {
        Text {
            content: text.content,
            ext: (), // TextExt is () for both phases
        }
    }

    fn fold_frame(&mut self, _frame: Frame<Indexed>) -> Node<Processed> {
        // Frame → Element conversion
        // In real implementation, this would convert typst frame to SVG element
        self.stats.frames_expanded += 1;

        // Create a placeholder SVG element for the frame
        // Real implementation would generate actual SVG from frame content
        Node::Element(Box::new(Element {
            tag: "svg".to_string(),
            attrs: vec![("class".to_string(), "typst-frame".to_string())],
            children: SmallVec::new(),
            ext: FamilyExt::Svg(ProcessedElemExt::default()),
        }))
    }
}

// ProcessFolder also implements Transform for public API
impl Transform<Indexed> for ProcessFolder {
    type To = Processed;

    fn transform(mut self, doc: Document<Indexed>) -> Document<Processed> {
        fold(doc, &mut self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::phase::IndexedDocExt;

    #[test]
    fn test_node_id_generator() {
        let mut id_gen = NodeIdGenerator::new();
        assert_eq!(id_gen.next().as_u32(), 0);
        assert_eq!(id_gen.next().as_u32(), 1);
        assert_eq!(id_gen.next().as_u32(), 2);
        assert_eq!(id_gen.current(), 3);
    }

    #[test]
    fn test_process_family_ext() {
        // Create an indexed SVG extension
        let indexed_ext: FamilyExt<Indexed> = FamilyExt::Svg(IndexedElemExt::default());

        // Transform to processed phase
        let processed_ext = process_family_ext(indexed_ext);

        // Verify family is preserved
        assert!(processed_ext.is_svg());
        assert!(!processed_ext.is_modified());
    }

    #[test]
    fn test_process_folder() {
        // Create a simple indexed document
        let root = Element::<Indexed> {
            tag: "div".to_string(),
            attrs: vec![],
            children: SmallVec::new(),
            ext: FamilyExt::Other(IndexedElemExt::default()),
        };
        let doc = Document {
            root,
            ext: IndexedDocExt::default(),
        };

        // Apply ProcessFolder
        let mut folder = ProcessFolder::new();
        let processed_doc = fold(doc, &mut folder);

        // Verify transformation
        assert_eq!(processed_doc.root.tag, "div");
    }

    #[test]
    fn test_process_folder_tracks_stats() {
        // Create document with SVG and Link elements
        let svg_elem = Element::<Indexed> {
            tag: "svg".to_string(),
            attrs: vec![],
            children: SmallVec::new(),
            ext: FamilyExt::Svg(IndexedElemExt::default()),
        };
        let link_elem = Element::<Indexed> {
            tag: "a".to_string(),
            attrs: vec![],
            children: SmallVec::new(),
            ext: FamilyExt::Link(IndexedElemExt::default()),
        };
        let root = Element::<Indexed> {
            tag: "div".to_string(),
            attrs: vec![],
            children: SmallVec::from_vec(vec![
                Node::Element(Box::new(svg_elem)),
                Node::Element(Box::new(link_elem)),
            ]),
            ext: FamilyExt::Other(IndexedElemExt::default()),
        };
        let doc = Document {
            root,
            ext: IndexedDocExt::default(),
        };

        // Apply ProcessFolder
        let mut folder = ProcessFolder::new();
        let processed_doc = fold(doc, &mut folder);

        // Verify stats in DocExt
        assert_eq!(processed_doc.ext.svg_count, 1);
        assert_eq!(processed_doc.ext.links_resolved, 1);
    }
}
