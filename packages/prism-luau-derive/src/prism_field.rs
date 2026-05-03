//! `#[derive(PrismField)]` — emits `field_specs() -> Vec<FieldSpec>`
//! from a typed prop struct. Built-in `FieldKind` variants are picked
//! from the field's Rust type; richer attributes (`label`, `default`,
//! `multiline`, `select`, `required`, `group`, `help`, `min`, `max`)
//! refine the spec.
//!
//! The generated code references `prism_core::widget::field::*` so the
//! consuming crate must depend on `prism-core`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Field, Fields, Lit, Meta, Type};

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let ident = &input.ident;
    let Data::Struct(s) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(PrismField)] only supports structs with named fields",
        ));
    };
    let Fields::Named(named) = &s.fields else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(PrismField)] requires named fields",
        ));
    };

    let mut field_lines = Vec::new();
    for field in &named.named {
        let line = build_field_spec(field)?;
        field_lines.push(line);
    }

    Ok(quote! {
        impl #ident {
            pub fn field_specs() -> ::std::vec::Vec<::prism_core::widget::field::FieldSpec> {
                let mut __out: ::std::vec::Vec<::prism_core::widget::field::FieldSpec> = ::std::vec::Vec::new();
                #(#field_lines)*
                __out
            }
        }
    })
}

#[derive(Default)]
struct FieldAttrs {
    label: Option<String>,
    help: Option<String>,
    group: Option<String>,
    required: bool,
    multiline: bool,
    default_lit: Option<Lit>,
    select: Option<Vec<(String, String)>>,
    min: Option<f64>,
    max: Option<f64>,
}

fn build_field_spec(field: &Field) -> syn::Result<TokenStream2> {
    let fname = field
        .ident
        .as_ref()
        .ok_or_else(|| syn::Error::new_spanned(field, "expected named field"))?;
    let key = fname.to_string();
    let attrs = parse_field_attrs(field)?;
    let label = attrs
        .label
        .clone()
        .unwrap_or_else(|| humanize(&key));

    let kind_expr = field_kind_expr(&field.ty, &attrs)?;

    let default_expr = attrs.default_lit.as_ref().map(lit_to_value_expr);

    let mut chain = quote! {
        ::prism_core::widget::field::FieldSpec::new(#key, #label, #kind_expr)
    };
    if let Some(def) = default_expr {
        chain = quote! { #chain.with_default(#def) };
    } else if attrs.select.is_some() {
        // Select default = first option (FieldSpec::select handles
        // this when constructed via the helper, but we built kind via
        // raw `Select(opts)` to allow defaults to override; keep
        // explicit default-from-first behavior below).
    }
    if attrs.required {
        chain = quote! { #chain.required() };
    }
    if let Some(g) = &attrs.group {
        chain = quote! { #chain.group(#g) };
    }
    if let Some(h) = &attrs.help {
        chain = quote! { #chain.with_help(#h) };
    }

    Ok(quote! {
        __out.push(#chain);
    })
}

fn parse_field_attrs(field: &Field) -> syn::Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in &field.attrs {
        if !attr.path().is_ident("field") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("label") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.label = Some(lit.value());
            } else if meta.path.is_ident("help") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.help = Some(lit.value());
            } else if meta.path.is_ident("group") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                out.group = Some(lit.value());
            } else if meta.path.is_ident("required") {
                out.required = true;
            } else if meta.path.is_ident("multiline") {
                out.multiline = true;
            } else if meta.path.is_ident("default") {
                let lit: Lit = meta.value()?.parse()?;
                out.default_lit = Some(lit);
            } else if meta.path.is_ident("min") {
                let lit: syn::LitFloat = meta.value()?.parse()?;
                out.min = Some(lit.base10_parse()?);
            } else if meta.path.is_ident("max") {
                let lit: syn::LitFloat = meta.value()?.parse()?;
                out.max = Some(lit.base10_parse()?);
            } else if meta.path.is_ident("select") {
                // Two accepted shapes:
                //   select("a", "b", "c")
                //   select = "a,b,c"
                if meta.input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let mut opts = Vec::new();
                    while !content.is_empty() {
                        let s: syn::LitStr = content.parse()?;
                        let v = s.value();
                        let label = humanize(&v);
                        opts.push((v, label));
                        if !content.is_empty() {
                            let _: syn::Token![,] = content.parse()?;
                        }
                    }
                    out.select = Some(opts);
                } else {
                    let s: syn::LitStr = meta.value()?.parse()?;
                    let opts = s
                        .value()
                        .split(',')
                        .map(|p| {
                            let v = p.trim().to_string();
                            let l = humanize(&v);
                            (v, l)
                        })
                        .collect();
                    out.select = Some(opts);
                }
            } else {
                return Err(meta.error("unknown #[field] arg"));
            }
            Ok(())
        })?;
        // Suppress unused-import warnings on Meta in this fn.
        let _ = std::marker::PhantomData::<Meta>;
    }
    Ok(out)
}

