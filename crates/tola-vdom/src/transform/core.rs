//! Transform trait and utilities for type-safe phase transitions
//!
//! This module provides the unified API for VDOM phase transformations.
//! All phase transitions (Raw → Indexed → Processed → Rendered) are
//! expressed through the `Transform` trait.
//!
//! # Design
//!
//! - `Transform` trait: The only public API for phase transitions
//! - `Processor`: Indexed → Processed transformation
//! - `Pipeline`: Fluent chain builder for transforms
//!
//! # Usage
//!
//! ```ignore
//! use tola::vdom::{Document, Transform, Processor};
//!
//! let indexed_doc: Document<Indexed> = ...;
//! let processed = indexed_doc.pipe(Processor::new());
//! // or
//! let processed = Processor::new().transform(indexed_doc);
//! ```

use smallvec::SmallVec;

use crate::family::{
    HeadingFamily, LinkFamily, MediaFamily, OtherFamily, SvgFamily, TagFamily,
};
use crate::node::{Document, Element, FamilyExt, Node, Text};
use crate::phase::{
    Indexed, IndexedElemExt, PhaseData, Processed, ProcessedDocExt, ProcessedElemExt,
};

// =============================================================================
// Transform trait
// =============================================================================

/// Type-safe phase transformation
///
/// Key design decisions (v8):
/// - Consumes `self` (no lifetime complications)
/// - No `name()` method (unnecessary at runtime)
/// - Output phase is an associated type named `To` for consistency with docs
///
/// Usage:
/// ```ignore
/// let indexed_doc = doc.pipe(IndexerTransform::new());
/// let processed_doc = indexed_doc.pipe(LinkProcessor::new());
/// ```
pub trait Transform<P: PhaseData> {
    /// The output phase after transformation
    type To: PhaseData;

    /// Transform a document from phase P to phase Self::To
    fn transform(self, doc: Document<P>) -> Document<Self::To>;
}

// =============================================================================
// Pipeline - Fluent chain builder
// =============================================================================

/// Pipeline builder for chaining transforms
///
/// Provides fluent API for document transformation:
/// ```ignore
/// let result = Pipeline::new(doc)
///     .then(Indexer::new())
///     .apply(|doc| process_links(doc))
///     .then(FrameExpander::new())
///     .finish();
/// ```
pub struct Pipeline<P: PhaseData> {
    doc: Document<P>,
}

impl<P: PhaseData> Pipeline<P> {
    /// Create a new pipeline with the given document
    pub fn new(doc: Document<P>) -> Self {
        Self { doc }
    }

    /// Apply a phase transform: P → T::To
    pub fn then<T>(self, transform: T) -> Pipeline<T::To>
    where
        T: Transform<P>,
        T::To: PhaseData,
    {
        Pipeline {
            doc: transform.transform(self.doc),
        }
    }

    /// Apply an in-place modification closure (same phase)
    pub fn apply<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut Document<P>),
    {
        f(&mut self.doc);
        self
    }

    /// Finish the pipeline and return the document
    pub fn finish(self) -> Document<P> {
        self.doc
    }

    /// Get a reference to the document
    pub fn doc(&self) -> &Document<P> {
        &self.doc
    }

    /// Get a mutable reference to the document
    pub fn doc_mut(&mut self) -> &mut Document<P> {
        &mut self.doc
    }
}

// =============================================================================
// Document extension methods for pipeline API
// =============================================================================

impl<P: PhaseData> Document<P> {
    /// Pipe this document through a transform
    ///
    /// Enables fluent API:
    /// ```ignore
    /// let result = doc
    ///     .pipe(IndexerTransform::new())
    ///     .pipe(LinkProcessor::new())
    ///     .pipe(SvgOptimizer::new());
    /// ```
    pub fn pipe<T>(self, transform: T) -> Document<T::To>
    where
        T: Transform<P>,
    {
        transform.transform(self)
    }

    /// Start a pipeline with this document
    pub fn into_pipeline(self) -> Pipeline<P> {
        Pipeline::new(self)
    }
}

// =============================================================================
// Identity Transform
// =============================================================================

/// Identity transform that returns the document unchanged
///
/// Useful for testing and as a base for conditional transforms.
pub struct IdentityTransform<P: PhaseData> {
    _phase: std::marker::PhantomData<P>,
}

impl<P: PhaseData> IdentityTransform<P> {
    pub fn new() -> Self {
        Self {
            _phase: std::marker::PhantomData,
        }
    }
}

