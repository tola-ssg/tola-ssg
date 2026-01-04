//! Asset metadata and path resolution.

use crate::config::SiteConfig;
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

// ============================================================================
// Asset Metadata
// ============================================================================

/// Metadata for a static asset file.
///
/// Handles path resolution for assets, ensuring consistent URL generation
/// and output path calculation.
#[derive(Debug, Clone)]
pub struct AssetMeta {
    /// Path information
    pub paths: AssetPaths,
}

/// Path information for an asset.
#[derive(Debug, Clone)]
pub struct AssetPaths {
    /// Source file path
    pub source: PathBuf,
    /// Output file path (in public directory)
    pub dest: PathBuf,
    /// Relative path from assets root (for logging)
    pub relative: String,
    /// URL path (for linking)
    pub url: String,
}

impl AssetMeta {
    /// Create `AssetMeta` from a source path.
    pub fn from_source(source: PathBuf, config: &SiteConfig) -> Result<Self> {
        let assets_dir = &config.build.assets;
        let output_dir = config.paths().output_dir();

        let relative = source
            .strip_prefix(assets_dir)
            .map_err(|_| anyhow!("File is not in assets directory: {}", source.display()))?
            .to_str()
            .ok_or_else(|| anyhow!("Invalid path encoding"))?
            .to_owned();

        let dest = output_dir.join(&relative);
        let url = url_from_output_path(&dest, config)?;

        Ok(Self {
            paths: AssetPaths {
                source,
                dest,
                relative,
                url,
            },
        })
    }
}

/// Generate a URL path from an output file path.
///
/// Handles path prefix stripping and cross-platform separators.
pub fn url_from_output_path(path: &Path, config: &SiteConfig) -> Result<String> {
    let output_root = &config.build.output;

    // Strip output root
    let rel_to_output = path
        .strip_prefix(output_root)
        .map_err(|_| anyhow!("Path is not in output directory: {}", path.display()))?;

    // Convert to string and ensure forward slashes
    let path_str = rel_to_output.to_string_lossy().replace('\\', "/");

    // Ensure it starts with /
    let url = if path_str.starts_with('/') {
        path_str
    } else {
        format!("/{path_str}")
    };

    Ok(url)
}
