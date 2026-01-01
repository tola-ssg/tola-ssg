//! Hot Reload Message Protocol
//!
//! Defines the JSON message format for WebSocket communication between
//! the development server and browser clients.
//!
//! # Message Types
//!
//! - `reload`: Trigger full page reload
//! - `patch`: Apply incremental DOM patches
//! - `css`: Inject updated CSS (no layout recalc)
//! - `ping`/`pong`: Keep connection alive
//!
//! # Patch Operations
//!
//! Fine-grained DOM updates:
//! - `replace`: Replace element content
//! - `update`: Update element attributes
//! - `insert`: Insert new nodes
//! - `remove`: Remove nodes
//! - `move`: Move node to new position

// Many methods are not yet used but will be for incremental hot reload
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Hot reload message sent over WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HotReloadMessage {
    /// Full page reload (fallback when diff is too complex)
    Reload {
        /// Optional reason for reload
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    /// Incremental DOM patch
    Patch {
        /// Target page path (e.g., "/blog/post.html")
        path: String,
        /// Sequence of patch operations
        ops: Vec<PatchOp>,
    },

    /// CSS-only update (fast path - no layout recalc)
    Css {
        /// CSS selector or stylesheet href
        target: String,
        /// New CSS content
        content: String,
    },

    /// Keep-alive ping (server → client)
    Ping {
        /// Timestamp for latency measurement
        ts: u64,
    },

    /// Keep-alive pong (client → server)
    Pong {
        /// Echo back the timestamp
        ts: u64,
    },

    /// Connection established
    Connected {
        /// Server version for compatibility check
        version: String,
    },
}

impl HotReloadMessage {
    /// Create a reload message
    pub fn reload() -> Self {
        Self::Reload { reason: None }
    }

    /// Create a reload message with reason
    pub fn reload_with_reason(reason: impl Into<String>) -> Self {
        Self::Reload {
            reason: Some(reason.into()),
        }
    }

    /// Create a patch message
    pub fn patch(path: impl Into<String>, ops: Vec<PatchOp>) -> Self {
        Self::Patch {
            path: path.into(),
            ops,
        }
    }

    /// Create a connected message
    pub fn connected() -> Self {
        Self::Connected {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Create a ping message
    pub fn ping() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self::Ping { ts }
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"type":"reload"}"#.to_string())
    }

    /// Parse from JSON string
    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }

    /// Create a patch message from VDOM Patches
    ///
    /// # Index Drift Prevention
    ///
    /// Position-based operations (`RemoveAtPosition`, `TextAtPosition`) are sorted
    /// to prevent index drift during sequential execution:
    /// - `RemoveAtPosition` ops with same parent are sorted by position descending
    ///   (remove from back to front to preserve indices)
    /// - Remove operations execute before inserts
    pub fn from_patches(path: &str, patches: &[crate::vdom::diff::Patch]) -> Self {
        use crate::vdom::diff::Patch;

        let mut ops: Vec<PatchOp> = patches
            .iter()
            .map(|p| match p {
                Patch::Replace { target, html } => PatchOp::Replace {
                    target: target.to_attr_value(),
                    html: html.clone(),
                },
                Patch::UpdateText { target, text } => PatchOp::Text {
                    target: target.to_attr_value(),
                    text: text.clone(),
                },
                Patch::UpdateTextAtPosition { parent, position, text } => PatchOp::TextAtPosition {
                    parent: parent.to_attr_value(),
                    position: *position,
                    text: text.clone(),
                },
                Patch::Remove { target } => PatchOp::Remove {
                    target: target.to_attr_value(),
                },
                Patch::RemoveAtPosition { parent, position } => PatchOp::RemoveAtPosition {
                    parent: parent.to_attr_value(),
                    position: *position,
                },
                Patch::Insert { parent, position, html } => PatchOp::Insert {
                    parent: parent.to_attr_value(),
                    position: position.to_string(),
                    html: html.clone(),
                },
                Patch::Move { target, new_parent, position } => PatchOp::Move {
                    from: target.to_attr_value(),
                    to_parent: new_parent.to_attr_value(),
                    position: position.to_string(),
                },
                Patch::UpdateAttrs { target, attrs } => PatchOp::Update {
                    target: target.to_attr_value(),
                    attrs: attrs.clone(),
                },
            })
            .collect();

        // Sort operations to prevent index drift:
        // 1. RemoveAtPosition with same parent: sort by position descending (back to front)
        // 2. All remove operations should come before inserts
        ops.sort_by(|a, b| {
            use std::cmp::Ordering;
            match (a, b) {
                // Both RemoveAtPosition with same parent: sort by position descending
                (
                    PatchOp::RemoveAtPosition { parent: p1, position: pos1 },
                    PatchOp::RemoveAtPosition { parent: p2, position: pos2 },
                ) if p1 == p2 => pos2.cmp(pos1), // Descending: remove from back first

                // RemoveAtPosition before other operations
                (PatchOp::RemoveAtPosition { .. }, PatchOp::Insert { .. }) => Ordering::Less,
                (PatchOp::Insert { .. }, PatchOp::RemoveAtPosition { .. }) => Ordering::Greater,

                // Remove (by ID) before Insert
                (PatchOp::Remove { .. }, PatchOp::Insert { .. }) => Ordering::Less,
                (PatchOp::Insert { .. }, PatchOp::Remove { .. }) => Ordering::Greater,

                // Keep other operations in original order
                _ => Ordering::Equal,
            }
        });

        Self::Patch {
            path: path.to_string(),
            ops,
        }
    }
}

