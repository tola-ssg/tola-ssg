//! Typst library integration for direct compilation without CLI overhead.
//!
//! This module provides a thin wrapper around `typst_batch` crate, adding
//! tola-specific functionality like virtual data files (`/_data/*.json`).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                       Global Shared Resources                           │
//! │           (provided by typst_batch crate)                               │
//! ├─────────────┬─────────────┬─────────────────┬───────────────────────────┤
//! │ GLOBAL_FONTS│GLOBAL_LIBRARY│GLOBAL_PACKAGE   │   GLOBAL_FILE_CACHE       │
//! │ (~100ms)    │ (std lib)    │ (pkg cache)     │ (source/bytes per FileId) │
//! └──────┬──────┴──────┬───────┴────────┬────────┴─────────────┬────────────┘
//!        │             │                │                      │
//!        └─────────────┴────────────────┴──────────────────────┘
//!                                 │
//!                                 ▼
//!          ┌─────────────────────────────────────────┐
//!          │        SystemWorld (per-file, ~free)    │
//!          │  + TolaVirtualData provider             │
//!          └─────────────────────────────────────────┘
//! ```

use std::path::{Path, PathBuf};

use typst_batch::typst_html;

// Re-export from typst_batch
pub use typst_batch::{
    clear_file_cache, get_fonts, DiagnosticsExt,
    reset_access_flags, get_accessed_files, SystemWorld, GLOBAL_LIBRARY,
    query_metadata,
};

use crate::data::{is_virtual_data_path, read_virtual_data};

// =============================================================================
// Virtual File System
// =============================================================================

/// Tola's virtual file system for `/_data/*.json` files.
///
/// Implements `VirtualFileSystem` to intercept file reads for virtual paths
/// and return dynamically generated JSON from the global site data store.
pub struct TolaVirtualFS;

impl typst_batch::VirtualFileSystem for TolaVirtualFS {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        if is_virtual_data_path(path) {
            read_virtual_data(path)
        } else {
            None
        }
    }
}

// =============================================================================
// Types
// =============================================================================

/// Compilation result containing HTML output and optional metadata.
#[derive(Debug)]
#[allow(dead_code)]
pub struct CompileResult {
    /// The compiled HTML content as bytes.
    pub html: Vec<u8>,
    /// Optional metadata value (None if label not found).
    pub metadata: Option<serde_json::Value>,
    /// Files accessed during compilation (for dependency tracking).
    pub accessed_files: Vec<PathBuf>,
}

#[allow(dead_code)]
impl CompileResult {
    /// Check if this compilation accessed any virtual data files.
    #[inline]
    pub fn uses_virtual_data(&self) -> bool {
        self.accessed_files.iter().any(|p| is_virtual_data_path(p))
    }

    /// Get all virtual data files accessed during compilation.
    pub fn accessed_virtual_files(&self) -> Vec<&PathBuf> {
        self.accessed_files
            .iter()
            .filter(|p| is_virtual_data_path(p))
            .collect()
    }
}

// =============================================================================
// Test Synchronization
// =============================================================================

/// Test-only mutex to serialize typst compilations.
#[cfg(test)]
static COMPILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(not(test))]
struct DummyGuard;

#[cfg(test)]
fn acquire_test_lock() -> std::sync::MutexGuard<'static, ()> {
    COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(not(test))]
const fn acquire_test_lock() -> DummyGuard {
    DummyGuard
}

// =============================================================================
// Public API
// =============================================================================

/// Pre-warm global resources (fonts, library, package storage).
///
/// Call once at startup to avoid lazy initialization during compilation.
/// Also registers the virtual file system for `/_data/*.json` files.
pub fn warmup_with_font_dirs(font_dirs: &[&Path]) {
    // Register tola's virtual file system for /_data/*.json files
    typst_batch::set_virtual_fs(TolaVirtualFS);

    let _ = get_fonts(font_dirs);
    let _ = &*GLOBAL_LIBRARY;
    let _ = typst_batch::config::package_storage();
    let _ = &*typst_batch::GLOBAL_FILE_CACHE;
}

