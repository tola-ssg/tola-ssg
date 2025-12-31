//! VDOM Diff Algorithm
//!
//! Computes minimal patch operations between two VDOM trees.
//! Used for generating efficient hot reload updates.
//!
//! # Algorithm
//!
//! 1. Compare nodes by StableId (not just position)
//! 2. Use LCS to detect moves, insertions, deletions
//! 3. Generate minimal patch operations
//!
//! # Key Features
//!
//! - **Move Detection**: Reordered nodes generate `Move` ops, not Delete+Insert
//! - **Stable Identity**: Same source Span = same node across edits
//! - **Incremental Updates**: Only changed subtrees are patched
//!
//! # Complexity
//!
//! - Time: O(n * d) where d is the edit distance
//! - Space: O(n + m) for patch list
//!
//! For typical document updates (small changes), this is effectively O(n).

// This module is not yet integrated but will be used for incremental hot reload
#![allow(dead_code)]

use super::lcs::{diff_sequences, Edit};
use super::message::PatchOp;
use crate::vdom::id::StableId;
use crate::vdom::node::{Document, Element, Node};
use crate::vdom::phase::{Indexed, Processed};

/// Maximum depth for recursive diffing before fallback to full replace
const MAX_DIFF_DEPTH: usize = 50;

/// Maximum number of operations before fallback to full reload
const MAX_OPS: usize = 100;

/// Diff two VDOM documents and generate patch operations
pub fn diff_documents(
    old: &Document<Processed>,
    new: &Document<Processed>,
    path_prefix: &str,
) -> DiffResult {
    let mut ctx = DiffContext::new(path_prefix);
    ctx.diff_element(&old.root, &new.root, "body");
    ctx.into_result()
}

/// Result of diff operation
#[derive(Debug)]
pub struct DiffResult {
    /// Generated patch operations
    pub ops: Vec<PatchOp>,
    /// Whether diff exceeded limits and should fallback to reload
    pub should_reload: bool,
    /// Reason for reload (if should_reload is true)
    pub reload_reason: Option<String>,
}

impl DiffResult {
    /// Create a result that triggers reload
    pub fn reload(reason: impl Into<String>) -> Self {
        Self {
            ops: vec![],
            should_reload: true,
            reload_reason: Some(reason.into()),
        }
    }

    /// Check if any changes were detected
    pub fn has_changes(&self) -> bool {
        !self.ops.is_empty() || self.should_reload
    }
}

/// Internal diff context
struct DiffContext {
    /// Accumulated patch operations
    ops: Vec<PatchOp>,
    /// Current path prefix for selectors
    path_prefix: String,
    /// Current recursion depth
    depth: usize,
    /// Whether we should abort and reload
    should_reload: bool,
    /// Reload reason
    reload_reason: Option<String>,
}

impl DiffContext {
    fn new(path_prefix: &str) -> Self {
        Self {
            ops: Vec::new(),
            path_prefix: path_prefix.to_string(),
            depth: 0,
            should_reload: false,
            reload_reason: None,
        }
    }

    fn into_result(self) -> DiffResult {
        DiffResult {
            ops: self.ops,
            should_reload: self.should_reload,
            reload_reason: self.reload_reason,
        }
    }

    /// Check if we should abort diffing
    fn should_abort(&self) -> bool {
        self.should_reload || self.ops.len() > MAX_OPS || self.depth > MAX_DIFF_DEPTH
    }

    /// Trigger a reload fallback
    fn trigger_reload(&mut self, reason: impl Into<String>) {
        self.should_reload = true;
        self.reload_reason = Some(reason.into());
    }

    /// Diff two elements
    fn diff_element(&mut self, old: &Element<Processed>, new: &Element<Processed>, selector: &str) {
        if self.should_abort() {
            return;
        }

        // If tags differ, replace entirely
        if old.tag != new.tag {
            self.ops.push(PatchOp::replace(
                selector,
                render_element_html(new),
            ));
            return;
        }

        // Diff attributes
        self.diff_attrs(old, new, selector);

        // Diff children
        self.depth += 1;
        self.diff_children(&old.children, &new.children, selector);
        self.depth -= 1;
    }

