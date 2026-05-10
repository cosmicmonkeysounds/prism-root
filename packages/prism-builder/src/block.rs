//! The unified [`Block`] trait — single source of truth for a renderable
//! block type. One declaration, one render method (`lower_ui`), shared
//! across the shell renderer and the relay's semantic-HTML SSR walker.
//!
//! A blanket impl gives every `Block` an automatic `Component` impl
//! so existing `ComponentRegistry` callers keep working unchanged.

use prism_core::help::HelpEntry;
use prism_core::widget::ToolbarAction;

use crate::component::{Component, ComponentId};
use crate::document::Node;
use crate::registry::{ComponentRegistry, FieldSpec, RegistryError};
use crate::signal::{common_signals, SignalDef};
use crate::variant::VariantAxis;

/// One block type, one declaration. Implement once; the blanket impl
/// below makes it a `Component` so it slots into `ComponentRegistry`
/// directly. Register a single instance via [`register_block`].
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
}

// ── Blanket impl so a Block is automatically a Component ──

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
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        style: &crate::style::StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        Block::lower_ui(self, ctx, node, style)
    }
}

/// Register a [`Block`] into the component registry.
pub fn register_block<T: Block + 'static>(
    components: &mut ComponentRegistry,
    block: std::sync::Arc<T>,
) -> Result<(), RegistryError> {
    components.register(block as std::sync::Arc<dyn Component>)?;
    Ok(())
}