/// Individual patch operation for DOM updates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum PatchOp {
    /// Replace entire element or text content
    Replace {
        /// CSS selector or StableId (hex) to target
        target: String,
        /// New HTML content
        html: String,
    },

    /// Update element attributes
    #[serde(rename = "attrs")]
    Update {
        /// CSS selector or StableId (hex) to target
        target: String,
        /// Attributes to set (None = remove attribute)
        attrs: Vec<(String, Option<String>)>,
    },

    /// Insert new node(s)
    Insert {
        /// Parent selector or StableId (hex)
        parent: String,
        /// Position index (for StableId) or "beforeend"/"afterbegin" (for CSS)
        position: String,
        /// HTML content to insert
        html: String,
    },

    /// Remove node(s)
    Remove {
        /// CSS selector or StableId (hex) to remove
        target: String,
    },

    /// Remove node at a specific child position
    /// Used when text nodes don't have their own data-tola-id
    #[serde(rename = "remove_at_pos")]
    RemoveAtPosition {
        /// Parent element's StableId (hex)
        parent: String,
        /// Child position (0-based index)
        position: u32,
    },

    /// Move node to new position
    Move {
        /// Source StableId (hex) - renamed from 'from' for JS compatibility
        #[serde(rename = "target")]
        from: String,
        /// Destination parent StableId (hex)
        #[serde(rename = "new_parent")]
        to_parent: String,
        /// Position index within parent
        position: String,
    },

    /// Update text content only (fast path)
    Text {
        /// CSS selector or StableId (hex) to target
        target: String,
        /// New text content
        text: String,
    },

    /// Update text content at a specific child position
    /// Used when text nodes don't have their own data-tola-id
    #[serde(rename = "text_at_pos")]
    TextAtPosition {
        /// Parent element's StableId (hex)
        parent: String,
        /// Child position (0-based index)
        position: u32,
        /// New text content
        text: String,
    },
}

impl PatchOp {
    /// Create a replace operation
    pub fn replace(target: impl Into<String>, html: impl Into<String>) -> Self {
        Self::Replace {
            target: target.into(),
            html: html.into(),
        }
    }

    /// Create a text update operation
    pub fn text(target: impl Into<String>, text: impl Into<String>) -> Self {
        Self::Text {
            target: target.into(),
            text: text.into(),
        }
    }

    /// Create a remove operation
    pub fn remove(target: impl Into<String>) -> Self {
        Self::Remove {
            target: target.into(),
        }
    }

    /// Create an insert operation
    pub fn insert(
        parent: impl Into<String>,
        position: impl Into<String>,
        html: impl Into<String>,
    ) -> Self {
        Self::Insert {
            parent: parent.into(),
            position: position.into(),
            html: html.into(),
        }
    }

