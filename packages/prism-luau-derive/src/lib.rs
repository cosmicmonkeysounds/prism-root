//! `#[luau_expose]` — derive `mlua::UserData` impls and emit Luau type
//! stubs from a single Rust source. Phase 1 of the Luau integration
//! plan: the proc macro is the bridge between Rust's type system and
//! Luau's, so the two never drift.
//!
//! Three things come out of every `#[luau_expose]` annotation:
//!
//! 1. A `pub const LUAU_TYPE_NAME: &str` and `pub const LUAU_TYPE_DEF:
//!    &str` on the annotated type — the typed-Luau text the codegen
//!    pipeline (`prism codegen luau-types`) collects into a
//!    `.d.luau` file tree.
//! 2. An `mlua::UserData` impl with field getters (and setters when
//!    `mutable` is set) — guarded by `#[cfg(feature = "luau")]` so
//!    crates that opt out of the runtime don't pay for the dep.
//! 3. A registry entry — the host crate reflects on a slice of
//!    `(name, def)` pairs by hand. `inventory`-style registration is
//!    deliberately avoided because cdylib + WASM targets don't play
//!    well with link-time collection.
//!
//! Supported shapes (Phase 1):
//!
//! * `struct Foo { a: T1, b: T2, ... }` — every field is exposed.
//!   Nested types must themselves be `UserData` or one of the
//!   supported primitives (`bool`, `String`, the integer/float
//!   primitives). Read-only by default; opt into setters with
//!   `#[luau_expose(mutable)]`.
//! * `enum Foo { A, B, C }` — unit-only enums become a Luau string
//!   union (`type Foo = "A" | "B" | "C"`). The runtime side emits
//!   `IntoLua` / `FromLua` impls that round-trip through strings.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Data, DataEnum, DataStruct, DeriveInput, Fields, Ident, Type};

#[proc_macro_attribute]
pub fn luau_expose(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match parse_attr_args(attr.into()) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };
    let input = parse_macro_input!(item as DeriveInput);
    let expanded = match expand(&input, &args) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error(),
    };
    quote! {
        #input
        #expanded
    }
    .into()
}

#[derive(Default)]
struct ExposeArgs {
    rename: Option<String>,
    mutable: bool,
}

fn parse_attr_args(attr: TokenStream2) -> syn::Result<ExposeArgs> {
    let mut out = ExposeArgs::default();
    if attr.is_empty() {
        return Ok(out);
    }
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("rename") {
            let lit: syn::LitStr = meta.value()?.parse()?;
            out.rename = Some(lit.value());
        } else if meta.path.is_ident("mutable") {
            out.mutable = true;
        } else if meta.path.is_ident("read_only") {
            out.mutable = false;
        } else {
            return Err(meta.error("unknown #[luau_expose] arg (expected `rename`, `mutable`, or `read_only`)"));
        }
        Ok(())
    });
    syn::parse::Parser::parse2(parser, attr)?;
    Ok(out)
}

fn expand(input: &DeriveInput, args: &ExposeArgs) -> syn::Result<TokenStream2> {
    match &input.data {
        Data::Struct(s) => expand_struct(input, s, args),
        Data::Enum(e) => expand_enum(input, e, args),
        Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "#[luau_expose] does not support unions",
        )),
    }
}

fn luau_name(input: &DeriveInput, args: &ExposeArgs) -> String {
    args.rename
        .clone()
        .unwrap_or_else(|| input.ident.to_string())
}

// ───── struct expansion ─────────────────────────────────────────────

fn expand_struct(
    input: &DeriveInput,
    data: &DataStruct,
    args: &ExposeArgs,
) -> syn::Result<TokenStream2> {
    let ident = &input.ident;
    let luau_name = luau_name(input, args);

    let named = match &data.fields {
        Fields::Named(n) => n,
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "#[luau_expose] struct must have named fields",
            ))
        }
    };

    let mut field_lines = Vec::with_capacity(named.named.len());
    let mut getters = Vec::with_capacity(named.named.len());
    let mut setters = Vec::new();

    for field in &named.named {
        let fname = field.ident.as_ref().expect("named field");
        let fname_str = fname.to_string();
        let luau_ty = rust_type_to_luau(&field.ty);
        field_lines.push(format!("    {fname_str}: {luau_ty},"));

        // Getter — clone is universal across Copy + Clone + UserData.
        getters.push(quote! {
            fields.add_field_method_get(#fname_str, |_, this| Ok(this.#fname.clone()));
        });

        if args.mutable {
            let ty = &field.ty;
            setters.push(quote! {
                fields.add_field_method_set(#fname_str, |_, this, val: #ty| {
                    this.#fname = val;
                    Ok(())
                });
            });
        }
    }

    let body = field_lines.join("\n");
    let type_def = format!("export type {luau_name} = {{\n{body}\n}}");

    let userdata_impl = quote! {
        #[cfg(feature = "luau")]
        impl ::mlua::UserData for #ident {
            fn add_fields<F: ::mlua::UserDataFields<Self>>(fields: &mut F) {
                #(#getters)*
                #(#setters)*
            }
        }
    };

    Ok(quote! {
        impl #ident {
            pub const LUAU_TYPE_NAME: &'static str = #luau_name;
            pub const LUAU_TYPE_DEF: &'static str = #type_def;
        }
        #userdata_impl
    })
}

