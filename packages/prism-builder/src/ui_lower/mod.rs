//! Builder→runtime lowering: `Component::lower_ui` plumbing + the
//! shared helpers every built-in block reuses.
//!
//! This is the seam called out in the Clay/Taffy migration plan
//! (`docs/dev/clay-migration-plan.md` §3, §6). The old `ui_runtime`
//! translator dispatched on `node.component` with a hard-coded string
//! match — every new block had to teach that match about itself.
//! Now each [`crate::component::Component`] knows how to lower itself
//! to a `prism_ui_runtime::layout::Node`, and the translator is just
//! "look the component up in the registry, call `lower_ui`".
//!
//! The helpers below are the *single source of truth* for the
//! translation primitives blocks reuse:
//!
//! - [`LowerCtx`] — passed to every `lower_ui` impl. Carries the
//!   inherited style cascade and the registry, exposes
//!   [`LowerCtx::lower_children`] for recursion, and
//!   [`LowerCtx::default_container`] as the generic fallback.
//! - [`container_props_from`] — turns a node's `FlowProps` + cascaded
//!   `StyleProperties` into runtime `ContainerProps`. Containers
//!   share this; bespoke blocks (cards, columns) layer on top.
//! - [`parse_color`] — `#rgb` / `#rrggbb` / `#rrggbbaa`. The cascade
//!   resolves to strings; this is where they become runtime `Color`.
//! - [`text_node`] / [`spacer_node`] — convenience constructors so
//!   `TextBlock` / `SpacerBlock` don't reimplement the same shape.
//!
//! Blocks that don't override `lower_ui` get the default container
//! lowering automatically — same behaviour the legacy `ui_runtime`
//! translator gave for unknown component ids.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use prism_core::reactive::{ReactiveContext, Signal};
use prism_ui_runtime::interpret::TagEmission;
use prism_ui_runtime::layout::{ContainerProps, Node as UiNode};
use serde_json::Value;

use crate::document::Node;
use crate::layout::LayoutMode;
use crate::modifier::ModifierRegistry;
use crate::reactive_props::DocumentBindings;
use crate::registry::ComponentRegistry;
use crate::style::{resolve_cascade, StyleProperties};

mod nodes;
pub use nodes::*;

/// **Phase 3b** of `docs/dev/dioxus-inspiration.md`: per-block reactive
/// invalidator. The host (shell, relay, tests) installs one of these
/// onto a [`LowerCtx`] via [`LowerCtx::with_block_invalidator`]; every
/// recursive `LowerCtx::lower(node)` call then wraps the block's
/// `Component::lower_ui` body in a per-NodeId
/// [`prism_core::reactive::ReactiveContext`].
///
/// Per-node contexts are cached across frames — the same NodeId reuses
/// its context via `ReactiveContext::reset_and_run_in`, which re-tracks
/// the dependency set on each lower call. When a tracked signal later
/// fires, the context's dirty callback invokes the host-supplied
/// [`on_dirty`] callback with the NodeId so the host can mark the node
/// for selective re-lower next frame.
///
/// `BlockInvalidator` is `Clone`-cheap (shared `Rc<Inner>`).
#[derive(Clone)]
pub struct BlockInvalidator {
    inner: Rc<BlockInvalidatorInner>,
}

struct BlockInvalidatorInner {
    contexts: RefCell<HashMap<String, ReactiveContext>>,
    on_dirty: Rc<dyn Fn(&str)>,
}

impl BlockInvalidator {
    /// Build an invalidator whose dirty callback fires `on_dirty(node_id)`
    /// whenever a tracked signal subscribed inside that NodeId's
    /// `lower_ui` body is later written to.
    pub fn new<F>(on_dirty: F) -> Self
    where
        F: Fn(&str) + 'static,
    {
        Self {
            inner: Rc::new(BlockInvalidatorInner {
                contexts: RefCell::new(HashMap::new()),
                on_dirty: Rc::new(on_dirty),
            }),
        }
    }

    /// Run `body` inside the per-NodeId reactive context for `node_id`.
    /// Creates the context lazily on first call for that id; on
    /// subsequent calls, the context is reused and its subscription
    /// set is rebuilt via `reset_and_run_in`.
    pub fn run_for_node<R, F>(&self, node_id: &str, body: F) -> R
    where
        F: FnOnce() -> R,
    {
        let ctx = self.ensure_context(node_id);
        ctx.reset_and_run_in(body)
    }

