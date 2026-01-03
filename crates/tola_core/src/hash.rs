//! Deterministic hashing utilities for content-based identity.
//!
//! Provides cross-process deterministic hashing using blake3.
//! This module should be used instead of `std::collections::hash_map::DefaultHasher`
//! which uses a random seed and is NOT deterministic across process restarts.
//!
//! # Why not DefaultHasher or FxHasher?
//!
//! - `DefaultHasher` uses SipHash with a random seed initialized at program start.
//! - `FxHasher` is fast but implementation-dependent - not guaranteed stable.
//! - `blake3` is cryptographic, fast, and produces identical output everywhere.
//!
//! # Usage
//!
//! ```
//! use tola_core::hash::{StableHasher, compute};
//!
//! // Simple hashing
//! let h = compute("hello");
//!
//! // Builder pattern for complex hashing
//! let h = StableHasher::new()
//!     .update_str("tag")
//!     .update_u64(42)
//!     .finish();
//! ```

// =============================================================================
// StableHasher - Builder Pattern
// =============================================================================

/// A deterministic hasher using blake3.
///
/// Unlike `std::hash::Hasher`, this produces the same output across
/// process restarts for the same input.
///
/// # Example
///
/// ```
/// use tola_core::hash::StableHasher;
///
/// let hash = StableHasher::new()
///     .update_str("element")
///     .update_str("div")
///     .update_usize(0)
///     .finish();
/// ```
pub struct StableHasher {
    inner: blake3::Hasher,
}

impl StableHasher {
    /// Create a new StableHasher.
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: blake3::Hasher::new(),
        }
    }

    /// Update with raw bytes.
    #[inline]
    pub fn update(mut self, data: &[u8]) -> Self {
        self.inner.update(data);
        self
    }

    /// Update with a string slice.
    #[inline]
    pub fn update_str(self, s: &str) -> Self {
        self.update(s.as_bytes())
    }

    /// Update with a u64 value (little-endian).
    #[inline]
    pub fn update_u64(self, v: u64) -> Self {
        self.update(&v.to_le_bytes())
    }

    /// Update with a usize value (little-endian).
    #[inline]
    pub fn update_usize(self, v: usize) -> Self {
        self.update(&v.to_le_bytes())
    }

    /// Finish and return the hash as u64.
    ///
    /// Takes the first 8 bytes of blake3 output as little-endian u64.
    #[inline]
    pub fn finish(self) -> u64 {
        let hash = self.inner.finalize();
        u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap())
    }

    /// Finish and return the full 256-bit hash.
    #[inline]
    pub fn finish_full(self) -> [u8; 32] {
        *self.inner.finalize().as_bytes()
    }
}

impl Default for StableHasher {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Convenience functions
// =============================================================================

/// Compute 64-bit deterministic hash from byte data.
///
/// # Example
///
/// ```
/// use tola_core::hash::compute;
///
/// let hash = compute("hello world");
/// assert_ne!(hash, 0);
/// ```
#[inline]
pub fn compute<T: AsRef<[u8]> + ?Sized>(data: &T) -> u64 {
    StableHasher::new().update(data.as_ref()).finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute() {
        let hash1 = compute("hello");
        let hash2 = compute("hello");
        let hash3 = compute("world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_stable_hasher() {
        let hash1 = StableHasher::new()
            .update_str("a")
            .update_str("b")
            .finish();
        let hash2 = StableHasher::new()
            .update_str("a")
            .update_str("b")
            .finish();
        let hash3 = StableHasher::new()
            .update_str("b")
            .update_str("a")
            .finish();

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3); // Order matters
    }

    #[test]
    fn test_determinism() {
        // These values should be stable across process restarts
        let hash = compute("test_determinism");
        // Just verify it's non-zero and consistent within test
        assert_ne!(hash, 0);
    }
}