// ───── enum expansion ───────────────────────────────────────────────

fn expand_enum(
    input: &DeriveInput,
    data: &DataEnum,
    args: &ExposeArgs,
) -> syn::Result<TokenStream2> {
    let ident = &input.ident;
    let luau_name = luau_name(input, args);

    if data.variants.iter().any(|v| !matches!(v.fields, Fields::Unit)) {
        return Err(syn::Error::new_spanned(
            input,
            "#[luau_expose] enums currently only support unit variants (Phase 1). Tagged unions land in Phase 2.",
        ));
    }

    let variant_idents: Vec<&Ident> = data.variants.iter().map(|v| &v.ident).collect();
    let variant_strs: Vec<String> = variant_idents.iter().map(|i| i.to_string()).collect();

    let union = variant_strs
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(" | ");
    let type_def = format!("export type {luau_name} = {union}");

    // `IntoLua` / `FromLua` rather than UserData: enums round-trip as
    // strings on the Luau side, which is the natural shape for a
    // tagged union and matches the type stub.
    let into_arms = variant_idents.iter().zip(variant_strs.iter()).map(|(v, s)| {
        quote! { #ident::#v => #s, }
    });
    let from_arms = variant_idents.iter().zip(variant_strs.iter()).map(|(v, s)| {
        quote! { #s => Ok(#ident::#v), }
    });

    let lua_impls = quote! {
        #[cfg(feature = "luau")]
        impl ::mlua::IntoLua for #ident {
            fn into_lua(self, lua: &::mlua::Lua) -> ::mlua::Result<::mlua::Value> {
                let s: &'static str = match self { #(#into_arms)* };
                Ok(::mlua::Value::String(lua.create_string(s)?))
            }
        }
        #[cfg(feature = "luau")]
        impl ::mlua::FromLua for #ident {
            fn from_lua(value: ::mlua::Value, _: &::mlua::Lua) -> ::mlua::Result<Self> {
                let s = match &value {
                    ::mlua::Value::String(s) => s.to_str()?.to_string(),
                    _ => return Err(::mlua::Error::FromLuaConversionError {
                        from: value.type_name(),
                        to: stringify!(#ident).to_string(),
                        message: Some(format!("expected one of {:?}", [#(#variant_strs),*])),
                    }),
                };
                match s.as_str() {
                    #(#from_arms)*
                    other => Err(::mlua::Error::FromLuaConversionError {
                        from: "string",
                        to: stringify!(#ident).to_string(),
                        message: Some(format!("unknown variant `{}`", other)),
                    }),
                }
            }
        }
    };

    Ok(quote! {
        impl #ident {
            pub const LUAU_TYPE_NAME: &'static str = #luau_name;
            pub const LUAU_TYPE_DEF: &'static str = #type_def;
        }
        #lua_impls
    })
}

// ───── type mapping ─────────────────────────────────────────────────

/// Map a Rust type to the Luau type that the macro will emit into the
/// `.d.luau` stub. Primitives collapse to `number` / `boolean` /
/// `string`; everything else is referenced by its leaf identifier and
/// expected to itself be `#[luau_expose]`-annotated.
fn rust_type_to_luau(ty: &Type) -> String {
    match ty {
        Type::Path(tp) => {
            let last = tp.path.segments.last();
            let Some(seg) = last else {
                return "any".to_string();
            };
            let name = seg.ident.to_string();
            match name.as_str() {
                "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32"
                | "i64" | "i128" | "isize" | "f32" | "f64" => "number".to_string(),
                "bool" => "boolean".to_string(),
                "String" | "str" => "string".to_string(),
                "Option" => {
                    if let Some(inner) = first_generic_type(seg) {
                        format!("{}?", rust_type_to_luau(inner))
                    } else {
                        "any?".to_string()
                    }
                }
                "Vec" => {
                    if let Some(inner) = first_generic_type(seg) {
                        format!("{{{}}}", rust_type_to_luau(inner))
                    } else {
                        "{any}".to_string()
                    }
                }
                _ => name,
            }
        }
        Type::Reference(r) => rust_type_to_luau(&r.elem),
        _ => "any".to_string(),
    }
}

fn first_generic_type(seg: &syn::PathSegment) -> Option<&Type> {
    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
        for a in &args.args {
            if let syn::GenericArgument::Type(t) = a {
                return Some(t);
            }
        }
    }
    None
}
