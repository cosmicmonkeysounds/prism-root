//! `#[derive(PrismBlock)]` — emit a `Block` impl from a struct that
//! exposes `template(props, children)` (and optionally `schema()`).
//!
//! The derive collapses the boilerplate of authoring a builder block
//! into a single template-returning function. The generated `lower_ui`
//! routes through `render_template_node` in `prism-builder`.
//!
//! Two attribute forms:
//!
//! ```ignore
//! // Form A — raw `&Value` props. The block hand-writes both
//! // `schema()` and `template(&Value, &[Node])`.
//! #[derive(PrismBlock, Default)]
//! #[block(id = "divider")]
//! pub struct DividerBlock;
//!
//! impl DividerBlock {
//!     fn schema() -> Vec<FieldSpec> { schemas::divider() }
//!     fn template(_props: &Value, _children: &[Node]) -> TemplateNode { /* ... */ }
//! }
//!
//! // Form B — typed `&MyProps` props. The struct named by `props = "..."`
//! // must derive `PrismField` (so the macro can call `MyProps::field_specs()`
//! // and `MyProps::from_value(&Value)`). `template(&MyProps, &[Node])`
//! // receives the extracted typed props directly.
//! #[derive(PrismBlock, Default)]
//! #[block(id = "text", props = "TextProps")]
//! pub struct TextBlock;
//!
//! impl TextBlock {
//!     fn template(p: &TextProps, _children: &[Node]) -> TemplateNode { /* ... */ }
//! }
//! ```

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let ident = &input.ident;

    let mut id_lit: Option<syn::LitStr> = None;
    let mut props_ty: Option<syn::Type> = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("block") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("id") {
                let s: syn::LitStr = meta.value()?.parse()?;
                id_lit = Some(s);
                Ok(())
            } else if meta.path.is_ident("props") {
                let s: syn::LitStr = meta.value()?.parse()?;
                props_ty = Some(s.parse()?);
                Ok(())
            } else {
                Err(meta
                    .error("unknown #[block] arg (expected `id = \"...\"` or `props = \"...\"`)"))
            }
        })?;
    }

    let id_lit = id_lit.ok_or_else(|| {
        syn::Error::new_spanned(
            input,
            "#[derive(PrismBlock)] requires #[block(id = \"...\")] attribute",
        )
    })?;

    // When `props = "MyProps"` is set the schema is derived from
    // `MyProps::field_specs()` and `template()` receives `&MyProps`.
    // Otherwise the host struct hand-writes `schema()` + `template(&Value, …)`.
    let (schema_body, template_call) = match &props_ty {
        Some(ty) => (
            quote! { <#ty>::field_specs() },
            quote! {{
                let __typed_props = <#ty>::from_value(&node.props);
                <#ident>::template(&__typed_props, &node.children)
            }},
        ),
        None => (
            quote! { <#ident>::schema() },
            quote! { <#ident>::template(&node.props, &node.children) },
        ),
    };

    Ok(quote! {
        impl ::prism_builder::Block for #ident {
            fn id(&self) -> &::prism_builder::ComponentId {
                static ID: ::std::sync::OnceLock<::std::string::String> =
                    ::std::sync::OnceLock::new();
                ID.get_or_init(|| #id_lit.to_string())
            }

            fn schema(&self) -> ::std::vec::Vec<::prism_builder::FieldSpec> {
                #schema_body
            }

            fn lower_ui(
                &self,
                ctx: &::prism_builder::ui_lower::LowerCtx<'_>,
                node: &::prism_builder::Node,
                style: &::prism_builder::StyleProperties,
            ) -> ::prism_ui_runtime::layout::Node {
                let __template = #template_call;
                ::prism_builder::lower_template(
                    ctx,
                    &__template,
                    &node.props,
                    &node.children,
                    style,
                    &node.id,
                )
            }
        }
    })
}
