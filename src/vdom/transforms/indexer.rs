//! Indexer Transform: Raw → Indexed
//!
//! Traverses the Raw VDOM tree and:
//! 1. Assigns unique NodeId and StableId to each element/frame
//! 2. Extracts family-specific data from attributes
//! 3. Collects node references by family type
//!
//! This is the first transform in the pipeline after conversion from typst-html.

use smallvec::SmallVec;

use crate::vdom::family::{
    FamilyKind, HeadingIndexedData, LinkIndexedData, LinkType,
    MediaIndexedData, SvgIndexedData, identify_family_kind,
};
use crate::vdom::id::StableId;
use crate::vdom::node::{Document, Element, FamilyExt, Frame, Node, NodeId, Text};
use crate::vdom::phase::{Indexed, IndexedDocExt, IndexedElemExt, IndexedFrameExt, Raw};
use crate::vdom::transform::Transform;

// =============================================================================
// Indexer Transform
// =============================================================================

/// Indexer: Raw → Indexed phase transformation
///
/// # Responsibilities
///
/// - Assign unique NodeId to each element and frame
/// - Extract family-specific data from element attributes
/// - Track node counts and family references in document extension
///
/// # Usage
///
/// ```ignore
/// let raw_doc: Document<Raw> = vdom::from_typst_html(&html_doc);
/// let indexed_doc: Document<Indexed> = Indexer::new().transform(raw_doc);
/// ```
pub struct Indexer {
    /// Next available node ID
    next_id: u32,
    /// Collected SVG node IDs
    svg_nodes: Vec<NodeId>,
    /// Collected link node IDs
    link_nodes: Vec<NodeId>,
    /// Collected heading node IDs
    heading_nodes: Vec<NodeId>,
    /// Collected media node IDs
    media_nodes: Vec<NodeId>,
    /// Collected frame node IDs
    frame_nodes: Vec<NodeId>,
    /// Total element count
    element_count: usize,
    /// Total text count
    text_count: usize,
    /// Total frame count
    frame_count: usize,
}

impl Indexer {
    /// Create a new Indexer
    pub fn new() -> Self {
        Self {
            next_id: 0,
            svg_nodes: Vec::new(),
            link_nodes: Vec::new(),
            heading_nodes: Vec::new(),
            media_nodes: Vec::new(),
            frame_nodes: Vec::new(),
            element_count: 0,
            text_count: 0,
            frame_count: 0,
        }
    }

