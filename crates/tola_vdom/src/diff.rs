//! VDOM diffing algorithm.
//!
//! Computes the minimal set of patches needed to transform one document
//! into another, enabling efficient hot reload updates.

use crate::id::StableId;

/// Result of diffing two documents.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// List of patches to apply.
    pub patches: Vec<Patch>,
    /// Statistics about the diff.
    pub stats: DiffStats,
}

/// A single patch operation.
#[derive(Debug, Clone)]
pub enum Patch {
    /// Replace an element entirely.
    Replace {
        /// ID of element to replace.
        target: StableId,
        /// New HTML content.
        html: String,
    },
    /// Update element attributes.
    UpdateAttrs {
        /// ID of target element.
        target: StableId,
        /// Attributes to set.
        set: Vec<(String, String)>,
        /// Attributes to remove.
        remove: Vec<String>,
    },
    /// Update text content.
    UpdateText {
        /// ID of target text node.
        target: StableId,
        /// New text content.
        text: String,
    },
    /// Insert a new element.
    Insert {
        /// ID of parent element.
        parent: StableId,
        /// Index at which to insert.
        index: usize,
        /// HTML content to insert.
        html: String,
    },
    /// Remove an element.
    Remove {
        /// ID of element to remove.
        target: StableId,
    },
    /// Move an element to a new position.
    Move {
        /// ID of element to move.
        target: StableId,
        /// New parent ID.
        new_parent: StableId,
        /// New index within parent.
        new_index: usize,
    },
}

/// Statistics about a diff operation.
#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    /// Number of elements kept unchanged.
    pub kept: usize,
    /// Number of elements replaced.
    pub replaced: usize,
    /// Number of elements inserted.
    pub inserted: usize,
    /// Number of elements removed.
    pub removed: usize,
    /// Number of elements moved.
    pub moved: usize,
    /// Number of attribute updates.
    pub attr_updates: usize,
    /// Number of text updates.
    pub text_updates: usize,
}

impl DiffStats {
    /// Total number of patch operations.
    pub fn total_patches(&self) -> usize {
        self.replaced + self.inserted + self.removed + self.moved + self.attr_updates + self.text_updates
    }

    /// Check if documents are identical (no patches needed).
    pub fn is_identical(&self) -> bool {
        self.total_patches() == 0
    }
}

/// Compute the diff between two documents.
///
/// This is a placeholder implementation. The full implementation
/// should use LCS-based diffing with StableId matching.
pub fn diff<D>(_old: &D, _new: &D) -> DiffResult {
    // TODO: Implement full diffing algorithm
    DiffResult {
        patches: Vec::new(),
        stats: DiffStats::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_stats() {
        let stats = DiffStats {
            kept: 10,
            replaced: 2,
            inserted: 1,
            removed: 1,
            moved: 0,
            attr_updates: 3,
            text_updates: 2,
        };

        assert_eq!(stats.total_patches(), 9);
        assert!(!stats.is_identical());
    }

    #[test]
    fn test_identical_stats() {
        let stats = DiffStats {
            kept: 10,
            ..Default::default()
        };

        assert!(stats.is_identical());
    }
}
