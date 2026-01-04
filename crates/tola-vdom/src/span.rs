//! Source Span abstraction for decoupling from typst
//!
//! This module provides a `SourceSpan` type that wraps typst's Span when
//! the `typst` feature is enabled, or uses a simple placeholder when not.
//!
//! # Design
//!
//! The goal is to allow tola-vdom to be used without typst dependency.
//! When `typst` is enabled, `SourceSpan` wraps the real `typst::syntax::Span`.
//! When disabled, `SourceSpan` is a simple u64 that can store span data
//! or be "detached" (None).

/// Source location span abstraction
///
/// When `typst` feature is enabled, this wraps `typst::syntax::Span`.
/// Otherwise, it's a simple wrapper around Option<u64>.
#[derive(Debug, Clone, Copy, Default)]
pub struct SourceSpan {
    /// Internal representation
    /// - When typst enabled: stores the raw u64 from typst::syntax::Span
    /// - When disabled: stores any u64 value, or None for detached
    inner: Option<u64>,
}

impl SourceSpan {
    /// Create a detached span (no source location)
    pub const fn detached() -> Self {
        Self { inner: None }
    }

    /// Check if this span is detached (has no source location)
    pub fn is_detached(&self) -> bool {
        self.inner.is_none()
    }

    /// Get the raw u64 value (if any)
    pub fn raw(&self) -> Option<u64> {
        self.inner
    }

    /// Create from a raw u64 value
    pub const fn from_raw(value: u64) -> Self {
        Self { inner: Some(value) }
    }
}

#[cfg(feature = "typst")]
impl From<typst_batch::typst::syntax::Span> for SourceSpan {
    fn from(span: typst_batch::typst::syntax::Span) -> Self {
        if span.is_detached() {
            Self::detached()
        } else {
            // typst::syntax::Span internally stores a NonZero<u64>
            // We extract it via the into_raw() method
            Self { inner: Some(span.into_raw().get()) }
        }
    }
}

#[cfg(feature = "typst")]
impl SourceSpan {
    /// Convert back to typst::syntax::Span (if possible)
    ///
    /// Returns detached Span if this SourceSpan is detached.
    pub fn to_typst_span(&self) -> typst_batch::typst::syntax::Span {
        use typst_batch::typst::syntax::Span;
        match self.inner.and_then(std::num::NonZero::new) {
            Some(raw) => Span::from_raw(raw),
            None => Span::detached(),
        }
    }
}
