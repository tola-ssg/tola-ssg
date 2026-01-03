//! Hot Reload Module
//!
//! Provides WebSocket-based live reload for development.
//!
//! # Architecture
//!
//! The hot reload system is built on the Actor model:
//!
//! ```text
//! FsActor → CompilerActor → VdomActor → WsActor → Browser
//!   (watch)    (typst)       (diff)    (broadcast)
//! ```
//!
//! # Modules
//!
//! - `message`: Hot reload message types (reload, patch, css)
//! - `ws`: WebSocket server for client connections
//!
//! # Diff Algorithm
//!
//! The diff algorithm lives in `crate::vdom::diff`.
//! This module handles WebSocket transport and message serialization.

pub mod logic;
pub mod message;
pub mod ws;