    /// Diff element attributes
    fn diff_attrs(&mut self, old: &Element<Processed>, new: &Element<Processed>, selector: &str) {
        if self.should_abort() {
            return;
        }

        let mut changes: Vec<(String, Option<String>)> = Vec::new();

        // Check for changed/added attributes
        for (name, value) in &new.attrs {
            let old_value = old.get_attr(name);
            if old_value != Some(value.as_str()) {
                changes.push((name.clone(), Some(value.clone())));
            }
        }

        // Check for removed attributes
        for (name, _) in &old.attrs {
            if new.get_attr(name).is_none() {
                changes.push((name.clone(), None));
            }
        }

        if !changes.is_empty() {
            self.ops.push(PatchOp::update_attrs(selector, changes));
        }
    }

    /// Diff child nodes
    fn diff_children(
        &mut self,
        old_children: &[Node<Processed>],
        new_children: &[Node<Processed>],
        parent_selector: &str,
    ) {
        if self.should_abort() {
            return;
        }

        // Quick path: both empty
        if old_children.is_empty() && new_children.is_empty() {
            return;
        }

        // Quick path: old empty, insert all new
        if old_children.is_empty() {
            for (_i, child) in new_children.iter().enumerate() {
                self.ops.push(PatchOp::insert(
                    parent_selector,
                    "beforeend",
                    render_node_html(child),
                ));
                if self.should_abort() {
                    return;
                }
            }
            return;
        }

        // Quick path: new empty, remove all old
        if new_children.is_empty() {
            // Remove in reverse order to maintain indices
            for i in (0..old_children.len()).rev() {
                let child_selector = format!("{} > :nth-child({})", parent_selector, i + 1);
                self.ops.push(PatchOp::remove(&child_selector));
                if self.should_abort() {
                    return;
                }
            }
            return;
        }

        // Use simple index-based diffing for now
        // TODO: Implement LCS-based keyed diffing for better performance
        let max_len = old_children.len().max(new_children.len());

        for i in 0..max_len {
            if self.should_abort() {
                return;
            }

            let child_selector = format!("{} > :nth-child({})", parent_selector, i + 1);

            match (old_children.get(i), new_children.get(i)) {
                (Some(old_node), Some(new_node)) => {
                    self.diff_nodes(old_node, new_node, &child_selector);
                }
                (None, Some(new_node)) => {
                    // New child added
                    self.ops.push(PatchOp::insert(
                        parent_selector,
                        "beforeend",
                        render_node_html(new_node),
                    ));
                }
                (Some(_), None) => {
                    // Child removed
                    self.ops.push(PatchOp::remove(&child_selector));
                }
                (None, None) => unreachable!(),
            }
        }
    }

    /// Diff two nodes
    fn diff_nodes(&mut self, old: &Node<Processed>, new: &Node<Processed>, selector: &str) {
        if self.should_abort() {
            return;
        }

        match (old, new) {
            (Node::Element(old_elem), Node::Element(new_elem)) => {
                self.diff_element(old_elem, new_elem, selector);
            }
            (Node::Text(old_text), Node::Text(new_text)) => {
                if old_text.content != new_text.content {
                    self.ops.push(PatchOp::text(selector, &new_text.content));
                }
            }
            // Different node types - replace
            _ => {
                self.ops.push(PatchOp::replace(selector, render_node_html(new)));
            }
        }
    }
}

/// Render an element to HTML string
fn render_element_html(elem: &Element<Processed>) -> String {
    let mut html = String::new();
    html.push('<');
    html.push_str(&elem.tag);

    for (name, value) in &elem.attrs {
        html.push(' ');
        html.push_str(name);
        html.push_str("=\"");
        html.push_str(&html_escape(value));
        html.push('"');
    }

    html.push('>');

    for child in &elem.children {
        html.push_str(&render_node_html(child));
    }

    // Close tag (skip void elements)
    if !is_void_element(&elem.tag) {
        html.push_str("</");
        html.push_str(&elem.tag);
        html.push('>');
    }

    html
}

/// Render a node to HTML string
fn render_node_html(node: &Node<Processed>) -> String {
    match node {
        Node::Element(elem) => render_element_html(elem),
        Node::Text(text) => html_escape(&text.content),
        Node::Frame(_) => String::new(), // Frames should not exist at Processed phase
    }
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(c),
        }
    }
    result
}

