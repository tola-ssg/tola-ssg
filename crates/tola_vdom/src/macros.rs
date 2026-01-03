//! FamilyExt transformation and accessor macros
//!
//! These macros eliminate repetitive match code when working with FamilyExt.
//! All macros use `paste` internally for identifier concatenation.

// =============================================================================
// FamilyExt accessor generation macros
// =============================================================================

/// Generate is_xxx, as_xxx, as_xxx_mut methods for FamilyExt
///
/// # Generated methods per variant:
/// - `is_xxx(&self) -> bool` - type check
/// - `as_xxx(&self) -> Option<&ElemExt>` - immutable accessor
/// - `as_xxx_mut(&mut self) -> Option<&mut ElemExt>` - mutable accessor
#[macro_export]
macro_rules! impl_family_accessors {
    ($($variant:ident),* $(,)?) => {
        ::paste::paste! {
            $(
                #[doc = "Check if this is a " $variant " family extension"]
                pub fn [<is_ $variant:lower>](&self) -> bool {
                    matches!(self, Self::$variant(_))
                }

                #[doc = "Get reference to " $variant " extension data"]
                pub fn [<as_ $variant:lower>](&self) -> Option<&P::ElemExt<[<$variant Family>]>> {
                    match self { Self::$variant(e) => Some(e), _ => None }
                }

                #[doc = "Get mutable reference to " $variant " extension data"]
                pub fn [<as_ $variant:lower _mut>](&mut self) -> Option<&mut P::ElemExt<[<$variant Family>]>> {
                    match self { Self::$variant(e) => Some(e), _ => None }
                }
            )*
        }
    };
}

/// Generate methods that match on all variants and return a value from TagFamily
///
/// # Example
/// ```ignore
/// impl_family_match!(family_name, NAME, &'static str);
/// // Expands to: pub fn family_name(&self) -> &'static str { match ... }
/// ```
#[macro_export]
macro_rules! impl_family_match {
    ($method:ident, $field:ident, $ret:ty, $($variant:ident),* $(,)?) => {
        ::paste::paste! {
            pub fn $method(&self) -> $ret {
                match self {
                    $(Self::$variant(_) => [<$variant Family>]::$field,)*
                }
            }
        }
    };
}

/// Generate method that reads a field from extension data across all variants
///
/// # Generated method
/// `pub fn $method(&self) -> $ret` - Returns the value of `$field` from any family variant
///
/// # Example
/// ```ignore
/// impl_family_field_get!(stable_id, stable_id, StableId, Svg, Link, Heading, Media, Other);
/// // Expands to: pub fn stable_id(&self) -> StableId { match self { ... e.stable_id ... } }
/// ```
#[macro_export]
macro_rules! impl_family_field_get {
    ($method:ident, $field:ident, $ret:ty, $($variant:ident),* $(,)?) => {
        #[doc = concat!("Get `", stringify!($field), "` from any family variant")]
        pub fn $method(&self) -> $ret {
            match self {
                $(Self::$variant(e) => e.$field,)*
            }
        }
    };
}

/// Generate method that sets a field on extension data across all variants
///
/// # Generated method
/// `pub fn $method(&mut self, value: $ty)` - Sets `$field` on any family variant
///
/// # Example
/// ```ignore
/// impl_family_field_set!(set_modified, modified, bool, Svg, Link, Heading, Media, Other);
/// // Expands to: pub fn set_modified(&mut self, value: bool) { match self { ... e.modified = value ... } }
/// ```
#[macro_export]
macro_rules! impl_family_field_set {
    ($method:ident, $field:ident, $ty:ty, $($variant:ident),* $(,)?) => {
        #[doc = concat!("Set `", stringify!($field), "` on any family variant")]
        pub fn $method(&mut self, value: $ty) {
            match self {
                $(Self::$variant(e) => e.$field = value,)*
            }
        }
    };
}

/// Generate is_xxx, as_xxx, as_xxx_mut for enums with typed variants
///
/// Uses paste's `:camel` modifier to convert method name to variant name.
/// # Generated methods per variant:
/// - `is_xxx(&self) -> bool`
/// - `as_xxx(&self) -> Option<&Type<P>>`
/// - `as_xxx_mut(&mut self) -> Option<&mut Type<P>>`
///
/// # Example
/// ```ignore
/// impl<P> Node<P> {
///     // element -> Element, text -> Text, frame -> Frame
///     impl_enum_accessors!(P; element, text, frame);
/// }
/// ```
#[macro_export]
macro_rules! impl_enum_accessors {
    ($phase:ty; $($variant:ident),* $(,)?) => {
        ::paste::paste! {
            $(
                #[doc = "Check if this is a " [<$variant:camel>] " node"]
                pub fn [<is_ $variant>](&self) -> bool {
                    matches!(self, Self::[<$variant:camel>](_))
                }

                #[doc = "Try to get as " $variant " reference"]
                pub fn [<as_ $variant>](&self) -> Option<&[<$variant:camel>]<$phase>> {
                    match self { Self::[<$variant:camel>](v) => Some(v), _ => None }
                }

                #[doc = "Try to get as mutable " $variant " reference"]
                pub fn [<as_ $variant _mut>](&mut self) -> Option<&mut [<$variant:camel>]<$phase>> {
                    match self { Self::[<$variant:camel>](v) => Some(v), _ => None }
                }
            )*
        }
    };
}

