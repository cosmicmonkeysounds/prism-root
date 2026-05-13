//! `#[peer_fn]` — §3.4 of `docs/dev/dioxus-inspiration.md`.
//!
//! Completes the macro family: `(DaemonInvoker, RelayInvoker, PeerInvoker)`.
//! Same authoring shape as the other two — one signature, a
//! `Result<Ok, Err>` return — but the emitted client stub takes a
//! `&dyn PeerInvoker` so the wire is a WebRTC data channel reached
//! through `prism_core::reactive::ipc::PeerInvoker` rather than a
//! WebSocket or local socket.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse2, spanned::Spanned, FnArg, Ident, ItemFn, LitStr, Pat, PatType, ReturnType, Type};

struct Args {
    id: LitStr,
    skip_client: bool,
}

pub fn expand(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args = parse_args(attr)?;
    let mut func: ItemFn = parse2(item)?;

    let inputs: Vec<&FnArg> = func.sig.inputs.iter().collect();
    let req_ty: &Type = match inputs.as_slice() {
        [req] => request_type(req)?,
        _ => {
            return Err(syn::Error::new(
                func.sig.inputs.span(),
                "#[peer_fn] expects exactly one argument (the typed request)",
            ));
        }
    };

    let (ok_ty, err_ty): (&Type, &Type) = match &func.sig.output {
        ReturnType::Type(_, ty) => result_arms(ty)?
            .ok_or_else(|| syn::Error::new(ty.span(), "#[peer_fn] must return Result<_, _>"))?,
        ReturnType::Default => {
            return Err(syn::Error::new(
                func.sig.output.span(),
                "#[peer_fn] must return Result<_, _>",
            ));
        }
    };

    if func.sig.asyncness.is_some() {
        return Err(syn::Error::new(
            func.sig.fn_token.span(),
            "#[peer_fn] async functions are a follow-up — the body runs synchronously today",
        ));
    }

    func.attrs.retain(|a| !a.path().is_ident("peer_fn"));

    let fn_ident = func.sig.ident.clone();
    let fn_vis = func.vis.clone();
    let id_lit = &args.id;

    let client_ident = format_ident!("{}_client", fn_ident);
    let id_const_ident = format_ident!(
        "{}_PEER_FN_ID",
        screaming_snake_case(fn_ident.to_string().trim_start_matches("r#"))
    );

    let client_stub = if args.skip_client {
        quote! {}
    } else {
        quote! {
            /// Client-side stub emitted by `#[peer_fn]`. Serialises
            /// `args` to JSON, invokes the configured peer transport,
            /// and deserialises the response. §3.4 of
            /// `docs/dev/dioxus-inspiration.md`.
            ///
            /// `Err(PeerFnError::Transport(_))` for transport failures
            /// (offline / timeout / data-channel closed),
            /// `Err(PeerFnError::Remote(_))` for typed peer-side errors.
            #[allow(non_snake_case)]
            #fn_vis fn #client_ident(
                invoker: &dyn ::prism_core::reactive::ipc::PeerInvoker,
                args: #req_ty,
            ) -> ::std::result::Result<
                #ok_ty,
                ::prism_core::reactive::ipc::PeerFnError<#err_ty>,
            > {
                let payload = ::serde_json::to_value(&args).map_err(|e| {
                    ::prism_core::reactive::ipc::PeerFnError::Encode(e.to_string())
                })?;
                let resp_payload = invoker
                    .invoke(#id_lit, payload)
                    .map_err(::prism_core::reactive::ipc::PeerFnError::from_remote)?;
                ::serde_json::from_value::<#ok_ty>(resp_payload).map_err(|e| {
                    ::prism_core::reactive::ipc::PeerFnError::Decode(e.to_string())
                })
            }
        }
    };

    let _ = err_ty;

    Ok(quote! {
        #func

        #client_stub

        /// Tooling marker: stable id for this `#[peer_fn]`.
        #[allow(non_upper_case_globals, dead_code)]
        #fn_vis const #id_const_ident: &str = #id_lit;
    })
}

fn parse_args(attr: TokenStream2) -> syn::Result<Args> {
    let mut id: Option<LitStr> = None;
    let mut skip_client: bool = false;
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("id") {
            id = Some(meta.value()?.parse::<LitStr>()?);
        } else if meta.path.is_ident("skip_client") {
            skip_client = true;
        } else {
            return Err(meta.error("unknown #[peer_fn] arg (expected `id` or `skip_client`)"));
        }
        Ok(())
    });
    syn::parse::Parser::parse2(parser, attr)?;
    let id = id.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[peer_fn] requires `id = \"...\"`",
        )
    })?;
    Ok(Args { id, skip_client })
}

fn request_type(arg: &FnArg) -> syn::Result<&Type> {
    match arg {
        FnArg::Typed(PatType { pat, ty, .. }) => {
            if matches!(pat.as_ref(), Pat::Ident(_) | Pat::Wild(_)) {
                Ok(ty.as_ref())
            } else {
                Err(syn::Error::new(
                    pat.span(),
                    "#[peer_fn] argument must be a plain identifier",
                ))
            }
        }
        _ => Err(syn::Error::new(
            arg.span(),
            "#[peer_fn] does not accept `self`",
        )),
    }
}

fn result_arms(ty: &Type) -> syn::Result<Option<(&Type, &Type)>> {
    let Type::Path(tp) = ty else { return Ok(None) };
    let Some(seg) = tp.path.segments.last() else {
        return Ok(None);
    };
    if seg.ident != "Result" {
        return Ok(None);
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return Ok(None);
    };
    let mut iter = args.args.iter().filter_map(|a| match a {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    });
    let ok = iter.next();
    let err = iter.next();
    match (ok, err) {
        (Some(ok), Some(err)) => Ok(Some((ok, err))),
        _ => Ok(None),
    }
}

fn screaming_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase()
            && i > 0
            && !chars[i - 1].is_ascii_uppercase()
            && chars[i - 1] != '_'
        {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

#[allow(dead_code)]
fn _unused_anchors(_: Ident) {}
