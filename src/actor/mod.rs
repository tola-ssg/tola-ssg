//! Actor System for Hot Reload
//!
//! Message-passing concurrency for watch mode:
//!
//! ```text
//! FsActor ──► CompilerActor ──► VdomActor ──► WsActor
//! (watch)       (typst)         (diff)      (broadcast)
//! ```
//!
//! # Module Structure
//!
//! - `messages` - Message types for inter-actor communication
//! - `fs` - File system watcher with debouncing
//! - `compiler` - Typst compilation wrapper
//! - `vdom` - VDOM diffing and caching
//! - `ws` - WebSocket broadcast
//! - `coordinator` - Wires up and runs actors

pub mod messages;
pub mod fs;
pub mod compiler;
pub mod vdom;
pub mod ws;
pub mod coordinator;

pub use coordinator::Coordinator;

