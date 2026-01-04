//! Source format converters to Raw VDOM
//!
//! This module provides converters from various source formats to `Document<Raw>`.
//! Each converter is feature-gated and lives in its own submodule.
//!
//! # Supported Formats
//!
//! | Format | Feature | Module | Function |
//! |--------|---------|--------|----------|
//! | Typst | `typst` | [`typst`] | [`from_typst()`] |
//! | Markdown | `markdown` | `markdown` | (planned) |
//! | HTML | `html-parser` | `html` | (planned) |
//!
//! # Adding New Converters
//!
//! To add support for a new format:
//!
//! 1. Create a new submodule (e.g., `convert/latex.rs`)
//! 2. Add feature flag to `Cargo.toml`
//! 3. Implement `from_xxx(...) -> Document<Raw>`
//! 4. Re-export in this module with `#[cfg(feature = "xxx")]`
//!
//! The converter only needs to produce a valid `Document<Raw>`.
//! The standard pipeline (Indexer → Processor → Renderer) handles the rest.

// =============================================================================
// Typst converter
// =============================================================================

#[cfg(feature = "typst")]
pub mod typst;

#[cfg(feature = "typst")]
pub use self::typst::{from_typst_html, from_typst_html_with_meta};

// =============================================================================
// Future converters (planned)
// =============================================================================

// #[cfg(feature = "markdown")]
// pub mod markdown;
//
// #[cfg(feature = "markdown")]
// pub use markdown::from_markdown;

// #[cfg(feature = "html-parser")]
// pub mod html;
//
// #[cfg(feature = "html-parser")]
// pub use html::from_html;
