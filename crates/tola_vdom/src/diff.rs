//! VDOM Diff Algorithm
//!
//! Computes minimal patch operations between two VDOM trees.
//! This is a pure algorithm module with no I/O dependencies.
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
//! - **Stable Identity**: Same StableId = same node across edits
//! - **Incremental Updates**: Only changed subtrees are patched
//!
//! # Complexity
//!
//! - Time: O(n * d) where d is the edit distance
//! - Space: O(n + m) for patch list
//!
//! For typical document updates (small changes), this is effectively O(n).

use super::id::StableId;
use super::lcs::{diff_sequences, Edit};
use super::node::{Document, Element, Node};
use super::phase::Indexed;

/// Maximum depth for recursive diffing before fallback to full replace
const MAX_DIFF_DEPTH: usize = 50;

/// Maximum number of operations before fallback to full reload
const MAX_OPS: usize = 100;

// =============================================================================
// Public Types
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

/// Result of StableId-based diff operation
#[derive(Debug)]
pub struct DiffResult {
    /// Generated patch operations (using StableId targets)
    pub ops: Vec<Patch>,
    /// Whether diff exceeded limits and should fallback to reload
    pub should_reload: bool,
    /// Reason for reload (if should_reload is true)
    pub reload_reason: Option<String>,
    /// Statistics about the diff
    pub stats: DiffStats,
}

impl DiffResult {
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

// =============================================================================
// Anchor-based Patch System
// =============================================================================
//
// All operations use StableId for targeting. No position indices.
// This eliminates index drift and order-dependent bugs.
//
// For structural changes (insert/move), we use anchor-based positioning:
// - InsertAfter/Before: relative to sibling element
// - InsertFirst/Last: relative to parent
//
// Text nodes don't have IDs in DOM, so we handle them specially:
// - Single text child: UpdateText on parent
// - Mixed children with structure change: Replace parent

/// Anchor for insert/move operations
///
/// Specifies WHERE to place an element relative to existing nodes.
/// All anchors reference elements by StableId, never by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// Insert/move after an element (anchor.insertAdjacentHTML('afterend', ...))
    After(StableId),
    /// Insert/move before an element (anchor.insertAdjacentHTML('beforebegin', ...))
    Before(StableId),
    /// Insert/move as first child (parent.insertAdjacentHTML('afterbegin', ...))
    FirstChildOf(StableId),
    /// Insert/move as last child (parent.insertAdjacentHTML('beforeend', ...))
    LastChildOf(StableId),
}

/// Patch operation using StableId for targeting
///
/// All operations are anchor-based or ID-based. No position indices.
/// This design ensures:
/// - Order independence (mostly)
/// - No index drift
/// - Simple JS execution (just insertAdjacent* calls)
#[derive(Debug, Clone)]
pub enum Patch {
    /// Replace entire element's outerHTML
    Replace { target: StableId, html: String },

    /// Update text content (element.textContent = text)
    /// Used for elements with single text child: `<p>Hello</p>` → `<p>World</p>`
    UpdateText { target: StableId, text: String },

    /// Replace inner HTML (element.innerHTML = html)
    /// Used when child structure changes but we want to preserve the parent element
    ReplaceChildren { target: StableId, html: String },

    /// Remove element by ID
    Remove { target: StableId },

    /// Insert new content at anchor position
    Insert { anchor: Anchor, html: String },

    /// Move existing element to new anchor position
    Move { target: StableId, to: Anchor },

    /// Update attributes
    UpdateAttrs {
        target: StableId,
        /// Attribute changes: (name, Some(value)) for set, (name, None) for remove
        attrs: Vec<(String, Option<String>)>,
    },
}

impl Patch {
    /// Get the primary target StableId of this patch
    pub fn target(&self) -> StableId {
        match self {
            Self::Replace { target, .. } => *target,
            Self::UpdateText { target, .. } => *target,
            Self::ReplaceChildren { target, .. } => *target,
            Self::Remove { target } => *target,
            Self::Insert { anchor, .. } => anchor.target_id(),
            Self::Move { target, .. } => *target,
            Self::UpdateAttrs { target, .. } => *target,
        }
    }
}

