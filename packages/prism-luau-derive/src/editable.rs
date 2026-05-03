//! `#[derive(Editable)]` — emits an inherent
//! `apply_field(&mut self, key: &str, value: &str)` method that
//! dispatches stringly-typed property-panel edits to the right
//! field by name. Designed to collapse the `apply_*` fns in
//! `prism-shell/src/app/mutations.rs`.
//!
//! Type → parser mapping (driven by the field's Rust type):
//!
//! | Rust type            | Behaviour                                              |
//! |----------------------|--------------------------------------------------------|
//! | `Option<String>`     | empty `value` → `None`, else `Some(value.to_string())` |
//! | `Option<bool>`       | `Some(value == "true")`                                |
//! | `Option<f32/f64>`    | `value.parse().ok()` (clamped if `clamp(…)` set)       |
//! | `Option<integer>`    | `value.parse().ok()` (clamped if `clamp(…)` set)       |
//! | `String`             | `value.to_string()`                                    |
//! | `bool`               | `value == "true"`                                      |
//! | `f32/f64/integer`    | `parse().ok()` falls back to existing field value      |
//!
//! Per-field attributes:
//! - `#[edit(skip)]` — drop the field from the dispatch table.
//! - `#[edit(rename = "key")]` — match `key` against this string
//!   instead of the field name.
//! - `#[edit(clamp(min, max))]` — for numeric (or `Option<numeric>`)
//!   fields, clamp the parsed value before assignment.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Field, Fields, Type};

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let ident = &input.ident;
    let Data::Struct(s) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(Editable)] only supports structs with named fields",
        ));
    };
    let Fields::Named(named) = &s.fields else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(Editable)] requires named fields",
        ));
    };

    let mut arms = Vec::new();
    for field in &named.named {
        let attrs = parse_field_attrs(field)?;
        if attrs.skip {
            continue;
        }
        arms.push(build_arm(field, &attrs)?);
    }

    Ok(quote! {
        impl #ident {
            pub fn apply_field(&mut self, key: &str, value: &str) {
                match key {
                    #(#arms)*
                    _ => {}
                }
            }
        }
    })
}

#[derive(Default)]
struct EditAttrs {
    skip: bool,
    rename: Option<String>,
    clamp: Option<(f64, f64)>,
}

fn parse_field_attrs(field: &Field) -> syn::Result<EditAttrs> {
    let mut out = EditAttrs::default();
    for attr in &field.attrs {
        if !attr.path().is_ident("edit") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                out.skip = true;
            } else if meta.path.is_ident("rename") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.rename = Some(lit.value());
            } else if meta.path.is_ident("clamp") {
                let content;
                syn::parenthesized!(content in meta.input);
                let lo: syn::LitFloat = content.parse()?;
                let _: syn::Token![,] = content.parse()?;
                let hi: syn::LitFloat = content.parse()?;
                out.clamp = Some((lo.base10_parse()?, hi.base10_parse()?));
            } else {
                return Err(
                    meta.error("unknown #[edit] arg (expected `skip`, `rename`, or `clamp`)")
                );
            }
            Ok(())
        })?;
    }
    Ok(out)
}

fn build_arm(field: &Field, attrs: &EditAttrs) -> syn::Result<TokenStream2> {
    let fname = field.ident.as_ref().expect("named field");
    let key = attrs.rename.clone().unwrap_or_else(|| fname.to_string());
    let assign = build_assign(field, attrs)?;
    Ok(quote! {
        #key => { #assign }
    })
}

fn build_assign(field: &Field, attrs: &EditAttrs) -> syn::Result<TokenStream2> {
    let fname = field.ident.as_ref().expect("named field");
    let ty = &field.ty;

    if let Some(inner) = option_inner(ty) {
        let inner_name = type_leaf_name(inner);
        let rhs = parse_inner_expr(inner, inner_name.as_deref(), attrs, field)?;
        Ok(quote! { self.#fname = #rhs; })
    } else {
        let name = type_leaf_name(ty);
        match name.as_deref() {
            Some("String") => Ok(quote! { self.#fname = value.to_string(); }),
            Some("bool") => Ok(quote! { self.#fname = value == "true"; }),
            Some(n) if is_numeric(n) => {
                let clamp = clamp_apply(attrs);
                Ok(quote! {
                    if let Ok(v) = value.parse::<#ty>() {
                        self.#fname = v #clamp;
                    }
                })
            }
            _ => Err(syn::Error::new_spanned(
                field,
                "#[derive(Editable)] cannot infer a parser for this field — supported: String, bool, f32/f64, integer types, or Option<…> of those. Use #[edit(skip)] to opt out.",
            )),
        }
    }
}

fn parse_inner_expr(
    inner: &Type,
    inner_name: Option<&str>,
    attrs: &EditAttrs,
    field: &Field,
) -> syn::Result<TokenStream2> {
    match inner_name {
        Some("String") => Ok(quote! {
            if value.is_empty() { ::core::option::Option::None }
            else { ::core::option::Option::Some(value.to_string()) }
        }),
        Some("bool") => Ok(quote! {
            ::core::option::Option::Some(value == "true")
        }),
        Some(n) if is_numeric(n) => {
            let clamp = clamp_map(attrs);
            Ok(quote! { value.parse::<#inner>().ok() #clamp })
        }
        _ => Err(syn::Error::new_spanned(
            field,
            "#[derive(Editable)] cannot infer a parser for this Option<…> field — supported inner types: String, bool, f32/f64, integer types. Use #[edit(skip)] to opt out.",
        )),
    }
}

/// Clamp suffix for a parsed required numeric value: ` .clamp(lo, hi)`.
fn clamp_apply(attrs: &EditAttrs) -> TokenStream2 {
    match attrs.clamp {
        Some((lo, hi)) => quote! { .clamp(#lo as _, #hi as _) },
        None => quote! {},
    }
}

/// Clamp suffix for an `Option<numeric>` parse result: ` .map(|v| v.clamp(...))`.
fn clamp_map(attrs: &EditAttrs) -> TokenStream2 {
    match attrs.clamp {
        Some((lo, hi)) => quote! { .map(|v| v.clamp(#lo as _, #hi as _)) },
        None => quote! {},
    }
}

fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    for a in &args.args {
        if let syn::GenericArgument::Type(t) = a {
            return Some(t);
        }
    }
    None
}

fn type_leaf_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(r) => type_leaf_name(&r.elem),
        _ => None,
    }
}

fn is_numeric(name: &str) -> bool {
    matches!(
        name,
        "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
    )
}
