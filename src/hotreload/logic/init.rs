//! Initial Build Pipeline
//!
//! Functions for initial site build to populate VDOM cache.
//! Separated from coordinator to keep Actor code focused on orchestration.

use std::path::PathBuf;

use crate::compiler::collect_all_files;
use crate::config::SiteConfig;
use crate::vdom::{Document, Indexed};

use super::compile::{compile_page, CompileOutcome};

/// Collect and compile all Typst files for initial cache population.
///
/// Returns a list of (url_path, vdom) pairs for successful compilations.
/// Errors are logged but don't stop the build.
pub fn build_initial_cache(config: &SiteConfig) -> Vec<(String, Document<Indexed>)> {
    let typ_files = collect_typst_files(&config.build.content);

    if typ_files.is_empty() {
        return vec![];
    }

    crate::log!("init"; "{} files", typ_files.len());

    let results: Vec<_> = typ_files
        .iter()
        .filter_map(|path| match compile_page(path, config) {
            CompileOutcome::Vdom { url_path, vdom, .. } => Some((url_path, vdom)),
            CompileOutcome::Error { path, error } => {
                crate::log!("init"; "error {}: {}", path.display(), error);
                None
            }
            _ => None,
        })
        .collect();

    crate::log!("init"; "cached {} vdoms", results.len());
    results
}

/// Collect all .typ files from content directory.
fn collect_typst_files(content_dir: &std::path::Path) -> Vec<PathBuf> {
    collect_all_files(content_dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "typ"))
        .collect()
}
