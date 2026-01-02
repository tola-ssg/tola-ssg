//! VDOM Cache for Hot Reload
//!
//! Caches Indexed VDOM documents keyed by source file path.
//! Used to compare old and new compilations for diff generation.
//!
//! # Thread Safety
//!
//! The cache uses `RwLock` for concurrent access from the watcher thread.

use crate::vdom::{Document, Indexed};
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Global VDOM cache for hot reload
pub static VDOM_CACHE: LazyLock<VdomCache> = LazyLock::new(VdomCache::new);

/// Cache for Indexed VDOM documents
pub struct VdomCache {
    inner: RwLock<FxHashMap<PathBuf, Document<Indexed>>>,
}

impl VdomCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(FxHashMap::default()),
        }
    }

    /// Get a cached VDOM for a path
    pub fn get(&self, path: &Path) -> Option<Document<Indexed>> {
        self.inner.read().get(path).cloned()
    }

    /// Insert or update a VDOM in the cache
    pub fn insert(&self, path: PathBuf, vdom: Document<Indexed>) {
        self.inner.write().insert(path, vdom);
    }

    /// Remove a VDOM from the cache
    pub fn remove(&self, path: &Path) -> Option<Document<Indexed>> {
        self.inner.write().remove(path)
    }

    /// Clear the entire cache
    pub fn clear(&self) {
        self.inner.write().clear();
    }

    /// Get the number of cached VDOMs
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    /// Check if cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }
}

impl Default for VdomCache {
    fn default() -> Self {
        Self::new()
    }
}
