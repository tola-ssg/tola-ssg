//! VDOM Cache - In-memory storage for diffing
//!
//! This replaces the global `VDOM_CACHE` static. Each consumer
//! (actor or watch.rs) owns its cache instance.

use rustc_hash::FxHashMap;

use crate::vdom::{Document, Indexed};

/// VDOM cache - stores previous VDOM state for diffing
#[derive(Debug, Default)]
pub struct VdomCache {
    /// Maps URL path to cached VDOM document
    pages: FxHashMap<String, Document<Indexed>>,
}

impl VdomCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Get cached VDOM for a URL path
    pub fn get(&self, url_path: &str) -> Option<&Document<Indexed>> {
        self.pages.get(url_path)
    }

    /// Insert or update cached VDOM, returns the old value if any
    pub fn insert(
        &mut self,
        url_path: String,
        doc: Document<Indexed>,
    ) -> Option<Document<Indexed>> {
        self.pages.insert(url_path, doc)
    }

    /// Remove a page from cache
    pub fn remove(&mut self, url_path: &str) -> Option<Document<Indexed>> {
        self.pages.remove(url_path)
    }

    /// Clear all cached pages
    pub fn clear(&mut self) {
        self.pages.clear();
    }

    /// Number of cached pages
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Get all cached URL paths
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.pages.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_empty() {
        let cache = VdomCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.get("/test").is_none());
    }
}
