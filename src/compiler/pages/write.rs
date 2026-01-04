//! HTML file writing for compiled pages.
//!
//! Handles writing compiled HTML to disk with post-processing.

use crate::compiler::meta::PageMeta;
use crate::config::SiteConfig;
use crate::freshness::{self, is_fresh, ContentHash};
use crate::utils::minify::{minify, MinifyType};
use crate::utils::xml::process_html;
use crate::log;
use anyhow::Result;
use std::fs;

/// Write a page's HTML to disk (public API).
///
/// Always writes the file (clean mode behavior).
/// Used by watch mode after compilation.
pub fn write_page_html(page: &PageMeta, config: &SiteConfig) -> Result<()> {
    write_page(page, config, true, None, false)
}

/// Write a page's HTML to disk with freshness check.
///
/// # Arguments
///
/// * `page` - Page metadata with compiled HTML
/// * `config` - Site configuration
/// * `clean` - If false, skip if output is up-to-date
/// * `deps_hash` - Optional dependency hash for freshness check
/// * `log_file` - Whether to log the file being written
pub(super) fn write_page(
    page: &PageMeta,
    config: &SiteConfig,
    clean: bool,
    deps_hash: Option<ContentHash>,
    log_file: bool,
) -> Result<()> {
    // Check if up-to-date (only for batch mode, process_page already checked)
    if !clean && is_fresh(&page.paths.source, &page.paths.html, deps_hash) {
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
    let html_content = minify(MinifyType::Html(&html_content), config.build.minify);

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
