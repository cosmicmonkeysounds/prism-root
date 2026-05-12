//! `#[daemon_fn]` — Phase 6 of the Dioxus-inspired reactive overhaul
//! (`docs/dev/dioxus-inspiration.md`).
//!
//! Authoring shape:
//!
//! ```ignore
//! #[daemon_fn(id = "fs.read_file")]
//! pub fn read_file(path: String) -> Result<Vec<u8>, FsError> {
//!     std::fs::read(&path).map_err(FsError::from)
//! }
//! ```
//!
//! Three things come out of every annotation:
//!
//! 1. The original function — left intact, available for direct
//!    server-side calls.
//! 2. A `register_<name>(registry)` helper that wires the function
//!    through `register_typed_with_permission` so the daemon
//!    command registry knows the typed shape. Same machinery
//!    `#[daemon_command]` already uses.
//! 3. A `<name>_client(invoker, args)` client-side stub that takes
//!    any `&dyn prism_core::reactive::ipc::DaemonInvoker`, serialises
//!    args via `serde_json::to_value`, calls
//!    [`DaemonInvoker::invoke`](prism_core::reactive::ipc::DaemonInvoker::invoke),
//!    and deserialises the response. Type-equal contract at the
//!    call site — same Result<T, E> on both sides.
//! 4. A `<NAME>_DAEMON_FN_ID: &str` const so tooling can enumerate
//!    available daemon RPC entry points.
//!
//! Required: `id = "..."`. Optional: `permission = User|Dev|Admin`
//! (default `Dev`). The plan's eventual postcard-over-interprocess
//! path swaps in by implementing `DaemonInvoker` with a different
//! payload encoding — the macro expansion is transport-agnostic.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse2, spanned::Spanned, FnArg, Ident, ItemFn, LitStr, Pat, PatType, ReturnType, Type};

struct Args {
    id: LitStr,
    permission: Option<Ident>,
    /// Skip the `_client` stub. Set when the request/response types
    /// don't implement `Serialize` / `DeserializeOwned` and the
    /// caller doesn't need the cross-process surface. Defaults to
    /// `false` (stub emitted by default).
    skip_client: bool,
}

