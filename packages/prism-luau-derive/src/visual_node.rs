//! `#[visual_node(category = "...", label = "...")]` — annotates a free
//! Rust function as a visual scripting node. Emits a sibling const
//! `pub const <FN_NAME_UPPER>_NODE_DEF: NodeKindDef` plus a Luau
//! snippet template inferred from the signature.
//!
//! This is the catalog wiring half — runtime semantics still come from
//! a graph compiler, but the palette entry, port shapes, and a default
//! Luau emitter expression all derive from a single Rust source.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{ItemFn, Lit, ReturnType, Type};

#[derive(Default)]
struct VisualNodeArgs {
    category: Option<String>,
    label: Option<String>,
    luau: Option<String>,
}

pub fn expand(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args = parse_args(attr)?;
    let func: ItemFn = syn::parse2(item)?;

    let fn_ident = &func.sig.ident;
    let fn_name = fn_ident.to_string();
    let category = args.category.clone().unwrap_or_else(|| "General".into());
    let label = args.label.clone().unwrap_or_else(|| pascal(&fn_name));

    let mut input_ports = Vec::new();
    for arg in &func.sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            let pname = match pt.pat.as_ref() {
                syn::Pat::Ident(pi) => pi.ident.to_string(),
                _ => continue,
            };
            let dt = data_type_for(&pt.ty);
            input_ports.push((pname, dt));
        }
    }

    let output_dt = match &func.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(data_type_for(ty)),
    };

    let luau_template = args.luau.unwrap_or_else(|| {
        let argnames: Vec<&str> = input_ports.iter().map(|(n, _)| n.as_str()).collect();
        format!("{}({})", fn_name, argnames.join(", "))
    });

    let const_ident = format_ident!("{}_NODE_DEF", fn_name.to_uppercase());

    let in_port_exprs = input_ports.iter().map(|(name, dt)| {
        let dt_tok = data_type_token(dt);
        quote! {
            ::prism_core::language::visual::PortDef {
                id: #name.to_string(),
                label: #name.to_string(),
                kind: ::prism_core::language::visual::PortKind::Data,
                direction: ::prism_core::language::visual::PortDirection::Input,
                data_type: #dt_tok,
            }
        }
    });
    let out_port_expr = if let Some(dt) = &output_dt {
        let dt_tok = data_type_token(dt);
        Some(quote! {
            ::prism_core::language::visual::PortDef {
                id: "result".to_string(),
                label: "result".to_string(),
                kind: ::prism_core::language::visual::PortKind::Data,
                direction: ::prism_core::language::visual::PortDirection::Output,
                data_type: #dt_tok,
            },
        })
    } else {
        None
    };

    let const_def = quote! {
        pub fn #const_ident() -> ::prism_core::language::visual::NodeKindDef {
            ::prism_core::language::visual::NodeKindDef {
                kind: ::prism_core::language::visual::ScriptNodeKind::Custom(#fn_name.to_string()),
                label: #label.to_string(),
                description: #luau_template.to_string(),
                category: #category.to_string(),
                default_ports: ::std::vec![
                    #(#in_port_exprs),*
                    , #out_port_expr
                ],
            }
        }
    };

    Ok(quote! {
        #func
        #const_def
    })
}

fn parse_args(attr: TokenStream2) -> syn::Result<VisualNodeArgs> {
    let mut out = VisualNodeArgs::default();
    if attr.is_empty() {
        return Ok(out);
    }
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("category") {
            let s: syn::LitStr = meta.value()?.parse()?;
            out.category = Some(s.value());
        } else if meta.path.is_ident("label") {
            let s: syn::LitStr = meta.value()?.parse()?;
            out.label = Some(s.value());
        } else if meta.path.is_ident("luau") {
            let s: syn::LitStr = meta.value()?.parse()?;
            out.luau = Some(s.value());
        } else {
            return Err(meta.error("unknown #[visual_node] arg"));
        }
        let _ = std::marker::PhantomData::<Lit>;
        Ok(())
    });
    syn::parse::Parser::parse2(parser, attr)?;
    Ok(out)
}

#[derive(Clone, Copy)]
enum DataTypeTag {
    Number,
    String,
    Boolean,
    Any,
}

fn data_type_for(ty: &Type) -> DataTypeTag {
    let leaf = match ty {
        Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(r) => return data_type_for(&r.elem),
        _ => None,
    };
    match leaf.as_deref() {
        Some("f32" | "f64" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
        | "usize" | "isize") => DataTypeTag::Number,
        Some("bool") => DataTypeTag::Boolean,
        Some("String" | "str") => DataTypeTag::String,
        _ => DataTypeTag::Any,
    }
}

fn data_type_token(dt: &DataTypeTag) -> TokenStream2 {
    match dt {
        DataTypeTag::Number => {
            quote! { ::prism_core::language::visual::DataType::Number }
        }
        DataTypeTag::String => {
            quote! { ::prism_core::language::visual::DataType::String }
        }
        DataTypeTag::Boolean => {
            quote! { ::prism_core::language::visual::DataType::Boolean }
        }
        DataTypeTag::Any => {
            quote! { ::prism_core::language::visual::DataType::Any }
        }
    }
}

fn pascal(snake: &str) -> String {
    let mut out = String::new();
    for word in snake.split('_') {
        let mut chars = word.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}
