//! # Capability System
//!
//! Compile-time pipeline dependency checking with zero runtime overhead.
//!
//! ## Complete Example
//!
//! ```ignore
//! use tola_vdom::capability::*;
//! use tola_vdom::caps;
//!
//! // ═══════════════════════════════════════════════════════════════════════════
//! // Define capabilities (built-in ones already provided)
//! // ═══════════════════════════════════════════════════════════════════════════
//!
//! struct MyCustomCap;
//! impl UserCapability for MyCustomCap {
//!     const NAME: &'static str = "MyCustom";
//! }
//!
//! // ═══════════════════════════════════════════════════════════════════════════
//! // Define transforms with dependencies
//! // ═══════════════════════════════════════════════════════════════════════════
//!
//! struct LinkChecker;
//! struct LinkResolver;
//! struct SvgOptimizer;
//!
//! impl<C: Capabilities> CapTransform<Indexed, C> for LinkChecker {
//!     type Provides = LinksCheckedCap;
//!     type Output = <C as AddCapability<LinksCheckedCap>>::Output;
//!
//!     fn cap_transform(self, doc: Doc<Indexed, C>) -> Doc<Indexed, Self::Output> {
//!         doc.add_capability::<LinksCheckedCap>()
//!     }
//! }
//!
//! // LinkResolver requires LinksCheckedCap - compiler enforces this!
//! #[requires(C: LinksCheckedCap)]
//! fn resolve_links<C>(doc: Doc<Indexed, C>) -> Doc<Indexed, (LinksResolvedCap, C)> {
//!     doc.add_capability::<LinksResolvedCap>()
//! }
//!
//! // ═══════════════════════════════════════════════════════════════════════════
//! // Build pipeline - order checked at compile time
//! // ═══════════════════════════════════════════════════════════════════════════
//!
//! let doc: Doc<Indexed, EmptyCap> = Doc::new(indexed_doc);
//!
//! // ✅ Correct order: check → resolve
//! let doc = LinkChecker.cap_transform(doc);        // caps![LinksCheckedCap]
//! let doc = resolve_links(doc);                    // caps![LinksResolvedCap, LinksCheckedCap]
//!
//! // ❌ Wrong order: would NOT compile!
//! // let doc: Doc<Indexed, EmptyCap> = Doc::new(indexed_doc);
//! // let doc = resolve_links(doc);  // ERROR: LinksCheckedCap required but not available
//!
//! // ═══════════════════════════════════════════════════════════════════════════
//! // Type aliases with caps! macro
//! // ═══════════════════════════════════════════════════════════════════════════
//!
//! type FullyProcessed = caps![LinksResolvedCap, LinksCheckedCap, SvgOptimizedCap];
//! //   Expands to: (LinksResolvedCap, (LinksCheckedCap, caps![SvgOptimizedCap]))
//! ```
//!
//! ## How Phantom Index Works
//!
//! ```text
//! Capability set: (A, (B, (C, ())))
//! Search for B:
//!
//!   (A, (B, (C, ())))     HasCapability<B, ?>
//!    │                         │
//!    │ A ≠ B                   │ Try There<?>
//!    ▼                         ▼
//!   (B, (C, ()))          HasCapability<B, There<?>>
//!    │                         │
//!    │ B = B                   │ Match Here!
//!    ▼                         ▼
//!   Found!                HasCapability<B, There<Here>>
//!
//! Index encoding:  Here = 0,  There<Here> = 1,  There<There<Here>> = 2
//! ```

use std::marker::PhantomData;

// =============================================================================
// Sealed Pattern
// =============================================================================

mod sealed {
    pub trait Sealed {}

    // Allow UserCapability to also be sealed
    impl<T: super::UserCapability> Sealed for T {}
}

// =============================================================================
// Capability Trait
// =============================================================================

/// Capability marker trait - all capabilities must implement this
///
/// Uses sealed pattern to prevent downstream crates from bypassing checks.
/// Users define new capabilities by implementing `UserCapability`.
pub trait Capability: sealed::Sealed + 'static + Send + Sync {
    /// Capability name, used for debugging and error messages
    const NAME: &'static str;
}

/// User-extensible capability trait
///
/// Implementing this trait automatically implements `Capability`:
///
/// ```ignore
/// struct MyCustomCap;
/// impl UserCapability for MyCustomCap {
///     const NAME: &'static str = "MyCustom";
/// }
/// // Now MyCustomCap can be used as a Capability
/// ```
pub trait UserCapability: 'static + Send + Sync {
    const NAME: &'static str;
}

