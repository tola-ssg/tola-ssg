//! Actor Message Definitions
//!
//! Defines the message types exchanged between actors in the hot reload system.

use std::path::PathBuf;

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
// WsActor Messages
// =============================================================================

/// Messages sent to the WebSocket Actor
#[derive(Debug)]
pub enum WsMsg {
    /// Send patch operations to all clients
    Patch(Vec<crate::hotreload::StableIdPatch>),
    /// Trigger full page reload
    Reload { reason: String },
    /// A new client has connected
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
