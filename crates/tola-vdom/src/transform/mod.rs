//! Transform trait and implementations for type-safe phase transitions
//!
//! This module provides the unified API for VDOM phase transformations.
//! All phase transitions (Raw → Indexed → Processed → Rendered) are
//! expressed through the `Transform` trait.
//!
//! ## Core types (in `core.rs`)
//!
//! - `Transform` trait: The only public API for phase transitions
//! - `Pipeline`: Fluent chain builder for transforms
//! - `Processor`: Indexed → Processed transformation
//!
//! ## Concrete implementations
//!
//! - `indexer`: Raw → Indexed (assign StableIds, identify families)
//! - `render`: Processed → HTML bytes
//!
//! # Usage
//!
//! ```ignore
//! use tola_vdom::transform::*;
//!
//! let html = raw_doc
//!     .pipe(Indexer::new())
//!     .pipe(Processor::new())
//!     .pipe(HtmlRenderer::new());
//! ```

mod core;
pub mod indexer;
pub mod render;

// Core re-exports
pub use self::core::{process_family_ext, IdentityTransform, Pipeline, Processor, Transform};

// Implementation re-exports
pub use indexer::Indexer;
pub use render::HtmlRenderer;
#[allow(unused_imports)]
pub use render::HtmlRendererConfig;
