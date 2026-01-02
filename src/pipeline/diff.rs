//! Diff Pipeline - VDOM Diffing and Patch Generation
//!
//! Pure functions for computing diffs between VDOM documents.
//! No Actor machinery, no global state.

use crate::pipeline::cache::VdomCache;
use crate::vdom::diff::{diff, DiffResult as VdomDiffResult, Patch};
use crate::vdom::{Document, Indexed};

/// Outcome of diff computation
#[derive(Debug)]
pub enum DiffOutcome {
    /// First time seeing this page, no diff possible
    Initial,
    /// No changes detected
    Unchanged,
    /// Patches to apply
    Patches(Vec<Patch>),
    /// Structural change requires full reload
    NeedsReload { reason: String },
}

/// Compute diff between new VDOM and cached version
///
/// This function:
/// 1. Looks up the old VDOM from cache
/// 2. Computes diff if old exists
/// 3. Updates cache with new VDOM
/// 4. Returns appropriate outcome
pub fn compute_diff(
    cache: &mut VdomCache,
    url_path: &str,
    new_vdom: Document<Indexed>,
) -> DiffOutcome {
    let outcome = if let Some(old_vdom) = cache.get(url_path) {
        let diff_result: VdomDiffResult = diff(old_vdom, &new_vdom);

        if diff_result.should_reload {
            DiffOutcome::NeedsReload {
                reason: diff_result
                    .reload_reason
                    .unwrap_or_else(|| "complex change".to_string()),
            }
        } else if diff_result.ops.is_empty() {
            DiffOutcome::Unchanged
        } else {
            DiffOutcome::Patches(diff_result.ops)
        }
    } else {
        DiffOutcome::Initial
    };

    // Always update cache with new VDOM
    cache.insert(url_path.to_string(), new_vdom);

    outcome
}

/// Compute diff without updating cache (for testing)
#[allow(dead_code)]
pub fn compute_diff_readonly(
    cache: &VdomCache,
    url_path: &str,
    new_vdom: &Document<Indexed>,
) -> DiffOutcome {
    if let Some(old_vdom) = cache.get(url_path) {
        let diff_result: VdomDiffResult = diff(old_vdom, new_vdom);

        if diff_result.should_reload {
            DiffOutcome::NeedsReload {
                reason: diff_result
                    .reload_reason
                    .unwrap_or_else(|| "complex change".to_string()),
            }
        } else if diff_result.ops.is_empty() {
            DiffOutcome::Unchanged
        } else {
            DiffOutcome::Patches(diff_result.ops)
        }
    } else {
        DiffOutcome::Initial
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdom::Element;

    #[test]
    fn test_diff_outcome_variants() {
        let _ = DiffOutcome::Initial;
        let _ = DiffOutcome::Unchanged;
        let _ = DiffOutcome::NeedsReload {
            reason: "test".to_string(),
        };
    }

    #[test]
    fn test_empty_cache_returns_initial() {
        let cache = VdomCache::new();
        let root: Element<Indexed> = Element::new("html");
        let doc = Document::new(root);
        let outcome = compute_diff_readonly(&cache, "/test", &doc);
        assert!(matches!(outcome, DiffOutcome::Initial));
    }
}
