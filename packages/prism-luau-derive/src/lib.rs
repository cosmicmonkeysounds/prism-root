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
//!    Suppress with `#[luau_expose(manual_impl)]` when the host
//!    crate hand-writes the `UserData` (the stateful subsystems —
//!    `ObjectRegistry`, `ConfigModel`, `GraphObject` — go down this
//!    path because their Lua surface is a method API, not a flat
//!    field projection).
//! 3. A registry entry — the host crate reflects on a slice of
//!    `(name, def)` pairs by hand. `inventory`-style registration is
//!    deliberately avoided because cdylib + WASM targets don't play
//!    well with link-time collection.
//!
//! Supported shapes:
//!
//! * `struct Foo { a: T1, b: T2, ... }` — every field is exposed.
//!   Fields tagged `#[luau_skip]` are dropped from both the type
//!   stub and the generated getters/setters; combine with
//!   `manual_impl` when a hand-written `UserData` will provide the
//!   missing surface. Read-only by default; opt into setters with
//!   `#[luau_expose(mutable)]`.
//! * `enum Foo { A, B, C }` — unit-only enums become a Luau string
//!   union (`type Foo = "A" | "B" | "C"`). The runtime side emits
//!   `IntoLua` / `FromLua` impls that round-trip through strings.
//! * `enum Foo { A, B(Inner), C { x: i32 } }` — tagged unions become a
//!   Luau table union with the literal discriminator key `tag` (we
//!   intentionally don't honour `#[serde(tag = "...")]` — scripts see
//!   one shape regardless of how the type serialises elsewhere).
//!   Single-tuple variants project the inner value under `value`;
//!   struct-like variants flatten their fields into the table.
//!   Multi-tuple variants are rejected.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Data, DataEnum, DataStruct, DeriveInput, Field, Fields, Ident, Type};

mod daemon_command;
mod daemon_fn;
mod daemon_module;
mod editable;
mod peer_fn;
mod prism_block;
mod prism_field;
mod relay_fn;
mod visual_node;

