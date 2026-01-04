//! Deterministic hashing utilities for VDOM
//!
//! Provides cross-process deterministic hashing using blake3.

// =============================================================================
// StableHasher - Builder Pattern
// =============================================================================

/// A deterministic hasher using blake3
///
/// Unlike `std::hash::Hasher`, this produces the same output across
/// process restarts for the same input.
pub struct StableHasher {
    inner: blake3::Hasher,
}

impl StableHasher {
    /// Create a new StableHasher
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: blake3::Hasher::new(),
        }
    }

    /// Update with raw bytes
    #[inline]
    pub fn update(mut self, data: &[u8]) -> Self {
        self.inner.update(data);
        self
    }

    /// Update with a string
    #[inline]
    pub fn update_str(self, s: &str) -> Self {
        self.update(s.as_bytes())
    }

    /// Update with a u64 value (little-endian)
    #[inline]
    pub fn update_u64(self, v: u64) -> Self {
        self.update(&v.to_le_bytes())
    }

    /// Update with a usize value (little-endian)
    #[inline]
    pub fn update_usize(self, v: usize) -> Self {
        self.update(&v.to_le_bytes())
    }

    /// Finish and return the hash as u64
    ///
    /// Takes the first 8 bytes of blake3 output as little-endian u64.
    #[inline]
    pub fn finish(self) -> u64 {
        let hash = self.inner.finalize();
        let bytes: [u8; 8] = hash.as_bytes()[..8].try_into().unwrap();
        u64::from_le_bytes(bytes)
    }
}

impl Default for StableHasher {
    fn default() -> Self {
        Self::new()
    }
}
