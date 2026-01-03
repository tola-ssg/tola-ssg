//! Text node type.

use compact_str::CompactString;

use crate::phase::PhaseData;

/// Text content node.
///
/// Represents text content within the document.
#[derive(Debug, Clone)]
pub struct Text<P: PhaseData> {
    /// Text content.
    pub content: CompactString,
    /// Phase-specific text extension data.
    pub ext: P::TextExt,
}

impl<P: PhaseData> Text<P> {
    /// Create a new text node.
    pub fn new(content: impl Into<CompactString>) -> Self
    where
        P::TextExt: Default,
    {
        Self {
            content: content.into(),
            ext: P::TextExt::default(),
        }
    }

    /// Create a text node with explicit extension.
    pub fn with_ext(content: impl Into<CompactString>, ext: P::TextExt) -> Self {
        Self {
            content: content.into(),
            ext,
        }
    }

    /// Get text content as str.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Check if text is empty or whitespace only.
    pub fn is_blank(&self) -> bool {
        self.content.trim().is_empty()
    }

    /// Get trimmed content.
    pub fn trimmed(&self) -> &str {
        self.content.trim()
    }
}