// Automatically implement Capability for UserCapability
impl<T: UserCapability> Capability for T {
    const NAME: &'static str = T::NAME;
}

// =============================================================================
// Built-in Capabilities
// =============================================================================

macro_rules! define_capability {
    ($(#[$meta:meta])* $name:ident, $display:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default)]
        pub struct $name;

        impl sealed::Sealed for $name {}

        impl Capability for $name {
            const NAME: &'static str = $display;
        }
    };
}

define_capability!(
    /// Marker: Links have been checked (existence validated)
    LinksCheckedCap,
    "LinksChecked"
);

define_capability!(
    /// Marker: Links have been resolved (relative → absolute paths)
    LinksResolvedCap,
    "LinksResolved"
);

define_capability!(
    /// Marker: SVGs have been optimized
    SvgOptimizedCap,
    "SvgOptimized"
);

define_capability!(
    /// Marker: Headings have been processed (anchor IDs generated)
    HeadingsProcessedCap,
    "HeadingsProcessed"
);

define_capability!(
    /// Marker: Media has been processed (image/video optimization)
    MediaProcessedCap,
    "MediaProcessed"
);

define_capability!(
    /// Marker: Metadata has been extracted
    MetadataExtractedCap,
    "MetadataExtracted"
);

// =============================================================================
// Capabilities Collection
// =============================================================================

/// Empty capability set - document has no capabilities yet
pub type EmptyCap = ();

/// Capability set trait - represents a type containing a set of capabilities
///
/// Uses recursive tuple definition:
/// - `EmptyCap` (aka `()`) is the empty set
/// - `(Cap, Rest)` represents a set containing `Cap` and all capabilities in `Rest`
pub trait Capabilities: 'static + Send + Sync {}

// Empty set
impl Capabilities for () {}

// Recursive definition: (Cap, Rest) is a capability set
impl<C: Capability, Rest: Capabilities> Capabilities for (C, Rest) {}

// =============================================================================
// Capability Query - Phantom Index Implementation
// =============================================================================

// Type-level index for capability search (phantom index technique)
// Inspired by tuplez/frunk HList search patterns

/// Type-level index: capability found at current position
pub struct Here;

/// Type-level index: capability found at deeper position
pub struct There<I>(PhantomData<I>);

/// Check if capability set `Self` contains capability `C`
///
/// Uses phantom index technique to allow searching at any depth.
/// The phantom index `I` disambiguates overlapping impls:
/// - `HasCapability<C, Here>` means C is at head
/// - `HasCapability<C, There<I>>` means C is deeper
///
/// The compiler will check this constraint when building pipelines.
/// If a required capability is missing, a clear compile error is produced.
///
/// # Example
///
/// ```ignore
/// fn requires_links_checked<C, I>(doc: Doc<Indexed, C>)
/// where
///     C: HasCapability<LinksCheckedCap, I>
/// {
///     // Can only be called when C contains LinksCheckedCap (at ANY position)
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "capability `{C}` is required but not available in `{Self}`",
    label = "this transform requires `{C}`",
    note = "try adding the appropriate Transform earlier in the pipeline"
)]
pub trait HasCapability<C: Capability, I = Here>: Capabilities {}

// Base case: capability at head position -> index is Here
impl<C: Capability, Rest: Capabilities> HasCapability<C, Here> for (C, Rest) {}

// Recursive case: capability deeper in the list -> index is There<I>
impl<C: Capability, Head: Capability, Rest, I> HasCapability<C, There<I>> for (Head, Rest)
where
    Rest: HasCapability<C, I>,
{
}

// =============================================================================
// Re-export #[requires] attribute macro from proc-macro crate
// =============================================================================

pub use tola_vdom_macros::requires;

// =============================================================================
// Helper Macros for Capability Types
// =============================================================================

/// Create a capability set type from a list of capabilities.
///
/// This macro converts a list of capabilities into the nested tuple representation:
/// ```ignore
/// caps![A, B, C]  // expands to: (A, (B, caps![C]))
/// caps![]         // expands to: EmptyCap
/// ```
///
/// Useful when calling functions that require specific capability sets:
/// ```ignore
/// // Instead of:
/// needs_a_and_b::<(TestCapA, (TestCapB, ())), _, _>();
///
/// // Write:
/// needs_a_and_b::<caps![TestCapA, TestCapB], _, _>();
/// ```
#[macro_export]
macro_rules! caps {
    () => { $crate::capability::EmptyCap };
    ($cap:ty) => { ($cap, $crate::capability::EmptyCap) };
    ($cap:ty, $($rest:ty),+ $(,)?) => {
        ($cap, $crate::caps![$($rest),+])
    };
}

