//! `#[derive(PrismBlock)]` — emit a `Block` impl from a struct that
//! exposes `schema()` and `template(props, children)` methods.
//!
//! The derive collapses the boilerplate of authoring a builder block
//! into a single template-returning function. Both `render_slint` and
//! `render_html` are derived; the macro routes them through the
//! `render_template_node` / `render_template_html` walkers in
//! `prism-builder`. Override either render method by hand if a block
//! needs Slint-specific or HTML-specific chrome that the
//! `TemplateNode` IR cannot express yet.
//!
//! ```ignore
//! use prism_luau_derive::PrismBlock;
//!
//! #[derive(PrismBlock, Default)]
//! #[block(id = "divider")]
//! pub struct DividerBlock;
//!
//! impl DividerBlock {
//!     fn schema() -> Vec<FieldSpec> { schemas::divider() }
//!     fn template(_props: &Value, _children: &[Node]) -> TemplateNode {
//!         TemplateNode::Container { /* ... */ }
//!     }
//! }
//! ```

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let ident = &input.ident;

    let mut id_lit: Option<syn::LitStr> = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("block") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("id") {
                let s: syn::LitStr = meta.value()?.parse()?;
                id_lit = Some(s);
                Ok(())
            } else {
                Err(meta.error("unknown #[block] arg (expected `id = \"...\"`)"))
            }
        })?;
    }

    let id_lit = id_lit.ok_or_else(|| {
        syn::Error::new_spanned(
            input,
            "#[derive(PrismBlock)] requires #[block(id = \"...\")] attribute",
        )
    })?;

    Ok(quote! {
        impl ::prism_builder::Block for #ident {
            fn id(&self) -> &::prism_builder::ComponentId {
                static ID: ::std::sync::OnceLock<::std::string::String> =
                    ::std::sync::OnceLock::new();
                ID.get_or_init(|| #id_lit.to_string())
            }

            fn schema(&self) -> ::std::vec::Vec<::prism_builder::FieldSpec> {
                <#ident>::schema()
            }

            fn render_slint(
                &self,
                ctx: &::prism_builder::RenderSlintContext<'_>,
                props: &::serde_json::Value,
                children: &[::prism_builder::Node],
                out: &mut ::prism_builder::SlintEmitter,
            ) -> ::std::result::Result<(), ::prism_builder::RenderError> {
                let template = <#ident>::template(props, children);
                ::prism_builder::render_template_node(ctx, &template, props, children, out)
            }

            fn render_html(
                &self,
                ctx: &::prism_builder::HtmlRenderContext<'_>,
                props: &::serde_json::Value,
                children: &[::prism_builder::Node],
                out: &mut ::prism_builder::Html,
            ) -> ::std::result::Result<(), ::prism_builder::RenderError> {
                let template = <#ident>::template(props, children);
                ::prism_builder::render_template_html(ctx, &template, props, children, out)
            }
        }
    })
}
