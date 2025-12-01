//! Content and asset compilation logic.
//!
//! Handles compilation of Typst files to HTML, asset processing, and batch operations.

use crate::utils::category::{FileCategory, categorize_path, normalize_path};
use crate::utils::exec::FilterRule;
use crate::utils::page::PageMeta;
use crate::utils::xml::process_html;
use crate::{config::SiteConfig, exec, log};
use anyhow::{Result, anyhow, bail};
use rayon::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};
use walkdir::WalkDir;

// ============================================================================
// Constants
// ============================================================================

/// Typst filter: skip known warnings.
const TYPST_FILTER: FilterRule = FilterRule::new(&[
    "warning: html export is under active development",
    "and incomplete",
    "= hint: its behaviour may change at any time",
    "= hint: do not rely on this feature for production use cases",
    "= hint: see https://github.com/typst/typst/issues/5512",
    "for more information",
    "warning: elem",
]);

/// Tailwind filter: skip version banner.
const TAILWIND_FILTER: FilterRule = FilterRule::new(&["≈ tailwindcss"]);

// ============================================================================
// Directory Operations
// ============================================================================

/// Files to ignore during directory traversal
const IGNORED_FILES: &[&str] = &[".DS_Store"];

/// Collect all files from a directory recursively
pub fn collect_all_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let name = e.file_name().to_str().unwrap_or_default();
            !IGNORED_FILES.contains(&name)
        })
        .map(|e| e.into_path())
        .collect()
}

// ============================================================================
// Content Processing
// ============================================================================

/// Check if destination is up-to-date compared to source and dependencies
pub fn is_up_to_date(src: &Path, dst: &Path, deps_mtime: Option<SystemTime>) -> bool {
    let Ok(src_meta) = src.metadata() else {
        return false;
    };
    let Ok(dst_meta) = dst.metadata() else {
        return false;
    };

    let Ok(src_time) = src_meta.modified() else {
        return false;
    };
    let Ok(dst_time) = dst_meta.modified() else {
        return false;
    };

    // Check if source is newer than destination
    if src_time > dst_time {
        return false;
    }

    // Check if any dependency is newer than destination
    if let Some(deps) = deps_mtime
        && deps > dst_time
    {
        return false;
    }

    true
}

pub fn process_content(
    content_path: &Path,
    config: &'static SiteConfig,
    force_rebuild: bool,
    deps_mtime: Option<SystemTime>,
    log_file: bool,
) -> Result<()> {
    if content_path.extension().is_some_and(|ext| ext == "typ") {
        process_typst_page(content_path, config, force_rebuild, deps_mtime, log_file)
    } else {
        process_relative_asset(content_path, config, force_rebuild, log_file)
    }
}