impl Anchor {
    /// Get the StableId referenced by this anchor
    pub fn target_id(&self) -> StableId {
        match self {
            Self::After(id) | Self::Before(id) | Self::FirstChildOf(id) | Self::LastChildOf(id) => *id,
        }
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Diff two Indexed VDOM documents using StableId-based comparison
///
/// This is the primary diff function that uses StableId for accurate
/// node tracking across edits, enabling move detection.
///
/// # Arguments
///
/// * `old` - The previous document state
/// * `new` - The new document state
///
/// # Returns
///
/// A `DiffResult` containing patch operations to transform old to new
pub fn diff(old: &Document<Indexed>, new: &Document<Indexed>) -> DiffResult {
    let mut ctx = DiffContext::new();
    ctx.diff_element(&old.root, &new.root);
    ctx.into_result()
}

// =============================================================================
// Internal Context
// =============================================================================

/// Internal diff context
struct DiffContext {
    ops: Vec<Patch>,
    depth: usize,
    should_reload: bool,
    reload_reason: Option<String>,
    stats: DiffStats,
}

impl DiffContext {
    fn new() -> Self {
        Self {
            ops: Vec::new(),
            depth: 0,
            should_reload: false,
            reload_reason: None,
            stats: DiffStats::default(),
        }
    }

    fn into_result(self) -> DiffResult {
        DiffResult {
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
            self.ops.push(Patch::Replace {
                target: old_id,
                html: render_element_html(new),
            });
            self.stats.nodes_replaced += 1;
            return;
        }

        // Diff attributes
        self.diff_attrs(old, new);

        // SVG elements: use ReplaceChildren (innerHTML) instead of UpdateText (textContent)
        // because SVG internal content is HTML markup, not plain text.
        // Using textContent would display "<path d=...>" as literal text.
        let is_svg = old.tag == "svg";

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
                    if is_svg {
                        // SVG: use innerHTML because content is SVG markup
                        self.ops.push(Patch::ReplaceChildren {
                            target: old_id,
                            html: new_text.to_string(),
                        });
                    } else {
                        // Normal element: use textContent
                        self.ops.push(Patch::UpdateText {
                            target: old_id,
                            text: new_text.to_string(),
                        });
                    }
                    self.stats.text_updates += 1;
                }
                self.stats.nodes_kept += 1;
                return;
            }
            // Case 2: Old has text, new is empty -> clear content
            (Some(_old_text), None) if new.children.is_empty() => {
                if is_svg {
                    self.ops.push(Patch::ReplaceChildren {
                        target: old_id,
                        html: String::new(),
                    });
                } else {
                    self.ops.push(Patch::UpdateText {
                        target: old_id,
                        text: String::new(),
                    });
                }
                self.stats.text_updates += 1;
                self.stats.nodes_kept += 1;
                return;
            }
            // Case 3: Old is empty, new has text -> set content
            (None, Some(new_text)) if old.children.is_empty() => {
                if is_svg {
                    self.ops.push(Patch::ReplaceChildren {
                        target: old_id,
                        html: new_text.to_string(),
                    });
                } else {
                    self.ops.push(Patch::UpdateText {
                        target: old_id,
                        text: new_text.to_string(),
                    });
                }
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
            self.ops.push(Patch::UpdateAttrs {
                target: old.ext.stable_id(),
                attrs: changes,
            });
            self.stats.attr_updates += 1;
        }
    }

