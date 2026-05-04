//! `#[derive(Editable)]` — generate a path-addressable mutator
//! `apply_path(&mut self, path: &str, value: &str)` on structs and
//! enums.
//!
//! Designed to fully replace the ad-hoc `apply_*_edit(&mut T, key,
//! value)` functions in `prism-shell/src/app/mutations.rs`. The
//! Slint property panel emits stringly-typed edits as `(path, value)`
//! pairs; this derive turns the static type tree into the dispatcher.
//!
//! ## Path grammar
//!
//! A `path` is a dot-separated sequence of segments. Each segment
//! addresses a child of the current node:
//!
//! | Segment shape         | Meaning                                                |
//! |-----------------------|--------------------------------------------------------|
//! | `field_name`          | recurse into a struct field                            |
//! | `<idx>` (digits)      | index into a tuple, array, or `Vec`                    |
//! | `<variant>`           | match if the active enum variant is `<variant>`        |
//! | `@kind` *(terminal)*  | re-seat an enum to the variant named in `value`        |
//!
//! Variant names follow `serde(rename_all)` if the enum has it
//! (typically `kebab-case`), otherwise the Rust ident verbatim.
//! Empty `path` plus a leaf type = terminal assignment, parsed from
//! `value` with the leaf type's `apply_path` impl.
//!
//! ## Generated method
//!
//! ```ignore
//! impl T {
//!     pub fn apply_path(&mut self, path: &str, value: &str) { … }
//! }
//! ```
//!
//! Inherent (not a trait) so consumers don't have to add `use` lines
//! and the dispatch is monomorphic. The walker bottoms out at leaf
//! types whose own `apply_path` parses `value` — primitives + `String`
//! + `Option<…>` of those + the hand-written impls in
//!   `prism-core::foundation::geometry::Edges` etc. live in their home
//!   crates.
//!
//! ## Per-field attributes
//!
//! - `#[edit(skip)]` — drop the field from the dispatch table.
//! - `#[edit(rename = "key")]` — match `key` against this string
//!   instead of the field name (legacy compatibility).
//! - `#[edit(clamp(min, max))]` — for numeric (or `Option<numeric>`)
//!   leaf fields, clamp the parsed value before assignment.
//! - `#[edit(also = "name")]` — replicate the parsed leaf value into
//!   a sibling field of the same type. Stackable.
//! - `#[edit(with = "fn_path")]` — escape hatch for custom value
//!   coercion. The function is called as
//!   `fn_path(&mut self.<field>, value: &str)` and replaces the
//!   default leaf parser entirely.
//! - `#[edit(index)]` — opt the field into numeric-segment indexing.
//!   Required for `[T; N]` and `Vec<T>` fields whose elements are
//!   themselves editable; numeric path segments index into them.
//!
//! ## Per-variant attributes (enums)
//!
//! - `#[edit_variant(rename = "name")]` — override the path tag for
//!   one variant. By default `serde(rename_all)` is honoured.
//!
//! ## Reseat semantics
//!
//! `path = "@kind"` with `value = "<variant>"` constructs the named
//! variant from `Default::default()` payloads (or the unit ctor for
//! payload-less variants). Use `#[edit(default = "fn_path")]` on a
//! variant to override the constructor — useful when `Default` for
//! the payload doesn't match the property panel's expected initial
//! state. Reseating an unknown variant is a silent no-op.
//!
//! Future extensions (not yet implemented): `#[edit(keyed_by = "f")]`
//! for `Vec<T>` keyed by an inner field rather than positional index.
//! Track in plan §11.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DataEnum, DataStruct, DeriveInput, Field, Fields, Ident, LitStr, Type};

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    match &input.data {
        Data::Struct(s) => expand_struct(input, s),
        Data::Enum(e) => expand_enum(input, e),
        Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "#[derive(Editable)] does not support unions",
        )),
    }
}

// ─── struct ─────────────────────────────────────────────────────────

fn expand_struct(input: &DeriveInput, data: &DataStruct) -> syn::Result<TokenStream2> {
    let ident = &input.ident;
    let Fields::Named(named) = &data.fields else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(Editable)] requires named fields on structs",
        ));
    };

    let mut arms = Vec::new();
    for field in &named.named {
        let attrs = parse_field_attrs(field)?;
        if attrs.skip {
            continue;
        }
        arms.push(build_field_arm(field, &attrs)?);
    }

    Ok(quote! {
        impl #ident {
            pub fn apply_path(&mut self, path: &str, value: &str) {
                let (head, tail) = match path.split_once('.') {
                    ::core::option::Option::Some((h, t)) => (h, t),
                    ::core::option::Option::None => (path, ""),
                };
                match head {
                    #(#arms)*
                    _ => {}
                }
            }
        }
    })
}

