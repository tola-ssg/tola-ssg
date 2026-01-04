//! File caching with fingerprint-based invalidation.
//!
//! Files are cached globally to enable reuse across compilations.
//! Fingerprint-based invalidation ensures changed files are re-read.
//!
//! # Caching Strategy
//!
//! ```text
//! GLOBAL_FILE_CACHE (shared across all compilations)
//! └── FxHashMap<FileId, FileSlot>
//!     └── FileSlot
//!         ├── source: SlotCell<Source>  ─┐
//!         └── file: SlotCell<Bytes>     ─┼── Fingerprint-based invalidation
//!
//! Access Flow:
//! 1. If accessed=true && data.is_some() → return cached (fast path)
//! 2. Load file, compute fingerprint
//! 3. If fingerprint unchanged → return cached
//! 4. Otherwise → recompute and cache
//! ```
//!
//! # Virtual Data Extension
//!
//! This module provides basic file caching. The main application (tola) extends
//! this with virtual data support for `/_data/*.json` files via the
//! [`VirtualDataProvider`] trait.

use std::cell::RefCell;
use std::fs;
use std::io::{self, Read};
use std::mem;
use std::path::Path;
use std::sync::LazyLock;

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use typst::diag::{FileError, FileResult};
use typst::foundations::Bytes;
use typst::syntax::{FileId, Source, VirtualPath};
use typst_kit::download::{DownloadState, Progress};

use crate::config::{default_typst_toml, package_storage};

// =============================================================================
// Constants
// =============================================================================

/// Virtual `FileId` for stdin input.
pub static STDIN_ID: LazyLock<FileId> =
    LazyLock::new(|| FileId::new_fake(VirtualPath::new("<stdin>")));

/// Virtual `FileId` for empty/no input.
pub static EMPTY_ID: LazyLock<FileId> =
    LazyLock::new(|| FileId::new_fake(VirtualPath::new("<empty>")));

// =============================================================================
// Virtual Data Provider Trait
// =============================================================================

/// Trait for providing virtual data files.
///
/// The main application can implement this to provide dynamically generated
/// files like `/_data/pages.json` that don't exist on disk.
pub trait VirtualDataProvider: Send + Sync {
    /// Check if the given path is a virtual data path.
    fn is_virtual_path(&self, path: &Path) -> bool;

    /// Read virtual data for the given path.
    /// Returns `None` if the path is not a virtual data path.
    fn read_virtual(&self, path: &Path) -> Option<Vec<u8>>;
}

/// No-op virtual data provider (no virtual files).
pub struct NoVirtualData;

impl VirtualDataProvider for NoVirtualData {
    fn is_virtual_path(&self, _path: &Path) -> bool {
        false
    }

    fn read_virtual(&self, _path: &Path) -> Option<Vec<u8>> {
        None
    }
}

// =============================================================================
// Global Virtual Data Provider
// =============================================================================

/// Global virtual data provider.
///
/// This allows the main application to register a custom virtual data provider
/// that will be used by `FileSlot::source()` and `FileSlot::file()`.
static GLOBAL_VIRTUAL_PROVIDER: LazyLock<RwLock<Box<dyn VirtualDataProvider>>> =
    LazyLock::new(|| RwLock::new(Box::new(NoVirtualData)));

/// Set the global virtual data provider.
///
/// Call this at application startup to enable virtual data files like
/// `/_data/pages.json`.
///
/// # Example
///
/// ```ignore
/// use typst_batch::file::{set_virtual_provider, VirtualDataProvider};
///
/// struct MyVirtualData;
///
/// impl VirtualDataProvider for MyVirtualData {
///     fn is_virtual_path(&self, path: &Path) -> bool {
///         path.starts_with("/_data/")
///     }
///     fn read_virtual(&self, path: &Path) -> Option<Vec<u8>> {
///         // Return virtual file content
///         Some(b"{}".to_vec())
///     }
/// }
///
/// set_virtual_provider(MyVirtualData);
/// ```
pub fn set_virtual_provider<V: VirtualDataProvider + 'static>(provider: V) {
    *GLOBAL_VIRTUAL_PROVIDER.write() = Box::new(provider);
}

/// Check if the given path is a virtual data path using the global provider.
pub fn is_virtual_path(path: &Path) -> bool {
    GLOBAL_VIRTUAL_PROVIDER.read().is_virtual_path(path)
}

/// Read virtual data for the given path using the global provider.
pub fn read_virtual(path: &Path) -> Option<Vec<u8>> {
    GLOBAL_VIRTUAL_PROVIDER.read().read_virtual(path)
}

