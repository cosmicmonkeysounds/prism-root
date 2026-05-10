//! The unified [`Block`] trait — single source of truth for a renderable
//! block type. One declaration, one render method (`lower_ui`), shared
//! across the shell renderer and the relay's semantic-HTML SSR walker.
//!
//! A blanket impl gives every `Block` an automatic `Component` impl
//! so existing `ComponentRegistry` callers keep working unchanged.
//!
//! For declarative registration without per-block trait impls, see
//! [`BlockSpec`] / [`SpecBlock`] / [`register_specs`] — that's how the
//! 17 starter builtins (see `starter.rs`) and the 48 shell primitives
//! (see `prism-shell/src/components/registry.rs`) collapse to one
//! const table apiece. Adding a block = one `const SPEC` + one row.

use std::sync::Arc;

use prism_core::help::HelpEntry;
use prism_core::widget::ToolbarAction;

use crate::component::{Component, ComponentId};
use crate::document::Node;
use crate::registry::{ComponentRegistry, FieldSpec, RegistryError};
use crate::signal::{common_signals, with_common_signals, SignalDef};
use crate::style::StyleProperties;
use crate::ui_lower::LowerCtx;
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
    block: Arc<T>,
) -> Result<(), RegistryError> {
    components.register(block as Arc<dyn Component>)?;
    Ok(())
}

// ── Declarative spec ────────────────────────────────────────────────
//
// `BlockSpec` is the data form of a `Block`. Instead of writing a
// per-block `pub struct Foo + impl Block for Foo`, declare a
// `const FOO: BlockSpec = BlockSpec::new("foo", schema_fn).lower(foo_lower)`
// and feed it through [`SpecBlock`]. Same `Block` surface, no
// per-block trait impl, no per-block struct.
//
// Used by:
// * `prism_builder::starter` — 17 starter builtins (one `BUILTINS` table)
// * `prism_shell::components::registry` — 48 shell primitives
// * any plugin / external crate that wants the same declarative shape

/// Optional help entry rendered in the inspector docs panel.
pub struct HelpDef {
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
}

/// Free-fn signature for `lower_ui`. The same shape every `impl Block`
/// uses today; lifting it to a function pointer is what lets a
/// `BlockSpec` exist as a `const`.
pub type LowerFn = fn(&LowerCtx<'_>, &Node, &StyleProperties) -> prism_ui_runtime::layout::Node;

/// Declarative `Block` description. One row per registered block.
/// `id` / `schema` are always required; `help` / `signals` /
/// `variants` / `lower` use sensible defaults — set them via the
/// const-fn builder methods.
pub struct BlockSpec {
    pub id: &'static str,
    pub schema: fn() -> Vec<FieldSpec>,
    pub help: Option<HelpDef>,
    pub signals: fn() -> Vec<SignalDef>,
    pub variants: fn() -> Vec<VariantAxis>,
    pub lower: LowerFn,
}

/// Default `signals()` — the 12 common signals only. Matches the
/// `Block::signals()` default (which calls `common_signals()`).
pub fn default_signals() -> Vec<SignalDef> {
    with_common_signals(vec![])
}
/// Default `variants()` — none.
pub fn no_variants() -> Vec<VariantAxis> {
    vec![]
}
/// Default `schema()` — empty.
pub fn no_schema() -> Vec<FieldSpec> {
    vec![]
}
/// Default `lower()` — generic container, identical to
/// `Block::lower_ui`'s default.
pub fn default_lower(
    ctx: &LowerCtx<'_>,
    node: &Node,
    style: &StyleProperties,
) -> prism_ui_runtime::layout::Node {
    ctx.default_container(node, style)
}

impl BlockSpec {
    /// New spec with `id` + `schema` only. `lower` defaults to the
    /// generic container; override via `.lower(...)`.
    pub const fn new(id: &'static str, schema: fn() -> Vec<FieldSpec>) -> Self {
        Self {
            id,
            schema,
            help: None,
            signals: default_signals,
            variants: no_variants,
            lower: default_lower,
        }
    }
    /// Convenience for blocks with no schema fields.
    pub const fn leaf(id: &'static str) -> Self {
        Self::new(id, no_schema)
    }
    pub const fn lower(mut self, f: LowerFn) -> Self {
        self.lower = f;
        self
    }
    pub const fn help(
        mut self,
        key: &'static str,
        title: &'static str,
        description: &'static str,
    ) -> Self {
        self.help = Some(HelpDef {
            key,
            title,
            description,
        });
        self
    }
    pub const fn signals(mut self, f: fn() -> Vec<SignalDef>) -> Self {
        self.signals = f;
        self
    }
    pub const fn variants(mut self, f: fn() -> Vec<VariantAxis>) -> Self {
        self.variants = f;
        self
    }
}

/// `Block` impl that delegates everything to a `&'static BlockSpec`.
/// One type, N specs, zero per-block trait impls.
pub struct SpecBlock {
    spec: &'static BlockSpec,
    id: ComponentId,
}

impl SpecBlock {
    pub fn new(spec: &'static BlockSpec) -> Self {
        Self {
            spec,
            id: spec.id.into(),
        }
    }
    pub fn arc(spec: &'static BlockSpec) -> Arc<Self> {
        Arc::new(Self::new(spec))
    }
}

impl Block for SpecBlock {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        (self.spec.schema)()
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        self.spec
            .help
            .as_ref()
            .map(|h| HelpEntry::new(h.key, h.title, h.description))
    }
    fn signals(&self) -> Vec<SignalDef> {
        (self.spec.signals)()
    }
    fn variants(&self) -> Vec<VariantAxis> {
        (self.spec.variants)()
    }
    fn lower_ui(
        &self,
        ctx: &LowerCtx<'_>,
        node: &Node,
        style: &StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        (self.spec.lower)(ctx, node, style)
    }
}

/// Register every spec in `specs` into `components`. One-line fan-out
/// for any `&[&BlockSpec]` table.
pub fn register_specs(
    components: &mut ComponentRegistry,
    specs: &[&'static BlockSpec],
) -> Result<(), RegistryError> {
    for spec in specs {
        register_block(components, SpecBlock::arc(spec))?;
    }
    Ok(())
}
