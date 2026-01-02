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
//!
//! # Usage
//!
//! ```ignore
//! // Initialize with project fonts
//! let fonts = get_fonts(Some(Path::new("/project/fonts")));
//!
//! // Access font book
//! let book: &FontBook = &fonts.1;
//!
//! // Get font by index
//! if let Some(font) = fonts.0.fonts.get(0) {
//!     let font: Font = font.get()?;
//! }
//! ```

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

/// Sorting key for deterministic font ordering.
///
/// `fontdb` uses `std::fs::read_dir()` which does not guarantee order,
/// causing non-deterministic font indices across process runs.
/// This key ensures fonts are always ordered the same way.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FontSortKey {
    path: Option<PathBuf>,
    index: u32,
}

/// Initialize fonts with custom font paths.
///
/// # Arguments
///
/// * `font_paths` - Additional directories to search for fonts
///
/// # Returns
///
/// A tuple of:
/// - `Fonts`: The font collection with lazy-loaded font data
/// - `LazyHash<FontBook>`: The font book index wrapped for comemo caching
///
/// # Determinism
///
/// Fonts are sorted by (path, index) to ensure deterministic ordering
/// across different process runs. This is necessary because `fontdb`
/// uses `read_dir()` which has filesystem-dependent ordering.
fn init_fonts(font_paths: &[&Path]) -> (Fonts, LazyHash<FontBook>) {
    let mut searcher = Fonts::searcher();
    // Include system fonts (platform-specific locations)
    searcher.include_system_fonts(true);
    // Search custom paths and system fonts
    let mut fonts = searcher.search_with(font_paths);

    // Sort fonts for deterministic ordering
    let sorted_fonts = sort_fonts_deterministically(fonts);
    fonts = sorted_fonts;

    // Wrap font book in LazyHash for comemo caching
    let book = LazyHash::new(fonts.book.clone());
    (fonts, book)
}

/// Sort fonts by (path, index) to ensure deterministic ordering.
///
/// This fixes non-determinism caused by `fontdb` using `read_dir()`.
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
/// * `font_path` - Optional project root to include project-specific fonts.
///   Pass `Some(root)` on the first call to include fonts from `{root}/fonts/`.
///
/// # Returns
///
/// A static reference to the shared font collection and book.
///
/// # Thread Safety
///
/// This function is thread-safe. If called concurrently, only one thread
/// performs initialization; others wait and receive the shared result.
pub fn get_fonts(font_path: Option<&Path>) -> &'static (Fonts, LazyHash<FontBook>) {
    GLOBAL_FONTS.get_or_init(|| {
        font_path.map_or_else(|| init_fonts(&[]), |path| init_fonts(&[path]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_fonts_initialized() {
        let fonts = get_fonts(None);
        // Should find at least some system fonts on most systems
        // Note: This test may fail in minimal container environments
        assert!(!fonts.0.fonts.is_empty(), "Should find system fonts");
    }

    #[test]
    fn test_font_book_not_empty() {
        let fonts = get_fonts(None);
        // FontBook should have indexed the fonts
        assert!(fonts.1.families().count() > 0, "Font book should have families");
    }

    #[test]
    fn test_fonts_are_shared() {
        let fonts1 = get_fonts(None);
        let fonts2 = get_fonts(None);
        // Should return the same static reference
        assert!(std::ptr::eq(fonts1, fonts2), "Fonts should be shared");
    }

    #[test]
    fn test_subsequent_calls_ignore_path() {
        // First call initializes (may have been done by other tests)
        let fonts1 = get_fonts(None);
        // Second call with different path should return same fonts
        let fonts2 = get_fonts(Some(Path::new("/nonexistent")));
        assert!(std::ptr::eq(fonts1, fonts2), "Path ignored after initialization");
    }
}
