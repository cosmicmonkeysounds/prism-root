//! `#[daemon_command]` — desugar a typed handler into the
//! `register_typed`-shaped registration glue every daemon module
//! currently writes by hand.
//!
//! Two function shapes are supported, distinguished by arity:
//!
//! ```ignore
//! // Stateless command — one arg is the typed request.
//! #[daemon_command(id = "crypto.keypair")]
//! fn keypair(_: EmptyArgs) -> Result<KeypairResp, Infallible> { ... }
//!
//! // Stateful command — first arg is `&Manager`, second is the request.
//! #[daemon_command(id = "watcher.watch")]
//! fn watch(mgr: &WatcherManager, args: WatchArgs) -> Result<WatchResp, String> { ... }
//! ```
//!
//! Each annotation emits the original function plus a sibling
//! `register_<fname>` helper. The stateless helper takes
//! `(&CommandRegistry)`; the stateful helper takes
//! `(&CommandRegistry, Arc<StateType>)` and clones the Arc into the
//! closure. Permission tier defaults to `Dev`; pass
//! `permission = User` (or any `Permission` variant ident) to override.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse2, spanned::Spanned, FnArg, Ident, ItemFn, LitStr, Pat, PatType, ReturnType, Type};

pub fn expand(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args = parse_args(attr)?;
    let func: ItemFn = parse2(item)?;

    let fn_ident = func.sig.ident.clone();
    let register_ident = format_ident!("register_{}", fn_ident);

    let inputs: Vec<&FnArg> = func.sig.inputs.iter().collect();
    let (state_ty, req_ty) = match inputs.as_slice() {
        [req] => (None, request_type(req)?),
        [state, req] => (Some(state_ref_type(state)?), request_type(req)?),
        _ => {
            return Err(syn::Error::new(
                func.sig.inputs.span(),
                "#[daemon_command] expects `fn(req)` or `fn(&state, req)`",
            ))
        }
    };

    // Confirm the return type is `Result<_, _>`. We don't need the
    // inner types — `register_typed` is generic — but we surface a
    // friendlier error if the user wrote `-> Resp` by accident.
    if let ReturnType::Type(_, ty) = &func.sig.output {
        if !type_is_result(ty) {
            return Err(syn::Error::new(
                ty.span(),
                "#[daemon_command] handler must return Result<_, _>",
            ));
        }
    } else {
        return Err(syn::Error::new(
            func.sig.output.span(),
            "#[daemon_command] handler must return Result<_, _>",
        ));
    }

    let id_lit = &args.id;
    let perm_ident = args
        .permission
        .clone()
        .unwrap_or_else(|| format_ident!("Dev"));

    let register_fn = if let Some(state_ty) = state_ty {
        quote! {
            pub fn #register_ident(
                registry: &::prism_daemon::registry::CommandRegistry,
                state: ::std::sync::Arc<#state_ty>,
            ) -> ::std::result::Result<(), ::prism_daemon::registry::CommandError> {
                use ::prism_daemon::typed_command::CommandRegistryExt;
                let __state = state;
                registry.register_typed_with_permission(
                    #id_lit,
                    ::prism_daemon::permission::Permission::#perm_ident,
                    move |req: #req_ty| #fn_ident(&__state, req),
                )
            }
        }
    } else {
        quote! {
            pub fn #register_ident(
                registry: &::prism_daemon::registry::CommandRegistry,
            ) -> ::std::result::Result<(), ::prism_daemon::registry::CommandError> {
                use ::prism_daemon::typed_command::CommandRegistryExt;
                registry.register_typed_with_permission(
                    #id_lit,
                    ::prism_daemon::permission::Permission::#perm_ident,
                    |req: #req_ty| #fn_ident(req),
                )
            }
        }
    };

    Ok(quote! {
        #func
        #register_fn
    })
}

struct Args {
    id: LitStr,
    permission: Option<Ident>,
}

fn parse_args(attr: TokenStream2) -> syn::Result<Args> {
    let mut id: Option<LitStr> = None;
    let mut permission: Option<Ident> = None;
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("id") {
            id = Some(meta.value()?.parse::<LitStr>()?);
        } else if meta.path.is_ident("permission") {
            permission = Some(meta.value()?.parse::<Ident>()?);
        } else {
            return Err(meta.error("unknown #[daemon_command] arg (expected `id` or `permission`)"));
        }
        Ok(())
    });
    syn::parse::Parser::parse2(parser, attr)?;
    let id = id.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[daemon_command] requires `id = \"...\"`",
        )
    })?;
    Ok(Args { id, permission })
}

fn pat_type(arg: &FnArg) -> syn::Result<&PatType> {
    match arg {
        FnArg::Typed(pt) => Ok(pt),
        FnArg::Receiver(_) => Err(syn::Error::new(
            arg.span(),
            "#[daemon_command] does not support `self` arguments",
        )),
    }
}

fn request_type(arg: &FnArg) -> syn::Result<&Type> {
    let pt = pat_type(arg)?;
    // Accept `req: T` or `_: T`; just take the type.
    match pt.pat.as_ref() {
        Pat::Ident(_) | Pat::Wild(_) => Ok(pt.ty.as_ref()),
        _ => Err(syn::Error::new(
            pt.pat.span(),
            "#[daemon_command] request arg must be a plain ident or `_`",
        )),
    }
}

fn state_ref_type(arg: &FnArg) -> syn::Result<&Type> {
    let pt = pat_type(arg)?;
    match pt.ty.as_ref() {
        Type::Reference(r) => Ok(r.elem.as_ref()),
        _ => Err(syn::Error::new(
            pt.ty.span(),
            "#[daemon_command] state arg must be `&StateType`",
        )),
    }
}

fn type_is_result(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == "Result";
        }
    }
    false
}
