//! VDOM Transform implementations
//!
//! This module contains all concrete Transform implementations for the
//! VDOM phase pipeline. Each transform handles a specific concern:
//!
//! - `indexer`: Raw → Indexed (assign StableIds, identify families)
//! - `link`: Process and validate links
//! - `heading`: Generate anchor IDs for headings
//! - `svg`: Optimize and optionally extract SVGs
//! - `media`: Process images and videos
//! - `render`: Processed → HTML bytes
//!
//! # Usage
//!
//! ```ignore
//! use vdom::transforms::*;
//!
//! let html = raw_doc
//!     .pipe(Indexer::new())
//!     .pipe(LinkProcessor::new(config))
//!     .pipe(HeadingProcessor::new(config))
//!     .pipe(SvgOptimizer::new(config))
//!     .pipe(HtmlRenderer::new(config));
//! ```

pub mod indexer;
pub mod render;
// TODO: Implement remaining transform modules
// pub mod link;
// pub mod heading;
// pub mod svg;
// pub mod media;

// Re-exports
pub use indexer::Indexer;
pub use render::HtmlRenderer;
#[allow(unused_imports)]
pub use render::HtmlRendererConfig;
#[allow(unused_imports)]
pub use super::transform::Processor;
