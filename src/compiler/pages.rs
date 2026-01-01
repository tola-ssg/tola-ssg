use crate::compiler::meta::{PageMeta, ContentMeta, Pages, TOLA_META_LABEL};
use crate::compiler::{collect_all_files, is_up_to_date};
use crate::data::{PageData, GLOBAL_SITE_DATA};
use crate::freshness::{self, ContentHash};
use crate::utils::minify::{minify, MinifyType};
use crate::utils::xml::process_html;
use crate::{config::SiteConfig, log, typst_lib};
use anyhow::Result;
use std::fs;
use std::path::Path;
use rayon::prelude::*;

// TOLA_META_LABEL is imported from crate::compiler::meta

// ============================================================================
// Public API
// ============================================================================

/// Compile all pages and write HTML files.
///
/// # Behavior
///
/// - **Lib mode**: Uses pre-compiled HTML from `page.compiled_html` (set by `collect_pages`).
/// - **CLI mode**: Compiles each page using `typst compile` CLI.
///
/// Skips pages that are up-to-date (unless `clean` is true).
/// Calls `on_progress` after each page is processed.
///
/// Note: This is the legacy single-phase compilation. For two-phase compilation
/// with virtual data support, use `collect_metadata` + `compile_pages`.
#[allow(dead_code)]
fn compile_pages_legacy(
    pages: &Pages,
    config: &SiteConfig,
    clean: bool,
    deps_hash: Option<ContentHash>,
    on_progress: impl Fn() + Sync,
) -> Result<()> {
    pages.items.par_iter().try_for_each(|page| {
        let result = write_page(page, config, clean, deps_hash, false);
        on_progress();
        result
    })
}

// ============================================================================
// process_page: Single page compilation (for watch mode)
// ============================================================================
// Page Processing with Driver
// ============================================================================

/// Result of page compilation
pub struct PageResult {
    /// Page metadata
    pub page: PageMeta,
    /// Indexed VDOM for diff comparison (only in development mode)
    pub indexed_vdom: Option<crate::vdom::Document<crate::vdom::Indexed>>,
    /// URL path for the page (e.g., "/blog/post")
    pub url_path: String,
}

/// Process a single .typ file.
///
/// The driver controls:
/// - `emit_ids()`: Whether to output `data-tola-id` attributes
/// - `cache_vdom()`: Whether to return indexed VDOM for hot reload
///
/// Note: This function does NOT write the HTML file to disk.
/// The caller should decide whether to write based on diff results.
pub fn process_page<D: crate::driver::BuildDriver>(
    driver: &D,
    path: &Path,
    config: &SiteConfig,
) -> Result<Option<PageResult>> {
    let mut page = PageMeta::from_paths(path.to_path_buf(), config)?;

    // Compile with driver
    let result = typst_lib::compile_vdom(driver, path, config.get_root(), TOLA_META_LABEL)?;

    // Extract metadata
    let content_meta: Option<ContentMeta> = result
        .metadata
        .and_then(|json| serde_json::from_value(json).ok());

    // Skip drafts
    if is_draft(content_meta.as_ref()) {
        return Ok(None);
    }

    // Record dependencies
    super::deps::DEPENDENCY_GRAPH
        .write()
        .record_dependencies(path, &result.accessed_files);

    page.content_meta = content_meta;
    page.compiled_html = Some(result.html);
    let url_path = page.paths.url_path.clone();

    // Update global site data
    GLOBAL_SITE_DATA.insert_page(page_meta_to_data(&page));

    // NOTE: Do NOT update VDOM_CACHE here!
    // The caller (watch.rs) is responsible for updating the cache AFTER
    // successfully sending patches or triggering reload.
    // This ensures VDOM_CACHE stays in sync with what the browser actually displays.

    Ok(Some(PageResult {
        page,
        indexed_vdom: result.indexed_vdom,
        url_path,
    }))
}

/// Write a page's HTML to disk.
pub fn write_page_html(page: &PageMeta, config: &SiteConfig) -> Result<()> {
    write_page(page, config, true, None, false)
}

// ============================================================================
// Internal
// ============================================================================