/// Check if tag is a void element (self-closing)
fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta"
            | "param" | "source" | "track" | "wbr"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("a < b"), "a &lt; b");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }
}

// =============================================================================
// StableId-based Diff (New Implementation)
// =============================================================================

/// Statistics from diff operation
#[derive(Debug, Default, Clone, Copy)]
pub struct DiffStats {
    /// Number of elements compared
    pub elements_compared: usize,
    /// Number of text nodes compared
    pub text_nodes_compared: usize,
    /// Number of nodes kept unchanged
    pub nodes_kept: usize,
    /// Number of nodes moved
    pub nodes_moved: usize,
    /// Number of nodes replaced
    pub nodes_replaced: usize,
    /// Number of text updates
    pub text_updates: usize,
    /// Number of attribute updates
    pub attr_updates: usize,
}

/// Diff two Indexed VDOM documents using StableId-based comparison
///
/// This is the new diff implementation that uses StableId for accurate
/// node tracking across edits, enabling move detection.
pub fn diff_indexed_documents(
    old: &Document<Indexed>,
    new: &Document<Indexed>,
) -> IndexedDiffResult {
    let mut ctx = IndexedDiffContext::new();
    ctx.diff_element(&old.root, &new.root);
    ctx.into_result()
}

/// Result of StableId-based diff operation
#[derive(Debug)]
pub struct IndexedDiffResult {
    /// Generated patch operations (using StableId targets)
    pub ops: Vec<StableIdPatch>,
    /// Whether diff exceeded limits and should fallback to reload
    pub should_reload: bool,
    /// Reason for reload (if should_reload is true)
    pub reload_reason: Option<String>,
    /// Statistics about the diff
    pub stats: DiffStats,
}

impl IndexedDiffResult {
    /// Create a result that triggers reload
    pub fn reload(reason: impl Into<String>) -> Self {
        Self {
            ops: vec![],
            should_reload: true,
            reload_reason: Some(reason.into()),
            stats: DiffStats::default(),
        }
    }

    /// Check if any changes were detected
    pub fn has_changes(&self) -> bool {
        !self.ops.is_empty() || self.should_reload
    }
}

/// Patch operation using StableId for targeting
#[derive(Debug, Clone)]
pub enum StableIdPatch {
    /// Replace entire element
    Replace {
        target: StableId,
        html: String,
    },
    /// Update text content
    UpdateText {
        target: StableId,
        text: String,
    },
    /// Remove element
    Remove {
        target: StableId,
    },
    /// Insert new element
    Insert {
        parent: StableId,
        position: u32,
        html: String,
    },
    /// Move element to new position
    Move {
        target: StableId,
        new_parent: StableId,
        position: u32,
    },
    /// Update attributes
    UpdateAttrs {
        target: StableId,
        /// Attribute changes: (name, Some(value)) for set, (name, None) for remove
        attrs: Vec<(String, Option<String>)>,
    },
}

impl StableIdPatch {
    /// Get the target StableId of this patch
    pub fn target(&self) -> StableId {
        match self {
            Self::Replace { target, .. } => *target,
            Self::UpdateText { target, .. } => *target,
            Self::Remove { target } => *target,
            Self::Insert { parent, .. } => *parent,
            Self::Move { target, .. } => *target,
            Self::UpdateAttrs { target, .. } => *target,
        }
    }
}

/// Internal diff context for StableId-based diffing
struct IndexedDiffContext {
    ops: Vec<StableIdPatch>,
    depth: usize,
    should_reload: bool,
    reload_reason: Option<String>,
    stats: DiffStats,
}

impl IndexedDiffContext {
    fn new() -> Self {
        Self {
            ops: Vec::new(),
            depth: 0,
            should_reload: false,
            reload_reason: None,
            stats: DiffStats::default(),
        }
    }

    fn into_result(self) -> IndexedDiffResult {
        IndexedDiffResult {
            ops: self.ops,
            should_reload: self.should_reload,
            reload_reason: self.reload_reason,
            stats: self.stats,
        }
    }

    fn should_abort(&self) -> bool {
        self.should_reload || self.ops.len() > MAX_OPS || self.depth > MAX_DIFF_DEPTH
    }

    #[allow(dead_code)]
    fn trigger_reload(&mut self, reason: impl Into<String>) {
        self.should_reload = true;
        self.reload_reason = Some(reason.into());
    }

