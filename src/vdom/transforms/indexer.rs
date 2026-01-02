//! Indexer Transform: Raw → Indexed
//!
//! Traverses the Raw VDOM tree and:
//! 1. Assigns unique NodeId and StableId to each element/frame
//! 2. Extracts family-specific data from attributes
//! 3. Collects node references by family type
//!
//! # Occurrence-based StableId
//!
//! Instead of using absolute position for StableId disambiguation,
//! we use "occurrence index" - how many times the same content key
//! appeared before in the sibling list.
//!
//! This enables Move detection: when `[A, B]` becomes `[B, A]`,
//! the IDs stay the same (Move) instead of changing (Replace).

use std::collections::HashMap;
use smallvec::SmallVec;

use crate::vdom::family::{
    FamilyKind, HeadingIndexedData, LinkIndexedData, LinkType,
    MediaIndexedData, SvgIndexedData, identify_family_kind,
};
use crate::vdom::id::StableId;
use crate::vdom::node::{Document, Element, FamilyExt, Node, NodeId, Text};
use crate::vdom::phase::{Indexed, IndexedDocExt, IndexedElemExt, Raw};
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
    /// Page-specific seed for globally unique StableIds
    /// When set, all StableIds will include this seed, making them
    /// unique across different pages.
    page_seed: u64,
    /// Collected SVG node IDs
    svg_nodes: Vec<NodeId>,
    /// Collected link node IDs
    link_nodes: Vec<NodeId>,
    /// Collected heading node IDs
    heading_nodes: Vec<NodeId>,
    /// Collected media node IDs
    media_nodes: Vec<NodeId>,
    /// Total element count
    element_count: usize,
    /// Total text count
    text_count: usize,
}

impl Indexer {
    /// Create a new Indexer
    pub fn new() -> Self {
        Self {
            next_id: 0,
            page_seed: 0,
            svg_nodes: Vec::new(),
            link_nodes: Vec::new(),
            heading_nodes: Vec::new(),
            media_nodes: Vec::new(),
            element_count: 0,
            text_count: 0,
        }
    }

    /// Set page-specific seed for globally unique StableIds.
    ///
    /// When building for hot reload, pass the page's URL path to ensure
    /// StableIds are unique across different pages. This allows the browser
    /// to safely ignore patches for elements that don't exist in current DOM.
    ///
    /// # Example
    /// ```ignore
    /// let indexed = Indexer::new()
    ///     .with_page_seed("/blog/post.html")
    ///     .transform(raw_doc);
    /// ```
    pub fn with_page_seed(mut self, page_path: &str) -> Self {
        use crate::utils::hash::StableHasher;
        self.page_seed = StableHasher::new()
            .update_str("__page__")
            .update_str(page_path)
            .finish();
        self
    }

