//! Global shared font management.
//!
//! Fonts are expensive to load (~100ms+), so we load them once at startup
//! and share across all compilations via `OnceLock`.
//!
//! # Design Rationale
//!
//! Font loading involves:
//! 1. Scanning system font directories (platform-specific)
//! 2. Parsing font metadata (family, weight, style, etc.)
//! 3. Building a searchable font book index
//!
//! This is done once and shared via `OnceLock` to ensure:
//! - Single initialization (first caller wins)
//! - Zero-cost subsequent access (just a pointer dereference)
//! - Thread-safe sharing across compilations
//!
//! # Font Sources
//!
//! Fonts are searched in order:
//! 1. Custom paths provided at initialization (e.g., project fonts)
//! 2. System fonts (if enabled)

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use typst::text::FontBook;
use typst::utils::LazyHash;
use typst_kit::fonts::Fonts;

/// Global shared fonts - initialized once with custom font paths.
///
/// Uses `OnceLock` for thread-safe, one-time initialization.
/// The first call to `get_fonts` determines the font paths for all
/// subsequent compilations.
static GLOBAL_FONTS: OnceLock<(Fonts, LazyHash<FontBook>)> = OnceLock::new();

// =============================================================================
// Font Configuration
// =============================================================================

/// Options for font initialization.
///
/// Use this to customize font loading behavior when calling [`init_fonts_with_options`].
///
/// # Example
///
/// ```ignore
/// use typst_batch::{FontOptions, init_fonts_with_options};
/// use std::path::Path;
///
/// let options = FontOptions::new()
///     .with_system_fonts(true)
///     .with_custom_paths(&[
///         Path::new("assets/fonts"),
///         Path::new("content/fonts"),
///     ]);
///
/// init_fonts_with_options(&options);
/// ```
#[derive(Debug, Clone, Default)]
pub struct FontOptions {
    /// Whether to include system fonts.
    pub include_system_fonts: bool,
    /// Custom font directories to search.
    pub custom_paths: Vec<PathBuf>,
}

impl FontOptions {
    /// Create new font options with default settings.
    ///
    /// Default:
    /// - System fonts: enabled
    /// - Custom paths: empty
    pub fn new() -> Self {
        Self {
            include_system_fonts: true,
            custom_paths: Vec::new(),
        }
    }

    /// Set whether to include system fonts.
    ///
    /// Disabling system fonts can speed up initialization in controlled
    /// environments where only specific fonts are needed.
    pub fn with_system_fonts(mut self, include: bool) -> Self {
        self.include_system_fonts = include;
        self
    }

    /// Set custom font paths to search.
    ///
    /// These directories are searched for `.ttf`, `.otf`, and other font files.
    pub fn with_custom_paths(mut self, paths: &[&Path]) -> Self {
        self.custom_paths = paths.iter().map(|p| p.to_path_buf()).collect();
        self
    }

    /// Add a single custom font path.
    pub fn add_path(mut self, path: impl AsRef<Path>) -> Self {
        self.custom_paths.push(path.as_ref().to_path_buf());
        self
    }
}

/// Sorting key for deterministic font ordering.
///
/// `fontdb` uses `std::fs::read_dir()` which does not guarantee order,
/// causing non-deterministic font indices across process runs.
/// This key ensures fonts are always ordered the same way.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FontSortKey {
    path: Option<PathBuf>,
    index: u32,
}

// =============================================================================
// Debug Utilities
// =============================================================================

/// **DEBUG ONLY**: Write font list to `/tmp/tola_fonts_debug.txt` for debugging.
///
/// This function is used to diagnose font loading issues, particularly:
/// - Non-deterministic font ordering across runs
/// - Duplicate fonts from different directories (e.g., `assets/` vs `public/`)
/// - Missing or unexpected fonts
///
/// # Output Format
///
/// ```text
/// === Font Debug Output (PID: 12345) ===
/// Total fonts: 977
///
///    0: Maple Mono | /path/to/font.otf | idx=0 | Normal-700-FontStretch(1000)
///    1: SF Pro | /System/Library/Fonts/SF-Pro.otf | idx=0 | Normal-400-FontStretch(1000)
/// ...
/// === End of Debug Output ===
/// ```
#[allow(dead_code)]
fn debug_dump_fonts(fonts: &Fonts) {
    use std::io::Write;
    let debug_path = std::path::Path::new("/tmp/tola_fonts_debug.txt");
    if let Ok(mut file) = std::fs::File::create(debug_path) {
        let _ = writeln!(
            file,
            "=== Font Debug Output (PID: {}) ===",
            std::process::id()
        );
        let _ = writeln!(file, "Total fonts: {}", fonts.fonts.len());
        let _ = writeln!(file);
        for (i, slot) in fonts.fonts.iter().enumerate() {
            let path = slot
                .path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "embedded".to_string());
            let info = fonts.book.info(i);
            let family = info.map(|i| i.family.as_str()).unwrap_or("?");
            let variant = info
                .map(|i| format!("{:?}", i.variant))
                .unwrap_or_else(|| "?".to_string());
            let _ = writeln!(
                file,
                "{:4}: {} | {} | idx={} | {}",
                i,
                family,
                path,
                slot.index(),
                variant
            );
        }
        let _ = writeln!(file);
        let _ = writeln!(file, "=== End of Debug Output ===");
        eprintln!(
            "[FONT DEBUG] Wrote {} fonts to {:?}",
            fonts.fonts.len(),
            debug_path
        );
    }
}

