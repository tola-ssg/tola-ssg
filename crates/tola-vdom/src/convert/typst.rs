//! Conversion from typst HtmlDocument to Raw VDOM
//!
//! This module bridges typst-html's Element type to our VDOM.
//! The conversion happens at build time, creating a Raw phase tree
//! that can then be indexed and processed.
//!
//! # Key Design Decision
//!
//! **Frames are converted to SVG Elements immediately** during conversion,
//! not stored as Frame nodes. This is because:
//! 1. `typst_svg::svg_html_frame()` requires `Introspector`
//! 2. `Introspector` is only available from `HtmlDocument`
//! 3. After conversion, we don't want to carry `Introspector` around
//!
//! # Flow
//!
//! ```text
//! typst_html::HtmlDocument
//!         │
//!         ▼ from_typst_html()
//! Document<Raw>  (Frames already rendered as SVG Elements)
//!         │
//!         ▼ (Indexer transform)
//! Document<Indexed>
//! ```

use smallvec::SmallVec;
use typst_batch::typst::introspection::Introspector;
use typst_batch::typst_html::{HtmlDocument, HtmlElement, HtmlFrame, HtmlNode};

use crate::node::{Document, Element, Node, Text};
use crate::phase::{Raw, RawDocExt, RawTextExt};
use crate::span::SourceSpan;

// =============================================================================
// Converter
// =============================================================================

/// Internal state for conversion
struct Converter<'a> {
    /// Reference to introspector for Frame→SVG rendering
    introspector: &'a Introspector,
}

impl<'a> Converter<'a> {
    fn new(introspector: &'a Introspector) -> Self {
        Self {
            introspector,
        }
    }

    /// Convert typst HtmlDocument to Raw VDOM Document
    fn convert_document(&mut self, doc: &HtmlDocument) -> Document<Raw> {
        let root = self.convert_element(&doc.root);
        Document {
            root,
            ext: RawDocExt::default(),
        }
    }

    /// Convert typst HtmlElement to Raw VDOM Element
    fn convert_element(&mut self, elem: &HtmlElement) -> Element<Raw> {
        // Convert tag name
        let tag = elem.tag.resolve().to_string();

        // Convert attributes: HtmlAttrs -> Vec<(String, String)>
        let attrs: Vec<(String, String)> = elem
            .attrs
            .0
            .iter()
            .map(|(k, v)| (k.resolve().to_string(), v.to_string()))
            .collect();

        // Convert children recursively
        let children: SmallVec<[Node<Raw>; 8]> = elem
            .children
            .iter()
            .filter_map(|child| self.convert_node(child))
            .collect();

        // Create element with auto-detected family and captured Span
        // Convert typst Span to our SourceSpan abstraction
        let mut element = Element::auto_with_span(&tag, &attrs, elem.span.into());
        element.attrs = attrs;
        element.children = children;

        element
    }

    /// Convert typst HtmlNode to Raw VDOM Node
    ///
    /// Returns None for Tag nodes (introspection markers not needed in VDOM)
    fn convert_node(&mut self, node: &HtmlNode) -> Option<Node<Raw>> {
        match node {
            // Skip introspection tags - they're internal to typst
            HtmlNode::Tag(_) => None,

            // Text nodes - capture Span for StableId generation
            // Convert typst Span to our SourceSpan abstraction
            HtmlNode::Text(text, span) => Some(Node::Text(Text {
                content: text.to_string(),
                ext: RawTextExt::with_span((*span).into()),
            })),

            // Recursive element conversion
            HtmlNode::Element(elem) => {
                Some(Node::Element(Box::new(self.convert_element(elem))))
            }

            // Frame nodes → SVG Elements (rendered immediately)
            HtmlNode::Frame(frame) => Some(self.convert_frame_to_svg(frame)),
        }
    }

    /// Convert typst HtmlFrame directly to SVG Element
    ///
    /// Uses `typst_svg::svg_html_frame()` to render the frame as inline SVG.
    /// The returned SVG string already contains the complete `<svg>` element,
    /// so we parse it to extract attributes and create a proper Element.
    ///
    /// Note: Frames don't have a direct Span, so we use detached Span.
    /// StableId is computed from the SVG content hash during indexing.
    fn convert_frame_to_svg(&mut self, frame: &HtmlFrame) -> Node<Raw> {
        // Render frame to SVG string using typst-svg
        // This returns a COMPLETE <svg>...</svg> string
        let svg_string = typst_batch::typst_svg::svg_html_frame(
            &frame.inner,
            frame.text_size,
            frame.id.as_deref(),
            &frame.link_points,
            self.introspector,
        );

        // Parse the SVG string to extract attributes and inner content
        // Format: <svg attr1="val1" attr2="val2">...inner content...</svg>
        let (attrs, inner_content) = parse_svg_string(&svg_string);

        // Create SVG element with parsed attributes
        // Use detached SourceSpan since frames don't have direct source spans
        let mut svg_elem = Element::auto_with_span("svg", &attrs, SourceSpan::detached());
        svg_elem.attrs = attrs;

        // The inner content is raw SVG (paths, groups, etc.)
        // Store as a single text child - renderer will output it unescaped
        // because it's inside an SVG element
        if !inner_content.is_empty() {
            svg_elem.children = SmallVec::from_vec(vec![Node::Text(Text {
                content: inner_content,
                ext: RawTextExt::detached(),
            })]);
        }

        Node::Element(Box::new(svg_elem))
    }
}

