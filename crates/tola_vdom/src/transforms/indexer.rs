//! Indexer transform: Raw → Indexed phase.
//!
//! Assigns stable IDs to all nodes for diffing.

use crate::id::PageSeed;

/// Indexer transform for Raw → Indexed conversion.
pub struct Indexer {
    seed: PageSeed,
}

impl Indexer {
    /// Create a new indexer with default seed.
    pub fn new() -> Self {
        Self {
            seed: PageSeed::zero(),
        }
    }

    /// Create an indexer with specific page seed.
    pub fn with_seed(seed: PageSeed) -> Self {
        Self { seed }
    }

    /// Get the page seed.
    pub fn seed(&self) -> PageSeed {
        self.seed
    }
}

impl Default for Indexer {
    fn default() -> Self {
        Self::new()
    }
}
