//! Content hashing and freshness detection using blake3.
//!
//! Provides the core logic for computing file hashes and determining
//! whether outputs are fresh relative to their inputs.

use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::path::Path;
use walkdir::WalkDir;

use super::cache::{get_cached_hash, set_cached_hash};
use crate::config::SiteConfig;

// =============================================================================
// ContentHash Type
// =============================================================================

/// A 256-bit content hash (blake3 output).
///
/// This is a thin wrapper around `[u8; 32]` that provides:
/// - `Copy` semantics for efficient passing
/// - Hex display for debugging
/// - Comparison operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Create a new ContentHash from raw bytes.
    #[inline]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Create a hash representing "no content" (all zeros).
    ///
    /// Used when a file doesn't exist or cannot be read.
    #[inline]
    pub const fn empty() -> Self {
        Self([0; 32])
    }

    /// Check if this is the empty/zero hash.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0 == [0; 32]
    }

    /// Convert to hex string (for debugging/display).
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    /// Create from hex string.
    #[allow(dead_code)]
    pub fn from_hex(s: &str) -> Option<Self> {
        let bytes = hex::decode(s).ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Some(Self(arr))
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display first 16 chars of hex for brevity
        write!(f, "{}", &self.to_hex()[..16])
    }
}

// =============================================================================
// Hash Computation
// =============================================================================

/// Compute blake3 hash of file contents.
///
/// Uses streaming for memory efficiency with large files.
/// Returns `ContentHash::empty()` if file cannot be read.
pub fn compute_file_hash(path: &Path) -> ContentHash {
    // Check cache first
    if let Some(cached) = get_cached_hash(path) {
        return cached;
    }

    // Compute hash
    let hash = compute_file_hash_uncached(path);

    // Cache result (only for existing files)
    if !hash.is_empty() {
        set_cached_hash(path, hash);
    }

    hash
}

/// Compute hash without cache lookup (internal use).
fn compute_file_hash_uncached(path: &Path) -> ContentHash {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return ContentHash::empty(),
    };

    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buffer[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return ContentHash::empty(),
        }
    }

    ContentHash::new(*hasher.finalize().as_bytes())
}

/// Compute combined hash of all dependency files.
///
/// Dependencies include:
/// - `tola.toml` (site configuration)
/// - All files in `deps` directories (templates, utils, etc.)
///
/// The combined hash changes if any dependency content changes.
pub fn compute_deps_hash(config: &SiteConfig) -> ContentHash {
    let mut hasher = blake3::Hasher::new();

    // Hash config file
    let config_hash = compute_file_hash(&config.config_path);
    hasher.update(config_hash.as_bytes());

    // Hash all deps directory files (sorted for determinism)
    let mut dep_files: Vec<_> = config
        .build
        .deps
        .iter()
        .flat_map(|dir| {
            WalkDir::new(dir)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path())
        })
        .collect();
    dep_files.sort();

    for path in dep_files {
        let hash = compute_file_hash(&path);
        hasher.update(hash.as_bytes());
    }

    ContentHash::new(*hasher.finalize().as_bytes())
}

/// Compute hash of a directory's contents.
///
/// Combines hashes of all files in the directory (recursively).
/// Files are processed in sorted order for deterministic results.
#[allow(dead_code)]
pub fn compute_dir_hash(path: &Path) -> ContentHash {
    if !path.is_dir() {
        return ContentHash::empty();
    }

    let mut hasher = blake3::Hasher::new();
    let mut files: Vec<_> = WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();
    files.sort();

    for file_path in files {
        let hash = compute_file_hash(&file_path);
        hasher.update(hash.as_bytes());
    }

    ContentHash::new(*hasher.finalize().as_bytes())
}

// =============================================================================
// Freshness Detection
// =============================================================================