    fn ensure_context(&self, node_id: &str) -> ReactiveContext {
        if let Some(ctx) = self.inner.contexts.borrow().get(node_id) {
            return *ctx;
        }
        let id_owned = node_id.to_string();
        let on_dirty = Rc::clone(&self.inner.on_dirty);
        let ctx = ReactiveContext::new(move || on_dirty(&id_owned));
        self.inner
            .contexts
            .borrow_mut()
            .insert(node_id.to_string(), ctx);
        ctx
    }

    /// Reclaim the per-NodeId context — call when a node is removed
    /// from the document so the thread-local context table doesn't
    /// accumulate dead entries.
    pub fn forget_node(&self, node_id: &str) {
        if let Some(ctx) = self.inner.contexts.borrow_mut().remove(node_id) {
            ctx.dispose();
        }
    }

    /// Number of NodeId contexts currently cached. Exposed for tests
    /// and diagnostics.
    pub fn cached_len(&self) -> usize {
        self.inner.contexts.borrow().len()
    }
}

impl Drop for BlockInvalidatorInner {
    fn drop(&mut self) {
        for (_, ctx) in self.contexts.borrow_mut().drain() {
            ctx.dispose();
        }
    }
}

/// Context threaded through `Component::lower_ui` impls during the
/// `BuilderDocument` → `prism_ui_runtime::layout::Node` walk.
///
/// `parent_style` is *the cascade output for the node currently being
/// lowered* — i.e. for a block authoring its own children, calling
/// [`Self::lower_children`] cascades correctly without the block
/// knowing anything about the cascade.
pub struct LowerCtx<'a> {
    registry: Option<&'a ComponentRegistry>,
    parent_style: &'a StyleProperties,
    /// Pre-lowered children supplied by a host upstream of `lower_ui`
    /// — currently the [`crate::ui_resolver::RegistryTagResolver`]
    /// path, which lowers an element's AST children through the
    /// runtime before delegating to the registered block. Composition-
    /// style blocks (`shell.app-window`) consume this slice in
    /// preference to walking `node.children`. Plain blocks — the 12/13
    /// chrome primitives whose layout comes from props — never read
    /// it; the field is `None` on every other path. Single-seam DI:
    /// no new abstraction, no parallel context type, the existing
    /// `LowerCtx` simply carries a sparse extra slot.
    host_children: Option<&'a [UiNode]>,
    /// **Wave 13.1** — named-slot map. The resolver partitions a
    /// dispatched element's AST children by their `slot="X"` attribute,
    /// pre-lowers each bucket in the caller's scope, and threads the
    /// resulting `HashMap<slot-name, Vec<UiNode>>` here. A DSL component
    /// body reads back via `<slot name="X"/>`, which falls through to
    /// the runtime's `LowerScope::host_children_for_slot(name)` when
    /// no AST-level slot binding carries that name. Children with no
    /// `slot=` attribute continue to land in `host_children` as the
    /// default bucket — back-compat with Wave 11.2's single-slot
    /// contract.
    host_children_by_slot: Option<Arc<HashMap<String, Vec<UiNode>>>>,
    /// Tag-keyed binding emissions snapshot, threaded through from
    /// [`prism_ui_runtime::interpret::LowerScope::with_tag_emissions`].
    /// When [`Self::lower_as`] synthesises a routed content tag (the
    /// dock-panel `panel-id` path is the canonical caller) it merges
    /// the caller's props with this map's entry and threads the
    /// recorded children through as `host_children` — without this
    /// the synthesised tag is rendered with empty props and zero
    /// children, ignoring whatever the binding registered emit.
    /// `Arc<HashMap<...>>` so the field can outlive the originating
    /// `LowerScope` value (the resolver consumes scope by reference
    /// but stores an Arc clone here for child-scope propagation).
    tag_emissions: Option<Arc<HashMap<String, TagEmission>>>,
    /// **Phase 3b**: per-block reactive invalidator. When present,
    /// every recursive `lower()` call wraps its `Component::lower_ui`
    /// body in a per-NodeId reactive context. Propagates through
    /// child scopes unchanged (a single invalidator instance covers
    /// the whole document walk).
    block_invalidator: Option<BlockInvalidator>,
    /// **Phase 4b**: per-document reactive prop store. When present,
    /// every [`Self::prop`] / [`Self::prop_str`] / [`Self::prop_bool`]
    /// call goes through `bindings.props_for(node.id, ..).signal(key)`,
    /// subscribing the current per-block reactive context (Phase 3b)
    /// so a later write to the prop signal marks the node dirty.
    /// `None` for headless tests + the non-reactive SSR path — those
    /// callers read directly from `node.props` and don't subscribe.
    bindings: Option<&'a DocumentBindings>,
    /// **Wave 1** of `docs/dev/composable-builder-plan.md`: open
    /// registry of `ModifierBehaviour` impls. When installed, every
    /// recursive [`Self::lower`] call folds the lowered child through
    /// `node.modifiers` innermost-first, calling each registered
    /// behaviour's `wrap`. `enabled = false` skips the wrap; unknown
    /// ids fall through unchanged. `None` on headless / SSR paths —
    /// those callers render the bare component output without
    /// behaviour wrapping. Propagates into recursive child scopes
    /// unchanged (one registry per document walk).
    modifier_registry: Option<&'a ModifierRegistry>,
}

