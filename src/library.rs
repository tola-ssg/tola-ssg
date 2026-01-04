//! Global shared Typst standard library.
//!
//! The Typst standard library contains all built-in functions, types, and modules
//! available in Typst documents (e.g., `#text`, `#image`, `#table`, etc.).
//!
//! # Design Rationale
//!
//! Creating the standard library is relatively cheap, but we still share it
//! globally for consistency and to enable comemo caching (via `LazyHash`).
//!
//! For documents that need `sys.inputs`, use [`create_library_with_inputs`] to
//! create a custom library instance with the specified inputs.
//!
//! # HTML Feature
//!
//! The library is initialized with the `Html` feature enabled, which adds
//! HTML-specific functions like `#html.elem` for raw HTML output. This is
//! required for HTML export via `typst-html`.

use std::sync::LazyLock;

use typst::foundations::Dict;
use typst::utils::LazyHash;
use typst::{Feature, Features, Library, LibraryExt};

/// Global shared library - Typst's standard library with HTML feature enabled.
///
/// Uses `LazyLock` for thread-safe, one-time initialization on first access.
/// Wrapped in `LazyHash` for comemo caching (enables incremental compilation).
///
/// # Features Enabled
///
/// - `Feature::Html` - Enables HTML-specific functions for HTML export
pub static GLOBAL_LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| {
    let library = Library::builder()
        // Enable HTML feature for html export support
        .with_features(Features::from_iter([Feature::Html]))
        .build();
    // Wrap in LazyHash for comemo caching
    LazyHash::new(library)
});

/// Create a library with custom `sys.inputs`.
///
/// This creates a new `Library` instance with the specified inputs accessible
/// via `sys.inputs` in Typst documents. Use this when you need to pass
/// document-specific data that can be accessed within the Typst document.
///
/// # When to Use
///
/// - **Single document compilation**: Pass document-specific metadata (title, date, etc.)
/// - **Custom build variables**: Environment-specific settings
///
/// # Performance Note
///
/// Unlike [`GLOBAL_LIBRARY`], this creates a new library instance each time.
/// For batch compilation where all documents share the same inputs (or no inputs),
/// use the global library via standard [`compile_html`](crate::compile_html).
///
/// # Example
///
/// ```ignore
/// use typst::foundations::{Dict, IntoValue};
///
/// // Create inputs dictionary
/// let mut inputs = Dict::new();
/// inputs.insert("title".into(), "My Document".into_value());
/// inputs.insert("author".into(), "Alice".into_value());
///
/// // Create library with inputs
/// let library = create_library_with_inputs(inputs);
///
/// // Use with SystemWorld::with_library()
/// let world = SystemWorld::new(path, root).with_library(library);
/// ```
///
/// In your Typst document:
/// ```typst
/// #let title = sys.inputs.at("title", default: "Untitled")
/// #let author = sys.inputs.at("author", default: "Unknown")
///
/// = #title
/// by #author
/// ```
pub fn create_library_with_inputs(inputs: Dict) -> LazyHash<Library> {
    let library = Library::builder()
        .with_inputs(inputs)
        .with_features(Features::from_iter([Feature::Html]))
        .build();
    LazyHash::new(library)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_initialized() {
        // Should not panic on access
        let _lib = &*GLOBAL_LIBRARY;
    }

    #[test]
    fn test_library_has_global_scope() {
        let lib = &*GLOBAL_LIBRARY;
        // The library should have a global scope with standard functions
        let scope = lib.global.scope();
        // Check for some standard typst functions
        assert!(scope.get("text").is_some(), "Should have text function");
        assert!(scope.get("image").is_some(), "Should have image function");
    }

    #[test]
    fn test_library_is_shared() {
        let lib1 = &*GLOBAL_LIBRARY;
        let lib2 = &*GLOBAL_LIBRARY;
        // Should return the same static reference
        assert!(std::ptr::eq(lib1, lib2), "Library should be shared");
    }
}
