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
use prism_ui_runtime::command::{Color, CornerRadius};
use prism_ui_runtime::interpret::TagEmission;
use prism_ui_runtime::layout::{
    ContainerProps, Direction, HoverOverrides, Node as UiNode, Padding, Semantic, Sizing, TextProps,
};
use serde_json::Value;

use crate::document::Node;
use crate::layout::{Dimension, FlexDirection, FlowProps, LayoutMode};
use crate::modifier::ModifierRegistry;
use crate::reactive_props::DocumentBindings;
use crate::registry::ComponentRegistry;
use crate::style::{resolve_cascade, StyleProperties};

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
    /// against. Exposed so composition seams (the Wave 11.2 `.prism-ui`
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

/// Build a `UiNode::Container` *without* going through a builder
/// `Node`. Used by composite blocks that synthesise nested sub-trees
/// (table headers, tab strips, accordion bars) where there's no
/// `Node` to drive cascade resolution from.
///
/// Defaults to a zero-padded, no-background, fit-sized container —
/// the closure is the *only* way fields move off the default. This
/// keeps every "build a styled box with these children" call
/// boilerplate-free at the call site.
pub fn bare_container(
    id: impl Into<String>,
    children: Vec<UiNode>,
    customize: impl FnOnce(&mut ContainerProps),
) -> UiNode {
    let mut props = ContainerProps::default();
    customize(&mut props);
    UiNode::Container {
        id: id.into(),
        props,
        children,
    }
}

/// Attach a [`Semantic`] hint to whichever variant carries one. Used
/// by blocks to declare SSR markup (`<h1>`, `<section>`, alt text)
/// alongside layout vocabulary, in the same `lower_ui` impl, with no
/// per-block walker. `Spacer` ignores the hint (no semantic field).
pub fn with_semantic(node: UiNode, semantic: Semantic) -> UiNode {
    match node {
        UiNode::Container {
            id,
            mut props,
            children,
        } => {
            props.semantic = semantic;
            UiNode::Container {
                id,
                props,
                children,
            }
        }
        UiNode::Text {
            id,
            content,
            mut props,
        } => {
            props.semantic = semantic;
            UiNode::Text { id, content, props }
        }
        UiNode::Image {
            id,
            source,
            width,
            height,
            radius,
            tint,
            ..
        } => UiNode::Image {
            id,
            source,
            width,
            height,
            radius,
            tint,
            semantic,
        },
        UiNode::TextInput {
            id,
            value,
            placeholder,
            props,
            width,
            height,
            radius,
            focused,
            ..
        } => UiNode::TextInput {
            id,
            value,
            placeholder,
            props,
            width,
            height,
            radius,
            semantic,
            focused,
        },
        UiNode::Spacer { .. } => node,
    }
}

/// Build a [`UiNode::Text`] with the cascade colour overridden by an
/// explicit per-block colour string. The "clone the cascade and stamp
/// `color`" dance shows up at every chrome primitive that paints a
/// label in a non-cascade tint (section-header label/badge, toast
/// title/body, future docs/app-card text). Centralising it keeps
/// every call site to one line and frees blocks from owning a tiny
/// private helper for the same shape.
///
/// The colour string follows the same vocabulary as [`parse_color`]
/// (`#rgb` / `#rrggbb` / `#rrggbbaa`); unparseable values silently
/// fall through to the cascade (same shape `text_node` itself uses
/// when `style.color` doesn't parse).
pub fn colored_text_node(
    node_id: String,
    content: String,
    style: &StyleProperties,
    default_size: f32,
    color: &str,
) -> UiNode {
    let mut scoped = style.clone();
    scoped.color = Some(color.into());
    text_node(node_id, content, &scoped, default_size)
}

/// One-line constructor for the most common interactive-primitive
/// hover shape: "swap the background only". Returns `None` when the
/// colour string fails to parse so the caller can `props.hover = ...`
/// unconditionally without a `parse_color`/`HoverOverrides` two-liner
/// at every call site. The full `HoverOverrides` struct stays
/// available for primitives that animate radius / future fields too.
pub fn hover_bg(color: &str) -> Option<HoverOverrides> {
    parse_color(color).map(|c| HoverOverrides {
        background: Some(c),
        radius: None,
    })
}