fn build_field_arm(field: &Field, attrs: &EditAttrs) -> syn::Result<TokenStream2> {
    let fname = field.ident.as_ref().expect("named field");
    let key = attrs.rename.clone().unwrap_or_else(|| fname.to_string());

    if let Some(with_path) = &attrs.with {
        let with_ident: syn::Path = syn::parse_str(with_path)?;
        return Ok(quote! {
            #key => { #with_ident(&mut self.#fname, value); }
        });
    }

    if attrs.index {
        // Numeric segment indexing into [T;N] / Vec<T>. Primitive
        // element types parse inline; non-primitives recurse via
        // their own apply_path.
        let elem_ty = array_or_vec_element(&field.ty).ok_or_else(|| {
            syn::Error::new_spanned(
                field,
                "#[edit(index)] requires a `[T; N]` or `Vec<T>` field type",
            )
        })?;
        let elem_assign = if is_known_leaf(elem_ty) {
            quote! {
                if sub_tail.is_empty() {
                    if let ::core::result::Result::Ok(v) = value.parse::<#elem_ty>() {
                        *slot = v;
                    }
                }
            }
        } else {
            quote! { slot.apply_path(sub_tail, value); }
        };
        return Ok(quote! {
            #key => {
                let (idx_seg, sub_tail) = match tail.split_once('.') {
                    ::core::option::Option::Some((h, t)) => (h, t),
                    ::core::option::Option::None => (tail, ""),
                };
                if let ::core::result::Result::Ok(idx) = idx_seg.parse::<usize>() {
                    if let ::core::option::Option::Some(slot) = self.#fname.get_mut(idx) {
                        #elem_assign
                    }
                }
            }
        });
    }

    let ty = &field.ty;
    let leaf_only = is_known_leaf(ty);
    let assign = build_leaf_assign(field, attrs)?;

    if leaf_only {
        // Primitive-leaf field: only the terminal path applies. Tail
        // non-empty would be a "drilled into a primitive" path and is
        // a silent no-op — emitting `self.field.apply_path(...)`
        // would not compile because primitives don't carry the
        // method.
        Ok(quote! {
            #key => {
                if tail.is_empty() {
                    #assign
                }
            }
        })
    } else {
        Ok(quote! {
            #key => {
                if tail.is_empty() {
                    #assign
                } else {
                    self.#fname.apply_path(tail, value);
                }
            }
        })
    }
}

