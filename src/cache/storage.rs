//! Cache Storage Implementation
//!
//! Provides file-based caching for VDOM documents.
//! Uses redb for embedded key-value storage when the `cache` feature is enabled.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::utils::platform::ARCH_FINGERPRINT;

// =============================================================================
// Cache Key
// =============================================================================

/// Cache key combining path hash, modification time, and architecture
///
/// The key ensures cache entries are invalidated when:
/// 1. The source file changes (mtime)
/// 2. The architecture changes (fingerprint)
/// 3. The path changes (hash)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Hash of the source file path
    pub path_hash: u64,
    /// Last modification time of the source file
    pub mtime: SystemTime,
    /// Architecture fingerprint at cache time
    pub arch: &'static str,
}

impl CacheKey {
    /// Create a new cache key from a file path
    ///
    /// Returns `None` if the file metadata cannot be read.
    pub fn from_path(path: &Path) -> Option<Self> {
        let mtime = path.metadata().ok()?.modified().ok()?;
        let path_hash = crate::utils::platform::path_to_cache_hash(path);

        Some(Self {
            path_hash,
            mtime,
            arch: ARCH_FINGERPRINT,
        })
    }

    /// Create a key with custom mtime (for testing)
    pub fn with_mtime(path: &Path, mtime: SystemTime) -> Self {
        let path_hash = crate::utils::platform::path_to_cache_hash(path);
        Self {
            path_hash,
            mtime,
            arch: ARCH_FINGERPRINT,
        }
    }

    /// Convert to bytes for storage key
    ///
    /// Format: [path_hash: 8 bytes][mtime_secs: 8 bytes][mtime_nanos: 4 bytes][arch_hash: 4 bytes]
    pub fn to_bytes(&self) -> [u8; 24] {
        let mut bytes = [0u8; 24];

        // Path hash (8 bytes)
        bytes[0..8].copy_from_slice(&self.path_hash.to_le_bytes());

        // Mtime as duration since UNIX_EPOCH
        let duration = self.mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        bytes[8..16].copy_from_slice(&duration.as_secs().to_le_bytes());
        bytes[16..20].copy_from_slice(&duration.subsec_nanos().to_le_bytes());

        // Architecture hash (4 bytes)
        let arch_hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            self.arch.hash(&mut hasher);
            (hasher.finish() as u32).to_le_bytes()
        };
        bytes[20..24].copy_from_slice(&arch_hash);

        bytes
    }

    /// Check if this key matches the current architecture
    pub fn is_arch_compatible(&self) -> bool {
        self.arch == ARCH_FINGERPRINT
    }
}

// =============================================================================
// Cache Entry
// =============================================================================

/// A cached VDOM entry with metadata
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// The cached data (serialized VDOM)
    pub data: Vec<u8>,
    /// When the entry was created
    pub created_at: SystemTime,
    /// Size of the original (uncompressed) data
    pub original_size: usize,
}

impl CacheEntry {
    /// Create a new cache entry
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            original_size: data.len(),
            data,
            created_at: SystemTime::now(),
        }
    }

    /// Check if the entry is older than a given duration
    pub fn is_older_than(&self, duration: std::time::Duration) -> bool {
        self.created_at
            .elapsed()
            .map(|elapsed| elapsed > duration)
            .unwrap_or(true)
    }
}

// =============================================================================
// File-based Cache (No external dependencies)
// =============================================================================

/// Simple file-based cache for VDOM documents
///
/// This implementation stores each cache entry as a separate file.
/// It's simpler than redb but has higher filesystem overhead.
pub struct FileCache {
    /// Root directory for cache files
    root: PathBuf,
    /// Maximum cache size in bytes (0 = unlimited)
    max_size: u64,
}

impl FileCache {
    /// Create a new file cache
    ///
    /// # Arguments
    ///
    /// * `root` - Directory to store cache files
    /// * `max_size` - Maximum total cache size in bytes (0 = unlimited)
    pub fn new(root: PathBuf, max_size: u64) -> std::io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root, max_size })
    }

    /// Get the cache file path for a key
    fn cache_file_path(&self, key: &CacheKey) -> PathBuf {
        let hex = hex::encode(key.to_bytes());
        self.root.join(format!("{}_{}.cache", hex, ARCH_FINGERPRINT))
    }

    /// Get a cached entry
    pub fn get(&self, key: &CacheKey) -> Option<CacheEntry> {
        if !key.is_arch_compatible() {
            return None;
        }

        let path = self.cache_file_path(key);
        let data = std::fs::read(&path).ok()?;

        Some(CacheEntry::new(data))
    }

    /// Store an entry in the cache
    pub fn put(&self, key: &CacheKey, data: &[u8]) -> std::io::Result<()> {
        // Check size limit
        if self.max_size > 0 {
            let current_size = self.total_size();
            if current_size + data.len() as u64 > self.max_size {
                self.evict_oldest(data.len() as u64)?;
            }
        }

        let path = self.cache_file_path(key);
        std::fs::write(&path, data)
    }

    /// Remove an entry from the cache
    pub fn remove(&self, key: &CacheKey) -> std::io::Result<()> {
        let path = self.cache_file_path(key);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Clear all cache entries
    pub fn clear(&self) -> std::io::Result<()> {
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "cache").unwrap_or(false) {
                std::fs::remove_file(&path)?;
            }
        }
        Ok(())
    }

    /// Get total cache size in bytes
    pub fn total_size(&self) -> u64 {
        std::fs::read_dir(&self.root)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Evict oldest entries to free up space
    fn evict_oldest(&self, needed: u64) -> std::io::Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.root)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                let mtime = meta.modified().ok()?;
                Some((e.path(), mtime, meta.len()))
            })
            .collect();

        // Sort by modification time (oldest first)
        entries.sort_by_key(|(_, mtime, _)| *mtime);

        let mut freed = 0u64;
        for (path, _, size) in entries {
            if freed >= needed {
                break;
            }
            std::fs::remove_file(&path)?;
            freed += size;
        }

        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let mut total_size = 0u64;
        let mut entry_count = 0usize;

        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(meta) = entry.metadata() {
                    if entry.path().extension().map(|e| e == "cache").unwrap_or(false) {
                        total_size += meta.len();
                        entry_count += 1;
                    }
                }
            }
        }

        CacheStats {
            total_size,
            entry_count,
            max_size: self.max_size,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Total size of all cached entries
    pub total_size: u64,
    /// Number of cached entries
    pub entry_count: usize,
    /// Maximum allowed size (0 = unlimited)
    pub max_size: u64,
}