fn field_kind_expr(ty: &Type, attrs: &FieldAttrs) -> syn::Result<TokenStream2> {
    if let Some(opts) = &attrs.select {
        let pairs = opts.iter().map(|(v, l)| {
            quote! { ::prism_core::widget::field::SelectOption::new(#v, #l) }
        });
        return Ok(quote! {
            ::prism_core::widget::field::FieldKind::Select(::std::vec![#(#pairs),*])
        });
    }
    let name = type_leaf_name(ty);
    let bounds_expr = bounds_expr(attrs);
    Ok(match name.as_deref() {
        Some("String" | "str") => {
            if attrs.multiline {
                quote! { ::prism_core::widget::field::FieldKind::TextArea }
            } else {
                quote! { ::prism_core::widget::field::FieldKind::Text }
            }
        }
        Some("bool") => quote! { ::prism_core::widget::field::FieldKind::Boolean },
        Some("f32" | "f64") => {
            quote! { ::prism_core::widget::field::FieldKind::Number(#bounds_expr) }
        }
        Some("i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
        | "u128" | "usize") => {
            quote! { ::prism_core::widget::field::FieldKind::Integer(#bounds_expr) }
        }
        _ => {
            return Err(syn::Error::new_spanned(
                ty,
                "#[derive(PrismField)] cannot infer FieldKind for this type — supported: String, bool, f32/f64, integer types, or use `#[field(select = [...])]`",
            ));
        }
    })
}

fn bounds_expr(attrs: &FieldAttrs) -> TokenStream2 {
    let min_tok = match attrs.min {
        Some(v) => quote! { ::core::option::Option::Some(#v) },
        None => quote! { ::core::option::Option::None },
    };
    let max_tok = match attrs.max {
        Some(v) => quote! { ::core::option::Option::Some(#v) },
        None => quote! { ::core::option::Option::None },
    };
    quote! {
        ::prism_core::widget::field::NumericBounds { min: #min_tok, max: #max_tok }
    }
}

fn lit_to_value_expr(lit: &Lit) -> TokenStream2 {
    match lit {
        Lit::Str(s) => {
            let v = s.value();
            quote! { ::serde_json::Value::String(#v.to_string()) }
        }
        Lit::Bool(b) => {
            let v = b.value;
            quote! { ::serde_json::Value::Bool(#v) }
        }
        Lit::Int(i) => {
            let v: i64 = i.base10_parse().unwrap_or(0);
            quote! { ::serde_json::Value::from(#v) }
        }
        Lit::Float(f) => {
            let v: f64 = f.base10_parse().unwrap_or(0.0);
            quote! { ::serde_json::Value::from(#v) }
        }
        _ => quote! { ::serde_json::Value::Null },
    }
}

fn type_leaf_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(r) => type_leaf_name(&r.elem),
        _ => None,
    }
}

fn humanize(snake: &str) -> String {
    let mut out = String::new();
    for (i, word) in snake.split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}