/// Derive a `prism_builder::Block` impl from a struct exposing
/// `schema()` and `template(props, children)` associated functions.
/// Requires `#[block(id = "...")]` on the struct.
#[proc_macro_derive(PrismBlock, attributes(block))]
pub fn derive_prism_block(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match prism_block::expand(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Derive `field_specs() -> Vec<FieldSpec>` from a struct of typed
/// props. See `prism_field` module for details.
#[proc_macro_derive(PrismField, attributes(field))]
pub fn derive_prism_field(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match prism_field::expand(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Annotate a free Rust function as a visual scripting node. The macro
/// emits a `pub const <FN_NAME>_NODE_DEF: NodeKindDef` capturing label,
/// category, and a Luau-snippet template derived from the function
/// signature.
#[proc_macro_attribute]
pub fn visual_node(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2: TokenStream2 = attr.into();
    let item2: TokenStream2 = item.into();
    match visual_node::expand(attr2, item2) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Annotate a typed daemon command handler. Desugars
/// `fn name(req)` or `fn name(&state, req) -> Result<Resp, E>` into
/// the original function plus a `register_<name>` helper that wires
/// the typed handler through `CommandRegistry::register_typed`.
/// Required: `id = "x.y"`. Optional: `permission = User|Dev|Admin`
/// (default `Dev`).
#[proc_macro_attribute]
pub fn daemon_command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2: TokenStream2 = attr.into();
    let item2: TokenStream2 = item.into();
    match daemon_command::expand(attr2, item2) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Phase 6 scaffold of the Dioxus-inspired reactive overhaul
/// (`docs/dev/dioxus-inspiration.md`). End-state: a single
/// `async fn name(args) -> Result<T, E>` annotated with
/// `#[daemon_fn(id = "…")]` emits both the server-side handler and
/// the typed client stub that serializes over `interprocess` +
/// `postcard`. Today the macro parses the syntax and validates the
/// signature (must be `async`, must return `Result<_, _>`) but only
/// emits the original function plus a tooling marker const.
/// Required: `id = "…"`. Optional: `permission = User|Dev|Admin`.
#[proc_macro_attribute]
pub fn daemon_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2: TokenStream2 = attr.into();
    let item2: TokenStream2 = item.into();
    match daemon_fn::expand(attr2, item2) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// `#[relay_fn(id = "…")]` — sister of `#[daemon_fn]` pointed at the
/// WebSocket relay transport. §3.4 of `docs/dev/dioxus-inspiration.md`.
///
/// Same shape: a sync function returning `Result<T, E>` becomes an
/// in-place handler + a `<name>_client(invoker: &dyn RelayInvoker, args)`
/// stub that serialises args, calls
/// `RelayInvoker::invoke`, and deserialises the response into
/// `Result<T, RelayFnError<E>>`. Different destination (relay vs.
/// daemon sidecar), same call-site ergonomics. The macro is intentionally
/// transport-agnostic — pair it with a concrete `RelayInvoker` impl
/// (the relay client's WebSocket transport in production,
/// `MockRelayInvoker` in tests) at the call site.
#[proc_macro_attribute]
pub fn relay_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2: TokenStream2 = attr.into();
    let item2: TokenStream2 = item.into();
    match relay_fn::expand(attr2, item2) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// `#[peer_fn(id = "…")]` — completes the §3.4 macro family pointed at
/// the WebRTC data-channel transport. One declaration emits the original
/// function plus a `<name>_client(invoker: &dyn PeerInvoker, args)` stub
/// that returns `Result<T, PeerFnError<E>>`. Distinct from `#[relay_fn]`
/// because the destinations are operationally different — a peer call
/// reaches one specific peer over a data channel, a relay call traverses
/// the relay's capability-gated routing path.
#[proc_macro_attribute]
pub fn peer_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2: TokenStream2 = attr.into();
    let item2: TokenStream2 = item.into();
    match peer_fn::expand(attr2, item2) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Annotate a daemon module struct. Emits a `DaemonModule` impl whose
/// `install()` body acquires shared state (per `slot = …, default = …`
/// or `state = …`, both optional) and calls each `register_<cmd>`
/// sibling listed in `commands(…)`. Required: `id = "…"`.
#[proc_macro_attribute]
pub fn daemon_module(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2: TokenStream2 = attr.into();
    let item2: TokenStream2 = item.into();
    match daemon_module::expand(attr2, item2) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Derive `apply_field(&mut self, key: &str, value: &str)` from a struct
/// of typed fields. Dispatches stringly-typed property-panel edits to
/// the matching field, parsing `value` based on the field's Rust type.
/// See the `editable` module for the supported type → parser mapping
/// and per-field attributes (`skip`, `rename`, `clamp`).
#[proc_macro_derive(Editable, attributes(edit))]
pub fn derive_editable(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    match editable::expand(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn luau_expose(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match parse_attr_args(attr.into()) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };
    let mut input = parse_macro_input!(item as DeriveInput);
    let expanded = match expand(&input, &args) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error(),
    };
    // Strip `#[luau_skip]` from the surviving struct so rustc doesn't
    // complain about an unknown attribute.
    if let Data::Struct(s) = &mut input.data {
        if let Fields::Named(named) = &mut s.fields {
            for f in named.named.iter_mut() {
                f.attrs.retain(|a| !a.path().is_ident("luau_skip"));
            }
        }
    }
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
    /// When true, emit only the `LUAU_TYPE_NAME` / `LUAU_TYPE_DEF`
    /// constants. Skip the `UserData` / `IntoLua` / `FromLua`
    /// generation so the host crate can hand-write the Lua surface.
    manual_impl: bool,
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
        } else if meta.path.is_ident("manual_impl") {
            out.manual_impl = true;
        } else {
            return Err(meta.error(
                "unknown #[luau_expose] arg (expected `rename`, `mutable`, `read_only`, or `manual_impl`)",
            ));
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

fn field_is_skipped(f: &Field) -> bool {
    f.attrs.iter().any(|a| a.path().is_ident("luau_skip"))
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
        if field_is_skipped(field) {
            continue;
        }
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

    let userdata_impl = if args.manual_impl {
        quote! {}
    } else {
        quote! {
            #[cfg(feature = "luau")]
            impl ::mlua::UserData for #ident {
                fn add_fields<F: ::mlua::UserDataFields<Self>>(fields: &mut F) {
                    #(#getters)*
                    #(#setters)*
                }
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
    let all_unit = data
        .variants
        .iter()
        .all(|v| matches!(v.fields, Fields::Unit));
    if !all_unit {
        return expand_tagged_enum(input, data, args);
    }

    let ident = &input.ident;
    let luau_name = luau_name(input, args);

    let variant_idents: Vec<&Ident> = data.variants.iter().map(|v| &v.ident).collect();
    let variant_strs: Vec<String> = variant_idents.iter().map(|i| i.to_string()).collect();

    let union = variant_strs
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(" | ");
    let type_def = format!("export type {luau_name} = {union}");

    let lua_impls = if args.manual_impl {
        quote! {}
    } else {
        // `IntoLua` / `FromLua` rather than UserData: enums round-trip
        // as strings on the Luau side, which is the natural shape for
        // a tagged union and matches the type stub.
        let into_arms = variant_idents
            .iter()
            .zip(variant_strs.iter())
            .map(|(v, s)| {
                quote! { #ident::#v => #s, }
            });
        let from_arms = variant_idents
            .iter()
            .zip(variant_strs.iter())
            .map(|(v, s)| {
                quote! { #s => Ok(#ident::#v), }
            });
        quote! {
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

// ───── tagged-union enum expansion (Phase 1.1) ──────────────────────

/// Expand an enum with at least one non-unit variant into a Luau
/// tagged-union type. The discriminator is always the literal string
/// `tag` (we deliberately don't honour `#[serde(tag = "...")]` —
/// scripts always see the same shape regardless of how the type
/// serialises elsewhere). Unit variants stay as `{ tag: "X" }`,
/// single-tuple variants project into `{ tag: "X", value: T }`, and
/// struct-like variants flatten their fields into the table.
///
/// Multi-tuple variants are rejected — there's no obvious table
/// projection and no use case in the current codebase.
fn expand_tagged_enum(
    input: &DeriveInput,
    data: &DataEnum,
    args: &ExposeArgs,
) -> syn::Result<TokenStream2> {
    let ident = &input.ident;
    let luau_name = luau_name(input, args);

    let mut stub_arms: Vec<String> = Vec::with_capacity(data.variants.len());
    let mut into_arms: Vec<TokenStream2> = Vec::with_capacity(data.variants.len());
    let mut from_arms: Vec<TokenStream2> = Vec::with_capacity(data.variants.len());
    let mut variant_strs: Vec<String> = Vec::with_capacity(data.variants.len());

    for variant in &data.variants {
        let v_ident = &variant.ident;
        let v_str = v_ident.to_string();
        variant_strs.push(v_str.clone());

        match &variant.fields {
            Fields::Unit => {
                stub_arms.push(format!("    {{ tag: \"{v_str}\" }}"));
                into_arms.push(quote! {
                    #ident::#v_ident => {
                        let t = lua.create_table()?;
                        t.set("tag", #v_str)?;
                        Ok(::mlua::Value::Table(t))
                    }
                });
                from_arms.push(quote! {
                    #v_str => Ok(#ident::#v_ident),
                });
            }
            Fields::Unnamed(unnamed) => {
                if unnamed.unnamed.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "#[luau_expose] tagged-union variants must have 0 or 1 unnamed fields, or named fields",
                    ));
                }
                let inner_ty = &unnamed.unnamed.first().unwrap().ty;
                let luau_ty = rust_type_to_luau(inner_ty);
                stub_arms.push(format!("    {{ tag: \"{v_str}\", value: {luau_ty} }}"));
                into_arms.push(quote! {
                    #ident::#v_ident(inner) => {
                        let t = lua.create_table()?;
                        t.set("tag", #v_str)?;
                        t.set("value", ::mlua::IntoLua::into_lua(inner, lua)?)?;
                        Ok(::mlua::Value::Table(t))
                    }
                });
                from_arms.push(quote! {
                    #v_str => {
                        let raw: ::mlua::Value = table.get("value")?;
                        let inner: #inner_ty = ::mlua::FromLua::from_lua(raw, lua)?;
                        Ok(#ident::#v_ident(inner))
                    }
                });
            }
            Fields::Named(named) => {
                let mut field_lines: Vec<String> = Vec::with_capacity(named.named.len() + 1);
                let mut into_field_sets: Vec<TokenStream2> = Vec::with_capacity(named.named.len());
                let mut from_field_reads: Vec<TokenStream2> = Vec::with_capacity(named.named.len());
                let mut field_idents: Vec<&Ident> = Vec::with_capacity(named.named.len());

                for f in &named.named {
                    let fname = f.ident.as_ref().expect("named");
                    let fname_str = fname.to_string();
                    let luau_ty = rust_type_to_luau(&f.ty);
                    field_lines.push(format!("{fname_str}: {luau_ty}"));
                    let f_ty = &f.ty;
                    into_field_sets.push(quote! {
                        t.set(#fname_str, ::mlua::IntoLua::into_lua(#fname, lua)?)?;
                    });
                    from_field_reads.push(quote! {
                        let #fname: #f_ty = {
                            let raw: ::mlua::Value = table.get(#fname_str)?;
                            ::mlua::FromLua::from_lua(raw, lua)?
                        };
                    });
                    field_idents.push(fname);
                }

                let body = field_lines.join(", ");
                stub_arms.push(format!("    {{ tag: \"{v_str}\", {body} }}"));
                into_arms.push(quote! {
                    #ident::#v_ident { #(#field_idents),* } => {
                        let t = lua.create_table()?;
                        t.set("tag", #v_str)?;
                        #(#into_field_sets)*
                        Ok(::mlua::Value::Table(t))
                    }
                });
                from_arms.push(quote! {
                    #v_str => {
                        #(#from_field_reads)*
                        Ok(#ident::#v_ident { #(#field_idents),* })
                    }
                });
            }
        }
    }

    let type_def = format!("export type {luau_name} =\n{}", stub_arms.join("\n  |"));

    let lua_impls = if args.manual_impl {
        quote! {}
    } else {
        quote! {
            #[cfg(feature = "luau")]
            impl ::mlua::IntoLua for #ident {
                fn into_lua(self, lua: &::mlua::Lua) -> ::mlua::Result<::mlua::Value> {
                    match self {
                        #(#into_arms)*
                    }
                }
            }
            #[cfg(feature = "luau")]
            impl ::mlua::FromLua for #ident {
                fn from_lua(value: ::mlua::Value, lua: &::mlua::Lua) -> ::mlua::Result<Self> {
                    let table = match value {
                        ::mlua::Value::Table(t) => t,
                        other => {
                            return Err(::mlua::Error::FromLuaConversionError {
                                from: other.type_name(),
                                to: stringify!(#ident).to_string(),
                                message: Some("expected table with `tag` field".to_string()),
                            });
                        }
                    };
                    let tag: String = table.get("tag")?;
                    match tag.as_str() {
                        #(#from_arms)*
                        other => Err(::mlua::Error::FromLuaConversionError {
                            from: "string",
                            to: stringify!(#ident).to_string(),
                            message: Some(format!(
                                "unknown variant tag `{}` (expected one of {:?})",
                                other,
                                [#(#variant_strs),*],
                            )),
                        }),
                    }
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
/// `string`; collection types map to typed Luau tables; everything
/// else is referenced by its leaf identifier and expected to itself
/// be `#[luau_expose]`-annotated.
pub(crate) fn rust_type_to_luau(ty: &Type) -> String {
    match ty {
        Type::Path(tp) => {
            let last = tp.path.segments.last();
            let Some(seg) = last else {
                return "any".to_string();
            };
            let name = seg.ident.to_string();
            match name.as_str() {
                "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64"
                | "i128" | "isize" | "f32" | "f64" => "number".to_string(),
                "bool" => "boolean".to_string(),
                "String" | "str" => "string".to_string(),
                // `chrono::DateTime<Tz>` round-trips through ISO-8601
                // strings on the Luau side. Naive dates fall through
                // here too — the host runtime is responsible for the
                // string conversion when it builds the userdata.
                "DateTime" | "NaiveDate" | "NaiveDateTime" => "string".to_string(),
                // `serde_json::Value` is the universal escape hatch
                // for opaque payloads (entity `data`, setting values).
                "Value" | "JsonValue" => "any".to_string(),
                "Option" => {
                    if let Some(inner) = first_generic_type(seg) {
                        format!("{}?", rust_type_to_luau(inner))
                    } else {
                        "any?".to_string()
                    }
                }
                "Vec" | "VecDeque" => {
                    if let Some(inner) = first_generic_type(seg) {
                        format!("{{{}}}", rust_type_to_luau(inner))
                    } else {
                        "{any}".to_string()
                    }
                }
                "BTreeMap" | "HashMap" | "IndexMap" | "Map" => {
                    let (k, v) = first_two_generic_types(seg);
                    let key = k.map(rust_type_to_luau).unwrap_or_else(|| "string".into());
                    let val = v.map(rust_type_to_luau).unwrap_or_else(|| "any".into());
                    format!("{{ [{key}]: {val} }}")
                }
                _ => name,
            }
        }
        Type::Reference(r) => rust_type_to_luau(&r.elem),
        _ => "any".to_string(),
    }
}

pub(crate) fn first_generic_type(seg: &syn::PathSegment) -> Option<&Type> {
    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
        for a in &args.args {
            if let syn::GenericArgument::Type(t) = a {
                return Some(t);
            }
        }
    }
    None
}

fn first_two_generic_types(seg: &syn::PathSegment) -> (Option<&Type>, Option<&Type>) {
    let mut out: (Option<&Type>, Option<&Type>) = (None, None);
    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
        for a in &args.args {
            if let syn::GenericArgument::Type(t) = a {
                if out.0.is_none() {
                    out.0 = Some(t);
                } else if out.1.is_none() {
                    out.1 = Some(t);
                    break;
                }
            }
        }
    }
    out
}