pub fn expand(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args = parse_args(attr)?;
    let mut func: ItemFn = parse2(item)?;

    // ── Validate signature shape ─────────────────────────────────
    // `#[daemon_fn]` accepts `fn(req)` or `async fn(req)`. The
    // initial scaffold required `async`; Phase 6 widens it because
    // the daemon's existing command registry is sync and the
    // typical handler is sync too. The macro emits the same
    // register helper for both shapes.
    let inputs: Vec<&FnArg> = func.sig.inputs.iter().collect();
    let req_ty: &Type = match inputs.as_slice() {
        [req] => request_type(req)?,
        _ => {
            return Err(syn::Error::new(
                func.sig.inputs.span(),
                "#[daemon_fn] expects exactly one argument (the typed request)",
            ));
        }
    };

    let (ok_ty, err_ty): (&Type, &Type) = match &func.sig.output {
        ReturnType::Type(_, ty) => result_arms(ty)?
            .ok_or_else(|| syn::Error::new(ty.span(), "#[daemon_fn] must return Result<_, _>"))?,
        ReturnType::Default => {
            return Err(syn::Error::new(
                func.sig.output.span(),
                "#[daemon_fn] must return Result<_, _>",
            ));
        }
    };

    // The original async-ness affects the register helper: an async
    // body needs an executor on the daemon side, which the current
    // sync registry doesn't have. For now require sync; document
    // the async-runtime follow-up.
    if func.sig.asyncness.is_some() {
        return Err(syn::Error::new(
            func.sig.fn_token.span(),
            "#[daemon_fn] async functions are a follow-up — the daemon's command registry is currently sync; switch to a sync body for now (the request is still served on a worker thread)",
        ));
    }

    // Drop attribute noise that's irrelevant once we've split the
    // signature apart. Doesn't affect emission — just keeps the
    // surface stable when adding tests.
    func.attrs.retain(|a| !a.path().is_ident("daemon_fn"));

    let fn_ident = func.sig.ident.clone();
    let fn_vis = func.vis.clone();
    let id_lit = &args.id;
    let perm_ident = args
        .permission
        .clone()
        .unwrap_or_else(|| format_ident!("Dev"));

    let register_ident = format_ident!("register_{}", fn_ident);
    let client_ident = format_ident!("{}_client", fn_ident);
    let id_const_ident = format_ident!(
        "{}_DAEMON_FN_ID",
        screaming_snake_case(fn_ident.to_string().trim_start_matches("r#"))
    );

    let register_helper = quote! {
        /// Server-side registration helper emitted by `#[daemon_fn]`.
        /// Wires `#fn_ident` into the daemon command registry with
        /// the configured permission tier.
        #[allow(non_snake_case)]
        #fn_vis fn #register_ident(
            registry: &::prism_daemon::registry::CommandRegistry,
        ) -> ::std::result::Result<(), ::prism_daemon::registry::CommandError> {
            use ::prism_daemon::typed_command::CommandRegistryExt;
            registry.register_typed_with_permission(
                #id_lit,
                ::prism_daemon::permission::Permission::#perm_ident,
                |req: #req_ty| #fn_ident(req),
            )
        }
    };

    let client_stub = if args.skip_client {
        quote! {}
    } else {
        quote! {
            /// Client-side stub emitted by `#[daemon_fn]`. Serialises
            /// `args` to JSON, invokes the daemon over the
            /// configured transport, and deserialises the response.
            /// Phase 6 of `docs/dev/dioxus-inspiration.md`.
            ///
            /// Returns `Err(DaemonFnError::Transport(_))` for
            /// transport failures, `Err(DaemonFnError::Remote(_))`
            /// for daemon-side errors, and the typed Ok arm on
            /// success.
            #[allow(non_snake_case)]
            #fn_vis fn #client_ident(
                invoker: &dyn ::prism_core::reactive::ipc::DaemonInvoker,
                args: #req_ty,
            ) -> ::std::result::Result<
                #ok_ty,
                ::prism_core::reactive::ipc::DaemonFnError<#err_ty>,
            > {
                let payload = ::serde_json::to_value(&args).map_err(|e| {
                    ::prism_core::reactive::ipc::DaemonFnError::Encode(e.to_string())
                })?;
                let resp_payload = invoker
                    .invoke(#id_lit, payload)
                    .map_err(::prism_core::reactive::ipc::DaemonFnError::from_remote)?;
                ::serde_json::from_value::<#ok_ty>(resp_payload).map_err(|e| {
                    ::prism_core::reactive::ipc::DaemonFnError::Decode(e.to_string())
                })
            }
        }
    };

    // Suppress the unused-imports warning for `err_ty`. We don't
    // emit a downcast path today; the error arm carries the typed
    // shape through generics so callers' Result<_, E> matches.
    let _ = err_ty;

    Ok(quote! {
        #func

        #register_helper

        #client_stub

        /// Tooling marker: stable id for this `#[daemon_fn]`.
        #[allow(non_upper_case_globals, dead_code)]
        #fn_vis const #id_const_ident: &str = #id_lit;
    })
}

fn parse_args(attr: TokenStream2) -> syn::Result<Args> {
    let mut id: Option<LitStr> = None;
    let mut permission: Option<Ident> = None;
    let mut skip_client: bool = false;
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("id") {
            id = Some(meta.value()?.parse::<LitStr>()?);
        } else if meta.path.is_ident("permission") {
            permission = Some(meta.value()?.parse::<Ident>()?);
        } else if meta.path.is_ident("skip_client") {
            skip_client = true;
        } else {
            return Err(meta.error(
                "unknown #[daemon_fn] arg (expected `id`, `permission`, or `skip_client`)",
            ));
        }
        Ok(())
    });
    syn::parse::Parser::parse2(parser, attr)?;
    let id = id.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[daemon_fn] requires `id = \"...\"`",
        )
    })?;
    Ok(Args {
        id,
        permission,
        skip_client,
    })
}

fn request_type(arg: &FnArg) -> syn::Result<&Type> {
    match arg {
        FnArg::Typed(PatType { pat, ty, .. }) => {
            // Reject `self` / pattern destructuring — keep the
            // surface predictable.
            if matches!(pat.as_ref(), Pat::Ident(_) | Pat::Wild(_)) {
                Ok(ty.as_ref())
            } else {
                Err(syn::Error::new(
                    pat.span(),
                    "#[daemon_fn] argument must be a simple ident pattern",
                ))
            }
        }
        FnArg::Receiver(_) => Err(syn::Error::new(
            arg.span(),
            "#[daemon_fn] functions are free fn — no `self` argument",
        )),
    }
}

