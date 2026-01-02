//! Binary Patch Protocol for hot reload
//!
//! Defines the VDOM patch operations using rkyv for zero-copy serialization.
//! This protocol replaces the JSON-based HotReloadMessage.
//!
//! # Wire Format
//!
//! All messages are serialized using rkyv, which provides:
//! - Zero-copy deserialization
//! - Architecture-specific memory layout
//! - Compact binary representation
//!
//! # Protocol Design
//!
//! Based on the PLAN.md specification, the protocol supports:
//! - `PatchOp`: Individual DOM operations (Replace, Move, UpdateAttrs, etc.)
//! - `ServerMessage`: Top-level message types (Patch, FullSync, Connected)

#![allow(dead_code)] // Protocol types will be used when diffing is integrated

use crate::vdom::id::StableId;

// =============================================================================
// Patch Operations
// =============================================================================

/// Individual DOM patch operation
///
/// Each operation targets a node by its `StableId` and describes
/// the modification to apply.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "rkyv", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
pub enum PatchOp {
    /// Replace entire node with new HTML
    ///
    /// The target node is completely replaced, including all children.
    Replace {
        target: StableId,
        /// Pre-rendered HTML fragment (UTF-8 bytes)
        html: Vec<u8>,
    },

    /// Update text content of a text node
    ///
    /// More efficient than Replace for simple text changes.
    UpdateText {
        target: StableId,
        text: String,
    },

    /// Remove a node from the DOM
    Remove {
        target: StableId,
    },

    /// Insert a new node
    ///
    /// The node is inserted as a child of `parent` at `position`.
    Insert {
        parent: StableId,
        /// Position among siblings (0 = first child)
        position: u32,
        /// Pre-rendered HTML fragment
        html: Vec<u8>,
    },

    /// Move a node to a new location
    ///
    /// This is more efficient than Remove + Insert because it
    /// preserves event listeners and DOM state.
    Move {
        target: StableId,
        new_parent: StableId,
        /// Position among new siblings (0 = first child)
        position: u32,
    },

    /// Update element attributes
    ///
    /// Each attribute can be set (Some) or removed (None).
    UpdateAttrs {
        target: StableId,
        /// Attribute updates: (name, Some(value)) to set, (name, None) to remove
        attrs: Vec<(String, Option<String>)>,
    },
}

impl PatchOp {
    /// Get the target StableId for this operation
    pub fn target(&self) -> StableId {
        match self {
            PatchOp::Replace { target, .. } => *target,
            PatchOp::UpdateText { target, .. } => *target,
            PatchOp::Remove { target } => *target,
            PatchOp::Insert { parent, .. } => *parent,
            PatchOp::Move { target, .. } => *target,
            PatchOp::UpdateAttrs { target, .. } => *target,
        }
    }

    /// Check if this operation modifies content (vs structural)
    pub fn is_content_change(&self) -> bool {
        matches!(self, PatchOp::UpdateText { .. } | PatchOp::UpdateAttrs { .. })
    }

    /// Check if this operation modifies structure
    pub fn is_structural_change(&self) -> bool {
        matches!(
            self,
            PatchOp::Replace { .. }
                | PatchOp::Remove { .. }
                | PatchOp::Insert { .. }
                | PatchOp::Move { .. }
        )
    }
}

// =============================================================================
// Server Messages
// =============================================================================

/// Top-level message from server to client
#[derive(Debug, Clone)]
#[cfg_attr(feature = "rkyv", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
pub enum ServerMessage {
    /// Incremental patch update
    ///
    /// Contains a list of operations to apply to the current DOM.
    Patch {
        /// Ordered list of operations
        ops: Vec<PatchOp>,
    },

    /// Full page reload required
    ///
    /// Sent when incremental patching isn't possible or efficient.
    FullReload {
        reason: String,
    },

    /// Full sync with complete HTML
    ///
    /// Sent on initial connection or after reconnection.
    FullSync {
        /// Root element's StableId
        root_id: StableId,
        /// Complete HTML content
        html: Vec<u8>,
    },

    /// Connection established acknowledgment
    Connected {
        /// Server version
        version: String,
        /// Protocol version for compatibility checking
        protocol_version: u32,
    },

    /// Heartbeat/ping
    Ping,
}

impl ServerMessage {
    /// Current protocol version
    pub const PROTOCOL_VERSION: u32 = 1;