/// Compile a Typst file and extract metadata in a single pass.
#[allow(dead_code)]
pub fn compile_meta(
    path: &Path,
    root: &Path,
    label_name: &str,
) -> anyhow::Result<CompileResult> {
    let _guard = acquire_test_lock();
    let (_world, document, _warnings) = compile_base(path, root)?;

    let accessed_files = collect_accessed_files(root);

    let html = typst_html::html(&document)
        .map_err(|e| anyhow::anyhow!("HTML export failed: {e:?}"))?
        .into_bytes();

    let metadata = extract_meta(&document, label_name);

    Ok(CompileResult {
        html,
        metadata,
        accessed_files,
    })
}

// =============================================================================
// Clean Compilation API (for bridge module)
// =============================================================================

/// Compile a Typst file to HtmlDocument (pure compilation, no VDOM).
pub fn compile_html(
    path: &Path,
    root: &Path,
) -> anyhow::Result<(typst_html::HtmlDocument, Vec<PathBuf>, Option<String>)> {
    let _guard = acquire_test_lock();
    let (_world, document, warnings) = compile_base(path, root)?;
    let accessed_files = collect_accessed_files(root);
    Ok((document, accessed_files, warnings))
}

// =============================================================================
// VDOM-Ready API
// =============================================================================

/// Result of document compilation (without HTML serialization).
#[derive(Debug)]
#[allow(dead_code)]
pub struct DocumentResult {
    pub document: typst_html::HtmlDocument,
    pub metadata: Option<serde_json::Value>,
    pub accessed_files: Vec<PathBuf>,
}

/// Compile a Typst file without HTML serialization.
#[allow(dead_code)]
pub fn compile_document(
    path: &Path,
    root: &Path,
    label_name: &str,
) -> anyhow::Result<DocumentResult> {
    let _guard = acquire_test_lock();
    let (_world, document, _warnings) = compile_base(path, root)?;

    let accessed_files = collect_accessed_files(root);
    let metadata = extract_meta(&document, label_name);

    Ok(DocumentResult {
        document,
        metadata,
        accessed_files,
    })
}

// =============================================================================
// Internal Helpers
// =============================================================================

/// Core compilation logic shared by all entry points.
fn compile_base(
    path: &Path,
    root: &Path,
) -> anyhow::Result<(SystemWorld, typst_html::HtmlDocument, Option<String>)> {
    reset_access_flags();

    let world = SystemWorld::new(path, root);
    let result = typst_batch::typst::compile(&world);

    // Check for errors in warnings
    if result.warnings.has_errors() {
        let formatted = result.warnings.format(&world);
        anyhow::bail!("Typst compilation warnings:\n{formatted}");
    }

    // Extract document or format errors
    let document = result.output.map_err(|errors| {
        let all_diags: Vec<_> = errors.iter().chain(&result.warnings).cloned().collect();
        let filtered = all_diags.filter_html_warnings();
        let formatted = filtered.format(&world);
        anyhow::anyhow!("Typst compilation failed:\n{formatted}")
    })?;

    // Format warnings for caller to display
    let filtered_warnings = result.warnings.filter_html_warnings();
    let warnings = if filtered_warnings.is_empty() {
        None
    } else {
        Some(filtered_warnings.format(&world))
    };

    Ok((world, document, warnings))
}

/// Extract metadata from a compiled document by label name.
fn extract_meta(document: &typst_batch::typst_html::HtmlDocument, label_name: &str) -> Option<serde_json::Value> {
    query_metadata(document, label_name)
}

/// Collect files accessed during the last compilation.
fn collect_accessed_files(root: &Path) -> Vec<PathBuf> {
    get_accessed_files()
        .into_iter()
        .filter(|id| id.package().is_none())
        .filter_map(|id| {
            id.vpath().resolve(root).or_else(|| {
                let vpath = id.vpath().as_rooted_path();
                if is_virtual_data_path(vpath) {
                    Some(vpath.to_path_buf())
                } else {
                    None
                }
            })
        })
        .collect()
}

/// Query metadata from a Typst file by label name.
#[allow(dead_code)]
pub fn query_meta(path: &Path, root: &Path, label_name: &str) -> anyhow::Result<serde_json::Value> {
    let _guard = acquire_test_lock();
    let (_world, document, _warnings) = compile_base(path, root)?;

    query_metadata(&document, label_name)
        .ok_or_else(|| anyhow::anyhow!("Metadata with label '{label_name}' not found"))
}