// =============================================================================
// Global File Cache
// =============================================================================

/// Global shared file cache - reused across all compilations.
pub static GLOBAL_FILE_CACHE: LazyLock<RwLock<FxHashMap<FileId, FileSlot>>> =
    LazyLock::new(|| RwLock::new(FxHashMap::default()));

// =============================================================================
// Thread-Local Access Tracking
// =============================================================================

thread_local! {
    /// Thread-local set of accessed file IDs for the current compilation.
    /// This avoids race conditions when compiling files in parallel.
    static ACCESSED_FILES: RefCell<rustc_hash::FxHashSet<FileId>> =
        RefCell::new(rustc_hash::FxHashSet::default());
}

/// Clear the thread-local accessed files set and reset global cache access flags.
///
/// Call at the start of each file compilation.
pub fn reset_access_flags() {
    // Reset thread-local tracking
    ACCESSED_FILES.with(|files| files.borrow_mut().clear());

    // Reset global cache access flags for fingerprint re-checking
    for slot in GLOBAL_FILE_CACHE.write().values_mut() {
        slot.reset_access();
    }
}

/// Record a file access in the thread-local set.
pub fn record_file_access(id: FileId) {
    ACCESSED_FILES.with(|files| {
        files.borrow_mut().insert(id);
    });
}

/// Get all files accessed during the current compilation.
///
/// Returns a list of `FileId`s that were accessed since last `reset_access_flags()`.
/// Thread-safe: each thread has its own tracking.
pub fn get_accessed_files() -> Vec<FileId> {
    ACCESSED_FILES.with(|files| files.borrow().iter().copied().collect())
}

/// Clear the global file cache.
///
/// Call when template/dependency files change to ensure fresh data is loaded.
/// This also clears the comemo cache.
pub fn clear_file_cache() {
    GLOBAL_FILE_CACHE.write().clear();
    typst::comemo::evict(0);
}

// =============================================================================
// SlotCell - Fingerprint-based Caching
// =============================================================================

/// Lazily processes data for a file with fingerprint-based caching.
pub struct SlotCell<T> {
    data: Option<FileResult<T>>,
    fingerprint: u128,
    /// Whether this cell has been accessed in the current compilation.
    pub accessed: bool,
}

impl<T: Clone> SlotCell<T> {
    /// Create a new empty slot cell.
    pub const fn new() -> Self {
        Self {
            data: None,
            fingerprint: 0,
            accessed: false,
        }
    }

    /// Reset the access flag for a new compilation.
    pub const fn reset_access(&mut self) {
        self.accessed = false;
    }

    /// Get or initialize cached data using fingerprint-based invalidation.
    pub fn get_or_init(
        &mut self,
        load: impl FnOnce() -> FileResult<Vec<u8>>,
        process: impl FnOnce(Vec<u8>, Option<T>) -> FileResult<T>,
    ) -> FileResult<T> {
        // Fast path: already accessed in this compilation
        if mem::replace(&mut self.accessed, true)
            && let Some(data) = &self.data
        {
            return data.clone();
        }

        let result = load();
        let fingerprint = typst::utils::hash128(&result);

        // Fingerprint unchanged: reuse previous result
        if mem::replace(&mut self.fingerprint, fingerprint) == fingerprint
            && let Some(data) = &self.data
        {
            return data.clone();
        }

        // Process and cache new data
        let prev = self.data.take().and_then(Result::ok);
        let value = result.and_then(|data| process(data, prev));
        self.data = Some(value.clone());
        value
    }
}

// =============================================================================
// File Reading
// =============================================================================

/// Read file content from a `FileId`.
///
/// Handles special cases:
/// - `EMPTY_ID`: Returns empty bytes
/// - `STDIN_ID`: Reads from stdin
/// - Package files: Downloads package if needed
/// - Missing `typst.toml`: Generates default
pub fn read(id: FileId, project_root: &Path) -> FileResult<Vec<u8>> {
    read_with_virtual(id, project_root, &NoVirtualData)
}

