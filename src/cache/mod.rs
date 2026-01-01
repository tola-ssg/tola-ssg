//! VDOM Cache System
//!
//! Provides persistent caching for compiled VDOM documents.
//!
//! # Backends
//!
//! - **Redb** (optional): High-performance embedded database, enabled with `cache` feature
//!
//! # Features
//!
//! The redb backend is gated behind the `cache` feature flag:
//!
//! ```toml
//! [features]
//! cache = ["redb"]
//! ```

#[cfg(feature = "cache")]
mod redb_storage;

#[cfg(feature = "cache")]
pub use redb_storage::*;