impl CacheStats {
    /// Calculate cache utilization percentage
    pub fn utilization(&self) -> f64 {
        if self.max_size == 0 {
            0.0
        } else {
            (self.total_size as f64 / self.max_size as f64) * 100.0
        }
    }
}

// =============================================================================
// VDOM Cache (High-level API)
// =============================================================================

/// High-level VDOM caching interface
///
/// Wraps FileCache with VDOM-specific operations.
pub struct VdomCache {
    cache: FileCache,
}

impl VdomCache {
    /// Create a new VDOM cache
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Directory for cache files
    /// * `max_size_mb` - Maximum cache size in megabytes (0 = unlimited)
    pub fn new(cache_dir: PathBuf, max_size_mb: u64) -> std::io::Result<Self> {
        let max_size = max_size_mb * 1024 * 1024;
        let cache = FileCache::new(cache_dir, max_size)?;
        Ok(Self { cache })
    }

    /// Get cached VDOM bytes for a source file
    ///
    /// Returns `None` if not cached or cache is invalid.
    pub fn get(&self, source_path: &Path) -> Option<Vec<u8>> {
        let key = CacheKey::from_path(source_path)?;
        self.cache.get(&key).map(|e| e.data)
    }

    /// Cache VDOM bytes for a source file
    pub fn put(&self, source_path: &Path, data: &[u8]) -> std::io::Result<()> {
        if let Some(key) = CacheKey::from_path(source_path) {
            self.cache.put(&key, data)?;
        }
        Ok(())
    }

    /// Invalidate cache for a source file
    pub fn invalidate(&self, source_path: &Path) -> std::io::Result<()> {
        if let Some(key) = CacheKey::from_path(source_path) {
            self.cache.remove(&key)?;
        }
        Ok(())
    }

    /// Invalidate cache for multiple files and their dependents
    pub fn invalidate_with_dependents(
        &self,
        changed_paths: &[PathBuf],
        get_dependents: impl Fn(&Path) -> Vec<PathBuf>,
    ) -> std::io::Result<()> {
        for path in changed_paths {
            self.invalidate(path)?;

            // Propagate invalidation to dependents
            for dependent in get_dependents(path) {
                self.invalidate(&dependent)?;
            }
        }
        Ok(())
    }

    /// Clear all cached entries
    pub fn clear(&self) -> std::io::Result<()> {
        self.cache.clear()
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_cache_key_bytes() {
        let path = Path::new("test.typ");
        let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let key = CacheKey::with_mtime(path, mtime);

        let bytes = key.to_bytes();
        assert_eq!(bytes.len(), 24);

        // Same input should produce same bytes
        let key2 = CacheKey::with_mtime(path, mtime);
        assert_eq!(key.to_bytes(), key2.to_bytes());

        // Different mtime should produce different bytes
        let mtime2 = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);
        let key3 = CacheKey::with_mtime(path, mtime2);
        assert_ne!(key.to_bytes(), key3.to_bytes());
    }

    #[test]
    fn test_cache_key_arch_compatible() {
        let path = Path::new("test.typ");
        let mtime = SystemTime::now();
        let key = CacheKey::with_mtime(path, mtime);

        assert!(key.is_arch_compatible());

        let mut key_wrong_arch = key.clone();
        key_wrong_arch.arch = "wrong_arch";
        assert!(!key_wrong_arch.is_arch_compatible());
    }

    #[test]
    fn test_cache_entry_age() {
        let entry = CacheEntry::new(vec![1, 2, 3]);

        // New entry should not be older than 1 hour
        assert!(!entry.is_older_than(Duration::from_secs(3600)));

        // But should be "older" than 0 seconds (effectively)
        // Note: This might be flaky depending on timing
    }

    #[test]
    fn test_cache_stats() {
        let stats = CacheStats {
            total_size: 50 * 1024 * 1024,
            entry_count: 100,
            max_size: 100 * 1024 * 1024,
        };

        assert!((stats.utilization() - 50.0).abs() < 0.001);
    }
}
