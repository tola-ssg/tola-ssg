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

use crate::config::package_storage;

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
// FileId Helper Functions
// =============================================================================

/// Create a `FileId` for a project file.
///
/// This is the standard way to create file IDs for files within your project.
/// The path should be root-relative (e.g., `/content/post.typ`).
///
/// # Arguments
///
/// * `path` - Path relative to project root, with leading `/`
///
/// # Example
///
/// ```ignore
/// use typst_batch::file_id;
///
/// let id = file_id("/content/post.typ");
/// ```
pub fn file_id(path: impl AsRef<Path>) -> FileId {
    FileId::new(None, VirtualPath::new(path.as_ref()))
}

/// Create a `FileId` for a file from its absolute path within a project root.
///
/// Returns `None` if the file is outside the root directory.
///
/// # Arguments
///
/// * `file_path` - Absolute path to the file
/// * `root` - Absolute path to the project root
///
/// # Example
///
/// ```ignore
/// use typst_batch::file_id_from_path;
/// use std::path::Path;
///
/// let id = file_id_from_path(
///     Path::new("/project/content/post.typ"),
///     Path::new("/project"),
/// );
/// ```
pub fn file_id_from_path(file_path: &Path, root: &Path) -> Option<FileId> {
    VirtualPath::within_root(file_path, root).map(|vpath| FileId::new(None, vpath))
}

/// Create a virtual/fake `FileId` for dynamically generated content.
///
/// Use this for content that doesn't correspond to a real file on disk.
/// Each call creates a unique ID that won't conflict with real file IDs.
///
/// # Arguments
///
/// * `name` - A descriptive name for the virtual file
///
/// # Example
///
/// ```ignore
/// use typst_batch::virtual_file_id;
///
/// let id = virtual_file_id("<generated-data>");
/// ```
pub fn virtual_file_id(name: &str) -> FileId {
    FileId::new_fake(VirtualPath::new(name))
}

// =============================================================================
// Virtual File System
// =============================================================================

/// Trait for providing virtual files that don't exist on disk.
///
/// This is the primary extension point for batch compilation scenarios.
/// Implement this trait to inject dynamically generated content into
/// Typst's file system.
///
/// # Use Cases
///
/// - **Data injection**: Provide `/_data/posts.json` with blog post metadata
/// - **Configuration**: Inject site configuration without physical files
/// - **Template variables**: Provide computed values accessible via `json()`
/// - **Asset manifests**: Generate asset URLs at compile time
///
/// # Example
///
/// ```ignore
/// use typst_batch::{VirtualFileSystem, set_virtual_fs};
/// use std::path::Path;
///
/// struct MyVirtualFS {
///     site_config: String,
/// }
///
/// impl VirtualFileSystem for MyVirtualFS {
///     fn read(&self, path: &Path) -> Option<Vec<u8>> {
///         match path.to_str()? {
///             "/_data/site.json" => Some(self.site_config.as_bytes().to_vec()),
///             "/_data/build-time.txt" => {
///                 Some(chrono::Utc::now().to_rfc3339().into_bytes())
///             }
///             _ => None, // Fall back to real filesystem
///         }
///     }
/// }
///
/// set_virtual_fs(MyVirtualFS { site_config: r#"{"title":"My Blog"}"#.into() });
/// ```
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` as compilation may run in parallel.
/// The provider is called for each file access, so implementations should be
/// efficient (consider caching expensive computations).
///
/// # Note on Global State
///
/// Due to Typst's `World` trait design, the virtual file system must be
/// registered globally via [`set_virtual_fs`]. This is a limitation of
/// the underlying architecture, not a design choice.
pub trait VirtualFileSystem: Send + Sync {
    /// Read a virtual file by path.
    ///
    /// Return `Some(bytes)` to provide virtual content, or `None` to fall
    /// back to the real filesystem.
    ///
    /// The path is root-relative (e.g., `/_data/config.json` or `/assets/style.css`).
    fn read(&self, path: &Path) -> Option<Vec<u8>>;
}

/// No-op virtual file system (all files from real filesystem).
pub struct NoVirtualFS;

impl VirtualFileSystem for NoVirtualFS {
    fn read(&self, _path: &Path) -> Option<Vec<u8>> {
        None
    }
}

/// A simple map-based virtual file system.
///
/// This provides a convenient way to inject virtual files without implementing
/// the [`VirtualFileSystem`] trait manually.
///
/// # Example
///
/// ```ignore
/// use typst_batch::{MapVirtualFS, set_virtual_fs};
///
/// let mut vfs = MapVirtualFS::new();
/// vfs.insert("/_data/site.json", r#"{"title":"My Blog"}"#);
/// vfs.insert("/_data/posts.json", serde_json::to_string(&posts)?);
///
/// set_virtual_fs(vfs);
/// ```
#[derive(Default, Clone)]
pub struct MapVirtualFS {
    files: FxHashMap<String, Vec<u8>>,
}

impl MapVirtualFS {
    /// Create a new empty virtual file system.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a virtual file with string content.
    ///
    /// The path should be root-relative (e.g., `/_data/config.json`).
    pub fn insert(&mut self, path: impl Into<String>, content: impl AsRef<str>) {
        self.files
            .insert(path.into(), content.as_ref().as_bytes().to_vec());
    }