impl<P: PhaseData> Default for IdentityTransform<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: PhaseData> Transform<P> for IdentityTransform<P> {
    type To = P;

    fn transform(self, doc: Document<P>) -> Document<P> {
        doc
    }
}

// =============================================================================
// Processor - Indexed → Processed transformation
// =============================================================================

/// Transform FamilyExt from Indexed to Processed phase
///
/// This function handles the GAT-based type transformation:
/// - Takes `FamilyExt<Indexed>` (each variant has `IndexedElemExt<F>`)
/// - Returns `FamilyExt<Processed>` (each variant has `ProcessedElemExt<F>`)
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
fn process_elem_ext<F: TagFamily>(indexed: IndexedElemExt<F>) -> ProcessedElemExt<F> {
    ProcessedElemExt {
        stable_id: indexed.stable_id,
        modified: false,
        family_data: F::process(&indexed.family_data),
    }
}

/// Indexed → Processed transformation
///
/// Transforms a document from Indexed phase to Processed phase,
/// collecting statistics along the way.
///
/// # Usage
/// ```ignore
/// let indexed_doc: Document<Indexed> = ...;
/// let processed_doc = indexed_doc.pipe(Processor::new());
/// ```
pub struct Processor {
    stats: ProcessedDocExt,
}

impl Processor {
    /// Create a new Processor
    pub fn new() -> Self {
        Self {
            stats: ProcessedDocExt::default(),
        }
    }

    /// Get processing statistics
    pub fn stats(&self) -> &ProcessedDocExt {
        &self.stats
    }

    /// Transform an element from Indexed to Processed
    fn transform_element(&mut self, elem: Element<Indexed>) -> Element<Processed> {
        // Track statistics based on family
        match &elem.ext {
            FamilyExt::Svg(_) => self.stats.svg_count += 1,
            FamilyExt::Link(_) => self.stats.links_resolved += 1,
            FamilyExt::Heading(_) => self.stats.headings_anchored += 1,
            _ => {}
        }

        // Recursively transform children
        let children: Vec<Node<Processed>> = elem
            .children
            .into_iter()
            .map(|child| self.transform_node(child))
            .collect();

        Element {
            tag: elem.tag,
            attrs: elem.attrs,
            children: SmallVec::from_vec(children),
            ext: process_family_ext(elem.ext),
        }
    }

    /// Transform a node from Indexed to Processed
    fn transform_node(&mut self, node: Node<Indexed>) -> Node<Processed> {
        match node {
            Node::Element(elem) => Node::Element(Box::new(self.transform_element(*elem))),
            Node::Text(text) => Node::Text(Text {
                content: text.content,
                ext: (),
            }),
        }
    }
}

impl Default for Processor {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform<Indexed> for Processor {
    type To = Processed;

    fn transform(mut self, doc: Document<Indexed>) -> Document<Processed> {
        let root = self.transform_element(doc.root);
        let stats = std::mem::take(&mut self.stats);
        Document { root, ext: stats }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Element;
    use crate::phase::{IndexedDocExt, Raw};

    #[test]
    fn test_identity_transform() {
        let elem: Element<Raw> = Element::new("div");
        let doc = Document::new(elem);

        let result = doc.pipe(IdentityTransform::new());
        assert_eq!(result.root.tag, "div");
    }

    #[test]
    fn test_pipeline_chain() {
        let elem: Element<Raw> = Element::new("html");
        let doc = Document::new(elem);

        // Chain multiple identity transforms
        let result = doc
            .pipe(IdentityTransform::new())
            .pipe(IdentityTransform::new())
            .pipe(IdentityTransform::new());

        assert_eq!(result.root.tag, "html");
    }

    #[test]
    fn test_pipeline_apply() {
        let elem: Element<Raw> = Element::new("div");
        let doc = Document::new(elem);

        let result = Pipeline::new(doc)
            .apply(|doc| {
                doc.root.set_attr("id", "main");
            })
            .finish();

        assert_eq!(result.root.get_attr("id"), Some("main"));
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

        // Apply Processor
        let processed_doc = doc.pipe(Processor::new());

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

        // Apply Processor via Transform trait
        let processed_doc = doc.pipe(Processor::new());

        // Verify stats in DocExt
        assert_eq!(processed_doc.ext.svg_count, 1);
        assert_eq!(processed_doc.ext.links_resolved, 1);
    }
}