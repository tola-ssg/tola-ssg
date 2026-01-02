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
//! # Status
//!
//! This module is under development. The actors are fully implemented but
//! not yet integrated into the main watch loop. Enable with `--features actor`.
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

// Allow dead code during development - actors not yet integrated into runtime
#[cfg(feature = "actor")]
#[allow(dead_code)]
pub mod messages;

#[cfg(feature = "actor")]
#[allow(dead_code)]
pub mod fs;

#[cfg(feature = "actor")]
#[allow(dead_code)]
pub mod compiler;

#[cfg(feature = "actor")]
#[allow(dead_code)]
pub mod vdom;

#[cfg(feature = "actor")]
#[allow(dead_code)]
pub mod ws;

#[cfg(feature = "actor")]
#[allow(unused_imports)]
pub use messages::*;
