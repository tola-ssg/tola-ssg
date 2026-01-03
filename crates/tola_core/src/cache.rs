//! Cache trait abstraction for memoization.
//!
//! This module provides a simple cache interface that can be implemented
//! by various backends (HashMap, LRU, persistent storage, etc.).

use std::hash::Hash;

/// A simple cache interface for storing computed values.
///
/// # Type Parameters
///
/// - `K`: Key type (must be hashable and comparable)
/// - `V`: Value type
pub trait Cache<K, V>
where
    K: Hash + Eq,
{
    /// Get a cached value by key.
    fn get(&self, key: &K) -> Option<&V>;

    /// Insert a value into the cache.
    fn insert(&mut self, key: K, value: V);

    /// Remove a value from the cache.
    fn remove(&mut self, key: &K) -> Option<V>;

    /// Clear all cached values.
    fn clear(&mut self);

    /// Get the number of cached entries.
    fn len(&self) -> usize;

    /// Check if the cache is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A simple HashMap-based cache implementation.
#[derive(Debug, Clone, Default)]
pub struct HashMapCache<K, V>
where
    K: Hash + Eq,
{
    inner: std::collections::HashMap<K, V>,
}

impl<K, V> HashMapCache<K, V>
where
    K: Hash + Eq,
{
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            inner: std::collections::HashMap::new(),
        }
    }

    /// Create a cache with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: std::collections::HashMap::with_capacity(capacity),
        }
    }
}

impl<K, V> Cache<K, V> for HashMapCache<K, V>
where
    K: Hash + Eq,
{
    fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    fn insert(&mut self, key: K, value: V) {
        self.inner.insert(key, value);
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        self.inner.remove(key)
    }

    fn clear(&mut self) {
        self.inner.clear();
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hashmap_cache() {
        let mut cache = HashMapCache::new();

        cache.insert("key1", 42);
        assert_eq!(cache.get(&"key1"), Some(&42));
        assert_eq!(cache.len(), 1);

        cache.remove(&"key1");
        assert_eq!(cache.get(&"key1"), None);
        assert!(cache.is_empty());
    }
}
