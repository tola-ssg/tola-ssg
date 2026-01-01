//! LCS Algorithm (Re-exports)
//!
//! The LCS algorithm has been migrated to `crate::vdom::lcs`.
//! This module now only provides re-exports for backward compatibility.

// Re-export all types from vdom::lcs for backward compatibility
pub use crate::vdom::lcs::{
    diff_sequences,
    Edit,
    LcsResult,
    LcsStats,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdom::id::StableId;

    #[test]
    fn test_reexports_work() {
        let old = vec![StableId::from_raw(1), StableId::from_raw(2)];
        let new = vec![StableId::from_raw(1), StableId::from_raw(3)];
        let result = diff_sequences(&old, &new);
        assert!(!result.edits.is_empty());
    }
}
