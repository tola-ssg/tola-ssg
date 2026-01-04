//! `SystemWorld` implementation - the core World trait.
//!
//! This module implements Typst's `World` trait, which provides the compilation
//! environment for Typst documents. The `SystemWorld` is a lightweight per-compilation
//! state that references globally shared resources (fonts, packages, library).
//!
//! # Architecture
//!
//! ```text
//! SystemWorld (per-compilation, ~lightweight)
//! ├── root: PathBuf          // Project root for path resolution
//! ├── main: FileId           // Entry point file ID
//! ├── fonts: &'static Fonts  // → Global shared fonts
//! ├── library: LibraryRef    // → Global or custom library
//! └── now: Now               // Lazy datetime
//!
//! World trait methods:
//! ├── library() → &GLOBAL_LIBRARY or custom
//! ├── book()    → &fonts.book
//! ├── main()    → main FileId
//! ├── source()  → FileSlot cache
//! ├── file()    → FileSlot cache
//! ├── font()    → fonts.fonts[index]
//! └── today()   → Now (lazy UTC)
//! ```
//!
//! # Performance
//!
//! Creating a `SystemWorld` is cheap because:
//! - Fonts, packages, and library are globally shared (static references)
//! - File cache is global (not per-instance)
//! - Datetime is lazily computed on first access
//!
//! # Custom Inputs (sys.inputs)
//!
//! For documents that need `sys.inputs`, use [`SystemWorld::with_inputs`]
//! or create a custom library with [`crate::library::create_library_with_inputs`].

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{DateTime, Datelike, FixedOffset, Local, Utc};
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime, Dict};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_kit::fonts::Fonts;

use crate::file::{FileSlot, VirtualFileSystem, GLOBAL_FILE_CACHE};
use crate::font::get_fonts;
use crate::library::{create_library_with_inputs, GLOBAL_LIBRARY};

// =============================================================================
// Path Utilities
// =============================================================================

/// Normalize a file system path to absolute form.
///
/// Tries `canonicalize()` first (resolves symlinks, `.`, `..`).
/// Falls back to:
/// - Return as-is if already absolute
/// - Join with current directory if relative
#[inline]
fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
        }
    })
}

// =============================================================================
// DateTime handling
// =============================================================================

/// Lazy-captured datetime for consistent `World::today()` within a compilation.
///
/// The current time is captured on first access and reused for consistency.
struct LazyNow(OnceLock<DateTime<Utc>>);

// =============================================================================
// Library Reference
// =============================================================================

/// Reference to either global shared library or a custom library instance.
///
/// This allows `SystemWorld` to efficiently use the global library for batch
/// compilation, while supporting custom libraries with `sys.inputs` for
/// single document compilation.
enum LibraryRef {
    /// Reference to the global shared library (no inputs).
    Global,
    /// Custom library instance with specific inputs.
    Custom(LazyHash<Library>),
}

impl LibraryRef {
    /// Get a reference to the underlying library.
    fn get(&self) -> &LazyHash<Library> {
        match self {
            Self::Global => &GLOBAL_LIBRARY,
            Self::Custom(lib) => lib,
        }
    }
}

// =============================================================================
// SystemWorld
// =============================================================================

/// A world that provides access to the operating system.
///
/// This struct is cheap to create because all expensive resources (fonts,
/// packages, library, file cache) are globally shared.
///
/// # Thread Safety
///
/// `SystemWorld` is `Send + Sync` because all mutable state is behind
/// thread-safe locks in global statics.
///
/// # Custom Inputs
///
/// By default, `SystemWorld` uses the global shared library (no `sys.inputs`).
/// To provide custom inputs, use [`SystemWorld::with_inputs`]:
///
/// ```ignore
/// let world = SystemWorld::new(path, root)
///     .with_inputs([("title", "Hello"), ("author", "Alice")]);
/// ```
pub struct SystemWorld {
    /// The root relative to which absolute paths are resolved.
    /// This is typically the project directory containing `tola.toml`.
    root: PathBuf,

    /// The input path (main entry point).
    /// This is the `FileId` of the file being compiled.
    main: FileId,

    /// Reference to global fonts (initialized on first use).
    /// This is a static reference to avoid allocation per compilation.
    fonts: &'static (Fonts, LazyHash<FontBook>),

    /// Library reference - either global or custom with inputs.
    library: LibraryRef,

    /// The current datetime if requested.
    /// Lazily initialized to ensure consistent time throughout compilation.
    now: LazyNow,
}