/// Convenience: equal corner radius on all four corners. Most blocks
/// want this; the long-form struct literal is noise.
pub fn uniform_radius(r: f32) -> CornerRadius {
    CornerRadius {
        tl: r,
        tr: r,
        br: r,
        bl: r,
    }
}

/// Shared "this surface responds to a pointer" tint. Every clickable
/// chrome surface uses the same intensity so hover reads identically
/// across the whole window — palette rows, inspector rows, canvas-doc
/// nodes, field-editor rows, menu pills. Authors who need a stronger
/// or softer tint can still pass any colour to [`hover_bg`] directly.
pub const POINTER_HOVER_TINT: &str = "#1a0060c0";

/// One-liner for the "clickable chrome surface" recipe — bundles
/// the three things every routable container needs into a single
/// call: a hover-bg tint, `data-role`, and (optional) `data-target-id`.
///
/// ```ignore
/// bare_container(node.id.clone(), kids, |p| {
///     p.padding = Padding::all(8.0);
///     p.radius = uniform_radius(4.0);
///     pointer_routing(p, "my-row", target_id);
/// });
/// ```
///
/// The caller still owns the rest of the `Semantic` shape (`tag`,
/// `aria-*`, custom `data-*`) — `pointer_routing` only appends the
/// two routing attrs and the hover tint, so existing builder calls
/// compose cleanly. The function deliberately takes `&mut props` so
/// it threads naturally through `bare_container`'s closure shape.
pub fn pointer_routing(props: &mut ContainerProps, role: &'static str, target_id: &str) {
    props.hover = hover_bg(POINTER_HOVER_TINT);
    let semantic = std::mem::take(&mut props.semantic);
    let mut s = semantic.with_attr("data-role", role);
    if !target_id.is_empty() {
        s = s.with_attr("data-target-id", target_id.to_string());
    }
    props.semantic = s;
}

/// Build runtime `ContainerProps` from a node's `FlowProps` + cascade.
/// Single source of truth — every container-shaped block routes here.
pub fn container_props_from(flow: Option<&FlowProps>, style: &StyleProperties) -> ContainerProps {
    let direction = flow
        .map(|f| match f.flex_direction {
            FlexDirection::Row | FlexDirection::RowReverse => Direction::Row,
            FlexDirection::Column | FlexDirection::ColumnReverse => Direction::Column,
        })
        .unwrap_or_default();

    let gap = flow.map(|f| f.gap).unwrap_or(0.0);

    let padding = flow
        .map(|f| Padding {
            left: f.padding.left,
            right: f.padding.right,
            top: f.padding.top,
            bottom: f.padding.bottom,
        })
        .unwrap_or_default();

    let width = flow
        .map(|f| sizing_from_dimension(f.width, f.flex_grow))
        .unwrap_or_default();
    let height = flow
        .map(|f| sizing_from_dimension(f.height, f.flex_grow))
        .unwrap_or_default();

    let background = style.background.as_deref().and_then(parse_color);
    let radius = style
        .border_radius
        .map(|r| CornerRadius {
            tl: r,
            tr: r,
            br: r,
            bl: r,
        })
        .unwrap_or_default();

    ContainerProps {
        direction,
        gap,
        padding,
        width,
        height,
        background,
        radius,
        ..Default::default()
    }
}

/// Map a builder `Dimension` + `flex_grow` to a runtime `Sizing`.
pub fn sizing_from_dimension(dim: Dimension, flex_grow: f32) -> Sizing {
    match dim {
        Dimension::Px { value } => Sizing::Fixed(value),
        Dimension::Auto if flex_grow > 0.0 => Sizing::Grow,
        Dimension::Percent { value } => Sizing::Percent((value / 100.0).clamp(0.0, 1.0)),
        Dimension::Auto => Sizing::Fit,
    }
}