/// Map FamilyExt to a new phase while preserving family information
///
/// Note: This macro works when all families use the same ElemExt transformation.
/// For Indexed → Processed cross-phase transformation, use `process_family_ext!` instead.
///
/// # Example
/// ```ignore
/// // Transform ext within the same phase (e.g., updating stable_id)
/// let new_ext: FamilyExt<Indexed> = map_family_ext!(old_ext, |e| IndexedElemExt {
///     stable_id: new_id,
///     family_data: e.family_data.clone(),
/// });
/// ```
#[macro_export]
macro_rules! map_family_ext {
    ($ext:expr, |$e:pat_param| $new_ext:expr) => {
        match $ext {
            $crate::FamilyExt::Svg($e) => $crate::FamilyExt::Svg($new_ext),
            $crate::FamilyExt::Link($e) => $crate::FamilyExt::Link($new_ext),
            $crate::FamilyExt::Heading($e) => $crate::FamilyExt::Heading($new_ext),
            $crate::FamilyExt::Media($e) => $crate::FamilyExt::Media($new_ext),
            $crate::FamilyExt::Other($e) => $crate::FamilyExt::Other($new_ext),
        }
    };
}

/// Transform FamilyExt from Indexed → Processed phase
///
/// Automatically calls `TagFamily::process()` for each family's data.
///
/// # Example
/// ```ignore
/// let indexed_ext: FamilyExt<Indexed> = elem.ext;
/// let processed_ext: FamilyExt<Processed> = process_family_ext!(indexed_ext);
/// ```
#[macro_export]
macro_rules! process_family_ext {
    ($ext:expr) => {
        match $ext {
            $crate::FamilyExt::Svg(indexed) => {
                $crate::FamilyExt::Svg($crate::ProcessedElemExt {
                    stable_id: indexed.stable_id,
                    modified: false,
                    family_data: <$crate::SvgFamily as $crate::TagFamily>::process(
                        &indexed.family_data,
                    ),
                })
            }
            $crate::FamilyExt::Link(indexed) => {
                $crate::FamilyExt::Link($crate::ProcessedElemExt {
                    stable_id: indexed.stable_id,
                    modified: false,
                    family_data: <$crate::LinkFamily as $crate::TagFamily>::process(
                        &indexed.family_data,
                    ),
                })
            }
            $crate::FamilyExt::Heading(indexed) => {
                $crate::FamilyExt::Heading($crate::ProcessedElemExt {
                    stable_id: indexed.stable_id,
                    modified: false,
                    family_data: <$crate::HeadingFamily as $crate::TagFamily>::process(
                        &indexed.family_data,
                    ),
                })
            }
            $crate::FamilyExt::Media(indexed) => {
                $crate::FamilyExt::Media($crate::ProcessedElemExt {
                    stable_id: indexed.stable_id,
                    modified: false,
                    family_data: <$crate::MediaFamily as $crate::TagFamily>::process(
                        &indexed.family_data,
                    ),
                })
            }
            $crate::FamilyExt::Other(indexed) => {
                $crate::FamilyExt::Other($crate::ProcessedElemExt {
                    stable_id: indexed.stable_id,
                    modified: false,
                    family_data: <$crate::OtherFamily as $crate::TagFamily>::process(
                        &indexed.family_data,
                    ),
                })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::id::StableId;
    use crate::phase::{Indexed, IndexedElemExt, Processed};
    use crate::node::FamilyExt;
    use crate::family::{LinkIndexedData, LinkType};

    #[test]
    fn test_map_family_ext() {
        let link_ext: FamilyExt<Indexed> = FamilyExt::Link(IndexedElemExt {
            stable_id: StableId::from_raw(1001),
            family_data: LinkIndexedData {
                link_type: LinkType::External,
                original_href: Some("https://example.com".into()),
            },
        });

        // Map to update stable_id
        let new_stable_id = StableId::from_raw(2001);
        let updated: FamilyExt<Indexed> = map_family_ext!(link_ext, |e| IndexedElemExt {
            stable_id: new_stable_id,
            family_data: e.family_data.clone(),
        });

        assert!(updated.is_link());
        if let FamilyExt::Link(ext) = updated {
            assert_eq!(ext.stable_id, new_stable_id);
        }
    }

    #[test]
    fn test_process_family_ext() {
        let indexed_ext: FamilyExt<Indexed> = FamilyExt::Link(IndexedElemExt {
            stable_id: StableId::from_raw(42),
            family_data: LinkIndexedData {
                link_type: LinkType::External,
                original_href: Some("https://example.com".into()),
            },
        });

        let processed_ext: FamilyExt<Processed> = process_family_ext!(indexed_ext);

        assert!(processed_ext.is_link());
        if let FamilyExt::Link(ext) = processed_ext {
            assert!(!ext.modified);
            assert!(ext.family_data.is_external);
            assert_eq!(ext.family_data.resolved_url, Some("https://example.com".into()));
        }
    }
}