/// Call a function with capability type and auto-inferred phantom indices.
///
/// This macro simplifies calling functions that have phantom index parameters:
/// ```ignore
/// // Instead of:
/// needs_a_and_b::<caps![TestCapA, TestCapB], _, _>();
///
/// // Write:
/// cap_call!(needs_a_and_b::<caps![TestCapA, TestCapB]>());
/// cap_call!(needs_a_and_b, caps![TestCapA, TestCapB]);
/// ```
#[macro_export]
macro_rules! cap_call {
    // Pattern: func_name::<Type>() - extracts and adds wildcards
    ($func:ident :: < $caps:ty > ( $($args:expr),* $(,)? )) => {
        $func::<$caps, _>($($args),*)
    };
    // Pattern: func_name, Type - simple form
    ($func:ident, $caps:ty) => {
        $func::<$caps, _>()
    };
    // Pattern with 2 phantom indices
    ($func:ident :: < $caps:ty > [2] ( $($args:expr),* $(,)? )) => {
        $func::<$caps, _, _>($($args),*)
    };
    ($func:ident, $caps:ty, 2) => {
        $func::<$caps, _, _>()
    };
    // Pattern with 3 phantom indices
    ($func:ident :: < $caps:ty > [3] ( $($args:expr),* $(,)? )) => {
        $func::<$caps, _, _, _>($($args),*)
    };
    ($func:ident, $caps:ty, 3) => {
        $func::<$caps, _, _, _>()
    };
}

// =============================================================================
// Add Capability
// =============================================================================

/// Add a new capability to a capability set
///
/// Transforms use this trait to declare capabilities they provide:
///
/// ```ignore
/// type Output = <C as AddCapability<LinksCheckedCap>>::Output;
/// ```
pub trait AddCapability<C: Capability>: Capabilities {
    type Output: Capabilities;
}

impl<C: Capability, Self_: Capabilities> AddCapability<C> for Self_ {
    type Output = (C, Self_);
}

// =============================================================================
// Doc Wrapper (Document with Capabilities)
// =============================================================================

use crate::node::Document;
use crate::phase::PhaseData;

/// Document wrapper carrying capability markers
///
/// `Doc<P, C>` represents a document in Phase `P` with capability set `C`.
///
/// # Type Parameters
///
/// * `P` - Phase (Raw, Indexed, Processed)
/// * `C` - Capabilities (capability set, defaults to empty `()`)
///
/// # Zero Overhead
///
/// This type uses `#[repr(transparent)]`, same memory layout as `Document<P>`.
/// `PhantomData<C>` is a zero-sized type, no runtime memory usage.
///
/// # Example
///
/// ```ignore
/// // Create initial document (no capabilities)
/// let doc: Doc<Indexed, EmptyCap> = Doc::new(indexed_doc);
///
/// // Add capabilities through Transform
/// let doc: Doc<Indexed, caps![LinksCheckedCap]> = doc.pipe(LinkChecker::new());
/// ```
#[repr(transparent)]
pub struct Doc<P: PhaseData, C: Capabilities = EmptyCap> {
    inner: Document<P>,
    _cap: PhantomData<C>,
}

impl<P: PhaseData> Doc<P, EmptyCap> {
    /// Create from Document, initial capabilities are empty
    pub fn new(doc: Document<P>) -> Self {
        Doc {
            inner: doc,
            _cap: PhantomData,
        }
    }
}

impl<P: PhaseData, C: Capabilities> Doc<P, C> {
    /// Set capability markers (internal use)
    pub(crate) fn with_capabilities<NewC: Capabilities>(doc: Document<P>) -> Doc<P, NewC> {
        Doc {
            inner: doc,
            _cap: PhantomData,
        }
    }

    /// Extract inner Document, discarding capability information
    pub fn into_inner(self) -> Document<P> {
        self.inner
    }

    /// Borrow inner Document
    pub fn as_inner(&self) -> &Document<P> {
        &self.inner
    }

