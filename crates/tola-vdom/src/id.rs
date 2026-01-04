//! Content-based stable node identity system for VDOM
//!
//! Implements a pure content hash strategy for stable node identification:
//! - **Elements**: Hash(tag + key_attrs + occurrence_in_siblings)
//! - **Text**: Hash(content + occurrence_in_siblings)
//!
//! # Design Decision
//!
//! We use content-based hashing instead of Typst Span because:
//! - Span values are NOT stable across separate compilations
//! - Content hash is deterministic and reproducible
//! - Same content always produces same ID, enabling reliable diffing
//!
//! # Occurrence Index (not Position!)
//!
//! To handle identical siblings (e.g., multiple `<p>same text</p>`),
//! we use **occurrence index** instead of absolute position:
//! - occurrence = "how many times this same content appeared before in siblings"
//!
//! This enables **Move detection**: when elements reorder, their IDs stay the same
//! because their content and occurrence count haven't changed.
//!
//! Example: `[A, B, C]` → `[C, A, B]`
//! - Position-based: all IDs change → Replace × 3
//! - Occurrence-based: IDs unchanged → Move × 3 (preserves CSS transitions!)

use std::fmt;
use std::hash::Hash;

// =============================================================================
// PageSeed - Page-specific seed for globally unique StableIds
// =============================================================================

/// Page-specific seed for globally unique StableIds
///
/// When building for hot reload, each page gets a unique seed based on its
/// URL path. This ensures StableIds are unique across different pages,
/// allowing the browser to safely ignore patches for elements that don't
/// exist in current DOM.
///
/// # Creation
///
/// ```ignore
/// let seed = PageSeed::from_path("/blog/post.html");
/// let indexed = Indexer::new().with_page_seed(seed).transform(raw_doc);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PageSeed(pub u64);

impl PageSeed {
    /// Create a PageSeed from a page path
    pub fn from_path(path: &str) -> Self {
        use crate::hash::StableHasher;
        Self(StableHasher::new()
            .update_str("__page__")
            .update_str(path)
            .finish())
    }

    /// Create a zero seed (for single-page or test scenarios)
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Get the raw u64 value
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

// =============================================================================
// StableId
// =============================================================================

/// Stable node identifier based on content hash
///
/// Computed from node content and structure, enabling:
/// - Efficient VDOM diffing (O(1) identity check)
/// - Cross-compilation stability (same content = same ID)
/// - Deterministic behavior for hot reload
///
/// # Memory Layout
///
/// - 8 bytes (u64)
/// - Copy, no heap allocation
/// - Null-optimized: `Option<StableId>` is also 8 bytes
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct StableId(pub u64);

impl StableId {
    /// Create a StableId from a raw u64 value
    ///
    /// # Safety Note
    ///
    /// This is primarily for deserialization. Prefer `from_span()` or
    /// `from_content_hash()` for creating new IDs.
    #[inline]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Get the raw u64 representation
    #[inline]
    pub const fn as_raw(&self) -> u64 {
        self.0
    }

    /// Return the StableId serialized for use in `data-tola-id` attributes
    /// and hot-reload messages.
    ///
    /// Returns lowercase hex representation (no leading `#`) for
    /// compactness and CSS attribute selector compatibility.
    #[inline]
    pub fn to_attr_value(self) -> String {
        format!("{:x}", self.as_raw())
    }

    /// Create a StableId for an element node
    ///
    /// Hash is computed from:
    /// - Tag name
    /// - Key attributes only (id, key, data-key-*) for stable identity
    /// - Occurrence index (how many same-tag elements appeared before)
    ///
    /// Note: Regular attributes (class, style) are NOT included in hash.
    /// This means attribute changes generate `UpdateAttrs` not `Replace`,
    /// preserving DOM node identity and CSS transitions.
    ///
    /// # Arguments
    ///
    /// * `tag` - Element tag name
    /// * `attrs` - All attributes (only key attrs will be hashed)
    /// * `_children` - Child StableIds (unused, kept for API compatibility)
    /// * `occurrence` - How many same-(tag, key_attrs) siblings appeared before this one
    ///
    /// # Example
    ///
    /// ```ignore
    /// let id = StableId::for_element("div", &attrs, &child_ids, 0);
    /// ```
    pub fn for_element(
        tag: &str,
        attrs: &[(String, String)],
        _children: &[StableId],
        occurrence: usize,
        parent_seed: u64,
    ) -> Self {
        use crate::hash::StableHasher;

        let mut hasher = StableHasher::new()
            .update_u64(parent_seed)
            .update_str(tag);

        // Hash ONLY key attributes (id, key, data-key-*) for stable identity
        for (k, v) in attrs {
            if k == "id" || k == "key" || k.starts_with("data-key") {
                hasher = hasher.update_str(k).update_str(v);
            }
        }

        // Hash occurrence index (NOT absolute position!)
        Self(hasher.update_usize(occurrence).finish())
    }

    /// Create a StableId for a text node
    ///
    /// Hash is computed from:
    /// - Text content
    /// - Occurrence index (how many same-content text nodes appeared before)
    ///
    /// # Arguments
    ///
    /// * `content` - Text content
    /// * `occurrence` - How many same-content text siblings appeared before this one
    ///
    /// # Example
    ///
    /// ```ignore
    /// let id = StableId::for_text(0);  // First text node at this position
    /// ```
    ///
    /// # Design Note
    ///
    /// Text node IDs are based on occurrence index only, NOT content.
    /// This is critical for correct diffing:
    /// - If content were included: "Hello" → "World" would be Delete + Insert
    /// - With position only: "Hello" → "World" is recognized as Keep + UpdateText
    #[inline]
    pub fn for_text(occurrence: usize, parent_seed: u64) -> Self {
        use crate::hash::StableHasher;

        Self(StableHasher::new()
            .update_u64(parent_seed)
            .update_str("__text__")
            .update_usize(occurrence)
            .finish())
    }