    /// Generate next NodeId
    fn next_node_id(&mut self) -> NodeId {
        let id = NodeId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Index a document
    fn index_document(&mut self, doc: Document<Raw>) -> Document<Indexed> {
        // Use page_seed as root seed - makes all StableIds globally unique
        let root = self.index_element(doc.root, 0, self.page_seed);
        let node_count = self.next_id;

        Document {
            root,
            ext: IndexedDocExt {
                base: doc.ext,
                node_count,
                element_count: self.element_count,
                text_count: self.text_count,
                svg_nodes: std::mem::take(&mut self.svg_nodes),
                link_nodes: std::mem::take(&mut self.link_nodes),
                heading_nodes: std::mem::take(&mut self.heading_nodes),
                media_nodes: std::mem::take(&mut self.media_nodes),
            },
        }
    }

    /// Index an element, extracting family-specific data
    ///
    /// Children are indexed first (bottom-up) so we can collect their StableIds
    /// for content hash computation.
    ///
    /// # Occurrence-based ID
    ///
    /// The `occurrence` parameter is the count of how many siblings with the
    /// same content key appeared before this element. This enables Move detection.
    fn index_element(&mut self, elem: Element<Raw>, occurrence: usize, parent_seed: u64) -> Element<Indexed> {
        self.element_count += 1;
        let node_id = self.next_node_id();

        // Extract data we need before moving children
        let tag = elem.tag;
        let attrs = elem.attrs;

        // Generate StableId FIRST (using parent seed)
        // We need this ID to use as seed for children
        let kind = identify_family_kind(&tag, &attrs);

        // Children processing logic moved AFTER ID generation because we need my_stable_id
        // But `StableId::for_element` signature previously took children IDs?
        // Wait, current signature is `(tag, attrs, children_ids, occ, seed)`.
        // BUT children_ids are ignored now!
        // So we can compute our ID first.
        let child_ids_placeholder: &[StableId] = &[]; // Ignored by id.rs
        let stable_id = StableId::for_element(&tag, &attrs, child_ids_placeholder, occurrence, parent_seed);

        // Use my StableId as seed for children (cast to u64 via StableId internal representation)
        // Since StableId wraps u64, we can access it (if pub) or re-hash it.
        // StableId(pub u64).
        let my_seed = stable_id.0;

        // Index children with occurrence-based IDs + parent seed
        let (children, _) = self.index_children(elem.children, my_seed);

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

    /// Index a list of children using occurrence-based IDs
    ///
    /// For each child, we compute a "content key" and count how many times
    /// it has appeared before in this sibling list. This count becomes the
    /// `occurrence` parameter for StableId generation.
    ///
    /// Content key:
    /// - Element: (tag, key_attrs) where key_attrs are id/key/data-key-*
    /// - Text: content string
    /// - Frame: frame_id
    fn index_children(
        &mut self,
        children: SmallVec<[Node<Raw>; 8]>,
        parent_seed: u64,
    ) -> (SmallVec<[Node<Indexed>; 8]>, Vec<StableId>) {
        // Track occurrence count for each content key
        let mut occurrence_counts: HashMap<ContentKey, usize> = HashMap::new();

        children
            .into_iter()
            .map(|child| {
                // Compute content key and get occurrence count
                let content_key = ContentKey::from_raw_node(&child);
                let occurrence = occurrence_counts.entry(content_key).or_insert(0);
                let current_occurrence = *occurrence;
                *occurrence += 1;

                // Index the child with its occurrence and parent seed
                let indexed_child = self.index_node_with_occurrence(child, current_occurrence, parent_seed);
                let stable_id = match &indexed_child {
                    Node::Element(e) => e.ext.stable_id(),
                    Node::Text(t) => t.ext.stable_id,
                };
                (indexed_child, stable_id)
            })
            .unzip()
    }

    /// Index a single node with its computed occurrence
    fn index_node_with_occurrence(&mut self, node: Node<Raw>, occurrence: usize, parent_seed: u64) -> Node<Indexed> {
        match node {
            Node::Element(elem) => {
                Node::Element(Box::new(self.index_element(*elem, occurrence, parent_seed)))
            }
            Node::Text(text) => {
                self.text_count += 1;
                // Text node ID is based on occurrence only, NOT content.
                // This ensures "Hello" → "World" is recognized as Keep + UpdateText
                // instead of Delete + Insert (which would cause position drift)
                let stable_id = StableId::for_text(occurrence, parent_seed);
                Node::Text(Text {
                    content: text.content,
                    ext: crate::vdom::phase::IndexedTextExt::new(stable_id),
                })
            }
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
        let width = get_attr(attrs, "width").and_then(parse_dimension);
        let height = get_attr(attrs, "height").and_then(parse_dimension);
        width.zip(height)
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
// ContentKey: For occurrence counting
// =============================================================================

/// Content key for occurrence counting
///
/// Used to determine if two nodes are "the same" for occurrence counting.
/// Two nodes with the same ContentKey in the same sibling list will get
/// different occurrence indices.
///
/// # Design Note
///
/// Text nodes use a fixed key (not content-based) so that text content changes
/// are handled as Keep + UpdateText instead of Delete + Insert.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ContentKey {
    /// Element: (tag, key_attrs_hash)
    /// Only id/key/data-key-* attributes are considered
    Element { tag: String, key_attrs_hash: u64 },
    /// Text: all text nodes share the same key type
    /// (occurrence index differentiates them, not content).
    /// This intentionally ignores content so that text updates are detected
    /// as "Same Node, New Content" (Keep + UpdateText/ReplaceChildren)
    /// rather than "Different Node" (Delete + Insert).
    Text,
}

impl ContentKey {
    /// Create a ContentKey from a Raw node
    fn from_raw_node(node: &Node<Raw>) -> Self {
        use crate::utils::hash::StableHasher;

        match node {
            Node::Element(elem) => {
                let mut hasher = StableHasher::new();
                // Only hash key attributes (id, key, data-key-*)
                for (k, v) in &elem.attrs {
                    if k == "id" || k == "key" || k.starts_with("data-key") {
                        hasher = hasher.update_str(k).update_str(v);
                    }
                }
                ContentKey::Element {
                    tag: elem.tag.clone(),
                    key_attrs_hash: hasher.finish(),
                }
            }
            // Text nodes all share the same key type
            // The occurrence index will differentiate them
            Node::Text(_) => ContentKey::Text,
        }
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

    #[test]
    fn test_occurrence_based_stable_id() {
        use crate::vdom::phase::{RawDocExt, RawElemExt, RawTextExt};
        use crate::vdom::node::FamilyExt;
        use crate::vdom::attr::Attrs;

        // Helper to create Raw elements with proper ext type
        fn make_element(tag: &str, attrs: Attrs, children: SmallVec<[Node<Raw>; 8]>) -> Element<Raw> {
            Element {
                tag: tag.to_string(),
                attrs,
                children,
                ext: FamilyExt::Other(RawElemExt::detached()),
            }
        }

        fn make_text(content: &str) -> Text<Raw> {
            Text {
                content: content.to_string(),
                ext: RawTextExt::default(),
            }
        }

        // Create document with identical siblings: [p(A), p(B), p(A)]
        // The two p(A) elements should have different IDs due to occurrence
        let raw_doc = Document {
            root: make_element("div", vec![], smallvec::smallvec![
                Node::Element(Box::new(make_element(
                    "p",
                    vec![("class".to_string(), "item".to_string())],
                    smallvec::smallvec![Node::Text(make_text("A"))],
                ))),
                Node::Element(Box::new(make_element(
                    "p",
                    vec![("class".to_string(), "item".to_string())],
                    smallvec::smallvec![Node::Text(make_text("B"))],
                ))),
                Node::Element(Box::new(make_element(
                    "p",
                    vec![("class".to_string(), "item".to_string())],
                    smallvec::smallvec![Node::Text(make_text("A"))],
                ))),
            ]),
            ext: RawDocExt::default(),
        };

        let indexed = Indexer::new().transform(raw_doc);

        // Get the three p elements
        let children = &indexed.root.children;
        assert_eq!(children.len(), 3);

        let p1_id = match &children[0] {
            Node::Element(e) => e.ext.stable_id(),
            _ => panic!("Expected element"),
        };
        let p2_id = match &children[1] {
            Node::Element(e) => e.ext.stable_id(),
            _ => panic!("Expected element"),
        };
        let p3_id = match &children[2] {
            Node::Element(e) => e.ext.stable_id(),
            _ => panic!("Expected element"),
        };

        // All three should have different IDs
        // p1 and p3 have same tag but different occurrence (0 vs 1)
        // because their key attrs (class is NOT a key attr) are the same
        assert_ne!(p1_id, p2_id, "p1 and p2 should have different IDs");
        assert_ne!(p2_id, p3_id, "p2 and p3 should have different IDs");
        assert_ne!(p1_id, p3_id, "p1 and p3 should have different IDs (different occurrence)");
    }

    #[test]
    fn test_occurrence_stable_on_reorder() {
        use crate::vdom::phase::{RawDocExt, RawElemExt};
        use crate::vdom::node::FamilyExt;
        use crate::vdom::attr::Attrs;

        // Helper to create Raw elements with proper ext type
        fn make_element(tag: &str, attrs: Attrs, children: SmallVec<[Node<Raw>; 8]>) -> Element<Raw> {
            Element {
                tag: tag.to_string(),
                attrs,
                children,
                ext: FamilyExt::Other(RawElemExt::detached()),
            }
        }

        // Create two documents: [A, B] and [B, A]
        // The elements should have the SAME IDs after reorder

        // Document 1: [A, B]
        let doc1 = Document {
            root: make_element("div", vec![], smallvec::smallvec![
                Node::Element(Box::new(make_element(
                    "p",
                    vec![("id".to_string(), "a".to_string())], // key attr
                    smallvec::smallvec![],
                ))),
                Node::Element(Box::new(make_element(
                    "p",
                    vec![("id".to_string(), "b".to_string())], // key attr
                    smallvec::smallvec![],
                ))),
            ]),
            ext: RawDocExt::default(),
        };

        // Document 2: [B, A] (reordered)
        let doc2 = Document {
            root: make_element("div", vec![], smallvec::smallvec![
                Node::Element(Box::new(make_element(
                    "p",
                    vec![("id".to_string(), "b".to_string())], // key attr
                    smallvec::smallvec![],
                ))),
                Node::Element(Box::new(make_element(
                    "p",
                    vec![("id".to_string(), "a".to_string())], // key attr
                    smallvec::smallvec![],
                ))),
            ]),
            ext: RawDocExt::default(),
        };

        let indexed1 = Indexer::new().transform(doc1);
        let indexed2 = Indexer::new().transform(doc2);

        // Get IDs from doc1
        let doc1_a_id = match &indexed1.root.children[0] {
            Node::Element(e) => e.ext.stable_id(),
            _ => panic!("Expected element"),
        };
        let doc1_b_id = match &indexed1.root.children[1] {
            Node::Element(e) => e.ext.stable_id(),
            _ => panic!("Expected element"),
        };

        // Get IDs from doc2 (reordered)
        let doc2_b_id = match &indexed2.root.children[0] {
            Node::Element(e) => e.ext.stable_id(),
            _ => panic!("Expected element"),
        };
        let doc2_a_id = match &indexed2.root.children[1] {
            Node::Element(e) => e.ext.stable_id(),
            _ => panic!("Expected element"),
        };

        // Key: IDs should be STABLE across reorder!
        // Because we use occurrence (count of same content key) not position
        // p#a appears once in both docs -> occurrence = 0
        // p#b appears once in both docs -> occurrence = 0
        assert_eq!(
            doc1_a_id, doc2_a_id,
            "Element with id='a' should have same StableId after reorder"
        );
        assert_eq!(
            doc1_b_id, doc2_b_id,
            "Element with id='b' should have same StableId after reorder"
        );
    }
}