    /// Diff two elements by StableId
    fn diff_element(&mut self, old: &Element<Indexed>, new: &Element<Indexed>) {
        if self.should_abort() {
            return;
        }

        self.stats.elements_compared += 1;
        let old_id = old.ext.stable_id();

        // If tags differ, must replace entirely
        if old.tag != new.tag {
            self.ops.push(StableIdPatch::Replace {
                target: old_id,
                html: render_indexed_element_html(new),
            });
            self.stats.nodes_replaced += 1;
            return;
        }

        // Diff attributes
        self.diff_attrs(old, new);

        // Fast path: single text child optimization
        // Text nodes don't have data-tola-id in the DOM, so we handle them specially
        // by updating the parent element's textContent directly.
        //
        // This handles three cases:
        // 1. Both have single text child -> update textContent if different
        // 2. Old has single text, new is empty -> clear textContent
        // 3. Old is empty, new has single text -> set textContent
        let old_single_text = get_single_text_child(&old.children);
        let new_single_text = get_single_text_child(&new.children);

        match (old_single_text, new_single_text) {
            // Case 1: Both have single text child
            (Some(old_text), Some(new_text)) => {
                if old_text != new_text {
                    self.ops.push(StableIdPatch::UpdateText {
                        target: old_id,
                        text: new_text.to_string(),
                    });
                    self.stats.text_updates += 1;
                }
                self.stats.nodes_kept += 1;
                return;
            }
            // Case 2: Old has text, new is empty -> clear content
            (Some(_old_text), None) if new.children.is_empty() => {
                self.ops.push(StableIdPatch::UpdateText {
                    target: old_id,
                    text: String::new(),
                });
                self.stats.text_updates += 1;
                self.stats.nodes_kept += 1;
                return;
            }
            // Case 3: Old is empty, new has text -> set content
            (None, Some(new_text)) if old.children.is_empty() => {
                self.ops.push(StableIdPatch::UpdateText {
                    target: old_id,
                    text: new_text.to_string(),
                });
                self.stats.text_updates += 1;
                self.stats.nodes_kept += 1;
                return;
            }
            // Other cases: fall through to full children diff
            _ => {}
        }

        // Diff children using LCS
        self.depth += 1;
        self.diff_children(&old.children, &new.children, old_id);
        self.depth -= 1;

        self.stats.nodes_kept += 1;
    }

    /// Diff element attributes
    fn diff_attrs(&mut self, old: &Element<Indexed>, new: &Element<Indexed>) {
        if self.should_abort() {
            return;
        }

        let mut changes: Vec<(String, Option<String>)> = Vec::new();

        // Check for changed/added attributes
        for (name, value) in &new.attrs {
            let old_value = old.get_attr(name);
            if old_value != Some(value.as_str()) {
                changes.push((name.clone(), Some(value.clone())));
            }
        }

        // Check for removed attributes
        for (name, _) in &old.attrs {
            if new.get_attr(name).is_none() {
                changes.push((name.clone(), None));
            }
        }

        if !changes.is_empty() {
            self.ops.push(StableIdPatch::UpdateAttrs {
                target: old.ext.stable_id(),
                attrs: changes,
            });
            self.stats.attr_updates += 1;
        }
    }

