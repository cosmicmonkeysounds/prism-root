//! `#[derive(SlintBinding)]` — emits `bind_to(&Global)` /
//! `pull_from(&Global)` impls that shuttle every named field through
//! the Slint generated trait getters/setters.
//!
//! The struct attribute `#[slint(global = "AppGlobal")]` declares which
//! Slint global the bindings target. Field names are converted to
//! kebab-then-snake via Slint's auto-generated `set_<field>` /
//! `get_<field>` accessors — Slint maps `foo-bar` Slint props to
//! `set_foo_bar` Rust fns, so we use the Rust field name verbatim and
//! rely on the user matching it on the Slint side.
//!
//! Type translation is currently identity — the macro assumes the
//! field's Rust type already matches what Slint's getter / setter
//! expects (typically `slint::SharedString` for strings, `i32` /
//! `bool` / `f32` for primitives, or generated Slint structs).
//! Conversions live at the call site, not in the derive.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields};

#[derive(Default)]
struct BindingArgs {
    global: Option<String>,
}

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let args = parse_struct_args(input)?;
    let Some(global_name) = args.global else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(SlintBinding)] requires `#[slint(global = \"...\")]`",
        ));
    };
    let global_ident = format_ident!("{}", global_name);

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
        let fname = f.ident.as_ref().expect("named");
        let setter = format_ident!("set_{}", fname);
        let getter = format_ident!("get_{}", fname);
        bind_lines.push(quote! {
            handle.#setter(self.#fname.clone().into());
        });
        pull_lines.push(quote! {
            self.#fname = handle.#getter().into();
        });
    }

    Ok(quote! {
        impl #ident {
            pub fn bind_to(&self, handle: &#global_ident<'_>) {
                #(#bind_lines)*
            }
            pub fn pull_from(&mut self, handle: &#global_ident<'_>) {
                #(#pull_lines)*
            }
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
            if meta.path.is_ident("global") {
                let s: syn::LitStr = meta.value()?.parse()?;
                out.global = Some(s.value());
            } else {
                return Err(meta.error("unknown #[slint] arg"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}
