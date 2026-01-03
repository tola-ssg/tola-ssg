//! HTML renderer transform.
//!
//! Renders processed documents to HTML strings.

/// HTML renderer configuration.
pub struct HtmlRenderer {
    /// Whether to pretty-print with indentation.
    pub pretty: bool,
    /// Indentation string (default: "  ").
    pub indent: String,
}

impl HtmlRenderer {
    /// Create a new renderer with default settings.
    pub fn new() -> Self {
        Self {
            pretty: false,
            indent: "  ".to_string(),
        }
    }

    /// Enable pretty printing.
    pub fn pretty(mut self) -> Self {
        self.pretty = true;
        self
    }

    /// Set indentation string.
    pub fn with_indent(mut self, indent: impl Into<String>) -> Self {
        self.indent = indent.into();
        self
    }
}

impl Default for HtmlRenderer {
    fn default() -> Self {
        Self::new()
    }
}