/// Write a page's HTML to disk.
fn write_page(
    page: &PageMeta,
    config: &SiteConfig,
    clean: bool,
    deps_hash: Option<ContentHash>,
    log_file: bool,
) -> Result<()> {
    // Check if up-to-date (only for batch mode, process_page already checked)
    if !clean && is_up_to_date(&page.paths.source, &page.paths.html, deps_hash) {
        return Ok(());
    }

    if log_file {
        log!("content"; "{}", page.paths.relative);
    }

    // Create output directory
    if let Some(parent) = page.paths.html.parent() {
        fs::create_dir_all(parent)?;
    }

    // Get HTML content (must have been compiled, no CLI fallback)
    let html_content = page.compiled_html.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Page has no compiled HTML: {:?}", page.paths.source))?
        .clone();

    // Post-process and write
    // Check if the source file was named "index.typ" for relative path resolution
    let is_source_index = page.paths.source
        .file_stem()
        .is_some_and(|stem| stem == "index");
    let html_content = process_html(&page.paths.html, &html_content, config, is_source_index)?;
    let html_content = minify(MinifyType::Html(&html_content), config);

    // Compute source hash and embed marker for freshness detection
    let source_hash = freshness::compute_file_hash(&page.paths.source);
    let hash_marker = freshness::build_hash_marker(&source_hash, deps_hash.as_ref());

    // Embed hash marker at the end of HTML content (before closing </html>)
    let html_str = String::from_utf8_lossy(&html_content);
    let final_html = if let Some(pos) = html_str.rfind("</html>") {
        format!("{}{}\n</html>", &html_str[..pos], hash_marker)
    } else {
        format!("{}\n{}", html_str, hash_marker)
    };

    fs::write(&page.paths.html, final_html)?;

    Ok(())
}

/// Compile a typst file and extract metadata.
///
/// # Type Parameter
/// * `D` - Build driver (Production or Development)
///
/// Also records dependencies for incremental rebuild tracking.
/// Uses the VDOM pipeline for HTML generation.
///
/// When using `Development` driver, emits `data-tola-id` attributes
/// and caches indexed VDOM for hot reload.
pub fn compile_meta<D: crate::driver::BuildDriver>(
    driver: &D,
    path: &Path,
    config: &SiteConfig,
) -> Result<(Vec<u8>, Option<ContentMeta>)> {
    let root = config.get_root();

    // Use unified compile_vdom with driver
    let result = typst_lib::compile_vdom(driver, path, root, TOLA_META_LABEL)?;
    let meta = result.metadata.and_then(|json| serde_json::from_value(json).ok());

    super::deps::DEPENDENCY_GRAPH
        .write()
        .record_dependencies(path, &result.accessed_files);

    // Cache the indexed VDOM if available (development mode)
    if let Some(indexed) = result.indexed_vdom {
        let cache_key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        crate::hotreload::VDOM_CACHE.insert(cache_key, indexed);
    }

    Ok((result.html, meta))
}

/// Check if content metadata indicates a draft.
#[inline]
fn is_draft(meta: Option<&ContentMeta>) -> bool {
    meta.is_some_and(|c| c.draft)
}

// ============================================================================
// Two-Phase Compilation Support
// ============================================================================

/// Convert a `PageMeta` to `PageData` for the global site data store.
fn page_meta_to_data(page: &PageMeta) -> PageData {
    let content = page.content_meta.as_ref();
    PageData {
        url: page.paths.url_path.clone(),
        title: content
            .and_then(|c| c.title.clone())
            .unwrap_or_else(|| page.paths.relative.clone()),
        summary: content.and_then(|c| c.summary.clone()),
        date: content.and_then(|c| c.date.clone()),
        update: content.and_then(|c| c.update.clone()),
        author: content.and_then(|c| c.author.clone()),
        tags: content
            .map(|c| c.tags.clone())
            .unwrap_or_default(),
        draft: content.is_some_and(|c| c.draft),
    }
}

