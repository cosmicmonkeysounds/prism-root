//! The `Component` trait — every Slint-renderable block implements this.
//!
//! [`Component::render_slint`] — the interactive Studio path. Components
//! emit `.slint` DSL snippets into a shared [`crate::slint_source::SlintEmitter`];
//! the document walker in [`crate::render::render_document_slint_source`]
//! composes those snippets into a self-contained component source the
//! Studio shell hands to [`slint_interpreter::Compiler`].
//!
//! HTML SSR is handled by the separate [`crate::html_block::HtmlBlock`]
//! trait and [`crate::html_block::HtmlRegistry`], which `prism-relay`
//! uses for Sovereign Portal rendering.
//!
//! The trait is deliberately object-safe so `Arc<dyn Component>`s can
//! live in the [`crate::registry::ComponentRegistry`] and be dispatched
//! by `ComponentId` at walk time.

use prism_core::help::HelpEntry;
use serde_json::Value;
use thiserror::Error;

use crate::document::Node;
use crate::registry::{ComponentRegistry, FieldSpec};
use crate::signal::SignalDef;
use crate::slint_source::SlintEmitter;
use crate::variant::VariantAxis;

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

/// Slint-side render context — used by the Sovereign Portal's Phase-3
/// interactive surface and by tests. Carries a reference to the design
/// tokens so components can pull the shared palette without a
/// thread-local.
///
/// Kept as a separate context from [`RenderSlintContext`] because the
/// Studio builder (which writes through [`SlintEmitter`]) and the
/// ad-hoc host-side Slint callers (which want typed values without
/// DSL generation) have different needs.
pub struct RenderContext<'a> {
    pub tokens: &'a prism_core::design_tokens::DesignTokens,
}

/// Slint-side render context used by the DSL emitter. Carries the
/// registry (so parents can recurse into their children by
/// `ComponentId`) plus the design tokens (so components can reference
/// the shared palette without a thread-local) and a running counter
/// the walker uses to mint unique element ids.
///
/// Constructed fresh per document walk in
/// [`crate::render::render_document_slint_source`].
pub struct RenderSlintContext<'a> {
    pub tokens: &'a prism_core::design_tokens::DesignTokens,
    pub registry: &'a ComponentRegistry,
    pub resources:
        &'a indexmap::IndexMap<crate::resource::ResourceId, crate::resource::ResourceDef>,
}

impl<'a> RenderSlintContext<'a> {
    pub fn render_child(&self, child: &Node, out: &mut SlintEmitter) -> Result<(), RenderError> {
        let component = self
            .registry
            .get(&child.component)
            .ok_or_else(|| RenderError::UnknownComponent(child.component.clone()))?;

        let props = crate::resource::resolve_resource_refs(&child.props, self.resources);
        let props = crate::variant::apply_variant_defaults(&props, &component.variants());

        if child.modifiers.is_empty() {
            component.render_slint(self, &props, &child.children, out)
        } else {
            self.apply_slint_modifiers(
                &child.modifiers,
                0,
                &props,
                &child.children,
                &*component,
                out,
            )
        }
    }

    pub fn render_children(
        &self,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        for child in children {
            self.render_child(child, out)?;
        }
        Ok(())
    }

    fn apply_slint_modifiers(
        &self,
        modifiers: &[crate::modifier::Modifier],
        index: usize,
        props: &serde_json::Value,
        children: &[Node],
        component: &dyn Component,
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        if index >= modifiers.len() {
            return component.render_slint(self, props, children, out);
        }
        let modifier = &modifiers[index];
        match modifier.kind {
            crate::modifier::ModifierKind::ScrollOverflow => out.block("Flickable", |out| {
                self.apply_slint_modifiers(modifiers, index + 1, props, children, component, out)
            }),
            _ => self.apply_slint_modifiers(modifiers, index + 1, props, children, component, out),
        }
    }
}

/// The core Slint-side contract. Trait-objects of this type live in the
/// registry; each node in the builder document is dispatched through
/// whichever impl the registry hands back for its `ComponentId`.
pub trait Component: Send + Sync {
    fn id(&self) -> &ComponentId;

    /// Typed schema for the Studio property panel. Returns the ordered
    /// list of fields the component expects in a node's `props` map;
    /// the panel renders one editor per entry using the factories in
    /// [`crate::registry`].
    fn schema(&self) -> Vec<FieldSpec>;

    fn help_entry(&self) -> Option<HelpEntry> {
        None
    }

    fn signals(&self) -> Vec<SignalDef> {
        vec![]
    }

    fn variants(&self) -> Vec<VariantAxis> {
        vec![]
    }

    /// Paint the component as `.slint` DSL into a shared
    /// [`SlintEmitter`]. The default impl emits a semantically
    /// transparent `Rectangle { }` wrapper and recurses into children.
    /// Override to produce bespoke Slint markup.
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let _ = props;
        let id = self.id().clone();
        out.block(format!("// component: {id}\nRectangle"), |out| {
            ctx.render_children(children, out)
        })?;
        Ok(())
    }
}