impl<'a> LowerCtx<'a> {
    /// Build a context anchored at a specific parent cascade. Callers
    /// that don't have a meaningful parent style (i.e. the root of a
    /// document) pass a borrow to a `StyleProperties::default()`.
    pub fn new(registry: Option<&'a ComponentRegistry>, parent_style: &'a StyleProperties) -> Self {
        Self {
            registry,
            parent_style,
            host_children: None,
            host_children_by_slot: None,
            tag_emissions: None,
            block_invalidator: None,
            bindings: None,
            modifier_registry: None,
        }
    }

    /// Install a [`BlockInvalidator`] — every recursive `lower()`
    /// call wraps its `Component::lower_ui` body in a per-NodeId
    /// `ReactiveContext` so signal reads inside the block body
    /// subscribe and writes drive the host's `on_dirty(node_id)`
    /// callback. **Phase 3b** of
    /// `docs/dev/dioxus-inspiration.md`.
    pub fn with_block_invalidator(mut self, invalidator: BlockInvalidator) -> Self {
        self.block_invalidator = Some(invalidator);
        self
    }

    /// The currently installed block invalidator, if any. Exposed for
    /// composition-style blocks (e.g. shell chrome wrappers) that
    /// recursively lower host-provided subtrees and need to share the
    /// same invalidator with the inner pass.
    pub fn block_invalidator(&self) -> Option<&BlockInvalidator> {
        self.block_invalidator.as_ref()
    }

    /// Install a [`DocumentBindings`] — every `prop_*` read on this
    /// context (and on every recursively-derived child) goes through
    /// `bindings.props_for(node.id, ..).signal(key)`, subscribing the
    /// current per-block reactive context (Phase 3b). Writes through
    /// [`crate::mutator::NodeMutator`] reach the same signal and wake
    /// subscribers automatically. **Phase 4b** of
    /// `docs/dev/dioxus-inspiration.md`.
    pub fn with_bindings(mut self, bindings: &'a DocumentBindings) -> Self {
        self.bindings = Some(bindings);
        self
    }

    /// The currently installed reactive prop store, if any.
    pub fn bindings(&self) -> Option<&DocumentBindings> {
        self.bindings
    }

    /// Install a [`ModifierRegistry`] — every recursive [`Self::lower`]
    /// call folds the lowered child through `node.modifiers` via the
    /// matching behaviour's `wrap`. **Wave 1** of
    /// `docs/dev/composable-builder-plan.md`.
    pub fn with_modifier_registry(mut self, registry: &'a ModifierRegistry) -> Self {
        self.modifier_registry = Some(registry);
        self
    }

    /// The currently installed modifier registry, if any.
    pub fn modifier_registry(&self) -> Option<&ModifierRegistry> {
        self.modifier_registry
    }

    /// Builder-style installer for host-supplied pre-lowered children.
    /// Composes with the existing [`Self::new`] surface — child scopes
    /// (control-flow forks, recursive `lower` calls) deliberately do
    /// **not** inherit this slot, since the children belong to the one
    /// block the resolver is delegating to.
    pub fn with_host_children(mut self, children: &'a [UiNode]) -> Self {
        self.host_children = Some(children);
        self
    }

    /// **Wave 13.1** — install the named-slot map. Composition-style
    /// blocks (`shell.app-window`) thread this through to the DSL
    /// loader, which seeds the runtime `LowerScope::host_children_by_slot`
    /// so `<slot name="X"/>` reads pull from the right bucket.
    pub fn with_host_children_by_slot(mut self, slots: Arc<HashMap<String, Vec<UiNode>>>) -> Self {
        self.host_children_by_slot = Some(slots);
        self
    }

