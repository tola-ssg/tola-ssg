//! VDOM Diff Algorithm (Re-exports)
//!
//! The diff algorithm has been migrated to `crate::vdom::diff`.
//! This module now only provides re-exports for backward compatibility.
//!
//! # Migration Notes
//!
//! - Use `crate::vdom::diff::diff` instead of `diff_indexed_documents`
//! - Use `crate::vdom::diff::Patch` instead of `StableIdPatch`
//! - Use `crate::vdom::diff::DiffResult` instead of `IndexedDiffResult`
//!
//! The old `Processed`-phase diff has been removed. All hot reload
//! now uses `Indexed`-phase VDOM with `StableId`-based targeting.

// Re-export all types from vdom::diff for backward compatibility
pub use crate::vdom::diff::{
    diff as diff_indexed_documents,
    DiffResult as IndexedDiffResult,
    DiffStats,
    Patch as StableIdPatch,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdom::diff::Patch;

    #[test]
    fn test_reexports_work() {
        // Verify that the type aliases work
        let _stats = DiffStats::default();

        // Verify IndexedDiffResult works
        let result = IndexedDiffResult::reload("test");
        assert!(result.should_reload);

        // Verify StableIdPatch alias
        let patch: StableIdPatch = Patch::Remove {
            target: crate::vdom::id::StableId::from_raw(1),
        };
        assert_eq!(patch.target().as_raw(), 1);
    }
}