    /// Generate next NodeId
    fn next_node_id(&mut self) -> NodeId {
        let id = NodeId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Index a document
    fn index_document(&mut self, doc: Document<Raw>) -> Document<Indexed> {
        let root = self.index_element(doc.root, 0); // Root is at position 0
        let node_count = self.next_id;

        Document {
            root,
            ext: IndexedDocExt {
                base: doc.ext,
                node_count,
                element_count: self.element_count,
                text_count: self.text_count,
                frame_count: self.frame_count,
                svg_nodes: std::mem::take(&mut self.svg_nodes),
                link_nodes: std::mem::take(&mut self.link_nodes),
                heading_nodes: std::mem::take(&mut self.heading_nodes),
                media_nodes: std::mem::take(&mut self.media_nodes),
                frame_nodes: std::mem::take(&mut self.frame_nodes),
            },
        }
    }

    /// Index an element, extracting family-specific data
    ///
    /// Children are indexed first (bottom-up) so we can collect their StableIds
    /// for content hash computation.
    fn index_element(&mut self, elem: Element<Raw>, position: usize) -> Element<Indexed> {
        self.element_count += 1;
        let node_id = self.next_node_id();

        // Extract data we need before moving children
        let tag = elem.tag;
        let attrs = elem.attrs;

        // Index children first (bottom-up) to collect their StableIds
        let (children, child_stable_ids): (SmallVec<[Node<Indexed>; 8]>, Vec<StableId>) = elem
            .children
            .into_iter()
            .enumerate()
            .map(|(pos, child)| {
                let indexed_child = self.index_node(child, pos);
                let stable_id = match &indexed_child {
                    Node::Element(e) => e.ext.stable_id(),
                    Node::Text(t) => t.ext.stable_id,
                    Node::Frame(f) => f.ext.stable_id(),
                };
                (indexed_child, stable_id)
            })
            .unzip();

        // Determine family
        let kind = identify_family_kind(&tag, &attrs);

        // Generate StableId using pure content hash
        let stable_id = StableId::for_element(&tag, &attrs, &child_stable_ids, position);

        let ext = self.create_indexed_ext(node_id, stable_id, kind, &tag, &attrs);

        // Track node by family
        match kind {
            FamilyKind::Svg => self.svg_nodes.push(node_id),
            FamilyKind::Link => self.link_nodes.push(node_id),
            FamilyKind::Heading => self.heading_nodes.push(node_id),
            FamilyKind::Media => self.media_nodes.push(node_id),
            FamilyKind::Other => {}
        }

        Element {
            tag,
            attrs,
            children,
            ext,
        }
    }

    /// Create FamilyExt<Indexed> with extracted data and StableId
    fn create_indexed_ext(
        &self,
        node_id: NodeId,
        stable_id: StableId,
        kind: FamilyKind,
        tag: &str,
        attrs: &[(String, String)],
    ) -> FamilyExt<Indexed> {
        match kind {
            FamilyKind::Svg => {
                FamilyExt::Svg(IndexedElemExt {
                    stable_id,
                    node_id,
                    family_data: self.extract_svg_data(tag, attrs),
                })
            }
            FamilyKind::Link => {
                FamilyExt::Link(IndexedElemExt {
                    stable_id,
                    node_id,
                    family_data: self.extract_link_data(attrs),
                })
            }
            FamilyKind::Heading => {
                FamilyExt::Heading(IndexedElemExt {
                    stable_id,
                    node_id,
                    family_data: self.extract_heading_data(tag, attrs),
                })
            }
            FamilyKind::Media => {
                FamilyExt::Media(IndexedElemExt {
                    stable_id,
                    node_id,
                    family_data: self.extract_media_data(attrs),
                })
            }
            FamilyKind::Other => {
                FamilyExt::Other(IndexedElemExt {
                    stable_id,
                    node_id,
                    family_data: (),
                })
            }
        }
    }

    /// Extract SVG-specific indexed data
    fn extract_svg_data(&self, tag: &str, attrs: &[(String, String)]) -> SvgIndexedData {
        let viewbox = get_attr(attrs, "viewBox").map(String::from);
        let dimensions = self.parse_dimensions(attrs);

        SvgIndexedData {
            is_root: tag == "svg",
            viewbox,
            dimensions,
        }
    }

    /// Extract Link-specific indexed data
    fn extract_link_data(&self, attrs: &[(String, String)]) -> LinkIndexedData {
        let href = get_attr(attrs, "href").map(String::from);
        let link_type = href.as_ref().map(|h| classify_link(h)).unwrap_or_default();

        LinkIndexedData {
            link_type,
            original_href: href,
        }
    }

    /// Extract Heading-specific indexed data
    fn extract_heading_data(&self, tag: &str, attrs: &[(String, String)]) -> HeadingIndexedData {
        let level = HeadingIndexedData::level_from_tag(tag);
        let original_id = get_attr(attrs, "id").map(String::from);

        HeadingIndexedData { level, original_id }
    }

    /// Extract Media-specific indexed data
    fn extract_media_data(&self, attrs: &[(String, String)]) -> MediaIndexedData {
        let src = get_attr(attrs, "src").map(String::from);
        let is_svg_image = src
            .as_ref()
            .map(|s| s.to_lowercase().ends_with(".svg"))
            .unwrap_or(false);

        MediaIndexedData { src, is_svg_image }
    }

    /// Parse width/height attributes
    fn parse_dimensions(&self, attrs: &[(String, String)]) -> Option<(f32, f32)> {
        let width = get_attr(attrs, "width").and_then(|w| parse_dimension(w));
        let height = get_attr(attrs, "height").and_then(|h| parse_dimension(h));
        width.zip(height)
    }

    /// Index a node with its position in parent
    fn index_node(&mut self, node: Node<Raw>, position: usize) -> Node<Indexed> {
        match node {
            Node::Element(elem) => Node::Element(Box::new(self.index_element(*elem, position))),
            Node::Text(text) => {
                self.text_count += 1;

                // Generate StableId for text node using content hash + position
                let stable_id = StableId::for_text(&text.content, position);

                Node::Text(Text {
                    content: text.content,
                    ext: crate::vdom::phase::IndexedTextExt::new(stable_id),
                })
            }
            // Note: Frame nodes should not exist at Raw phase anymore.
            // Frames are converted to SVG Elements during from_typst_html().
            // This branch exists for type completeness only.
            Node::Frame(_frame) => {
                self.frame_count += 1;
                let node_id = self.next_node_id();
                self.frame_nodes.push(node_id);

                // Frame nodes use content hash with position
                let stable_id = StableId::for_frame(node_id.0 as usize, position);

                Node::Frame(Box::new(Frame::new(IndexedFrameExt {
                    stable_id,
                    node_id,
                    frame_id: 0,
                    estimated_svg_size: 0,
                })))
            }
        }
    }
}

impl Default for Indexer {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform<Raw> for Indexer {
    type To = Indexed;