    /// **Wave 13.1** — the named-slot bucket map, if a host upstream
    /// supplied one. Clone-cheap (`Arc`); callers forward into a
    /// runtime `LowerScope` via
    /// `LowerScope::with_host_children_by_slot(ctx.host_children_by_slot().cloned())`.
    pub fn host_children_by_slot(&self) -> Option<&Arc<HashMap<String, Vec<UiNode>>>> {
        self.host_children_by_slot.as_ref()
    }

    /// Install the tag-keyed emissions map snapshot. The resolver hands
    /// this through from
    /// [`prism_ui_runtime::interpret::LowerScope::tag_emissions_arc`];
    /// it propagates into every child `LowerCtx` constructed inside
    /// `lower_as` / `lower` so routed content several layers deep
    /// still inherits the live binding data.
    pub fn with_tag_emissions(mut self, emissions: Arc<HashMap<String, TagEmission>>) -> Self {
        self.tag_emissions = Some(emissions);
        self
    }

    /// Look up the binding emission recorded under `tag`, if any.
    /// Returns `None` either when no map was installed (headless
    /// tests, no-DI render paths) or when the tag was never registered
    /// — the caller falls through to its synthesised default.
    pub fn tag_emission(&self, tag: &str) -> Option<&TagEmission> {
        self.tag_emissions.as_ref().and_then(|m| m.get(tag))
    }

    /// Borrow the underlying tag-emissions map so a DSL block's
    /// `lower_ui` can forward it onto its inner `LowerScope`. Without
    /// this propagation a `<dispatch component="{expr}"/>` element
    /// inside a DSL body never sees the live binding props for the
    /// dispatched tag — the dock-panel migration relies on this for
    /// `shell.component-palette` / `shell.properties-panel` / every
    /// other shell tag the dock workspace routes to. Wave 11.3
    /// addition.
    pub fn tag_emissions_arc(&self) -> Option<Arc<HashMap<String, TagEmission>>> {
        self.tag_emissions.clone()
    }

    /// Pre-lowered children, if a host upstream of `lower_ui`
    /// supplied any. Composition blocks read this in preference to
    /// walking `node.children`:
    ///
    /// ```ignore
    /// let kids = ctx
    ///     .host_children()
    ///     .map(|s| s.to_vec())
    ///     .unwrap_or_else(|| ctx.lower_children(&node.children));
    /// ```
    pub fn host_children(&self) -> Option<&[UiNode]> {
        self.host_children
    }

    /// Lower a single node. The cascade is resolved internally and a
    /// fresh child-scope `LowerCtx` is handed to whichever
    /// `Component::lower_ui` impl owns this node's component id.
    /// Unknown ids fall back to [`Self::default_container`].
    ///
    /// **Wave 1** modifier fold: after the component lowers, the
    /// resulting `UiNode` is folded through `node.modifiers`
    /// innermost-first via each modifier's `ModifierBehaviour::wrap`.
    /// Disabled modifiers (`enabled = false`) skip; unknown ids fall
    /// through unchanged. Headless / SSR paths with no
    /// `modifier_registry` skip the fold entirely.
    pub fn lower(&self, node: &Node) -> UiNode {
        let style = resolve_cascade(self.parent_style, &StyleProperties::default(), &node.style);
        // Note: host_children is intentionally not propagated — it
        // belongs to the block currently being resolved, not its
        // recursive sub-children. tag_emissions *is* propagated:
        // it's a snapshot keyed by tag, valid for the entire pass.
        // block_invalidator IS propagated: one invalidator instance
        // covers the whole document walk; per-NodeId contexts are
        // managed inside the invalidator. modifier_registry IS
        // propagated: one registry covers the whole walk.
        let child = LowerCtx {
            registry: self.registry,
            parent_style: &style,
            host_children: None,
            host_children_by_slot: None,
            tag_emissions: self.tag_emissions.clone(),
            block_invalidator: self.block_invalidator.clone(),
            bindings: self.bindings,
            modifier_registry: self.modifier_registry,
        };
        // Phase 3b: wrap the `Component::lower_ui` call in a per-NodeId
        // reactive context when an invalidator is installed. Signal
        // reads inside the block body subscribe automatically; later
        // writes invoke the invalidator's `on_dirty(node_id)` callback.
        let body = || -> UiNode {
            if let Some(reg) = self.registry {
                if let Some(comp) = reg.get(&node.component) {
                    return comp.lower_ui(&child, node, &style);
                }
            }
            child.default_container(node, &style)
        };
        let lowered = match &self.block_invalidator {
            Some(inv) if !node.id.is_empty() => inv.run_for_node(&node.id, body),
            _ => body(),
        };
        // Wave 1 modifier fold. Innermost-first means later entries in
        // `node.modifiers` wrap earlier entries — `iter().rev()`
        // produces the right ordering for `fold`.
        match self.modifier_registry {
            Some(reg) if !node.modifiers.is_empty() => {
                node.modifiers.iter().rev().fold(lowered, |child, m| {
                    if !m.enabled {
                        return child;
                    }
                    match reg.get(&m.kind) {
                        Some(beh) => beh.wrap(m, child),
                        None => child,
                    }
                })
            }
            _ => lowered,
        }
    }

