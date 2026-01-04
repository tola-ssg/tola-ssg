//! Text node type
//!
//! Simple text content nodes in the VDOM tree.

use crate::phase::PhaseData;

// =============================================================================
// Text<P>
// =============================================================================

/// Text content node
#[derive(Debug, Clone)]
pub struct Text<P: PhaseData> {
    /// Text content
    pub content: String,
    /// Phase-specific extension data
    pub ext: P::TextExt,
}

impl<P: PhaseData> Text<P> {
    /// Create a new text node
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ext: P::TextExt::default(),
        }
    }

    /// Check if text content is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get text length in bytes
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if text is only whitespace
    pub fn is_whitespace(&self) -> bool {
        self.content.trim().is_empty()
    }

    /// Get trimmed content
    pub fn trimmed(&self) -> &str {
        self.content.trim()
    }
}

// Note: Frame<P> has been removed.
// Typst frames are now eagerly converted to SVG Elements in convert.rs.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::Raw;

    #[test]
    fn test_text_node() {
        let text: Text<Raw> = Text::new("  hello world  ");
        assert!(!text.is_empty());
        assert!(!text.is_whitespace());
        assert_eq!(text.trimmed(), "hello world");
    }
}