    /// Insert a virtual file with binary content.
    pub fn insert_bytes(&mut self, path: impl Into<String>, content: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), content.into());
    }

    /// Check if a path exists in the virtual file system.
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// Remove a virtual file.
    pub fn remove(&mut self, path: &str) -> Option<Vec<u8>> {
        self.files.remove(path)
    }

    /// Get the number of virtual files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Check if the virtual file system is empty.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Iterate over all virtual file paths.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

impl VirtualFileSystem for MapVirtualFS {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        let path_str = path.to_str()?;
        self.files.get(path_str).cloned()
    }
}

// Legacy type alias for backward compatibility
#[doc(hidden)]
pub type VirtualDataProvider = dyn VirtualFileSystem;

// =============================================================================
// Global Virtual File System
// =============================================================================

/// Global virtual file system instance.
///
/// This allows the main application to register a custom virtual file system
/// that will be used by all file access operations during compilation.
static GLOBAL_VIRTUAL_FS: LazyLock<RwLock<Box<dyn VirtualFileSystem>>> =
    LazyLock::new(|| RwLock::new(Box::new(NoVirtualFS)));

/// Set the global virtual file system.
///
/// Call this at application startup to enable virtual files.
/// The virtual file system will be consulted for every file access.
/// Return `Some(bytes)` from your implementation to provide virtual content,
/// or `None` to fall back to the real filesystem.
///
/// # Example
///
/// ```ignore
/// use typst_batch::{VirtualFileSystem, set_virtual_fs};
/// use std::path::Path;
///
/// struct SiteData {
///     posts: Vec<Post>,
/// }
///
/// impl VirtualFileSystem for SiteData {
///     fn read(&self, path: &Path) -> Option<Vec<u8>> {
///         if path == Path::new("/_data/posts.json") {
///             Some(serde_json::to_vec(&self.posts).unwrap())
///         } else {
///             None
///         }
///     }
/// }
///
/// set_virtual_fs(SiteData { posts: vec![...] });
/// ```
pub fn set_virtual_fs<V: VirtualFileSystem + 'static>(fs: V) {
    *GLOBAL_VIRTUAL_FS.write() = Box::new(fs);
}

/// Read a file, checking virtual file system first.
///
/// Returns virtual content if the VFS provides it, otherwise returns `None`
/// to indicate the real filesystem should be used.
pub fn read_virtual(path: &Path) -> Option<Vec<u8>> {
    GLOBAL_VIRTUAL_FS.read().read(path)
}

/// Check if a path has virtual content available.
pub fn is_virtual_path(path: &Path) -> bool {
    GLOBAL_VIRTUAL_FS.read().read(path).is_some()
}

// Legacy function alias for backward compatibility
#[doc(hidden)]
pub fn set_virtual_provider<V: VirtualFileSystem + 'static>(provider: V) {
    set_virtual_fs(provider);
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

impl<T: Clone> Default for SlotCell<T> {
    fn default() -> Self {
        Self::new()
    }
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
pub fn read(id: FileId, project_root: &Path) -> FileResult<Vec<u8>> {
    read_with_virtual(id, project_root, &NoVirtualFS)
}

/// Read file content using the global virtual file system.
///
/// This function uses the globally registered virtual file system.
/// Call [`set_virtual_fs`] at startup to register your VFS.
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

    // Resolve path and read from disk
    let path = resolve_path(project_root, id)?;
    read_disk(&path)
}

/// Read file content with virtual data support.
///
/// Like [`read`], but also handles virtual data files via the provider.
pub fn read_with_virtual<V: VirtualFileSystem>(
    id: FileId,
    project_root: &Path,
    virtual_fs: &V,
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
    if let Some(data) = virtual_fs.read(vpath) {
        record_file_access(id);
        return Ok(data);
    }

    // Resolve path and read from disk
    let path = resolve_path(project_root, id)?;
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
        self.source_with_virtual(project_root, &NoVirtualFS)
    }

    /// Retrieve parsed source using the global virtual file system.
    ///
    /// This uses the VFS registered via [`set_virtual_fs`].
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

    /// Retrieve parsed source with virtual file system support.
    pub fn source_with_virtual<V: VirtualFileSystem>(
        &mut self,
        project_root: &Path,
        virtual_fs: &V,
    ) -> FileResult<Source> {
        record_file_access(self.id);
        self.source.get_or_init(
            || read_with_virtual(self.id, project_root, virtual_fs),
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
        self.file_with_virtual(project_root, &NoVirtualFS)
    }

    /// Retrieve raw bytes using the global virtual file system.
    ///
    /// This uses the VFS registered via [`set_virtual_fs`].
    pub fn file_with_global_virtual(&mut self, project_root: &Path) -> FileResult<Bytes> {
        record_file_access(self.id);
        self.file.get_or_init(
            || read_with_global_virtual(self.id, project_root),
            |data, _| Ok(Bytes::new(data)),
        )
    }

    /// Retrieve raw bytes with virtual file system support.
    pub fn file_with_virtual<V: VirtualFileSystem>(
        &mut self,
        project_root: &Path,
        virtual_fs: &V,
    ) -> FileResult<Bytes> {
        record_file_access(self.id);
        self.file.get_or_init(
            || read_with_virtual(self.id, project_root, virtual_fs),
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
