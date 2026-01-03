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
//! │  (notify)   │                   │  (typst)     │
//! └─────────────┘                   └──────┬───────┘
//!                                          │ Process (VDOM)
//!                                          ▼
//!                                   ┌──────────────┐
//!                                   │  VdomActor   │
//!                                   │  (bridge)    │
//!                                   └──────┬───────┘
//!                                          │ Patch/Reload
//!                                          ▼
//!                                   ┌──────────────┐
//!                                   │   WsActor    │
//!                                   │ (broadcast)  │
//!                                   └──────────────┘
//! ```
//!
//! # Actor Responsibilities
//!
//! | Actor | Responsibility |
//! |-------|----------------|
//! | `FsActor` | File watching, debouncing, event routing |
//! | `CompilerActor` | Typst compilation only (AST → VDOM) |
//! | `VdomActor` | TTG conversion, Pipeline, Diff, Cache |
//! | `WsActor` | WebSocket broadcast (pure relay) |
//!
//! # Watcher-First Pattern
//!
//! The `FsActor` implements the "Watcher-First" pattern from ds-store-killer:
//! 1. Establish channel and watcher FIRST (starts buffering events)
//! 2. Perform initial build (events accumulate in channel)
//! 3. Enter event loop (drains buffered events)
//!
//! This eliminates the "vacuum period" where events could be lost.

#[allow(dead_code)]
pub mod messages;

#[allow(dead_code)]
pub mod fs;

#[allow(dead_code)]
pub mod compiler;

#[allow(dead_code)]
pub mod vdom;

#[allow(dead_code)]
pub mod ws;

#[allow(dead_code)]
pub mod coordinator;

// Re-exports for convenience
#[allow(unused_imports)]
pub use coordinator::Coordinator;

#[allow(unused_imports)]
pub use messages::*;

