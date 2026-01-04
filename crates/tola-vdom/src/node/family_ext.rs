//! FamilyExt enum and HasFamilyData trait
//!
//! Zero-cost family extension mechanism for type-safe phase transformations.

use crate::family::{
    HeadingFamily, HeadingIndexedData, HeadingProcessedData, LinkFamily, LinkIndexedData,
    LinkProcessedData, MediaFamily, MediaIndexedData, MediaProcessedData, OtherFamily, SvgFamily,
    SvgIndexedData, SvgProcessedData, TagFamily,
};
use crate::phase::{Indexed, PhaseData, Processed, Raw};

use super::Element;

// =============================================================================
// FamilyExt - Zero-cost family extension enum
// =============================================================================

/// Family extension enum - compile-time determined, zero runtime overhead
///
/// Key design: Use enum instead of Box<dyn Any>
/// - Stack allocated (no heap overhead)
/// - Size known at compile time
/// - Pattern matching (no downcast overhead)
#[derive(Debug, Clone)]
pub enum FamilyExt<P: PhaseData> {
    Svg(P::ElemExt<SvgFamily>),
    Link(P::ElemExt<LinkFamily>),
    Heading(P::ElemExt<HeadingFamily>),
    Media(P::ElemExt<MediaFamily>),
    Other(P::ElemExt<OtherFamily>),
}

impl<P: PhaseData> FamilyExt<P> {
    // Generates: family_name() -> &'static str (returns TagFamily::NAME)
    impl_family_match!(family_name, NAME, &'static str, Svg, Link, Heading, Media, Other);

    /// Get the FamilyKind for this extension
    pub fn kind(&self) -> crate::family::FamilyKind {
        use crate::family::FamilyKind;
        match self {
            Self::Svg(_) => FamilyKind::Svg,
            Self::Link(_) => FamilyKind::Link,
            Self::Heading(_) => FamilyKind::Heading,
            Self::Media(_) => FamilyKind::Media,
            Self::Other(_) => FamilyKind::Other,
        }
    }

    // Generates for each variant (Svg, Link, Heading, Media, Other):
    //   - is_xxx(&self) -> bool
    //   - as_xxx(&self) -> Option<&ElemExt<XxxFamily>>
    //   - as_xxx_mut(&mut self) -> Option<&mut ElemExt<XxxFamily>>
    impl_family_accessors!(Svg, Link, Heading, Media, Other);
}

// NOTE: FamilyExt intentionally does NOT implement Default.
// Rationale: Silently defaulting to `Other` family hides errors.
// Users must explicitly specify the family when creating elements.
// Use Element::svg(), Element::link(), etc. or Element::auto() instead.

// =============================================================================
// FamilyExt phase-specific implementations
// =============================================================================

/// Raw phase: access and set Span for StableId generation
impl FamilyExt<Raw> {
    /// Get the Span from any family variant
    pub fn span(&self) -> Option<crate::span::SourceSpan> {
        match self {
            Self::Svg(ext) => ext.span,
            Self::Link(ext) => ext.span,
            Self::Heading(ext) => ext.span,
            Self::Media(ext) => ext.span,
            Self::Other(ext) => ext.span,
        }
    }

    /// Set the Span on any family variant
    pub fn set_span(&mut self, span: crate::span::SourceSpan) {
        match self {
            Self::Svg(ext) => ext.span = Some(span),
            Self::Link(ext) => ext.span = Some(span),
            Self::Heading(ext) => ext.span = Some(span),
            Self::Media(ext) => ext.span = Some(span),
            Self::Other(ext) => ext.span = Some(span),
        }
    }

    /// Check if this element has a valid (non-detached) Span
    pub fn has_span(&self) -> bool {
        self.span().map(|s| !s.is_detached()).unwrap_or(false)
    }
}

/// Indexed phase: access common fields across all families
impl FamilyExt<Indexed> {
    /// Get the StableId from any family variant
    pub fn stable_id(&self) -> crate::id::StableId {
        match self {
            Self::Svg(ext) => ext.stable_id,
            Self::Link(ext) => ext.stable_id,
            Self::Heading(ext) => ext.stable_id,
            Self::Media(ext) => ext.stable_id,
            Self::Other(ext) => ext.stable_id,
        }
    }
}

/// Processed phase: access common fields across all families
impl FamilyExt<Processed> {
    // Generates: is_modified(&self) -> bool (reads e.modified from each variant)
    impl_family_field_get!(is_modified, modified, bool, Svg, Link, Heading, Media, Other);