/// Returns `Some((ok_ty, err_ty))` if `ty` matches `Result<O, E>`,
/// preserving the AST nodes so the macro can splice them into the
/// emission. Returns `Ok(None)` if the type isn't a Result; returns
/// `Err` on syntactic shapes we explicitly want to surface.
fn result_arms(ty: &Type) -> syn::Result<Option<(&Type, &Type)>> {
    if let Type::Path(p) = ty {
        if let Some(last) = p.path.segments.last() {
            if last.ident != "Result" {
                return Ok(None);
            }
            if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                let mut iter = args.args.iter().filter_map(|a| match a {
                    syn::GenericArgument::Type(t) => Some(t),
                    _ => None,
                });
                let ok = iter.next();
                let err = iter.next();
                if let (Some(ok), Some(err)) = (ok, err) {
                    return Ok(Some((ok, err)));
                }
            }
            return Err(syn::Error::new(
                last.arguments.span(),
                "#[daemon_fn] Result must have two type parameters",
            ));
        }
    }
    Ok(None)
}

fn screaming_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_uppercase() && prev_lower {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
        prev_lower = c.is_lowercase();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screaming_snake_handles_snake_and_camel() {
        assert_eq!(screaming_snake_case("read_file"), "READ_FILE");
        assert_eq!(screaming_snake_case("readFile"), "READ_FILE");
        assert_eq!(screaming_snake_case("a"), "A");
    }

    #[test]
    fn expand_emits_register_helper_client_stub_and_id_const() {
        let attr = quote! { id = "fs.read_file" };
        let item = quote! {
            pub fn read_file(req: ReadFileReq) -> Result<ReadFileResp, FsError> {
                unimplemented!()
            }
        };
        let out = expand(attr, item).expect("scaffold accepts a valid sync Result fn");
        let text = out.to_string();
        assert!(text.contains("READ_FILE_DAEMON_FN_ID"));
        assert!(text.contains("\"fs.read_file\""));
        assert!(text.contains("register_read_file"));
        assert!(text.contains("read_file_client"));
        assert!(text.contains("DaemonInvoker"));
        assert!(text.contains("register_typed_with_permission"));
    }

    #[test]
    fn expand_skip_client_omits_stub() {
        let attr = quote! { id = "x.y", skip_client };
        let item = quote! {
            pub fn x(req: A) -> Result<B, C> { unimplemented!() }
        };
        let out = expand(attr, item).expect("ok");
        let text = out.to_string();
        assert!(text.contains("register_x"));
        assert!(
            !text.contains("x_client"),
            "skip_client should suppress the client stub"
        );
    }

    #[test]
    fn expand_uses_dev_permission_by_default() {
        let attr = quote! { id = "x" };
        let item = quote! { pub fn x(req: A) -> Result<B, C> { unimplemented!() } };
        let out = expand(attr, item).expect("ok");
        assert!(out.to_string().contains("Permission :: Dev"));
    }

    #[test]
    fn expand_honours_explicit_permission() {
        let attr = quote! { id = "x", permission = Admin };
        let item = quote! { pub fn x(req: A) -> Result<B, C> { unimplemented!() } };
        let out = expand(attr, item).expect("ok");
        assert!(out.to_string().contains("Permission :: Admin"));
    }

    #[test]
    fn expand_rejects_async_for_now() {
        let attr = quote! { id = "x" };
        let item = quote! {
            pub async fn x(req: A) -> Result<B, C> { unimplemented!() }
        };
        let err = expand(attr, item).unwrap_err();
        assert!(err.to_string().contains("async"));
    }

    #[test]
    fn expand_rejects_function_without_result_return() {
        let attr = quote! { id = "x" };
        let item = quote! {
            pub fn no_result(req: A) -> Vec<u8> { vec![] }
        };
        let err = expand(attr, item).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("result"));
    }

    #[test]
    fn expand_rejects_function_with_zero_args() {
        let attr = quote! { id = "x" };
        let item = quote! {
            pub fn x() -> Result<(), ()> { Ok(()) }
        };
        let err = expand(attr, item).unwrap_err();
        assert!(err.to_string().contains("exactly one"));
    }

    #[test]
    fn expand_rejects_function_with_multiple_args() {
        let attr = quote! { id = "x" };
        let item = quote! {
            pub fn x(a: A, b: B) -> Result<(), ()> { Ok(()) }
        };
        let err = expand(attr, item).unwrap_err();
        assert!(err.to_string().contains("exactly one"));
    }

    #[test]
    fn expand_rejects_function_without_id() {
        let attr = quote! { permission = User };
        let item = quote! {
            pub fn x(req: A) -> Result<(), ()> { Ok(()) }
        };
        let err = expand(attr, item).unwrap_err();
        assert!(err.to_string().contains("id"));
    }
}
