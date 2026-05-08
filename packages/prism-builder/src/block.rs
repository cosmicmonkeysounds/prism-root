//! The unified [`Block`] trait — single source of truth for a renderable
//! block type.
//!
//! Historically every built-in block type was authored twice: once as
//! [`crate::component::Component`] (Slint DSL emission for Studio's live
//! builder) and once as [`crate::html_block::HtmlBlock`] (HTML SSR for
//! `prism-relay`). The non-renderer methods (`id`, `schema`, `signals`,
//! `variants`, `help_entry`, `toolbar_actions`) were near-identical
//! across the two impls.
//!
//! `Block` collapses both into one trait with both render methods. A
//! blanket impl gives every `Block` automatic `Component` *and*
//! `HtmlBlock` impls so existing registries keep working unchanged.
//!
//! See `docs/dev/declarative-refactorings.md` for the broader context.

use prism_core::help::HelpEntry;
use prism_core::widget::ToolbarAction;
use serde_json::Value;

use crate::component::{Component, ComponentId, RenderError, RenderSlintContext};
use crate::document::Node;
use crate::html::Html;
use crate::html_block::{HtmlBlock, HtmlRegistry, HtmlRenderContext};
use crate::registry::{ComponentRegistry, FieldSpec, RegistryError};
use crate::signal::{common_signals, SignalDef};
use crate::slint_source::SlintEmitter;
use crate::variant::VariantAxis;

/// One block type, both render targets.
///
/// Implement this once; both [`Component`] and [`HtmlBlock`] are derived
/// via blanket impls below. Register a single instance into both
/// registries via [`register_block`].
pub trait Block: Send + Sync + 'static {
    fn id(&self) -> &ComponentId;
    fn schema(&self) -> Vec<FieldSpec>;

    fn help_entry(&self) -> Option<HelpEntry> {
        None
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }

    fn variants(&self) -> Vec<VariantAxis> {
        vec![]
    }

    fn toolbar_actions(&self) -> Vec<ToolbarAction> {
        vec![]
    }

    /// Slint DSL emission. Default: a transparent `Rectangle` wrapper
    /// recursing into children — matches [`Component`]'s default.
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
        })
    }

    /// Lower the block to a `prism_ui_runtime::layout::Node`. Default:
    /// generic container — same as [`Component::lower_ui`]'s default.
    /// Override on text-shaped, image-shaped, or otherwise-bespoke
    /// blocks; everything structural (containers, columns, lists)
    /// inherits the default for free.
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &crate::style::StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        ctx.default_container(node, style)
    }

    /// HTML SSR emission. Default: a `<div data-component="…">` wrapper
    /// recursing into children — matches [`HtmlBlock`]'s default.
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
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

// ── Blanket impls so a Block is automatically Component + HtmlBlock ──

impl<T: Block> Component for T {
    fn id(&self) -> &ComponentId {
        Block::id(self)
    }
    fn schema(&self) -> Vec<FieldSpec> {
        Block::schema(self)
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Block::help_entry(self)
    }
    fn signals(&self) -> Vec<SignalDef> {
        Block::signals(self)
    }
    fn variants(&self) -> Vec<VariantAxis> {
        Block::variants(self)
    }
    fn toolbar_actions(&self) -> Vec<ToolbarAction> {
        Block::toolbar_actions(self)
    }
    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        Block::render_slint(self, ctx, props, children, out)
    }
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &crate::style::StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        Block::lower_ui(self, ctx, node, style)
    }
}

impl<T: Block> HtmlBlock for T {
    fn id(&self) -> &ComponentId {
        Block::id(self)
    }
    fn schema(&self) -> Vec<FieldSpec> {
        Block::schema(self)
    }
    fn signals(&self) -> Vec<SignalDef> {
        Block::signals(self)
    }
    fn variants(&self) -> Vec<VariantAxis> {
        Block::variants(self)
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        Block::render_html(self, ctx, props, children, out)
    }
}

/// Register a [`Block`] into both registries. Every call sets up one
/// type id with both render targets in lockstep.
pub fn register_block<T: Block + 'static>(
    components: &mut ComponentRegistry,
    html: &mut HtmlRegistry,
    block: std::sync::Arc<T>,
) -> Result<(), RegistryError> {
    components.register(block.clone() as std::sync::Arc<dyn Component>)?;
    html.register(block as std::sync::Arc<dyn HtmlBlock>)?;
    Ok(())
}