    /// Diff child nodes using LCS algorithm
    fn diff_children(
        &mut self,
        old_children: &[Node<Indexed>],
        new_children: &[Node<Indexed>],
        parent_id: StableId,
    ) {
        if self.should_abort() {
            return;
        }

        // Quick path: both empty
        if old_children.is_empty() && new_children.is_empty() {
            return;
        }

        // Quick path: old empty, insert all new
        if old_children.is_empty() {
            for (i, child) in new_children.iter().enumerate() {
                self.ops.push(StableIdPatch::Insert {
                    parent: parent_id,
                    position: i as u32,
                    html: render_indexed_node_html(child),
                });
            }
            return;
        }

        // Quick path: new empty, remove all old
        if new_children.is_empty() {
            for child in old_children.iter().rev() {
                self.ops.push(StableIdPatch::Remove {
                    target: get_indexed_node_stable_id(child),
                });
            }
            return;
        }

        // Extract StableIds for LCS comparison
        let old_ids: Vec<StableId> = old_children
            .iter()
            .map(get_indexed_node_stable_id)
            .collect();
        let new_ids: Vec<StableId> = new_children
            .iter()
            .map(get_indexed_node_stable_id)
            .collect();

        // Compute LCS diff
        let lcs_result = diff_sequences(&old_ids, &new_ids);

        // Collect edits and apply in safe order to avoid duplication issues
        let mut deletes: Vec<usize> = Vec::new();
        let mut inserts: Vec<(usize, &Node<Indexed>)> = Vec::new();
        let mut moves: Vec<(usize, usize)> = Vec::new(); // (old_idx, new_idx)
        let mut keeps: Vec<(usize, usize)> = Vec::new();

        for edit in &lcs_result.edits {
            match edit {
                Edit::Keep { old_idx, new_idx } => keeps.push((*old_idx, *new_idx)),
                Edit::Insert { new_idx } => inserts.push((*new_idx, &new_children[*new_idx])),
                Edit::Delete { old_idx } => deletes.push(*old_idx),
                Edit::Move { old_idx, new_idx } => moves.push((*old_idx, *new_idx)),
            }
        }

        if self.should_abort() {
            return;
        }

        // Remove deletions first in reverse order to preserve indices
        deletes.sort_unstable_by(|a, b| b.cmp(a));
        for old_idx in deletes {
            let old_node = &old_children[old_idx];
            self.ops.push(StableIdPatch::Remove {
                target: get_indexed_node_stable_id(old_node),
            });
        }

        if self.should_abort() {
            return;
        }

        // Apply moves next
        for (old_idx, new_idx) in &moves {
            let old_node = &old_children[*old_idx];
            self.ops.push(StableIdPatch::Move {
                target: get_indexed_node_stable_id(old_node),
                new_parent: parent_id,
                position: *new_idx as u32,
            });
            self.stats.nodes_moved += 1;
        }

        if self.should_abort() {
            return;
        }

        // Apply inserts in ascending order of position
        inserts.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for (new_idx, new_node) in inserts {
            self.ops.push(StableIdPatch::Insert {
                parent: parent_id,
                position: new_idx as u32,
                html: render_indexed_node_html(new_node),
            });
        }

        if self.should_abort() {
            return;
        }

        // Finally, diff content of kept nodes and moved nodes
        for (old_idx, new_idx) in keeps {
            let old_node = &old_children[old_idx];
            let new_node = &new_children[new_idx];
            self.diff_nodes(old_node, new_node);
        }

        for (old_idx, new_idx) in moves {
            let old_node = &old_children[old_idx];
            let new_node = &new_children[new_idx];
            self.diff_nodes(old_node, new_node);
        }
    }

    /// Diff two nodes
    fn diff_nodes(&mut self, old: &Node<Indexed>, new: &Node<Indexed>) {
        if self.should_abort() {
            return;
        }

        match (old, new) {
            (Node::Element(old_elem), Node::Element(new_elem)) => {
                self.diff_element(old_elem, new_elem);
            }
            (Node::Text(old_text), Node::Text(new_text)) => {
                self.stats.text_nodes_compared += 1;
                if old_text.content != new_text.content {
                    self.ops.push(StableIdPatch::UpdateText {
                        target: old_text.ext.stable_id,
                        text: new_text.content.clone(),
                    });
                    self.stats.text_updates += 1;
                }
            }
            (Node::Frame(old_frame), Node::Frame(new_frame)) => {
                // Frames at Indexed phase are placeholders (SVG content is in ext)
                // They should have been converted to SVG Elements, so just compare by ID
                if old_frame.ext.stable_id != new_frame.ext.stable_id {
                    self.ops.push(StableIdPatch::Replace {
                        target: old_frame.ext.stable_id,
                        html: render_indexed_node_html(&Node::Frame(new_frame.clone())),
                    });
                    self.stats.nodes_replaced += 1;
                }
            }
            // Different node types - replace
            _ => {
                self.ops.push(StableIdPatch::Replace {
                    target: get_indexed_node_stable_id(old),
                    html: render_indexed_node_html(new),
                });
                self.stats.nodes_replaced += 1;
            }
        }
    }
}

