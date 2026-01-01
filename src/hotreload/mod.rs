//! Hot Reload Module
//!
//! Provides WebSocket-based live reload for development:
//! - WebSocket server for push updates to browsers
//! - Message protocol for incremental DOM updates
//! - JavaScript client runtime for applying patches
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                       Hot Reload System                             │
//! │                                                                     │
//! │  ┌──────────────┐    ┌───────────────┐    ┌──────────────────────┐  │
//! │  │ File Watcher │───►│ VDOM Compiler │───►│ Diff Engine          │  │
//! │  │ (notify)     │    │               │    │ (old_vdom, new_vdom) │  │
//! │  └──────────────┘    └───────────────┘    └──────────┬───────────┘  │
//! │                                                      │              │
//! │                                                      ▼              │
//! │  ┌──────────────┐    ┌───────────────┐    ┌──────────────────────┐  │
//! │  │ Browser      │◄───│ WebSocket     │◄───│ Patch Message        │  │
//! │  │ (JS Runtime) │    │ Server        │    │ (JSON)               │  │
//! │  └──────────────┘    └───────────────┘    └──────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Message Protocol
//!
//! Messages are JSON-encoded:
//!
//! - `reload`: Full page reload (fallback)
//! - `patch`: Incremental DOM update (primary)
//! - `css`: CSS-only update (fast path)
//! - `ping`/`pong`: Keep-alive
//!
//! # Diff Algorithm
//!
//! The diff algorithm has been moved to `crate::vdom::diff` for better modularity.
//! This module re-exports the diff types for backward compatibility.

pub mod cache;
pub mod diff;
pub mod lcs;
pub mod message;
pub mod protocol;
pub mod server;

// Public API - these are used by serve.rs and watch.rs
pub use cache::VDOM_CACHE;
pub use server::{broadcast_patches, broadcast_reload, HotReloadServer};

// Re-export diff types for backward compatibility
// The actual algorithm is in crate::vdom::diff, re-exported via sub-modules
pub use diff::{diff_indexed_documents, IndexedDiffResult, DiffStats, StableIdPatch};
pub use lcs::{diff_sequences, Edit, LcsResult, LcsStats};

// Legacy re-exports (allow unused for now)
#[allow(unused_imports)]
pub use message::{HotReloadMessage, PatchOp};
#[allow(unused_imports)]
pub use protocol::{PatchOp as BinaryPatchOp, ServerMessage, ARCH_FINGERPRINT};
#[allow(unused_imports)]
pub use server::broadcast;