    /// Create a StableId for a frame node (SVG content)
    ///
    /// # Arguments
    ///
    /// * `frame_id` - Unique frame identifier
    /// * `occurrence` - How many same-frame_id siblings appeared before this one
    #[inline]
    pub fn for_frame(frame_id: usize, occurrence: usize, parent_seed: u64) -> Self {
        use crate::hash::StableHasher;

        Self(StableHasher::new()
            .update_u64(parent_seed)
            .update_str("__frame__")
            .update_usize(frame_id)
            .update_usize(occurrence)
            .finish())
    }

    /// Create from content hash (legacy API, kept for compatibility)
    ///
    /// # Deprecated
    /// Use `for_element()` instead which includes position for disambiguation.
    pub fn from_content_hash(tag: &str, attrs: &[(String, String)], children: &[StableId]) -> Self {
        Self::for_element(tag, attrs, children, 0, 0)
    }

    /// Create from text content (legacy API, kept for compatibility)
    ///
    /// # Deprecated
    /// Use `for_text()` instead - text IDs no longer include content.
    #[allow(unused_variables)]
    pub fn from_text_content(content: &str) -> Self {
        Self::for_text(0, 0)
    }

    /// Create a detached/placeholder ID
    ///
    /// Use sparingly - this creates an ID that won't match any real node.
    /// Useful for testing or as a temporary placeholder.
    #[inline]
    pub const fn detached() -> Self {
        Self(0)
    }

    /// Check if this is a detached/placeholder ID
    #[inline]
    pub const fn is_detached(&self) -> bool {
        self.0 == 0
    }
}

impl fmt::Debug for StableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_detached() {
            write!(f, "StableId(detached)")
        } else {
            write!(f, "StableId({:016x})", self.0)
        }
    }
}

impl fmt::Display for StableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:x}", self.0)
    }
}

impl Default for StableId {
    fn default() -> Self {
        Self::detached()
    }
}

// =============================================================================
// rkyv serialization support
// =============================================================================

// rkyv 0.8 requires derive macros for proper trait implementation.
// We use a simple newtype wrapper that derives all necessary traits.
#[cfg(feature = "rkyv")]
mod rkyv_impl {
    use super::StableId;

    // Re-export for use in parent module
    pub use rkyv::{Archive, Deserialize, Serialize};

    /// Wrapper type for rkyv serialization
    #[derive(Archive, Serialize, Deserialize)]
    #[rkyv(compare(PartialEq))]
    pub struct StableIdWrapper(pub u64);

    impl From<StableId> for StableIdWrapper {
        fn from(id: StableId) -> Self {
            StableIdWrapper(id.0)
        }
    }

    impl From<StableIdWrapper> for StableId {
        fn from(wrapper: StableIdWrapper) -> Self {
            StableId(wrapper.0)
        }
    }

    impl From<&ArchivedStableIdWrapper> for StableId {
        fn from(archived: &ArchivedStableIdWrapper) -> Self {
            StableId(archived.0.into())
        }
    }
}

#[cfg(feature = "rkyv")]
pub use rkyv_impl::{ArchivedStableIdWrapper, StableIdWrapper};

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detached_id() {
        let id = StableId::detached();
        assert!(id.is_detached());
        // Note: has_span was removed when switching from Span to content hash
    }

    #[test]
    fn test_content_hash_deterministic() {
        let attrs = vec![("class".to_string(), "foo".to_string())];
        let children = vec![StableId::from_raw(1), StableId::from_raw(2)];

        let id1 = StableId::from_content_hash("div", &attrs, &children);
        let id2 = StableId::from_content_hash("div", &attrs, &children);

        assert_eq!(id1, id2);
    }

    #[test]
    fn test_content_hash_differs() {
        // Use key attributes (id, key) to test hash difference
        // Note: class is NOT a key attribute, so it won't affect hash
        let attrs1 = vec![("id".to_string(), "foo".to_string())];
        let attrs2 = vec![("id".to_string(), "bar".to_string())];
        let children: Vec<StableId> = vec![];

        let id1 = StableId::from_content_hash("div", &attrs1, &children);
        let id2 = StableId::from_content_hash("div", &attrs2, &children);

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_non_key_attrs_dont_affect_hash() {
        // class and style are NOT key attributes
        // Changing them should NOT change the StableId
        let attrs1 = vec![("class".to_string(), "foo".to_string())];
        let attrs2 = vec![("class".to_string(), "bar".to_string())];
        let children: Vec<StableId> = vec![];

        let id1 = StableId::from_content_hash("div", &attrs1, &children);
        let id2 = StableId::from_content_hash("div", &attrs2, &children);

        // Same ID because class is not a key attribute!
        assert_eq!(id1, id2, "class attr should not affect StableId");
    }

    #[test]
    fn test_display_format() {
        let id = StableId::from_raw(0x123456789abcdef0);
        assert_eq!(format!("{}", id), "#123456789abcdef0");
    }
}