/// Construct a `UiNode::Text` with cascade-resolved size/color and a
/// per-block default font size (paragraph / heading / code differ).
pub fn text_node(
    node_id: String,
    content: String,
    style: &StyleProperties,
    default_size: f32,
) -> UiNode {
    let font_size = style.font_size.unwrap_or(default_size);
    let color = style
        .color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(DEFAULT_TEXT_COLOR);
    UiNode::Text {
        id: node_id,
        content,
        props: TextProps {
            font_size,
            color,
            ..Default::default()
        },
    }
}

/// Construct a `UiNode::Spacer`. Trivial wrapper, kept here so all
/// node constructors live in one place.
pub fn spacer_node(node_id: String, width: f32, height: f32) -> UiNode {
    UiNode::Spacer {
        id: node_id,
        width,
        height,
    }
}

/// Construct a `UiNode::TextInput` — the editable single-line input
/// leaf. Same builder shape as [`text_node`] / [`image_node`]: the
/// caller hands over the few fields that vary, and cascade-resolved
/// font_size / colour are pulled from `style`. `value` is what the
/// user has typed (often empty); `placeholder` paints when the value
/// is empty. Width / height drive the runtime `Sizing` policy — most
/// inputs want `Sizing::Grow` along the parent's main axis.
pub fn text_input_node(
    node_id: String,
    value: String,
    placeholder: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
    default_size: f32,
) -> UiNode {
    text_input_node_with_focus(
        node_id,
        value,
        placeholder,
        style,
        width,
        height,
        default_size,
        false,
    )
}

/// Variant of [`text_input_node`] that lets the caller mark the input
/// as the active focus target. The paint pass uses this to bump the
/// border to the accent colour and draw a 1-px caret bar after the
/// rendered text. Field-editor rows wire this through their
/// `focused` prop so users can see where their keystrokes will land.
#[allow(clippy::too_many_arguments)]
pub fn text_input_node_with_focus(
    node_id: String,
    value: String,
    placeholder: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
    default_size: f32,
    focused: bool,
) -> UiNode {
    let font_size = style.font_size.unwrap_or(default_size);
    let color = style
        .color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(DEFAULT_TEXT_COLOR);
    UiNode::TextInput {
        id: node_id,
        value,
        placeholder,
        props: TextProps {
            font_size,
            color,
            ..Default::default()
        },
        width,
        height,
        radius: style.border_radius.map(uniform_radius).unwrap_or_default(),
        semantic: Semantic::default(),
        focused,
    }
}

/// Construct a `UiNode::Image`. Width/height default to `Grow` so an
/// image inside a sized container fills its slot.
pub fn image_node(
    node_id: String,
    source: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
) -> UiNode {
    let radius = style.border_radius.map(uniform_radius).unwrap_or_default();
    UiNode::Image {
        id: node_id,
        source,
        width,
        height,
        radius,
        tint: None,
        semantic: prism_ui_runtime::layout::Semantic::default(),
    }
}

/// Construct a `UiNode::Image` with a colour tint applied. Tint
/// instructs the renderer to mask-paint the colour through the image
/// (canonical icon-tint pattern). Same default sizing rules as
/// [`image_node`] — `Grow`/`Grow` so an icon inside a sized container
/// fills its slot.
pub fn tinted_image_node(
    node_id: String,
    source: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
    tint: prism_ui_runtime::command::Color,
) -> UiNode {
    let radius = style.border_radius.map(uniform_radius).unwrap_or_default();
    UiNode::Image {
        id: node_id,
        source,
        width,
        height,
        radius,
        tint: Some(tint),
        semantic: prism_ui_runtime::layout::Semantic::default(),
    }
}

const DEFAULT_TEXT_COLOR: Color = Color {
    r: 20,
    g: 20,
    b: 20,
    a: 255,
};

