//! Redb-based cache storage (optional high-performance backend)
//!
//! This module provides a redb-backed implementation of the cache storage,
//! offering better performance for large caches compared to the file-based
//! implementation.
//!
//! Enabled with the `cache` feature flag.

#![cfg(feature = "cache")]

use std::path::{Path, PathBuf};

use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};

use super::storage::{CacheKey, CacheStats};
use crate::utils::platform::ARCH_FINGERPRINT;

// Table definitions
const VDOM_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vdom_cache");
const META_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");

/// Redb-backed VDOM cache
///
/// Uses an embedded key-value store for better performance with large caches.
/// Automatically handles:
/// - Architecture fingerprint validation
/// - Atomic writes
/// - Cache size management
pub struct RedbCache {
    db: Database,
    max_size: u64,
}

impl RedbCache {
    /// Create or open a redb cache
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the database file
    /// * `max_size_mb` - Maximum cache size in megabytes (0 = unlimited)
    pub fn new(path: PathBuf, max_size_mb: u64) -> Result<Self, redb::Error> {
        // Add architecture fingerprint to path
        let db_path = path.with_extension(format!("{}.redb", ARCH_FINGERPRINT));

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let db = Database::create(&db_path)?;

        // Initialize tables
        let write_txn = db.begin_write()?;
        {
            // Create tables if they don't exist
            let _ = write_txn.open_table(VDOM_TABLE)?;
            let _ = write_txn.open_table(META_TABLE)?;
        }
        write_txn.commit()?;

        Ok(Self {
            db,
            max_size: max_size_mb * 1024 * 1024,
        })
    }

    /// Get cached data for a source file
    pub fn get(&self, source_path: &Path) -> Option<Vec<u8>> {
        let key = CacheKey::from_path(source_path)?;
        if !key.is_arch_compatible() {
            return None;
        }

        let key_bytes = key.to_bytes();
        let read_txn = self.db.begin_read().ok()?;
        let table = read_txn.open_table(VDOM_TABLE).ok()?;

        table
            .get(key_bytes.as_slice())
            .ok()?
            .map(|v| v.value().to_vec())
    }

    /// Store cached data for a source file
    pub fn put(&self, source_path: &Path, data: &[u8]) -> anyhow::Result<()> {
        let Some(key) = CacheKey::from_path(source_path) else {
            return Ok(());
        };

        // Check size limit and evict if needed
        if self.max_size > 0 {
            let current_size = self.total_size();
            if current_size + data.len() as u64 > self.max_size {
                self.evict_oldest(data.len() as u64)?;
            }
        }

        let key_bytes = key.to_bytes();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(VDOM_TABLE)?;
            table.insert(key_bytes.as_slice(), data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Remove cached data for a source file
    pub fn remove(&self, source_path: &Path) -> anyhow::Result<()> {
        let Some(key) = CacheKey::from_path(source_path) else {
            return Ok(());
        };

        let key_bytes = key.to_bytes();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(VDOM_TABLE)?;
            table.remove(key_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Clear all cached entries
    pub fn clear(&self) -> anyhow::Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            // Drop and recreate the table
            write_txn.delete_table(VDOM_TABLE)?;
            let _ = write_txn.open_table(VDOM_TABLE)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get total cache size in bytes
    pub fn total_size(&self) -> u64 {
        let Ok(read_txn) = self.db.begin_read() else {
            return 0;
        };

        let Ok(table) = read_txn.open_table(VDOM_TABLE) else {
            return 0;
        };

        let Ok(iter) = table.iter() else {
            return 0;
        };

        iter.filter_map(|r| r.ok())
            .map(|(_, v)| v.value().len() as u64)
            .sum()
    }

    /// Get number of cached entries
    pub fn entry_count(&self) -> usize {
        let Ok(read_txn) = self.db.begin_read() else {
            return 0;
        };

        let Ok(table) = read_txn.open_table(VDOM_TABLE) else {
            return 0;
        };

        table.len().unwrap_or(0) as usize
    }

    /// Evict oldest entries to free space
    fn evict_oldest(&self, needed: u64) -> anyhow::Result<()> {
        // For now, just clear everything if we need space
        // A more sophisticated LRU implementation would track access times
        if needed > 0 {
            self.clear()?;
        }
        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            total_size: self.total_size(),
            entry_count: self.entry_count(),
            max_size: self.max_size,
        }
    }

    /// Invalidate cache for multiple files and their dependents
    pub fn invalidate_with_dependents(
        &self,
        changed_paths: &[PathBuf],
        get_dependents: impl Fn(&Path) -> Vec<PathBuf>,
    ) -> anyhow::Result<()> {
        for path in changed_paths {
            self.remove(path)?;

            // Propagate invalidation
            for dependent in get_dependents(path) {
                self.remove(&dependent)?;
            }
        }
        Ok(())
    }
}

/// High-level VDOM cache using redb backend
pub struct RedbVdomCache {
    cache: RedbCache,
}

impl RedbVdomCache {
    /// Create a new redb-backed VDOM cache
    pub fn new(cache_dir: PathBuf, max_size_mb: u64) -> Result<Self, redb::Error> {
        let cache = RedbCache::new(cache_dir.join("vdom"), max_size_mb)?;
        Ok(Self { cache })
    }

    /// Get cached VDOM bytes
    pub fn get(&self, source_path: &Path) -> Option<Vec<u8>> {
        self.cache.get(source_path)
    }

    /// Store VDOM bytes
    pub fn put(&self, source_path: &Path, data: &[u8]) -> anyhow::Result<()> {
        self.cache.put(source_path, data)
    }

    /// Invalidate cached entry
    pub fn invalidate(&self, source_path: &Path) -> anyhow::Result<()> {
        self.cache.remove(source_path)
    }

    /// Clear all cache
    pub fn clear(&self) -> anyhow::Result<()> {
        self.cache.clear()
    }

    /// Get statistics
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_redb_cache_basic() {
        let temp_dir = TempDir::new().unwrap();
        let cache = RedbCache::new(temp_dir.path().join("test"), 0).unwrap();

        // Create a test file
        let test_file = temp_dir.path().join("test.typ");
        std::fs::write(&test_file, "test content").unwrap();

        // Initially empty
        assert!(cache.get(&test_file).is_none());

        // Put and get
        cache.put(&test_file, b"cached data").unwrap();
        assert_eq!(cache.get(&test_file), Some(b"cached data".to_vec()));

        // Remove
        cache.remove(&test_file).unwrap();
        assert!(cache.get(&test_file).is_none());
    }

    #[test]
    fn test_redb_cache_stats() {
        let temp_dir = TempDir::new().unwrap();
        let cache = RedbCache::new(temp_dir.path().join("test"), 100).unwrap();

        let stats = cache.stats();
        assert_eq!(stats.entry_count, 0);
        assert_eq!(stats.total_size, 0);
        assert_eq!(stats.max_size, 100 * 1024 * 1024);
    }
}