fn process_relative_asset(
    path: &Path,
    config: &SiteConfig,
    force_rebuild: bool,
    log_file: bool,
) -> Result<()> {
    let content = &config.build.content;
    let output = &config.build.output.join(&config.build.path_prefix);

    let relative_path = path
        .strip_prefix(content)?
        .to_str()
        .ok_or(anyhow!("Invalid path"))?;

    let output_path = output.join(relative_path);

    // Relative assets don't depend on templates/config, just check source vs dest
    if !force_rebuild && is_up_to_date(path, &output_path, None) {
        return Ok(());
    }

    if log_file {
        log!("content"; "{}", relative_path);
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::copy(path, output_path)?;
    Ok(())
}

fn process_typst_page(
    path: &Path,
    config: &'static SiteConfig,
    force_rebuild: bool,
    deps_mtime: Option<SystemTime>,
    log_file: bool,
) -> Result<()> {
    // Process .typ file: get output paths, compile, and post-process
    let page = PageMeta::from_source(path.to_path_buf(), config)?;

    // Check source and dependencies (templates, utils, config)
    if !force_rebuild && is_up_to_date(path, &page.html, deps_mtime) {
        return Ok(());
    }

    if log_file {
        log!("content"; "{}", page.relative);
    }

    if let Some(parent) = page.html.parent() {
        fs::create_dir_all(parent)?;
    }

    let html_content = compile_typst(path, config)?;
    let html_content = process_html(&page.html, &html_content, config)?;

    // Minify HTML if enabled
    let html_content = if config.build.minify {
        let mut cfg = minify_html::Cfg::new();
        cfg.keep_closing_tags = true;
        cfg.keep_html_and_head_opening_tags = true;
        cfg.keep_comments = false;
        cfg.minify_css = true;
        cfg.minify_js = true;
        cfg.remove_bangs = true;
        cfg.remove_processing_instructions = true;
        minify_html::minify(html_content.as_slice(), &cfg)
    } else {
        html_content
    };

    fs::write(&page.html, html_content)?;
    Ok(())
}

fn compile_typst(content_path: &Path, config: &SiteConfig) -> Result<Vec<u8>> {
    let root = config.get_root();
    let output = exec!(
        filter=&TYPST_FILTER;
        &config.build.typst.command;
        "compile", "--features", "html", "--format", "html",
        "--font-path", root, "--root", root,
        content_path, "-"
    )?;
    Ok(output.stdout)
}

// ============================================================================
// Asset Processing
// ============================================================================

pub fn process_asset(
    asset_path: &Path,
    config: &'static SiteConfig,
    log_file: bool,
) -> Result<()> {
    let assets = &config.build.assets;
    let output = &config.build.output.join(&config.build.path_prefix);

    let asset_extension = asset_path
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default();
    let relative_asset_path = asset_path
        .strip_prefix(assets)?
        .to_str()
        .ok_or(anyhow!("Invalid path"))?;

    if log_file {
        log!("assets"; "{}", relative_asset_path);
    }

    let output_path = output.join(relative_asset_path);

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    match asset_extension {
        "css" if config.build.tailwind.enable => {
            let input = config.build.tailwind.input.as_ref().unwrap();
            // Config paths are already absolute, just canonicalize the runtime path
            let asset_path = asset_path.canonicalize().unwrap();
            if *input == asset_path {
                run_tailwind(input, &output_path, config)?;
            } else {
                fs::copy(asset_path, &output_path)?;
            }
        }
        _ => {
            fs::copy(asset_path, &output_path)?;
        }
    }

    Ok(())
}

// ============================================================================
// Batch Processing (for Watch Mode)
// ============================================================================

/// Process changed content files (.typ)
pub fn process_watched_content(files: &[&PathBuf], config: &'static SiteConfig) -> Result<()> {
    // In watch mode, we always force rebuild changed files (deps_mtime = None)
    // because FullRebuild is triggered separately for template/config changes
    files.par_iter().for_each(|path| {
        let path = normalize_path(path);
        if let Err(e) = process_content(&path, config, false, None, true) {
            log!("watch"; "{e}");
        }
    });

    // Rebuild tailwind CSS if enabled
    if config.build.tailwind.enable {
        rebuild_tailwind(config)?;
    }

    Ok(())
}

/// Process changed asset files
pub fn process_watched_assets(
    files: &[&PathBuf],
    config: &'static SiteConfig,
    should_wait_until_stable: bool,
) -> Result<()> {
    files
        .par_iter()
        .filter(|path| path.exists())
        .try_for_each(|path| {
            let path = normalize_path(path);
            if should_wait_until_stable {
                wait_until_stable(&path, 5)?;
            }
            process_asset(&path, config, true)
        })
}

/// Process all watched file changes
pub fn process_watched_files(files: &[PathBuf], config: &'static SiteConfig) -> Result<()> {
    let mut content_files = Vec::new();
    let mut asset_files = Vec::new();

    // Categorize files by type
    for path in files.iter().filter(|p| p.exists()) {
        match categorize_path(path, config) {
            FileCategory::Content => content_files.push(path),
            FileCategory::Asset => asset_files.push(path),
            // Dependency changes trigger full rebuild in watch.rs before reaching here.
            // Unknown files (e.g., .DS_Store) are silently ignored.
            _ => {}
        }
    }

    if !content_files.is_empty() {
        process_watched_content(&content_files, config)?;
    }
    if !asset_files.is_empty() {
        process_watched_assets(&asset_files, config, true)?;
    }

    Ok(())
}

/// Rebuild tailwind CSS
fn rebuild_tailwind(config: &'static SiteConfig) -> Result<()> {
    let input = config
        .build
        .tailwind
        .input
        .as_ref()
        .ok_or_else(|| anyhow!("Tailwind input path not configured"))?;

    let relative_path = input
        .strip_prefix(&config.build.assets)?
        .to_str()
        .ok_or_else(|| anyhow!("Invalid tailwind input path"))?;

    // Output path includes path_prefix for consistency with other assets
    let output = config
        .build
        .output
        .join(&config.build.path_prefix)
        .join(relative_path);

    run_tailwind(input, &output, config)?;

    Ok(())
}

fn run_tailwind(input: &Path, output: &Path, config: &SiteConfig) -> Result<()> {
    exec!(
        filter=&TAILWIND_FILTER;
        config.get_root();
        &config.build.tailwind.command;
        "-i", input, "-o", output,
        if config.build.minify { "--minify" } else { "" }
    )?;
    Ok(())
}

/// Wait for file to stop being written to
pub fn wait_until_stable(path: &Path, max_retries: usize) -> Result<()> {
    const POLL_INTERVAL: Duration = Duration::from_millis(50);

    let mut last_size = fs::metadata(path)?.len();

    for _ in 0..max_retries {
        thread::sleep(POLL_INTERVAL);
        let current_size = fs::metadata(path)?.len();
        if current_size == last_size {
            return Ok(());
        }
        last_size = current_size;
    }

    bail!("File did not stabilize after {max_retries} retries")
}

