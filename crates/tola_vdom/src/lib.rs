//! # tola_vdom
//!
//! Type-safe, multi-phase HTML/XML DOM processing using the Trees That Grow (TTG) pattern.
//!
//! ## Overview
//!
//! `tola_vdom` provides a virtual DOM implementation specifically designed for:
//! - **Multi-phase document processing**: Parse → Transform → Diff → Patch
//! - **Type-safe phase transitions**: GATs enforce correct phase usage at compile time
//! - **Efficient diffing**: LCS-based algorithm with stable ID tracking
//! - **Hot reload support**: WebSocket-based incremental updates
//!
//! ## Architecture
//!
//! The crate is built around the Trees That Grow pattern, where a single AST type
//! can represent different compilation phases through type-level parameterization:
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │   Parsed    │ ──► │ Transformed │ ──► │  Diffable   │
//! │   (Raw)     │     │  (Stable)   │     │  (With ID)  │
//! └─────────────┘     └─────────────┘     └─────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use tola_vdom::{Document, Phase, diff};
//!
//! // Parse HTML into initial phase
//! let doc1 = parse_html("<div>Hello</div>")?;
//! let doc2 = parse_html("<div>World</div>")?;
//!
//! // Transform through phases
//! let stable1 = doc1.stabilize();
//! let stable2 = doc2.stabilize();
//!
//! // Generate diff
//! let patches = diff(&stable1, &stable2);
//! ```
//!
//! ## Feature Flags
//!
//! - `std` (default): Enable standard library support
//! - `serde`: Serialization support for all types
//! - `parallel`: Parallel processing with rayon
//! - `hotreload`: WebSocket-based hot reload support
//!
//! ## Modules
//!
//! - [`node`]: Core DOM node types (Element, Text, Document, etc.)
//! - [`phase`]: Phase definitions and transitions
//! - [`diff`]: DOM diffing algorithm
//! - [`transform`]: Transform pipeline utilities
//! - [`id`]: Stable ID generation and tracking

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// Core modules
pub mod attr;
pub mod cache;
pub mod diff;
pub mod family;
pub mod id;
pub mod lcs;
pub mod phase;
pub mod transform;

// Node types
pub mod node;

// Transform implementations
pub mod transforms;

// Macros
#[macro_use]
mod macros;

// Prelude for convenient imports
pub mod prelude {
    //! Commonly used types and traits.

    pub use crate::attr::{Attrs, AttrsExt};
    pub use crate::diff::*;
    pub use crate::family::{
        HeadingFamily, LinkFamily, MediaFamily, OtherFamily, SvgFamily, TagFamily,
    };
    pub use crate::id::{PageSeed, StableId};
    pub use crate::node::{Document, Element, FamilyExt, HasFamilyData, Node, Text};
    pub use crate::phase::{Indexed, Phase, PhaseData, Processed, Raw, Rendered};
    pub use crate::transform::*;
}