impl SystemWorld {
    /// Create a new world for compiling a specific file.
    ///
    /// This is cheap because fonts/packages/library/file-cache are globally shared.
    /// No per-instance allocation is needed.
    ///
    /// # Arguments
    ///
    /// * `entry_file` - Path to the `.typ` file to compile
    /// * `root_dir` - Project root directory for resolving imports
    ///
    /// # Returns
    ///
    /// A new `SystemWorld` ready for compilation.
    pub fn new(entry_file: &Path, root_dir: &Path) -> Self {
        // Canonicalize root path for consistent path resolution
        let root = normalize_path(root_dir);

        // Resolve the virtual path of the main file within the project root.
        // Virtual paths are root-relative and use forward slashes.
        let entry_abs = normalize_path(entry_file);
        let virtual_path = VirtualPath::within_root(&entry_abs, &root)
            .unwrap_or_else(|| VirtualPath::new(entry_file.file_name().unwrap()));
        let main = FileId::new(None, virtual_path);

        // Get global fonts. Fonts are already initialized via warmup_with_font_dirs().
        // If not yet initialized, this returns an empty font set.
        let fonts = get_fonts(&[]);

        Self {
            root,
            main,
            fonts,
            library: LibraryRef::Global,
            now: LazyNow(OnceLock::new()),
        }
    }

    /// Configure `sys.inputs` for the compilation.
    ///
    /// This creates a custom library with the specified inputs accessible
    /// via `sys.inputs` in Typst documents.
    ///
    /// # Arguments
    ///
    /// * `inputs` - Key-value pairs to make available as `sys.inputs`
    ///
    /// # Example
    ///
    /// ```ignore
    /// use typst_batch::world::SystemWorld;
    /// use std::path::Path;
    ///
    /// let world = SystemWorld::new(Path::new("doc.typ"), Path::new("."))
    ///     .with_inputs([("title", "Hello"), ("author", "Alice")]);
    /// ```
    ///
    /// In your Typst document:
    /// ```typst
    /// #let title = sys.inputs.at("title", default: "Untitled")
    /// = #title
    /// ```
    ///
    /// # Performance Note
    ///
    /// Using this method creates a new library instance, bypassing the global
    /// shared library. For batch compilation without inputs, use [`SystemWorld::new`].
    pub fn with_inputs<I, K, V>(mut self, inputs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<typst::foundations::Str>,
        V: typst::foundations::IntoValue,
    {

        let dict: Dict = inputs
            .into_iter()
            .map(|(k, v)| (k.into(), v.into_value()))
            .collect();

        self.library = LibraryRef::Custom(create_library_with_inputs(dict));
        self
    }

    /// Configure `sys.inputs` from a pre-built `Dict`.
    ///
    /// This is useful when you already have a `Dict` of inputs.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use typst::foundations::{Dict, IntoValue};
    ///
    /// let mut inputs = Dict::new();
    /// inputs.insert("version".into(), "1.0".into_value());
    ///
    /// let world = SystemWorld::new(path, root).with_inputs_dict(inputs);
    /// ```
    pub fn with_inputs_dict(mut self, inputs: Dict) -> Self {
        self.library = LibraryRef::Custom(create_library_with_inputs(inputs));
        self
    }

    /// Configure a custom library for the compilation.
    ///
    /// Use this for full control over the library configuration.
    /// Create a custom library with [`crate::library::create_library_with_inputs`].
    pub fn with_library(mut self, library: LazyHash<Library>) -> Self {
        self.library = LibraryRef::Custom(library);
        self
    }

    /// Get the project root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Access the canonical slot for the given file id from global cache.
    ///
    /// Creates a new slot if one doesn't exist. The callback receives
    /// mutable access to the slot for reading source/file data.
    #[allow(clippy::unused_self)]
    fn slot<F, T>(&self, id: FileId, f: F) -> T
    where
        F: FnOnce(&mut FileSlot) -> T,
    {
        let mut cache = GLOBAL_FILE_CACHE.write();
        f(cache.entry(id).or_insert_with(|| FileSlot::new(id)))
    }

    /// Access a file slot with virtual file system support.
    #[allow(clippy::unused_self)]
    fn slot_virtual<V, F, T>(&self, id: FileId, virtual_fs: &V, f: F) -> T
    where
        V: VirtualFileSystem,
        F: FnOnce(&mut FileSlot, &V) -> T,
    {
        let mut cache = GLOBAL_FILE_CACHE.write();
        let slot = cache.entry(id).or_insert_with(|| FileSlot::new(id));
        f(slot, virtual_fs)
    }

    /// Load source with virtual file system support.
    pub fn source_with_virtual<V: VirtualFileSystem>(
        &self,
        id: FileId,
        virtual_fs: &V,
    ) -> FileResult<Source> {
        self.slot_virtual(id, virtual_fs, |slot, vfs| {
            slot.source_with_virtual(&self.root, vfs)
        })
    }

    /// Load file with virtual file system support.
    pub fn file_with_virtual<V: VirtualFileSystem>(
        &self,
        id: FileId,
        virtual_fs: &V,
    ) -> FileResult<Bytes> {
        self.slot_virtual(id, virtual_fs, |slot, vfs| {
            slot.file_with_virtual(&self.root, vfs)
        })
    }
}

/// Implementation of Typst's `World` trait.
///
/// This trait provides the compilation environment:
/// - Standard library access
/// - Font discovery
/// - File system access
/// - Package management (via file resolution)
/// - Current date/time
impl World for SystemWorld {
    /// Returns the standard library.
    ///
    /// Returns either the global shared library or a custom library
    /// with `sys.inputs` if configured via [`SystemWorld::with_inputs`].
    fn library(&self) -> &LazyHash<Library> {
        self.library.get()
    }

