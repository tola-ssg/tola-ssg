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

    /// Create a patch message from VDOM Patches (anchor-based)
    ///
    /// All operations use StableId for targeting. No position indices.
    /// Order of operations doesn't matter for correctness.
    pub fn from_patches(path: &str, patches: &[crate::vdom::diff::Patch]) -> Self {
        use crate::vdom::diff::{Anchor, Patch};

        let ops: Vec<PatchOp> = patches
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
                Patch::ReplaceChildren { target, html } => PatchOp::Html {
                    target: target.to_attr_value(),
                    html: html.clone(),
                },
                Patch::Remove { target } => PatchOp::Remove {
                    target: target.to_attr_value(),
                },
                Patch::Insert { anchor, html } => {
                    let (anchor_type, anchor_id) = match anchor {
                        Anchor::After(id) => ("after", id.to_attr_value()),
                        Anchor::Before(id) => ("before", id.to_attr_value()),
                        Anchor::FirstChildOf(id) => ("first", id.to_attr_value()),
                        Anchor::LastChildOf(id) => ("last", id.to_attr_value()),
                    };
                    PatchOp::Insert {
                        anchor_type: anchor_type.to_string(),
                        anchor_id,
                        html: html.clone(),
                    }
                }
                Patch::Move { target, to } => {
                    let (anchor_type, anchor_id) = match to {
                        Anchor::After(id) => ("after", id.to_attr_value()),
                        Anchor::Before(id) => ("before", id.to_attr_value()),
                        Anchor::FirstChildOf(id) => ("first", id.to_attr_value()),
                        Anchor::LastChildOf(id) => ("last", id.to_attr_value()),
                    };
                    PatchOp::Move {
                        target: target.to_attr_value(),
                        anchor_type: anchor_type.to_string(),
                        anchor_id,
                    }
                }
                Patch::UpdateAttrs { target, attrs } => PatchOp::Attrs {
                    target: target.to_attr_value(),
                    attrs: attrs.clone(),
                },
            })
            .collect();

        Self::Patch {
            path: path.to_string(),
            ops,
        }
    }
}

/// Individual patch operation for DOM updates (anchor-based)
///
/// All operations use StableId for targeting. No position indices.
/// This design ensures order independence and prevents index drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum PatchOp {
    /// Replace entire element's outerHTML
    Replace {
        /// StableId (hex) of element to replace
        target: String,
        /// New HTML content
        html: String,
    },

    /// Update text content (element.textContent = text)
    /// Used for single-text-child elements: `<p>Hello</p>` → `<p>World</p>`
    Text {
        /// StableId (hex) of element
        target: String,
        /// New text content (plain text, will be escaped by textContent)
        text: String,
    },

    /// Replace inner HTML (element.innerHTML = html)
    /// Used when child structure changes but parent element preserved
    Html {
        /// StableId (hex) of element
        target: String,
        /// New innerHTML content
        html: String,
    },

    /// Remove element by ID
    Remove {
        /// StableId (hex) of element to remove
        target: String,
    },

    /// Insert new content at anchor position
    Insert {
        /// Anchor type: "after", "before", "first", "last"
        anchor_type: String,
        /// StableId (hex) of anchor element
        anchor_id: String,
        /// HTML content to insert
        html: String,
    },

    /// Move element to new anchor position
    Move {
        /// StableId (hex) of element to move
        target: String,
        /// Anchor type: "after", "before", "first", "last"
        anchor_type: String,
        /// StableId (hex) of anchor element
        anchor_id: String,
    },

    /// Update element attributes
    Attrs {
        /// StableId (hex) of element
        target: String,
        /// Attributes to set (None = remove attribute)
        attrs: Vec<(String, Option<String>)>,
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

    /// Create an insert-after operation
    pub fn insert_after(
        anchor_id: impl Into<String>,
        html: impl Into<String>,
    ) -> Self {
        Self::Insert {
            anchor_type: "after".to_string(),
            anchor_id: anchor_id.into(),
            html: html.into(),
        }
    }

    /// Create an insert-first-child operation
    pub fn insert_first(
        parent_id: impl Into<String>,
        html: impl Into<String>,
    ) -> Self {
        Self::Insert {
            anchor_type: "first".to_string(),
            anchor_id: parent_id.into(),
            html: html.into(),
        }
    }

    /// Create an attribute update operation
    pub fn attrs(target: impl Into<String>, attrs: Vec<(String, Option<String>)>) -> Self {
        Self::Attrs {
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
                PatchOp::replace("abc123", "<p>New content</p>"),
                PatchOp::text("def456", "Updated Title"),
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
    fn test_anchor_based_insert() {
        use crate::vdom::diff::{Anchor, Patch};
        use crate::vdom::id::StableId;

        let anchor_id = StableId::from_raw(0x1234);

        let patches = vec![
            Patch::Insert {
                anchor: Anchor::After(anchor_id),
                html: "<span>new</span>".to_string(),
            },
        ];

        let msg = HotReloadMessage::from_patches("/test.html", &patches);
        if let HotReloadMessage::Patch { ops, .. } = msg {
            assert_eq!(ops.len(), 1);
            if let PatchOp::Insert { anchor_type, anchor_id: id, .. } = &ops[0] {
                assert_eq!(anchor_type, "after");
                assert_eq!(id, &StableId::from_raw(0x1234).to_attr_value());
            } else {
                panic!("Expected Insert op");
            }
        } else {
            panic!("Expected Patch message");
        }
    }

    #[test]
    fn test_anchor_based_move() {
        use crate::vdom::diff::{Anchor, Patch};
        use crate::vdom::id::StableId;

        let target_id = StableId::from_raw(0x1111);
        let anchor_id = StableId::from_raw(0x2222);

        let patches = vec![
            Patch::Move {
                target: target_id,
                to: Anchor::FirstChildOf(anchor_id),
            },
        ];

        let msg = HotReloadMessage::from_patches("/test.html", &patches);
        if let HotReloadMessage::Patch { ops, .. } = msg {
            assert_eq!(ops.len(), 1);
            if let PatchOp::Move { target, anchor_type, anchor_id: id } = &ops[0] {
                assert_eq!(target, &target_id.to_attr_value());
                assert_eq!(anchor_type, "first");
                assert_eq!(id, &anchor_id.to_attr_value());
            } else {
                panic!("Expected Move op");
            }
        } else {
            panic!("Expected Patch message");
        }
    }
}