    /// Create a new Connected message with current version
    pub fn connected() -> Self {
        Self::Connected {
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: Self::PROTOCOL_VERSION,
        }
    }

    /// Create a Patch message from operations
    pub fn patch(ops: Vec<PatchOp>) -> Self {
        Self::Patch { ops }
    }

    /// Create a FullReload message
    pub fn full_reload(reason: impl Into<String>) -> Self {
        Self::FullReload {
            reason: reason.into(),
        }
    }

    /// Create a FullSync message
    pub fn full_sync(root_id: StableId, html: impl Into<Vec<u8>>) -> Self {
        Self::FullSync {
            root_id,
            html: html.into(),
        }
    }

    /// Check if this message requires full page handling
    pub fn is_full(&self) -> bool {
        matches!(self, Self::FullReload { .. } | Self::FullSync { .. })
    }

    /// Get patch operations if this is a Patch message
    pub fn as_patch(&self) -> Option<&[PatchOp]> {
        match self {
            Self::Patch { ops } => Some(ops),
            _ => None,
        }
    }
}

// =============================================================================
// Serialization helpers (when rkyv feature is enabled)
// =============================================================================

#[cfg(feature = "rkyv")]
pub mod serialize {
    use super::*;

    /// Serialize a ServerMessage to bytes
    pub fn to_bytes(msg: &ServerMessage) -> Result<rkyv::util::AlignedVec, rkyv::rancor::Error> {
        rkyv::to_bytes(msg)
    }

    /// Deserialize a ServerMessage from bytes (unchecked, for trusted data)
    ///
    /// # Safety
    ///
    /// The bytes must have been serialized by a compatible version of rkyv
    /// on the same architecture.
    pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> &rkyv::Archived<ServerMessage> {
        rkyv::access_unchecked::<rkyv::Archived<ServerMessage>>(bytes)
    }
}

// =============================================================================
// Platform fingerprint for cache compatibility
// =============================================================================

/// Compile-time architecture fingerprint
///
/// Used to ensure rkyv caches are only used on matching architectures.
pub const ARCH_FINGERPRINT: &str = {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    { "x86_64_linux" }

    #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
    { "x86_64_macos" }

    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    { "aarch64_macos" }

    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    { "aarch64_linux" }

    #[cfg(all(target_arch = "x86_64", target_os = "windows"))]
    { "x86_64_windows" }

    #[cfg(not(any(
        all(target_arch = "x86_64", target_os = "linux"),
        all(target_arch = "x86_64", target_os = "macos"),
        all(target_arch = "aarch64", target_os = "macos"),
        all(target_arch = "aarch64", target_os = "linux"),
        all(target_arch = "x86_64", target_os = "windows"),
    )))]
    { concat!(env!("CARGO_CFG_TARGET_ARCH"), "_", env!("CARGO_CFG_TARGET_OS")) }
};

/// Generate cache file path with architecture fingerprint
pub fn cache_path(base: &str, name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(base).join(format!("{}_{}.rkyv", name, ARCH_FINGERPRINT))
}

/// Check if a cache file is valid for current architecture
pub fn is_cache_valid(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.contains(ARCH_FINGERPRINT))
        .unwrap_or(false)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_op_target() {
        let op = PatchOp::UpdateText {
            target: StableId::from_raw(42),
            text: "hello".to_string(),
        };
        assert_eq!(op.target().as_raw(), 42);
    }

    #[test]
    fn test_server_message_connected() {
        let msg = ServerMessage::connected();
        match msg {
            ServerMessage::Connected { protocol_version, .. } => {
                assert_eq!(protocol_version, ServerMessage::PROTOCOL_VERSION);
            }
            _ => panic!("Expected Connected message"),
        }
    }

    #[test]
    fn test_arch_fingerprint() {
        // Should be a non-empty string
        assert!(!ARCH_FINGERPRINT.is_empty());

        // Should contain architecture info
        #[cfg(target_arch = "aarch64")]
        assert!(ARCH_FINGERPRINT.contains("aarch64"));

        #[cfg(target_arch = "x86_64")]
        assert!(ARCH_FINGERPRINT.contains("x86_64"));
    }

    #[test]
    fn test_cache_path() {
        let path = cache_path(".cache", "index");
        let path_str = path.to_str().unwrap();
        assert!(path_str.contains(".cache"));
        assert!(path_str.contains("index"));
        assert!(path_str.contains(ARCH_FINGERPRINT));
        assert!(path_str.ends_with(".rkyv"));
    }
}