/// Check if children contain exactly one text node and return its content
///
/// Returns `Some(&str)` if children is exactly `[Text(content)]`, `None` otherwise.
/// Used to optimize single-text-child elements like `<p>Hello</p>`.
fn get_single_text_child(children: &[Node<Indexed>]) -> Option<&str> {
    if children.len() == 1 {
        if let Node::Text(text) = &children[0] {
            return Some(&text.content);
        }
    }
    None
}

/// Get StableId from any Indexed node type
fn get_indexed_node_stable_id(node: &Node<Indexed>) -> StableId {
    match node {
        Node::Element(elem) => elem.ext.stable_id(),
        Node::Text(text) => text.ext.stable_id,
        Node::Frame(frame) => frame.ext.stable_id,
    }
}

/// Render an Indexed element to HTML string with data-tola-id attribute
fn render_indexed_element_html(elem: &Element<Indexed>) -> String {
    let mut html = String::new();
    html.push('<');
    html.push_str(&elem.tag);

    // Add StableId as data attribute for client-side targeting
    html.push_str(" data-tola-id=\"");
    html.push_str(&elem.ext.stable_id().to_attr_value());
    html.push('"');

    for (name, value) in &elem.attrs {
        html.push(' ');
        html.push_str(name);
        html.push_str("=\"");
        html.push_str(&html_escape(value));
        html.push('"');
    }

    html.push('>');

    for child in &elem.children {
        html.push_str(&render_indexed_node_html(child));
    }

    // Close tag (skip void elements)
    if !is_void_element(&elem.tag) {
        html.push_str("</");
        html.push_str(&elem.tag);
        html.push('>');
    }

    html
}

/// Render an Indexed node to HTML string
fn render_indexed_node_html(node: &Node<Indexed>) -> String {
    match node {
        Node::Element(elem) => render_indexed_element_html(elem),
        Node::Text(text) => {
            // Render text directly without wrapper - matches production HTML output
            html_escape(&text.content)
        }
        Node::Frame(frame) => {
            // Frame at Indexed phase is a placeholder
            // In practice, frames are converted to SVG Elements before this point
            format!(
                "<span data-tola-id=\"{}\"><!-- frame --></span>",
                frame.ext.stable_id
            )
        }
    }
}

#[cfg(test)]
mod indexed_tests {
    use super::*;

    #[test]
    fn test_stable_id_patch_target() {
        let patch = StableIdPatch::Replace {
            target: StableId::from_raw(42),
            html: "<div></div>".to_string(),
        };
        assert_eq!(patch.target().as_raw(), 42);
    }

    #[test]
    fn test_indexed_diff_result_reload() {
        let result = IndexedDiffResult::reload("test reason");
        assert!(result.should_reload);
        assert_eq!(result.reload_reason, Some("test reason".to_string()));
        assert!(result.has_changes());
    }

    #[test]
    fn test_diff_stats_default() {
        let stats = DiffStats::default();
        assert_eq!(stats.elements_compared, 0);
        assert_eq!(stats.nodes_kept, 0);
        assert_eq!(stats.nodes_moved, 0);
    }

