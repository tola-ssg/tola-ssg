//! HTML Renderer: Processed → HTML bytes
//!
//! Final stage of the VDOM pipeline. Serializes the processed document
//! tree to HTML bytes for output.
//!
//! # Design
//!
//! - Uses a simple recursive traversal (no visitor pattern overhead)
//! - Outputs directly to a `Vec<u8>` buffer
//! - Handles text escaping, attribute encoding
//! - Void elements (br, img, etc.) handled correctly
//!
//! # Hot Reload Support
//!
//! When `emit_stable_ids` is enabled, elements include `data-tola-id` attributes
//! for client-side patching via the Tola hot reload runtime.
//!
//! # Note
//!
//! This is a "terminal" transform - it doesn't return a Document<P>,
//! but rather HTML bytes. It consumes Document<Processed> and produces Vec<u8>.

use crate::node::{Document, Element, Node, Text};
use crate::phase::Processed;

// =============================================================================
// HtmlRenderer
// =============================================================================

/// HTML Renderer configuration
#[derive(Debug, Clone)]
pub struct HtmlRendererConfig {
    /// Whether to minify output (remove unnecessary whitespace)
    pub minify: bool,
    /// Indent string (only used when not minifying)
    pub indent: &'static str,
    /// Whether to emit data-tola-id attributes for hot reload
    pub emit_stable_ids: bool,
}

impl Default for HtmlRendererConfig {
    fn default() -> Self {
        Self {
            minify: true,
            indent: "  ",
            emit_stable_ids: false,
        }
    }
}

impl HtmlRendererConfig {
    /// Create a config for development with hot reload support
    pub fn for_dev() -> Self {
        Self {
            minify: false,
            emit_stable_ids: true,
            ..Default::default()
        }
    }

    /// Create a config for production (minified, no IDs)
    pub fn for_production() -> Self {
        Self {
            minify: true,
            emit_stable_ids: false,
            ..Default::default()
        }
    }
}

/// HTML Renderer - converts Processed VDOM to HTML bytes
pub struct HtmlRenderer {
    config: HtmlRendererConfig,
    /// Output buffer
    buffer: Vec<u8>,
    /// Current indentation level (only used when not minifying)
    indent_level: usize,
}

impl HtmlRenderer {
    /// Create a new renderer with default config
    pub fn new() -> Self {
        Self::with_config(HtmlRendererConfig::default())
    }

    /// Create a new renderer with custom config
    pub fn with_config(config: HtmlRendererConfig) -> Self {
        Self {
            config,
            buffer: Vec::with_capacity(16 * 1024), // 16KB initial
            indent_level: 0,
        }
    }

    /// Render a document to HTML bytes
    pub fn render(mut self, doc: Document<Processed>) -> Vec<u8> {
        self.render_element(&doc.root);
        self.buffer
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal rendering methods
    // ─────────────────────────────────────────────────────────────────────────

    fn render_element(&mut self, elem: &Element<Processed>) {
        // Opening tag
        self.write_byte(b'<');
        self.write_str(&elem.tag);

        // Emit data-tola-id for hot reload (if enabled)
        if self.config.emit_stable_ids {
            let stable_id = elem.ext.stable_id();
            self.write_str(" data-tola-id=\"");
            // Use StableId::to_attr_value() to ensure consistent formatting
            self.write_str(&stable_id.to_attr_value());
            self.write_byte(b'"');
        }
        // Attributes
        for (name, value) in &elem.attrs {
            self.write_byte(b' ');
            self.write_str(name);
            self.write_str("=\"");
            self.write_attr_escaped(value);
            self.write_byte(b'"');
        }

        // Void elements (self-closing)
        if is_void_element(&elem.tag) {
            self.write_str(">");
            return;
        }

        self.write_byte(b'>');

        // Children
        // For SVG elements, inner content is raw SVG (already escaped/safe)
        // so we output text children without escaping
        let is_svg = elem.tag == "svg";
        for child in &elem.children {
            self.render_node_in_context(child, is_svg);
        }

        // Closing tag
        self.write_str("</");
        self.write_str(&elem.tag);
        self.write_byte(b'>');
    }

    fn render_node(&mut self, node: &Node<Processed>) {
        self.render_node_in_context(node, false)
    }

    fn render_node_in_context(&mut self, node: &Node<Processed>, raw_text: bool) {
        match node {
            Node::Element(elem) => self.render_element(elem),
            Node::Text(text) => {
                if raw_text {
                    // Output raw (for SVG inner content)
                    self.write_str(&text.content);
                } else {
                    // Normal HTML escaping
                    self.write_text_escaped(&text.content);
                }
            }
        }
    }

    fn render_text(&mut self, text: &Text<Processed>) {
        self.write_text_escaped(&text.content);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Buffer operations
    // ─────────────────────────────────────────────────────────────────────────

    #[inline]
    fn write_byte(&mut self, b: u8) {
        self.buffer.push(b);
    }

    #[inline]
    fn write_str(&mut self, s: &str) {
        self.buffer.extend_from_slice(s.as_bytes());
    }

    /// Write attribute value with HTML entity escaping
    fn write_attr_escaped(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '&' => self.write_str("&amp;"),
                '"' => self.write_str("&quot;"),
                '<' => self.write_str("&lt;"),
                '>' => self.write_str("&gt;"),
                _ => {
                    let mut buf = [0u8; 4];
                    self.buffer.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
        }
    }

    /// Write text content with HTML entity escaping
    fn write_text_escaped(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '&' => self.write_str("&amp;"),
                '<' => self.write_str("&lt;"),
                '>' => self.write_str("&gt;"),
                _ => {
                    let mut buf = [0u8; 4];
                    self.buffer.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
        }
    }
}

impl Default for HtmlRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Check if tag is a void element (no closing tag)
fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img"
            | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
    )
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::FamilyExt;
    use crate::phase::ProcessedDocExt;
    use smallvec::smallvec;

