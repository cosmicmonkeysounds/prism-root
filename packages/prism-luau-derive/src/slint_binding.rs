//! `#[derive(SlintBinding)]` — emits `bind_to(&Target)` /
//! `pull_from(&Target)` impls that shuttle every named field through
//! the Slint generated trait getters/setters.
//!
//! The struct attribute `#[slint(global = "...")]` declares the Slint
//! handle the bindings target. The value is parsed as a Rust type, so
//! both root-component shapes work directly:
//!
//! ```ignore
//! #[slint(global = "AppWindow")]      // root ComponentHandle, &AppWindow
//! #[slint(global = "AppGlobal<'_>")]  // generated global borrow type
//! ```
//!
//! Field names map to Slint's generated `set_<field>` / `get_<field>`
//! accessors. Slint maps Slint-side `foo-bar` props to `set_foo_bar` Rust
//! fns, so the Rust field name is used verbatim — match it on the Slint
//! side.
//!
//! Per-field attributes:
//!  * `#[slint(skip)]` — drop the field from both `bind_to` and `pull_from`.
//!  * `#[slint(rename = "other")]` — call `set_other` / `get_other` instead
//!    of `set_<field>` / `get_<field>` (lets the Rust field name diverge
//!    from the Slint property name).
//!
//! Struct-level direction flags:
//!  * `#[slint(push_only)]` — emit only `bind_to`, skip `pull_from`. Use
//!    when the target properties are Slint `in` (no generated getter).
//!  * `#[slint(pull_only)]` — emit only `pull_from`, skip `bind_to`. Use
//!    for read-only `out` properties.
//!
//! Type translation is identity — fields' Rust types must already match
//! the generated `set_<field>` / `get_<field>` signatures (typically
//! `slint::SharedString` for strings, `i32` / `bool` / `f32` for primitives,
//! or generated Slint structs). The macro inserts `.clone().into()` on the
//! push side and `.into()` on the pull side, which is enough for the
//! `String ↔ SharedString` round-trip and a no-op for primitives.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields};

#[derive(Default)]
struct BindingArgs {
    global: Option<syn::Type>,
    push_only: bool,
    pull_only: bool,
}

#[derive(Default)]
struct FieldArgs {
    skip: bool,
    rename: Option<String>,
}

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let args = parse_struct_args(input)?;
    let Some(global_ty) = args.global else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(SlintBinding)] requires `#[slint(global = \"...\")]`",
        ));
    };
    if args.push_only && args.pull_only {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(SlintBinding)] cannot be both push_only and pull_only",
        ));
    }

    let ident = &input.ident;
    let Data::Struct(s) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(SlintBinding)] only supports structs",
        ));
    };
    let Fields::Named(named) = &s.fields else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(SlintBinding)] requires named fields",
        ));
    };

    let mut bind_lines = Vec::new();
    let mut pull_lines = Vec::new();
    for f in &named.named {
        let fa = parse_field_args(f)?;
        if fa.skip {
            continue;
        }
        let fname = f.ident.as_ref().expect("named");
        let prop = fa.rename.unwrap_or_else(|| fname.to_string());
        let setter = format_ident!("set_{}", prop);
        let getter = format_ident!("get_{}", prop);
        bind_lines.push(quote! {
            handle.#setter(self.#fname.clone().into());
        });
        pull_lines.push(quote! {
            self.#fname = handle.#getter().into();
        });
    }

    let bind_method = (!args.pull_only).then(|| {
        quote! {
            pub fn bind_to(&self, handle: &#global_ty) {
                #(#bind_lines)*
            }
        }
    });
    let pull_method = (!args.push_only).then(|| {
        quote! {
            pub fn pull_from(&mut self, handle: &#global_ty) {
                #(#pull_lines)*
            }
        }
    });

    Ok(quote! {
        impl #ident {
            #bind_method
            #pull_method
        }
    })
}

fn parse_struct_args(input: &DeriveInput) -> syn::Result<BindingArgs> {
    let mut out = BindingArgs::default();
    for attr in &input.attrs {
        if !attr.path().is_ident("slint") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("global") || meta.path.is_ident("target") {
                let s: syn::LitStr = meta.value()?.parse()?;
                out.global = Some(syn::parse_str::<syn::Type>(&s.value())?);
            } else if meta.path.is_ident("push_only") {
                out.push_only = true;
            } else if meta.path.is_ident("pull_only") {
                out.pull_only = true;
            } else {
                return Err(meta.error("unknown #[slint] arg"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

fn parse_field_args(f: &syn::Field) -> syn::Result<FieldArgs> {
    let mut out = FieldArgs::default();
    for attr in &f.attrs {
        if !attr.path().is_ident("slint") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                out.skip = true;
            } else if meta.path.is_ident("rename") {
                let s: syn::LitStr = meta.value()?.parse()?;
                out.rename = Some(s.value());
            } else {
                return Err(meta.error("unknown #[slint] field arg"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}
