//! VDOM Cache System
//!
//! Provides persistent caching for compiled VDOM documents.
//! Supports zero-copy loading via rkyv for near-instant hot restarts.
//!
//! # Architecture Safety
//!
//! Cache files are architecture-specific due to rkyv's memory layout sensitivity.
//! Files are named with architecture fingerprints (e.g., `index_aarch64_macos.rkyv`)
//! and automatically invalidated when the architecture doesn't match.
//!
//! # Backends
//!
//! - **File-based** (default): Simple file-per-entry storage
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

mod storage;

#[cfg(feature = "cache")]
mod redb_storage;

pub use storage::*;

#[cfg(feature = "cache")]
pub use redb_storage::*;
