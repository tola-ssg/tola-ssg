//! Compilation Pipeline - Typst to VDOM
//!
//! Pure functions for compiling Typst files to VDOM.
//! No Actor machinery, minimal side effects.
//!
//! # Side Effect Isolation
//!
//! `compile_page` has ONE side effect: writing HTML to disk.
//! This is intentional - we want compilation to be atomic (compile + write).
//! The alternative (returning HTML string) would require callers to handle writes,
//! complicating error handling and atomicity.

use std::path::{Path, PathBuf};

use crate::compiler::pages::process_page;
use crate::config::SiteConfig;
use crate::driver::Development;
use crate::vdom::{Document, Indexed};

/// Result of compiling a single file
#[derive(Debug)]
pub enum CompileOutcome {
    /// Successfully compiled to VDOM
    Vdom {
        /// Source file path
        path: PathBuf,
        /// URL path for the page (e.g., "/blog/post")
        url_path: String,
        /// Indexed VDOM document
        vdom: Document<Indexed>,
    },
    /// Non-content file changed, needs full reload
    Reload { reason: String },
    /// File skipped (draft, not found, etc.)
    Skipped,
    /// Compilation error
    Error {
        /// Source file path
        path: PathBuf,
        /// Error message
        error: String,
    },
}

/// Compile a single file to VDOM
///
/// This is a pure function that:
/// 1. Routes by file extension
/// 2. Calls the existing `process_page` with Development driver for .typ files
/// 3. Returns a unified outcome type
pub fn compile_page(path: &Path, config: &SiteConfig) -> CompileOutcome {
    let ext = path.extension().and_then(|e| e.to_str());

    match ext {
        Some("typ") => compile_typst_file(path, config),
        Some("css" | "js" | "html") => CompileOutcome::Reload {
            reason: format!("asset changed: {}", path.display()),
        },
        // Unknown file types are ignored (whitelist approach)
        // This prevents editor temp files from triggering reload
        _ => CompileOutcome::Skipped,
    }
}

/// Compile a single Typst file to VDOM
fn compile_typst_file(path: &Path, config: &SiteConfig) -> CompileOutcome {
    let driver = Development;

    match process_page(&driver, path, config) {
        Ok(Some(page_result)) => {
            let url_path = page_result.url_path;

            // Write HTML to disk (process_page doesn't write)
            if let Err(e) = crate::compiler::pages::write_page_html(&page_result.page, config) {
                return CompileOutcome::Error {
                    path: path.to_path_buf(),
                    error: format!("failed to write HTML: {}", e),
                };
            }

            // indexed_vdom is only populated in development mode
            if let Some(vdom) = page_result.indexed_vdom {
                CompileOutcome::Vdom {
                    path: path.to_path_buf(),
                    url_path,
                    vdom,
                }
            } else {
                // No VDOM available (shouldn't happen in dev mode)
                CompileOutcome::Skipped
            }
        }
        Ok(None) => {
            // Page was skipped (draft, etc.)
            CompileOutcome::Skipped
        }
        Err(e) => CompileOutcome::Error {
            path: path.to_path_buf(),
            error: e.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_outcome_variants() {
        let _ = CompileOutcome::Reload {
            reason: "test".to_string(),
        };
        let _ = CompileOutcome::Skipped;
        let _ = CompileOutcome::Error {
            path: PathBuf::from("/test.typ"),
            error: "test error".to_string(),
        };
    }
}
