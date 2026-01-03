//! CSS utilities: Tailwind integration.
//!
//! This module provides Tailwind CSS build integration.
//! Auto-enhance CSS is handled directly via `embed::ENHANCE_CSS`.

use crate::config::SiteConfig;
use crate::utils::exec::FilterRule;
use crate::exec;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

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
