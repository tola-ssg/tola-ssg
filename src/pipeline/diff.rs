//! Diff Pipeline - VDOM Diffing and Patch Generation
//!
//! Pure functions for computing diffs between VDOM documents.
//! No Actor machinery, no global state.

use crate::vdom::VdomCache;
use crate::vdom::diff::{diff, DiffResult as VdomDiffResult, Patch};
use crate::vdom::{Document, Indexed};

/// Normalize URL path for consistent cache keys.
///
/// Ensures:
/// - Always starts with `/`
/// - No trailing slash (except for root `/`)
/// - No double slashes
/// - No fragment (#...) or query string (?...)
fn normalize_url_path(url_path: &str) -> String {
    // Remove fragment (#...) and query string (?...)
    let path = url_path
        .split('#').next().unwrap_or(url_path)
        .split('?').next().unwrap_or(url_path);

    let mut path = path.to_string();

    // Collapse multiple slashes
    while path.contains("//") {
        path = path.replace("//", "/");
    }

    // Ensure starts with /
    if !path.starts_with('/') {
        path = format!("/{}", path);
    }

    // Remove trailing slash (except for root)
    if path.len() > 1 && path.ends_with('/') {
        path.pop();
    }

    path
}

/// Outcome of diff computation
#[derive(Debug)]
pub enum DiffOutcome {
    /// First time seeing this page, no diff possible
    Initial,
    /// No changes detected
    Unchanged,
    /// Patches to apply (includes new VDOM for cache update after broadcast)
    Patches(Vec<Patch>, Document<Indexed>),
    /// Structural change requires full reload
    NeedsReload { reason: String },
}

/// Compute diff between new VDOM and cached version
///
/// This function:
/// 1. Looks up the old VDOM from cache
/// 2. Computes diff if old exists
/// 3. Returns appropriate outcome (caller updates cache after successful broadcast)
///
/// Note: Cache is NOT updated here. Caller must update cache after
/// successfully sending patches to browser, to keep cache in sync with
/// what the browser actually displays.
pub fn compute_diff(
    cache: &mut VdomCache,
    url_path: &str,
    new_vdom: Document<Indexed>,
) -> DiffOutcome {
    // Normalize url_path for consistent cache keys
    let url_path = normalize_url_path(url_path);

    crate::log!("diff"; "computing diff for {} (cache size: {})", url_path, cache.len());

    if let Some(old_vdom) = cache.get(&url_path) {
        crate::log!("diff"; "found cached vdom for {}", url_path);
        let diff_result: VdomDiffResult = diff(old_vdom, &new_vdom);

        if diff_result.should_reload {
            // For reload, update cache immediately since browser will refresh
            cache.insert(url_path.to_string(), new_vdom);
            crate::log!("diff"; "needs reload: {:?}", diff_result.reload_reason);
            DiffOutcome::NeedsReload {
                reason: diff_result
                    .reload_reason
                    .unwrap_or_else(|| "complex change".to_string()),
            }
        } else if diff_result.ops.is_empty() {
            // No changes - update cache (content is same, safe to update)
            cache.insert(url_path.to_string(), new_vdom);
            crate::log!("diff"; "unchanged");
            DiffOutcome::Unchanged
        } else {
            // Return patches WITH new_vdom - caller updates cache after broadcast
            crate::log!("diff"; "patches: {} ops", diff_result.ops.len());
            DiffOutcome::Patches(diff_result.ops, new_vdom)
        }
    } else {
        // Initial - update cache since browser will reload
        crate::log!("diff"; "no cache for {}, inserting initial", url_path);
        cache.insert(url_path.to_string(), new_vdom);
        DiffOutcome::Initial
    }
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
            DiffOutcome::Patches(diff_result.ops, new_vdom.clone())
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