/// Read file content using the global virtual data provider.
///
/// This function uses the globally registered virtual data provider.
/// Call [`set_virtual_provider`] at startup to register your provider.
pub fn read_with_global_virtual(id: FileId, project_root: &Path) -> FileResult<Vec<u8>> {
    // Handle virtual file IDs first (don't need provider)
    if id == *EMPTY_ID {
        return Ok(Vec::new());
    }
    if id == *STDIN_ID {
        return read_stdin();
    }

    // Check global virtual provider
    let vpath = id.vpath().as_rooted_path();
    if is_virtual_path(vpath) {
        record_file_access(id);
        return read_virtual(vpath).ok_or_else(|| FileError::NotFound(vpath.to_path_buf()));
    }

    // Resolve path with typst.toml fallback
    let path = resolve_path(project_root, id).or_else(|e| {
        id.vpath()
            .resolve(project_root)
            .filter(|p| p.ends_with("typst.toml") && !p.exists())
            .ok_or(e)
    })?;

    // Generate default typst.toml if missing
    if path.ends_with("typst.toml") && !path.exists() {
        return Ok(default_typst_toml());
    }

    read_disk(&path)
}

/// Read file content with virtual data support.
///
/// Like [`read`], but also handles virtual data files via the provider.
pub fn read_with_virtual<V: VirtualDataProvider>(
    id: FileId,
    project_root: &Path,
    virtual_provider: &V,
) -> FileResult<Vec<u8>> {
    // Handle virtual file IDs
    if id == *EMPTY_ID {
        return Ok(Vec::new());
    }
    if id == *STDIN_ID {
        return read_stdin();
    }

    // Handle virtual data files (e.g., /_data/*.json)
    let vpath = id.vpath().as_rooted_path();
    if virtual_provider.is_virtual_path(vpath) {
        record_file_access(id);
        return virtual_provider
            .read_virtual(vpath)
            .ok_or_else(|| FileError::NotFound(vpath.to_path_buf()));
    }

    // Resolve path with typst.toml fallback
    let path = resolve_path(project_root, id).or_else(|e| {
        id.vpath()
            .resolve(project_root)
            .filter(|p| p.ends_with("typst.toml") && !p.exists())
            .ok_or(e)
    })?;

    // Generate default typst.toml if missing
    if path.ends_with("typst.toml") && !path.exists() {
        return Ok(default_typst_toml());
    }

    read_disk(&path)
}

/// Decode bytes as UTF-8, stripping BOM if present.
pub fn decode_utf8(buf: &[u8]) -> FileResult<&str> {
    let buf = buf.strip_prefix(b"\xef\xbb\xbf").unwrap_or(buf);
    std::str::from_utf8(buf).map_err(|_| FileError::InvalidUtf8)
}

/// Resolve file path, downloading package if needed.
fn resolve_path(project_root: &Path, id: FileId) -> FileResult<std::path::PathBuf> {
    let root = id
        .package()
        .map(|spec| package_storage().prepare_package(spec, &mut SilentProgress))
        .transpose()?
        .unwrap_or_else(|| project_root.to_path_buf());

    id.vpath().resolve(&root).ok_or(FileError::AccessDenied)
}

/// Read file from disk.
fn read_disk(path: &Path) -> FileResult<Vec<u8>> {
    let map_err = |e| FileError::from_io(e, path);
    fs::metadata(path).map_err(map_err).and_then(|m| {
        if m.is_dir() {
            Err(FileError::IsDirectory)
        } else {
            fs::read(path).map_err(map_err)
        }
    })
}

/// Read all data from stdin.
fn read_stdin() -> FileResult<Vec<u8>> {
    let mut buf = Vec::new();
    io::stdin()
        .read_to_end(&mut buf)
        .or_else(|e| {
            if e.kind() == io::ErrorKind::BrokenPipe {
                Ok(0)
            } else {
                Err(FileError::from_io(e, Path::new("<stdin>")))
            }
        })?;
    Ok(buf)
}

/// No-op progress reporter for silent package downloads.
struct SilentProgress;

impl Progress for SilentProgress {
    fn print_start(&mut self) {}
    fn print_progress(&mut self, _: &DownloadState) {}
    fn print_finish(&mut self, _: &DownloadState) {}
}

// =============================================================================
// FileSlot - Per-file Caching
// =============================================================================

/// Holds cached data for a file ID.
pub struct FileSlot {
    id: FileId,
    source: SlotCell<Source>,
    file: SlotCell<Bytes>,
}

impl FileSlot {
    /// Create a new file slot for the given ID.
    pub const fn new(id: FileId) -> Self {
        Self {
            id,
            source: SlotCell::new(),
            file: SlotCell::new(),
        }
    }

    /// Reset access flags for a new compilation.
    pub const fn reset_access(&mut self) {
        self.source.reset_access();
        self.file.reset_access();
    }

    /// Retrieve parsed source for this file (no virtual data).
    pub fn source(&mut self, project_root: &Path) -> FileResult<Source> {
        self.source_with_virtual(project_root, &NoVirtualData)
    }

