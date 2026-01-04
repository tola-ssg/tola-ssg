//! VDOM Cache - In-memory storage for diffing
//!
//! Stores `Document<Indexed>` keyed by URL path (e.g., "/blog/hello").
//! Each consumer (watch.rs, actor) owns its own cache instance.
//!
//! # Type Safety
//!
//! The [`CacheKey`] type ensures all cache keys are normalized consistently.
//! This prevents bugs from trailing slashes or other URL variations.

use std::borrow::Borrow;
use std::fmt;
use std::hash::Hash;

use rustc_hash::FxHashMap;

use super::{Document, Indexed};

// =============================================================================
// CacheKey - Type-safe normalized URL path
// =============================================================================

/// A normalized URL path used as cache key.
///
/// This type guarantees that the URL is normalized (no trailing slash,
/// starts with `/`, etc.) at construction time, preventing cache key
/// mismatch bugs.
///
/// # Example
///
/// ```ignore
/// let key1 = CacheKey::new("/blog/post/");
/// let key2 = CacheKey::new("/blog/post");
/// assert_eq!(key1, key2); // Both normalize to "/blog/post"
/// ```
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CacheKey(String);

impl CacheKey {
    /// Create a new cache key from a URL path.
    ///
    /// The URL is automatically normalized:
    /// - Removes trailing slash (except root)
    /// - Ensures starts with `/`
    /// - Removes query string and fragment
    /// - Collapses multiple slashes
    pub fn new(url_path: impl AsRef<str>) -> Self {
        Self(Self::normalize(url_path.as_ref()))
    }

    /// Normalize a URL path for consistent cache keys.
    fn normalize(url: &str) -> String {
        // Remove fragment and query string
        let path = url
            .split('#').next().unwrap_or(url)
            .split('?').next().unwrap_or(url);

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

    /// Get the normalized URL path as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert into the inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Debug for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CacheKey({:?})", self.0)
    }
}

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for CacheKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for CacheKey {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CacheKey {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for CacheKey {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&String> for CacheKey {
    fn from(s: &String) -> Self {
        Self::new(s)
    }
}

// =============================================================================
// VdomCache
// =============================================================================

/// VDOM cache - stores previous VDOM state for diffing
#[derive(Debug, Default)]
pub struct VdomCache {
    /// Maps URL path to cached VDOM document
    pages: FxHashMap<CacheKey, Document<Indexed>>,
}

impl VdomCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Get cached VDOM for a cache key.
    pub fn get(&self, key: &CacheKey) -> Option<&Document<Indexed>> {
        self.pages.get(key)
    }

    /// Insert or update cached VDOM, returns the old value if any.
    pub fn insert(
        &mut self,
        key: CacheKey,
        doc: Document<Indexed>,
    ) -> Option<Document<Indexed>> {
        self.pages.insert(key, doc)
    }

    /// Remove a page from cache.
    pub fn remove(&mut self, key: &CacheKey) -> Option<Document<Indexed>> {
        self.pages.remove(key)
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
    pub fn keys(&self) -> impl Iterator<Item = &CacheKey> {
        self.pages.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_normalization() {
        let key1 = CacheKey::new("/blog/post/");
        let key2 = CacheKey::new("/blog/post");
        let key3 = CacheKey::new("blog/post");

        assert_eq!(key1, key2);
        assert_eq!(key2, key3);
        assert_eq!(key1.as_str(), "/blog/post");
    }

    #[test]
    fn test_cache_with_trailing_slash() {
        use crate::Element;

        let mut cache = VdomCache::new();
        let root: Element<Indexed> = Element::new("html");
        let doc = Document::new(root);

        // Insert with trailing slash - CacheKey normalizes it
        let key_with_slash = CacheKey::new("/blog/post/");
        cache.insert(key_with_slash.clone(), doc.clone());

        // Get with same key should work
        assert!(cache.get(&key_with_slash).is_some());

        // Get with key created without trailing slash should also work
        // because CacheKey normalizes both to the same value
        let key_without_slash = CacheKey::new("/blog/post");
        assert!(cache.get(&key_without_slash).is_some());
    }

    #[test]
    fn test_cache_empty() {
        let cache = VdomCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        let key = CacheKey::new("/test");
        assert!(cache.get(&key).is_none());
    }
}