    /// Recurse into a slice of children with this context's cascade
    /// as their parent. Blocks that wrap their children call this.
    pub fn lower_children(&self, children: &[Node]) -> Vec<UiNode> {
        children.iter().map(|c| self.lower(c)).collect()
    }

    /// Generic container lowering — what `Component::lower_ui` falls
    /// back to when a block doesn't override the method. Mirrors the
    /// pre-migration `translate_container` behaviour exactly.
    pub fn default_container(&self, node: &Node, style: &StyleProperties) -> UiNode {
        self.container_with(node, style, |_| {})
    }

    /// Declarative container lowering. Builds the same `UiNode::Container`
    /// [`Self::default_container`] would, threading cascade + flow props
    /// through [`container_props_from`], then hands the resulting
    /// `ContainerProps` to `customize` so a block can tweak the few
    /// fields it actually owns (direction, gap, padding, background…)
    /// without restating the whole construction.
    ///
    /// This is the seam every "I'm a container with one knob different"
    /// block uses — `ColumnsBlock` flips direction to `Row`,
    /// `ListBlock` overrides `gap`, `ContainerBlock` adds padding and
    /// border-derived background, etc. The cascade, sizing,
    /// colour-parsing, and child recursion live exactly once (here +
    /// in `container_props_from`); blocks contribute only their
    /// difference.
    pub fn container_with(
        &self,
        node: &Node,
        style: &StyleProperties,
        customize: impl FnOnce(&mut ContainerProps),
    ) -> UiNode {
        let flow = match &node.layout_mode {
            LayoutMode::Flow(f) | LayoutMode::Relative(f) => Some(f),
            _ => None,
        };
        let mut props = container_props_from(flow, style);
        customize(&mut props);
        UiNode::Container {
            id: node.id.clone(),
            props,
            children: self.lower_children(&node.children),
        }
    }

    /// Like [`Self::container_with`] but for blocks that synthesise
    /// children (a button rendering its own label, a code block
    /// rendering pre-formatted text) rather than walking
    /// `node.children`. Saves the per-block "build a container with
    /// these children and these prop tweaks" boilerplate.
    pub fn synthetic_container(
        &self,
        node: &Node,
        style: &StyleProperties,
        children: Vec<UiNode>,
        customize: impl FnOnce(&mut ContainerProps),
    ) -> UiNode {
        let flow = match &node.layout_mode {
            LayoutMode::Flow(f) | LayoutMode::Relative(f) => Some(f),
            _ => None,
        };
        let mut props = container_props_from(flow, style);
        customize(&mut props);
        UiNode::Container {
            id: node.id.clone(),
            props,
            children,
        }
    }

    /// Cascade output the *current scope* sees as its inherited
    /// style. Useful for blocks that need to peek at parent values
    /// without owning the cascade machinery.
    pub fn parent_style(&self) -> &StyleProperties {
        self.parent_style
    }

    /// The live [`ComponentRegistry`] the cascade walk is dispatching
    /// against. Exposed so composition seams (the Wave 11.2 `.prui`
    /// shell-component loader, future DSL-authored prefab hosts) can
    /// build a fresh [`crate::ui_resolver::RegistryTagResolver`] over
    /// the same id namespace the calling render walk uses. `None` on
    /// headless / no-DI paths (matching the existing
    /// [`Self::new`] signature).
    pub fn registry(&self) -> Option<&ComponentRegistry> {
        self.registry
    }

