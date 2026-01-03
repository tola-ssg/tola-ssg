//! File Classification Pipeline
//!
//! Pure functions for classifying changed files and determining rebuild strategy.
//! No Actor machinery, no side effects.

use std::path::{Path, PathBuf};

use rustc_hash::FxHashSet;

use crate::compiler::deps::get_dependents;
use crate::config::SiteConfig;
use crate::utils::path::normalize_path;

// =============================================================================
// File Category
// =============================================================================

/// Category of a changed file, determines rebuild strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCategory {
    /// Content file (.typ) - rebuild individually
    Content,
    /// Asset file - copy individually (currently triggers reload)
    Asset,
    /// Site config (tola.toml) - full rebuild
    Config,
    /// Dependency (templates, utils) - rebuild dependents
    Deps,
    /// Outside watched dirs - ignored
    Unknown,
}

impl FileCategory {
    pub fn name(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Asset => "asset",
            Self::Config => "config",
            Self::Deps => "deps",
            Self::Unknown => "unknown",
        }
    }
}

// =============================================================================
// Classification
// =============================================================================

/// Categorize a path based on config directories.
pub fn categorize_path(path: &Path, config: &SiteConfig) -> FileCategory {
    let path = normalize_path(path);

    if path == config.config_path {
        FileCategory::Config
    } else if config.build.deps.iter().any(|dep| path.starts_with(dep)) {
        FileCategory::Deps
    } else if path.starts_with(&config.build.content) {
        FileCategory::Content
    } else if path.starts_with(&config.build.assets) {
        FileCategory::Asset
    } else {
        FileCategory::Unknown
    }
}

/// Result of classifying changed files.
#[derive(Debug)]
pub struct ClassifyResult {
    /// Files grouped by category (for logging)
    pub classified: Vec<(PathBuf, FileCategory)>,
    /// Config changed - requires full rebuild
    pub config_changed: bool,
    /// Content files to compile (direct changes + affected by deps)
    pub content_to_compile: Vec<PathBuf>,
    /// Optional note (e.g., "deps changed but no dependents")
    pub note: Option<String>,
}

/// Classify changed files and determine rebuild strategy.
///
/// This is a pure function that:
/// 1. Categorizes each file
/// 2. Resolves dependency relationships
/// 3. Returns actionable results
pub fn classify_changes(paths: &[PathBuf], config: &SiteConfig) -> ClassifyResult {
    let mut classified = Vec::new();
    let mut config_changed = false;
    let mut deps_changed = Vec::new();
    let mut content_changed = Vec::new();

    // Categorize each path
    for path in paths {
        let category = categorize_path(path, config);
        classified.push((path.clone(), category));

        match category {
            FileCategory::Config => config_changed = true,
            FileCategory::Deps => deps_changed.push(path.clone()),
            FileCategory::Content => content_changed.push(path.clone()),
            FileCategory::Asset => content_changed.push(path.clone()),
            FileCategory::Unknown => {}
        }
    }

    // Resolve deps → affected content
    let mut note = None;
    let content_to_compile = if config_changed {
        vec![] // Full rebuild, no need to list files
    } else if !deps_changed.is_empty() {
        let affected = collect_dependents(&deps_changed);
        if affected.is_empty() {
            note = Some("deps changed but no dependents found".to_string());
            vec![]
        } else {
            // Merge affected with direct content changes
            affected
                .into_iter()
                .chain(content_changed)
                .collect::<FxHashSet<_>>()
                .into_iter()
                .collect()
        }
    } else {
        content_changed
    };

    // If deps changed but no dependents, treat as config change
    let config_changed =
        config_changed || (!deps_changed.is_empty() && content_to_compile.is_empty() && note.is_some());

    ClassifyResult {
        classified,
        config_changed,
        content_to_compile,
        note,
    }
}

/// Collect all content files that depend on the changed files.
pub fn collect_dependents(changed_files: &[PathBuf]) -> Vec<PathBuf> {
    let mut affected = FxHashSet::default();

    for path in changed_files {
        affected.extend(get_dependents(path.as_path()));
    }

    affected.into_iter().collect()
}
