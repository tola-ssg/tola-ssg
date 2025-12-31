use crate::compiler::meta::{PageMeta, ContentMeta, Pages, TOLA_META_LABEL};
use crate::compiler::{collect_all_files, is_up_to_date};
use crate::data::{PageData, GLOBAL_SITE_DATA};
use crate::utils::minify::{minify, MinifyType};
use crate::utils::xml::process_html;
use crate::{config::SiteConfig, driver::Production, log, typst_lib};
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::time::SystemTime;
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
    deps_mtime: Option<SystemTime>,
    on_progress: impl Fn() + Sync,
) -> Result<()> {
    pages.items.par_iter().try_for_each(|page| {
        let result = write_page(page, config, clean, deps_mtime, false);
        on_progress();
        result
    })
}

// ============================================================================
// process_page: Single page compilation (for watch mode)
// ============================================================================

/// Process a single .typ file: compile and write HTML.
///
/// Used by build mode to compile and write individual files.
/// Also updates `GLOBAL_SITE_DATA` with the page's metadata for virtual JSON files.
/// Returns `Some(PageMeta)` if compiled, `None` if skipped (up-to-date or draft).
pub fn compile_and_write_page(
    path: &Path,
    config: &SiteConfig,
    clean: bool,
    deps_mtime: Option<SystemTime>,
    log_file: bool,
) -> Result<Option<PageMeta>> {
    let mut page = PageMeta::from_paths(path.to_path_buf(), config)?;

    // Check if up-to-date
    if !clean && is_up_to_date(path, &page.paths.html, deps_mtime) {
        return Ok(None);
    }

    // Compile the page and get metadata
    let (html_content, content_meta) = compile_meta(&Production, path, config)?;

    // Skip drafts
    if is_draft(content_meta.as_ref()) {
        // Remove from global data if it was previously published
        // (This handles the case where a page is marked as draft after being published)
        return Ok(None);
    }

    page.content_meta = content_meta;
    page.compiled_html = Some(html_content);

    // Update global site data for virtual JSON files
    GLOBAL_SITE_DATA.insert_page(page_meta_to_data(&page));

    // Write the page
    write_page(&page, config, true, None, log_file)?;

    Ok(Some(page))
}

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
    /// Whether this page depends on virtual data files (pages.json, tags.json)
    pub uses_virtual_data: bool,
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

    // Track if this page depends on virtual data (before moving metadata)
    let uses_virtual_data = result.uses_virtual_data();

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
        uses_virtual_data,
    }))
}

/// Write a page's HTML to disk.
pub fn write_page_html(page: &PageMeta, config: &SiteConfig) -> Result<()> {
    write_page(page, config, true, None, false)
}

// ============================================================================
// Deprecated APIs (backward compatibility)
// ============================================================================

/// Result of development mode page compilation
///
/// Deprecated: Use `PageResult` instead.
pub struct DevPageResult {
    /// Page metadata
    pub page: PageMeta,
    /// Indexed VDOM for diff comparison
    pub indexed_vdom: crate::vdom::Document<crate::vdom::Indexed>,
    /// URL path for the page (e.g., "/blog/post")
    pub url_path: String,
}

/// Deprecated: Use `process_page(&Development, ...)` instead.
#[deprecated(since = "0.7.0", note = "Use `process_page(&Development, ...)` instead")]
pub fn process_page_for_dev(
    path: &Path,
    config: &SiteConfig,
) -> Result<Option<DevPageResult>> {
    let result = process_page(&crate::driver::Development, path, config)?;
    Ok(result.map(|r| DevPageResult {
        page: r.page,
        indexed_vdom: r.indexed_vdom.expect("Development driver should cache VDOM"),
        url_path: r.url_path,
    }))
}

/// Deprecated: Use `write_page_html` instead.
#[deprecated(since = "0.7.0", note = "Use `write_page_html` instead")]
pub fn write_page_for_dev(page: &PageMeta, config: &SiteConfig) -> Result<()> {
    write_page_html(page, config)
}

// ============================================================================
// Internal
// ============================================================================

/// Write a page's HTML to disk.
fn write_page(
    page: &PageMeta,
    config: &SiteConfig,
    clean: bool,
    deps_mtime: Option<SystemTime>,
    log_file: bool,
) -> Result<()> {
    // Check if up-to-date (only for batch mode, process_page already checked)
    if !clean && is_up_to_date(&page.paths.source, &page.paths.html, deps_mtime) {
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
    fs::write(&page.paths.html, &*html_content)?;

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
        crate::hotreload::VDOM_CACHE.insert(path.to_path_buf(), indexed);
    }

    Ok((result.html, meta))
}