    #[test]
    fn test_diff_detects_text_update_and_revert() {
        use crate::vdom::phase::Indexed;
        use crate::vdom::{Document, Element, Node, Text, FamilyExt, NodeId};
        use crate::vdom::id::StableId;
        use super::{diff_indexed_documents, StableIdPatch};

        // Helper to build a simple document with three paragraphs
        fn build_doc(texts: &[&str], base_id: u64) -> Document<Indexed> {
            let mut root = Element::new("body");

            for (i, t) in texts.iter().enumerate() {
                // Create indexed elem ext with stable id
                let sid = StableId::from_raw(base_id + i as u64 + 1);
                let elem_ext = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> {
                    stable_id: sid,
                    node_id: NodeId::new((i + 1) as u32),
                    family_data: (),
                };
                let mut p = Element::with_ext("p", FamilyExt::Other(elem_ext));

                // Text node with stable id
                let text_sid = StableId::from_raw(base_id + 100 + i as u64 + 1);
                let mut txt = Text::new(*t);
                txt.ext = crate::vdom::phase::IndexedTextExt::new(text_sid);
                p.children.push(Node::Text(txt));
                root.children.push(Node::Element(Box::new(p)));
            }

            Document::new(root)
        }

        // Original
        let old = build_doc(&["cosplay堂吉珂德", "浊富 or 清贫?", "GALGAME!"], 1000);
        // First edit: remove a char (simulate small change)
        let new1 = build_doc(&["cosplay堂吉珂德", "浊富 or 清?", "GALGAME!"], 1000);
        // Second edit: restore
        let new2 = build_doc(&["cosplay堂吉珂德", "浊富 or 清贫?", "GALGAME!"], 1000);

        // Diff old -> new1
        let r1 = diff_indexed_documents(&old, &new1);
        assert!(!r1.should_reload, "diff should not force reload");
        // Should have at least one UpdateText or Replace targeting the second paragraph's text id
        let has_update1 = r1.ops.iter().any(|op| match op {
            StableIdPatch::UpdateText { target, text } => {
                target.as_raw() == (100 + 100 + 2) || text.contains("浊富")
            }
            StableIdPatch::Replace { target, html } => target.as_raw() == (100 + 2) || html.contains("浊富") ,
            _ => false,
        });
        assert!(has_update1, "Expected a text update or replace for the first edit, got ops={:?}", r1.ops);

        // Update cache scenario: diff new1 -> new2 (restore)
        let r2 = diff_indexed_documents(&new1, &new2);
        assert!(!r2.should_reload, "diff should not force reload on revert");
        let has_update2 = r2.ops.iter().any(|op| match op {
            StableIdPatch::UpdateText { target, text } => text.contains("清贫"),
            StableIdPatch::Replace { target, html } => html.contains("清贫"),
            _ => false,
        });
        assert!(has_update2, "Expected a text update or replace restoring the text, got ops={:?}", r2.ops);
    }

    #[test]
    fn test_edit_ordering_prevents_duplicates() {
        use crate::vdom::phase::Indexed;
        use crate::vdom::{Document, Element, Node, FamilyExt, NodeId};
        use crate::vdom::id::StableId;

        // Build old: A,B,C
        let mut root_old = Element::new("div");
        let a_ext = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> { stable_id: StableId::from_raw(1), node_id: NodeId::new(1), family_data: () };
        let b_ext = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> { stable_id: StableId::from_raw(2), node_id: NodeId::new(2), family_data: () };
        let c_ext = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> { stable_id: StableId::from_raw(3), node_id: NodeId::new(3), family_data: () };
        root_old.children.push(Node::Element(Box::new(Element::with_ext("p", FamilyExt::Other(a_ext)))));
        root_old.children.push(Node::Element(Box::new(Element::with_ext("p", FamilyExt::Other(b_ext)))));
        root_old.children.push(Node::Element(Box::new(Element::with_ext("p", FamilyExt::Other(c_ext)))));
        let old = Document::new(root_old);

        // Build new: A,C,B (move C before B) and insert D at end
        let mut root_new = Element::new("div");
        let a_ext2 = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> { stable_id: StableId::from_raw(1), node_id: NodeId::new(1), family_data: () };
        let c_ext2 = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> { stable_id: StableId::from_raw(3), node_id: NodeId::new(3), family_data: () };
        let b_ext2 = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> { stable_id: StableId::from_raw(2), node_id: NodeId::new(2), family_data: () };
        let d_ext = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> { stable_id: StableId::from_raw(4), node_id: NodeId::new(4), family_data: () };
        root_new.children.push(Node::Element(Box::new(Element::with_ext("p", FamilyExt::Other(a_ext2)))));
        root_new.children.push(Node::Element(Box::new(Element::with_ext("p", FamilyExt::Other(c_ext2)))));
        root_new.children.push(Node::Element(Box::new(Element::with_ext("p", FamilyExt::Other(b_ext2)))));
        root_new.children.push(Node::Element(Box::new(Element::with_ext("p", FamilyExt::Other(d_ext)))));
        let new = Document::new(root_new);

        let r = diff_indexed_documents(&old, &new);
        // Ensure all removes appear before inserts in the ops order (no duplicate insert of moved node)
        let mut first_insert = None;
        let mut first_remove = None;
        for (i, op) in r.ops.iter().enumerate() {
            match op {
                StableIdPatch::Insert { .. } => if first_insert.is_none() { first_insert = Some(i) },
                StableIdPatch::Remove { .. } => if first_remove.is_none() { first_remove = Some(i) },
                _ => {}
            }
        }
        assert!(first_remove.is_none() || first_insert.is_none() || first_remove.unwrap() < first_insert.unwrap(), "Removals should come before inserts to avoid duplicates: ops={:?}", r.ops);
    }

