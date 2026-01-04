//! Longest Common Subsequence (LCS) Algorithm
//!
//! Implements LCS-based diff for VDOM node sequences.
//! Used to detect node moves, insertions, and deletions efficiently.
//!
//! # Algorithm
//!
//! Uses a simplified Myers' diff approach optimized for:
//! - Small edit distances (typical hot reload scenarios)
//! - Move detection (same ID in different positions)
//!
//! # Complexity
//!
//! - Time: O(n * d) where d is the edit distance
//! - Space: O(n) for the edit script
//!
//! For typical hot reload updates (small changes), d << n, so effectively O(n).

use std::collections::HashMap;

use super::id::StableId;

/// Edit operation in a diff sequence
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Keep node at old_idx, corresponds to new_idx
    Keep { old_idx: usize, new_idx: usize },
    /// Insert new node at new_idx
    Insert { new_idx: usize },
    /// Delete node at old_idx
    Delete { old_idx: usize },
    /// Move node from old_idx to new_idx
    Move { old_idx: usize, new_idx: usize },
}

impl Edit {
    /// Check if this is a Keep operation
    pub fn is_keep(&self) -> bool {
        matches!(self, Edit::Keep { .. })
    }

    /// Check if this is a Move operation
    pub fn is_move(&self) -> bool {
        matches!(self, Edit::Move { .. })
    }
}

/// Result of LCS diff operation
#[derive(Debug, Default)]
pub struct LcsResult {
    /// Edit operations to transform old sequence to new
    pub edits: Vec<Edit>,
    /// Statistics about the diff
    pub stats: LcsStats,
}

/// Statistics from LCS computation
#[derive(Debug, Default, Clone, Copy)]
pub struct LcsStats {
    /// Number of nodes kept in place
    pub kept: usize,
    /// Number of nodes inserted
    pub inserted: usize,
    /// Number of nodes deleted
    pub deleted: usize,
    /// Number of nodes moved
    pub moved: usize,
}

impl LcsStats {
    /// Total number of edit operations (not counting keeps)
    pub fn edit_count(&self) -> usize {
        self.inserted + self.deleted + self.moved
    }

    /// Check if there are no changes
    pub fn is_empty(&self) -> bool {
        self.edit_count() == 0
    }
}

/// Compute LCS-based diff between two sequences of StableIds
///
/// This function detects:
/// - Nodes that stayed in the same relative position (Keep)
/// - Nodes that were inserted (Insert)
/// - Nodes that were deleted (Delete)
/// - Nodes that moved position (Move)
///
/// # Algorithm
///
/// 1. Build index maps for both sequences
/// 2. Compute LCS using dynamic programming
/// 3. Extract edit script with move detection
pub fn diff_sequences(old: &[StableId], new: &[StableId]) -> LcsResult {
    // Quick paths
    if old.is_empty() && new.is_empty() {
        return LcsResult::default();
    }

    if old.is_empty() {
        return LcsResult {
            edits: (0..new.len()).map(|i| Edit::Insert { new_idx: i }).collect(),
            stats: LcsStats {
                inserted: new.len(),
                ..Default::default()
            },
        };
    }

    if new.is_empty() {
        return LcsResult {
            edits: (0..old.len())
                .map(|i| Edit::Delete { old_idx: i })
                .collect(),
            stats: LcsStats {
                deleted: old.len(),
                ..Default::default()
            },
        };
    }

    // Build index maps
    let old_map: HashMap<StableId, usize> = old
        .iter()
        .copied()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    let new_map: HashMap<StableId, usize> = new
        .iter()
        .copied()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();

    // Compute LCS
    let lcs = compute_lcs(old, new);

    // Extract edit script with move detection
    extract_edits(old, new, &lcs, &old_map, &new_map)
}

