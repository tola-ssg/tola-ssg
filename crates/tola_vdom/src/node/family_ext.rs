//! FamilyExt enum - Zero-cost family extension mechanism.
//!
//! This is the core of the TTG pattern: each element carries family-specific
//! extension data through a compile-time determined enum.

use crate::family::{
    FamilyKind, HeadingFamily, LinkFamily, MediaFamily, OtherFamily, SvgFamily, TagFamily,
};
use crate::phase::PhaseData;

// =============================================================================
// FamilyExt - Zero-cost family extension enum
// =============================================================================

/// Family extension enum - compile-time determined, zero runtime overhead.
///
/// Key design: Use enum instead of `Box<dyn Any>`
/// - Stack allocated (no heap overhead)
/// - Size known at compile time
/// - Pattern matching (no downcast overhead)
#[derive(Debug, Clone)]
pub enum FamilyExt<P: PhaseData> {
    /// SVG family extension.
    Svg(P::ElemExt<SvgFamily>),
    /// Link family extension.
    Link(P::ElemExt<LinkFamily>),
    /// Heading family extension.
    Heading(P::ElemExt<HeadingFamily>),
    /// Media family extension.
    Media(P::ElemExt<MediaFamily>),
    /// Other family extension.
    Other(P::ElemExt<OtherFamily>),
}

impl<P: PhaseData> FamilyExt<P> {
    /// Get the family name.
    pub fn family_name(&self) -> &'static str {
        match self {
            Self::Svg(_) => SvgFamily::NAME,
            Self::Link(_) => LinkFamily::NAME,
            Self::Heading(_) => HeadingFamily::NAME,
            Self::Media(_) => MediaFamily::NAME,
            Self::Other(_) => OtherFamily::NAME,
        }
    }

    /// Get the FamilyKind for this extension.
    pub fn kind(&self) -> FamilyKind {
        match self {
            Self::Svg(_) => FamilyKind::Svg,
            Self::Link(_) => FamilyKind::Link,
            Self::Heading(_) => FamilyKind::Heading,
            Self::Media(_) => FamilyKind::Media,
            Self::Other(_) => FamilyKind::Other,
        }
    }

    // =========================================================================
    // Type checking methods
    // =========================================================================

    /// Check if this is an SVG family.
    pub fn is_svg(&self) -> bool {
        matches!(self, Self::Svg(_))
    }

    /// Check if this is a Link family.
    pub fn is_link(&self) -> bool {
        matches!(self, Self::Link(_))
    }

    /// Check if this is a Heading family.
    pub fn is_heading(&self) -> bool {
        matches!(self, Self::Heading(_))
    }

    /// Check if this is a Media family.
    pub fn is_media(&self) -> bool {
        matches!(self, Self::Media(_))
    }

    /// Check if this is an Other family.
    pub fn is_other(&self) -> bool {
        matches!(self, Self::Other(_))
    }

    // =========================================================================
    // Accessor methods
    // =========================================================================

    /// Get SVG extension data if this is an SVG family.
    pub fn as_svg(&self) -> Option<&P::ElemExt<SvgFamily>> {
        match self {
            Self::Svg(ext) => Some(ext),
            _ => None,
        }
    }

    /// Get mutable SVG extension data.
    pub fn as_svg_mut(&mut self) -> Option<&mut P::ElemExt<SvgFamily>> {
        match self {
            Self::Svg(ext) => Some(ext),
            _ => None,
        }
    }

    /// Get Link extension data if this is a Link family.
    pub fn as_link(&self) -> Option<&P::ElemExt<LinkFamily>> {
        match self {
            Self::Link(ext) => Some(ext),
            _ => None,
        }
    }

    /// Get mutable Link extension data.
    pub fn as_link_mut(&mut self) -> Option<&mut P::ElemExt<LinkFamily>> {
        match self {
            Self::Link(ext) => Some(ext),
            _ => None,
        }
    }

    /// Get Heading extension data if this is a Heading family.
    pub fn as_heading(&self) -> Option<&P::ElemExt<HeadingFamily>> {
        match self {
            Self::Heading(ext) => Some(ext),
            _ => None,
        }
    }

    /// Get mutable Heading extension data.
    pub fn as_heading_mut(&mut self) -> Option<&mut P::ElemExt<HeadingFamily>> {
        match self {
            Self::Heading(ext) => Some(ext),
            _ => None,
        }
    }

    /// Get Media extension data if this is a Media family.
    pub fn as_media(&self) -> Option<&P::ElemExt<MediaFamily>> {
        match self {
            Self::Media(ext) => Some(ext),
            _ => None,
        }
    }

    /// Get mutable Media extension data.
    pub fn as_media_mut(&mut self) -> Option<&mut P::ElemExt<MediaFamily>> {
        match self {
            Self::Media(ext) => Some(ext),
            _ => None,
        }
    }

    /// Get Other extension data if this is an Other family.
    pub fn as_other(&self) -> Option<&P::ElemExt<OtherFamily>> {
        match self {
            Self::Other(ext) => Some(ext),
            _ => None,
        }
    }

    /// Get mutable Other extension data.
    pub fn as_other_mut(&mut self) -> Option<&mut P::ElemExt<OtherFamily>> {
        match self {
            Self::Other(ext) => Some(ext),
            _ => None,
        }
    }
}

// NOTE: FamilyExt intentionally does NOT implement Default.
// Rationale: Silently defaulting to `Other` family hides errors.
// Users must explicitly specify the family when creating elements.

/// Trait for types that have family data.
pub trait HasFamilyData<P: PhaseData> {
    /// Get family extension reference.
    fn family_ext(&self) -> &FamilyExt<P>;

    /// Get mutable family extension reference.
    fn family_ext_mut(&mut self) -> &mut FamilyExt<P>;

    /// Get the family kind.
    fn family_kind(&self) -> FamilyKind {
        self.family_ext().kind()
    }
}