    /// Returns the font book for font lookup.
    ///
    /// The font book indexes all available fonts for name-based lookup.
    fn book(&self) -> &LazyHash<FontBook> {
        &self.fonts.1
    }

    /// Returns the main source file ID.
    ///
    /// This is the entry point for compilation.
    fn main(&self) -> FileId {
        self.main
    }

    /// Load a source file by ID.
    ///
    /// Returns the parsed source code, using the file slot cache
    /// for incremental compilation.
    ///
    /// Uses the global virtual data provider registered via
    /// [`crate::file::set_virtual_provider`].
    fn source(&self, id: FileId) -> FileResult<Source> {
        self.slot(id, |slot| slot.source_with_global_virtual(&self.root))
    }

    /// Load a file's raw bytes by ID.
    ///
    /// Used for binary files (images, etc.) that don't need parsing.
    ///
    /// Uses the global virtual data provider registered via
    /// [`crate::file::set_virtual_provider`].
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.slot(id, |slot| slot.file_with_global_virtual(&self.root))
    }

    /// Load a font by index.
    ///
    /// Fonts are indexed in the order they were discovered during
    /// font search. The index comes from font book lookups.
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.0.fonts.get(index)?.get()
    }

    /// Get the current date.
    ///
    /// Returns the date at the time of first access within this compilation.
    /// The time is captured once and reused for consistency.
    ///
    /// # Arguments
    ///
    /// * `offset` - Optional UTC offset in hours. If `None`, uses local timezone.
    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        let now = self.now.0.get_or_init(Utc::now);

        // Apply timezone offset
        let with_offset = match offset {
            None => now.with_timezone(&Local).fixed_offset(),
            Some(hours) => {
                let seconds = i32::try_from(hours).ok()?.checked_mul(3600)?;
                now.with_timezone(&FixedOffset::east_opt(seconds)?)
            }
        };

        // Convert to Typst's Datetime type
        Datetime::from_ymd(
            with_offset.year(),
            with_offset.month().try_into().ok()?,
            with_offset.day().try_into().ok()?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_create_system_world() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");
        fs::write(&file_path, "= Hello").unwrap();

        let _world = SystemWorld::new(&file_path, dir.path());
        // If we get here without panicking, the world was created successfully
    }

    #[test]
    fn test_world_library_access() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");
        fs::write(&file_path, "= Hello").unwrap();

        let world = SystemWorld::new(&file_path, dir.path());
        let _lib = world.library();
        // Should not panic
    }

    #[test]
    fn test_world_book_access() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");
        fs::write(&file_path, "= Hello").unwrap();

        let world = SystemWorld::new(&file_path, dir.path());
        let book = world.book();
        // Should have some fonts
        assert!(book.families().count() > 0);
    }

    #[test]
    fn test_world_main_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");
        fs::write(&file_path, "= Hello").unwrap();

        let world = SystemWorld::new(&file_path, dir.path());
        let main = world.main();
        // Main file should have the correct virtual path
        assert!(main.vpath().as_rootless_path().ends_with("test.typ"));
    }

    #[test]
    fn test_world_source_loading() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");
        fs::write(&file_path, "= Hello World").unwrap();

        let world = SystemWorld::new(&file_path, dir.path());
        let source = world.source(world.main());
        assert!(source.is_ok());
        assert!(source.unwrap().text().contains("Hello World"));
    }

    #[test]
    fn test_world_today() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");
        fs::write(&file_path, "= Hello").unwrap();

        let world = SystemWorld::new(&file_path, dir.path());

        // Test with local timezone
        let today = world.today(None);
        assert!(today.is_some());

        // Test with UTC offset
        let today_utc = world.today(Some(0));
        assert!(today_utc.is_some());
    }

    #[test]
    fn test_world_font_access() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");
        fs::write(&file_path, "= Hello").unwrap();

        let world = SystemWorld::new(&file_path, dir.path());

        // Should be able to access at least one font
        let font = world.font(0);
        // May be None in minimal environments, but shouldn't panic
        let _ = font;
    }
}