    /// Mutably borrow inner Document
    pub fn as_inner_mut(&mut self) -> &mut Document<P> {
        &mut self.inner
    }

    /// Add capability marker (does not change document content)
    ///
    /// This is a zero-overhead operation, only changes the type.
    pub fn add_capability<Cap: Capability>(self) -> Doc<P, (Cap, C)> {
        Doc {
            inner: self.inner,
            _cap: PhantomData,
        }
    }

    /// Map inner Document
    pub fn map<F, Q: PhaseData>(self, f: F) -> Doc<Q, C>
    where
        F: FnOnce(Document<P>) -> Document<Q>,
    {
        Doc {
            inner: f(self.inner),
            _cap: PhantomData,
        }
    }
}

// Deref to inner Document for easy access
impl<P: PhaseData, C: Capabilities> std::ops::Deref for Doc<P, C> {
    type Target = Document<P>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<P: PhaseData, C: Capabilities> std::ops::DerefMut for Doc<P, C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

// =============================================================================
// CapTransform trait (Capability-aware Transform)
// =============================================================================

/// Transform trait with capability checking
///
/// Unlike the base `Transform` trait, `CapTransform` declares required and
/// provided capabilities at the type level, with compiler-checked dependencies.
///
/// # Example
///
/// ```ignore
/// impl<C> CapTransform<Indexed, C> for LinkResolver
/// where
///     C: HasCapability<LinksCheckedCap>,  // Requires: links checked
/// {
///     type Provides = LinksResolvedCap;
///     type Output = <C as AddCapability<LinksResolvedCap>>::Output;
///
///     fn cap_transform(self, doc: Doc<Indexed, C>) -> Doc<Indexed, Self::Output> {
///         // Implement link resolution logic...
///         doc.add_capability::<LinksResolvedCap>()
///     }
/// }
/// ```
pub trait CapTransform<P: PhaseData, C: Capabilities> {
    /// The capability this Transform provides
    type Provides: Capability;

    /// The output capability set
    type Output: Capabilities;

    /// Execute the transformation
    fn cap_transform(self, doc: Doc<P, C>) -> Doc<P, Self::Output>;
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Test capabilities
    struct TestCapA;
    impl sealed::Sealed for TestCapA {}
    impl Capability for TestCapA {
        const NAME: &'static str = "TestA";
    }

    struct TestCapB;
    impl sealed::Sealed for TestCapB {}
    impl Capability for TestCapB {
        const NAME: &'static str = "TestB";
    }

    struct TestCapC;
    impl sealed::Sealed for TestCapC {}
    impl Capability for TestCapC {
        const NAME: &'static str = "TestC";
    }

    #[test]
    fn test_capability_names() {
        assert_eq!(LinksCheckedCap::NAME, "LinksChecked");
        assert_eq!(SvgOptimizedCap::NAME, "SvgOptimized");
    }

    #[test]
    fn test_has_capability_at_any_depth() {
        // Test arbitrary depth search with phantom index
        // Note: The index I must be explicitly provided or inferred

        // Functions that accept phantom index (the idiomatic way)
        fn requires_a<C, I>()
        where
            C: HasCapability<TestCapA, I>,
        {
        }
        fn requires_b<C, I>()
        where
            C: HasCapability<TestCapB, I>,
        {
        }
        fn requires_c<C, I>()
        where
            C: HasCapability<TestCapC, I>,
        {
        }

        // Single capability at head
        requires_a::<(TestCapA, ()), Here>();
        requires_b::<(TestCapB, ()), Here>();

        // Capability at depth 1 - THIS IS THE KEY TEST
        requires_b::<(TestCapA, (TestCapB, ())), There<Here>>(); // B at depth 1
        requires_a::<(TestCapB, (TestCapA, ())), There<Here>>(); // A at depth 1

        // Capability at depth 2
        requires_a::<(TestCapC, (TestCapB, (TestCapA, ()))), There<There<Here>>>(); // A at depth 2
    }

