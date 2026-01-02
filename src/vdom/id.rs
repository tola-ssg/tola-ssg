//! Content-based stable node identity system for VDOM
//!
//! Implements a pure content hash strategy for stable node identification:
//! - **Elements**: Hash(tag + sorted_attrs + child_ids + position_in_parent)
//! - **Text**: Hash(content + position_in_parent)
//!
//! # Design Decision
//!
//! We use content-based hashing instead of Typst Span because:
//! - Span values are NOT stable across separate compilations
//! - Content hash is deterministic and reproducible
//! - Same content always produces same ID, enabling reliable diffing
//!
//! # Position Disambiguation
//!
//! To handle identical siblings (e.g., multiple `<p>same text</p>`),
//! we include position-in-parent in the hash. This ensures each node
//! has a unique ID even with identical content.

use std::fmt;
use std::hash::{Hash, Hasher};

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
pub struct StableId(u64);

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
    pub fn to_attr_value(&self) -> String {
        format!("{:x}", self.as_raw())
    }

    /// Create a StableId for an element node
    ///
    /// Hash is computed from:
    /// - Tag name
    /// - Sorted attributes (for order-independence)
    /// - Child StableIds (structural identity)
    /// - Position in parent (disambiguate identical siblings)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let id = StableId::for_element("div", &attrs, &child_ids, 0);
    /// ```
    pub fn for_element(
        tag: &str,
        attrs: &[(String, String)],
        children: &[StableId],
        position: usize,
    ) -> Self {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();

        // Hash tag
        tag.hash(&mut hasher);

        // Hash attributes (sort by key for order-independence)
        let mut sorted_attrs: Vec<_> = attrs.iter().collect();
        sorted_attrs.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in sorted_attrs {
            k.hash(&mut hasher);
            v.hash(&mut hasher);
        }

        // Hash child IDs (order matters for children)
        for child in children {
            child.0.hash(&mut hasher);
        }

        // Hash position to disambiguate identical siblings
        position.hash(&mut hasher);

        Self(hasher.finish())
    }

    /// Create a StableId for a text node
    ///
    /// Hash is computed from:
    /// - Text content
    /// - Position in parent (disambiguate identical text nodes)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let id = StableId::for_text("Hello, world!", 0);
    /// ```
    #[inline]
    pub fn for_text(content: &str, position: usize) -> Self {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        "__text__".hash(&mut hasher);
        content.hash(&mut hasher);
        position.hash(&mut hasher);
        Self(hasher.finish())
    }

    /// Create a StableId for a frame node (SVG content)
    #[inline]
    pub fn for_frame(frame_id: usize, position: usize) -> Self {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        "__frame__".hash(&mut hasher);
        frame_id.hash(&mut hasher);
        position.hash(&mut hasher);
        Self(hasher.finish())
    }

    /// Create from content hash (legacy API, kept for compatibility)
    ///
    /// # Deprecated
    /// Use `for_element()` instead which includes position for disambiguation.
    pub fn from_content_hash(tag: &str, attrs: &[(String, String)], children: &[StableId]) -> Self {
        Self::for_element(tag, attrs, children, 0)
    }

    /// Create from text content (legacy API, kept for compatibility)
    ///
    /// # Deprecated
    /// Use `for_text()` instead which includes position for disambiguation.
    pub fn from_text_content(content: &str) -> Self {
        Self::for_text(content, 0)
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

#[cfg(feature = "rkyv")]
mod rkyv_impl {
    use super::StableId;
    use rkyv::{Archive, Deserialize, Serialize};

    impl Archive for StableId {
        type Archived = ArchivedStableId;
        type Resolver = ();

        unsafe fn resolve(&self, _pos: usize, _resolver: Self::Resolver, out: *mut Self::Archived) {
            out.write(ArchivedStableId(self.0.to_le()));
        }
    }

    impl<S: rkyv::ser::Serializer + ?Sized> Serialize<S> for StableId {
        fn serialize(&self, _serializer: &mut S) -> Result<Self::Resolver, S::Error> {
            Ok(())
        }
    }

    impl<D: rkyv::Fallible + ?Sized> Deserialize<StableId, D> for ArchivedStableId {
        fn deserialize(&self, _deserializer: &mut D) -> Result<StableId, D::Error> {
            Ok(StableId(u64::from_le(self.0)))
        }
    }

    /// Archived form of StableId (little-endian for cross-platform)
    #[repr(transparent)]
    pub struct ArchivedStableId(u64);

    impl ArchivedStableId {
        /// Get the raw value (in native endianness)
        #[inline]
        pub fn as_raw(&self) -> u64 {
            u64::from_le(self.0)
        }
    }
}

#[cfg(feature = "rkyv")]
pub use rkyv_impl::ArchivedStableId;

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
        assert!(!id.has_span());
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
        let attrs1 = vec![("class".to_string(), "foo".to_string())];
        let attrs2 = vec![("class".to_string(), "bar".to_string())];
        let children: Vec<StableId> = vec![];

        let id1 = StableId::from_content_hash("div", &attrs1, &children);
        let id2 = StableId::from_content_hash("div", &attrs2, &children);

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_display_format() {
        let id = StableId::from_raw(0x123456789abcdef0);
        assert_eq!(format!("{}", id), "#123456789abcdef0");
    }
}
