//! Page compilation and processing.
//!
//! This module handles compiling `.typ` files to HTML:
//!
//! - [`process_page`] - Single page compilation for watch mode
//! - [`compile_meta`] - Compile and extract metadata
//! - [`write_page_html`] - Write compiled HTML to disk
//! - [`collect_metadata_smart`] - Phase 1: collect metadata, classify pages
//! - [`compile_dynamic_pages`] - Phase 2: recompile dynamic pages

mod collect;
mod compile;
mod write;

pub use collect::{collect_metadata_smart, compile_dynamic_pages};
pub use compile::process_page;
pub use write::write_page_html;

use std::sync::Mutex;

// ============================================================================
// Type Aliases
// ============================================================================

/// Indexed VDOM document for hot reload diffing
pub type IndexedDocument = crate::vdom::Document<crate::vdom::Indexed>;

/// Result of compile_meta: (html, metadata, indexed_vdom)
pub type CompileMetaResult = (Vec<u8>, Option<crate::compiler::meta::ContentMeta>, Option<IndexedDocument>);

/// Result of collect_metadata_smart: (dynamic_pages, static_count, dynamic_count)
pub type MetadataResult = (Vec<std::path::PathBuf>, usize, usize);

/// Result of page compilation
pub struct PageResult {
    /// Page metadata
    pub page: crate::compiler::meta::PageMeta,
    /// Indexed VDOM for diff comparison (only in development mode)
    pub indexed_vdom: Option<IndexedDocument>,
    /// URL path for the page (e.g., "/blog/post")
    pub url_path: String,
}

// ============================================================================
// Warnings Collection
// ============================================================================

/// Global warnings collector for compilation warnings.
///
/// Collects warnings (e.g., unknown font family) during compilation.
/// Call `drain_warnings()` after build to get and clear all warnings.
static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Add a warning to the global collector.
fn collect_warning(warning: String) {
    if let Ok(mut warnings) = WARNINGS.lock() {
        warnings.push(warning);
    }
}

/// Drain all collected warnings.
///
/// Returns all warnings and clears the collector.
/// Should be called after build completes to display warnings.
pub fn drain_warnings() -> Vec<String> {
    WARNINGS.lock().map(|mut w| std::mem::take(&mut *w)).unwrap_or_default()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::meta::{ContentMeta, PageMeta};
    use crate::config::SiteConfig;
    use compile::{compile_meta, is_draft};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Create a test config with the given content directory.
    fn make_test_config(content_dir: PathBuf, output_dir: PathBuf) -> SiteConfig {
        let mut config = SiteConfig::default();
        config.build.content = content_dir;
        config.build.output = output_dir;
        config
    }

    #[test]
    fn test_is_draft_none() {
        assert!(!is_draft(None));
    }

    #[test]
    fn test_is_draft_false() {
        let meta = ContentMeta {
            draft: false,
            ..Default::default()
        };
        assert!(!is_draft(Some(&meta)));
    }

    #[test]
    fn test_is_draft_true() {
        let meta = ContentMeta {
            draft: true,
            ..Default::default()
        };
        assert!(is_draft(Some(&meta)));
    }

    // Tests that use typst compilation

    #[test]
    fn test_compile_meta_no_label() {
        use crate::driver::Development;

        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");

        // File without <tola-meta> label
        fs::write(&file_path, "= Hello World").unwrap();

        let mut config = SiteConfig::default();
        config.set_root(dir.path());

        let result = compile_meta(&Development, &file_path, &config);
        assert!(result.is_ok(), "compile_meta should succeed: {:?}", result);

        let (html, meta, _indexed_vdom) = result.unwrap();
        assert!(!html.is_empty(), "HTML should not be empty");
        assert!(meta.is_none(), "Metadata should be None when no <tola-meta> label");
    }

    #[test]
    fn test_compile_meta_with_label() {
        use crate::driver::Development;

        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");

        fs::write(
            &file_path,
            r#"#metadata((
  title: "Test",
  author: "Author",
)) <tola-meta>

= Content
"#,
        )
        .unwrap();

        let mut config = SiteConfig::default();
        config.set_root(dir.path());

        let result = compile_meta(&Development, &file_path, &config);
        assert!(result.is_ok(), "compile_meta should succeed: {:?}", result);

        let (html, meta, _indexed_vdom) = result.unwrap();
        assert!(!html.is_empty());
        assert!(meta.is_some());

        let meta = meta.unwrap();
        assert_eq!(meta.title, Some("Test".to_string()));
        assert_eq!(meta.author, Some("Author".to_string()));
    }

    #[test]
    fn test_compile_meta_draft_field() {
        use crate::driver::Development;

        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.typ");

        fs::write(
            &file_path,
            r#"#metadata((
  title: "Draft Post",
  draft: true,
)) <tola-meta>

= Draft
"#,
        )
        .unwrap();

        let mut config = SiteConfig::default();
        config.set_root(dir.path());

        let result = compile_meta(&Development, &file_path, &config);
        assert!(result.is_ok());

        let (_, meta, _indexed_vdom) = result.unwrap();
        assert!(meta.is_some());
        assert!(is_draft(meta.as_ref()), "Should detect draft: true");
    }

    #[test]
    fn test_compile_error_returns_err() {
        use crate::driver::Development;

        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("invalid.typ");

        // Create an invalid typst file
        fs::write(&file_path, "#invalid-syntax-that-will-fail").unwrap();

        let mut config = SiteConfig::default();
        config.set_root(dir.path());

        let result = compile_meta(&Development, &file_path, &config);

        // Should return an error, not panic or silently skip
        assert!(result.is_err(), "Invalid typst should return Err");
    }

    #[test]
    fn test_page_meta_from_paths() {
        let dir = TempDir::new().unwrap();
        let content_dir = dir.path().join("content");
        let output_dir = dir.path().join("public");
        fs::create_dir_all(&content_dir).unwrap();

        // Create a dummy file
        let file_path = content_dir.join("test.typ");
        fs::write(&file_path, "= Test").unwrap();

        let config = make_test_config(content_dir.clone(), output_dir);
        let page = PageMeta::from_paths(file_path, &config);

        assert!(page.is_ok());
        let page = page.unwrap();
        assert_eq!(page.paths.relative, "test");
        assert!(page.paths.html.ends_with("test/index.html"));
    }

    #[test]
    fn test_page_meta_nested_path() {
        let dir = TempDir::new().unwrap();
        let content_dir = dir.path().join("content");
        let output_dir = dir.path().join("public");
        fs::create_dir_all(content_dir.join("posts")).unwrap();

        let file_path = content_dir.join("posts/hello.typ");
        fs::write(&file_path, "= Hello").unwrap();

        let config = make_test_config(content_dir.clone(), output_dir);
        let page = PageMeta::from_paths(file_path, &config);

        assert!(page.is_ok());
        let page = page.unwrap();
        assert_eq!(page.paths.relative, "posts/hello");
    }
}
