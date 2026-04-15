//! The `Component` trait — every renderable block implements this.
//!
//! A component has two render targets:
//!
//! * **Slint** ([`Component::render_slint`]) — the interactive Studio
//!   path. Components emit a value tree that Phase 3 will feed into a
//!   runtime-compiled Slint component via `slint-interpreter`, so the
//!   same component registry that drives HTML SSR also drives the live
//!   Studio preview. Still a stub until Phase 3 wires the interpreter
//!   end-to-end.
//! * **HTML** ([`Component::render_html`]) — the Sovereign Portal SSR
//!   path. Components emit semantic HTML into an [`Html`] buffer so
//!   `prism-relay` can serve a crawler-friendly, JS-less document to
//!   anonymous visitors. This target is live today.
//!
//! The trait is deliberately object-safe so `Arc<dyn Component>`s can
//! live in the [`crate::registry::ComponentRegistry`] and be dispatched
//! by `ComponentId` at walk time.

use serde_json::Value;
use thiserror::Error;

use crate::document::Node;
use crate::html::Html;
use crate::registry::ComponentRegistry;

/// Stable identifier for a component *type* (e.g. `"card"`, `"button"`).
///
/// This is the key the [`crate::registry::ComponentRegistry`] uses when
/// looking up a renderer for a given node.
pub type ComponentId = String;

/// Errors a component can hit while rendering into a target backend.
/// Surfaces as 500s in `prism-relay` and as a developer-visible red
/// banner in Studio's builder pane.
#[derive(Debug, Error)]
pub enum RenderError {
    /// A child node references a `ComponentId` that isn't registered.
    #[error("unknown component: {0}")]
    UnknownComponent(ComponentId),

    /// Props failed validation or a component chose to bail.
    #[error("render failed: {0}")]
    Failed(String),
}

/// Slint-side render context. Placeholder — grows a
/// `slint_interpreter::ComponentInstance`, selection state, and
/// hot-reload handles when Phase 3 lands the rest of the builder UI.
/// Today it carries design tokens so components can already refer
/// to the shared palette.
pub struct RenderContext<'a> {
    pub tokens: &'a prism_core::design_tokens::DesignTokens,
}

/// HTML-side render context. Carries the registry (so parents can
/// recurse into their children by `ComponentId`) plus the design
/// tokens (so portal markup can inline a consistent theme without
/// dragging in Studio's Slint path). Constructed fresh for each
/// request by [`crate::render::render_document_html`].
pub struct RenderHtmlContext<'a> {
    pub tokens: &'a prism_core::design_tokens::DesignTokens,
    pub registry: &'a ComponentRegistry,
}

impl<'a> RenderHtmlContext<'a> {
    /// Render one child node into `out`. Components with slots call
    /// this for each child they want to emit — the context owns the
    /// registry lookup so components don't have to.
    pub fn render_child(&self, child: &Node, out: &mut Html) -> Result<(), RenderError> {
        let component = self
            .registry
            .get(&child.component)
            .ok_or_else(|| RenderError::UnknownComponent(child.component.clone()))?;
        component.render_html(self, &child.props, &child.children, out)
    }

    /// Render every child in order. The common case for layout-only
    /// wrappers (cards, rows, columns) that don't need to reorder or
    /// decorate their slots.
    pub fn render_children(&self, children: &[Node], out: &mut Html) -> Result<(), RenderError> {
        for child in children {
            self.render_child(child, out)?;
        }
        Ok(())
    }
}

/// The core contract. Trait-objects of this type live in the
/// registry; each node in the builder document is dispatched through
/// whichever impl the registry hands back for its `ComponentId`.
pub trait Component: Send + Sync {
    fn id(&self) -> &ComponentId;

    /// Schema for the property panel. JSON today so the shape can
    /// evolve without churning every call site; a typed field-factory
    /// layer will land on top of this in Phase 3.
    fn schema(&self) -> Value;

    /// Paint the component into Slint. Stub until Phase 3 wires the
    /// `slint-interpreter` pipeline — the default impl echoes props
    /// so existing round-trip tests keep compiling.
    fn render_slint(&self, ctx: &RenderContext<'_>, props: &Value) -> Value {
        let _ = ctx;
        props.clone()
    }

    /// Paint the component as semantic HTML for the Sovereign Portal
    /// SSR path. Default impl emits a generic `<div data-component="id">`
    /// wrapper and renders children in order — good enough for
    /// layout-only containers and a safe fallback for any component
    /// that hasn't opted into a bespoke markup shape yet.
    fn render_html(
        &self,
        ctx: &RenderHtmlContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let _ = props;
        out.open_attrs("div", &[("data-component", self.id())]);
        ctx.render_children(children, out)?;
        out.close("div");
        Ok(())
    }
}
