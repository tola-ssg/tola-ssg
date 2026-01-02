//! Transform trait for type-safe phase transitions
//!
//! The Transform trait is the public API for phase transformations.
//! Concrete implementations (like `ProcessFolder`) implement this trait
//! to enable fluent document transformation.
//!
//! # Usage
//!
//! ```ignore
//! use tola::vdom::{Document, ProcessFolder};
//!
//! let indexed_doc: Document<Indexed> = ...;
//! let processed = indexed_doc.pipe(ProcessFolder::new());
//! ```
//!
//! # Design Notes
//!
//! The `Folder` trait in `folder.rs` is an internal implementation detail.
//! Users should only interact with `Transform` and concrete implementations
//! like `ProcessFolder`, `IndexFolder`, etc.

use super::node::Document;
use super::phase::PhaseData;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdom::node::Element;
    use crate::vdom::phase::Raw;

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
}

