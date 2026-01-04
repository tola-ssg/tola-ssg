//! Procedural macros for tola-vdom capability system
//!
//! Provides the `#[requires]` attribute macro and helper macros for capabilities.

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parse_macro_input, punctuated::Punctuated, GenericParam, ItemFn, Type, TypeParam, WhereClause,
    WherePredicate,
};

/// Attribute macro to add capability requirements to a function.
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
/// The macro automatically:
/// 1. Adds phantom index type parameters (__I0, __I1, ...)
/// 2. Generates HasCapability bounds with those indices
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

    // Add phantom index type parameters
    for (i, _) in caps.iter().enumerate() {
        let idx_name = syn::Ident::new(&format!("__I{}", i), proc_macro2::Span::call_site());
        func.sig
            .generics
            .params
            .push(GenericParam::Type(TypeParam::from(idx_name)));
    }

    // Build where clause predicates
    let type_param_ident: syn::Ident = syn::parse_str(type_param_name).unwrap();
    let mut bounds = Vec::new();

    for (i, cap) in caps.iter().enumerate() {
        let cap_type: Type = syn::parse_str(cap).unwrap();
        let idx_name = syn::Ident::new(&format!("__I{}", i), proc_macro2::Span::call_site());

        // Always use ::tola_vdom:: path (works both inside and outside the crate
        // thanks to `extern crate self as tola_vdom` in lib.rs)
        bounds.push(quote! {
            ::tola_vdom::capability::HasCapability<#cap_type, #idx_name>
        });
    }

    let new_predicate: WherePredicate = syn::parse_quote! {
        #type_param_ident: #(#bounds)+*
    };

    // Add or extend where clause
    if let Some(ref mut where_clause) = func.sig.generics.where_clause {
        where_clause.predicates.push(new_predicate);
    } else {
        func.sig.generics.where_clause = Some(WhereClause {
            where_token: Default::default(),
            predicates: {
                let mut p = Punctuated::new();
                p.push(new_predicate);
                p
            },
        });
    }

    func.into_token_stream().into()
}
