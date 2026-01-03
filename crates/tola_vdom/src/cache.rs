//! VDOM cache utilities.
//!
//! Provides caching for transformation results to avoid redundant
//! computation during hot reload.

use std::collections::HashMap;

use crate::id::StableId;

/// Cache for transformed document fragments.
///
/// Keyed by StableId for efficient lookup of previously processed nodes.
pub struct VdomCache<T> {
    entries: HashMap<StableId, T>,
}

impl<T> VdomCache<T> {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Get a cached value by StableId.
    pub fn get(&self, id: &StableId) -> Option<&T> {
        self.entries.get(id)
    }

    /// Insert a value into the cache.
    pub fn insert(&mut self, id: StableId, value: T) {
        self.entries.insert(id, value);
    }

    /// Check if the cache contains an entry.
    pub fn contains(&self, id: &StableId) -> bool {
        self.entries.contains_key(id)
    }

    /// Remove an entry from the cache.
    pub fn remove(&mut self, id: &StableId) -> Option<T> {
        self.entries.remove(id)
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get the number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<T> Default for VdomCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_operations() {
        let mut cache: VdomCache<String> = VdomCache::new();
        let id = StableId::from_raw(123);

        assert!(!cache.contains(&id));
        cache.insert(id, "test".to_string());
        assert!(cache.contains(&id));
        assert_eq!(cache.get(&id), Some(&"test".to_string()));
        assert_eq!(cache.len(), 1);

        cache.remove(&id);
        assert!(!cache.contains(&id));
        assert!(cache.is_empty());
    }
}
