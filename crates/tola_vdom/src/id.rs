//! Content-based stable node identity system for VDOM
//!
//! Implements a pure content hash strategy for stable node identification:
//! - **Elements**: Hash(tag + key_attrs + occurrence_in_siblings)
//! - **Text**: Hash(content + occurrence_in_siblings)
//!
//! # Design Decision
//!
//! We use content-based hashing instead of source spans because:
//! - Source spans are NOT stable across separate compilations
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

use tola_core::hash::StableHasher;

// =============================================================================
// PageSeed - Page-specific seed for globally unique StableIds
// =============================================================================

/// Page-specific seed for globally unique StableIds.
///
/// When building for hot reload, each page gets a unique seed based on its
/// URL path. This ensures StableIds are unique across different pages,
/// allowing the browser to safely ignore patches for elements that don't
/// exist in current DOM.
///
/// # Creation
///
/// ```
/// use tola_vdom::id::PageSeed;
///
/// let seed = PageSeed::from_path("/blog/post.html");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PageSeed(pub u64);

impl PageSeed {
    /// Create a PageSeed from a page path.
    pub fn from_path(path: &str) -> Self {
        Self(
            StableHasher::new()
                .update_str("__page__")
                .update_str(path)
                .finish(),
        )
    }

    /// Create a zero seed (for single-page or test scenarios).
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Get the raw u64 value.
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

// =============================================================================
// StableId
// =============================================================================

/// Stable node identifier based on content hash.
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
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StableId(pub u64);

impl StableId {
    /// Zero StableId constant for default initialization.
    pub const ZERO: Self = Self(0);

    /// Create a StableId from a raw u64 value.
    ///
    /// This is primarily for deserialization. Prefer `for_element()` or
    /// `for_text()` for creating new IDs.
    #[inline]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Get the raw u64 representation.
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

    /// Create a StableId for an element node.
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
    /// * `parent_seed` - Seed from parent element for global uniqueness
    pub fn for_element(
        tag: &str,
        attrs: &[(String, String)],
        _children: &[StableId],
        occurrence: usize,
        parent_seed: u64,
    ) -> Self {
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

    /// Create a StableId for a text node.
    ///
    /// Hash is computed from:
    /// - Text content marker
    /// - Occurrence index (how many same-content text nodes appeared before)
    ///
    /// # Arguments
    ///
    /// * `occurrence` - How many same-content text siblings appeared before this one
    /// * `parent_seed` - Seed from parent element for global uniqueness
    ///
    /// # Design Note
    ///
    /// Text node IDs are based on occurrence index only, NOT content.
    /// This is critical for correct diffing:
    /// - If content were included: "Hello" → "World" would be Delete + Insert
    /// - With position only: "Hello" → "World" is recognized as Keep + UpdateText
    #[inline]
    pub fn for_text(occurrence: usize, parent_seed: u64) -> Self {
        Self(
            StableHasher::new()
                .update_u64(parent_seed)
                .update_str("__text__")
                .update_usize(occurrence)
                .finish(),
        )
    }

    /// Create a StableId for a frame node (SVG content).
    ///
    /// # Arguments
    ///
    /// * `frame_id` - Unique frame identifier
    /// * `occurrence` - How many same-frame_id siblings appeared before this one
    /// * `parent_seed` - Seed from parent element for global uniqueness
    #[inline]
    pub fn for_frame(frame_id: usize, occurrence: usize, parent_seed: u64) -> Self {
        Self(
            StableHasher::new()
                .update_u64(parent_seed)
                .update_str("__frame__")
                .update_usize(frame_id)
                .update_usize(occurrence)
                .finish(),
        )
    }

    /// Create from content hash (legacy API, kept for compatibility).
    ///
    /// # Deprecated
    /// Use `for_element()` instead which includes position for disambiguation.
    #[deprecated(since = "0.1.0", note = "Use for_element() instead")]
    pub fn from_content_hash(tag: &str, attrs: &[(String, String)], children: &[StableId]) -> Self {
        Self::for_element(tag, attrs, children, 0, 0)
    }

    /// Create a detached/placeholder ID.
    ///
    /// Use sparingly - this creates an ID that won't match any real node.
    /// Useful for testing or as a temporary placeholder.
    #[inline]
    pub const fn detached() -> Self {
        Self(0)
    }

    /// Check if this is a detached/placeholder ID.
    #[inline]
    pub const fn is_detached(&self) -> bool {
        self.0 == 0
    }
}

// =============================================================================
// Display / Debug implementations
// =============================================================================

impl fmt::Debug for StableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StableId({:016x})", self.0)
    }
}

impl fmt::Display for StableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}", self.0)
    }
}

// =============================================================================
// Serde support (optional)
// =============================================================================

#[cfg(feature = "serde")]
mod serde_impl {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    impl Serialize for StableId {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(&self.to_attr_value())
        }
    }

    impl<'de> Deserialize<'de> for StableId {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let s = String::deserialize(deserializer)?;
            u64::from_str_radix(&s, 16)
                .map(StableId)
                .map_err(serde::de::Error::custom)
        }
    }

    impl Serialize for PageSeed {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.0.serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for PageSeed {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            u64::deserialize(deserializer).map(PageSeed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stable_id_for_element() {
        let attrs = vec![("id".to_string(), "main".to_string())];
        let id1 = StableId::for_element("div", &attrs, &[], 0, 0);
        let id2 = StableId::for_element("div", &attrs, &[], 0, 0);
        let id3 = StableId::for_element("div", &attrs, &[], 1, 0); // Different occurrence

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_stable_id_for_text() {
        let id1 = StableId::for_text(0, 0);
        let id2 = StableId::for_text(0, 0);
        let id3 = StableId::for_text(1, 0);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_page_seed() {
        let seed1 = PageSeed::from_path("/blog/post.html");
        let seed2 = PageSeed::from_path("/blog/post.html");
        let seed3 = PageSeed::from_path("/about.html");

        assert_eq!(seed1, seed2);
        assert_ne!(seed1, seed3);
    }

    #[test]
    fn test_attr_value_format() {
        let id = StableId::from_raw(0x123456789abcdef0);
        assert_eq!(id.to_attr_value(), "123456789abcdef0");
    }
}
