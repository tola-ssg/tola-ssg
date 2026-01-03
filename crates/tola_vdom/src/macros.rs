//! Helper macros for VDOM construction.

/// Create an element with tag and optional attributes/children.
///
/// # Examples
///
/// ```ignore
/// // Simple element
/// let div = elem!("div");
///
/// // With attributes
/// let div = elem!("div", { "class" => "container" });
///
/// // With children
/// let div = elem!("div", [child1, child2]);
/// ```
#[macro_export]
macro_rules! elem {
    ($tag:expr) => {
        $crate::node::Element::new($tag)
    };
    ($tag:expr, { $($key:expr => $val:expr),* $(,)? }) => {{
        let mut e = $crate::node::Element::new($tag);
        $(
            e.attrs.push(($key.into(), $val.into()));
        )*
        e
    }};
}

/// Create a text node.
#[macro_export]
macro_rules! text {
    ($content:expr) => {
        $crate::node::Text::new($content)
    };
}