    #[test]
    fn test_single_text_child_empty_to_text() {
        // Test: old element is empty, new element has single text child
        // Should generate UpdateText to parent, not Insert of text node
        use crate::vdom::{Document, Element, Node, Text, FamilyExt, NodeId};
        use crate::vdom::id::StableId;

        // Build old: <p></p>
        let mut root_old = Element::new("body");
        let p_ext = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> {
            stable_id: StableId::from_raw(100),
            node_id: NodeId::new(1),
            family_data: (),
        };
        let p_old = Element::with_ext("p", FamilyExt::Other(p_ext));
        // Empty children
        root_old.children.push(Node::Element(Box::new(p_old)));
        let old = Document::new(root_old);

        // Build new: <p>Hello</p>
        let mut root_new = Element::new("body");
        let p_ext2 = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> {
            stable_id: StableId::from_raw(100),
            node_id: NodeId::new(1),
            family_data: (),
        };
        let mut p_new = Element::with_ext("p", FamilyExt::Other(p_ext2));
        let mut txt = Text::new("Hello");
        txt.ext = crate::vdom::phase::IndexedTextExt::new(StableId::from_raw(200));
        p_new.children.push(Node::Text(txt));
        root_new.children.push(Node::Element(Box::new(p_new)));
        let new = Document::new(root_new);

        let result = diff_indexed_documents(&old, &new);

        // Should have UpdateText targeting the paragraph (id=100), not Insert
        let has_update_text = result.ops.iter().any(|op| {
            matches!(op, StableIdPatch::UpdateText { target, text } if target.as_raw() == 100 && text == "Hello")
        });
        assert!(has_update_text, "Expected UpdateText to parent element, got: {:?}", result.ops);

        // Should NOT have any Insert operations
        let has_insert = result.ops.iter().any(|op| matches!(op, StableIdPatch::Insert { .. }));
        assert!(!has_insert, "Should not have Insert for text node: {:?}", result.ops);
    }

    #[test]
    fn test_single_text_child_text_to_empty() {
        // Test: old element has single text child, new element is empty
        // Should generate UpdateText with empty string, not Remove of text node
        use crate::vdom::{Document, Element, Node, Text, FamilyExt, NodeId};
        use crate::vdom::id::StableId;

        // Build old: <p>Hello</p>
        let mut root_old = Element::new("body");
        let p_ext = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> {
            stable_id: StableId::from_raw(100),
            node_id: NodeId::new(1),
            family_data: (),
        };
        let mut p_old = Element::with_ext("p", FamilyExt::Other(p_ext));
        let mut txt = Text::new("Hello");
        txt.ext = crate::vdom::phase::IndexedTextExt::new(StableId::from_raw(200));
        p_old.children.push(Node::Text(txt));
        root_old.children.push(Node::Element(Box::new(p_old)));
        let old = Document::new(root_old);

        // Build new: <p></p>
        let mut root_new = Element::new("body");
        let p_ext2 = crate::vdom::phase::IndexedElemExt::<crate::vdom::family::OtherFamily> {
            stable_id: StableId::from_raw(100),
            node_id: NodeId::new(1),
            family_data: (),
        };
        let p_new = Element::with_ext("p", FamilyExt::Other(p_ext2));
        // Empty children
        root_new.children.push(Node::Element(Box::new(p_new)));
        let new = Document::new(root_new);

        let result = diff_indexed_documents(&old, &new);

        // Should have UpdateText targeting the paragraph with empty string
        let has_update_text = result.ops.iter().any(|op| {
            matches!(op, StableIdPatch::UpdateText { target, text } if target.as_raw() == 100 && text.is_empty())
        });
        assert!(has_update_text, "Expected UpdateText with empty string to parent element, got: {:?}", result.ops);

        // Should NOT have any Remove operations
        let has_remove = result.ops.iter().any(|op| matches!(op, StableIdPatch::Remove { .. }));
        assert!(!has_remove, "Should not have Remove for text node: {:?}", result.ops);
    }
}