    /// Subscribing read of one of `node`'s props. When a
    /// [`DocumentBindings`] is installed on this scope (Phase 4b), the
    /// read goes through `bindings.props_for(node.id, &node.props).signal(key)`,
    /// subscribing the current reactive context — a later write to the
    /// same prop (through [`crate::mutator::NodeMutator`] or any signal
    /// writer pointing at the same bag) marks the node dirty. Without
    /// bindings, falls back to a direct, non-subscribing `node.props`
    /// read so headless tests / no-reactivity SSR keep working.
    ///
    /// Returns `Value::Null` when the key isn't present. Block authors
    /// typically prefer the typed accessors ([`Self::prop_str`],
    /// [`Self::prop_bool`]); this is the escape hatch for object /
    /// array values.
    pub fn prop(&self, node: &Node, key: &str) -> Value {
        match self.bindings {
            Some(b) if !node.id.is_empty() => {
                let bag = b.props_for(&node.id, &node.props);
                bag.signal(key).get()
            }
            _ => node.props.get(key).cloned().unwrap_or(Value::Null),
        }
    }

    /// Raw reactive signal handle for `node.props[key]`. Returns `None`
    /// when no [`DocumentBindings`] is installed on this scope.
    /// Advanced consumers (memos that derive across multiple props,
    /// effects authored outside `lower_ui`) hold this directly; chrome
    /// blocks normally use [`Self::prop_str`] / [`Self::prop_bool`].
    pub fn prop_signal(&self, node: &Node, key: &str) -> Option<Signal<Value>> {
        let b = self.bindings?;
        if node.id.is_empty() {
            return None;
        }
        Some(b.props_for(&node.id, &node.props).signal(key))
    }

    /// Subscribing read of a string-valued prop. Returns an empty
    /// `String` when missing or not a string. Owned (rather than
    /// borrowed-from-`Node`) so the call site composes uniformly
    /// whether reading from JSON or from a reactive signal — the
    /// latter doesn't hand out borrows into the cell.
    pub fn prop_str(&self, node: &Node, key: &str) -> String {
        match self.bindings {
            Some(b) if !node.id.is_empty() => b
                .props_for(&node.id, &node.props)
                .signal(key)
                .read(|v| v.as_str().unwrap_or("").to_string()),
            _ => node
                .props
                .get(key)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }
    }

    /// Subscribing read of a bool-valued prop with a fallback.
    pub fn prop_bool(&self, node: &Node, key: &str, default: bool) -> bool {
        match self.bindings {
            Some(b) if !node.id.is_empty() => b
                .props_for(&node.id, &node.props)
                .signal(key)
                .read(|v| v.as_bool().unwrap_or(default)),
            _ => node
                .props
                .get(key)
                .and_then(|v| v.as_bool())
                .unwrap_or(default),
        }
    }

    /// Phase 4b follow-up: a typed `Memo<String>` derived from the
    /// `Value`-typed `prop_signal`. Advanced consumers (cross-block
    /// effects, derived bindings) that want a `Copy + 'static`-able
    /// reactive read of a string prop hold one of these; chrome
    /// blocks still reach for the eager `prop_str` accessor.
    ///
    /// Returns `None` when no [`DocumentBindings`] is installed
    /// (headless tests, SSR) — the call site falls back to a direct
    /// JSON read in that branch.
    pub fn prop_memo_str(
        &self,
        node: &Node,
        key: &str,
    ) -> Option<prism_core::reactive::Memo<String>> {
        let owner = self.bindings?.owner();
        let sig = self.prop_signal(node, key)?;
        Some(owner.insert_memo(move || sig.read(|v| v.as_str().unwrap_or("").to_string())))
    }

    /// Typed `Memo<bool>` companion to [`Self::prop_memo_str`]. Same
    /// `None` semantics: returns `None` without a bindings install.
    pub fn prop_memo_bool(
        &self,
        node: &Node,
        key: &str,
        default: bool,
    ) -> Option<prism_core::reactive::Memo<bool>> {
        let owner = self.bindings?.owner();
        let sig = self.prop_signal(node, key)?;
        Some(owner.insert_memo(move || sig.read(|v| v.as_bool().unwrap_or(default))))
    }

