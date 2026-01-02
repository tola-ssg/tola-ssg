//! Global freshness cache for avoiding redundant hash computations.
//!
//! The cache stores file content hashes keyed by canonical path.
//! It is cleared at the start of each build to ensure fresh data.

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use super::ContentHash;

// =============================================================================
// Cache Types
// =============================================================================

/// Global cache for file content hashes.
///
/// Thread-safe via `RwLock` for concurrent access during parallel builds.
pub struct FreshnessCache {
    /// Map from canonical file path to content hash.
    hashes: RwLock<FxHashMap<PathBuf, ContentHash>>,
}

impl FreshnessCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            hashes: RwLock::new(FxHashMap::default()),
        }
    }

    /// Get cached hash for a file path.
    ///
    /// Returns `None` if not cached or path cannot be canonicalized.
    pub fn get(&self, path: &Path) -> Option<ContentHash> {
        let canonical = path.canonicalize().ok()?;
        self.hashes.read().get(&canonical).copied()
    }

    /// Store hash in cache.
    ///
    /// Path is canonicalized before storage for consistent lookup.
    pub fn set(&self, path: &Path, hash: ContentHash) {
        if let Ok(canonical) = path.canonicalize() {
            self.hashes.write().insert(canonical, hash);
        }
    }

    /// Remove a specific path from cache.
    ///
    /// Used when a file is known to have changed (e.g., from file watcher).
    #[allow(dead_code)]
    pub fn invalidate(&self, path: &Path) {
        if let Ok(canonical) = path.canonicalize() {
            self.hashes.write().remove(&canonical);
        }
    }

    /// Clear all cached hashes.
    ///
    /// Called at the start of each build to ensure fresh data.
    pub fn clear(&self) {
        self.hashes.write().clear();
    }

    /// Get number of cached entries (for debugging/stats).
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.hashes.read().len()
    }
}

impl Default for FreshnessCache {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Global Instance
// =============================================================================

/// Global freshness cache instance.
///
/// Shared across all compilation threads within a single build.
pub static FRESHNESS_CACHE: LazyLock<FreshnessCache> = LazyLock::new(FreshnessCache::new);

// =============================================================================
// Convenience Functions
// =============================================================================

/// Get cached hash for a file.
#[inline]
pub fn get_cached_hash(path: &Path) -> Option<ContentHash> {
    FRESHNESS_CACHE.get(path)
}

/// Store hash in global cache.
#[inline]
pub fn set_cached_hash(path: &Path, hash: ContentHash) {
    FRESHNESS_CACHE.set(path, hash);
}

/// Clear the global freshness cache.
///
/// Call at the start of each build cycle.
#[inline]
pub fn clear_cache() {
    FRESHNESS_CACHE.clear();
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_cache_get_set() {
        let cache = FreshnessCache::new();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "content").unwrap();

        let hash = ContentHash::new([1; 32]);
        cache.set(&path, hash);

        assert_eq!(cache.get(&path), Some(hash));
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = FreshnessCache::new();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "content").unwrap();

        let hash = ContentHash::new([1; 32]);
        cache.set(&path, hash);
        cache.invalidate(&path);

        assert_eq!(cache.get(&path), None);
    }

    #[test]
    fn test_cache_clear() {
        let cache = FreshnessCache::new();
        let dir = TempDir::new().unwrap();

        let path1 = dir.path().join("a.txt");
        let path2 = dir.path().join("b.txt");
        fs::write(&path1, "a").unwrap();
        fs::write(&path2, "b").unwrap();

        cache.set(&path1, ContentHash::new([1; 32]));
        cache.set(&path2, ContentHash::new([2; 32]));
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert_eq!(cache.len(), 0);
    }
}
