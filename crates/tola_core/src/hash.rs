//! Content-based hashing utilities.
//!
//! This module provides utilities for computing content hashes,
//! primarily used for change detection and cache invalidation.

use std::hash::{Hash, Hasher};

/// Compute a stable hash for any hashable value.
///
/// Uses the standard library's default hasher. For cryptographic
/// hashing or cross-platform stability, consider using blake3.
pub fn compute_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Compute a hash from multiple values.
///
/// # Example
///
/// ```rust
/// use tola_core::hash::compute_combined_hash;
///
/// let hash = compute_combined_hash(&["hello", "world"]);
/// ```
pub fn compute_combined_hash<T: Hash>(values: &[T]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for value in values {
        value.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash() {
        let hash1 = compute_hash(&"hello");
        let hash2 = compute_hash(&"hello");
        let hash3 = compute_hash(&"world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_combined_hash() {
        let hash1 = compute_combined_hash(&["a", "b"]);
        let hash2 = compute_combined_hash(&["a", "b"]);
        let hash3 = compute_combined_hash(&["b", "a"]);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3); // Order matters
    }
}
