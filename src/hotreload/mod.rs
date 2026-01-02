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

pub mod cache;
pub mod diff;
pub mod lcs;
pub mod message;
pub mod protocol;
pub mod server;

// Public API - these are used by serve.rs and watch.rs
pub use server::{HotReloadServer, broadcast_reload, broadcast_patches};
pub use cache::VDOM_CACHE;

// Re-export for future use (allow unused for now)
#[allow(unused_imports)]
pub use message::{HotReloadMessage, PatchOp};
#[allow(unused_imports)]
pub use server::broadcast;
#[allow(unused_imports)]
pub use diff::{DiffResult, DiffStats, IndexedDiffResult, StableIdPatch, diff_indexed_documents};
#[allow(unused_imports)]
pub use protocol::{ServerMessage, PatchOp as BinaryPatchOp, ARCH_FINGERPRINT};
#[allow(unused_imports)]
pub use lcs::{Edit, LcsResult, LcsStats, diff_sequences};
