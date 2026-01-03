//! Attribute system for VDOM elements.
//!
//! Provides a simple attribute type and extension trait for common operations.
//!
//! # Design
//!
//! Uses `Vec<(String, String)>` directly instead of complex wrapper types.
//! This is simple and sufficient for HTML attribute handling while supporting
//! all standard operations.

/// Element attributes as simple key-value pairs.
///
/// This is the primary attribute type used throughout the VDOM.
/// Each attribute is a (name, value) tuple stored in insertion order.
pub type Attrs = Vec<(String, String)>;

/// Extension trait for attribute operations on [`Attrs`].
///
/// # Example
///
/// ```
/// use tola_vdom::attr::{Attrs, AttrsExt};
///
/// let mut attrs: Attrs = Vec::new();
/// attrs.set_attr("id", "main");
/// attrs.set_attr("class", "container");
///
/// assert_eq!(attrs.get_attr("id"), Some("main"));
/// assert!(attrs.has_attr("class"));
/// ```
pub trait AttrsExt {
    /// Get an attribute value by name.
    fn get_attr(&self, name: &str) -> Option<&str>;

    /// Check if an attribute exists.
    fn has_attr(&self, name: &str) -> bool;

    /// Set an attribute value (insert or update).
    fn set_attr(&mut self, name: impl Into<String>, value: impl Into<String>);

    /// Remove an attribute by name, returning the old value if present.
    fn remove_attr(&mut self, name: &str) -> Option<String>;

    /// Get id attribute if present.
    fn id(&self) -> Option<&str>;

    /// Get class attribute if present.
    fn class(&self) -> Option<&str>;

    /// Check if element has a specific class.
    fn has_class(&self, class_name: &str) -> bool;
}

impl AttrsExt for Attrs {
    fn get_attr(&self, name: &str) -> Option<&str> {
        self.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn has_attr(&self, name: &str) -> bool {
        self.iter().any(|(k, _)| k == name)
    }

    fn set_attr(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        if let Some(attr) = self.iter_mut().find(|(k, _)| k == &name) {
            attr.1 = value;
        } else {
            self.push((name, value));
        }
    }

    fn remove_attr(&mut self, name: &str) -> Option<String> {
        self.iter()
            .position(|(k, _)| k == name)
            .map(|pos| self.remove(pos).1)
    }

    fn id(&self) -> Option<&str> {
        self.get_attr("id")
    }

    fn class(&self) -> Option<&str> {
        self.get_attr("class")
    }

    fn has_class(&self, class_name: &str) -> bool {
        self.class()
            .map(|c| c.split_whitespace().any(|cls| cls == class_name))
            .unwrap_or(false)
    }
}

/// Extension trait for building attributes fluently.
pub trait AttrsBuilder {
    /// Add an attribute and return self for chaining.
    fn with_attr(self, name: impl Into<String>, value: impl Into<String>) -> Self;

    /// Add id attribute.
    fn with_id(self, id: impl Into<String>) -> Self;

    /// Add class attribute.
    fn with_class(self, class: impl Into<String>) -> Self;
}

impl AttrsBuilder for Attrs {
    fn with_attr(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.set_attr(name, value);
        self
    }

    fn with_id(self, id: impl Into<String>) -> Self {
        self.with_attr("id", id)
    }

    fn with_class(self, class: impl Into<String>) -> Self {
        self.with_attr("class", class)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attrs_operations() {
        let mut attrs: Attrs = Vec::new();

        // Set
        attrs.set_attr("id", "main");
        attrs.set_attr("class", "container");
        assert_eq!(attrs.len(), 2);

        // Get
        assert_eq!(attrs.get_attr("id"), Some("main"));
        assert_eq!(attrs.get_attr("class"), Some("container"));
        assert_eq!(attrs.get_attr("href"), None);

        // Has
        assert!(attrs.has_attr("id"));
        assert!(!attrs.has_attr("href"));

        // Update existing
        attrs.set_attr("class", "wrapper");
        assert_eq!(attrs.get_attr("class"), Some("wrapper"));
        assert_eq!(attrs.len(), 2);

        // Remove
        let removed = attrs.remove_attr("id");
        assert_eq!(removed.as_deref(), Some("main"));
        assert!(!attrs.has_attr("id"));
        assert_eq!(attrs.len(), 1);
    }

    #[test]
    fn test_convenience_methods() {
        let attrs: Attrs = vec![
            ("id".into(), "main".into()),
            ("class".into(), "foo bar baz".into()),
        ];

        assert_eq!(attrs.id(), Some("main"));
        assert_eq!(attrs.class(), Some("foo bar baz"));
        assert!(attrs.has_class("foo"));
        assert!(attrs.has_class("bar"));
        assert!(!attrs.has_class("qux"));
    }

    #[test]
    fn test_builder() {
        let attrs = Attrs::new()
            .with_id("main")
            .with_class("container")
            .with_attr("data-value", "42");

        assert_eq!(attrs.len(), 3);
        assert_eq!(attrs.id(), Some("main"));
        assert_eq!(attrs.class(), Some("container"));
        assert_eq!(attrs.get_attr("data-value"), Some("42"));
    }
}