/// Disable ANSI color output for tests.
#[cfg(test)]
pub fn disable_colors() {
    colored::control::set_override(false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::meta::TOLA_META_LABEL;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_project() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let content_dir = dir.path().join("content");
        fs::create_dir_all(&content_dir).unwrap();

        let file_path = content_dir.join("test.typ");
        fs::write(&file_path, "= Hello World\n\nThis is a test.").unwrap();

        (dir, file_path)
    }

    #[test]
    fn test_warmup_does_not_panic() {
        let dir = TempDir::new().unwrap();
        warmup_with_font_dirs(&[dir.path()]);
    }

    #[test]
    fn test_compile_simple_document() {
        let (dir, file_path) = create_test_project();

        let result = compile_meta(&file_path, dir.path(), TOLA_META_LABEL);
        assert!(result.is_ok(), "Compilation should succeed: {:?}", result);

        let compiled = result.unwrap();
        let html = String::from_utf8_lossy(&compiled.html);
        assert!(html.contains("Hello World"), "HTML should contain heading");
    }

    #[test]
    fn test_compile_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let fake_path = dir.path().join("nonexistent.typ");

        let result = compile_meta(&fake_path, dir.path(), TOLA_META_LABEL);
        assert!(result.is_err(), "Should fail for nonexistent file");
    }

    #[test]
    fn test_compile_with_imports() {
        let dir = TempDir::new().unwrap();
        let content_dir = dir.path().join("content");
        fs::create_dir_all(&content_dir).unwrap();

        let template_dir = dir.path().join("templates");
        fs::create_dir_all(&template_dir).unwrap();
        fs::write(template_dir.join("header.typ"), "#let header = \"My Site\"").unwrap();

        let main_file = content_dir.join("main.typ");
        fs::write(
            &main_file,
            "#import \"/templates/header.typ\": header\n= #header\n",
        )
        .unwrap();

        let result = compile_meta(&main_file, dir.path(), TOLA_META_LABEL);
        assert!(result.is_ok(), "Should compile with imports: {:?}", result);
    }

    #[test]
    fn test_multiple_compilations_share_resources() {
        let (dir, file_path) = create_test_project();

        for _ in 0..3 {
            let result = compile_meta(&file_path, dir.path(), TOLA_META_LABEL);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_compile_error_shows_formatted_message() {
        super::disable_colors();

        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("error.typ");
        fs::write(&file_path, "#let x = \n= Hello").unwrap();

        let result = compile_meta(&file_path, dir.path(), TOLA_META_LABEL);
        assert!(result.is_err(), "Should fail for syntax error");

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("error:"),
            "Error should have 'error:' prefix: {}",
            err_msg
        );
    }

    #[test]
    fn test_compile_error_shows_line_numbers() {
        super::disable_colors();

        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("error.typ");
        fs::write(&file_path, "= Title\n\n#undefined_var").unwrap();

        let result = compile_meta(&file_path, dir.path(), TOLA_META_LABEL);
        assert!(result.is_err(), "Should fail for undefined variable");

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("error.typ"),
            "Error should contain filename: {}",
            err_msg
        );
    }

    #[test]
    fn test_query_meta_basic() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("post.typ");

        fs::write(
            &file_path,
            r#"#metadata((
  title: "Test Post",
  date: "2024-01-01",
  author: "Test Author",
)) <tola-meta>

= Hello World
"#,
        )
        .unwrap();

        let result = query_meta(&file_path, dir.path(), TOLA_META_LABEL);
        assert!(result.is_ok(), "Query should succeed: {:?}", result);

        let value = result.unwrap();
        let json: serde_json::Value = serde_json::to_value(&value).unwrap();
        assert_eq!(json.get("title").and_then(|v| v.as_str()), Some("Test Post"));
        assert_eq!(json.get("date").and_then(|v| v.as_str()), Some("2024-01-01"));
        assert_eq!(json.get("author").and_then(|v| v.as_str()), Some("Test Author"));
    }

    #[test]
    fn test_query_meta_not_found() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("post.typ");
        fs::write(&file_path, "= Hello World").unwrap();

        let result = query_meta(&file_path, dir.path(), "nonexistent-label");
        assert!(result.is_err(), "Should fail for missing label");
    }
}