// =============================================================================
// Font Initialization
// =============================================================================

/// Initialize fonts with custom font paths (legacy API).
///
/// # Arguments
///
/// * `font_paths` - Directories to search for fonts (e.g., `[assets/, content/]`).
///   **Important**: Should NOT include output directory (e.g., `public/`) to avoid
///   loading duplicate fonts that cause non-deterministic behavior.
///
/// # Returns
///
/// A tuple of:
/// - `Fonts`: The font collection with lazy-loaded font data
/// - `LazyHash<FontBook>`: The font book index wrapped for comemo caching
fn init_fonts(font_paths: &[&Path]) -> (Fonts, LazyHash<FontBook>) {
    let options = FontOptions {
        include_system_fonts: true,
        custom_paths: font_paths.iter().map(|p| p.to_path_buf()).collect(),
    };
    init_fonts_impl(&options)
}

/// Initialize fonts with detailed options.
///
/// This is the implementation used by both [`init_fonts`] and [`init_fonts_with_options`].
fn init_fonts_impl(options: &FontOptions) -> (Fonts, LazyHash<FontBook>) {
    let mut searcher = Fonts::searcher();
    // Include system fonts if enabled
    searcher.include_system_fonts(options.include_system_fonts);

    // Convert PathBuf to &Path for the API
    let paths: Vec<&Path> = options.custom_paths.iter().map(|p| p.as_path()).collect();

    // Search custom paths and optionally system fonts
    let fonts = searcher.search_with(&paths);

    // DEBUG: Uncomment to dump font list for debugging
    // debug_dump_fonts(&fonts);

    // NOTE: Font sorting is currently disabled.
    // See `sort_fonts_deterministically` for details on when it's needed.
    // let fonts = sort_fonts_deterministically(fonts);

    // Wrap font book in LazyHash for comemo caching
    let book = LazyHash::new(fonts.book.clone());
    (fonts, book)
}

/// Initialize fonts with detailed options.
///
/// Use this for more control over font loading. Unlike [`get_fonts`], this
/// allows you to:
/// - Disable system font loading
/// - Specify exact font paths
///
/// # Arguments
///
/// * `options` - Font initialization options
///
/// # Returns
///
/// A static reference to the shared font collection and book.
///
/// # Note
///
/// Like [`get_fonts`], this function only initializes fonts once. Subsequent
/// calls return the already-initialized fonts, ignoring the options parameter.
pub fn init_fonts_with_options(options: &FontOptions) -> &'static (Fonts, LazyHash<FontBook>) {
    GLOBAL_FONTS.get_or_init(|| init_fonts_impl(options))
}

/// Check if fonts have been initialized.
///
/// Returns `true` if fonts have already been loaded via [`get_fonts`] or
/// [`init_fonts_with_options`].
pub fn fonts_initialized() -> bool {
    GLOBAL_FONTS.get().is_some()
}

/// Get the number of loaded fonts.
///
/// Returns `None` if fonts have not been initialized yet.
pub fn font_count() -> Option<usize> {
    GLOBAL_FONTS.get().map(|(fonts, _)| fonts.fonts.len())
}

/// Get the number of font families.
///
/// Returns `None` if fonts have not been initialized yet.
pub fn font_family_count() -> Option<usize> {
    GLOBAL_FONTS.get().map(|(_, book)| book.families().count())
}

// =============================================================================
// Font Sorting (Currently Disabled)
// =============================================================================

