//! Conversion from typst-html output to Raw VDOM
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
use typst::introspection::Introspector;
use typst::syntax::Span;
use typst_html::{HtmlDocument, HtmlElement, HtmlFrame, HtmlNode};

use super::node::{Document, Element, Node, Text};
use super::phase::{Raw, RawDocExt, RawTextExt};

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
        let mut element = Element::auto_with_span(&tag, &attrs, elem.span);
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
            HtmlNode::Text(text, span) => Some(Node::Text(Text {
                content: text.to_string(),
                ext: RawTextExt::with_span(*span),
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
    /// Uses `typst_svg::svg_html_frame()` to render the frame as inline SVG,
    /// then wraps it in an Element node.
    ///
    /// Note: Frames don't have a direct Span, so we use detached Span.
    /// StableId is computed from the SVG content hash during indexing.
    fn convert_frame_to_svg(&mut self, frame: &HtmlFrame) -> Node<Raw> {
        // Render frame to SVG string using typst-svg
        let svg_string = typst_svg::svg_html_frame(
            &frame.inner,
            frame.text_size,
            frame.id.as_deref(),
            &frame.link_points,
            self.introspector,
        );

        // Create SVG wrapper element
        // StableId will be computed from the svg_string content during indexing
        let mut svg_elem = Element::auto_with_span("svg", &[], Span::detached());
        svg_elem.attrs = vec![]; // No dynamic attributes - use content hash for StableId
        // Store the raw SVG content as a text child
        svg_elem.children = SmallVec::from_vec(vec![Node::Text(Text {
            content: svg_string,
            ext: RawTextExt::detached(),
        })]);

        Node::Element(Box::new(svg_elem))
    }
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