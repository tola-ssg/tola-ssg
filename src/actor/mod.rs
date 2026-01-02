//! Actor Concurrency Model for Hot Reload
//!
//! This module implements a message-passing actor architecture for the
//! watch/hot-reload system. Benefits over the current blocking model:
//!
//! - **No nested Mutex**: Each actor owns its state exclusively
//! - **Non-blocking compilation**: File events buffer while compiling
//! - **Clean shutdown**: Actors receive shutdown messages gracefully
//! - **SyncTeX support**: Request-response for source location queries
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐    FileChanged    ┌──────────────┐
//! │   FsActor   │ ─────────────────►│ CompilerActor│
//! │  (notify)   │                   │  (rayon)     │
//! └─────────────┘                   └──────┬───────┘
//!                                          │ Patch/Reload
//!                                          ▼
//!                                   ┌──────────────┐
//!                                   │   WsActor    │
//!                                   │ (broadcast)  │
//!                                   └──────────────┘
//! ```
//!
//! # Feature Flag
//!
//! This module requires the `actor` feature:
//!
//! ```toml
//! [dependencies]
//! tola = { features = ["actor"] }
//! ```
//!
//! # Watcher-First Pattern
//!
//! The `FsActor` implements the "Watcher-First" pattern from ds-store-killer:
//! 1. Establish channel and watcher FIRST (starts buffering events)
//! 2. Perform initial build (events accumulate in channel)
//! 3. Enter event loop (drains buffered events)
//!
//! This eliminates the "vacuum period" where events could be lost.

#[cfg(feature = "actor")]
pub mod messages;

#[cfg(feature = "actor")]
pub mod fs;

#[cfg(feature = "actor")]
pub mod compiler;

#[cfg(feature = "actor")]
pub mod ws;

#[cfg(feature = "actor")]
pub use messages::*;
