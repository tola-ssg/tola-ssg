//! Path normalization utilities.
//!
//! Provides consistent path handling across the codebase:
//! - `normalize_path` - file system paths (canonicalize + fallback)

use std::path::{Path, PathBuf};

/// Normalize a file system path to absolute form.
///
/// Tries `canonicalize()` first (resolves symlinks, `.`, `..`).
/// Falls back to:
/// - Return as-is if already absolute
/// - Join with current directory if relative
///
/// # Example
/// ```ignore
/// use tola::utils::path::normalize_path;
/// let abs = normalize_path(Path::new("./content/post.typ"));
/// ```
#[inline]
pub fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_absolute() {
        let path = Path::new("/absolute/path/file.txt");
        let normalized = normalize_path(path);
        assert!(normalized.is_absolute());
    }

    #[test]
    fn test_normalize_path_relative() {
        let path = Path::new("relative/path/file.txt");
        let normalized = normalize_path(path);
        assert!(normalized.is_absolute());
    }
}
