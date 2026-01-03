//! LCS (Longest Common Subsequence) algorithm for VDOM diffing.
//!
//! This module provides an efficient LCS implementation used by the
//! diff algorithm to detect moved elements.

/// LCS result containing indices of matching elements.
#[derive(Debug, Clone)]
pub struct LcsResult {
    /// Indices from the first sequence that are part of LCS.
    pub old_indices: Vec<usize>,
    /// Indices from the second sequence that are part of LCS.
    pub new_indices: Vec<usize>,
}

/// Compute LCS of two sequences based on a key function.
///
/// Returns indices of matching elements in both sequences.
pub fn lcs_by_key<T, K, F>(old: &[T], new: &[T], key_fn: F) -> LcsResult
where
    K: Eq + std::hash::Hash,
    F: Fn(&T) -> K,
{
    let n = old.len();
    let m = new.len();

    if n == 0 || m == 0 {
        return LcsResult {
            old_indices: Vec::new(),
            new_indices: Vec::new(),
        };
    }

    // Build DP table
    let mut dp = vec![vec![0usize; m + 1]; n + 1];

    for i in 1..=n {
        for j in 1..=m {
            if key_fn(&old[i - 1]) == key_fn(&new[j - 1]) {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to find LCS indices
    let mut old_indices = Vec::with_capacity(dp[n][m]);
    let mut new_indices = Vec::with_capacity(dp[n][m]);

    let mut i = n;
    let mut j = m;

    while i > 0 && j > 0 {
        if key_fn(&old[i - 1]) == key_fn(&new[j - 1]) {
            old_indices.push(i - 1);
            new_indices.push(j - 1);
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    old_indices.reverse();
    new_indices.reverse();

    LcsResult {
        old_indices,
        new_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lcs_simple() {
        let old = vec![1, 2, 3, 4, 5];
        let new = vec![2, 3, 5, 6];

        let result = lcs_by_key(&old, &new, |x| *x);

        assert_eq!(result.old_indices, vec![1, 2, 4]); // indices of 2, 3, 5 in old
        assert_eq!(result.new_indices, vec![0, 1, 2]); // indices of 2, 3, 5 in new
    }

    #[test]
    fn test_lcs_empty() {
        let old: Vec<i32> = vec![];
        let new = vec![1, 2, 3];

        let result = lcs_by_key(&old, &new, |x| *x);
        assert!(result.old_indices.is_empty());
        assert!(result.new_indices.is_empty());
    }

    #[test]
    fn test_lcs_identical() {
        let old = vec![1, 2, 3];
        let new = vec![1, 2, 3];

        let result = lcs_by_key(&old, &new, |x| *x);
        assert_eq!(result.old_indices, vec![0, 1, 2]);
        assert_eq!(result.new_indices, vec![0, 1, 2]);
    }
}