/// Sort fonts by (path, index) to ensure deterministic ordering.
///
/// # Background: The Non-Determinism Problem
///
/// `fontdb` uses `std::fs::read_dir()` to scan font directories, which does NOT
/// guarantee consistent ordering across runs. This causes font indices to vary:
///
/// ```text
/// Run 1: [SF Pro (idx=0), Helvetica (idx=1), Arial (idx=2)]
/// Run 2: [Arial (idx=0), SF Pro (idx=1), Helvetica (idx=2)]
/// ```
///
/// Typst uses these indices in SVG output (e.g., `font-family: f0, f1`), so
/// different indices → different SVG content → non-reproducible builds.
///
/// # Why This Is Currently Disabled
///
/// The root cause was fixed differently: instead of sorting fonts after loading,
/// we now only scan `assets/` and `content/` directories for fonts, excluding
/// the output directory (`public/`). This prevents:
///
/// 1. **Duplicate fonts**: `public/fonts/` contains copies of `assets/fonts/`,
///    causing the same font to be loaded twice with different paths.
///
/// 2. **Font count variation**: First build has N fonts, subsequent builds
///    have N+M fonts (where M = fonts copied to public/), changing all indices.
#[allow(dead_code)]
fn sort_fonts_deterministically(fonts: Fonts) -> Fonts {
    let n = fonts.fonts.len();
    if n == 0 {
        return fonts;
    }

    // Create (original_index, sort_key) pairs
    let mut indices: Vec<(usize, FontSortKey)> = fonts
        .fonts
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            (
                i,
                FontSortKey {
                    path: slot.path().map(|p| p.to_path_buf()),
                    index: slot.index(),
                },
            )
        })
        .collect();

    // Sort by (path, index)
    indices.sort_by(|a, b| a.1.cmp(&b.1));

    // Collect FontInfo in sorted order
    let sorted_infos: Vec<_> = indices
        .iter()
        .filter_map(|(old_idx, _)| fonts.book.info(*old_idx).cloned())
        .collect();

    // Rebuild FontBook from sorted infos
    let new_book = FontBook::from_infos(sorted_infos);

    // Reorder fonts Vec to match
    // We need to move FontSlots, but they're not Clone.
    // Use a permutation approach with Option<FontSlot>
    let mut old_fonts: Vec<Option<_>> = fonts.fonts.into_iter().map(Some).collect();
    let mut new_fonts = Vec::with_capacity(n);
    for (old_idx, _) in indices {
        if let Some(slot) = old_fonts[old_idx].take() {
            new_fonts.push(slot);
        }
    }

    Fonts {
        book: new_book,
        fonts: new_fonts,
    }
}

/// Get or initialize global fonts.
///
/// The first call determines the font paths used for all subsequent compilations.
/// This is intentional: fonts rarely change during a program's lifetime, and
/// sharing them saves ~100ms per compilation.
///
/// # Arguments
///
/// * `font_dirs` - Directories to search for fonts (e.g., `[assets/, content/]`).
///   Pass on the first call to include fonts from these directories.
///   Should NOT include output directory (e.g., `public/`) to avoid duplicates.
///
/// # Returns
///
/// A static reference to the shared font collection and book.
///
/// # Thread Safety
///
/// This function is thread-safe. If called concurrently, only one thread
/// performs initialization; others wait and receive the shared result.
pub fn get_fonts(font_dirs: &[&Path]) -> &'static (Fonts, LazyHash<FontBook>) {
    GLOBAL_FONTS.get_or_init(|| init_fonts(font_dirs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_fonts_initialized() {
        let fonts = get_fonts(&[]);
        // Should find at least some system fonts on most systems
        // Note: This test may fail in minimal container environments
        assert!(!fonts.0.fonts.is_empty(), "Should find system fonts");
    }

    #[test]
    fn test_font_book_not_empty() {
        let fonts = get_fonts(&[]);
        // FontBook should have indexed the fonts
        assert!(
            fonts.1.families().count() > 0,
            "Font book should have families"
        );
    }

    #[test]
    fn test_fonts_are_shared() {
        let fonts1 = get_fonts(&[]);
        let fonts2 = get_fonts(&[]);
        // Should return the same static reference
        assert!(std::ptr::eq(fonts1, fonts2), "Fonts should be shared");
    }

    #[test]
    fn test_subsequent_calls_ignore_path() {
        // First call initializes (may have been done by other tests)
        let fonts1 = get_fonts(&[]);
        // Second call with different path should return same fonts
        let fonts2 = get_fonts(&[Path::new("/nonexistent")]);
        assert!(
            std::ptr::eq(fonts1, fonts2),
            "Path ignored after initialization"
        );
    }
}
