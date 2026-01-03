//! Document node type.

use crate::phase::PhaseData;
use super::Element;

/// Root document container.
///
/// A document holds a root element and phase-specific extension data.
#[derive(Debug, Clone)]
pub struct Document<P: PhaseData> {
    /// Root element of the document.
    pub root: Element<P>,
    /// Phase-specific document extension data.
    pub ext: P::DocExt,
}

impl<P: PhaseData> Document<P> {
    /// Create a new document with root element and default extension.
    pub fn new(root: Element<P>) -> Self
    where
        P::DocExt: Default,
    {
        Self {
            root,
            ext: P::DocExt::default(),
        }
    }

    /// Create a document with explicit extension data.
    pub fn with_ext(root: Element<P>, ext: P::DocExt) -> Self {
        Self { root, ext }
    }
}