/// Phase 1: Collect metadata and classify pages as static or dynamic.
///
/// Returns paths of dynamic pages that need recompilation in Phase 2.
/// Static pages (those not using virtual data) are written directly.
///
/// # Smart Skip Logic
///
/// - **Static pages**: Do not access `/_data/*.json` files. Their HTML is
///   complete after Phase 1, so we write it immediately.
/// - **Dynamic pages**: Access virtual data files. Their HTML depends on
///   other pages' metadata, so they must wait for Phase 2.
pub fn collect_metadata_smart<D: crate::driver::BuildDriver + Copy>(
    driver: D,
    config: &SiteConfig,
    clean: bool,
    deps_hash: Option<ContentHash>,
    on_progress: impl Fn() + Sync,
) -> Result<(Vec<std::path::PathBuf>, usize, usize)> {
    let content_files = collect_all_files(&config.build.content);

    let typ_files: Vec<_> = content_files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|ext| ext == "typ"))
        .collect();

    // Clear global data store for fresh collection
    GLOBAL_SITE_DATA.clear();

    let results: Vec<Result<Option<(std::path::PathBuf, PageMeta, bool)>>> = typ_files
        .par_iter()
        .map(|path| {
            let page = PageMeta::from_paths(path.clone(), config)?;

            // Compile to extract metadata
            let root = config.get_root();
            let result = typst_lib::compile_vdom(&driver, path, root, TOLA_META_LABEL)?;

            // Cache indexed VDOM for hot reload (Development mode only)
            // This ensures the cache matches the HTML written to disk
            // Use canonicalize to match watch.rs cache lookup
            if let Some(ref indexed_vdom) = result.indexed_vdom {
                let cache_key = path.canonicalize().unwrap_or_else(|_| path.clone());
                crate::hotreload::VDOM_CACHE.insert(cache_key, indexed_vdom.clone());
            }

            // Check if this page uses virtual data
            let uses_virtual_data = result.uses_virtual_data();

            // Record dependencies for incremental rebuild
            super::deps::DEPENDENCY_GRAPH
                .write()
                .record_dependencies(path, &result.accessed_files);

            let content_meta: Option<ContentMeta> = result.metadata.and_then(|json| serde_json::from_value(json).ok());

            // Skip drafts
            if is_draft(content_meta.as_ref()) {
                on_progress();
                return Ok(None);
            }

            let mut page = page;
            page.content_meta = content_meta;
            page.compiled_html = Some(result.html);

            // Store in global data
            GLOBAL_SITE_DATA.insert_page(page_meta_to_data(&page));

            // For static pages, write immediately (their HTML is complete)
            if !uses_virtual_data {
                write_page(&page, config, clean, deps_hash, false)?;
            }

            on_progress();
            Ok(Some((path.clone(), page, uses_virtual_data)))
        })
        .collect();

    // Collect dynamic page paths and counts
    let mut dynamic_paths = Vec::new();
    let mut static_count = 0;
    let mut dynamic_count = 0;

    for result in results {
        match result {
            Ok(Some((path, _page, uses_virtual_data))) => {
                if uses_virtual_data {
                    dynamic_paths.push(path);
                    dynamic_count += 1;
                } else {
                    static_count += 1;
                }
            }
            Ok(None) => {} // Draft, skip
            Err(e) => return Err(e),
        }
    }

    Ok((dynamic_paths, static_count, dynamic_count))
}

/// Phase 2: Recompile only dynamic pages with complete global data.
///
/// Only recompiles pages that access virtual data files (`/_data/*.json`).
/// Static pages were already written in Phase 1.
pub fn compile_dynamic_pages<D: crate::driver::BuildDriver + Copy>(
    driver: D,
    paths: &[std::path::PathBuf],
    config: &SiteConfig,
    clean: bool,
    deps_hash: Option<ContentHash>,
    on_progress: impl Fn() + Sync,
) -> Result<Vec<PageMeta>> {
    let results: Vec<Result<PageMeta>> = paths
        .par_iter()
        .map(|path| {
            let mut page = PageMeta::from_paths(path.clone(), config)?;

            // Compile with complete data
            let (html, content_meta) = compile_meta(&driver, path, config)?;

            page.content_meta = content_meta;
            page.compiled_html = Some(html);

            // Write the page
            write_page(&page, config, clean, deps_hash, false)?;

            on_progress();
            Ok(page)
        })
        .collect();

    // Collect successful pages
    let mut items = Vec::with_capacity(results.len());
    for result in results {
        items.push(result?);
    }

    Ok(items)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
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

        let (html, meta) = result.unwrap();
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

        let (html, meta) = result.unwrap();
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

        let (_, meta) = result.unwrap();
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
