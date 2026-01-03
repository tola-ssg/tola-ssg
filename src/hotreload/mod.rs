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
//! The diff algorithm lives in `crate::vdom::diff`.
//! This module handles WebSocket transport and message serialization.
//!
//! # VDOM Cache
//!
//! VDOM caching is handled by `crate::vdom::VdomCache`.
//! Each consumer (watch.rs, actor) owns its own cache instance.

pub mod message;
pub mod server;

// Public API