    /// Resolve and lower an *embedded* block by its registered component
    /// id, synthesising a derived `Node` from a JSON props value. The
    /// dispatch goes through whichever [`ComponentRegistry`] is on this
    /// `LowerCtx`, so a host that registers an alternative
    /// `shell.menu-bar-row` impl transparently overrides the default —
    /// no `lower_ui` call site has to import a concrete block type.
    ///
    /// Returns `None` when the ctx has no registry, or the id is
    /// unregistered. Composition-style blocks (`AppWindow`, future
    /// `TabPanel`, …) call this for each chrome region they host and
    /// fall back to a placeholder when None — the structural shape
    /// stays correct under headless / no-registry test contexts, and
    /// production paths (resolver-driven, registry-attached) get full
    /// rendering with zero block-type knowledge baked into the host.
    ///
    /// Smart pattern: the *only* way to embed one registered block
    /// inside another's lowering. Eliminates the "import the impl,
    /// instantiate it manually, build a fresh `LowerCtx`" duplication
    /// that AppWindow originally carried — that pattern bypassed the
    /// registry and silently ignored host-side overrides.
    pub fn lower_as(
        &self,
        component_id: &str,
        derived_id: impl Into<String>,
        props: serde_json::Value,
    ) -> Option<UiNode> {
        let reg = self.registry?;
        let comp = reg.get(component_id)?;
        // Pull the host's emission for this tag, if any. The caller's
        // own props win (so dock-panel can still pass `{ panel-id: ...
        // }` and have it stick); every key the caller didn't set falls
        // back to the binding's. Children are wholesale — no merge.
        let emission = self.tag_emission(component_id);
        let merged_props = merge_with_emission_props(props, emission.map(|e| &e.props));
        let derived = Node {
            id: derived_id.into(),
            component: component_id.into(),
            props: merged_props,
            children: Vec::new(),
            layout_mode: LayoutMode::default(),
            transform: prism_core::foundation::spatial::Transform2D::default(),
            modifiers: Vec::new(),
            style: StyleProperties::default(),
        };
        let style = resolve_cascade(
            self.parent_style,
            &StyleProperties::default(),
            &derived.style,
        );
        // Only thread an emission's children through as host_children
        // when it actually carries any. The shell registers an
        // auto-stub `{}`-props binding for every `SHELL_BUILTINS` row
        // that doesn't get a live binding (e.g. `shell.dock-panel`,
        // `shell.menu-item`, every per-row leaf). If we propagated
        // those empty children slices, callers that compose the tag
        // via `lower_as` (the dock-workspace routing path) would see
        // `ctx.host_children() == Some(&[])` and short-circuit their
        // fallback recursion. Treating "empty" as "no override"
        // preserves the original routing semantics.
        let host_children = emission
            .map(|e| e.children.as_slice())
            .filter(|s| !s.is_empty());
        let child = LowerCtx {
            registry: self.registry,
            parent_style: &style,
            host_children,
            host_children_by_slot: None,
            tag_emissions: self.tag_emissions.clone(),
            block_invalidator: self.block_invalidator.clone(),
            bindings: self.bindings,
            modifier_registry: self.modifier_registry,
        };
        // Phase 3b: wrap the synthesised composition's lower in a
        // per-NodeId reactive context too, scoped on the *derived* id.
        let body = || comp.lower_ui(&child, &derived, &style);
        let lowered = match &self.block_invalidator {
            Some(inv) if !derived.id.is_empty() => inv.run_for_node(&derived.id, body),
            _ => body(),
        };
        Some(lowered)
    }
}

/// Overlay caller-provided props on top of a binding emission. The
/// caller's keys win (so explicit `panel-id="builder"` is preserved
/// even when the `shell.dock-panel` binding also emits one) and any
/// emission key absent from the caller drops in. Returns a JSON
/// `Object` even when both sides are empty so downstream code that
/// expects an object shape (every chrome block does) keeps working.
fn merge_with_emission_props(
    caller: serde_json::Value,
    emission: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut map = match caller {
        serde_json::Value::Object(m) => m,
        // Caller passed a non-object (a leaf string, an array): treat
        // it as "no props" and just hand back the emission's object.
        // Same shape every chrome block reads through `node.props.get(...)`.
        _ => serde_json::Map::new(),
    };
    if let Some(serde_json::Value::Object(em)) = emission {
        for (k, v) in em {
            if !map.contains_key(k) {
                map.insert(k.clone(), v.clone());
            }
        }
    }
    serde_json::Value::Object(map)
}

#[cfg(test)]
mod tests;
