//! Two-phase page collection with smart skip.
//!
//! Phase 1: Collect metadata, classify pages as static/dynamic
//! Phase 2: Recompile only dynamic pages with complete data

use crate::compiler::collect_all_files;
use crate::compiler::meta::{ContentMeta, PageMeta, TOLA_META_LABEL};
use crate::config::SiteConfig;
use crate::data::GLOBAL_SITE_DATA;
use crate::freshness::ContentHash;
use anyhow::Result;
use rayon::prelude::*;
use std::path::PathBuf;

use super::compile::{compile_meta, is_draft, page_meta_to_data};
use super::write::write_page;
use super::{collect_warning, MetadataResult};

// ============================================================================
// Phase 1: Metadata Collection
// ============================================================================

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
) -> Result<MetadataResult> {
    let content_files = collect_all_files(&config.build.content);

    let typ_files: Vec<_> = content_files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|ext| ext == "typ"))
        .collect();

    // Clear global data store for fresh collection
    GLOBAL_SITE_DATA.clear();

    let results: Vec<Result<Option<(PathBuf, PageMeta, bool)>>> = typ_files
        .par_iter()
        .map(|path| {
            let page = PageMeta::from_paths(path.clone(), config)?;

            // Get url_path for globally unique StableIds
            let url_path = page.paths.url_path.clone();

            // Compile to extract metadata
            let root = config.get_root();
            let result = crate::compiler::bridge::compile_vdom(
                &driver,
                path,
                root,
                TOLA_META_LABEL,
                Some(&url_path),
            )?;

            // Check if this page uses virtual data (before moving indexed_vdom)
            let uses_virtual_data = result.uses_virtual_data();

            // Store indexed_vdom for caller to cache (decouples compiler from hotreload)
            let _indexed_vdom = result.indexed_vdom;

            // Record dependencies for incremental rebuild
            crate::compiler::deps::DEPENDENCY_GRAPH
                .write()
                .record_dependencies(path, &result.accessed_files);

            let content_meta: Option<ContentMeta> = result.metadata.and_then(|json| serde_json::from_value(json).ok());

            // Collect warnings after all other uses of result (to avoid partial move)
            if let Some(warnings) = result.warnings {
                collect_warning(warnings);
            }

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

// ============================================================================
// Phase 2: Dynamic Page Compilation
// ============================================================================

/// Phase 2: Recompile only dynamic pages with complete global data.
///
/// Only recompiles pages that access virtual data files (`/_data/*.json`).
/// Static pages were already written in Phase 1.
pub fn compile_dynamic_pages<D: crate::driver::BuildDriver + Copy>(
    driver: D,
    paths: &[PathBuf],
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
            let (html, content_meta, _indexed_vdom) = compile_meta(&driver, path, config)?;

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
