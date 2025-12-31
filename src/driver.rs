//! Build driver abstraction for production/development mode.
//!
//! Replaces `dev_mode: bool` parameters with type-safe driver pattern.

/// Build environment abstraction.
///
/// Controls behavior differences between production and development builds:
/// - **Production**: Optimized output, no debug metadata
/// - **Development**: Hot reload support with `data-tola-id` attributes
pub trait BuildDriver: Send + Sync {
    /// Whether to emit `data-tola-id` attributes on elements.
    ///
    /// Used for VDOM diffing and hot reload.
    fn emit_ids(&self) -> bool;

    /// Whether to cache indexed VDOM for hot reload diffs.
    fn cache_vdom(&self) -> bool;
}

/// Production build driver.
///
/// Used for `tola build` - optimized output without debug metadata.
#[derive(Debug, Clone, Copy, Default)]
pub struct Production;

impl BuildDriver for Production {
    #[inline]
    fn emit_ids(&self) -> bool {
        false
    }

    #[inline]
    fn cache_vdom(&self) -> bool {
        false
    }
}

/// Development build driver.
///
/// Used for `tola serve` - includes hot reload support.
#[derive(Debug, Clone, Copy, Default)]
pub struct Development;

impl BuildDriver for Development {
    #[inline]
    fn emit_ids(&self) -> bool {
        true
    }

    #[inline]
    fn cache_vdom(&self) -> bool {
        true
    }
}