    /// Create an attribute update operation
    pub fn update_attrs(target: impl Into<String>, attrs: Vec<(String, Option<String>)>) -> Self {
        Self::Update {
            target: target.into(),
            attrs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = HotReloadMessage::patch(
            "/index.html",
            vec![
                PatchOp::replace("#content", "<p>New content</p>"),
                PatchOp::text("h1.title", "Updated Title"),
            ],
        );

        let json = msg.to_json();
        assert!(json.contains(r#""type":"patch""#));
        assert!(json.contains(r#""path":"/index.html""#));

        let parsed = HotReloadMessage::from_json(&json).unwrap();
        match parsed {
            HotReloadMessage::Patch { path, ops } => {
                assert_eq!(path, "/index.html");
                assert_eq!(ops.len(), 2);
            }
            _ => panic!("Expected Patch message"),
        }
    }

    #[test]
    fn test_reload_message() {
        let msg = HotReloadMessage::reload_with_reason("template changed");
        let json = msg.to_json();
        assert!(json.contains(r#""type":"reload""#));
        assert!(json.contains(r#""reason":"template changed""#));
    }

    #[test]
    fn test_remove_at_position_sorted_descending() {
        // Test that RemoveAtPosition ops with same parent are sorted by position descending
        // This prevents index drift when removing multiple children
        use crate::vdom::diff::Patch;
        use crate::vdom::id::StableId;

        let parent_id = StableId::from_raw(0x1234);

        // Create patches in ascending order (wrong execution order)
        let patches = vec![
            Patch::RemoveAtPosition {
                parent: parent_id,
                position: 0,
            },
            Patch::RemoveAtPosition {
                parent: parent_id,
                position: 2,
            },
            Patch::RemoveAtPosition {
                parent: parent_id,
                position: 1,
            },
        ];

        let msg = HotReloadMessage::from_patches("/test.html", &patches);
        if let HotReloadMessage::Patch { ops, .. } = msg {
            // Should be sorted descending: 2, 1, 0
            let positions: Vec<u32> = ops
                .iter()
                .filter_map(|op| {
                    if let PatchOp::RemoveAtPosition { position, .. } = op {
                        Some(*position)
                    } else {
                        None
                    }
                })
                .collect();

            assert_eq!(positions, vec![2, 1, 0], "RemoveAtPosition should be sorted descending");
        } else {
            panic!("Expected Patch message");
        }
    }

    #[test]
    fn test_removes_before_inserts() {
        // Test that remove operations come before insert operations
        use crate::vdom::diff::Patch;
        use crate::vdom::id::StableId;

        let parent_id = StableId::from_raw(0x1234);
        let target_id = StableId::from_raw(0x5678);

        let patches = vec![
            Patch::Insert {
                parent: parent_id,
                position: 0,
                html: "<span>new</span>".to_string(),
            },
            Patch::Remove { target: target_id },
            Patch::RemoveAtPosition {
                parent: parent_id,
                position: 1,
            },
        ];

        let msg = HotReloadMessage::from_patches("/test.html", &patches);
        if let HotReloadMessage::Patch { ops, .. } = msg {
            // Find positions of each op type
            let mut first_remove_idx = None;
            let mut first_remove_at_pos_idx = None;
            let mut first_insert_idx = None;

            for (i, op) in ops.iter().enumerate() {
                match op {
                    PatchOp::Remove { .. } if first_remove_idx.is_none() => first_remove_idx = Some(i),
                    PatchOp::RemoveAtPosition { .. } if first_remove_at_pos_idx.is_none() => {
                        first_remove_at_pos_idx = Some(i)
                    }
                    PatchOp::Insert { .. } if first_insert_idx.is_none() => first_insert_idx = Some(i),
                    _ => {}
                }
            }

            // Both remove operations should come before insert
            assert!(
                first_remove_at_pos_idx.unwrap() < first_insert_idx.unwrap(),
                "RemoveAtPosition should come before Insert"
            );
            assert!(
                first_remove_idx.unwrap() < first_insert_idx.unwrap(),
                "Remove should come before Insert"
            );
        } else {
            panic!("Expected Patch message");
        }
    }
}