    #[test]
    fn test_has_capability_multiple() {
        // Test multiple capability requirements at various depths
        fn requires_both<C, Ia, Ib>()
        where
            C: HasCapability<TestCapA, Ia> + HasCapability<TestCapB, Ib>,
        {
        }

        fn requires_all<C, Ia, Ib, Ic>()
        where
            C: HasCapability<TestCapA, Ia>
                + HasCapability<TestCapB, Ib>
                + HasCapability<TestCapC, Ic>,
        {
        }

        // A@head, B@1
        requires_both::<(TestCapA, (TestCapB, ())), Here, There<Here>>();

        // B@head, A@1
        requires_both::<(TestCapB, (TestCapA, ())), There<Here>, Here>();

        // C@head, A@1, B@2
        requires_both::<(TestCapC, (TestCapA, (TestCapB, ()))), There<Here>, There<There<Here>>>();

        // All three at different depths
        requires_all::<(TestCapA, (TestCapB, (TestCapC, ()))), Here, There<Here>, There<There<Here>>>(
        );
        requires_all::<(TestCapC, (TestCapB, (TestCapA, ()))), There<There<Here>>, There<Here>, Here>(
        );
    }

    #[test]
    fn test_add_capability() {
        // Test AddCapability trait
        type Initial = ();
        type WithA = <Initial as AddCapability<TestCapA>>::Output;
        type WithAB = <WithA as AddCapability<TestCapB>>::Output;
        type WithABC = <WithAB as AddCapability<TestCapC>>::Output;

        fn check_type<T>() {}
        check_type::<WithA>(); // (TestCapA, ())
        check_type::<WithAB>(); // (TestCapB, (TestCapA, ()))
        check_type::<WithABC>(); // (TestCapC, (TestCapB, (TestCapA, ())))
    }

    #[test]
    fn test_pipeline_simulation() {
        // Simulate a real pipeline where transforms add capabilities
        // With phantom index, we can check capabilities at ANY depth!

        fn requires_a<C, I>()
        where
            C: HasCapability<TestCapA, I>,
        {
        }
        fn requires_b<C, I>()
        where
            C: HasCapability<TestCapB, I>,
        {
        }
        fn requires_c<C, I>()
        where
            C: HasCapability<TestCapC, I>,
        {
        }

        // Initial: no capabilities
        type C0 = ();

        // After Transform1 (provides A): (A, ())
        type C1 = <C0 as AddCapability<TestCapA>>::Output;
        requires_a::<C1, Here>();

        // After Transform2 (provides B): (B, (A, ()))
        type C2 = <C1 as AddCapability<TestCapB>>::Output;
        requires_b::<C2, Here>(); // B at head
        requires_a::<C2, There<Here>>(); // A at depth 1 - NOW WORKS!

        // After Transform3 (provides C): (C, (B, (A, ())))
        type C3 = <C2 as AddCapability<TestCapC>>::Output;
        requires_c::<C3, Here>(); // C at head
        requires_b::<C3, There<Here>>(); // B at depth 1
        requires_a::<C3, There<There<Here>>>(); // A at depth 2
    }

    #[test]
    fn test_flexible_pipeline_ordering() {
        // This is the key benefit: transforms can be reordered freely
        // as long as dependencies are satisfied somewhere in the chain

        fn requires_a<C, I>()
        where
            C: HasCapability<TestCapA, I>,
        {
        }
        fn requires_b<C, I>()
        where
            C: HasCapability<TestCapB, I>,
        {
        }

        // Scenario: LinkChecker(A) -> SvgOptimizer(B) -> LinkResolver(needs A)
        type AfterLinkChecker = (TestCapA, ());
        type AfterSvgOptimizer = (TestCapB, (TestCapA, ()));

        // LinkResolver can now run because A is findable at depth 1
        requires_a::<AfterSvgOptimizer, There<Here>>(); // ✅ Works with phantom index!
        requires_b::<AfterSvgOptimizer, Here>(); // ✅ B at head
    }

    #[test]
    fn test_cap_transform_with_phantom_index() {
        use crate::phase::Indexed;

        struct MockTransformA;

        // Transform that provides A (no requirements)
        impl<C: Capabilities> CapTransform<Indexed, C> for MockTransformA {
            type Provides = TestCapA;
            type Output = <C as AddCapability<TestCapA>>::Output;
            fn cap_transform(self, doc: Doc<Indexed, C>) -> Doc<Indexed, Self::Output> {
                doc.add_capability::<TestCapA>()
            }
        }

        // Type-level pipeline verification
        type AfterA = <MockTransformA as CapTransform<Indexed, EmptyCap>>::Output;

        fn check<T>() {}
        check::<AfterA>(); // caps![TestCapA]

        // Note: For transforms that need capabilities at arbitrary depth,
        // the phantom index I must be part of the impl signature.
        // This is a limitation that can be worked around with helper traits
        // or by accepting the index as a type parameter on the transform itself.
    }

