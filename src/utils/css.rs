//! CSS utilities: auto-enhance generation and Tailwind integration.
//!
//! This module provides:
//! - Auto-enhanced CSS generation for SVG theme adaptation
//! - Tailwind CSS build integration

use crate::config::SiteConfig;
use crate::embed::ENHANCE_CSS;
use crate::utils::exec::FilterRule;
use crate::exec;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

// ============================================================================
// Auto-enhance CSS
// ============================================================================

/// Get the enhance CSS filename (hidden file with hash).
///
/// Returns a filename like `.enhance-a1b2c3d4.css`.
pub fn enhance_css_filename() -> String {
    ENHANCE_CSS.filename()
}

/// Generate and write the auto-enhance CSS file to the output directory.
///
/// Returns the relative path to the generated file.
pub fn generate_enhance_css(output_dir: &Path) -> Result<PathBuf> {
    Ok(ENHANCE_CSS.write_to(output_dir)?)
}

/// Clean up old enhance CSS files (files matching `.enhance-*.css` pattern).
///
/// Keeps only the current version based on hash.
pub fn cleanup_old_enhance_css(output_dir: &Path) -> Result<()> {
    Ok(ENHANCE_CSS.cleanup_old(output_dir, &[])?)
}

// ============================================================================
// Tailwind CSS
// ============================================================================

/// Tailwind filter: skip version banner in output.
pub static TAILWIND_FILTER: FilterRule = FilterRule::new(&["≈ tailwindcss"]);

/// Check if a path is the Tailwind input file.
pub fn is_tailwind_input(path: &Path, config: &SiteConfig) -> bool {
    config.build.css.tailwind.enable
        && config
            .build
            .css
            .tailwind
            .input
            .as_ref()
            .is_some_and(|input| path.canonicalize().ok().as_ref() == Some(input))
}

/// Run Tailwind CSS build for the input file.
pub fn run_tailwind(input: &Path, output: &Path, config: &SiteConfig, quiet: bool) -> Result<()> {
    use super::exec::{SILENT_FILTER, FilterRule};
    let filter: &'static FilterRule = if quiet { &SILENT_FILTER } else { &TAILWIND_FILTER };
    exec!(
        pty=true;
        filter=filter;
        config.get_root();
        &config.build.css.tailwind.command;
        "-i", input, "-o", output,
        if config.build.minify { "--minify" } else { "" }
    )?;
    Ok(())
}

/// Rebuild Tailwind CSS using configured input path.
///
/// Used by watch mode to rebuild when source files change.
/// When `quiet` is true, output is suppressed (for watch mode).
pub fn rebuild_tailwind(
    config: &SiteConfig,
    get_output_path: impl FnOnce(&Path) -> Result<PathBuf>,
    quiet: bool,
) -> Result<()> {
    let input = config
        .build
        .css
        .tailwind
        .input
        .as_ref()
        .ok_or_else(|| anyhow!("Tailwind input path not configured"))?;

    let output = get_output_path(input)?;
    run_tailwind(input, &output, config, quiet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_enhance_css_filename_format() {
        let filename = enhance_css_filename();
        assert!(filename.starts_with(".enhance-"));
        assert!(filename.ends_with(".css"));
        assert_eq!(filename.len(), ".enhance-12345678.css".len());
    }

    #[test]
    fn test_enhance_css_filename_stable() {
        // Same content should produce same hash
        let f1 = enhance_css_filename();
        let f2 = enhance_css_filename();
        assert_eq!(f1, f2);
    }

    #[test]
    fn test_generate_enhance_css() {
        let dir = tempdir().unwrap();
        let result = generate_enhance_css(dir.path()).unwrap();

        // Check filename format
        let filename = result.to_string_lossy();
        assert!(filename.starts_with(".enhance-"));
        assert!(filename.ends_with(".css"));

        // Check file exists and has content
        let content = fs::read_to_string(dir.path().join(&result)).unwrap();
        assert!(content.contains("typst-text"));
        assert!(content.contains("currentColor"));
    }

    #[test]
    fn test_cleanup_old_enhance_css() {
        let dir = tempdir().unwrap();

        // Create some old files
        fs::write(dir.path().join(".enhance-old1.css"), "old").unwrap();
        fs::write(dir.path().join(".enhance-old2.css"), "old").unwrap();

        // Generate current file
        generate_enhance_css(dir.path()).unwrap();

        // Cleanup
        cleanup_old_enhance_css(dir.path()).unwrap();

        // Only current file should remain
        let files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".enhance-"))
            .collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().to_string_lossy(), enhance_css_filename());
    }
}