    /// Retrieve parsed source using the global virtual data provider.
    ///
    /// This uses the provider registered via [`set_virtual_provider`].
    pub fn source_with_global_virtual(&mut self, project_root: &Path) -> FileResult<Source> {
        record_file_access(self.id);
        self.source.get_or_init(
            || read_with_global_virtual(self.id, project_root),
            |data, prev| {
                let text = decode_utf8(&data)?;
                match prev {
                    Some(mut src) => {
                        src.replace(text);
                        Ok(src)
                    }
                    None => Ok(Source::new(self.id, text.into())),
                }
            },
        )
    }

    /// Retrieve parsed source with virtual data support.
    pub fn source_with_virtual<V: VirtualDataProvider>(
        &mut self,
        project_root: &Path,
        virtual_provider: &V,
    ) -> FileResult<Source> {
        record_file_access(self.id);
        self.source.get_or_init(
            || read_with_virtual(self.id, project_root, virtual_provider),
            |data, prev| {
                let text = decode_utf8(&data)?;
                match prev {
                    Some(mut src) => {
                        src.replace(text);
                        Ok(src)
                    }
                    None => Ok(Source::new(self.id, text.into())),
                }
            },
        )
    }

    /// Retrieve raw bytes for this file (no virtual data).
    pub fn file(&mut self, project_root: &Path) -> FileResult<Bytes> {
        self.file_with_virtual(project_root, &NoVirtualData)
    }

    /// Retrieve raw bytes using the global virtual data provider.
    ///
    /// This uses the provider registered via [`set_virtual_provider`].
    pub fn file_with_global_virtual(&mut self, project_root: &Path) -> FileResult<Bytes> {
        record_file_access(self.id);
        self.file.get_or_init(
            || read_with_global_virtual(self.id, project_root),
            |data, _| Ok(Bytes::new(data)),
        )
    }

    /// Retrieve raw bytes with virtual data support.
    pub fn file_with_virtual<V: VirtualDataProvider>(
        &mut self,
        project_root: &Path,
        virtual_provider: &V,
    ) -> FileResult<Bytes> {
        record_file_access(self.id);
        self.file.get_or_init(
            || read_with_virtual(self.id, project_root, virtual_provider),
            |data, _| Ok(Bytes::new(data)),
        )
    }
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
    fn test_decode_utf8_valid() {
        let text = "Hello, 世界!";
        assert_eq!(decode_utf8(text.as_bytes()).unwrap(), text);
    }

    #[test]
    fn test_decode_utf8_strips_bom() {
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(b"Hello");
        assert_eq!(decode_utf8(&bytes).unwrap(), "Hello");
    }

    #[test]
    fn test_decode_utf8_invalid() {
        let invalid = vec![0xff, 0xfe];
        assert!(decode_utf8(&invalid).is_err());
    }

    #[test]
    fn test_read_disk() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "test content").unwrap();

        assert_eq!(read_disk(&path).unwrap(), b"test content");
    }

    #[test]
    fn test_read_disk_directory() {
        let dir = TempDir::new().unwrap();
        assert!(read_disk(dir.path()).is_err());
    }

    #[test]
    fn test_read_disk_nonexistent() {
        assert!(read_disk(Path::new("/nonexistent/file.txt")).is_err());
    }

    #[test]
    fn test_slot_cell_fingerprint() {
        let mut slot: SlotCell<String> = SlotCell::new();

        let result1 = slot.get_or_init(
            || Ok(b"hello".to_vec()),
            |data, _| Ok(String::from_utf8(data).unwrap()),
        );
        assert_eq!(result1.unwrap(), "hello");

        slot.accessed = false;
        let result2 = slot.get_or_init(
            || Ok(b"hello".to_vec()),
            |_, _| panic!("Should not reprocess"),
        );
        assert_eq!(result2.unwrap(), "hello");
    }

    #[test]
    fn test_file_slot_caching() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.typ");
        fs::write(&path, "= Hello").unwrap();

        let vpath = VirtualPath::new("test.typ");
        let id = FileId::new(None, vpath);
        let mut slot = FileSlot::new(id);

        let result1 = slot.file(dir.path());
        let result2 = slot.file(dir.path());

        assert!(result1.is_ok());
        assert_eq!(result1.unwrap(), result2.unwrap());
    }

    #[test]
    fn test_empty_id() {
        let dir = TempDir::new().unwrap();
        let result = read(*EMPTY_ID, dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