    // Generates: set_modified(&mut self, value: bool) (sets e.modified on each variant)
    impl_family_field_set!(set_modified, modified, bool, Svg, Link, Heading, Media, Other);

    /// Get the StableId from any family variant (preserved from Indexed phase)
    pub fn stable_id(&self) -> crate::id::StableId {
        match self {
            Self::Svg(ext) => ext.stable_id,
            Self::Link(ext) => ext.stable_id,
            Self::Heading(ext) => ext.stable_id,
            Self::Media(ext) => ext.stable_id,
            Self::Other(ext) => ext.stable_id,
        }
    }
}

// =============================================================================
// HasFamilyData trait - unified family data access
// =============================================================================

/// Unified family data access trait
///
/// Allows accessing family-specific data without manual match branches.
///
/// # Example
/// ```ignore
/// use tola::vdom::{Element, Indexed, LinkFamily, HasFamilyData};
///
/// fn process_link(elem: &Element<Indexed>) {
///     if let Some(link_data) = elem.family_data::<LinkFamily>() {
///         println!("href: {:?}", link_data.original_href);
///     }
/// }
/// ```
pub trait HasFamilyData<F: TagFamily> {
    /// The concrete data type for this (Phase, Family) combination
    type Data;

    /// Get immutable reference to family data if this element belongs to family F
    fn family_data(&self) -> Option<&Self::Data>;

    /// Get mutable reference to family data if this element belongs to family F
    fn family_data_mut(&mut self) -> Option<&mut Self::Data>;
}

// Macro to generate HasFamilyData implementations
// Uses paste to auto-generate method names (Svg -> as_svg, as_svg_mut)
// and data types (Indexed + Svg -> SvgIndexedData, Processed + Svg -> SvgProcessedData)
macro_rules! impl_has_family_data {
    // With explicit data type (for OtherFamily which uses () instead of OtherXxxData)
    ($phase:ident, $family:ident, $data_type:ty) => {
        ::paste::paste! {
            impl HasFamilyData<[<$family Family>]> for Element<$phase> {
                type Data = $data_type;

                fn family_data(&self) -> Option<&Self::Data> {
                    self.ext.[<as_ $family:lower>]().map(|e| &e.family_data)
                }

                fn family_data_mut(&mut self) -> Option<&mut Self::Data> {
                    self.ext.[<as_ $family:lower _mut>]().map(|e| &mut e.family_data)
                }
            }
        }
    };
    // Auto-generate data type (XxxFamily + Phase -> XxxPhaseData)
    ($phase:ident, $family:ident) => {
        ::paste::paste! {
            impl_has_family_data!($phase, $family, [<$family $phase Data>]);
        }
    };
}

// Generates for each (Phase, Family):
//   impl HasFamilyData<XxxFamily> for Element<Phase> {
//     type Data = XxxPhaseData;  // or () for Other
//     fn family_data(&self) -> Option<&Self::Data>
//     fn family_data_mut(&mut self) -> Option<&mut Self::Data>
//   }

// Indexed phase implementations
impl_has_family_data!(Indexed, Svg);         // -> SvgIndexedData
impl_has_family_data!(Indexed, Link);        // -> LinkIndexedData
impl_has_family_data!(Indexed, Heading);     // -> HeadingIndexedData
impl_has_family_data!(Indexed, Media);       // -> MediaIndexedData
impl_has_family_data!(Indexed, Other, ());   // -> Other uses ()

// Processed phase implementations
impl_has_family_data!(Processed, Svg);       // -> SvgProcessedData
impl_has_family_data!(Processed, Link);      // -> LinkProcessedData
impl_has_family_data!(Processed, Heading);   // -> HeadingProcessedData
impl_has_family_data!(Processed, Media);     // -> MediaProcessedData
impl_has_family_data!(Processed, Other, ()); // -> Other uses ()
