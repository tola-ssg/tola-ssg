//! Actor Message Definitions
//!
//! Defines the message types exchanged between actors in the hot reload system.
//!
//! # Message Flow
//!
//! ```text
//! FsMsg::FileChanged → CompilerMsg::Compile → VdomMsg::Process → WsMsg::Patch
//! ```

use std::path::PathBuf;

use crate::vdom::{diff::Patch, Document, Indexed};

// =============================================================================
// FsActor Messages
// =============================================================================

/// Messages sent to/from the FileSystem Actor
#[derive(Debug)]
pub enum FsMsg {
    /// Files have changed and need recompilation
    FileChanged(Vec<PathBuf>),
    /// Shutdown the actor
    Shutdown,
}

// =============================================================================
// CompilerActor Messages
// =============================================================================

/// Messages sent to the Compiler Actor
#[derive(Debug)]
pub enum CompilerMsg {
    /// Compile the specified files
    Compile(Vec<PathBuf>),
    /// Shutdown the actor
    Shutdown,
}

// =============================================================================
// VdomActor Messages
// =============================================================================

/// Messages sent to the VDOM Actor (the Bridge)
#[derive(Debug)]
pub enum VdomMsg {
    /// Process a compiled VDOM (diff against cache)
    Process {
        /// Source file path
        path: PathBuf,
        /// URL path for the page (e.g., "/blog/post")
        url_path: String,
        /// New VDOM document
        vdom: Document<Indexed>,
    },
    /// Invalidate cache for a specific URL path
    Invalidate { url_path: String },
    /// Clear all cached VDOM
    Clear,
    /// Shutdown the actor
    Shutdown,
}

// =============================================================================
// WsActor Messages
// =============================================================================

/// Messages sent to the WebSocket Actor
pub enum WsMsg {
    /// Send patch operations to all clients for a specific page
    Patch {
        url_path: String,
        patches: Vec<Patch>,
    },
    /// Trigger full page reload
    Reload { reason: String },
    /// Add a new WebSocket client connection
    AddClient(std::net::TcpStream),
    /// A new client has connected (notification only)
    ClientConnected,
    /// Shutdown the actor
    Shutdown,
}

// =============================================================================
// System Messages
// =============================================================================

/// Top-level system messages for coordinating actors
#[derive(Debug)]
pub enum SystemMsg {
    /// Initial build completed
    InitialBuildComplete,
    /// A compile cycle completed
    CompileComplete {
        changed_files: usize,
        duration_ms: u64,
    },
    /// An error occurred
    Error(String),
}