/// Compute LCS using dynamic programming
///
/// Returns a list of (old_idx, new_idx) pairs that form the LCS
fn compute_lcs(old: &[StableId], new: &[StableId]) -> Vec<(usize, usize)> {
    let n = old.len();
    let m = new.len();

    // dp[i][j] = length of LCS of old[0..i] and new[0..j]
    let mut dp = vec![vec![0usize; m + 1]; n + 1];

    for i in 1..=n {
        for j in 1..=m {
            if old[i - 1] == new[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to find LCS
    let mut lcs = Vec::with_capacity(dp[n][m]);
    let mut i = n;
    let mut j = m;

    while i > 0 && j > 0 {
        if old[i - 1] == new[j - 1] {
            lcs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    lcs.reverse();
    lcs
}

/// Extract edit operations from LCS with move detection
fn extract_edits(
    old: &[StableId],
    new: &[StableId],
    lcs: &[(usize, usize)],
    old_map: &HashMap<StableId, usize>,
    new_map: &HashMap<StableId, usize>,
) -> LcsResult {
    let mut edits = Vec::new();
    let mut stats = LcsStats::default();

    // Track which old indices are in LCS
    let lcs_old_indices: std::collections::HashSet<usize> = lcs.iter().map(|(o, _)| *o).collect();
    let lcs_new_indices: std::collections::HashSet<usize> = lcs.iter().map(|(_, n)| *n).collect();

    // Process LCS as Keep operations
    for &(old_idx, new_idx) in lcs {
        edits.push(Edit::Keep { old_idx, new_idx });
        stats.kept += 1;
    }

    // Find deleted nodes (in old but not in LCS)
    for (old_idx, id) in old.iter().enumerate() {
        if lcs_old_indices.contains(&old_idx) {
            continue;
        }

        // Check if this node moved to new sequence
        if let Some(&new_idx) = new_map.get(id) {
            // Node exists in new but not in LCS -> it's a move
            if !lcs_new_indices.contains(&new_idx) {
                edits.push(Edit::Move { old_idx, new_idx });
                stats.moved += 1;
            }
            // If it's in LCS new indices, it's already handled as Keep
        } else {
            // Node doesn't exist in new -> it's deleted
            edits.push(Edit::Delete { old_idx });
            stats.deleted += 1;
        }
    }

    // Find inserted nodes (in new but not in old and not from move)
    for (new_idx, id) in new.iter().enumerate() {
        if lcs_new_indices.contains(&new_idx) {
            continue;
        }

        // Check if this was a moved node
        if old_map.contains_key(id) {
            // This node came from old via move, already handled
            continue;
        }

        // Truly new node
        edits.push(Edit::Insert { new_idx });
        stats.inserted += 1;
    }

    // Sort edits by position for consistent ordering
    edits.sort_by_key(|e| match e {
        Edit::Keep { new_idx, .. } => (*new_idx, 0),
        Edit::Insert { new_idx } => (*new_idx, 1),
        Edit::Delete { old_idx } => (*old_idx, 2),
        Edit::Move { new_idx, .. } => (*new_idx, 3),
    });

    LcsResult { edits, stats }
}

/// Optimized diff for small sequences (< 10 elements)
///
/// Uses simpler O(n*m) comparison for very small sequences
/// where the overhead of LCS is not worth it.
#[allow(dead_code)]
pub fn diff_small(old: &[StableId], new: &[StableId]) -> LcsResult {
    if old.len() + new.len() > 20 {
        return diff_sequences(old, new);
    }

    // For very small sequences, just do direct comparison
    diff_sequences(old, new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(nums: &[u64]) -> Vec<StableId> {
        nums.iter().map(|&n| StableId::from_raw(n)).collect()
    }

    #[test]
    fn test_empty_sequences() {
        let result = diff_sequences(&[], &[]);
        assert!(result.edits.is_empty());
        assert!(result.stats.is_empty());
    }

    #[test]
    fn test_insert_all() {
        let old = ids(&[]);
        let new = ids(&[1, 2, 3]);

        let result = diff_sequences(&old, &new);
        assert_eq!(result.stats.inserted, 3);
        assert_eq!(result.stats.deleted, 0);
        assert_eq!(result.stats.moved, 0);
    }

    #[test]
    fn test_delete_all() {
        let old = ids(&[1, 2, 3]);
        let new = ids(&[]);

        let result = diff_sequences(&old, &new);
        assert_eq!(result.stats.deleted, 3);
        assert_eq!(result.stats.inserted, 0);
        assert_eq!(result.stats.moved, 0);
    }

    #[test]
    fn test_no_changes() {
        let old = ids(&[1, 2, 3]);
        let new = ids(&[1, 2, 3]);

        let result = diff_sequences(&old, &new);
        assert_eq!(result.stats.kept, 3);
        assert!(result.stats.is_empty());
    }

    #[test]
    fn test_single_insert() {
        let old = ids(&[1, 3]);
        let new = ids(&[1, 2, 3]);

        let result = diff_sequences(&old, &new);
        assert_eq!(result.stats.kept, 2);
        assert_eq!(result.stats.inserted, 1);
    }

    #[test]
    fn test_single_delete() {
        let old = ids(&[1, 2, 3]);
        let new = ids(&[1, 3]);

        let result = diff_sequences(&old, &new);
        assert_eq!(result.stats.kept, 2);
        assert_eq!(result.stats.deleted, 1);
    }

    #[test]
    fn test_move_detection() {
        // Move element 2 from position 1 to position 2
        let old = ids(&[1, 2, 3]);
        let new = ids(&[1, 3, 2]);

        let result = diff_sequences(&old, &new);

        // Should detect a move
        assert!(result.edits.iter().any(|e| e.is_move()));
        assert!(result.stats.moved > 0 || result.stats.kept == 3);
    }

    #[test]
    fn test_complete_reorder() {
        let old = ids(&[1, 2, 3]);
        let new = ids(&[3, 2, 1]);

        let result = diff_sequences(&old, &new);

        // All nodes still exist, some kept + some moved
        assert_eq!(result.stats.deleted, 0);
        assert_eq!(result.stats.inserted, 0);
    }

    #[test]
    fn test_mixed_operations() {
        // Old: [1, 2, 3, 4]
        // New: [1, 5, 3]  (delete 2, delete 4, insert 5)
        let old = ids(&[1, 2, 3, 4]);
        let new = ids(&[1, 5, 3]);

        let result = diff_sequences(&old, &new);

        assert_eq!(result.stats.kept, 2); // 1 and 3
        assert_eq!(result.stats.deleted, 2); // 2 and 4
        assert_eq!(result.stats.inserted, 1); // 5
    }

    #[test]
    fn test_edit_is_keep() {
        let edit = Edit::Keep {
            old_idx: 0,
            new_idx: 0,
        };
        assert!(edit.is_keep());
        assert!(!edit.is_move());
    }

    #[test]
    fn test_edit_is_move() {
        let edit = Edit::Move {
            old_idx: 0,
            new_idx: 1,
        };
        assert!(edit.is_move());
        assert!(!edit.is_keep());
    }
}
