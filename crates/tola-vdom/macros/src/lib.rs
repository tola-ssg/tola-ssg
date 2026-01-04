//! Procedural macros for tola-vdom capability system
//!
//! Provides the `#[requires]` attribute macro and helper macros for capabilities.

use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{
    parse_macro_input, punctuated::Punctuated, GenericParam, ItemFn, Token, Type, TypeParam,
    TypeParamBound, WhereClause, WherePredicate,
};

/// Attribute macro to add capability requirements to a function.
///
/// # Basic Usage
///
/// Instead of writing:
/// ```ignore
/// fn my_transform<C, I1, I2>(doc: Doc<Indexed, C>)
/// where
///     C: HasCapability<LinksCheckedCap, I1> + HasCapability<SvgOptimizedCap, I2>,
/// { ... }
/// ```
///
/// You can write:
/// ```ignore
/// #[requires(C: LinksCheckedCap, SvgOptimizedCap)]
/// fn my_transform<C>(doc: Doc<Indexed, C>) { ... }
/// ```
///
/// # Advanced Usage
///
/// The macro correctly handles existing bounds:
///
/// ```ignore
/// // Bounds in generic parameters are preserved
/// #[requires(C: LinksCheckedCap)]
/// fn foo<C: Capabilities>(doc: Doc<P, C>) { ... }
/// // Expands to: fn foo<C: Capabilities, __I0>(doc: Doc<P, C>)
/// //             where C: HasCapability<LinksCheckedCap, __I0>
///
/// // Existing where clauses are merged
/// #[requires(C: LinksCheckedCap)]
/// fn bar<C>(doc: Doc<P, C>)
/// where
///     C: Capabilities + Clone,
/// { ... }
/// // Expands to: fn bar<C, __I0>(doc: Doc<P, C>)
/// //             where C: Capabilities + Clone + HasCapability<LinksCheckedCap, __I0>
///
/// // Multiple constraints in where clause work too
/// #[requires(C: CapA, CapB)]
/// fn baz<P, C>(doc: Doc<P, C>)
/// where
///     P: PhaseData,
///     C: Debug,
/// { ... }
/// // Expands to: fn baz<P, C, __I0, __I1>(doc: Doc<P, C>)
/// //             where P: PhaseData,
/// //                   C: Debug + HasCapability<CapA, __I0> + HasCapability<CapB, __I1>
/// ```
///
/// # What the macro does
///
/// 1. Adds phantom index type parameters (`__I0`, `__I1`, ...)
/// 2. Generates `HasCapability` bounds with those indices
/// 3. Merges with existing bounds on the capability type parameter
/// 4. Preserves all other generic parameters and where clause predicates
#[proc_macro_attribute]
pub fn requires(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut func = parse_macro_input!(item as ItemFn);

    // Parse attribute: "C: Cap1, Cap2, Cap3"
    let attr_str = attr.to_string();

    let parts: Vec<&str> = attr_str.splitn(2, ':').collect();
    if parts.len() != 2 {
        return syn::Error::new_spanned(
            func.sig.ident.clone(),
            "Expected format: #[requires(C: Cap1, Cap2, ...)]",
        )
        .to_compile_error()
        .into();
    }

    let type_param_name = parts[0].trim();
    let caps_str = parts[1].trim();
    let caps: Vec<&str> = caps_str.split(',').map(|s| s.trim()).collect();

    let type_param_ident: syn::Ident = match syn::parse_str(type_param_name) {
        Ok(ident) => ident,
        Err(e) => return e.to_compile_error().into(),
    };

    // Add phantom index type parameters
    for (i, _) in caps.iter().enumerate() {
        let idx_name = syn::Ident::new(&format!("__I{}", i), proc_macro2::Span::call_site());
        func.sig
            .generics
            .params
            .push(GenericParam::Type(TypeParam::from(idx_name)));
    }

    // Build HasCapability bounds for each required capability
    let mut new_bounds: Vec<TypeParamBound> = Vec::new();

    for (i, cap) in caps.iter().enumerate() {
        let cap_type: Type = match syn::parse_str(cap) {
            Ok(t) => t,
            Err(e) => return e.to_compile_error().into(),
        };
        let idx_name = syn::Ident::new(&format!("__I{}", i), proc_macro2::Span::call_site());

        // Always use ::tola_vdom:: path (works both inside and outside the crate
        // thanks to `extern crate self as tola_vdom` in lib.rs)
        let bound: TypeParamBound = syn::parse_quote! {
            ::tola_vdom::capability::HasCapability<#cap_type, #idx_name>
        };
        new_bounds.push(bound);
    }

    // Strategy: Try to merge with existing where clause predicate for C,
    // otherwise add a new predicate
    let mut merged = false;

    if let Some(ref mut where_clause) = func.sig.generics.where_clause {
        // Look for existing predicate on C and merge
        for predicate in where_clause.predicates.iter_mut() {
            if let WherePredicate::Type(pred_type) = predicate {
                // Check if this predicate is for our type parameter
                if let Type::Path(ref type_path) = pred_type.bounded_ty {
                    if type_path.path.is_ident(&type_param_ident) {
                        // Found it! Merge our bounds into existing bounds
                        for bound in new_bounds.drain(..) {
                            pred_type.bounds.push(bound);
                        }
                        merged = true;
                        break;
                    }
                }
            }
        }

        // If not merged, add new predicate
        if !merged {
            let new_predicate: WherePredicate = syn::parse_quote! {
                #type_param_ident: #(#new_bounds)+*
            };
            where_clause.predicates.push(new_predicate);
        }
    } else {
        // No where clause, create one
        let new_predicate: WherePredicate = syn::parse_quote! {
            #type_param_ident: #(#new_bounds)+*
        };
        func.sig.generics.where_clause = Some(WhereClause {
            where_token: Token![where](proc_macro2::Span::call_site()),
            predicates: {
                let mut p = Punctuated::new();
                p.push(new_predicate);
                p
            },
        });
    }

    func.into_token_stream().into()
}
