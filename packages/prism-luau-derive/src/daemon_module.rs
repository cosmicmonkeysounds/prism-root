//! `#[daemon_module]` — companion to `#[daemon_command]`. Annotates a
//! module struct and emits the `DaemonModule` impl that wires every
//! sibling `register_<cmd>` symbol the macro is told about.
//!
//! Three shapes are supported:
//!
//! ```ignore
//! // Stateless — every command's `register_<fn>` takes only `&CommandRegistry`.
//! #[daemon_module(id = "prism.crypto", commands(keypair, encrypt, decrypt))]
//! pub struct CryptoModule;
//!
//! // Slot-backed — the manager is stashed on a `DaemonBuilder::*_slot()`
//! // accessor so hosts can inject a preconfigured one.
//! #[daemon_module(
//!     id = "prism.crdt",
//!     slot = doc_manager_slot,
//!     default = || Arc::new(DocManager::new()),
//!     commands(write, read, export, import),
//! )]
//! pub struct CrdtModule;
//!
//! // Direct state — the manager is constructed inline at install time.
//! #[daemon_module(
//!     id = "prism.debug",
//!     state = Arc::new(DebugManager::new()),
//!     commands(launch, set_breakpoints, /* … */),
//! )]
//! pub struct DebugModule;
//! ```

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse2, Expr, Ident, ItemStruct, LitStr};

pub fn expand(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args = parse_args(attr)?;
    let item_struct: ItemStruct = parse2(item)?;
    let mod_ident = &item_struct.ident;
    let id_lit = &args.id;

    let register_calls: Vec<TokenStream2> = args
        .commands
        .iter()
        .map(|cmd| {
            let reg_ident = format_ident!("register_{}", cmd);
            match &args.state {
                StateForm::Stateless => quote! { #reg_ident(&__registry)?; },
                _ => quote! { #reg_ident(&__registry, ::std::sync::Arc::clone(&__state))?; },
            }
        })
        .collect();

    let state_acquire = match &args.state {
        StateForm::Stateless => quote! {},
        StateForm::Direct(expr) => quote! {
            let __state = #expr;
        },
        StateForm::Slot { slot, default } => quote! {
            let __state = builder
                .#slot()
                .get_or_insert_with(#default)
                .clone();
        },
    };

    Ok(quote! {
        #item_struct

        impl ::prism_daemon::module::DaemonModule for #mod_ident {
            fn id(&self) -> &str {
                #id_lit
            }

            fn install(
                &self,
                builder: &mut ::prism_daemon::builder::DaemonBuilder,
            ) -> ::std::result::Result<(), ::prism_daemon::registry::CommandError> {
                #state_acquire
                let __registry = builder.registry().clone();
                #(#register_calls)*
                Ok(())
            }
        }
    })
}

enum StateForm {
    Stateless,
    Direct(Expr),
    Slot { slot: Ident, default: Expr },
}

struct Args {
    id: LitStr,
    state: StateForm,
    commands: Vec<Ident>,
}

fn parse_args(attr: TokenStream2) -> syn::Result<Args> {
    let mut id: Option<LitStr> = None;
    let mut state_expr: Option<Expr> = None;
    let mut slot: Option<Ident> = None;
    let mut default_expr: Option<Expr> = None;
    let mut commands: Vec<Ident> = Vec::new();

    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("id") {
            id = Some(meta.value()?.parse::<LitStr>()?);
        } else if meta.path.is_ident("state") {
            state_expr = Some(meta.value()?.parse::<Expr>()?);
        } else if meta.path.is_ident("slot") {
            slot = Some(meta.value()?.parse::<Ident>()?);
        } else if meta.path.is_ident("default") {
            default_expr = Some(meta.value()?.parse::<Expr>()?);
        } else if meta.path.is_ident("commands") {
            meta.parse_nested_meta(|inner| {
                let ident = inner
                    .path
                    .get_ident()
                    .ok_or_else(|| inner.error("expected command ident"))?
                    .clone();
                commands.push(ident);
                Ok(())
            })?;
        } else {
            return Err(meta.error(
                "unknown #[daemon_module] arg (expected `id`, `state`, `slot`, `default`, or `commands`)",
            ));
        }
        Ok(())
    });
    syn::parse::Parser::parse2(parser, attr)?;

    let id = id.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[daemon_module] requires `id = \"...\"`",
        )
    })?;

    if commands.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[daemon_module] requires `commands(name1, name2, …)`",
        ));
    }

    let state = match (state_expr, slot, default_expr) {
        (None, None, None) => StateForm::Stateless,
        (Some(expr), None, None) => StateForm::Direct(expr),
        (None, Some(slot), Some(default)) => StateForm::Slot { slot, default },
        (None, Some(_), None) => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[daemon_module] `slot = …` requires a paired `default = …`",
            ));
        }
        (None, None, Some(_)) => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[daemon_module] `default = …` requires a paired `slot = …`",
            ));
        }
        _ => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[daemon_module] `state` is mutually exclusive with `slot`/`default`",
            ));
        }
    };

    Ok(Args {
        id,
        state,
        commands,
    })
}
