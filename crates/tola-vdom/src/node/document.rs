//! Document type and related utilities
//!
//! The root container for VDOM trees, with query and traversal APIs.

use crate::family::FamilyKind;
use crate::phase::PhaseData;

use super::{Element, Node};

// =============================================================================
// Document<P>
// =============================================================================

/// Root document container
#[derive(Debug, Clone)]
pub struct Document<P: PhaseData> {
    /// Root element (typically <html> or a wrapper)
    pub root: Element<P>,
    /// Document-level extension data (metadata, stats)
    pub ext: P::DocExt,
}

impl<P: PhaseData> Document<P> {
    /// Create a new document with a root element
    pub fn new(root: Element<P>) -> Self {
        Self {
            root,
            ext: P::DocExt::default(),
        }
    }

    /// Get the phase name for debugging
    pub fn phase_name(&self) -> &'static str {
        P::NAME
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Query API (v8)
    // ─────────────────────────────────────────────────────────────────────────

    /// Find first element matching predicate (depth-first search)
    pub fn find_element<F>(&self, predicate: F) -> Option<&Element<P>>
    where
        F: Fn(&Element<P>) -> bool,
    {
        Self::find_in_element(&self.root, &predicate)
    }

    fn find_in_element<'a, F>(elem: &'a Element<P>, predicate: &F) -> Option<&'a Element<P>>
    where
        F: Fn(&Element<P>) -> bool,
    {
        if predicate(elem) {
            return Some(elem);
        }
        for child in &elem.children {
            if let Some(child_elem) = child.as_element()
                && let Some(found) = Self::find_in_element(child_elem, predicate)
            {
                return Some(found);
            }
        }
        None
    }

    /// Find all elements matching predicate
    pub fn find_all<F>(&self, predicate: F) -> Vec<&Element<P>>
    where
        F: Fn(&Element<P>) -> bool,
    {
        let mut results = Vec::new();
        Self::collect_elements(&self.root, &predicate, &mut results);
        results
    }

    fn collect_elements<'a, F>(elem: &'a Element<P>, predicate: &F, results: &mut Vec<&'a Element<P>>)
    where
        F: Fn(&Element<P>) -> bool,
    {
        if predicate(elem) {
            results.push(elem);
        }
        for child in &elem.children {
            if let Some(child_elem) = child.as_element() {
                Self::collect_elements(child_elem, predicate, results);
            }
        }
    }

    /// Check if any element matches predicate
    pub fn has_element<F>(&self, predicate: F) -> bool
    where
        F: Fn(&Element<P>) -> bool,
    {
        self.find_element(predicate).is_some()
    }

    /// Count total elements in document
    pub fn element_count(&self) -> usize {
        Self::count_elements(&self.root)
    }

    fn count_elements(elem: &Element<P>) -> usize {
        let mut count = 1; // this element
        for child in &elem.children {
            if let Some(child_elem) = child.as_element() {
                count += Self::count_elements(child_elem);
            }
        }
        count
    }

    /// Iterate over all elements (depth-first)
    pub fn iter_elements(&self) -> impl Iterator<Item = &Element<P>> {
        ElementIterator::new(&self.root)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Mutable query API
    // ─────────────────────────────────────────────────────────────────────────

    /// Find first element matching predicate (mutable)
    pub fn find_element_mut<F>(&mut self, predicate: F) -> Option<&mut Element<P>>
    where
        F: Fn(&Element<P>) -> bool + Copy,
    {
        Self::find_in_element_mut(&mut self.root, predicate)
    }

    fn find_in_element_mut<F>(elem: &mut Element<P>, predicate: F) -> Option<&mut Element<P>>
    where
        F: Fn(&Element<P>) -> bool + Copy,
    {
        if predicate(elem) {
            return Some(elem);
        }
        for child in &mut elem.children {
            if let Some(child_elem) = child.as_element_mut()
                && let Some(found) = Self::find_in_element_mut(child_elem, predicate)
            {
                return Some(found);
            }
        }
        None
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Closure-based traversal API
    // ─────────────────────────────────────────────────────────────────────────

    /// Visit all elements with a closure (read-only)
    pub fn for_each_element<F>(&self, mut f: F)
    where
        F: FnMut(&Element<P>),
    {
        Self::visit_elements_recursive(&self.root, &mut f);
    }

    fn visit_elements_recursive<F>(elem: &Element<P>, f: &mut F)
    where
        F: FnMut(&Element<P>),
    {
        f(elem);
        for child in &elem.children {
            if let Some(child_elem) = child.as_element() {
                Self::visit_elements_recursive(child_elem, f);
            }
        }
    }

    /// Visit all elements with a closure (mutable)
    pub fn for_each_element_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut Element<P>),
    {
        Self::visit_elements_mut_recursive(&mut self.root, &mut f);
    }

    fn visit_elements_mut_recursive<F>(elem: &mut Element<P>, f: &mut F)
    where
        F: FnMut(&mut Element<P>),
    {
        f(elem);
        for child in &mut elem.children {
            if let Some(child_elem) = child.as_element_mut() {
                Self::visit_elements_mut_recursive(child_elem, f);
            }
        }
    }

    /// Collect statistics about the document
    pub fn collect_stats(&self) -> Stats {
        let mut stats = Stats::default();
        Self::collect_stats_recursive(&self.root, &mut stats);
        stats
    }

    fn collect_stats_recursive(elem: &Element<P>, stats: &mut Stats) {
        stats.element_count += 1;

        // Count by family
        match elem.ext.kind() {
            FamilyKind::Svg => stats.svg_count += 1,
            FamilyKind::Link => stats.link_count += 1,
            FamilyKind::Heading => stats.heading_count += 1,
            FamilyKind::Media => stats.media_count += 1,
            FamilyKind::Other => {}
        }

        // Recurse into children
        for child in &elem.children {
            match child {
                Node::Element(e) => Self::collect_stats_recursive(e, stats),
                Node::Text(_) => stats.text_count += 1,
            }
        }
    }
}

// =============================================================================
// ElementIterator - depth-first element traversal
// =============================================================================

/// Depth-first iterator over elements
pub struct ElementIterator<'a, P: PhaseData> {
    stack: Vec<&'a Element<P>>,
}

impl<'a, P: PhaseData> ElementIterator<'a, P> {
    fn new(root: &'a Element<P>) -> Self {
        Self { stack: vec![root] }
    }
}

impl<'a, P: PhaseData> Iterator for ElementIterator<'a, P> {
    type Item = &'a Element<P>;

    fn next(&mut self) -> Option<Self::Item> {
        let elem = self.stack.pop()?;
        // Push children in reverse order so they're visited left-to-right
        for child in elem.children.iter().rev() {
            if let Some(child_elem) = child.as_element() {
                self.stack.push(child_elem);
            }
        }
        Some(elem)
    }
}

// =============================================================================
// Stats - document statistics
// =============================================================================

/// Document statistics collected from traversal
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    pub svg_count: usize,
    pub link_count: usize,
    pub heading_count: usize,
    pub media_count: usize,
    pub text_count: usize,
    pub element_count: usize,
}

impl Stats {
    /// Check if document has any SVG elements
    pub fn has_svg(&self) -> bool {
        self.svg_count > 0
    }

    /// Check if document has any links
    pub fn has_links(&self) -> bool {
        self.link_count > 0
    }

    /// Check if document has any headings
    pub fn has_headings(&self) -> bool {
        self.heading_count > 0
    }

    /// Total family-specific elements (svg + link + heading + media)
    pub fn family_element_count(&self) -> usize {
        self.svg_count + self.link_count + self.heading_count + self.media_count
    }
}