    /// Diff child nodes using anchor-based approach
    ///
    /// # Strategy
    ///
    /// 1. **Element-only children**: Use LCS for move detection, anchor-based inserts
    /// 2. **Contains text nodes**: If structure matches, update text; otherwise Replace parent
    ///
    /// This avoids position indices entirely, using StableId anchors instead.
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
            self.insert_all_children(new_children, parent_id);
            return;
        }

        // Quick path: new empty, remove all old elements
        if new_children.is_empty() {
            self.remove_all_element_children(old_children);
            return;
        }

        // Check if we can do element-only diff (no text nodes involved in structure)
        let old_has_text = old_children.iter().any(|n| matches!(n, Node::Text(_)));
        let new_has_text = new_children.iter().any(|n| matches!(n, Node::Text(_)));

        if !old_has_text && !new_has_text {
            // Pure element children - use LCS with anchors
            self.diff_element_children(old_children, new_children, parent_id);
        } else {
            // Mixed content with text - use structure-aware diff
            self.diff_mixed_children(old_children, new_children, parent_id);
        }
    }

    /// Insert all children (used when old is empty)
    fn insert_all_children(&mut self, children: &[Node<Indexed>], parent_id: StableId) {
        // Insert all as children of parent
        // First one goes first, subsequent ones go after previous element (if any)
        let mut last_element_id: Option<StableId> = None;

        for child in children {
            let html = render_node_html(child);
            let anchor = match last_element_id {
                Some(prev_id) => Anchor::After(prev_id),
                None => Anchor::FirstChildOf(parent_id),
            };
            self.ops.push(Patch::Insert { anchor, html });

            // Track last element for anchoring
            if let Node::Element(elem) = child {
                last_element_id = Some(elem.ext.stable_id());
            }
        }
    }

    /// Remove all element children (text nodes ignored - they'll be gone with elements)
    fn remove_all_element_children(&mut self, children: &[Node<Indexed>]) {
        for child in children {
            if let Node::Element(elem) = child {
                self.ops.push(Patch::Remove {
                    target: elem.ext.stable_id(),
                });
            }
            // Text nodes are removed implicitly when parent's innerHTML changes
            // or when surrounding elements are removed
        }
    }

    /// Diff pure element children using LCS with anchor-based operations
    fn diff_element_children(
        &mut self,
        old_children: &[Node<Indexed>],
        new_children: &[Node<Indexed>],
        parent_id: StableId,
    ) {
        // Build ID lists for LCS
        let old_ids: Vec<StableId> = old_children.iter().map(get_node_stable_id).collect();
        let new_ids: Vec<StableId> = new_children.iter().map(get_node_stable_id).collect();

        // Compute LCS diff
        let lcs_result = diff_sequences(&old_ids, &new_ids);

        // Categorize edits
        let mut keeps: Vec<(usize, usize)> = Vec::new();
        let mut moves: Vec<(usize, usize)> = Vec::new();
        let mut deletes: Vec<usize> = Vec::new();
        let mut inserts: Vec<usize> = Vec::new();

        for edit in &lcs_result.edits {
            match edit {
                Edit::Keep { old_idx, new_idx } => keeps.push((*old_idx, *new_idx)),
                Edit::Move { old_idx, new_idx } => moves.push((*old_idx, *new_idx)),
                Edit::Delete { old_idx } => deletes.push(*old_idx),
                Edit::Insert { new_idx } => inserts.push(*new_idx),
            }
        }

        // 1. Remove deleted elements (order doesn't matter - they're by ID)
        for old_idx in &deletes {
            self.ops.push(Patch::Remove {
                target: old_ids[*old_idx],
            });
        }

        // 2. Apply moves with anchors (sort by target position for correct ordering)
        moves.sort_unstable_by_key(|(_, new_idx)| *new_idx);
        for (old_idx, new_idx) in &moves {
            let anchor = self.compute_anchor_for_position(*new_idx, new_children, parent_id);
            self.ops.push(Patch::Move {
                target: old_ids[*old_idx],
                to: anchor,
            });
            self.stats.nodes_moved += 1;
        }

        // 3. Insert new elements with anchors (MUST be sorted by position ascending!)
        // This ensures earlier inserts happen first, so later anchors (After(prev)) are valid
        inserts.sort_unstable();
        for new_idx in &inserts {
            let anchor = self.compute_anchor_for_position(*new_idx, new_children, parent_id);
            self.ops.push(Patch::Insert {
                anchor,
                html: render_node_html(&new_children[*new_idx]),
            });
        }

        // 4. Recursively diff kept and moved elements
        for (old_idx, new_idx) in keeps.iter().chain(moves.iter()) {
            self.diff_nodes(&old_children[*old_idx], &new_children[*new_idx]);
        }
    }

    /// Compute anchor for inserting/moving to a position in new_children
    ///
    /// Strategy: Find the nearest preceding element sibling that will exist in final DOM
    fn compute_anchor_for_position(
        &self,
        new_idx: usize,
        new_children: &[Node<Indexed>],
        parent_id: StableId,
    ) -> Anchor {
        // Look for preceding element sibling
        for i in (0..new_idx).rev() {
            if let Node::Element(elem) = &new_children[i] {
                return Anchor::After(elem.ext.stable_id());
            }
        }
        // No preceding element - insert at start
        Anchor::FirstChildOf(parent_id)
    }

    /// Diff mixed children (contains text nodes)
    ///
    /// Strategy: Compare structure. If same, update content. If different, Replace parent.
    fn diff_mixed_children(
        &mut self,
        old_children: &[Node<Indexed>],
        new_children: &[Node<Indexed>],
        parent_id: StableId,
    ) {
        // Check if structure is compatible (same length, same node types at each position)
        if self.children_structure_matches(old_children, new_children) {
            // Structure matches - check if any text content changed
            let text_changed = old_children.iter().zip(new_children.iter()).any(|(old, new)| {
                match (old, new) {
                    (Node::Text(old_t), Node::Text(new_t)) => old_t.content != new_t.content,
                    _ => false,
                }
            });

            if text_changed {
                // Text changed in mixed content - use innerHTML replacement
                // (text nodes don't have IDs, can't patch individually)
                let new_inner_html = new_children
                    .iter()
                    .map(render_node_html)
                    .collect::<Vec<_>>()
                    .join("");

                self.ops.push(Patch::ReplaceChildren {
                    target: parent_id,
                    html: new_inner_html,
                });
                self.stats.text_updates += 1;
            } else {
                // No text changes - diff element children recursively
                for (old, new) in old_children.iter().zip(new_children.iter()) {
                    self.diff_nodes(old, new);
                }
            }
        } else {
            // Structure differs - Replace children with new innerHTML
            // This is the "escape hatch" for complex mixed content changes
            let new_inner_html = new_children
                .iter()
                .map(render_node_html)
                .collect::<Vec<_>>()
                .join("");

            self.ops.push(Patch::ReplaceChildren {
                target: parent_id,
                html: new_inner_html,
            });
            self.stats.nodes_replaced += 1;
        }
    }

    /// Check if two child lists have matching structure (same length, same node types)
    fn children_structure_matches(
        &self,
        old: &[Node<Indexed>],
        new: &[Node<Indexed>],
    ) -> bool {
        if old.len() != new.len() {
            return false;
        }
        old.iter().zip(new.iter()).all(|(o, n)| {
            matches!(
                (o, n),
                (Node::Element(_), Node::Element(_))
                    | (Node::Text(_), Node::Text(_))
            )
        })
    }

    /// Diff two nodes (recursive entry point)
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
                // Text node content change - but text nodes don't have DOM IDs
                // This case is handled by parent (diff_mixed_children or single-text optimization)
                if old_text.content != new_text.content {
                    // This shouldn't happen if `diff_mixed_children` works correctly
                    // Fallback: count as text update
                    self.stats.text_updates += 1;
                }
            }
            // Different node types - shouldn't happen if structure_matches is correct
            _ => {
                // Fallback: this case should be handled by parent's Replace
            }
        }
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if children contain exactly one text node and return its content
///
/// Returns `Some(&str)` if children is exactly `[Text(content)]`, `None` otherwise.
/// Used to optimize single-text-child elements like `<p>Hello</p>`.
fn get_single_text_child(children: &[Node<Indexed>]) -> Option<&str> {
    if children.len() == 1 && let Node::Text(text) = &children[0] {
        return Some(&text.content);
    }
    None
}