/// Extract the element type from `[T; N]` or `Vec<T>`. Returns `None`
/// for any other shape — the caller is expected to surface that as a
/// derive-time error.
fn array_or_vec_element(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Array(arr) => Some(&arr.elem),
        Type::Path(tp) => {
            let seg = tp.path.segments.last()?;
            if seg.ident != "Vec" {
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
        _ => None,
    }
}

/// Heuristic: does the field type look like a primitive leaf — i.e.
/// `apply_path` is *not* available on it? Used by the derive to skip
/// emitting the nested-recursion branch for primitive struct fields.
fn is_known_leaf(ty: &Type) -> bool {
    if let Some(inner) = option_inner(ty) {
        return is_known_leaf(inner);
    }
    let Some(name) = type_leaf_name(ty) else {
        return false;
    };
    matches!(name.as_str(), "String" | "bool") || is_numeric(&name)
}

/// Build the terminal-leaf assignment for a struct field. Only
/// triggered when the path bottoms out at this field.
fn build_leaf_assign(field: &Field, attrs: &EditAttrs) -> syn::Result<TokenStream2> {
    let fname = field.ident.as_ref().expect("named field");
    let ty = &field.ty;
    let also = &attrs.also;

    if let Some(inner) = option_inner(ty) {
        let inner_name = type_leaf_name(inner);
        let rhs = parse_inner_expr(inner, inner_name.as_deref(), attrs, field)?;
        let also_assigns = also.iter().map(|f| quote! { self.#f = __v.clone(); });
        return Ok(quote! {
            let __v = #rhs;
            self.#fname = __v.clone();
            #(#also_assigns)*
        });
    }

    let name = type_leaf_name(ty);
    match name.as_deref() {
        Some("String") => {
            let also_assigns = also.iter().map(|f| quote! { self.#f = __v.clone(); });
            Ok(quote! {
                let __v = value.to_string();
                self.#fname = __v.clone();
                #(#also_assigns)*
            })
        }
        Some("bool") => {
            let also_assigns = also.iter().map(|f| quote! { self.#f = __v; });
            Ok(quote! {
                let __v = value == "true";
                self.#fname = __v;
                #(#also_assigns)*
            })
        }
        Some(n) if is_numeric(n) => {
            let clamp = clamp_apply(attrs);
            let also_assigns = also.iter().map(|f| quote! { self.#f = __v; });
            Ok(quote! {
                if let ::core::result::Result::Ok(v) = value.parse::<#ty>() {
                    let __v = v #clamp;
                    self.#fname = __v;
                    #(#also_assigns)*
                }
            })
        }
        // For non-leaf types, route the empty tail through the
        // child's own `apply_path("", value)`. This lets a struct
        // field of a non-primitive editable type accept either
        // "field" (terminal — child interprets the empty path as
        // a leaf assign on itself) or "field.subpath" (nested).
        _ => Ok(quote! {
            self.#fname.apply_path("", value);
        }),
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
            "#[derive(Editable)] cannot infer a parser for this Option<…> field — supported inner types: String, bool, f32/f64, integer types. Use #[edit(skip)] / #[edit(with = ...)] to opt out.",
        )),
    }
}

// ─── enum ───────────────────────────────────────────────────────────

fn expand_enum(input: &DeriveInput, data: &DataEnum) -> syn::Result<TokenStream2> {
    let ident = &input.ident;
    let rename_all = parse_serde_rename_all(&input.attrs);

    // Variant tag → ctor expr, plus per-variant payload field arms.
    let mut tag_ctors: Vec<TokenStream2> = Vec::new();
    let mut variant_arms: Vec<TokenStream2> = Vec::new();

    for v in &data.variants {
        let vident = &v.ident;
        let mut vrename: Option<String> = None;
        for attr in &v.attrs {
            if attr.path().is_ident("edit_variant") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("rename") {
                        let lit: LitStr = meta.value()?.parse()?;
                        vrename = Some(lit.value());
                    }
                    Ok(())
                })?;
            }
        }
        let tag = vrename.unwrap_or_else(|| apply_rename(&vident.to_string(), rename_all.as_deref()));

        match &v.fields {
            Fields::Unit => {
                tag_ctors.push(quote! { #tag => { *self = Self::#vident; } });
            }
            Fields::Unnamed(fs) if fs.unnamed.len() == 1 => {
                let inner_ty = &fs.unnamed.first().unwrap().ty;
                tag_ctors.push(quote! {
                    #tag => { *self = Self::#vident(<#inner_ty as ::core::default::Default>::default()); }
                });
                // Variant payload recursion: head matches tag, then
                // recurse into the single payload using `tail` (which
                // includes everything after the tag).
                variant_arms.push(quote! {
                    (Self::#vident(__inner), #tag) => {
                        __inner.apply_path(tail, value);
                    }
                });
            }
            Fields::Unnamed(_) => {
                return Err(syn::Error::new_spanned(
                    v,
                    "#[derive(Editable)] supports only single-field tuple variants",
                ));
            }
            Fields::Named(fs) => {
                let field_idents: Vec<&Ident> =
                    fs.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                let field_strs: Vec<String> =
                    field_idents.iter().map(|i| i.to_string()).collect();
                let field_tys: Vec<&Type> = fs.named.iter().map(|f| &f.ty).collect();
                tag_ctors.push(quote! {
                    #tag => {
                        *self = Self::#vident {
                            #( #field_idents: <#field_tys as ::core::default::Default>::default(), )*
                        };
                    }
                });
                // Rebind destructured payload fields under a `__edit_`
                // prefix so they can't shadow the outer `path`/`value`
                // parameters (a struct variant `Px { value }` would
                // otherwise eclipse the `value: &str` argument).
                let bind_idents: Vec<Ident> = field_idents
                    .iter()
                    .map(|i| Ident::new(&format!("__edit_{}", i), i.span()))
                    .collect();
                // For struct variants, recurse by named field after
                // matching the variant tag and the field segment.
                // Primitive payload fields inline-parse; non-primitives
                // delegate via `apply_path`.
                let inner_match_arms = bind_idents
                    .iter()
                    .zip(field_strs.iter())
                    .zip(field_tys.iter())
                    .map(|((bi, fs), fty)| {
                        if is_known_leaf(fty) {
                            quote! {
                                #fs => {
                                    if sub_tail.is_empty() {
                                        if let ::core::result::Result::Ok(__v) = value.parse::<#fty>() {
                                            *#bi = __v;
                                        }
                                    }
                                }
                            }
                        } else {
                            quote! {
                                #fs => { #bi.apply_path(sub_tail, value); }
                            }
                        }
                    });
                let destructure = field_idents.iter().zip(bind_idents.iter()).map(|(fi, bi)| {
                    quote! { #fi: #bi }
                });
                variant_arms.push(quote! {
                    (Self::#vident { #( #destructure, )* }, #tag) => {
                        let (sub_head, sub_tail) = match tail.split_once('.') {
                            ::core::option::Option::Some((h, t)) => (h, t),
                            ::core::option::Option::None => (tail, ""),
                        };
                        match sub_head {
                            #(#inner_match_arms)*
                            _ => {}
                        }
                    }
                });
            }
        }
    }

    Ok(quote! {
        impl #ident {
            pub fn apply_path(&mut self, path: &str, value: &str) {
                // Empty path or `@kind` segment: reseat to the variant
                // named in `value`. The empty-path form lets a parent
                // dispatcher's leaf branch (`field.apply_path("", val)`)
                // do the right thing for both unit enums and tagged
                // enums; `@kind` is the explicit form when the parent
                // wants to express "reseat" without ambiguity.
                if path.is_empty() {
                    match value {
                        #(#tag_ctors)*
                        _ => {}
                    }
                    return;
                }
                let (head, tail) = match path.split_once('.') {
                    ::core::option::Option::Some((h, t)) => (h, t),
                    ::core::option::Option::None => (path, ""),
                };
                if head == "@kind" {
                    match value {
                        #(#tag_ctors)*
                        _ => {}
                    }
                    return;
                }
                match (&mut *self, head) {
                    #(#variant_arms)*
                    _ => {}
                }
            }
        }
    })
}

// ─── attr parsing ───────────────────────────────────────────────────

#[derive(Default)]
struct EditAttrs {
    skip: bool,
    rename: Option<String>,
    clamp: Option<(f64, f64)>,
    also: Vec<Ident>,
    with: Option<String>,
    index: bool,
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
                let lit: LitStr = meta.value()?.parse()?;
                out.rename = Some(lit.value());
            } else if meta.path.is_ident("clamp") {
                let content;
                syn::parenthesized!(content in meta.input);
                let lo: syn::LitFloat = content.parse()?;
                let _: syn::Token![,] = content.parse()?;
                let hi: syn::LitFloat = content.parse()?;
                out.clamp = Some((lo.base10_parse()?, hi.base10_parse()?));
            } else if meta.path.is_ident("also") {
                let lit: LitStr = meta.value()?.parse()?;
                out.also.push(syn::Ident::new(&lit.value(), lit.span()));
            } else if meta.path.is_ident("with") {
                let lit: LitStr = meta.value()?.parse()?;
                out.with = Some(lit.value());
            } else if meta.path.is_ident("index") {
                out.index = true;
            } else {
                return Err(meta.error(
                    "unknown #[edit] arg (expected `skip`, `rename`, `clamp`, `also`, `with`, or `index`)",
                ));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

fn parse_serde_rename_all(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let mut found = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") {
                let lit: LitStr = meta.value()?.parse()?;
                found = Some(lit.value());
            } else if meta.input.peek(syn::Token![=]) {
                // Sibling serde args like `tag = "type"`: consume the
                // `= <expr>` so the comma-separated parse continues.
                // Use Expr (not TokenStream) so we stop at the next
                // comma instead of swallowing the rest of the list.
                let _: syn::Token![=] = meta.input.parse()?;
                let _: syn::Expr = meta.input.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                // `bound(...)` etc. — consume the parenthesised group.
                let _content;
                syn::parenthesized!(_content in meta.input);
                let _: proc_macro2::TokenStream = _content.parse()?;
            }
            Ok(())
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

fn apply_rename(ident: &str, mode: Option<&str>) -> String {
    match mode {
        Some("kebab-case") => to_kebab(ident),
        Some("snake_case") => to_snake(ident),
        Some("lowercase") => ident.to_lowercase(),
        Some("UPPERCASE") => ident.to_uppercase(),
        Some("camelCase") => to_camel(ident, false),
        Some("PascalCase") | None | Some(_) => ident.to_string(),
    }
}

fn to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_camel(s: &str, upper: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut up = upper;
    for ch in s.chars() {
        if ch == '_' {
            up = true;
        } else if up {
            out.push(ch.to_ascii_uppercase());
            up = false;
        } else {
            out.push(ch);
        }
    }
    out
}

// ─── helpers ────────────────────────────────────────────────────────

fn clamp_apply(attrs: &EditAttrs) -> TokenStream2 {
    match attrs.clamp {
        Some((lo, hi)) => quote! { .clamp(#lo as _, #hi as _) },
        None => quote! {},
    }
}

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
        "f32" | "f64" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" |
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
    )
}
