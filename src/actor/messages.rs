//! Actor Message Definitions
//!
//! Message types for inter-actor communication.
//!
//! ```text
//! FsActor ──Compile──► CompilerActor ──Process──► VdomActor ──Patch──► WsActor
//! ```

use std::path::PathBuf;

use crate::vdom::{diff::Patch, Document, Indexed};

// =============================================================================
// CompilerActor Messages
// =============================================================================

/// Messages to Compiler Actor
#[derive(Debug)]
pub enum CompilerMsg {
    /// Compile files
    Compile(Vec<PathBuf>),
    /// Compile content files that depend on changed deps
    #[allow(dead_code)] // Reserved for future dependency-aware rebuild
    CompileDependents(Vec<PathBuf>),
    /// Full rebuild (config changed)
    FullRebuild,
    /// Shutdown
    Shutdown,
}

// =============================================================================
// VdomActor Messages
// =============================================================================

/// Messages to VDOM Actor
#[derive(Debug)]
pub enum VdomMsg {
    /// Populate cache (initial build)
    Populate {
        entries: Vec<(String, Document<Indexed>)>,
    },
    /// Process compiled VDOM
    Process {
        path: PathBuf,
        url_path: String,
        vdom: Document<Indexed>,
    },
    /// Trigger reload
    Reload { reason: String },
    /// File skipped
    Skip,
    /// Invalidate cache for URL path
    #[allow(dead_code)] // Reserved for selective cache invalidation
    Invalidate { url_path: String },
    /// Clear cache
    Clear,
    /// Shutdown
    #[allow(dead_code)] // Reserved for graceful shutdown
    Shutdown,
}

// =============================================================================
// WsActor Messages
// =============================================================================

/// Messages to WebSocket Actor
pub enum WsMsg {
    /// Send patches
    Patch {
        url_path: String,
        patches: Vec<Patch>,
    },
    /// Full reload
    Reload { reason: String },
    /// Add client
    AddClient(std::net::TcpStream),
    /// Client connected notification
    #[allow(dead_code)] // Reserved for connection tracking
    ClientConnected,
    /// Shutdown
    #[allow(dead_code)] // Reserved for graceful shutdown
    Shutdown,
}