    fn make_element(tag: &str, children: Vec<Node<Processed>>) -> Element<Processed> {
        Element {
            tag: tag.to_string(),
            attrs: vec![],
            children: children.into_iter().collect(),
            ext: FamilyExt::Other(Default::default()),
        }
    }

    fn make_text(content: &str) -> Node<Processed> {
        Node::Text(Text {
            content: content.to_string(),
            ext: (),
        })
    }

    #[test]
    fn test_simple_element() {
        let doc = Document {
            root: make_element("div", vec![make_text("Hello")]),
            ext: ProcessedDocExt::default(),
        };

        let html = HtmlRenderer::new().render(doc);
        assert_eq!(String::from_utf8_lossy(&html), "<div>Hello</div>");
    }

    #[test]
    fn test_void_element() {
        let doc = Document {
            root: make_element("div", vec![
                Node::Element(Box::new(Element {
                    tag: "br".to_string(),
                    attrs: vec![],
                    children: smallvec![],
                    ext: FamilyExt::Other(Default::default()),
                })),
            ]),
            ext: ProcessedDocExt::default(),
        };

        let html = HtmlRenderer::new().render(doc);
        assert_eq!(String::from_utf8_lossy(&html), "<div><br></div>");
    }

    #[test]
    fn test_attribute_escaping() {
        let doc = Document {
            root: Element {
                tag: "a".to_string(),
                attrs: vec![("href".to_string(), "test?a=1&b=2".to_string())],
                children: smallvec![make_text("Link")],
                ext: FamilyExt::Other(Default::default()),
            },
            ext: ProcessedDocExt::default(),
        };

        let html = HtmlRenderer::new().render(doc);
        assert_eq!(
            String::from_utf8_lossy(&html),
            r#"<a href="test?a=1&amp;b=2">Link</a>"#
        );
    }

    #[test]
    fn test_text_escaping() {
        let doc = Document {
            root: make_element("p", vec![make_text("1 < 2 && 3 > 2")]),
            ext: ProcessedDocExt::default(),
        };

        let html = HtmlRenderer::new().render(doc);
        assert_eq!(
            String::from_utf8_lossy(&html),
            "<p>1 &lt; 2 &amp;&amp; 3 &gt; 2</p>"
        );
    }

    #[test]
    fn test_nested_elements() {
        let doc = Document {
            root: make_element("html", vec![
                Node::Element(Box::new(make_element("head", vec![
                    Node::Element(Box::new(Element {
                        tag: "title".to_string(),
                        attrs: vec![],
                        children: smallvec![make_text("Test")],
                        ext: FamilyExt::Other(Default::default()),
                    })),
                ]))),
                Node::Element(Box::new(make_element("body", vec![
                    make_text("Content"),
                ]))),
            ]),
            ext: ProcessedDocExt::default(),
        };

        let html = HtmlRenderer::new().render(doc);
        assert_eq!(
            String::from_utf8_lossy(&html),
            "<html><head><title>Test</title></head><body>Content</body></html>"
        );
    }
}