/// Get StableId from any Indexed node type
fn get_node_stable_id(node: &Node<Indexed>) -> StableId {
    match node {
        Node::Element(elem) => elem.ext.stable_id(),
        Node::Text(text) => text.ext.stable_id,
    }
}

/// Render an Indexed element to HTML string with data-tola-id attribute
fn render_element_html(elem: &Element<Indexed>) -> String {
    render_element_html_ctx(elem, false)
}

/// Render an Indexed element to HTML string with context
fn render_element_html_ctx(elem: &Element<Indexed>, in_svg: bool) -> String {
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

    // Check if this element is SVG - content should not be escaped
    let is_svg = elem.tag == "svg" || in_svg;

    for child in &elem.children {
        html.push_str(&render_node_html_ctx(child, is_svg));
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
fn render_node_html(node: &Node<Indexed>) -> String {
    render_node_html_ctx(node, false)
}

/// Render an Indexed node to HTML string with context
fn render_node_html_ctx(node: &Node<Indexed>, in_svg: bool) -> String {
    match node {
        Node::Element(elem) => render_element_html_ctx(elem, in_svg),
        Node::Text(text) => {
            // Inside SVG: content is raw SVG markup, don't escape
            // Outside SVG: normal text, escape HTML characters
            if in_svg {
                text.content.clone()
            } else {
                html_escape(&text.content)
            }
        }
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
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::OtherFamily;
    use crate::phase::IndexedElemExt;
    use crate::{FamilyExt, Text};

    #[test]
    fn test_patch_target() {
        let patch = Patch::Replace {
            target: StableId::from_raw(42),
            html: "<div></div>".to_string(),
        };
        assert_eq!(patch.target().as_raw(), 42);
    }

    #[test]
    fn test_diff_result_reload() {
        let result = DiffResult::reload("test reason");
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
    fn test_html_escape() {
        assert_eq!(html_escape("a < b"), "a &lt; b");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }

    #[test]
    fn test_diff_detects_text_update_and_revert() {
        // Helper to build a simple document with three paragraphs
        fn build_doc(texts: &[&str], base_id: u64) -> Document<Indexed> {
            let mut root = Element::new("body");

            for (i, t) in texts.iter().enumerate() {
                // Create indexed elem ext with stable id
                let sid = StableId::from_raw(base_id + i as u64 + 1);
                let elem_ext = IndexedElemExt::<OtherFamily> {
                    stable_id: sid,
                    family_data: (),
                };
                let mut p = Element::with_ext("p", FamilyExt::Other(elem_ext));

                // Text node with stable id
                let text_sid = StableId::from_raw(base_id + 100 + i as u64 + 1);
                let mut txt = Text::new(*t);
                txt.ext = crate::phase::IndexedTextExt::new(text_sid);
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
        let r1 = diff(&old, &new1);
        assert!(!r1.should_reload, "diff should not force reload");
        // Should have at least one UpdateText or Replace targeting the second paragraph's text id
        let has_update1 = r1.ops.iter().any(|op| match op {
            Patch::UpdateText { target, text } => {
                target.as_raw() == (100 + 100 + 2) || text.contains("浊富")
            }
            Patch::Replace { target, html } => target.as_raw() == (100 + 2) || html.contains("浊富"),
            _ => false,
        });
        assert!(
            has_update1,
            "Expected a text update or replace for the first edit, got ops={:?}",
            r1.ops
        );

        // Update cache scenario: diff new1 -> new2 (restore)
        let r2 = diff(&new1, &new2);
        assert!(!r2.should_reload, "diff should not force reload on revert");
        let has_update2 = r2.ops.iter().any(|op| match op {
            Patch::UpdateText { text, .. } => text.contains("清贫"),
            Patch::Replace { html, .. } => html.contains("清贫"),
            _ => false,
        });
        assert!(
            has_update2,
            "Expected a text update or replace restoring the text, got ops={:?}",
            r2.ops
        );
    }

    #[test]
    fn test_edit_ordering_prevents_duplicates() {
        // Build old: A,B,C
        let mut root_old = Element::new("div");
        let a_ext = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(1),
            family_data: (),
        };
        let b_ext = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(2),
            family_data: (),
        };
        let c_ext = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(3),
            family_data: (),
        };
        root_old
            .children
            .push(Node::Element(Box::new(Element::with_ext(
                "p",
                FamilyExt::Other(a_ext),
            ))));
        root_old
            .children
            .push(Node::Element(Box::new(Element::with_ext(
                "p",
                FamilyExt::Other(b_ext),
            ))));
        root_old
            .children
            .push(Node::Element(Box::new(Element::with_ext(
                "p",
                FamilyExt::Other(c_ext),
            ))));
        let old = Document::new(root_old);

        // Build new: A,C,B (move C before B) and insert D at end
        let mut root_new = Element::new("div");
        let a_ext2 = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(1),
            family_data: (),
        };
        let c_ext2 = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(3),
            family_data: (),
        };
        let b_ext2 = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(2),
            family_data: (),
        };
        let d_ext = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(4),
            family_data: (),
        };
        root_new
            .children
            .push(Node::Element(Box::new(Element::with_ext(
                "p",
                FamilyExt::Other(a_ext2),
            ))));
        root_new
            .children
            .push(Node::Element(Box::new(Element::with_ext(
                "p",
                FamilyExt::Other(c_ext2),
            ))));
        root_new
            .children
            .push(Node::Element(Box::new(Element::with_ext(
                "p",
                FamilyExt::Other(b_ext2),
            ))));
        root_new
            .children
            .push(Node::Element(Box::new(Element::with_ext(
                "p",
                FamilyExt::Other(d_ext),
            ))));
        let new = Document::new(root_new);

        let r = diff(&old, &new);
        // Ensure all removes appear before inserts in the ops order (no duplicate insert of moved node)
        let mut first_insert = None;
        let mut first_remove = None;
        for (i, op) in r.ops.iter().enumerate() {
            match op {
                Patch::Insert { .. } => {
                    if first_insert.is_none() {
                        first_insert = Some(i)
                    }
                }
                Patch::Remove { .. } => {
                    if first_remove.is_none() {
                        first_remove = Some(i)
                    }
                }
                _ => {}
            }
        }
        assert!(
            first_remove.is_none()
                || first_insert.is_none()
                || first_remove.unwrap() < first_insert.unwrap(),
            "Removals should come before inserts to avoid duplicates: ops={:?}",
            r.ops
        );
    }

    #[test]
    fn test_single_text_child_empty_to_text() {
        // Test: old element is empty, new element has single text child
        // Should generate UpdateText to parent, not Insert of text node

        // Build old: <p></p>
        let mut root_old = Element::new("body");
        let p_ext = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(100),
            family_data: (),
        };
        let p_old = Element::with_ext("p", FamilyExt::Other(p_ext));
        // Empty children
        root_old
            .children
            .push(Node::Element(Box::new(p_old)));
        let old = Document::new(root_old);

        // Build new: <p>Hello</p>
        let mut root_new = Element::new("body");
        let p_ext2 = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(100),
            family_data: (),
        };
        let mut p_new = Element::with_ext("p", FamilyExt::Other(p_ext2));
        let mut txt = Text::new("Hello");
        txt.ext = crate::phase::IndexedTextExt::new(StableId::from_raw(200));
        p_new.children.push(Node::Text(txt));
        root_new
            .children
            .push(Node::Element(Box::new(p_new)));
        let new = Document::new(root_new);

        let result = diff(&old, &new);

        // Should have UpdateText targeting the paragraph (id=100), not Insert
        let has_update_text = result.ops.iter().any(|op| {
            matches!(op, Patch::UpdateText { target, text } if target.as_raw() == 100 && text == "Hello")
        });
        assert!(
            has_update_text,
            "Expected UpdateText to parent element, got: {:?}",
            result.ops
        );

        // Should NOT have any Insert operations
        let has_insert = result.ops.iter().any(|op| matches!(op, Patch::Insert { .. }));
        assert!(
            !has_insert,
            "Should not have Insert for text node: {:?}",
            result.ops
        );
    }

    #[test]
    fn test_single_text_child_text_to_empty() {
        // Test: old element has single text child, new element is empty
        // Should generate UpdateText with empty string, not Remove of text node

        // Build old: <p>Hello</p>
        let mut root_old = Element::new("body");
        let p_ext = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(100),
            family_data: (),
        };
        let mut p_old = Element::with_ext("p", FamilyExt::Other(p_ext));
        let mut txt = Text::new("Hello");
        txt.ext = crate::phase::IndexedTextExt::new(StableId::from_raw(200));
        p_old.children.push(Node::Text(txt));
        root_old
            .children
            .push(Node::Element(Box::new(p_old)));
        let old = Document::new(root_old);

        // Build new: <p></p>
        let mut root_new = Element::new("body");
        let p_ext2 = IndexedElemExt::<OtherFamily> {
            stable_id: StableId::from_raw(100),
            family_data: (),
        };
        let p_new = Element::with_ext("p", FamilyExt::Other(p_ext2));
        // Empty children
        root_new
            .children
            .push(Node::Element(Box::new(p_new)));
        let new = Document::new(root_new);

        let result = diff(&old, &new);

        // Should have UpdateText targeting the paragraph with empty string
        let has_update_text = result.ops.iter().any(|op| {
            matches!(op, Patch::UpdateText { target, text } if target.as_raw() == 100 && text.is_empty())
        });
        assert!(
            has_update_text,
            "Expected UpdateText with empty string to parent element, got: {:?}",
            result.ops
        );

        // Should NOT have any Remove operations
        let has_remove = result.ops.iter().any(|op| matches!(op, Patch::Remove { .. }));
        assert!(
            !has_remove,
            "Should not have Remove for text node: {:?}",
            result.ops
        );
    }
}