/// Tiny CSS-color parser — `#rgb`, `#rrggbb`, `#rrggbbaa`. Anything
/// else returns `None` and the caller falls back to a default. Richer
/// parsing (named colours, `rgb(...)`, `oklch(...)`) lands with the
/// design-tokens cascade wiring.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    let hex = s.strip_prefix('#')?;
    let bytes = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            [r * 17, g * 17, b * 17, 255]
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            [r, g, b, 255]
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            [r, g, b, a]
        }
        _ => return None,
    };
    Some(Color {
        r: bytes[0],
        g: bytes[1],
        b: bytes[2],
        a: bytes[3],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_as_resolves_through_registry_when_attached() {
        use crate::block::{register_block, Block};
        use crate::registry::{ComponentRegistry, FieldSpec};
        use crate::ComponentId;
        use prism_ui_runtime::layout::{ContainerProps, Sizing};
        use std::sync::Arc;

        struct Tag {
            id: ComponentId,
        }
        impl Block for Tag {
            fn id(&self) -> &ComponentId {
                &self.id
            }
            fn schema(&self) -> Vec<FieldSpec> {
                vec![]
            }
            fn lower_ui(&self, _: &LowerCtx<'_>, node: &Node, _: &StyleProperties) -> UiNode {
                UiNode::Container {
                    id: node.id.clone(),
                    props: ContainerProps {
                        width: Sizing::Fixed(99.0),
                        ..Default::default()
                    },
                    children: vec![],
                }
            }
        }

        let mut reg = ComponentRegistry::new();
        register_block(
            &mut reg,
            Arc::new(Tag {
                id: "demo.tag".into(),
            }),
        )
        .unwrap();
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade);

        let out = ctx
            .lower_as("demo.tag", "derived", serde_json::json!({}))
            .expect("registered tag resolves");
        let UiNode::Container { id, props, .. } = out else {
            panic!()
        };
        assert_eq!(id, "derived");
        assert_eq!(props.width, Sizing::Fixed(99.0));
    }

    #[test]
    fn lower_as_returns_none_when_no_registry() {
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        assert!(ctx
            .lower_as("anything", "x", serde_json::json!({}))
            .is_none());
    }

    #[test]
    fn lower_as_returns_none_when_id_unregistered() {
        use crate::registry::ComponentRegistry;
        let reg = ComponentRegistry::new();
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade);
        assert!(ctx
            .lower_as("never.registered", "x", serde_json::json!({}))
            .is_none());
    }

    #[test]
    fn block_invalidator_subscribes_signal_reads_inside_lower_ui() {
        // Phase 3b: a block's `lower_ui` body that reads a reactive
        // Signal auto-subscribes a per-NodeId reactive context;
        // a later signal write fires the invalidator's on_dirty
        // callback with that NodeId. The signal lives in a thread-
        // local so the Block struct stays Send+Sync as required by
        // the Block trait.
        use crate::block::{register_block, Block};
        use crate::registry::{ComponentRegistry, FieldSpec};
        use crate::ComponentId;
        use prism_core::reactive::{Owner, Signal};
        use prism_ui_runtime::layout::{ContainerProps, Sizing};
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;
        use std::sync::Arc;

        thread_local! {
            static SIG: Cell<Option<Signal<i32>>> = const { Cell::new(None) };
        }

        struct Reader {
            id: ComponentId,
        }
        impl Block for Reader {
            fn id(&self) -> &ComponentId {
                &self.id
            }
            fn schema(&self) -> Vec<FieldSpec> {
                vec![]
            }
            fn lower_ui(&self, _: &LowerCtx<'_>, node: &Node, _: &StyleProperties) -> UiNode {
                // Read the thread-local signal inside the lower body.
                SIG.with(|s| {
                    if let Some(sig) = s.get() {
                        let _ = sig.get();
                    }
                });
                UiNode::Container {
                    id: node.id.clone(),
                    props: ContainerProps {
                        width: Sizing::Fixed(1.0),
                        ..Default::default()
                    },
                    children: vec![],
                }
            }
        }

        let outer = Owner::new();
        let sig = outer.insert(0_i32);
        SIG.with(|s| s.set(Some(sig)));

        let mut reg = ComponentRegistry::new();
        register_block(
            &mut reg,
            Arc::new(Reader {
                id: "test.reader".into(),
            }),
        )
        .unwrap();

        let dirty: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let dirty_cb = Rc::clone(&dirty);
        let invalidator =
            BlockInvalidator::new(move |id: &str| dirty_cb.borrow_mut().push(id.to_string()));

        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade).with_block_invalidator(invalidator.clone());
        let node = Node {
            id: "node-A".into(),
            component: "test.reader".into(),
            ..Default::default()
        };
        let _ = ctx.lower(&node);
        assert!(dirty.borrow().is_empty(), "initial subscribe did not fire");

        sig.set(1);
        assert_eq!(
            dirty.borrow().as_slice(),
            &["node-A".to_string()],
            "signal write fired the invalidator with the block's NodeId",
        );
        SIG.with(|s| s.set(None));
    }

    #[test]
    fn block_invalidator_reuses_per_node_contexts_across_lowers() {
        // Phase 3b: lowering the same node twice must reuse the
        // cached per-NodeId reactive context (via reset_and_run_in
        // semantics) rather than allocate a fresh one each frame.
        use crate::block::{register_block, Block};
        use crate::registry::{ComponentRegistry, FieldSpec};
        use crate::ComponentId;
        use prism_ui_runtime::layout::ContainerProps;
        use std::sync::Arc;

        struct Pass {
            id: ComponentId,
        }
        impl Block for Pass {
            fn id(&self) -> &ComponentId {
                &self.id
            }
            fn schema(&self) -> Vec<FieldSpec> {
                vec![]
            }
            fn lower_ui(&self, _: &LowerCtx<'_>, node: &Node, _: &StyleProperties) -> UiNode {
                UiNode::Container {
                    id: node.id.clone(),
                    props: ContainerProps::default(),
                    children: vec![],
                }
            }
        }

        let mut reg = ComponentRegistry::new();
        register_block(&mut reg, Arc::new(Pass { id: "p".into() })).unwrap();

        let invalidator = BlockInvalidator::new(|_id: &str| {});

        let cascade = StyleProperties::default();
        let node = Node {
            id: "stable-id".into(),
            component: "p".into(),
            ..Default::default()
        };

        for _ in 0..3 {
            let ctx =
                LowerCtx::new(Some(&reg), &cascade).with_block_invalidator(invalidator.clone());
            let _ = ctx.lower(&node);
        }
        // One context per distinct NodeId, regardless of how many
        // times we re-lowered.
        assert_eq!(invalidator.cached_len(), 1);
    }

    #[test]
    fn block_invalidator_forget_node_disposes_context() {
        let invalidator = BlockInvalidator::new(|_id: &str| {});
        // Force a context to materialise via run_for_node.
        invalidator.run_for_node("ephemeral", || ());
        assert_eq!(invalidator.cached_len(), 1);
        invalidator.forget_node("ephemeral");
        assert_eq!(invalidator.cached_len(), 0);
    }

    #[test]
    fn lower_without_invalidator_is_pass_through() {
        // Headless test path: no invalidator installed → blocks lower
        // exactly as before, no reactive wrapping.
        use crate::block::{register_block, Block};
        use crate::registry::{ComponentRegistry, FieldSpec};
        use crate::ComponentId;
        use prism_ui_runtime::layout::ContainerProps;
        use std::sync::Arc;

        struct Pass {
            id: ComponentId,
        }
        impl Block for Pass {
            fn id(&self) -> &ComponentId {
                &self.id
            }
            fn schema(&self) -> Vec<FieldSpec> {
                vec![]
            }
            fn lower_ui(&self, _: &LowerCtx<'_>, node: &Node, _: &StyleProperties) -> UiNode {
                UiNode::Container {
                    id: node.id.clone(),
                    props: ContainerProps::default(),
                    children: vec![],
                }
            }
        }
        let mut reg = ComponentRegistry::new();
        register_block(&mut reg, Arc::new(Pass { id: "p".into() })).unwrap();
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade);
        let n = Node {
            id: "n1".into(),
            component: "p".into(),
            ..Default::default()
        };
        let out = ctx.lower(&n);
        let UiNode::Container { id, .. } = out else {
            panic!()
        };
        assert_eq!(id, "n1");
    }

    #[test]
    fn hover_bg_returns_some_for_valid_color() {
        let h = hover_bg("#1f000000").expect("valid color");
        assert!(h.background.is_some());
        assert!(h.radius.is_none());
    }

    #[test]
    fn hover_bg_returns_none_for_invalid_color() {
        assert!(hover_bg("not-a-color").is_none());
    }

    #[test]
    fn prop_helpers_extract_with_sensible_defaults() {
        use crate::layout::LayoutMode;
        use prism_core::foundation::spatial::Transform2D;
        use serde_json::json;
        let node = Node {
            id: "n".into(),
            component: "x".into(),
            props: json!({ "label": "Hi", "selected": true }),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let style = StyleProperties::default();
        let ctx = LowerCtx::new(None, &style);
        assert_eq!(ctx.prop_str(&node, "label"), "Hi");
        assert_eq!(ctx.prop_str(&node, "missing"), "");
        assert!(ctx.prop_bool(&node, "selected", false));
        assert!(!ctx.prop_bool(&node, "missing", false));
        assert!(ctx.prop_bool(&node, "missing", true));
    }

    #[test]
    fn prop_helpers_subscribe_through_bindings() {
        // Phase 4b: when a `DocumentBindings` is wired and we read a
        // prop through `ctx.prop_str` inside a reactive context, the
        // context subscribes; a later write through `NodeMutator`
        // fires the dirty callback. This is the contract that makes
        // `lower_ui` bodies reactive without per-block plumbing.
        use crate::layout::LayoutMode;
        use crate::mutator::NodeMutator;
        use crate::reactive_props::DocumentBindings;
        use prism_core::foundation::spatial::Transform2D;
        use prism_core::reactive::ReactiveContext;
        use serde_json::json;
        use std::cell::Cell;
        let mut node = Node {
            id: "n".into(),
            component: "x".into(),
            props: json!({ "label": "hello" }),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let bindings = DocumentBindings::new();
        let style = StyleProperties::default();
        let ctx = LowerCtx::new(None, &style).with_bindings(&bindings);

        let dirty = Rc::new(Cell::new(0_usize));
        let dirty_for_ctx = Rc::clone(&dirty);
        let rcx = ReactiveContext::new(move || {
            dirty_for_ctx.set(dirty_for_ctx.get() + 1);
        });
        let read = rcx.reset_and_run_in(|| ctx.prop_str(&node, "label"));
        assert_eq!(read, "hello");
        assert_eq!(dirty.get(), 0, "subscribe alone doesn't fire dirty");

        NodeMutator::with_bindings(&bindings).write(&mut node, "label", json!("world"));
        assert_eq!(
            dirty.get(),
            1,
            "reactive write wakes the subscribing context"
        );
        rcx.dispose();
    }

    #[test]
    fn colored_text_node_overrides_cascade_color() {
        let cascade = StyleProperties {
            color: Some("#000000".into()),
            ..Default::default()
        };
        let UiNode::Text { props, .. } =
            colored_text_node("t".into(), "hello".into(), &cascade, 14.0, "#ff0000")
        else {
            panic!("expected text")
        };
        assert_eq!(props.color.r, 0xff);
        assert_eq!(props.color.g, 0x00);
    }

    #[test]
    fn hover_bg_round_trips_alpha() {
        // #RRGGBBAA — last byte is alpha.
        let h = hover_bg("#0000001f").unwrap();
        let c = h.background.unwrap();
        assert_eq!(c.a, 0x1f);
    }

    // ── Wave 1: modifier render fold ─────────────────────────────────

    /// A behaviour that wraps the child in a tagged container so the
    /// fold order is observable from the output tree.
    struct TagWrapBehaviour {
        id: &'static str,
        marker: &'static str,
    }
    impl crate::modifier::ModifierBehaviour for TagWrapBehaviour {
        fn id(&self) -> crate::modifier::ModifierId {
            std::borrow::Cow::Borrowed(self.id)
        }
        fn label(&self) -> &str {
            self.id
        }
        fn schema(&self) -> Vec<crate::registry::FieldSpec> {
            Vec::new()
        }
        fn wrap(&self, _modifier: &crate::modifier::Modifier, child: UiNode) -> UiNode {
            // Wrap the child in a container whose semantic tag carries
            // the marker — the fold-order pin reads these back off the
            // output tree.
            UiNode::Container {
                id: format!("wrap-{}", self.marker),
                props: ContainerProps {
                    semantic: prism_ui_runtime::layout::Semantic::tag(self.marker),
                    ..ContainerProps::default()
                },
                children: vec![child],
            }
        }
    }

    fn make_test_node(id: &str) -> Node {
        Node {
            id: id.into(),
            component: "container".into(),
            props: serde_json::Value::Null,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: prism_core::foundation::spatial::Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        }
    }

    #[test]
    fn modifier_fold_applies_innermost_first() {
        let mut reg = crate::modifier::ModifierRegistry::new();
        reg.register(std::sync::Arc::new(TagWrapBehaviour {
            id: "outer-beh",
            marker: "outer",
        }))
        .unwrap();
        reg.register(std::sync::Arc::new(TagWrapBehaviour {
            id: "inner-beh",
            marker: "inner",
        }))
        .unwrap();

        let mut node = make_test_node("n0");
        // Order on disk: [outer, inner]. Innermost-first means `inner`
        // wraps the bare child first, then `outer` wraps that.
        // Resulting tree: outer → inner → bare.
        node.modifiers
            .push(crate::modifier::Modifier::new("outer-beh"));
        node.modifiers
            .push(crate::modifier::Modifier::new("inner-beh"));

        let style = StyleProperties::default();
        let ctx = LowerCtx::new(None, &style).with_modifier_registry(&reg);
        let lowered = ctx.lower(&node);

        let UiNode::Container {
            props: outer_props,
            children: outer_children,
            ..
        } = &lowered
        else {
            panic!("expected outer wrap container, got {lowered:?}");
        };
        assert_eq!(outer_props.semantic.tag.as_deref(), Some("outer"));
        let UiNode::Container {
            props: inner_props, ..
        } = &outer_children[0]
        else {
            panic!("expected inner wrap container");
        };
        assert_eq!(inner_props.semantic.tag.as_deref(), Some("inner"));
    }

    #[test]
    fn modifier_fold_skips_disabled_entries() {
        let mut reg = crate::modifier::ModifierRegistry::new();
        reg.register(std::sync::Arc::new(TagWrapBehaviour {
            id: "should-wrap",
            marker: "applied",
        }))
        .unwrap();
        reg.register(std::sync::Arc::new(TagWrapBehaviour {
            id: "skip-me",
            marker: "skipped",
        }))
        .unwrap();

        let mut node = make_test_node("n1");
        node.modifiers
            .push(crate::modifier::Modifier::new("should-wrap"));
        node.modifiers
            .push(crate::modifier::Modifier::new("skip-me").disabled());

        let style = StyleProperties::default();
        let ctx = LowerCtx::new(None, &style).with_modifier_registry(&reg);
        let lowered = ctx.lower(&node);

        let UiNode::Container { props, .. } = &lowered else {
            panic!()
        };
        // Only the enabled behaviour wrapped — the disabled one didn't.
        assert_eq!(props.semantic.tag.as_deref(), Some("applied"));
    }

    #[test]
    fn modifier_fold_passes_through_unknown_ids() {
        let reg = crate::modifier::ModifierRegistry::new(); // empty
        let mut node = make_test_node("n2");
        node.modifiers
            .push(crate::modifier::Modifier::new("nonexistent"));

        let style = StyleProperties::default();
        let ctx = LowerCtx::new(None, &style).with_modifier_registry(&reg);
        let lowered = ctx.lower(&node);
        // Default container fallback — no wrap applied.
        let UiNode::Container { id, .. } = &lowered else {
            panic!()
        };
        assert_eq!(id, "n2");
    }

    #[test]
    fn modifier_fold_no_op_without_registry() {
        // Without `with_modifier_registry`, the fold is skipped
        // entirely — headless / SSR paths preserve their current
        // output unchanged.
        let mut node = make_test_node("n3");
        node.modifiers.push(crate::modifier::Modifier::from_kind(
            crate::modifier::ModifierKind::Tooltip,
        ));

        let style = StyleProperties::default();
        let ctx = LowerCtx::new(None, &style);
        let lowered = ctx.lower(&node);
        let UiNode::Container { id, .. } = &lowered else {
            panic!()
        };
        assert_eq!(id, "n3");
    }
}