/// Check if output file is fresh (up-to-date) relative to source.
///
/// An output is fresh if:
/// 1. Both source and output exist
/// 2. Output contains a hash marker matching current source hash
///
/// # Arguments
///
/// * `source` - Path to source file
/// * `output` - Path to output file
/// * `deps_hash` - Optional hash of dependencies (templates, config, etc.)
///
/// # Returns
///
/// `true` if output is fresh and doesn't need rebuilding.
pub fn is_fresh(source: &Path, output: &Path, deps_hash: Option<ContentHash>) -> bool {
    // Output must exist
    if !output.exists() {
        return false;
    }

    // Source must exist
    if !source.exists() {
        return false;
    }

    // Compute current source hash
    let source_hash = compute_file_hash(source);
    if source_hash.is_empty() {
        return false;
    }

    // Read output and check for hash marker
    let output_content = match fs::read(output) {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Look for embedded hash marker: <!-- tola:hash:SOURCEHASH:DEPSHASH -->
    let marker = build_hash_marker(&source_hash, deps_hash.as_ref());
    output_content
        .windows(marker.len())
        .any(|w| w == marker.as_bytes())
}

/// Build hash marker string for embedding in output.
///
/// Format: `<!-- tola:hash:SOURCEHASH:DEPSHASH -->`
pub fn build_hash_marker(source_hash: &ContentHash, deps_hash: Option<&ContentHash>) -> String {
    let deps = deps_hash.map_or_else(|| "0".to_string(), |h| h.to_hex()[..16].to_string());
    format!(
        "<!-- tola:hash:{}:{} -->",
        &source_hash.to_hex()[..16],
        deps
    )
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_content_hash_display() {
        let hash = ContentHash::new([0xab; 32]);
        assert_eq!(format!("{}", hash), "abababababababab");
    }

    #[test]
    fn test_content_hash_hex_roundtrip() {
        let original = ContentHash::new([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
                                         0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
                                         0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
                                         0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        let hex = original.to_hex();
        let recovered = ContentHash::from_hex(&hex).unwrap();
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_compute_file_hash() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "hello world").unwrap();

        let hash1 = compute_file_hash(&path);
        let hash2 = compute_file_hash(&path);

        // Same content = same hash
        assert_eq!(hash1, hash2);
        assert!(!hash1.is_empty());

        // Different content = different hash
        fs::write(&path, "goodbye world").unwrap();
        super::super::cache::FRESHNESS_CACHE.invalidate(&path);
        let hash3 = compute_file_hash(&path);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_compute_file_hash_nonexistent() {
        let hash = compute_file_hash(Path::new("/nonexistent/file.txt"));
        assert!(hash.is_empty());
    }

    #[test]
    fn test_is_fresh_with_marker() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.typ");
        let output = dir.path().join("output.html");

        // Write source
        fs::write(&source, "content").unwrap();
        let source_hash = compute_file_hash(&source);

        // Write output without marker - should not be fresh
        fs::write(&output, "<html>output</html>").unwrap();
        assert!(!is_fresh(&source, &output, None));

        // Write output with correct marker - should be fresh
        let marker = build_hash_marker(&source_hash, None);
        fs::write(&output, format!("<html>output</html>{}", marker)).unwrap();
        assert!(is_fresh(&source, &output, None));

        // Change source - should no longer be fresh
        fs::write(&source, "changed content").unwrap();
        super::super::cache::FRESHNESS_CACHE.invalidate(&source);
        assert!(!is_fresh(&source, &output, None));
    }

    #[test]
    fn test_is_fresh_with_deps() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.typ");
        let output = dir.path().join("output.html");

        fs::write(&source, "content").unwrap();
        let source_hash = compute_file_hash(&source);
        let deps_hash = ContentHash::new([0xde; 32]);

        let marker = build_hash_marker(&source_hash, Some(&deps_hash));
        fs::write(&output, format!("<html>output</html>{}", marker)).unwrap();

        // Correct deps hash - fresh
        assert!(is_fresh(&source, &output, Some(deps_hash)));

        // Different deps hash - not fresh
        let different_deps = ContentHash::new([0xab; 32]);
        assert!(!is_fresh(&source, &output, Some(different_deps)));
    }
}