    #[test]
    fn test_requires_macro() {
        use crate::capability::requires;

        // ═══════════════════════════════════════════════════════════════════════
        // Basic usage
        // ═══════════════════════════════════════════════════════════════════════

        // Single capability requirement using attribute macro
        #[requires(C: TestCapA)]
        fn needs_a<C>() {}

        // Multiple capability requirements
        #[requires(C: TestCapA, TestCapB)]
        fn needs_a_and_b<C>() {}

        // ═══════════════════════════════════════════════════════════════════════
        // With existing where clause
        // ═══════════════════════════════════════════════════════════════════════

        // Existing where clause on DIFFERENT type parameter - should add new predicate
        #[requires(C: TestCapA)]
        fn with_other_where<P, C>()
        where
            P: 'static,
        {
        }

        // Existing where clause on SAME type parameter - should MERGE bounds
        #[requires(C: TestCapA, TestCapB)]
        fn with_c_where<C>()
        where
            C: Capabilities,
        {
        }

        // Existing where clause with multiple bounds on C - should merge
        #[requires(C: TestCapC)]
        fn with_multi_bound_where<C>()
        where
            C: Capabilities + Send,
        {
        }

        // ═══════════════════════════════════════════════════════════════════════
        // With bounds in generic parameter position
        // ═══════════════════════════════════════════════════════════════════════

        // Bounds in generic params are preserved, new constraints go to where clause
        #[requires(C: TestCapA)]
        fn with_inline_bound<C: Capabilities>() {}

        // Multiple generic params with inline bounds
        #[requires(C: TestCapB)]
        fn multi_generic_inline<P: 'static, C: Capabilities>() {}

        // ═══════════════════════════════════════════════════════════════════════
        // Complex scenarios
        // ═══════════════════════════════════════════════════════════════════════

        // Both inline bounds AND where clause
        #[requires(C: TestCapA)]
        fn both_inline_and_where<P: Clone, C: Capabilities>()
        where
            P: Send,
        {
        }

        // Where clause already has a HasCapability (non-overlapping with requires)
        // Note: This is rare but valid - manually specifying one while requiring another
        #[requires(C: TestCapB)]
        fn already_has_cap<C, ExistingI>()
        where
            C: HasCapability<TestCapA, ExistingI>,
        {
        }

        // ═══════════════════════════════════════════════════════════════════════
        // Call all functions to verify they compile and work
        // ═══════════════════════════════════════════════════════════════════════

        needs_a::<caps![TestCapA], _>();
        needs_a::<caps![TestCapB, TestCapA], _>(); // A at depth 1

        needs_a_and_b::<caps![TestCapA, TestCapB], _, _>();
        needs_a_and_b::<caps![TestCapB, TestCapA], _, _>();
        needs_a_and_b::<caps![TestCapC, TestCapB, TestCapA], _, _>(); // Both at depth

        with_other_where::<String, caps![TestCapA], _>();
        with_c_where::<caps![TestCapA, TestCapB], _, _>();
        with_multi_bound_where::<caps![TestCapC], _>();
        with_inline_bound::<caps![TestCapA], _>();
        multi_generic_inline::<i32, caps![TestCapB], _>();
        both_inline_and_where::<String, caps![TestCapA], _>();
        already_has_cap::<caps![TestCapA, TestCapB], _, _>();

        // Using cap_call! macro to avoid writing _ placeholders
        cap_call!(needs_a, caps![TestCapA]);
        cap_call!(needs_a_and_b, caps![TestCapA, TestCapB], 2);
    }

    #[test]
    fn test_caps_macro() {
        // Test the caps! macro generates correct types
        fn assert_type<T>() {}

        assert_type::<caps![]>(); // ()
        assert_type::<caps![TestCapA]>(); // (TestCapA, ())
        assert_type::<caps![TestCapA, TestCapB]>(); // (TestCapA, (TestCapB, ()))
        assert_type::<caps![TestCapA, TestCapB, TestCapC]>(); // (TestCapA, (TestCapB, (TestCapC, ())))

        // Verify they're the same types
        let _: caps![] = ();
        let _: caps![TestCapA] = (TestCapA, ());
        let _: caps![TestCapA, TestCapB] = (TestCapA, (TestCapB, ()));
    }
}
