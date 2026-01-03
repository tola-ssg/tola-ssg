//! Path normalization utilities.
//!
//! Provides consistent path handling across the codebase:
//! - `normalize_path` - file system paths (canonicalize + fallback)
//! - `normalize_url` - URL paths (cache keys)

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

/// Normalize a URL path for consistent cache keys.
///
/// Ensures:
/// - Always starts with `/`
/// - No trailing slash (except for root `/`)
/// - No double slashes
/// - No fragment (`#...`) or query string (`?...`)
///
/// # Example
/// ```
/// use tola::utils::path::normalize_url;
/// assert_eq!(normalize_url("/blog//post/?foo#bar"), "/blog/post");
/// assert_eq!(normalize_url("posts/"), "/posts");
/// ```
pub fn normalize_url(url: &str) -> String {
    // Remove fragment and query string
    let path = url
        .split('#').next().unwrap_or(url)
        .split('?').next().unwrap_or(url);

    let mut path = path.to_string();

    // Collapse multiple slashes
    while path.contains("//") {
        path = path.replace("//", "/");
    }

    // Ensure starts with /
    if !path.starts_with('/') {
        path = format!("/{}", path);
    }

    // Remove trailing slash (except for root)
    if path.len() > 1 && path.ends_with('/') {
        path.pop();
    }

    path
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

    #[test]
    fn test_normalize_url_basic() {
        assert_eq!(normalize_url("/blog/post"), "/blog/post");
        assert_eq!(normalize_url("posts"), "/posts");
    }

    #[test]
    fn test_normalize_url_trailing_slash() {
        assert_eq!(normalize_url("/blog/"), "/blog");
        assert_eq!(normalize_url("/"), "/");
    }

    #[test]
    fn test_normalize_url_double_slashes() {
        assert_eq!(normalize_url("/blog//post///page"), "/blog/post/page");
    }

    #[test]
    fn test_normalize_url_query_fragment() {
        assert_eq!(normalize_url("/blog/post?foo=bar"), "/blog/post");
        assert_eq!(normalize_url("/blog/post#section"), "/blog/post");
        assert_eq!(normalize_url("/blog/post?foo#bar"), "/blog/post");
    }
}