/// Query metadata only.
pub fn query_meta(path: &Path, config: &SiteConfig) -> Option<ContentMeta> {
    let root = config.get_root();
    let result = typst_lib::compile_meta(path, root, TOLA_META_LABEL).ok()?;
    result.metadata.and_then(|json| serde_json::from_value(json).ok())
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

/// Phase 1: Collect metadata from all pages.
///
/// Compiles all pages to extract metadata, populating `GLOBAL_SITE_DATA`.
/// HTML output is discarded since it may be incomplete (virtual JSON returns empty).
///
/// After this phase, `GLOBAL_SITE_DATA` contains complete metadata from all pages.
pub fn collect_metadata(
    config: &SiteConfig,
    on_progress: impl Fn() + Sync,
) -> Result<Vec<std::path::PathBuf>> {
    let content_files = collect_all_files(&config.build.content);

    let typ_files: Vec<_> = content_files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|ext| ext == "typ"))
        .collect();

    // Clear global data store for fresh collection
    GLOBAL_SITE_DATA.clear();

    let results: Vec<Result<Option<(std::path::PathBuf, PageMeta)>>> = typ_files
        .par_iter()
        .map(|path| {
            let page = PageMeta::from_paths(path.clone(), config)?;

            // Compile to extract metadata (HTML discarded)
            let root = config.get_root();
            let result = typst_lib::compile_meta(path, root, TOLA_META_LABEL)?;

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

            // Store in global data
            GLOBAL_SITE_DATA.insert_page(page_meta_to_data(&page));

            on_progress();
            Ok(Some((path.clone(), page)))
        })
        .collect();

    // Collect paths of non-draft pages
    let mut paths = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(Some((path, _))) => paths.push(path),
            Ok(None) => {} // Draft, skip
            Err(e) => return Err(e),
        }
    }

    Ok(paths)
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
    deps_mtime: Option<SystemTime>,
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
                write_page(&page, config, clean, deps_mtime, false)?;
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
    deps_mtime: Option<SystemTime>,
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
            write_page(&page, config, clean, deps_mtime, false)?;

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

/// Phase 2: Compile pages with complete global data (legacy).
///
/// Compiles all pages again, this time with `GLOBAL_SITE_DATA` fully populated.
/// Virtual JSON files now return complete data, so HTML output is correct.
///
/// # Type Parameter
/// * `D` - Build driver (Production or Development)
pub fn compile_pages<D: crate::driver::BuildDriver + Copy>(
    driver: D,
    paths: &[std::path::PathBuf],
    config: &SiteConfig,
    clean: bool,
    deps_mtime: Option<SystemTime>,
    on_progress: impl Fn() + Sync,
) -> Result<Pages> {
    let results: Vec<Result<PageMeta>> = paths
        .par_iter()
        .map(|path| {
            let mut page = PageMeta::from_paths(path.clone(), config)?;

            // Compile with complete data
            let (html, content_meta) = compile_meta(&driver, path, config)?;

            page.content_meta = content_meta;
            page.compiled_html = Some(html);

            // Write the page
            write_page(&page, config, clean, deps_mtime, false)?;

            on_progress();
            Ok(page)
        })
        .collect();

    // Collect successful pages
    let mut items = Vec::with_capacity(results.len());
    for result in results {
        items.push(result?);
    }

    Ok(Pages { items })
}

/// Collect all pages from content directory with metadata.
///
/// This function scans the content directory for `.typ` files and collects
/// their metadata using either lib mode or CLI mode based on config.
///
/// # Behavior
///
/// - **Lib mode**: Compiles each file once, extracting both HTML and metadata.
///   The compiled HTML is stored in `PageMeta.compiled_html` for later use.
/// - **CLI mode**: Only queries metadata (no compilation yet).
///   `PageMeta.compiled_html` will be `None`.
///
/// Draft pages (with `draft: true` in metadata) are automatically filtered out.
/// Pages without `<tola-meta>` are still built (`content_meta` = None).
///
/// Note: This is the legacy single-phase collection. For two-phase compilation
/// with virtual data support, use `collect_metadata` + `compile_pages_with_data`.
#[allow(dead_code)]
pub fn collect_pages(config: &SiteConfig) -> Result<Pages> {
    let content_files = collect_all_files(&config.build.content);

    let typ_files: Vec<_> = content_files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|ext| ext == "typ"))
        .collect();

    let results: Vec<Result<Option<PageMeta>>> = typ_files
        .par_iter()
        .map(|path| {
            let mut page = PageMeta::from_paths(path.clone(), config)?;

            // Compile once, get both HTML and metadata (metadata may be None)
            let result = compile_meta(&Production, path, config)?;
            let html = Some(result.0);
            let content_meta = result.1;

            // Skip drafts
            if is_draft(content_meta.as_ref()) {
                return Ok(None);
            }

            page.content_meta = content_meta;
            page.compiled_html = html;
            Ok(Some(page))
        })
        .collect();

    // Check for errors and collect successful pages
    let mut items = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(Some(page)) => items.push(page),
            Ok(None) => {} // Draft, skip
            Err(e) => return Err(e), // Propagate compilation error
        }
    }

    Ok(Pages { items })
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