/// Parse an SVG string to extract attributes and inner content
///
/// Input: `<svg viewBox="0 0 100 100" class="foo">inner content</svg>`
/// Output: (vec![("viewBox", "0 0 100 100"), ("class", "foo")], "inner content")
fn parse_svg_string(svg: &str) -> (Vec<(String, String)>, String) {
    // Find the opening tag end
    let Some(tag_start) = svg.find('<') else {
        return (vec![], svg.to_string());
    };

    let Some(tag_end) = svg[tag_start..].find('>') else {
        return (vec![], svg.to_string());
    };
    let tag_end = tag_start + tag_end;

    // Check if it's self-closing
    let is_self_closing = svg[..tag_end].ends_with('/');

    // Extract opening tag content: "svg viewBox="0 0 100 100" ..."
    let tag_content = &svg[tag_start + 1..if is_self_closing { tag_end - 1 } else { tag_end }];
    let tag_content = tag_content.trim();

    // Skip "svg" tag name
    let attr_start = tag_content.find(char::is_whitespace).unwrap_or(tag_content.len());
    let attr_str = &tag_content[attr_start..].trim();

    // Parse attributes
    let attrs = parse_attributes(attr_str);

    // Extract inner content (between > and </svg>)
    let inner_content = if is_self_closing {
        String::new()
    } else {
        let content_start = tag_end + 1;
        let content_end = svg.rfind("</svg>").unwrap_or(svg.len());
        svg[content_start..content_end].to_string()
    };

    (attrs, inner_content)
}

/// Parse HTML-style attributes from a string
///
/// Input: `viewBox="0 0 100 100" class="foo" disabled`
/// Output: vec![("viewBox", "0 0 100 100"), ("class", "foo"), ("disabled", "")]
fn parse_attributes(s: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        // Skip whitespace
        if c.is_whitespace() {
            continue;
        }

        // Read attribute name
        let mut name = String::new();
        name.push(c);
        while let Some(&next) = chars.peek() {
            if next == '=' || next.is_whitespace() {
                break;
            }
            name.push(chars.next().unwrap());
        }

        // Skip whitespace
        while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
            chars.next();
        }

        // Check for value
        if chars.peek() == Some(&'=') {
            chars.next(); // consume '='

            // Skip whitespace
            while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
                chars.next();
            }

            // Read value
            let value = if chars.peek() == Some(&'"') || chars.peek() == Some(&'\'') {
                let quote = chars.next().unwrap();
                let mut val = String::new();
                for c in chars.by_ref() {
                    if c == quote {
                        break;
                    }
                    val.push(c);
                }
                val
            } else {
                // Unquoted value (read until whitespace)
                let mut val = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    val.push(chars.next().unwrap());
                }
                val
            };

            attrs.push((name, value));
        } else {
            // Boolean attribute (no value)
            attrs.push((name, String::new()));
        }
    }

    attrs
}

// =============================================================================
// Public API
// =============================================================================

/// Convert a typst-html HtmlDocument to a Raw phase VDOM Document
///
/// Frames are immediately rendered to SVG Elements using the document's
/// introspector. This avoids carrying around the introspector reference.
///
/// # Example
///
/// ```ignore
/// let result = typst_lib::compile_document(path, root, "tola-meta")?;
/// let raw_doc = vdom::from_typst_html(&result.document);
/// ```
pub fn from_typst_html(doc: &HtmlDocument) -> Document<Raw> {
    let mut converter = Converter::new(&doc.introspector);
    converter.convert_document(doc)
}

/// Convert with source metadata
///
/// Includes source file information in the document extension.
pub fn from_typst_html_with_meta(
    doc: &HtmlDocument,
    source_path: Option<String>,
    is_index: bool,
) -> Document<Raw> {
    let mut converter = Converter::new(&doc.introspector);
    let mut document = converter.convert_document(doc);

    document.ext = RawDocExt {
        source_path,
        is_index,
        dependencies: Vec::new(),
        content_meta: None,
    };
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_svg_string_basic() {
        let svg = r#"<svg viewBox="0 0 100 100" class="test">inner content</svg>"#;
        let (attrs, inner) = parse_svg_string(svg);

        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0], ("viewBox".to_string(), "0 0 100 100".to_string()));
        assert_eq!(attrs[1], ("class".to_string(), "test".to_string()));
        assert_eq!(inner, "inner content");
    }

    #[test]
    fn test_parse_svg_string_complex() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 595.28 841.89"><g>paths...</g></svg>"#;
        let (attrs, inner) = parse_svg_string(svg);

        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].0, "xmlns");
        assert_eq!(attrs[1].0, "viewBox");
        assert!(inner.contains("<g>"));
    }

    #[test]
    fn test_parse_svg_string_self_closing() {
        let svg = r#"<svg viewBox="0 0 10 10"/>"#;
        let (attrs, inner) = parse_svg_string(svg);

        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].0, "viewBox");
        assert!(inner.is_empty());
    }

    #[test]
    fn test_parse_attributes() {
        let attrs = parse_attributes(r#"a="1" b='2' c=3 disabled"#);
        assert_eq!(attrs.len(), 4);
        assert_eq!(attrs[0], ("a".to_string(), "1".to_string()));
        assert_eq!(attrs[1], ("b".to_string(), "2".to_string()));
        assert_eq!(attrs[2], ("c".to_string(), "3".to_string()));
        assert_eq!(attrs[3], ("disabled".to_string(), "".to_string()));
    }
}