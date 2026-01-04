//! Single page compilation and metadata extraction.
//!
//! Handles compiling individual `.typ` files and extracting metadata.

use crate::compiler::meta::{ContentMeta, PageMeta, TOLA_META_LABEL};
use crate::config::SiteConfig;
use crate::data::{PageData, GLOBAL_SITE_DATA};
use anyhow::Result;
use std::path::Path;

use super::{collect_warning, CompileMetaResult, PageResult};

// ============================================================================
// Single Page Processing (watch mode)
// ============================================================================

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

    // Get url_path first for globally unique StableIds
    let url_path = page.paths.url_path.clone();

    // Compile with driver, passing url_path for unique StableIds
    let result = crate::compiler::bridge::compile_vdom(
        driver,
        path,
        config.get_root(),
        TOLA_META_LABEL,
        Some(&url_path),
    )?;

    // Extract metadata
    let content_meta: Option<ContentMeta> = result
        .metadata
        .and_then(|json| serde_json::from_value(json).ok());

    // Skip drafts
    if is_draft(content_meta.as_ref()) {
        return Ok(None);
    }

    // Record dependencies
    crate::compiler::deps::DEPENDENCY_GRAPH
        .write()
        .record_dependencies(path, &result.accessed_files);

    page.content_meta = content_meta;
    page.compiled_html = Some(result.html);

    // Collect warnings after all other uses of result (to avoid partial move)
    if let Some(warnings) = result.warnings {
        collect_warning(warnings);
    }

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

// ============================================================================
// Metadata Extraction
// ============================================================================

/// Compile a typst file and extract metadata.
///
/// # Type Parameter
/// * `D` - Build driver (Production or Development)
///
/// Also records dependencies for incremental rebuild tracking.
/// Uses the VDOM pipeline for HTML generation.
///
/// When using `Development` driver, emits `data-tola-id` attributes
/// and returns indexed VDOM for caller to cache (decoupled from hotreload).
pub fn compile_meta<D: crate::driver::BuildDriver>(
    driver: &D,
    path: &Path,
    config: &SiteConfig,
) -> Result<CompileMetaResult> {
    let root = config.get_root();

    // Use unified compile_vdom with driver
    // Pass None for url_path - compile_meta is typically used for production
    // where globally unique StableIds aren't needed
    let result = crate::compiler::bridge::compile_vdom(driver, path, root, TOLA_META_LABEL, None)?;

    let meta = result.metadata.and_then(|json| serde_json::from_value(json).ok());

    crate::compiler::deps::DEPENDENCY_GRAPH
        .write()
        .record_dependencies(path, &result.accessed_files);

    // Return indexed_vdom to caller for caching decision
    // (decouples compiler from hotreload)
    let indexed_vdom = result.indexed_vdom;

    let html = result.html;

    // Collect warnings after all other uses of result (to avoid partial move)
    if let Some(warnings) = result.warnings {
        collect_warning(warnings);
    }

    Ok((html, meta, indexed_vdom))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if content metadata indicates a draft.
#[inline]
pub(super) fn is_draft(meta: Option<&ContentMeta>) -> bool {
    meta.is_some_and(|c| c.draft)
}

/// Convert a `PageMeta` to `PageData` for the global site data store.
pub(super) fn page_meta_to_data(page: &PageMeta) -> PageData {
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