    fn transform(mut self, doc: Document<Raw>) -> Document<Indexed> {
        self.index_document(doc)
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Get attribute value from attrs slice
fn get_attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

/// Classify link type from href value
fn classify_link(href: &str) -> LinkType {
    if href.starts_with('#') {
        LinkType::Fragment
    } else if href.starts_with("http://") || href.starts_with("https://") {
        LinkType::External
    } else if href.starts_with('/') {
        LinkType::Absolute
    } else if href.starts_with("./") || href.starts_with("../") {
        LinkType::Relative
    } else if href.contains("://") || href.contains(':') {
        // Contains protocol separator (://), or colon for mailto:, tel:, etc.
        LinkType::External
    } else {
        // Relative path without prefix
        LinkType::Relative
    }
}

/// Parse dimension value (e.g., "100", "100px", "100.5")
fn parse_dimension(value: &str) -> Option<f32> {
    let trimmed = value
        .trim_end_matches("px")
        .trim_end_matches("pt")
        .trim_end_matches("em")
        .trim_end_matches('%');
    trimmed.parse().ok()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_link() {
        assert_eq!(classify_link("#section"), LinkType::Fragment);
        assert_eq!(classify_link("https://example.com"), LinkType::External);
        assert_eq!(classify_link("http://example.com"), LinkType::External);
        assert_eq!(classify_link("/about"), LinkType::Absolute);
        assert_eq!(classify_link("./page.html"), LinkType::Relative);
        assert_eq!(classify_link("../index.html"), LinkType::Relative);
        assert_eq!(classify_link("page.html"), LinkType::Relative);
        assert_eq!(classify_link("mailto:test@example.com"), LinkType::External);
    }

    #[test]
    fn test_parse_dimension() {
        assert_eq!(parse_dimension("100"), Some(100.0));
        assert_eq!(parse_dimension("100px"), Some(100.0));
        assert_eq!(parse_dimension("100.5"), Some(100.5));
        assert_eq!(parse_dimension("100pt"), Some(100.0));
        assert_eq!(parse_dimension("invalid"), None);
    }

    #[test]
    fn test_indexer_basic() {
        use crate::vdom::phase::RawDocExt;

        // Create a simple Raw document
        let raw_doc = Document {
            root: Element::auto("html", &[]),
            ext: RawDocExt::default(),
        };

        let indexer = Indexer::new();
        let indexed = indexer.transform(raw_doc);

        assert_eq!(indexed.ext.node_count, 1);
        assert_eq!(indexed.ext.element_count, 1);
    }
}
