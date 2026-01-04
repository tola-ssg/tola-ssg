//! Attribute system for VDOM elements
//!
//! Simplified design following ttg_demo.rs v8:
//! - Direct `Vec<(String, String)>` for attributes
//! - No wrapper types (AttrName/AttrValue removed)

/// Element attributes as simple key-value pairs
///
/// Following ttg_demo.rs v8 design: use Vec<(String, String)> directly
/// instead of complex wrapper types. This is simpler and sufficient
/// for HTML attribute handling.
pub type Attrs = Vec<(String, String)>;

/// Extension trait for attribute operations on Attrs
pub trait AttrsExt {
    /// Get an attribute value by name
    fn get_attr(&self, name: &str) -> Option<&str>;

    /// Check if an attribute exists
    fn has_attr(&self, name: &str) -> bool;

    /// Set an attribute value (insert or update)
    fn set_attr(&mut self, name: impl Into<String>, value: impl Into<String>);

    /// Remove an attribute by name, returning the old value if present
    fn remove_attr(&mut self, name: &str) -> Option<String>;
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
}

// =============================================================================
// Tests
// =============================================================================

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
}
