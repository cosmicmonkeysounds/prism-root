//! Interpret path — parse a `.prui` source string and lower the
//! resulting AST onto the runtime's typed [`layout::Node`] tree.
//!
//! This is the runtime half of the codegen story (Phase 2 tail, plan
//! §4.6 / §4.7). The DSL parser lives in
//! `prism_core::language::prism_ui`; here we walk its [`AstDocument`]
//! and emit `Node`s the layout engine already understands.
//!
//! ## Design
//!
//! The lowering is one pass parameterised by [`LowerScope`] — a
//! single declarative carrier for everything an element might need at
//! lowering time:
//!
//! * **Bindings** — `{name}` interpolations and `if=` / `for=`
//!   expressions resolve through the scope's binding map.
//! * **Slots** — a `<slot/>` element resolves to whatever AST nodes
//!   the *caller* (parent scope) parked under that slot name; falls
//!   back to the slot's own children as default content.
//!
//! Control flow (`if`, `else-if`, `else`, `for`) is handled in a
//! single sibling pre-pass via [`expand_control_flow`], so the
//! per-element lowering body stays free of branches it can't honour.
//! Adding a new control-flow keyword is one match arm in that helper.
//!
//! All lowering paths funnel through [`lower_document_with_scope`];
//! the public [`lower_document`] / [`interpret`] entry points are
//! thin wrappers that supply an empty scope.

use std::collections::HashMap;
use std::sync::Arc;

use prism_core::language::prism_ui::{
    parse, split_state_suffix, AttributeNamespace, AttributeValue, Document as AstDocument,
    Element, Node as AstNode, ParseError,
};

use crate::command::{Color, CornerRadius};
use crate::layout::{
    ContainerProps, Direction, HoverOverrides, Node, Padding, Semantic, Sizing, TextProps,
};

mod control_flow;
#[cfg(feature = "luau")]
use control_flow::expand_language;
use control_flow::{expand_control_flow, expand_match, expand_suspense, resolve_for_iteration};

// ---------------------------------------------------------------------------
// Tag resolver — DI hook for unknown tags
// ---------------------------------------------------------------------------

/// Resolve a `.prui` element whose tag the runtime doesn't own
/// (`<shell.icon-button …/>`, `<my.card …/>`, …) into runtime
/// [`Node`]s.
///
/// The runtime's built-in vocabulary (`container`, `text`, `heading`,
/// `spacer`, `input`, `slot`) is closed by design — a host that
/// registers component blocks supplies a [`TagResolver`] through
/// [`LowerScope::with_resolver`]. The resolver sees the raw [`Element`]
/// (so it can read namespaced attributes like `style:bg` or `data:key`
/// without re-parsing) plus the active [`LowerScope`] (so it can fork
/// child scopes for binding/slot propagation).
///
/// Returning `None` signals "I don't know this tag" and the runtime
/// falls through to its default behaviour (drop the wrapper, keep
/// children). Returning `Some(vec![...])` short-circuits the default
/// path with the resolver's nodes.
///
/// **Smart pattern.** This is the *only* extension seam the runtime
/// exposes — every host-specific component vocabulary (Prism Builder
/// blocks, shell chrome, future plugin-provided components) plugs in
/// through one trait, not three parallel hooks. The runtime stays
/// component-registry-agnostic; resolvers compose freely.
pub trait TagResolver: Send + Sync {
    fn resolve(&self, element: &Element, scope: &LowerScope) -> Option<Vec<Node>>;
}

/// **Wave H (`prui-luau-fusion.md` §5.4)** — host hook that resolves
/// an `<import kind="path"/>` to the imported file's source text.
/// The runtime parses a `.prui` from a string and has no filesystem
/// of its own, so sidecar/`<import>` resolution is delegated: the
/// shell/relay host (which *does* know the document's directory and
/// the workspace's `prism://` roots) supplies a resolver via
/// [`LowerScope::with_import_resolver`].
///
/// `kind` is one of `stylesheet` / `script` / `widget` / `dialect`
/// (the four `<import>` projections); `path` is the verbatim
/// attribute value (`./theme.prss`, `prism://lib/fmt.luau`).
/// Returning `None` means "unresolved" — the import is skipped
/// (graceful, like an unknown tag) rather than failing the render.
pub trait ImportResolver: Send + Sync {
    fn resolve_import(&self, kind: &str, path: &str) -> Option<String>;
}

/// Per-tag host emission: the props the binding emitted and the
/// pre-lowered children, kept side-by-side in one record. Used by
/// downstream `lower_as` callers (the dock-panel routing path is
/// the canonical consumer) to populate a synthesised content tag
/// with the same data the resolver-driven path would have applied
/// for an authored AST tag.
///
/// Distinct from [`LowerScope::host_children_by_tag`], which is
/// children-only and consumed by the resolver. The two could be
/// folded together; they're kept separate so the resolver path
/// (which uses host_children) and the `lower_as` path (which also
/// wants props) can evolve independently.
#[derive(Debug, Clone, Default)]
pub struct TagEmission {
    pub props: serde_json::Value,
    pub children: Vec<Node>,
}

/// Scope handed to every element lowering. Owns the bindings used by
/// `{ident}` interpolations and control-flow predicates, plus the
/// slot bindings a parent component injected.
///
/// Cloning is cheap by design — the maps are small (one entry per
/// `for` iterator variable / named slot) and lowering paths fork
/// scopes constantly via [`Self::with_binding`]. Keeping the API
/// builder-shaped means a child scope reads as one expression at the
/// call site rather than three lines of `let mut child = parent.clone();`.
#[derive(Clone, Default)]
pub struct LowerScope {
    bindings: HashMap<String, serde_json::Value>,
    slots: SlotBindings,
    resolver: Option<Arc<dyn TagResolver>>,
    /// **Wave H** — host hook for `<import>` resolution. `None` on
    /// the string-only `interpret()` path (imports are skipped); the
    /// shell/relay installs one that reads the document directory +
    /// `prism://` roots.
    import_resolver: Option<Arc<dyn ImportResolver>>,
    /// Tag-keyed pre-lowered children supplied by the *host*, not the
    /// AST. Distinct from [`SlotBindings`] (which holds AST nodes for
    /// `<slot/>` expansion) and from
    /// [`crate::layout::Node`]-as-host_children inside `LowerCtx`
    /// (which is set by the resolver from AST children). This slot lets
    /// a host inject already-lowered children for a specific tag —
    /// the canonical use case is rendering a host-side document tree
    /// (`prism_builder::BuilderDocument`) inside a registered tag's
    /// composition slot (`shell.builder-canvas`).
    ///
    /// `Arc` so scope clones stay cheap when forking for control-flow
    /// / slot expansion; the inner map is replaced wholesale through
    /// [`Self::with_host_children_by_tag`], never mutated in place.
    host_children_by_tag: Arc<HashMap<String, Vec<Node>>>,
    /// Tag-keyed props + children emissions, surfaced through
    /// `LowerCtx::lower_as` so dynamically routed content tags (the
    /// dock-panel `panel-id` → content-tag path is the canonical case)
    /// inherit the same binding data the resolver-driven AST path
    /// would have applied. `Arc` for the same cheap-fork rationale as
    /// `host_children_by_tag`.
    tag_emissions: Arc<HashMap<String, TagEmission>>,
    /// Wave 11.2 — `<host-children/>` injection point for a DSL-
    /// authored shell component composing its caller's pre-lowered
    /// children. The loader's [`crate::interpret::lower_document_with_scope`]
    /// caller stuffs the calling `LowerCtx::host_children()` here at
    /// invocation time; the element handler emits the UiNodes
    /// verbatim. Distinct from `host_children_by_tag` (resolver-side,
    /// tag-keyed pre-injection) and from [`SlotBindings`] (AST-level
    /// `<slot/>` expansion). `None` outside the loader's seam.
    host_children_ui: Option<Arc<Vec<Node>>>,
    /// **Wave 13.1** — pre-lowered named-slot map. The resolver
    /// buckets a dispatched element's children by their `slot="X"`
    /// attribute and threads the resulting map here via the DSL
    /// loader. A `<slot name="X"/>` element in a DSL component body
    /// pulls from this map (after first checking the AST-level
    /// [`SlotBindings`]) before falling back to its own fallback
    /// children. Empty map (`None`-equivalent default) on every path
    /// that doesn't go through `RegistryTagResolver`.
    host_children_by_slot: Arc<HashMap<String, Vec<Node>>>,
    /// **PRSS** — installed stylesheet. When set, every container
    /// with a `class="…"` attribute applies the named classes
    /// (with `extends` flattened, base properties first then state
    /// overrides) via the existing `apply_style_override` vocabulary
    /// before the user's inline `style:` attributes layer on top.
    /// `None` when no stylesheet is loaded (the headless / SSR /
    /// pre-stylesheet path), in which case `class="…"` round-trips
    /// as a no-op so authored DSL stays valid against either runtime
    /// configuration. See `docs/dev/prss-reference.md`.
    stylesheet: Option<Arc<prism_core::language::prss::StyleSheet>>,
    /// **Wave 14.3** — `<teleport to="X">payload</teleport>` index.
    /// Built once by [`lower_document_with_scope`] from a top-down
    /// AST scan: every `<teleport>` element contributes its
    /// (un-lowered) children to `teleports[to]`. During the main
    /// lowering pass, a container or component with `id="X"` appends
    /// those AST children after its own — lowered in the target's
    /// scope so binding resolution flows from the destination, not
    /// the source. Targets without a matching teleport see this map
    /// as empty. `Arc` so scope clones stay cheap; the inner map is
    /// installed wholesale through [`Self::with_teleports`] and
    /// never mutated in place.
    teleports: Arc<HashMap<String, Vec<AstNode>>>,
    /// **Wave 14.3** — `memo="[dep1, dep2]"` cache. Optional shared
    /// memoisation table keyed by element id. When installed via
    /// [`Self::with_memo_cache`], any element carrying a literal
    /// `memo="…"` attribute *and* a resolvable id skips re-lowering
    /// whenever its dep tuple matches the cached one. `RefCell` so
    /// the host can keep one cache across frames and still hand the
    /// scope around by value.
    memo_cache: Option<std::rc::Rc<std::cell::RefCell<MemoCache>>>,
    /// **PRSS descendant selectors** — outermost-first list of
    /// per-ancestor class lists. Pushed by `lower_element_body`
    /// whenever a container/component carries a non-empty active
    /// class set so descendant selectors (`btn icon`,
    /// `.card .title`, …) can match against the ancestor chain at
    /// apply time. `Arc` so scope clones stay cheap during
    /// control-flow / slot expansion; the inner Vec is cloned only
    /// when the chain extends.
    class_chain: Arc<Vec<Vec<String>>>,
    /// **Wave E.3 (`prui-luau-fusion.md` §7.8)** — host-supplied
    /// builtin Luau sources (bundled dialect / macro registrations,
    /// e.g. `prism_builder::builtin_dialect_sources()`). Prepended as
    /// flat modules ahead of every document's own `<script>` blocks
    /// so `<markdown>` / `~sql{…}` resolve without the author wiring
    /// `prism.dialect{…}` by hand. `Arc`-cheap to fork; empty on the
    /// bare `interpret()` path (no builtins, same as no resolver).
    builtin_scripts: Arc<Vec<String>>,
    /// **Wave A (`prui-luau-fusion.md` §7.1)** — the per-document
    /// Luau state harvested from `<script>` blocks. When
    /// set, expression-slot identifier lookups and call resolution
    /// fall through to this frame after the binding map / functional
    /// builtins miss, so a script's top-level `local`s and helper
    /// functions are reachable from every `{expr}`. `None` on every
    /// document with no script block (the common case) and on every
    /// build without the `luau` feature. `Arc`-cheap to fork.
    #[cfg(feature = "luau")]
    luau_scope: Option<crate::luau_scope::LuauScopeFrame>,
    /// **Phase 3 of `docs/dev/dioxus-inspiration.md`** — the set of
    /// builder NodeIds the reactive substrate marked dirty this frame
    /// (drained from the shell's `RenderScope` `DirtyQueue`). When
    /// installed alongside [`Self::with_memo_cache`], `lower_element`
    /// reuses the cached subtree of any id'd element whose id is not
    /// dirty *and* whose cached subtree contains no dirty descendant;
    /// only the dirty NodeIds (and their ancestor paths) re-lower.
    /// `None` = full walk (the event / animator / first-frame path),
    /// which also (re)populates the per-id cache so the next reactive
    /// frame can splice. `Rc` so scope forks stay cheap.
    dirty_nodes: Option<std::rc::Rc<std::collections::HashSet<String>>>,
    /// **Per-class PRSS invalidation (fusion F.3)** — the class→NodeId
    /// usage collector. When installed, `apply_container_attributes`
    /// records every `(class, id)` it resolves so the shell's `.prss`
    /// hot-reload consumer can mark only the affected NodeIds dirty.
    /// `None` on headless / SSR / no-stylesheet paths. `Rc` so scope
    /// forks stay cheap.
    class_deps: Option<std::rc::Rc<std::cell::RefCell<ClassUsage>>>,
    /// **§4.3 — probe/inspector seam.** Optional one-shot sink the
    /// document-scope builder writes the per-document
    /// [`crate::luau_scope::LuauScopeFrame`] into once it's built, so
    /// a host (the shell) can *retain* it past the render and reach
    /// `fire_probe` / `has_probe` from the event router. `None` on
    /// every path that doesn't care (SSR, tests). `Rc<RefCell<…>>`
    /// so the host keeps its end and reads it after `render_tree`.
    #[cfg(feature = "luau")]
    frame_sink: Option<std::rc::Rc<std::cell::RefCell<Option<crate::luau_scope::LuauScopeFrame>>>>,
}

/// **Wave 14.3** — per-element memo cache keyed by `id`. Hosts that
/// want stable v-memo semantics across renders construct one and
/// thread it into [`LowerScope::with_memo_cache`]; the same handle
/// can live for the lifetime of the surface (frames, panel swaps,
/// hot reloads — anything short of a document replacement).
#[derive(Debug, Default)]
pub struct MemoCache {
    entries: HashMap<String, (Vec<serde_json::Value>, Vec<Node>)>,
    /// **Phase 3** — every element id that was *(re)lowered* (rather
    /// than spliced from cache) during the current render pass. The
    /// shell reads this after a reactive (dirty-set) render to verify
    /// every drained dirty NodeId actually mapped to a real element;
    /// any dirty id that was *not* touched means the splice may have
    /// skipped a needed update (an unknown / renamed id), so the
    /// shell safely re-renders that frame with a full walk. This
    /// makes the optimisation strictly non-lossy: worst case equals
    /// today's behaviour.
    touched: std::collections::HashSet<String>,
}

impl MemoCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Forget every cached entry — useful when the underlying tree
    /// shape changes drastically (panel swap, hot reload).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.touched.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// **Phase 3** — reset the per-pass touched set. The shell calls
    /// this immediately before a dirty-set render so the post-render
    /// [`Self::untouched`] check sees only this frame's re-lowers.
    pub fn begin_pass(&mut self) {
        self.touched.clear();
    }

    /// **Phase 3** — every id in `dirty` that was *not* (re)lowered
    /// this pass. Empty result ⇒ the spliced render addressed every
    /// dirty NodeId and is safe to present; non-empty ⇒ the shell
    /// must fall back to a full walk this frame.
    pub fn untouched<'a>(&self, dirty: impl IntoIterator<Item = &'a String>) -> Vec<String> {
        dirty
            .into_iter()
            .filter(|d| !self.touched.contains(*d))
            .cloned()
            .collect()
    }
}

/// **Per-class PRSS invalidation (fusion F.3)** — the class→NodeId
/// dependency table built during lowering. When a container with a
/// resolvable `id` resolves one or more PRSS classes, each `(class
/// name → NodeId)` pair is recorded here. The shell's `.prss`
/// hot-reload consumer reads it: a `LiteralOnly` change carrying
/// `PrssLiteralPatch { owner: Class { name, .. }, .. }` marks just
/// the NodeIds that used `name` dirty, so Phase 3 splices the rest
/// instead of a full re-walk. PRSS classes resolve from the plain
/// `StyleSheet` (not a `Signal`), so this explicit usage table is
/// the dependency edge the reactive substrate can't infer on its
/// own. Host-owned, persisted across frames; cleared + repopulated
/// each full render.
#[derive(Debug, Default)]
pub struct ClassUsage {
    map: HashMap<String, std::collections::HashSet<String>>,
}

impl ClassUsage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that the element `node_id` resolved PRSS class `class`.
    pub fn record(&mut self, class: &str, node_id: &str) {
        self.map
            .entry(class.to_string())
            .or_default()
            .insert(node_id.to_string());
    }

    /// Every NodeId that resolved `class` (insertion-agnostic).
    pub fn nodes_for(&self, class: &str) -> Vec<String> {
        self.map
            .get(class)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Forget every recorded dependency — the shell calls this before
    /// a full render pass so a removed `class="…"` doesn't keep a
    /// stale NodeId alive across edits.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl std::fmt::Debug for LowerScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LowerScope")
            .field("bindings", &self.bindings)
            .field("slots", &self.slots)
            .field(
                "resolver",
                &self.resolver.as_ref().map(|_| "<dyn TagResolver>"),
            )
            .field(
                "host_children_by_tag",
                &self.host_children_by_tag.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl LowerScope {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind `name` to `value`. Returns a new scope rather than mutating
    /// in place so call sites can write `scope.with_binding("post", v)`
    /// inline as a `for`-loop body without juggling temporaries.
    pub fn with_binding(mut self, name: impl Into<String>, value: serde_json::Value) -> Self {
        self.bindings.insert(name.into(), value);
        self
    }

    /// Replace the slot bindings wholesale — used at component-
    /// instantiation time when the caller knows the full slot map up
    /// front.
    pub fn with_slots(mut self, slots: SlotBindings) -> Self {
        self.slots = slots;
        self
    }

    /// Install a tag resolver. Subsequent lowering passes consult this
    /// resolver before falling through to the unknown-tag default.
    /// Resolver propagates through child scopes (control-flow forks,
    /// slot expansion) automatically — the same scope chain carries
    /// it.
    pub fn with_resolver(mut self, resolver: Arc<dyn TagResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// **Wave H** — install the `<import>` resolver. Propagates
    /// through scope clones like [`Self::with_resolver`].
    pub fn with_import_resolver(mut self, resolver: Arc<dyn ImportResolver>) -> Self {
        self.import_resolver = Some(resolver);
        self
    }

    /// **Wave H** — borrow the installed import resolver, if any.
    pub fn import_resolver(&self) -> Option<&Arc<dyn ImportResolver>> {
        self.import_resolver.as_ref()
    }

    /// **Wave E.3** — install host-supplied builtin Luau sources
    /// (bundled dialect / macro registrations). They run as flat
    /// modules ahead of the document's own `<script>` blocks.
    pub fn with_builtin_scripts(mut self, scripts: Vec<String>) -> Self {
        self.builtin_scripts = Arc::new(scripts);
        self
    }

    /// **Wave E.3** — the installed builtin Luau sources (empty by
    /// default).
    pub fn builtin_scripts(&self) -> &[String] {
        &self.builtin_scripts
    }

    /// Install a tag-keyed map of pre-lowered children. When the
    /// resolver dispatches an element whose tag is a key in this map,
    /// the values become the block's `host_children` — overriding the
    /// pre-lowered AST children for that tag. Other tags are
    /// unaffected.
    ///
    /// Single use case today: a host populates this map from binding
    /// emissions so a registered tag (`shell.builder-canvas`) can host
    /// a live `BuilderDocument` tree without round-tripping it through
    /// attribute JSON. The propagation rule mirrors the existing
    /// resolver: the map carries through scope clones (control-flow,
    /// slot expansion) so nested resolver dispatches see the same
    /// host_children injection.
    pub fn with_host_children_by_tag(mut self, map: HashMap<String, Vec<Node>>) -> Self {
        self.host_children_by_tag = Arc::new(map);
        self
    }

    /// Install the tag-keyed emissions map. Mirrors
    /// [`Self::with_host_children_by_tag`] in shape — a host snapshots
    /// every binding's `(props, children)` pair into this map so
    /// `lower_as` callers (dock-panel routing, future composition
    /// blocks) inherit the live data.
    pub fn with_tag_emissions(mut self, map: HashMap<String, TagEmission>) -> Self {
        self.tag_emissions = Arc::new(map);
        self
    }

    /// Wave 11.3 — install an already-`Arc<HashMap>`-wrapped emissions
    /// table. Used by the DSL loader to forward the snapshot it
    /// received from its `LowerCtx` without cloning the underlying
    /// map. Caller relinquishes ownership; subsequent reads share the
    /// installed `Arc`.
    pub fn with_tag_emissions_arc(mut self, map: Arc<HashMap<String, TagEmission>>) -> Self {
        self.tag_emissions = map;
        self
    }

    pub fn binding(&self, name: &str) -> Option<&serde_json::Value> {
        self.bindings.get(name)
    }

    /// **Wave I (`prui-luau-fusion.md` §6.2)** — snapshot every
    /// host/document binding as one JSON object. Seeded into the
    /// per-document Lua state as `prism.scope` so a `<script>` can
    /// read host-provided props (`prism.scope.task`) the same way an
    /// `{expr}` slot resolves a bare identifier. (The *typed* view —
    /// `---@type Task` checked against the host `BlockSpec` schema —
    /// is the external `luau-analyze` half of Wave I; this is the
    /// runtime value bridge it type-annotates.)
    pub fn bindings_json(&self) -> serde_json::Value {
        serde_json::Value::Object(self.bindings.clone().into_iter().collect())
    }

    pub fn resolver(&self) -> Option<&Arc<dyn TagResolver>> {
        self.resolver.as_ref()
    }

    /// Look up a host-injected pre-lowered children slice for `tag`.
    /// Resolvers call this before falling back to AST pre-lowering.
    pub fn host_children_for(&self, tag: &str) -> Option<&[Node]> {
        self.host_children_by_tag.get(tag).map(|v| v.as_slice())
    }

    /// Look up the full tag emission (props + children) for `tag`.
    /// Returns `None` when no host binding emitted under that tag
    /// during this snapshot — the caller falls back to its own props
    /// shape (the dock-panel routing path: passes `{}`).
    pub fn tag_emission_for(&self, tag: &str) -> Option<&TagEmission> {
        self.tag_emissions.get(tag)
    }

    /// Borrow the underlying `Arc<HashMap<…>>` for embedding into a
    /// [`crate::layout::Node`]-side lookup carrier (the `LowerCtx`'s
    /// new `tag_emissions` field). `Arc` clone is cheap and lets the
    /// builder-side lookup outlive any single scope value.
    pub fn tag_emissions_arc(&self) -> Arc<HashMap<String, TagEmission>> {
        Arc::clone(&self.tag_emissions)
    }

    /// Wave 11.2 — install the pre-lowered children the `<host-children/>`
    /// element should emit. The shell's `.prui` loader sets this
    /// before invoking [`lower_document_with_scope`] so a DSL-authored
    /// wrapper component (toast-stack, launchpad) consumes its caller's
    /// children via one declarative element instead of a Rust `ctx.host_children()`
    /// call.
    pub fn with_host_children_ui(mut self, children: Vec<Node>) -> Self {
        self.host_children_ui = Some(Arc::new(children));
        self
    }

    /// The pre-lowered children currently bound to the
    /// `<host-children/>` element. `None` outside the loader's seam.
    pub fn host_children_ui(&self) -> Option<&[Node]> {
        self.host_children_ui.as_deref().map(|v| v.as_slice())
    }

    /// **Wave 13.1** — install the named-slot map. Keys are
    /// `slot="X"` attribute values from the caller's AST children;
    /// values are the pre-lowered UI nodes for that bucket. The
    /// resolver populates this from a dispatched element's children;
    /// `<slot name="X"/>` reads from it.
    pub fn with_host_children_by_slot(mut self, slots: Arc<HashMap<String, Vec<Node>>>) -> Self {
        self.host_children_by_slot = slots;
        self
    }

    /// **Wave 13.1** — look up the pre-lowered children for a named
    /// slot. Returns `None` when no `host_children_by_slot` map was
    /// installed *or* when the requested slot is absent.
    pub fn host_children_for_slot(&self, name: &str) -> Option<&[Node]> {
        self.host_children_by_slot.get(name).map(|v| v.as_slice())
    }

    /// **Wave 13.1** — clone-cheap snapshot of the slot map. Loader
    /// callers thread this through from `LowerCtx::host_children_by_slot()`.
    pub fn host_children_by_slot_arc(&self) -> Arc<HashMap<String, Vec<Node>>> {
        Arc::clone(&self.host_children_by_slot)
    }

    /// **Wave 14.3** — install the teleport payload index. Built by
    /// the document-level pre-scan in [`lower_document_with_scope`];
    /// callers that compose AST trees directly (resolvers, tests)
    /// can install one explicitly here.
    pub fn with_teleports(mut self, teleports: Arc<HashMap<String, Vec<AstNode>>>) -> Self {
        self.teleports = teleports;
        self
    }

    /// **Wave 14.3** — borrow the AST payload routed to a given
    /// target id, if any. Empty (`&[]`) when no `<teleport>` in the
    /// document targeted this id.
    pub fn teleport_payload_for(&self, target: &str) -> &[AstNode] {
        self.teleports
            .get(target)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// **Wave 14.3** — install the [`MemoCache`] handle. Elements
    /// carrying both `memo="[dep1, dep2]"` and a resolvable `id`
    /// will short-circuit re-lowering whenever the dep tuple matches
    /// the previously-cached one. Without a cache installed,
    /// `memo="…"` is a no-op (the data round-trip pattern from Wave
    /// 9.4 — author intent is preserved without behaviour change).
    pub fn with_memo_cache(mut self, cache: std::rc::Rc<std::cell::RefCell<MemoCache>>) -> Self {
        self.memo_cache = Some(cache);
        self
    }

    /// **Wave 14.3** — borrow the installed memo cache handle. Used
    /// by the lowering pass to look up and store memoised subtrees.
    pub fn memo_cache(&self) -> Option<&std::rc::Rc<std::cell::RefCell<MemoCache>>> {
        self.memo_cache.as_ref()
    }

    /// **Phase 3** — install the per-frame dirty NodeId set. Only
    /// meaningful with a [`Self::with_memo_cache`] also installed;
    /// the splice path keys off the cache. See [`Self::dirty_nodes`].
    pub fn with_dirty_nodes(
        mut self,
        dirty: std::rc::Rc<std::collections::HashSet<String>>,
    ) -> Self {
        self.dirty_nodes = Some(dirty);
        self
    }

    /// **Phase 3** — the per-frame dirty NodeId set, if the host
    /// installed one this render.
    pub fn dirty_nodes(&self) -> Option<&std::collections::HashSet<String>> {
        self.dirty_nodes.as_deref()
    }

    /// **Fusion F.3** — install the class→NodeId usage collector.
    pub fn with_class_deps(mut self, deps: std::rc::Rc<std::cell::RefCell<ClassUsage>>) -> Self {
        self.class_deps = Some(deps);
        self
    }

    /// **Fusion F.3** — borrow the class-usage collector handle.
    pub fn class_deps(&self) -> Option<&std::rc::Rc<std::cell::RefCell<ClassUsage>>> {
        self.class_deps.as_ref()
    }

    /// **§4.3** — install the one-shot frame sink. After
    /// `lower_document_with_scope` builds the per-document
    /// `LuauScopeFrame`, it deposits a clone here so the host can
    /// retain it (event-router probe dispatch).
    #[cfg(feature = "luau")]
    pub fn with_frame_sink(
        mut self,
        sink: std::rc::Rc<std::cell::RefCell<Option<crate::luau_scope::LuauScopeFrame>>>,
    ) -> Self {
        self.frame_sink = Some(sink);
        self
    }

    /// **§4.3** — borrow the frame sink, if installed.
    #[cfg(feature = "luau")]
    pub fn frame_sink(
        &self,
    ) -> Option<&std::rc::Rc<std::cell::RefCell<Option<crate::luau_scope::LuauScopeFrame>>>> {
        self.frame_sink.as_ref()
    }

    /// **Wave 14.1** — seed the design-token table as a `tokens`
    /// binding. Every migrated `.prui` file authors visual
    /// constants today as hardcoded hex / px — `style:background="#161a22ff"`,
    /// `padding="12"`. With the token binding in scope, the same
    /// authoring surface reads `style:background="{tokens.colors.surface}"`
    /// / `padding="{tokens.spacing.md}"` and resolves through the
    /// existing dotted-path lookup. One source of truth for visual
    /// constants across the shell; one binding seed.
    ///
    /// The shape mirrors [`prism_core::design_tokens::DesignTokens`]
    /// exactly — `colors.<name>` returns a hex string suitable for
    /// `style:background` / `style:color`; `spacing.<size>`,
    /// `radius.<size>`, `typography.<key>` return numeric pixel
    /// values suitable for any numeric prop.
    pub fn with_design_tokens(mut self, tokens: &prism_core::design_tokens::DesignTokens) -> Self {
        self.bindings
            .insert("tokens".to_string(), design_tokens_to_json(tokens));
        self
    }

    /// **PRSS** — install a stylesheet. Subsequent `class="…"`
    /// attributes on lowered containers consult the sheet to
    /// resolve named classes through `apply_style_override`
    /// against the same vocabulary inline `style:` uses. Token
    /// overrides on the sheet are merged over the currently-bound
    /// `tokens` JSON so `{tokens.colors.<name>}` interpolations
    /// resolve through the stylesheet's overrides as well.
    pub fn with_stylesheet(mut self, sheet: Arc<prism_core::language::prss::StyleSheet>) -> Self {
        // Merge stylesheet token overrides into the existing
        // `tokens` binding (Wave 14.1 substrate). The PRUI doc
        // promises a single `tokens` namespace; PRSS overrides any
        // values already seeded by `with_design_tokens` per the
        // application-order rule in `prss-reference.md` §4.6.
        if let Some(tokens) = self.bindings.get_mut("tokens") {
            merge_token_overrides(tokens, &sheet.tokens);
        } else {
            // No design tokens were seeded yet — start from the
            // sheet's overrides alone.
            let mut empty = serde_json::json!({
                "colors": {}, "spacing": {}, "radius": {}, "typography": {}
            });
            merge_token_overrides(&mut empty, &sheet.tokens);
            self.bindings.insert("tokens".to_string(), empty);
        }
        self.stylesheet = Some(sheet);
        self
    }

    /// Borrow the installed stylesheet, if any. `apply_container_attributes`
    /// uses this to walk a container's `class="…"` attribute.
    pub fn stylesheet(&self) -> Option<&prism_core::language::prss::StyleSheet> {
        self.stylesheet.as_deref()
    }

    /// **Wave C** — clone-cheap handle to the installed stylesheet
    /// `Arc`, so a forked scope (the hygienic macro-expansion scope)
    /// can re-thread the same PRSS sheet without owning the original.
    /// `None` when no sheet is loaded (headless / SSR / pre-PRSS).
    pub fn stylesheet_arc(&self) -> Option<Arc<prism_core::language::prss::StyleSheet>> {
        self.stylesheet.clone()
    }

    /// **Token-driven rem base** — the pixel size one `rem` / `em`
    /// resolves to during length parsing. Reads
    /// `tokens.typography.font-size-md` from the active `tokens`
    /// binding when present, defaulting to the canonical browser
    /// 16px when no tokens are seeded or the lookup fails. Hosts
    /// that swap a custom design-token table see `1rem` rescale
    /// across the entire shell uniformly.
    pub fn rem_px(&self) -> f32 {
        const DEFAULT_REM_PX: f32 = 16.0;
        let Some(tokens) = self.bindings.get("tokens") else {
            return DEFAULT_REM_PX;
        };
        let Some(typography) = tokens.get("typography") else {
            return DEFAULT_REM_PX;
        };
        let Some(value) = typography.get("font-size-md") else {
            return DEFAULT_REM_PX;
        };
        match value {
            serde_json::Value::Number(n) => n.as_f64().map(|f| f as f32).unwrap_or(DEFAULT_REM_PX),
            serde_json::Value::String(s) => s.trim().parse::<f32>().unwrap_or(DEFAULT_REM_PX),
            _ => DEFAULT_REM_PX,
        }
    }

    /// **PRSS descendant selectors** — return a fork of this scope
    /// with `classes` appended to the ancestor class chain. Empty
    /// `classes` returns the scope unchanged so the cheap path
    /// (no class on this container) avoids the Arc clone + Vec push.
    /// Scope forks during control-flow / slot expansion inherit the
    /// chain via the `Arc` clone; only the lowering boundary that
    /// actually adds an ancestor class pays the deep clone.
    pub fn with_class_chain_appending(mut self, classes: Vec<String>) -> Self {
        if classes.is_empty() {
            return self;
        }
        let mut next = (*self.class_chain).clone();
        next.push(classes);
        self.class_chain = Arc::new(next);
        self
    }

    /// **PRSS descendant selectors** — borrow the ancestor class
    /// chain, outermost-first. The current element's classes are
    /// **not** in this list; the descendant matcher reads them
    /// directly from the active set computed at apply time.
    pub fn class_chain(&self) -> &[Vec<String>] {
        self.class_chain.as_slice()
    }

    /// **Wave A** — install the per-document Luau scope frame
    /// harvested from `<script>` blocks. Forks of this
    /// scope (control-flow / slot expansion) inherit the frame via
    /// the `Arc` clone, so a script local resolves identically at
    /// every nesting depth.
    #[cfg(feature = "luau")]
    pub fn with_luau_scope(mut self, frame: crate::luau_scope::LuauScopeFrame) -> Self {
        self.luau_scope = Some(frame);
        self
    }

    /// **Wave A** — borrow the installed Luau scope frame, if any.
    #[cfg(feature = "luau")]
    pub fn luau_scope(&self) -> Option<&crate::luau_scope::LuauScopeFrame> {
        self.luau_scope.as_ref()
    }
}

/// Merge per-bucket token overrides from a PRSS [`TokenOverrides`]
/// into the JSON `tokens` binding. Both shapes are
/// `{ colors: {…}, spacing: {…}, radius: {…}, typography: {…} }` —
/// per-bucket key-level merge, sheet keys win.
fn merge_token_overrides(
    tokens: &mut serde_json::Value,
    overrides: &prism_core::language::prss::TokenOverrides,
) {
    let serde_json::Value::Object(map) = tokens else {
        return;
    };
    merge_bucket(map, "colors", &overrides.colors, false);
    merge_bucket(map, "spacing", &overrides.spacing, true);
    merge_bucket(map, "radius", &overrides.radius, true);
    merge_bucket(map, "typography", &overrides.typography, true);
}

fn merge_bucket(
    tokens: &mut serde_json::Map<String, serde_json::Value>,
    bucket: &str,
    overrides: &indexmap::IndexMap<String, String>,
    numeric: bool,
) {
    if overrides.is_empty() {
        return;
    }
    let entry = tokens
        .entry(bucket.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    let serde_json::Value::Object(target) = entry else {
        return;
    };
    for (k, v) in overrides {
        let value = if numeric {
            // Stringified numeric → JSON number. Falls back to a
            // String for `"1rem"` / `"50%"` / unrecognised shapes;
            // the runtime's `parse_f32` consumes either through
            // the unified string path.
            v.parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number)
                .unwrap_or_else(|| serde_json::Value::String(v.clone()))
        } else {
            serde_json::Value::String(v.clone())
        };
        target.insert(k.clone(), value);
    }
}

/// **Wave 14.1** — serialise [`DesignTokens`](prism_core::design_tokens::DesignTokens)
/// into the DSL-readable shape. Colours emit as `#rrggbbaa` strings
/// (the same shape `parse_color` consumes); spacing / radius / type
/// emit as raw integers (the same shape `parse_f32` consumes). Kept
/// pub so the prism-shell can mirror the binding into its
/// non-loader paths (skeleton render, scene tests) without a fresh
/// helper per call site.
pub fn design_tokens_to_json(
    tokens: &prism_core::design_tokens::DesignTokens,
) -> serde_json::Value {
    let c = &tokens.colors;
    let s = &tokens.spacing;
    let r = &tokens.radius;
    let t = &tokens.typography;
    serde_json::json!({
        "colors": {
            "background": rgba_to_hex(&c.background),
            "surface": rgba_to_hex(&c.surface),
            "surface-elevated": rgba_to_hex(&c.surface_elevated),
            "border": rgba_to_hex(&c.border),
            "text-primary": rgba_to_hex(&c.text_primary),
            "text-secondary": rgba_to_hex(&c.text_secondary),
            "accent": rgba_to_hex(&c.accent),
            "accent-muted": rgba_to_hex(&c.accent_muted),
            "danger": rgba_to_hex(&c.danger),
            "success": rgba_to_hex(&c.success),
        },
        "spacing": {
            "xs": s.xs, "sm": s.sm, "md": s.md, "lg": s.lg, "xl": s.xl,
        },
        "radius": {
            "sm": r.sm, "md": r.md, "lg": r.lg, "pill": r.pill,
        },
        "typography": {
            "font-size-sm": t.font_size_sm,
            "font-size-md": t.font_size_md,
            "font-size-lg": t.font_size_lg,
            "font-size-xl": t.font_size_xl,
            "line-height-md": t.line_height_md,
        },
    })
}

fn rgba_to_hex(c: &prism_core::design_tokens::Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
}

/// Slot content the caller of a component injected. Default-only is
/// the common case (a wrapper `<card>` with one body slot); the named
/// map covers multi-slot components (`<dialog>` with `header` /
/// `footer` / default).
///
/// Stores AST nodes rather than already-lowered runtime nodes so the
/// receiving component's own bindings (e.g. iteration variables) are
/// visible inside the slot expansion — matching how every modern
/// component DSL (Vue, Svelte, Solid) treats slot scope.
#[derive(Debug, Clone, Default)]
pub struct SlotBindings {
    default: Vec<AstNode>,
    named: HashMap<String, Vec<AstNode>>,
}

impl SlotBindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default(mut self, nodes: Vec<AstNode>) -> Self {
        self.default = nodes;
        self
    }

    pub fn with_named(mut self, name: impl Into<String>, nodes: Vec<AstNode>) -> Self {
        self.named.insert(name.into(), nodes);
        self
    }

    fn resolve(&self, name: Option<&str>) -> Option<&[AstNode]> {
        match name {
            Some(n) => self.named.get(n).map(|v| v.as_slice()),
            None => {
                if self.default.is_empty() {
                    None
                } else {
                    Some(&self.default)
                }
            }
        }
    }
}

/// Parse + lower in one shot. Recoverable parse errors abort the
/// lowering — callers (build script, live-edit loop) surface them to
/// the editor.
/// Canonical attribute-key form for an `on:<event>[.modifier]` local
/// part. The DSL author writes `on:click.once`; consumers downstream
/// (hit-test cache, resolver-side handler attach) read a flattened
/// dash-joined key (`click-once`). One seam, two callers — the
/// runtime's `apply_container_attributes` and the resolver's
/// `attach_on_handlers` both route through here so the wire shape
/// stays in sync.
#[doc(hidden)]
pub fn on_event_attr_key(local: &str) -> String {
    local.replace('.', "-")
}

pub fn interpret(source: &str) -> Result<Vec<Node>, Vec<ParseError>> {
    interpret_with_scope(source, &LowerScope::default())
}

/// Same as [`interpret`] with an explicit scope. Component
/// instantiation paths (Phase 3) call this with the caller's
/// `LowerScope` carrying iterator variables and slot bindings.
pub fn interpret_with_scope(
    source: &str,
    scope: &LowerScope,
) -> Result<Vec<Node>, Vec<ParseError>> {
    let (document, errors) = parse(source);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(lower_document_with_scope(&document, scope))
}

/// Lower an already-parsed AST document into a flat sequence of
/// runtime `Node`s. Public alias kept for callers that don't need
/// scoped lowering (build-time validation, snapshot tests).
pub fn lower_document(document: &AstDocument) -> Vec<Node> {
    lower_document_with_scope(document, &LowerScope::default())
}

pub fn lower_document_with_scope(document: &AstDocument, scope: &LowerScope) -> Vec<Node> {
    // **Wave A (`prui-luau-fusion.md` §7.1)** — harvest every
    // top-level `<script>` block before the main walk.
    // The bodies run once in a per-document Lua state; the script's
    // top-level `local`s become document-scope bindings the
    // expression resolver consults. The `<script>` element itself
    // lowers to nothing (it's behaviour, not tree). Gated on the
    // `luau` feature — without it, scripts are inert (the HTML/SSR
    // path stays mlua-free).
    // **Wave H (§5.3/§5.4)** — inline `<style>` blocks
    // and `<import stylesheet="…">` sidecars merge into the document
    // stylesheet *before* the Luau frame is built, so a computed
    // `{ lua = "…" }` value (Wave F) in an inline sheet still
    // evaluates against the frame at apply time. Successive sheets
    // layer in order (later wins); the pre-existing scope sheet (a
    // host-installed sidecar) is the base. No `<style>`/`<import>`
    // → untouched.
    let style_owned;
    let scope: &LowerScope = {
        let mut bodies = collect_inline_stylesheets(&document.nodes);
        if let Some(res) = scope.import_resolver() {
            for imp in collect_imports(&document.nodes) {
                if imp.kind == "stylesheet" {
                    if let Some(src) = res.resolve_import("stylesheet", &imp.path) {
                        bodies.push(src);
                    }
                }
            }
        }
        if bodies.is_empty() {
            scope
        } else {
            let mut merged = scope
                .stylesheet_arc()
                .map(|a| (*a).clone())
                .unwrap_or_default();
            for body in &bodies {
                let (parsed, _errs) = prism_core::language::prss::parse(body);
                merged = merged.merged_with(parsed);
            }
            style_owned = scope.clone().with_stylesheet(Arc::new(merged));
            &style_owned
        }
    };

    #[cfg(feature = "luau")]
    let luau_owned;
    #[cfg(feature = "luau")]
    let scope: &LowerScope = {
        // Inline `<script>` blocks + the tier-1 sibling are flat
        // (top-level locals merge into document scope).
        let mut modules: Vec<crate::luau_scope::LuauModule> =
            collect_script_bodies(&document.nodes)
                .into_iter()
                .map(crate::luau_scope::LuauModule::flat)
                .collect();
        // **Wave H.6 (§5.9)** — `<import script="…" [as="ns"]/>` and
        // `<import dialect="…"/>`. A `script` import *with* `as=` is
        // a tier-2 named module (isolated, bound under `ns`); without
        // `as=` it flat-merges (back-compat). A `dialect` file is a
        // script that calls `prism.dialect{…}` — always flat (its
        // effect is the registration, not a namespace).
        if let Some(res) = scope.import_resolver() {
            for imp in collect_imports(&document.nodes) {
                if !matches!(imp.kind.as_str(), "script" | "dialect") {
                    continue;
                }
                let Some(src) = res.resolve_import(&imp.kind, &imp.path) else {
                    continue;
                };
                match imp.alias {
                    Some(ns) if imp.kind == "script" => {
                        modules.push(crate::luau_scope::LuauModule::named(ns, src));
                    }
                    _ => modules.push(crate::luau_scope::LuauModule::flat(src)),
                }
            }
        }
        // **Wave E.3 (§7.8)** — prepend host-supplied builtin
        // dialect/macro registrations *only* when the document
        // actually uses a dialect (`<language>` / desugared `~x{…}`
        // sigil). A plain document never pays for a Lua state just
        // because builtins are installed; a `<markdown>` document
        // gets `prism.dialect{…}` registered without hand-wiring.
        if !scope.builtin_scripts().is_empty() && document_uses_dialect(&document.nodes) {
            let mut prepended: Vec<crate::luau_scope::LuauModule> = scope
                .builtin_scripts()
                .iter()
                .map(|s| crate::luau_scope::LuauModule::flat(s.clone()))
                .collect();
            prepended.append(&mut modules);
            modules = prepended;
        }
        // **Wave B** — a script-less document can still use closure
        // builtins / pipes (`for="t in tasks | filter(|t| …)"`).
        // Those need a Lua state to compile the closure in, so
        // provision an empty (helpers + tokens only) frame when the
        // AST's expression text uses the closure / pipe sigils. The
        // scan is over parsed expression bodies only and the `||`
        // logical-or is stripped first, so a plain `{a || b}`
        // document never pays for a Lua state.
        if scope.luau_scope().is_some() {
            scope
        } else if !modules.is_empty() {
            let tokens = scope.binding("tokens");
            let scope_json = scope.bindings_json();
            // **Wave H.7 (§5.9 tier 3)** — transitively resolve the
            // `require("…")` graph through the same `ImportResolver`
            // as `<import>`, deps-first. No resolver → no `require`
            // (graceful, same as imports on wasm).
            let requires = match scope.import_resolver() {
                Some(res) => {
                    let roots: Vec<&str> = modules.iter().map(|m| m.source.as_str()).collect();
                    resolve_require_graph(&roots, res.as_ref())
                }
                None => Vec::new(),
            };
            match crate::luau_scope::LuauScopeFrame::from_modules_with_requires(
                &modules,
                &requires,
                tokens,
                Some(&scope_json),
            ) {
                Ok(frame) => {
                    // §4.3 — hand the host a retained clone so the
                    // event router can fire probes registered in this
                    // document's `<script>` after the render returns.
                    if let Some(sink) = scope.frame_sink() {
                        *sink.borrow_mut() = Some(frame.clone());
                    }
                    luau_owned = scope.clone().with_luau_scope(frame);
                    &luau_owned
                }
                // A script error is non-fatal: the document still
                // renders, just without script bindings. (Inline
                // diagnostics for the error land with the Wave I
                // type-checking pass.)
                Err(_) => scope,
            }
        } else if document_uses_luau_expr(&document.nodes) {
            let scope_json = scope.bindings_json();
            match crate::luau_scope::LuauScopeFrame::from_scripts_with_scope(
                &[],
                scope.binding("tokens"),
                Some(&scope_json),
            ) {
                Ok(frame) => {
                    if let Some(sink) = scope.frame_sink() {
                        *sink.borrow_mut() = Some(frame.clone());
                    }
                    luau_owned = scope.clone().with_luau_scope(frame);
                    &luau_owned
                }
                Err(_) => scope,
            }
        } else {
            scope
        }
    };

    // **Wave 14.3** — pre-scan the entire AST for `<teleport
    // to="…">` elements before the main lowering walk starts. Each
    // teleport's children are stashed by target id; later, when the
    // main walk hits a container / component carrying `id="<target>"`,
    // it appends those children to its own. The teleport element
    // itself lowers to nothing at its source position.
    let mut teleports: HashMap<String, Vec<AstNode>> = HashMap::new();
    collect_teleports(&document.nodes, &mut teleports);
    if teleports.is_empty() {
        return lower_children(&document.nodes, scope);
    }
    let scope_with_teleports = scope.clone().with_teleports(Arc::new(teleports));
    lower_children(&document.nodes, &scope_with_teleports)
}

/// **Wave A / §5.10** — collect the raw body of every top-level
/// `<script>` element, in source order. `<script>` *is* Luau —
/// there is no `lang=` selector (the grammar flags one as a
/// recoverable diagnostic), so every block feeds the Lua state. The
/// grammar parses a `<script>` block as a raw-text element (one
/// [`AstNode::Text`] child); multiple inline blocks concatenate at
/// the [`crate::luau_scope::LuauScopeFrame`] seam.
#[cfg(feature = "luau")]
fn collect_script_bodies(nodes: &[AstNode]) -> Vec<String> {
    let mut out = Vec::new();
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        if el.tag != "script" {
            continue;
        }
        // **§5.10** — `<script>` *is* Luau; there is no `lang=`
        // selector any more (the grammar flags it as a recoverable
        // diagnostic). Every `<script>` body feeds the Lua state.
        for child in &el.children {
            if let AstNode::Text { value, .. } = child {
                out.push(value.clone());
            }
        }
    }
    out
}

/// **Wave H / §5.10 (`prui-luau-fusion.md`)** — collect the body of
/// every top-level inline `<style>` block, in source order.
/// `<style>` *is* PRSS — no `lang=` selector. Grammar parses
/// `<style>` as raw-text (one [`AstNode::Text`] child).
fn collect_inline_stylesheets(nodes: &[AstNode]) -> Vec<String> {
    let mut out = Vec::new();
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        if el.tag != "style" {
            continue;
        }
        // **§5.10** — `<style>` *is* PRSS; no `lang=` selector.
        for child in &el.children {
            if let AstNode::Text { value, .. } = child {
                out.push(value.clone());
            }
        }
    }
    out
}

/// **Wave H (§5.4 / §5.9 / §5.10)** — a parsed `<import
/// KIND="path"/> [as <alias>]` row. `kind` is one of the three live
/// projections; `alias` carries the postfix `as` namespace (applied
/// for `script` imports — §5.9 tier 2 — parsed-only for the others).
struct ImportSpec {
    kind: String,
    path: String,
    /// Only consumed by the `#[cfg(feature = "luau")]` script-import
    /// path (§5.9 tier 2); the HTML/SSR build never namespaces Luau.
    #[cfg_attr(not(feature = "luau"), allow(dead_code))]
    alias: Option<String>,
}

/// **Wave H (§5.4)** — collect every `<import>` row. The `kind`/path
/// is the first attribute whose local name is one of the three
/// projections (`stylesheet` / `script` / `dialect`); an `as=`
/// attribute on the same element supplies the alias. The `widget=`
/// projection was Phase-0-removed (`docs/dev/prui-expressiveness-roadmap.md`
/// §7.0); Phase 7 reintroduces component imports under `component=`.
fn collect_imports(nodes: &[AstNode]) -> Vec<ImportSpec> {
    let mut out = Vec::new();
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        if el.tag != "import" {
            continue;
        }
        let alias = el.attributes.iter().find_map(|a| {
            (a.name.local == "as").then(|| match &a.value {
                AttributeValue::String { value, .. } => Some(value.clone()),
                _ => None,
            })?
        });
        for a in &el.attributes {
            if matches!(a.name.local.as_str(), "stylesheet" | "script" | "dialect") {
                if let AttributeValue::String { value, .. } = &a.value {
                    out.push(ImportSpec {
                        kind: a.name.local.clone(),
                        path: value.clone(),
                        alias: alias.clone(),
                    });
                }
            }
        }
    }
    out
}

/// **Wave H.7 (`prui-luau-fusion.md` §5.9 tier 3)** — extract every
/// literal `require("…")` / `require('…')` path from a Luau source,
/// skipping comments and string/long-string bodies so a `require`
/// token inside prose or a quote is never mistaken for a call. The
/// Lua expression form `require(expr)` (non-literal) is intentionally
/// not followed — only static, resolver-checkable paths participate
/// (design principle 1: the graph is sized up front).
#[cfg(feature = "luau")]
fn scan_require_literals(src: &str) -> Vec<String> {
    use prism_core::language::syntax::Scanner;
    // Consume through a `]]` long-bracket close (block comment / long
    // string terminator), or EOF.
    fn skip_to_long_close(sc: &mut Scanner) {
        loop {
            if sc.is_at_end() {
                break;
            }
            if sc.peek() == Some(']') && sc.peek_ahead(1) == Some(']') {
                sc.advance();
                sc.advance();
                break;
            }
            sc.advance();
        }
    }
    let mut sc = Scanner::new(src);
    let mut out = Vec::new();
    while let Some(c) = sc.peek() {
        // `--` line / `--[[ ]]` block comment.
        if c == '-' && sc.peek_ahead(1) == Some('-') {
            sc.advance();
            sc.advance();
            if sc.peek() == Some('[') && sc.peek_ahead(1) == Some('[') {
                sc.advance();
                sc.advance();
                skip_to_long_close(&mut sc);
            } else {
                while let Some(ch) = sc.peek() {
                    if ch == '\n' {
                        break;
                    }
                    sc.advance();
                }
            }
            continue;
        }
        // `[[ … ]]` long string.
        if c == '[' && sc.peek_ahead(1) == Some('[') {
            sc.advance();
            sc.advance();
            skip_to_long_close(&mut sc);
            continue;
        }
        // `"…"` / `'…'` short string.
        if c == '"' || c == '\'' {
            sc.advance();
            while let Some(ch) = sc.advance() {
                if ch == '\\' {
                    sc.advance();
                    continue;
                }
                if ch == c {
                    break;
                }
            }
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let word = sc
                .scan_while(|x| x.is_ascii_alphanumeric() || x == '_')
                .to_string();
            if word == "require" {
                while matches!(sc.peek(), Some(' ' | '\t' | '\n' | '\r')) {
                    sc.advance();
                }
                if sc.peek() == Some('(') {
                    sc.advance();
                    while matches!(sc.peek(), Some(' ' | '\t' | '\n' | '\r')) {
                        sc.advance();
                    }
                    if let Some(q @ ('"' | '\'')) = sc.peek() {
                        sc.advance();
                        let mut p = String::new();
                        while let Some(ch) = sc.advance() {
                            if ch == '\\' {
                                if let Some(n) = sc.advance() {
                                    p.push(n);
                                }
                                continue;
                            }
                            if ch == q {
                                break;
                            }
                            p.push(ch);
                        }
                        if !p.is_empty() {
                            out.push(p);
                        }
                    }
                }
            }
            continue;
        }
        sc.advance();
    }
    out
}

/// **Wave H.7** — depth-first, deps-first transitive resolution of
/// the `require` graph through the host [`ImportResolver`] (kind
/// `script`, the same seam `<import script>` uses). Each module is
/// emitted *after* its dependencies and *exactly once* (keyed by the
/// literal path); a re-entered path (cycle) is skipped — its
/// `require` then hits the not-yet-cached branch and errors, so a
/// structural cycle is a bounded load error, never a hang.
#[cfg(feature = "luau")]
fn resolve_require_graph(roots: &[&str], res: &dyn ImportResolver) -> Vec<(String, String)> {
    use std::collections::HashSet;
    fn visit(
        path: &str,
        res: &dyn ImportResolver,
        out: &mut Vec<(String, String)>,
        done: &mut HashSet<String>,
        stack: &mut HashSet<String>,
    ) {
        if done.contains(path) || stack.contains(path) {
            return;
        }
        let Some(src) = res.resolve_import("script", path) else {
            return;
        };
        stack.insert(path.to_string());
        for dep in scan_require_literals(&src) {
            visit(&dep, res, out, done, stack);
        }
        stack.remove(path);
        out.push((path.to_string(), src));
        done.insert(path.to_string());
    }
    let mut out = Vec::new();
    let mut done = HashSet::new();
    let mut stack = HashSet::new();
    for root in roots {
        for dep in scan_require_literals(root) {
            visit(&dep, res, &mut out, &mut done, &mut stack);
        }
    }
    out
}

/// **Wave B** — does any expression body in the document use the
/// closure (`|x| …`, `\fn(x) …`) or pipe (`|>`, ` | `) sigils?
/// Drives lazy provisioning of an empty Luau frame for script-less
/// documents. Scans only parsed expression text (attribute values,
/// interpolations, control-flow predicates) — never literal text
/// runs — and strips `||` first so a plain logical-or never
/// triggers a Lua state.
/// **Wave E.3 (§7.8)** — does the document contain a `<language>`
/// element (the desugar target of a `~name{…}` sigil too)? Gates
/// whether host builtin dialect scripts are worth a Lua state.
#[cfg(feature = "luau")]
fn document_uses_dialect(nodes: &[AstNode]) -> bool {
    nodes.iter().any(|n| match n {
        AstNode::Element(el) => el.tag == "language" || document_uses_dialect(&el.children),
        _ => false,
    })
}

#[cfg(feature = "luau")]
fn document_uses_luau_expr(nodes: &[AstNode]) -> bool {
    fn expr_has_sigil(body: &str) -> bool {
        let stripped = body.replace("||", "");
        stripped.contains("\\fn") || stripped.contains('|')
    }
    fn attr_has(value: &AttributeValue) -> bool {
        match value {
            AttributeValue::Expression(e) => expr_has_sigil(&e.body),
            AttributeValue::Template { parts, .. } => parts.iter().any(|p| match p {
                prism_core::language::prism_ui::ast::TemplatePart::Expression(e) => {
                    expr_has_sigil(&e.body)
                }
                prism_core::language::prism_ui::ast::TemplatePart::Literal { .. } => false,
            }),
            _ => false,
        }
    }
    nodes.iter().any(|node| match node {
        AstNode::Element(el) => {
            el.attributes.iter().any(|a| attr_has(&a.value))
                || document_uses_luau_expr(&el.children)
        }
        AstNode::Interpolation(e) => expr_has_sigil(&e.body),
        _ => false,
    })
}

/// **Wave 14.3** — recursive AST scan that collects every
/// `<teleport to="X">` element's children into a `target → AST
/// payload` map. Walks normal element children but stops at
/// `<teleport>` so a payload that itself contains a `<teleport>` is
/// kept verbatim (the inner teleport is re-discovered when the
/// payload is appended at the outer target and lowered through the
/// same `lower_document_with_scope` recursion — see the teleport
/// handler in `lower_element`).
/// Build a synthetic `Element` whose tag is `new_tag` and whose
/// attributes are `el`'s minus the `tag=` slot the runtime consumed
/// for dispatch routing. Used by `<dispatch tag="{expr}"/>` so the
/// rewritten element flows through the same lowering path an authored
/// element would have. Range data is copied verbatim — diagnostics
/// retain the original source span.
fn element_with_retagged(el: &Element, new_tag: &str) -> Element {
    let attributes = el
        .attributes
        .iter()
        .filter(|attr| {
            !(matches!(attr.name.namespace, AttributeNamespace::Bare) && attr.name.local == "tag")
        })
        .cloned()
        .collect();
    Element {
        tag: new_tag.to_string(),
        attributes,
        children: el.children.clone(),
        self_closing: el.self_closing,
        range: el.range,
        tag_range: el.tag_range,
    }
}

fn collect_teleports(nodes: &[AstNode], out: &mut HashMap<String, Vec<AstNode>>) {
    for node in nodes {
        let AstNode::Element(el) = node else { continue };
        if el.tag == "teleport" {
            if let Some(target) = teleport_target(el) {
                out.entry(target)
                    .or_default()
                    .extend(el.children.iter().cloned());
            }
            // Don't recurse into a teleport's children — they're the
            // payload, not search targets for nested teleports.
            continue;
        }
        collect_teleports(&el.children, out);
    }
}

/// **Wave 14.3** — extract the literal `to="X"` attribute from a
/// `<teleport>` element. Only literal-string targets are recognised
/// today; expression-valued `to="{…}"` rounds-trip without routing
/// (the pre-scan has no scope to evaluate against). Authors who need
/// a dynamic destination wrap multiple `<teleport>` elements in an
/// `if=` chain instead.
fn teleport_target(el: &Element) -> Option<String> {
    for attr in &el.attributes {
        if matches!(attr.name.namespace, AttributeNamespace::Bare) && attr.name.local == "to" {
            if let AttributeValue::String { value, .. } = &attr.value {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

/// Lower an arbitrary AST sibling list with the given scope. Public
/// so [`TagResolver`] impls can pre-lower an element's children
/// before invoking a host-supplied component (e.g. composition-style
/// blocks like `shell.app-window` that host real subtrees from
/// `.prui` source). Re-uses the same control-flow + slot +
/// resolver-propagation logic the document walk uses — single
/// chokepoint for every "lower these AST children" call.
pub fn lower_ast_children(nodes: &[AstNode], scope: &LowerScope) -> Vec<Node> {
    lower_children(nodes, scope)
}

/// Walk a sibling list, expanding control flow then dispatching each
/// element through [`lower_node`]. The control-flow expansion runs
/// once per parent so `else-if`/`else` chains see their preceding
/// `if` siblings — handling them inside [`lower_node`] would lose
/// that context.
pub(super) fn lower_children(nodes: &[AstNode], scope: &LowerScope) -> Vec<Node> {
    let expanded = expand_control_flow(nodes, scope);
    let mut out = Vec::with_capacity(expanded.len());
    for (node, child_scope) in expanded {
        out.extend(lower_node(&node, child_scope.as_ref().unwrap_or(scope)));
    }
    out
}

fn lower_node(node: &AstNode, scope: &LowerScope) -> Vec<Node> {
    match node {
        AstNode::Element(el) => lower_element(el, scope),
        AstNode::Text { value, .. } => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![Node::Text {
                    id: String::new(),
                    content: interpolate(trimmed, scope),
                    props: TextProps::default(),
                }]
            }
        }
        AstNode::Interpolation(expr) => {
            // **Wave C** — inside a macro expansion, `{children}`
            // (and `<fragment>{children}</fragment>`) splices the
            // caller's already-lowered child nodes verbatim. The
            // macro path binds them via `with_host_children_ui`;
            // `<host-children/>` is the equivalent explicit form.
            if expr.body.trim() == "children" {
                if let Some(injected) = scope.host_children_ui() {
                    return injected.to_vec();
                }
            }
            let resolved = lookup_expression(&expr.body, scope)
                .map(stringify_value)
                .unwrap_or_default();
            if resolved.is_empty() {
                Vec::new()
            } else {
                vec![Node::Text {
                    id: String::new(),
                    content: resolved,
                    props: TextProps::default(),
                }]
            }
        }
        AstNode::Comment { .. } => Vec::new(),
    }
}

fn lower_element(el: &Element, scope: &LowerScope) -> Vec<Node> {
    // **Wave 14.3** — `memo="[dep1, dep2]"` cache. When the host
    // installed a [`MemoCache`] and this element carries both a
    // `memo` attribute and a resolvable `id`, check the cache
    // before lowering. A dep-tuple match returns the cached subtree
    // verbatim; a mismatch (or cache miss) falls through to the
    // normal lowering body and stores the result on the way out.
    // Authors without an id, or running on a host that didn't
    // install a cache, see `memo` round-trip as a no-op.
    if let Some(cache_handle) = scope.memo_cache() {
        // **Phase 3** — reactive dirty-set splice. When the shell
        // installed the per-frame dirty NodeId set, the signal graph
        // is authoritative: an id'd element is reused verbatim from
        // cache iff its own id is not dirty *and* its cached subtree
        // contains no dirty descendant. Anything dirty (or on its
        // ancestor path) re-lowers; the recursion prunes at the
        // highest clean boundary. Supersedes the authored-`memo=`
        // path while a dirty set is active.
        if let Some(dirty) = scope.dirty_nodes() {
            if let Some(id) = resolve_element_id(el, scope) {
                let reuse = !dirty.contains(&id)
                    && cache_handle
                        .borrow()
                        .entries
                        .get(&id)
                        .is_some_and(|(_, nodes)| !subtree_has_dirty(nodes, dirty));
                if reuse {
                    return cache_handle.borrow().entries[&id].1.clone();
                }
                let lowered = lower_element_body(el, scope);
                let mut cache = cache_handle.borrow_mut();
                cache
                    .entries
                    .insert(id.clone(), (Vec::new(), lowered.clone()));
                cache.touched.insert(id);
                return lowered;
            }
            return lower_element_body(el, scope);
        }

        // **Wave 14.3** — no dirty set: the authored `memo="[…]"`
        // dep-tuple cache. A match returns the cached subtree; a
        // mismatch re-lowers and stores.
        if let Some((deps, id)) = extract_memo_and_id(el, scope) {
            {
                let cache = cache_handle.borrow();
                if let Some((cached_deps, cached_nodes)) = cache.entries.get(&id) {
                    if cached_deps == &deps {
                        return cached_nodes.clone();
                    }
                }
            }
            let lowered = lower_element_body(el, scope);
            cache_handle
                .borrow_mut()
                .entries
                .insert(id, (deps, lowered.clone()));
            return lowered;
        }

        // **Phase 3** — even with no authored `memo=`, populate the
        // per-id cache on a full (dirty-set-less) pass so the *next*
        // reactive frame can splice this subtree. Elements without a
        // resolvable id can't be keyed and always re-lower.
        if let Some(id) = resolve_element_id(el, scope) {
            let lowered = lower_element_body(el, scope);
            cache_handle
                .borrow_mut()
                .entries
                .insert(id, (Vec::new(), lowered.clone()));
            return lowered;
        }
    }
    lower_element_body(el, scope)
}

/// **Phase 3** — does any node in `nodes` (recursively) carry an id
/// in `dirty`? Used to decide whether a cached subtree is safe to
/// splice: a clean cached subtree is reused; one containing a dirty
/// descendant re-lowers so the change propagates.
fn subtree_has_dirty(nodes: &[Node], dirty: &std::collections::HashSet<String>) -> bool {
    nodes.iter().any(|n| {
        let id = n.id();
        (!id.is_empty() && dirty.contains(id))
            || matches!(n, Node::Container { children, .. } if subtree_has_dirty(children, dirty))
    })
}

/// **Wave 14.3 / Phase 3** — resolve an element's `id` attribute to
/// a non-empty string, or `None`. Shared by the memo-dep path and
/// the reactive dirty-set splice so id resolution lives once.
fn resolve_element_id(el: &Element, scope: &LowerScope) -> Option<String> {
    el.attributes
        .iter()
        .find(|a| {
            matches!(a.name.namespace, AttributeNamespace::Identifier) && a.name.local == "id"
        })
        .and_then(|a| resolved_attribute_string(&a.value, scope))
        .filter(|s| !s.is_empty())
}

/// **Wave 14.3** — extract the memo dep tuple and the resolved id
/// of an element in one pass. Returns `None` when *either* the
/// element has no `memo=` attribute or its id resolves to an empty
/// string — both are required for the cache to key cleanly.
fn extract_memo_and_id(
    el: &Element,
    scope: &LowerScope,
) -> Option<(Vec<serde_json::Value>, String)> {
    let memo = el
        .attributes
        .iter()
        .find_map(|attr| match attr.name.namespace {
            AttributeNamespace::Bare if attr.name.local == "memo" => {
                let body = attribute_string(&attr.value).unwrap_or_default();
                Some(eval_memo_deps(&body, scope))
            }
            _ => None,
        })?;
    let id = resolve_element_id(el, scope)?;
    Some((memo, id))
}

/// **Wave 14.3** — evaluate a `memo="[a, b]"` body to a list of
/// JSON values. The leading `[` and trailing `]` are optional —
/// `memo="a, b"` and `memo="[a, b]"` are equivalent. Each
/// comma-separated expression goes through the same Pratt parser
/// the `{a + b}` interpolation path uses, so `memo="count, mode"`
/// reads bare bindings and `memo="state.kind, items.length"`
/// resolves dotted paths.
fn eval_memo_deps(body: &str, scope: &LowerScope) -> Vec<serde_json::Value> {
    let body = body
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');
    body.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|expr| {
            lookup_expression(expr, scope)
                .cloned()
                .or_else(|| evaluate_expression(expr, scope))
                .unwrap_or(serde_json::Value::Null)
        })
        .collect()
}

/// The original `lower_element` body, factored out so the memo
/// short-circuit at the top of `lower_element` can wrap it cleanly.
/// Every author-visible lowering still flows through this function;
/// the memo gate is purely a cache layer.
fn lower_element_body(el: &Element, scope: &LowerScope) -> Vec<Node> {
    // **`<dispatch tag="{expr}"/>` — runtime-tag dispatch.** When the
    // tag attribute resolves to a closed-set runtime primitive
    // (`container`, `text`, `heading`, `image`, `spacer`, `input`,
    // `fragment`, `slot`, `host-children`), rebuild a synthetic
    // element with the resolved tag and lower it through the same
    // primitive arms below. When it resolves to anything else, the
    // synthetic element falls through to the resolver — which now
    // sees the resolved tag instead of `dispatch`, so plugin / shell
    // tags route uniformly. Closes the §15 PRUI-reference gap that
    // listed `<{panel.tag} .../>` as inexpressible: data-driven tag
    // routing now works for the runtime's primitive vocabulary as
    // well as registered tags. The pre-existing `component=` form
    // (resolver-side dispatch by registered-component id) keeps
    // working — that path is still handled by `RegistryTagResolver`
    // when the synthesised tag stays "dispatch".
    if el.tag == "dispatch" {
        if let Some(target_tag) = bare_attr_value(el, "tag", scope) {
            let trimmed = target_tag.trim();
            if !trimmed.is_empty() && trimmed != "dispatch" {
                let synthetic = element_with_retagged(el, trimmed);
                return lower_element_body(&synthetic, scope);
            }
        }
    }
    match el.tag.as_str() {
        "container" | "component" => {
            let mut props = ContainerProps::default();
            let mut id = String::new();
            apply_container_attributes(el, scope, &mut props, &mut id);
            // **PRSS descendant selectors** — extend the ancestor
            // class chain for child lowering so `[class."btn icon"]`
            // can match an `<icon>` nested under this container's
            // `class="btn"`. The active-class collection runs again
            // here (and once already inside `apply_container_attributes`);
            // it's a cheap walk over the element's attribute list and
            // keeps the function-level seam intact. Empty class set
            // skips the scope clone via `with_class_chain_appending`'s
            // early return.
            let active = active_class_names(el, scope);
            let child_scope_owned;
            let child_scope: &LowerScope = if active.is_empty() {
                scope
            } else {
                child_scope_owned = scope.clone().with_class_chain_appending(active);
                &child_scope_owned
            };
            let mut children = lower_children(&el.children, child_scope);
            // **Wave 14.3** — `<teleport to="X">payload</teleport>`
            // routing. Any teleport in the document whose `to` matches
            // this element's id appends its payload here, lowered in
            // the target's scope. Payload binding resolution flows
            // from the destination, not the source — overlay tags at
            // the root naturally see the root scope. Authors who need
            // source-scope bindings should compute the values into
            // literal strings at the source.
            if !id.is_empty() {
                let payload = scope.teleport_payload_for(&id);
                if !payload.is_empty() {
                    children.extend(lower_children(payload, scope));
                }
            }
            vec![Node::Container {
                id,
                props,
                children,
            }]
        }
        // **Wave 14.3** — at its source position, a `<teleport>`
        // emits nothing. Its children were collected by the
        // document-level pre-scan and will land at the target site
        // when the matching `id="…"` is lowered.
        "teleport" => Vec::new(),
        // **Wave A** — `<script>` is behaviour, `<style>` is theme;
        // neither is tree. Their bodies are harvested in the
        // document pre-pass (`collect_script_bodies`) / the Wave H
        // stylesheet pass — at their source position they render
        // nothing. Listed here so they never fall through to the
        // unknown-tag resolver and accidentally surface as an empty
        // container.
        // **Wave H** — `<import>` is resolved in the document
        // pre-pass; at its source position it renders nothing.
        "script" | "style" | "import" => Vec::new(),
        // **Wave D (`prui-luau-fusion.md` §7.5)** — `<match on="{x}">`
        // with `<case>` children. Pure parser sugar: rewritten to a
        // synthetic `<let>` (the matched value, bound once) plus a
        // chained `if`/`else-if`/`else` over `<fragment>` wrappers,
        // then lowered through the existing control-flow expander.
        "match" => expand_match(el, scope),
        // A stray `<case>` (outside `<match>`) is malformed — it
        // renders nothing rather than leaking its body.
        "case" => Vec::new(),
        // **Wave D (§7.6)** — `<suspense>` / `<fallback>`. The
        // fallback body shows while any binding the primary subtree
        // reads is a pending coroutine marker; otherwise the primary
        // subtree renders (fallback stripped).
        "suspense" => expand_suspense(el, scope),
        // `<fallback>` only has meaning as a `<suspense>` child;
        // consumed there. Standalone → nothing.
        "fallback" => Vec::new(),
        // **Wave E (`prui-luau-fusion.md` §7.8)** — sub-dialect
        // block. `<language name="md">…</language>` (and the
        // `~md{…}` sigil it desugars from) routes the raw body
        // through the script-registered dialect's `parse(source)`,
        // then re-parses + lowers the returned `prui[[…]]` source.
        // Unknown dialect / no Luau scope → nothing (graceful, like
        // an unresolved tag).
        #[cfg(feature = "luau")]
        "language" => expand_language(el, scope),
        #[cfg(not(feature = "luau"))]
        "language" => Vec::new(),
        "text" | "heading" => {
            let mut props = TextProps::default();
            let mut id = String::new();
            apply_text_attributes(el, scope, &mut props, &mut id);
            if el.tag == "heading" && font_size_is_default(&props) {
                props.font_size = heading_font_size(el);
            }
            let content = collect_text_content(&el.children, scope);
            vec![Node::Text { id, content, props }]
        }
        "spacer" => vec![spacer_from(el, scope)],
        "input" => vec![input_from(el, scope)],
        "image" => vec![image_from(el, scope)],
        // `<slot/>` and `<slot name="x"/>` resolve to whatever the
        // caller injected. Lookup order (Wave 13.1):
        //   1. AST-level slot bindings (`LowerScope::slots`) — set
        //      when a parent component's body interpolated AST-level
        //      slot content (used by template expansion).
        //   2. Pre-lowered named-slot map (`host_children_by_slot`) —
        //      set when the resolver partitioned a dispatched
        //      element's children by `slot="X"` attribute.
        //   3. The element's own children (fallback content).
        // Default slot (no `name=`) maps to the empty-string slot
        // bucket when reading from the pre-lowered map.
        "slot" => {
            let name = bare_attr_value(el, "name", scope);
            if let Some(injected) = scope.slots.resolve(name.as_deref()) {
                return lower_children(injected, scope);
            }
            let key = name.as_deref().unwrap_or("");
            if let Some(injected) = scope.host_children_for_slot(key) {
                return injected.to_vec();
            }
            lower_children(&el.children, scope)
        }
        // Wave 11.2 — `<host-children/>` injection point. A DSL-
        // authored shell component (toast-stack, launchpad, app-window)
        // composes its caller's pre-lowered children at this seam.
        // The shell's `.prui` loader installs the children via
        // [`LowerScope::with_host_children_ui`] before invoking
        // `lower_document_with_scope`; here the runtime emits the
        // stored `Vec<Node>` verbatim. Falls back to the element's own
        // AST children (acting as a fallback slot) when nothing is
        // bound — same semantics `<slot/>` carries.
        //
        // Wave 13.1 — opt-in `name="X"` attribute pulls from the
        // pre-lowered named-slot map instead, so a DSL author can
        // pick either spelling.
        // **Fragment** — `<fragment>…</fragment>` (also `<></>`-shape
        // counterpart). React `<>…</>` / Vue `<template>` /
        // Svelte `<svelte:fragment>` equivalent. Emits children
        // verbatim with no wrapping container, useful for grouping
        // a multi-element `if`/`else` branch or `for` body without
        // imposing a flex parent. Drops `if=` / `for=` correctly
        // because those are handled at the sibling expansion layer.
        "fragment" => lower_children(&el.children, scope),
        // **A4** — declarative facet repeater. `<facet name="post"
        // from="state.posts">…</facet>` lowers its children once per
        // item in the resolved `from` source, binding each item to
        // the per-iteration scope under `name` (default `"item"`).
        // Sugars `<container for="post in state.posts">…</container>`
        // into a dedicated tag that reads at the call site as data
        // iteration rather than control-flow plumbing.
        //
        // Resolves through the same `resolve_for_iteration` helper
        // `for=` uses, so the `from` source accepts dotted-path
        // bindings (`state.posts`), virtual segments, ranges
        // (`0..5`), and the array/object iteration shapes — same
        // vocabulary, one resolver. Closes A4 of
        // `docs/dev/ui-migration-followups.md`.
        "facet" => {
            let name = bare_attr_value(el, "name", scope).unwrap_or_else(|| "item".to_string());
            let Some(from) = bare_attr_value(el, "from", scope) else {
                // No `from` → render nothing rather than panic. Same
                // shape as a `for=` with an unresolved source.
                return Vec::new();
            };
            let iter = resolve_for_iteration(&from, None, false, scope);
            let mut out = Vec::new();
            for (_key, item) in iter {
                let child_scope = scope.clone().with_binding(name.clone(), item);
                out.extend(lower_children(&el.children, &child_scope));
            }
            out
        }
        "host-children" => {
            if let Some(name) = bare_attr_value(el, "name", scope) {
                if let Some(injected) = scope.host_children_for_slot(&name) {
                    return injected.to_vec();
                }
            }
            match scope.host_children_ui() {
                Some(injected) => injected.to_vec(),
                None => lower_children(&el.children, scope),
            }
        }
        // Unknown tag — first ask the host's tag resolver (if any).
        // Hosts plug a `TagResolver` (e.g. `prism-builder`'s
        // `RegistryTagResolver`) through `LowerScope::with_resolver`
        // so registered component vocabularies (`shell.icon-button`,
        // user prefabs) materialise into runtime nodes here. If no
        // resolver claims the tag, fall back to the default behaviour:
        // drop the wrapping element and keep its children, so a host
        // can nest a scene inside `<scene>` without forcing the runtime
        // to know about it.
        _ => {
            // **Wave C (`prui-luau-fusion.md` §7.7)** — a Luau macro
            // tag. Checked *before* the host resolver so a script can
            // shadow / define element vocabulary document-locally.
            // The macro receives the caller's resolved attributes +
            // already-lowered children and returns `prui[[…]]` source
            // the host re-parses and lowers in a hygienic scope.
            #[cfg(feature = "luau")]
            if let Some(frame) = scope.luau_scope() {
                if frame.has_macro(&el.tag) {
                    return expand_macro_element(el, scope, frame);
                }
            }
            if let Some(resolver) = scope.resolver() {
                if let Some(nodes) = resolver.resolve(el, scope) {
                    return nodes;
                }
            }
            lower_children(&el.children, scope)
        }
    }
}

/// **Wave C** — expand a Luau macro element. Resolves the call
/// site's attributes to a JSON object and lowers its children in the
/// *caller's* scope (so `{caller_binding}` inside macro children
/// resolves at the call site, Vue-slot style), then hands both to
/// the macro. The returned `prui[[…]]` source is parsed and lowered
/// in a **hygienic** scope: document context (resolver, tokens,
/// Luau frame) is carried so nested tags / `{tokens.*}` / nested
/// macros still work, but the caller's PRUI bindings are dropped —
/// the macro body sees only `attrs` + `children` (§7.7 hygiene).
#[cfg(feature = "luau")]
fn expand_macro_element(
    el: &Element,
    scope: &LowerScope,
    frame: &crate::luau_scope::LuauScopeFrame,
) -> Vec<Node> {
    // Resolved attribute object: bare/identifier attrs keyed by
    // local name. Empty (valueless) attrs are booleans.
    let mut attrs = serde_json::Map::new();
    for a in &el.attributes {
        if !matches!(
            a.name.namespace,
            AttributeNamespace::Bare | AttributeNamespace::Identifier
        ) {
            continue;
        }
        let v = match &a.value {
            AttributeValue::Empty => serde_json::Value::Bool(true),
            AttributeValue::Expression(e) => lookup_expression(&e.body, scope)
                .cloned()
                .or_else(|| evaluate_expression(&e.body, scope))
                .unwrap_or(serde_json::Value::Null),
            _ => resolved_attribute_string(&a.value, scope)
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        };
        attrs.insert(a.name.local.clone(), v);
    }
    let attrs_json = serde_json::Value::Object(attrs);

    // Caller's children, lowered at the call site.
    let children_nodes = lower_children(&el.children, scope);
    let children_json = serde_json::to_value(&children_nodes).unwrap_or(serde_json::Value::Null);

    let src = match frame.expand_macro(&el.tag, &attrs_json, &children_json) {
        Some(Ok(s)) => s,
        // Macro error / not-a-macro (shouldn't happen — `has_macro`
        // gated) → render nothing rather than a broken subtree.
        _ => return Vec::new(),
    };

    let (doc, errors) = parse(&src);
    if !errors.is_empty() {
        return Vec::new();
    }

    // Hygienic scope: keep document context, drop caller bindings.
    let mut macro_scope = LowerScope::default().with_luau_scope(frame.clone());
    if let Some(resolver) = scope.resolver() {
        macro_scope = macro_scope.with_resolver(Arc::clone(resolver));
    }
    if let Some(tokens) = scope.binding("tokens") {
        macro_scope = macro_scope.with_binding("tokens", tokens.clone());
    }
    // **Wave C** — re-thread the PRSS sheet so a macro body's
    // `class="…"` resolves named classes exactly as it would at the
    // call site. Installed after the `tokens` binding so the sheet's
    // `[tokens.*]` overrides cascade over it (the §4.6 order), then
    // the macro-specific bindings layer on last.
    if let Some(sheet) = scope.stylesheet_arc() {
        macro_scope = macro_scope.with_stylesheet(sheet);
    }
    macro_scope = macro_scope
        .with_binding("attrs", attrs_json)
        .with_host_children_ui(children_nodes);
    lower_document_with_scope(&doc, &macro_scope)
}

// ---------------------------------------------------------------------------
// Per-element attribute application
// ---------------------------------------------------------------------------

fn spacer_from(el: &Element, scope: &LowerScope) -> Node {
    let mut id = String::new();
    let mut width = 0.0;
    let mut height = 0.0;
    for attr in &el.attributes {
        if !matches!(
            attr.name.namespace,
            AttributeNamespace::Bare | AttributeNamespace::Identifier
        ) {
            continue;
        }
        let raw = resolved_attribute_string(&attr.value, scope);
        match attr.name.local.as_str() {
            "id" => id = raw.unwrap_or_default(),
            "width" => width = raw.as_deref().and_then(parse_f32).unwrap_or(0.0),
            "height" => height = raw.as_deref().and_then(parse_f32).unwrap_or(0.0),
            _ => {}
        }
    }
    Node::Spacer { id, width, height }
}

/// Lower `<image src="…" width="…" height="…"/>` to [`Node::Image`].
/// Wave 11.2: closes the last shape gap blocking icon-bearing shell
/// components (icon-button, nav-button, section-header, app-card, …)
/// from migrating to `.prui` source. Same attr vocabulary the
/// container shape uses — `width`/`height` parse through
/// [`parse_sizing`], `style:radius` builds a uniform [`CornerRadius`],
/// `style:tint` parses through [`parse_color`], `aria-label` /
/// `aria:*` / `data:*` round-trip onto [`Semantic`].
fn image_from(el: &Element, scope: &LowerScope) -> Node {
    let mut id = String::new();
    let mut source = String::new();
    let mut width = Sizing::default();
    let mut height = Sizing::default();
    let mut radius = CornerRadius::default();
    let mut tint: Option<Color> = None;
    let mut semantic = Semantic::default();
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        let raw = resolved_attribute_string(&attr.value, scope);
        match attr.name.namespace {
            AttributeNamespace::Bare => match local {
                "src" | "source" => source = raw.unwrap_or_default(),
                "width" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        width = s;
                    }
                }
                "height" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        height = s;
                    }
                }
                "tag" => semantic.tag = raw,
                "role" => semantic.role = raw,
                "aria-label" => semantic.aria_label = raw,
                _ => {}
            },
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    id = v;
                }
            }
            AttributeNamespace::Style => match local {
                "radius" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        radius = CornerRadius {
                            tl: v,
                            tr: v,
                            br: v,
                            bl: v,
                        };
                    }
                }
                "tint" | "color" => {
                    if let Some(c) = raw.as_deref().and_then(parse_color) {
                        tint = Some(c);
                    }
                }
                _ => {}
            },
            AttributeNamespace::Aria => {
                if let Some(value) = raw {
                    semantic.attrs.push((format!("aria-{}", local), value));
                }
            }
            AttributeNamespace::Data | AttributeNamespace::Route => {
                if let Some(value) = raw {
                    semantic.attrs.push((format!("data-{}", local), value));
                }
            }
            _ => {}
        }
    }
    Node::Image {
        id,
        source,
        width,
        height,
        radius,
        tint,
        semantic,
    }
}

fn input_from(el: &Element, scope: &LowerScope) -> Node {
    let mut id = String::new();
    let mut value = String::new();
    let mut placeholder = String::new();
    let mut props = TextProps::default();
    let mut width = Sizing::default();
    let mut height = Sizing::default();
    let mut background: Option<crate::command::Color> = None;
    let mut hover: Option<crate::layout::HoverOverrides> = None;
    let mut focused = false;
    let mut multiline = false;
    let mut caret_byte: Option<usize> = None;
    let mut selection: Option<(usize, usize)> = None;
    let mut spans: Vec<crate::command::TextSpan> = Vec::new();
    let mut scroll_x: f32 = 0.0;
    let mut scroll_y: f32 = 0.0;
    let mut underline: Option<(usize, usize)> = None;
    let mut bracket_match: Vec<usize> = Vec::new();
    let mut highlight_current_line = false;
    // `syntax-language` chooses the tokenizer the interpret pass
    // runs against the input's `value`. Set by the code editor's
    // DSL (`<input syntax-language="luau"/>`); ignored when empty.
    let mut syntax_language: Option<String> = None;
    let mut semantic = Semantic::default();
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        let raw = resolved_attribute_string(&attr.value, scope);
        match attr.name.namespace {
            AttributeNamespace::Bare => match local {
                "value" => value = raw.unwrap_or_default(),
                "placeholder" => placeholder = raw.unwrap_or_default(),
                "font-size" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        props.font_size = v;
                    }
                }
                "width" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        width = s;
                    }
                }
                "height" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        height = s;
                    }
                }
                // Wave 11.3 — focus state surfaces through the DSL so
                // `<input focused="{is-focused}"/>` reports the
                // live edit target without a Rust helper.
                "focused" => {
                    focused = matches!(raw.as_deref(), Some("true"));
                }
                // Editor support: `multiline="true"` flips the input
                // from string-property mode (single line, newlines
                // stripped) to code-editor mode (`\n` is real, Up/Down
                // arrows navigate rows). `caret-byte` + `selection`
                // surface the host's editor state so the renderer can
                // paint caret + selection at the exact byte offsets.
                "multiline" => {
                    multiline = matches!(raw.as_deref(), Some("true"));
                }
                "caret-byte" => {
                    if let Some(n) = raw.as_deref().and_then(|s| s.parse::<usize>().ok()) {
                        caret_byte = Some(n);
                    }
                }
                // `selection="<start>,<end>"` — two non-negative byte
                // offsets, comma-separated, with `start <= end`. The
                // shell binding emits this only when an active
                // selection is non-empty.
                "selection" => {
                    if let Some(s) = raw.as_deref() {
                        if let Some((a, b)) = s.split_once(',') {
                            if let (Ok(a), Ok(b)) =
                                (a.trim().parse::<usize>(), b.trim().parse::<usize>())
                            {
                                if a < b {
                                    selection = Some((a, b));
                                }
                            }
                        }
                    }
                }
                // `scroll-x` / `scroll-y` — host-controlled
                // viewport offsets. Subtracted from the text's
                // draw origin and combined with a scissor at
                // emit time so glyphs outside the bounds are
                // clipped. The shell auto-updates these to keep
                // the caret visible on every editor mutation.
                "scroll-x" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        scroll_x = v;
                    }
                }
                "scroll-y" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        scroll_y = v;
                    }
                }
                // `bracket-match="<open>,<close>"` — pair of byte
                // offsets the renderer should paint thin outlines
                // around. Used for the matching-bracket highlight.
                "bracket-match" => {
                    if let Some(s) = raw.as_deref() {
                        if let Some((a, b)) = s.split_once(',') {
                            if let (Ok(a), Ok(b)) =
                                (a.trim().parse::<usize>(), b.trim().parse::<usize>())
                            {
                                bracket_match = vec![a, b];
                            }
                        }
                    }
                }
                // `highlight-current-line="true"` — paint a subtle
                // strip behind the caret's row. Default off so
                // inline string-property fields don't gain one.
                "highlight-current-line" => {
                    highlight_current_line = matches!(raw.as_deref(), Some("true"));
                }
                // `underline="<start>,<end>"` — byte range the
                // renderer should paint an underline through. Used
                // by IME preedit decoration; same parser shape as
                // `selection`.
                "underline" => {
                    if let Some(s) = raw.as_deref() {
                        if let Some((a, b)) = s.split_once(',') {
                            if let (Ok(a), Ok(b)) =
                                (a.trim().parse::<usize>(), b.trim().parse::<usize>())
                            {
                                if a < b {
                                    underline = Some((a, b));
                                }
                            }
                        }
                    }
                }
                // `syntax-language="luau"` — the input's value is
                // tokenized against this language and the resulting
                // [`TextSpan`]s ride the runtime node into the paint
                // pass. Empty / unknown languages produce no spans
                // (the input paints in `style:color`).
                "syntax-language" => {
                    if let Some(s) = raw {
                        if !s.is_empty() {
                            syntax_language = Some(s);
                        }
                    }
                }
                _ => {}
            },
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    id = v;
                }
            }
            AttributeNamespace::Style if local == "color" => {
                if let Some(c) = raw.as_deref().and_then(parse_color) {
                    props.color = c;
                }
            }
            // `style:background="#…"` / `style:background:hovered="#…"`
            // give the input its own resting + hover fill. Without these
            // the input falls back to the legacy white default at emit
            // time. Inline rows (property fields) use this to put the
            // hover affordance on the input itself rather than spilling
            // it onto the parent row.
            AttributeNamespace::Style => {
                let (key, state) = split_state_suffix(local);
                if key == "background" {
                    if let Some(c) = raw.as_deref().and_then(parse_color) {
                        match state {
                            None => background = Some(c),
                            Some("hovered") => {
                                hover
                                    .get_or_insert_with(crate::layout::HoverOverrides::default)
                                    .background = Some(c);
                            }
                            _ => {}
                        }
                    }
                }
            }
            // `bind:value="<node-id>.<key>"` lowers to a
            // `data-bind-value` semantic attr on the input. The shell
            // event router consumes it at pointer-down time to start
            // a field-focus session against the bound source. Same
            // round-trip shape the container path uses for `bind:`.
            AttributeNamespace::Bind => {
                if let Some(v) = raw {
                    semantic.attrs.push((format!("data-bind-{}", local), v));
                }
            }
            AttributeNamespace::Facet => {
                if let Some(v) = raw {
                    semantic.attrs.push((format!("data-fct-{}", local), v));
                }
            }
            AttributeNamespace::Signal => {
                if let Some(v) = raw {
                    semantic.attrs.push((format!("data-sig-{}", local), v));
                }
            }
            // `aria:*` / `data:*` / `route:*` pass through to the
            // semantic carrier so inputs participate in the same
            // hit-test / SSR routing the containers do.
            AttributeNamespace::Aria => {
                if let Some(v) = raw.filter(|s| !s.is_empty()) {
                    semantic.attrs.push((format!("aria-{}", local), v));
                }
            }
            AttributeNamespace::Data => {
                if let Some(v) = raw.filter(|s| !s.is_empty()) {
                    semantic.attrs.push((format!("data-{}", local), v));
                }
            }
            AttributeNamespace::Route => {
                if let Some(v) = raw {
                    semantic.attrs.push((format!("data-{}", local), v));
                }
            }
            _ => {}
        }
    }
    // Resolve `syntax-language` against the input's `value` to
    // produce per-byte spans. Done at DSL-lower time (rather than
    // at paint time) so the same shape JSON-snapshots / SSR
    // emitters reach can carry the highlighted ranges unchanged —
    // and so a code-editor input that's bound to a 5000-line
    // buffer doesn't retokenize on every animator tick.
    if let Some(lang) = syntax_language.as_deref() {
        if !value.is_empty() {
            spans = crate::syntax::highlight(&value, lang);
        }
    }
    // Interaction defaults — single-line inputs are the canonical
    // "property-row text box" surface, so resting + hover backgrounds
    // come for free. `.prui` authors override with `style:background`
    // / `style:background:hovered` when they want a different palette.
    // Multi-line inputs (code editors, text-buffer primitives) keep
    // the legacy white-on-nothing default so a 5000-line editor
    // doesn't gain a stray hover tint.
    if !multiline {
        if background.is_none() {
            background = Some(crate::command::Color {
                r: 0xf5,
                g: 0xf5,
                b: 0xf7,
                a: 0xff,
            });
        }
        let needs_hover = hover.as_ref().is_none_or(|h| h.background.is_none());
        if needs_hover {
            hover
                .get_or_insert_with(crate::layout::HoverOverrides::default)
                .background = Some(crate::command::Color {
                r: 0xe7,
                g: 0xe9,
                b: 0xee,
                a: 0xff,
            });
        }
    }
    Node::TextInput {
        id,
        value,
        placeholder,
        props,
        width,
        height,
        radius: CornerRadius::default(),
        semantic,
        background,
        hover,
        focused,
        multiline,
        caret_byte,
        selection,
        spans,
        scroll_x,
        scroll_y,
        underline,
        bracket_match,
        highlight_current_line,
    }
}

fn apply_container_attributes(
    el: &Element,
    scope: &LowerScope,
    props: &mut ContainerProps,
    id: &mut String,
) {
    // **PRSS pre-pass.** Resolve any `class="…"` attribute *and* any
    // `class:<name>="{cond}"` toggles before the per-attribute loop
    // so user inline `style:` overrides always win on conflicts (the
    // canonical specificity rule from `prss-reference.md` §4.6).
    // The Svelte-style toggle layers truthy class names on top of the
    // static `class="…"` list, in document order: a later
    // `class:foo="{true}"` wins on conflicts the same way a literal
    // `class="… foo"` would. No-op when no stylesheet is loaded
    // (headless / SSR / first-boot path) — both forms round-trip as
    // stale-but-harmless authored attrs in that case. The active
    // class set is *always* mirrored into [`Semantic::class`] so the
    // SSR / semantic-HTML emitters pick up the same `class="…"`
    // author intent — even when no stylesheet is loaded the
    // static + toggle list still round-trips through HTML.
    //
    // **Descendant selectors** apply *after* flat classes so a more
    // specific match (`btn icon`) overrides the flat (`icon`) entry,
    // matching CSS specificity ordering. The chain comes from the
    // outer `lower_element_body`, which threads each container's
    // active classes into [`LowerScope::with_class_chain_appending`]
    // before lowering children.
    let active = active_class_names(el, scope);
    if !active.is_empty() {
        // Dedup while preserving first-seen order so authors writing
        // `class="btn" class:btn="{true}"` don't get a doubled-up
        // class attr through the SSR backend.
        let mut seen: Vec<&str> = Vec::with_capacity(active.len());
        for name in &active {
            if !seen.contains(&name.as_str()) {
                seen.push(name.as_str());
            }
        }
        props.semantic.class = Some(seen.join(" "));
    }
    if let Some(sheet) = scope.stylesheet() {
        for class_name in &active {
            apply_prss_class(sheet, class_name, props, scope);
        }
        apply_descendant_selectors(sheet, &active, scope, props);
        // **Fusion F.3** — record the class→NodeId dependency so a
        // later `.prss` literal swap can mark just this element dirty
        // (Phase 3 then splices the rest). Keyed by the element's
        // resolvable id; anonymous containers can't be targeted
        // selectively and fall back to the broad path.
        if let Some(deps) = scope.class_deps() {
            if let Some(node_id) = resolve_element_id(el, scope) {
                let mut deps = deps.borrow_mut();
                for class_name in &active {
                    deps.record(class_name, &node_id);
                }
            }
        }
    }
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        let raw = resolved_attribute_string(&attr.value, scope).map(|v| {
            // **Short-name token references** — `padding="md"` /
            // `style:background="accent"` resolve to
            // `tokens.spacing.md` / `tokens.colors.accent` *before*
            // the per-attribute parse runs. Limited to Bare + Style
            // attrs so `data:role="md"` / `aria:level="md"` don't
            // accidentally substitute. See `prss-reference.md` §6.
            // **Token-driven rem base** — any `Nrem` / `Nem` segments
            // expand to pixel-resolved numerics using
            // [`LowerScope::rem_px`] so a custom
            // `tokens.typography.font-size-md` rescales every length
            // through the same parser uniformly.
            match attr.name.namespace {
                AttributeNamespace::Bare | AttributeNamespace::Style => {
                    let resolved = resolve_short_token(local, &v, scope).unwrap_or(v);
                    expand_length_units(&resolved, scope)
                }
                _ => v,
            }
        });
        match attr.name.namespace {
            AttributeNamespace::Bare => match local {
                "direction" => {
                    if let Some(v) = raw.as_deref() {
                        props.direction = parse_direction(v);
                    }
                }
                "gap" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        props.gap = v;
                    }
                }
                "padding" => {
                    // CSS-shorthand: `padding="8"` (uniform),
                    // `padding="8 16"`, `padding="8 16 24"`,
                    // `padding="8 16 24 32"` (TRBL). Single seam with
                    // the inline `style:padding` lowering path —
                    // both call into `parse_padding_shorthand`.
                    if let Some(p) = raw.as_deref().and_then(parse_padding_shorthand) {
                        props.padding = p;
                    }
                }
                "padding-left" => set_padding_side(&mut props.padding, raw.as_deref(), Side::Left),
                "padding-right" => {
                    set_padding_side(&mut props.padding, raw.as_deref(), Side::Right)
                }
                "padding-top" => set_padding_side(&mut props.padding, raw.as_deref(), Side::Top),
                "padding-bottom" => {
                    set_padding_side(&mut props.padding, raw.as_deref(), Side::Bottom)
                }
                "width" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        props.width = s;
                    }
                }
                "height" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        props.height = s;
                    }
                }
                // Wave 11.2 — Semantic surface for `.prui`-authored
                // shell components. Hand-rolled Rust blocks build
                // `Semantic::tag(..).with_role(..).with_aria_label(..)`
                // imperatively; the DSL needs the same vocabulary so
                // an author can write `<container tag="section"
                // role="navigation" aria-label="Pages"/>` against the
                // same struct. The dedicated fields land on
                // `props.semantic.{tag, role, aria_label}` (separate
                // from the generic `attrs` vec the `data:` / `aria:` /
                // `route:` namespaces append to) so the HTML emitter
                // picks them up at the same seam it always did.
                "tag" => {
                    if let Some(v) = raw {
                        props.semantic.tag = Some(v);
                    }
                }
                "role" => {
                    if let Some(v) = raw {
                        props.semantic.role = Some(v);
                    }
                }
                "aria-label" => {
                    if let Some(v) = raw {
                        props.semantic.aria_label = Some(v);
                    }
                }
                // Reconciliation hint (Vue `:key`, React `key={}`,
                // Svelte `(x.id)` keyed iteration). Round-trip as a
                // `data-key` semantic attr so SSR and the future
                // incremental-diff substrate inherit author intent
                // verbatim. The runtime tree-diff that would actually
                // consume this hint is the unblock; today it carries
                // through identically to a hand-authored
                // `data:key="…"`. Empty string drops cleanly so a
                // ternary that resolves to `""` omits the attr.
                "key" => {
                    if let Some(v) = raw {
                        if !v.is_empty() {
                            props.semantic.attrs.push(("data-key".to_string(), v));
                        }
                    }
                }
                _ => {}
            },
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    *id = v;
                }
            }
            // Wave 9.2: `style:<key>:<state>="<value>"` peels the
            // trailing `:hovered` / `:selected` / `:focused` suffix
            // and routes the override into the matching sparse
            // bundle. `:hovered` lands on
            // [`ContainerProps::hover`], which the layout pass
            // already swaps in when the node's id matches
            // [`Surface::hovered_id`]. `:selected` / `:focused` have
            // no container-level runtime infra yet, so the override
            // round-trips as a `data-style-<key>-<state>="<value>"`
            // semantic attribute — same shape as Wave 9.4
            // transitions: data carries the intent, runtime wiring
            // lands as a follow-up without changing the authoring
            // grammar.
            AttributeNamespace::Style => {
                if let Some(value) = raw.as_deref() {
                    apply_style_override(props, local, value);
                }
            }
            // §43 A1: `on:<event>="<action>"` lowers to a
            // `data-on-<event>` semantic attribute. The shell event
            // router reads it back at pointer-down time and dispatches
            // through `prism_builder::signal::parse_action`. Only
            // containers contribute to the runtime's hit-test cache,
            // so attaching handlers to text / spacer leaves is a
            // separate follow-up (every leaf with author-driven
            // events lives inside a container today).
            //
            // Empty-string filter (PRUI ref §4): `on:event=""` is
            // dropped so a ternary that resolves to `""` omits the
            // handler — matches the `data:` / `aria:` rule.
            //
            // **Wave 14.3** — recognise dotted event-modifier suffixes
            // (Vue `@click.once`, Svelte `on:click|once`). The dot
            // becomes a dash so the existing `data-on-<key>` hit-test
            // cache picks it up uniformly.
            AttributeNamespace::On => {
                if let Some(action) = raw.filter(|s| !s.is_empty()) {
                    let local_key = on_event_attr_key(local);
                    props
                        .semantic
                        .attrs
                        .push((format!("data-on-{}", local_key), action));
                }
            }
            // Wave 9.1: `route:<key>="<value>"` lowers to a
            // `data-<key>` semantic attribute. Lifts the hit-test
            // routing convention — `data-role`, `data-target-id`,
            // `data-direction`, etc. — into a typed namespace so
            // `.prui` authors write
            // `<container route:role="resize-handle" route:direction="br"/>`
            // instead of the bare `data-` ladder. The runtime
            // contract is the same — `data-*` attrs flow through
            // the hit-test cache verbatim.
            AttributeNamespace::Route => {
                if let Some(value) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-{}", local), value));
                }
            }
            // `aria:<role>="<value>"` and `data:<key>="<value>"` are
            // pass-through; preserve them on the semantic emission
            // so the HTML / SSR backends inherit them and the
            // hit-test cache can route off them like any `data-*`.
            //
            // Both filter empty resolved values so authors can write
            // a ternary (`aria:level="{depth > 0 ? depth + 1 : ''}"`,
            // `data:on-click="{enabled ? 'cmd save' : ''}"`) to omit
            // the attribute conditionally — same shape `on:` uses.
            AttributeNamespace::Aria => {
                if let Some(value) = raw.filter(|s| !s.is_empty()) {
                    props
                        .semantic
                        .attrs
                        .push((format!("aria-{}", local), value));
                }
            }
            AttributeNamespace::Data => {
                // Skip empty resolved values so authors can use a
                // ternary (`data:on-click="{cmd ? 'cmd ' + cmd : ''}"`)
                // to omit the attr conditionally — Wave 11.2 substrate
                // for the menu-item / row-variant migrations. A literal
                // `data-foo=""` was already meaningless to every
                // hit-test consumer (parse_action returned None) so
                // omitting it is the correct behaviour, not a change.
                if let Some(value) = raw.filter(|s| !s.is_empty()) {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-{}", local), value));
                }
            }
            // Wave 9.4: `transition:<prop>="<duration>"` records a
            // declarative animation hint as `data-transition-<prop>`
            // so the host can read it at install time. The runtime
            // `Effect`-driven animator that consumes the hint is
            // the follow-up — today the data round-trips through
            // the semantic attrs without behaviour change.
            AttributeNamespace::Transition => {
                if local == "easing" {
                    // **§4.1** — `transition:easing` selects the timing
                    // curve. A Luau closure (`{\fn(t) … end}`) is
                    // *sampled here* (the per-document Lua frame is
                    // live during lowering) into a comma-joined LUT so
                    // the animator never calls Lua per frame; a named
                    // keyword (`ease-in`, `linear`, …) round-trips
                    // verbatim. No frame / not a closure → the keyword
                    // path; unresolvable → attr omitted (animator
                    // defaults to linear).
                    if let Some(encoded) = encode_easing_attr(&attr.value, scope) {
                        props
                            .semantic
                            .attrs
                            .push(("data-transition-easing".to_string(), encoded));
                    }
                } else if let Some(value) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-transition-{}", local), value));
                }
            }
            // Wave 14.6 — `animate:<prop>="<from> <duration>"`
            // records the entry-transition hint as
            // `data-animate-in-<prop>`. The runtime animator
            // (`prism-ui-runtime::animator`) reads this attr on the
            // first observation of the node and starts a transition
            // from the parsed `from` to the prop's declared value.
            // No behaviour change on re-render — the entry runs once
            // per mount lifecycle.
            //
            // Wave 14.8 — explicit `animate:in-<prop>` and
            // `animate:out-<prop>` differentiate entry vs unmount
            // transitions; the bare `animate:<prop>` form remains a
            // shorthand for `animate:in-<prop>` so existing call
            // sites continue to work. `out` lowers to
            // `data-animate-out-<prop>` for the animator's pending
            // node-retention path to consume.
            AttributeNamespace::Animate => {
                if let Some(value) = raw {
                    let attr = if let Some(prop) = local.strip_prefix("in-") {
                        format!("data-animate-in-{}", prop)
                    } else if let Some(prop) = local.strip_prefix("out-") {
                        format!("data-animate-out-{}", prop)
                    } else {
                        format!("data-animate-in-{}", local)
                    };
                    props.semantic.attrs.push((attr, value));
                }
            }
            // **Wave G (§7.11)** — `probe:<name>="event-key"` taps a
            // value/interaction into the document probe stream.
            // Lowers to `data-probe-<name>` (same round-trip
            // discipline as `route:` / `use:`); `prism.probes:on`
            // subscribes. Live event firing off the data attr is a
            // host event-router follow-up (open question 3 family).
            AttributeNamespace::Probe => {
                if let Some(value) = raw.filter(|s| !s.is_empty()) {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-probe-{}", local), value));
                }
            }
            // **Wave G (§7.12)** — `at:<time>="{…}"` keyframe stop.
            // Lowers to `data-at-<time>` so the animator can read the
            // timeline at observe time; mirrors `transition:` /
            // `animate:` (the Effect-driven animator that consumes
            // these is the shared follow-up).
            AttributeNamespace::At => {
                if let Some(value) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-at-{}", local), value));
                }
            }
            // Wave 13.3: `use:<id>[="<value>"]` directive sugar for
            // attaching a registered `ModifierBehaviour`. Today the
            // namespace lowers to `data-use-<id>="<value>"` so author
            // intent round-trips through the SSR / hit-test caches;
            // full runtime modifier-fold integration follows when the
            // resolver-side `ModifierRegistry` thread-through lands.
            // Same data-round-trips-now pattern Wave 9.4 (transitions)
            // and Wave 9.2 (:selected / :focused state styles) use.
            AttributeNamespace::Use => {
                let value = raw.unwrap_or_else(|| "true".into());
                if !value.is_empty() {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-use-{}", local), value));
                }
            }
            // `bind:<key>="<source>"` is sugar for
            // `prism_builder::signal::ActionKind::Bind { target_key,
            // source }` — but the bind installer runs against the
            // canvas document's `connections` list, not the
            // interpreted shell skeleton. Surface the binding
            // verbatim as `data-bind-<key>` so the host can either
            // install the effect at boot (the dioxus-inspiration
            // Phase 4 path) or no-op cleanly during the SSR walk.
            AttributeNamespace::Bind => {
                if let Some(value) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-bind-{}", local), value));
                }
            }
            // `fct:<key>="<source>"` (facet) and `sig:<key>="<source>"`
            // (signal declaration) follow the same carry-through
            // pattern. The host walks the lowered tree post-interpret
            // and acts on `data-fct-*` / `data-sig-*` semantic attrs
            // — facets expand against `BuilderDocument::facets`,
            // signal declarations register against the shell's signal
            // scope. SSR backends pass them through to the rendered
            // HTML where consumers (e.g. relay JS) can pick them up.
            AttributeNamespace::Facet => {
                if let Some(value) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-fct-{}", local), value));
                }
            }
            AttributeNamespace::Signal => {
                if let Some(value) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-sig-{}", local), value));
                }
            }
            _ => {}
        }
    }
    // Interaction defaults for button-like containers. Authors signal
    // "this thing is clickable" via `tag="button"` (semantic HTML
    // intent) or `data:type="button"` (legacy carrier from the Slint
    // era). Either gets a resting + hover background for free — the
    // .prui file no longer needs `style:background:hovered="…"` on
    // every icon button / pill / tab to feel alive. Inline overrides
    // still win (the helper short-circuits when either field is set).
    let is_button_like = props.semantic.tag.as_deref() == Some("button")
        || props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-type" && v == "button");
    if is_button_like {
        if props.background.is_none() {
            props.background = Some(crate::command::Color {
                r: 0x00,
                g: 0x00,
                b: 0x00,
                a: 0x08,
            });
        }
        let needs_hover = props.hover.as_ref().is_none_or(|h| h.background.is_none());
        if needs_hover {
            props
                .hover
                .get_or_insert_with(crate::layout::HoverOverrides::default)
                .background = Some(crate::command::Color {
                r: 0x00,
                g: 0x00,
                b: 0x00,
                a: 0x14,
            });
        }
    }
}

fn apply_text_attributes(el: &Element, scope: &LowerScope, props: &mut TextProps, id: &mut String) {
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        // Mirror the resolution + rem-expansion seam used by
        // [`apply_container_attributes`] so `font-size="md"` reads
        // through `tokens.typography.font-size-md` and `font-size="1rem"`
        // honours the scope-driven rem base.
        let raw =
            resolved_attribute_string(&attr.value, scope).map(|v| match attr.name.namespace {
                AttributeNamespace::Bare | AttributeNamespace::Style => {
                    let resolved = resolve_short_token(local, &v, scope).unwrap_or(v);
                    expand_length_units(&resolved, scope)
                }
                _ => v,
            });
        match attr.name.namespace {
            AttributeNamespace::Bare if local == "font-size" => {
                if let Some(v) = raw.as_deref().and_then(parse_f32) {
                    props.font_size = v;
                }
            }
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    *id = v;
                }
            }
            AttributeNamespace::Style if local == "color" => {
                if let Some(c) = raw.as_deref().and_then(parse_color) {
                    props.color = c;
                }
            }
            _ => {}
        }
    }
}

fn font_size_is_default(props: &TextProps) -> bool {
    (props.font_size - TextProps::default().font_size).abs() < f32::EPSILON
}

/// `<heading level="N">` mirrors HTML — h1..h6 step down in 4pt
/// increments from a 28pt base, capped at the body default.
fn heading_font_size(el: &Element) -> f32 {
    let level = el
        .attributes
        .iter()
        .find(|a| a.name.raw == "level")
        .and_then(|a| attribute_string(&a.value))
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1);
    match level.clamp(1, 6) {
        1 => 28.0,
        2 => 24.0,
        3 => 20.0,
        4 => 18.0,
        5 => 16.0,
        _ => 14.0,
    }
}

fn collect_text_content(children: &[AstNode], scope: &LowerScope) -> String {
    let mut out = String::new();
    for c in children {
        match c {
            AstNode::Text { value, .. } => {
                let resolved = interpolate(value.trim(), scope);
                if resolved.is_empty() {
                    continue;
                }
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&resolved);
            }
            AstNode::Interpolation(expr) => {
                // Cheap bare-path lookup first (Wave 11.2 substrate),
                // with virtual `.length`/`.first`/`.last` segments
                // included; operator-bearing bodies (ternary, `||`,
                // `&&`, `==`) fall through to the full evaluator so
                // authors can write `<text>{text ? text : status}</text>`
                // against the same vocabulary attribute interpolations use.
                let resolved = if let Some(v) = lookup_path_owned(&expr.body, scope) {
                    stringify_value(&v)
                } else if let Some(v) = evaluate_expression(&expr.body, scope) {
                    stringify_value(&v)
                } else {
                    continue;
                };
                if resolved.is_empty() {
                    // Skip the leading-space insertion when an interpolation
                    // resolves to "" — `{text}{status}` with `status=""`
                    // shouldn't add a trailing separator.
                    continue;
                }
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&resolved);
            }
            _ => {}
        }
    }
    out
}

pub(super) fn bare_attr_value(el: &Element, name: &str, scope: &LowerScope) -> Option<String> {
    el.attributes
        .iter()
        .find(|a| matches!(a.name.namespace, AttributeNamespace::Bare) && a.name.local == name)
        .and_then(|a| resolved_attribute_string(&a.value, scope))
}

/// **Wave 14.2** — typed-aware bare-attribute reader for `<let>`.
/// For a pure `{expr}` body, returns the underlying
/// [`serde_json::Value`] (number / bool / object preserved). For a
/// templated `"prefix-{expr}"`, returns a resolved [`Value::String`]
/// — mixing literal text with interpolation can only land as a
/// string. Plain string attributes (`value="lit"`) land as
/// [`Value::String`] verbatim.
pub(super) fn evaluate_bare_attr_typed(
    el: &Element,
    name: &str,
    scope: &LowerScope,
) -> Option<serde_json::Value> {
    let attr = el
        .attributes
        .iter()
        .find(|a| matches!(a.name.namespace, AttributeNamespace::Bare) && a.name.local == name)?;
    match &attr.value {
        AttributeValue::String { value, .. } => Some(serde_json::Value::String(value.clone())),
        AttributeValue::Empty => None,
        AttributeValue::Expression(expr) => {
            if let Some(v) = lookup_expression(&expr.body, scope) {
                return Some(v.clone());
            }
            evaluate_expression(&expr.body, scope)
        }
        AttributeValue::Template { .. } => {
            resolved_attribute_string(&attr.value, scope).map(serde_json::Value::String)
        }
    }
}

/// Plain attribute reader — returns the raw text without resolving
/// `{expr}` interpolations against any scope. Used by `heading_font_size`
/// where the value must be a literal integer.
pub(super) fn attribute_string(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::String { value, .. } => Some(value.clone()),
        AttributeValue::Empty => None,
        AttributeValue::Expression(expr) => Some(format!("{{{}}}", expr.body)),
        AttributeValue::Template { parts, .. } => {
            let mut out = String::new();
            for part in parts {
                match part {
                    prism_core::language::prism_ui::ast::TemplatePart::Literal {
                        value, ..
                    } => out.push_str(value),
                    prism_core::language::prism_ui::ast::TemplatePart::Expression(e) => {
                        out.push('{');
                        out.push_str(&e.body);
                        out.push('}');
                    }
                }
            }
            Some(out)
        }
    }
}

/// Same as [`attribute_string`] but resolves `{expr}` parts through
/// the scope's bindings. Single seam for "interpolated attribute" —
/// every namespaced/bare reader routes through here so adding a new
/// expression form is one place to update.
fn resolved_attribute_string(value: &AttributeValue, scope: &LowerScope) -> Option<String> {
    fn resolve_one(body: &str, scope: &LowerScope) -> String {
        // Cheap path first — bare dotted-path binding lookup, with
        // virtual `.length`/`.first`/`.last` segments handled by
        // [`lookup_path_owned`]. Keeps typed `Value::String("hi")`
        // stringified verbatim (lookup returns `"hi"`, `stringify_value`
        // strips the quotes) and avoids paying the expression-parser
        // cost for plain `{name}` interpolations. Operator-bearing
        // expressions fall through to `evaluate_expression`.
        if let Some(v) = lookup_path_owned(body, scope) {
            return stringify_value(&v);
        }
        match evaluate_expression(body, scope) {
            Some(v) => stringify_value(&v),
            None => String::new(),
        }
    }
    match value {
        AttributeValue::String { value, .. } => Some(value.clone()),
        AttributeValue::Empty => None,
        AttributeValue::Expression(expr) => Some(resolve_one(&expr.body, scope)),
        AttributeValue::Template { parts, .. } => {
            let mut out = String::new();
            for part in parts {
                match part {
                    prism_core::language::prism_ui::ast::TemplatePart::Literal {
                        value, ..
                    } => out.push_str(value),
                    prism_core::language::prism_ui::ast::TemplatePart::Expression(e) => {
                        out.push_str(&resolve_one(&e.body, scope))
                    }
                }
            }
            Some(out)
        }
    }
}

/// Number of points a Luau easing closure is sampled at when lowered
/// to a `data-transition-easing` LUT. 24 stops resolves a cubic /
/// spring curve smoothly under linear inter-sample interpolation while
/// keeping the one-time sampling cost (24 cached Lua calls) trivial.
#[cfg_attr(not(feature = "luau"), allow(dead_code))]
const EASING_LUT_SAMPLES: usize = 24;

/// **§4.1 (`prism-cross-cutting-systems.md`)** — encode a
/// `transition:easing` value into the `data-transition-easing` attr
/// the [`crate::animator::Animator`] reads.
///
/// - A **named keyword** (`"ease-in"`, `linear`, …), whether written
///   bare or via a binding, round-trips as the keyword string —
///   `animator::parse_easing` maps it to the matching builtin curve.
/// - A **Luau closure** (`{\fn(t) return 1-(1-t)^3 end}`) is sampled
///   *here*, while the per-document Lua frame is live, into a
///   comma-joined LUT of [`EASING_LUT_SAMPLES`] stops. The animator
///   then interpolates the table with zero per-frame Lua calls and
///   never holds a Lua handle past the lowering pass.
///
/// Returns `None` (attr omitted → animator defaults to linear) when
/// the closure can't be sampled (no Lua frame on the SSR/no-`luau`
/// path, or a closure error) — a malformed easing degrades motion to
/// linear, it never fails the render.
fn encode_easing_attr(value: &AttributeValue, scope: &LowerScope) -> Option<String> {
    let body = match value {
        AttributeValue::Expression(e) => e.body.trim().to_string(),
        // A keyword string / binding / template resolves through the
        // normal attribute path (`transition:easing="ease-in"` or
        // `transition:easing={someKeyword}`).
        _ => return resolved_attribute_string(value, scope).filter(|s| !s.is_empty()),
    };

    #[cfg(feature = "luau")]
    {
        if looks_like_closure(&body) {
            let frame = scope.luau_scope()?;
            let mut samples = Vec::with_capacity(EASING_LUT_SAMPLES);
            for i in 0..EASING_LUT_SAMPLES {
                let t = i as f64 / (EASING_LUT_SAMPLES - 1) as f64;
                let out = frame.call_closure(&body, &[serde_json::Value::from(t)])?;
                let v = out.ok()?;
                samples.push(v.as_f64()? as f32);
            }
            let joined = samples
                .iter()
                .map(|f| {
                    // Trim trailing zeros so the attr stays compact
                    // and the round-trip is exact for typical curves.
                    let s = format!("{f:.6}");
                    let s = s.trim_end_matches('0').trim_end_matches('.');
                    if s.is_empty() { "0" } else { s }.to_string()
                })
                .collect::<Vec<_>>()
                .join(",");
            return Some(joined);
        }
    }

    // Not a closure (or no `luau` feature): resolve as a keyword
    // expression (`transition:easing={mode}` where `mode == "ease"`).
    let resolved = resolved_attribute_string(value, scope)?;
    if resolved.is_empty() {
        Some(body).filter(|s| !s.is_empty())
    } else {
        Some(resolved)
    }
}

/// Scan a literal text run for `{expr}` segments and resolve them
/// through the same `lookup_expression` → `evaluate_expression` cascade
/// the attribute-template path uses, so an authored `<text>Hello, {kind == 'error' ? 'oops' : name}</text>`
/// reads through the same vocabulary as `<container style:bg="{…}"/>`.
fn interpolate(text: &str, scope: &LowerScope) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut body = String::new();
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
                body.push(inner);
            }
            let body = body.trim();
            if let Some(v) = lookup_path_owned(body, scope) {
                out.push_str(&stringify_value(&v));
            } else if let Some(v) = evaluate_expression(body, scope) {
                out.push_str(&stringify_value(&v));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Resolve a `{...}`-style expression body against the scope. Phase-2
/// minimum: bare identifiers + dotted paths into the bound JSON value
/// (object fields, array indices). Anything richer returns `None` and
/// the caller falls back to an empty string. The full expression
/// evaluator lives in `prism_core::language::expression` and lands here
/// behind the same seam when component instantiation grows past
/// identifiers.
///
/// Public-but-`#[doc(hidden)]` so the `prism-builder` resolver
/// (`RegistryTagResolver`) can pre-resolve attribute interpolations
/// before constructing the builder `Node`. Sole non-runtime caller.
#[doc(hidden)]
pub fn lookup_expression_in_scope<'a>(
    body: &str,
    scope: &'a LowerScope,
) -> Option<&'a serde_json::Value> {
    lookup_expression(body, scope)
}

/// Owned-value counterpart of [`lookup_expression_in_scope`] that
/// also supports virtual trailing segments (`.length`, `.size`,
/// `.count`, `.first`, `.last`) on arrays / objects / strings. Used
/// by `prism-builder`'s resolver so dispatched-tag attribute
/// resolution (`<shell.foo prop="{items.length}"/>`) reads through
/// the same vocabulary the runtime's own attribute paths use.
#[doc(hidden)]
pub fn lookup_path_owned_in_scope(body: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    lookup_path_owned(body, scope)
}

/// Internal counterpart used by `lower_*` paths.
fn lookup_expression<'a>(body: &str, scope: &'a LowerScope) -> Option<&'a serde_json::Value> {
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    let mut parts = body.split('.');
    let head = parts.next()?.trim();
    let mut cursor = scope.binding(head)?;
    for segment in parts {
        let key = segment.trim();
        if key.is_empty() {
            return None;
        }
        cursor = match cursor {
            serde_json::Value::Object(map) => map.get(key)?,
            serde_json::Value::Array(arr) => {
                let idx: usize = key.parse().ok()?;
                arr.get(idx)?
            }
            _ => return None,
        };
    }
    Some(cursor)
}

/// Owned-value path resolution that augments [`lookup_expression`]
/// with **virtual trailing segments** on arrays / objects:
///
/// | Segment | Array | Object | String |
/// |---|---|---|---|
/// | `.length`, `.size`, `.count` | number of items | number of keys | grapheme count |
/// | `.first` | first item | — | first char (as string) |
/// | `.last`  | last item  | — | last char (as string)  |
///
/// `arr.length` returns the integer length; `arr.first` / `.last`
/// return the JSON value at the leaf position (or `Null` for an empty
/// container). Strings inherit a parity surface so authors aren't
/// surprised by `name.length` on a string binding. Returns `None` when
/// the path doesn't terminate in a virtual segment AND
/// [`lookup_expression`] can't resolve it either.
///
/// Used by every "resolve a path body" seam — `resolved_attribute_string`,
/// `resolve_to_i64`, `eval_truthy`, the text-interpolation path —
/// so the same author-facing dotted-path vocabulary works everywhere.
pub(super) fn lookup_path_owned(body: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    if let Some(v) = lookup_expression(body, scope) {
        return Some(v.clone());
    }
    // **Wave B (`prui-luau-fusion.md` §7.2)** — pipe rewrite. `a | f(x)`
    // and `a |> f(x)` desugar to `f(a, x)` (left-associative, F#/Elm
    // shape) *before* the call resolver runs, so a pipeline like
    // `tasks | filter('status','open') | take(5)` reads as nested
    // builtin calls. Pure string transform — no Lua needed for the
    // pipe itself (the closure args, if any, are resolved later by
    // the closure-aware builtin path).
    if let Some(rewritten) = rewrite_pipes(body) {
        if let Some(v) = lookup_path_owned(&rewritten, scope) {
            return Some(v);
        }
    }
    let body = body.trim();
    // **Functional helpers** — `map`, `reduce`, `filter`, `find`,
    // `slice`, `sort_by`, `unique`, `reverse`, `keys`, `values`,
    // `entries`, `includes`, `index_of`, `join`. These operate on
    // typed JSON values (arrays/objects) which the expression layer's
    // `ExprValue::{Number,String,Boolean}` can't carry, so the owned
    // lookup surface is the natural seam. Authors compose them inside
    // for-sources, attribute interpolations, and text bodies through
    // the same `{call(arr, …)}` shape.
    if let Some(v) = try_call_owned(body, scope) {
        return Some(v);
    }
    // **Wave A** — script-block scope. A `<script>`
    // block's top-level `local`s resolve here after the JSON binding
    // map and functional builtins miss (the §5.6 resolution stack:
    // script locals sit below `let`/`for` vars, above host bindings).
    #[cfg(feature = "luau")]
    if let Some(frame) = scope.luau_scope() {
        if let Some(v) = frame.lookup(body) {
            return Some(v);
        }
    }
    let (head, virtual_seg) = body.rsplit_once('.')?;
    let head = head.trim();
    let virtual_seg = virtual_seg.trim();
    if !matches!(virtual_seg, "length" | "size" | "count" | "first" | "last") {
        return None;
    }
    let parent = lookup_expression(head, scope)?;
    Some(match (parent, virtual_seg) {
        (serde_json::Value::Array(arr), "length" | "size" | "count") => {
            serde_json::Value::from(arr.len() as i64)
        }
        (serde_json::Value::Object(map), "length" | "size" | "count") => {
            serde_json::Value::from(map.len() as i64)
        }
        (serde_json::Value::String(s), "length" | "size" | "count") => {
            serde_json::Value::from(s.chars().count() as i64)
        }
        (serde_json::Value::Array(arr), "first") => {
            arr.first().cloned().unwrap_or(serde_json::Value::Null)
        }
        (serde_json::Value::Array(arr), "last") => {
            arr.last().cloned().unwrap_or(serde_json::Value::Null)
        }
        (serde_json::Value::String(s), "first") => s
            .chars()
            .next()
            .map(|c| serde_json::Value::from(c.to_string()))
            .unwrap_or(serde_json::Value::Null),
        (serde_json::Value::String(s), "last") => s
            .chars()
            .last()
            .map(|c| serde_json::Value::from(c.to_string()))
            .unwrap_or(serde_json::Value::Null),
        _ => return None,
    })
}

/// **Wave B (`prui-luau-fusion.md` §7.2)** — rewrite the pipe
/// operator into nested calls. `a | f(x)` and the `|>` alias both
/// desugar to `f(a, x)`; the operator is left-associative so
/// `t | filter(p) | take(5)` becomes `take(filter(t, p), 5)`.
///
/// Returns `Some(rewritten)` when at least one top-level pipe was
/// found (fully resolved — the result is pipe-free), `None` when the
/// body has no pipe so callers skip the extra resolution attempt.
///
/// Disambiguation rules (no Lua needed — this is a pure string
/// transform):
/// - `||` (logical or) is never a pipe.
/// - A closure literal's own bars (`|t| t.x`) are not pipes: the
///   pipe operator is either `|>` or a `|` with whitespace on *both*
///   sides, and closure bars never present that shape.
/// - Scanning is depth- and quote-aware, so a `|` inside
///   `filter(|t| …)` (depth ≥ 1) or inside a string is skipped.
fn rewrite_pipes(body: &str) -> Option<String> {
    let split = find_last_top_level_pipe(body)?;
    let lhs = body[..split.0].trim();
    let rhs = body[split.1..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    // Left-associative: recurse on the LHS first so `a | f | g`
    // resolves inner-out to `g(f(a))`.
    let lhs_rewritten = rewrite_pipes(lhs).unwrap_or_else(|| lhs.to_string());
    // RHS must be a call or a bare callable name. `f(x)` →
    // `f(lhs, x)`; `f()` / `f` → `f(lhs)`.
    let piped = if let Some(open) = rhs.find('(') {
        let close = matching_close_paren(rhs, open)?;
        // Anything after the call's `)` (a trailing `.field` or
        // another operator) isn't a valid pipe RHS — bail so the
        // caller treats the body as non-pipe.
        if rhs[close + 1..].trim() != "" {
            return None;
        }
        let name = rhs[..open].trim();
        let inner = rhs[open + 1..close].trim();
        if inner.is_empty() {
            format!("{name}({lhs_rewritten})")
        } else {
            format!("{name}({lhs_rewritten}, {inner})")
        }
    } else {
        // Bare name. Reject anything with operator/space chars so we
        // don't swallow a malformed RHS.
        if rhs
            .chars()
            .any(|c| !(c.is_alphanumeric() || c == '_' || c == '.'))
        {
            return None;
        }
        format!("{rhs}({lhs_rewritten})")
    };
    Some(piped)
}

/// Find the byte range `(start, end)` of the last top-level pipe
/// operator in `body`, where `start..end` is the operator span (so
/// `body[..start]` is the LHS and `body[end..]` the RHS). Skips
/// `||`, quoted regions, and any `|` nested in parens/brackets.
fn find_last_top_level_pipe(body: &str) -> Option<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut last: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match (in_str, c) {
            (Some(q), x) if x == q => in_str = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => in_str = Some(c),
            (None, b'(' | b'[' | b'{') => depth += 1,
            (None, b')' | b']' | b'}') => depth -= 1,
            (None, b'|') if depth == 0 => {
                // `|>` operator.
                if bytes.get(i + 1) == Some(&b'>') {
                    last = Some((i, i + 2));
                    i += 2;
                    continue;
                }
                // `||` logical-or — skip both bars.
                if bytes.get(i + 1) == Some(&b'|') {
                    i += 2;
                    continue;
                }
                // `|` pipe only when whitespace-flanked (a closure's
                // own `|t|` bars never are).
                let prev_ws = i > 0 && bytes[i - 1].is_ascii_whitespace();
                let next_ws = bytes.get(i + 1).is_some_and(u8::is_ascii_whitespace);
                if prev_ws && next_ws {
                    last = Some((i, i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    last
}

/// Recognised functional builtin names that operate on typed JSON
/// values (arrays / objects / strings). Used by [`try_call_owned`]
/// to gate the cheap call-form parse — anything else falls through
/// to the full expression evaluator. Listed here as a single seam so
/// every consumer (call resolver, parse helper, future LSP
/// completion) reads from the same table.
const ARRAY_CALL_NAMES: &[&str] = &[
    "map",
    "reduce",
    "filter",
    "find",
    "slice",
    "sort_by",
    "unique",
    "reverse",
    "keys",
    "values",
    "entries",
    "includes",
    "index_of",
    "join",
    "concat_arr",
    "take",
    "drop",
    "pluck",
    "group_by",
    "count_by",
    "any",
    "all",
    "chunk",
    "zip",
    "range",
    // **Wave B** — closure-only builtin (no field-name form). Listed
    // so the call gate admits it; `eval_array_call` has no `reject`
    // arm, so a non-closure `reject(...)` resolves to `None`.
    "reject",
];

/// **Wave B (`prui-luau-fusion.md` §7.2 / B.4)** — builtins that
/// accept a closure literal as a second call form
/// (`filter(arr, |t| t.x)` alongside `filter(arr, "x", v)`). The
/// closure is evaluated per element through the per-document Luau
/// scope.
#[cfg(feature = "luau")]
const CLOSURE_BUILTINS: &[&str] = &[
    "filter", "reject", "map", "find", "any", "all", "sort_by", "group_by", "count_by",
];

/// **Wave B** — cheap shape check: does this raw arg look like a
/// closure literal? Full validation happens in `desugar_closure`;
/// this only decides whether to take the closure-aware builtin path
/// instead of the field-name path.
#[cfg(feature = "luau")]
fn looks_like_closure(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("\\fn") || (s.starts_with('|') && !s.starts_with("||"))
}

/// **Wave B** — split a call-arg list on top-level commas, returning
/// the raw (unevaluated) slices. Mirrors [`parse_call_args`]'s
/// depth/quote scanner but keeps the substrings verbatim so a
/// closure literal (`|t| t.x`) survives to `desugar_closure` instead
/// of being mangled by argument evaluation.
#[cfg(feature = "luau")]
fn split_top_level_args(inside: &str) -> Option<Vec<&str>> {
    let trimmed = inside.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }
    let bytes = inside.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut start = 0usize;
    for (i, &c) in bytes.iter().enumerate() {
        match (in_str, c) {
            (Some(q), x) if x == q => in_str = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => in_str = Some(c),
            (None, b'(' | b'[' | b'{') => depth += 1,
            (None, b')' | b']' | b'}') => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            (None, b',') if depth == 0 => {
                out.push(inside[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if depth != 0 || in_str.is_some() {
        return None;
    }
    out.push(inside[start..].trim());
    Some(out)
}

/// **Wave B** — evaluate a closure-form functional builtin. `raw`
/// are the unevaluated arg slices: `raw[0]` is the collection
/// expression (resolved through the owned-value vocabulary so a
/// pipeline / nested call still works), `raw[1]` is the closure
/// literal, and (for `sort_by`) an optional `raw[2]` order token
/// (`'desc'`). Returns `None` on any shape mismatch so the caller
/// fails the call cleanly rather than emitting wrong data.
#[cfg(feature = "luau")]
fn eval_closure_builtin(
    name: &str,
    raw: &[&str],
    scope: &LowerScope,
    frame: &crate::luau_scope::LuauScopeFrame,
) -> Option<serde_json::Value> {
    use serde_json::Value;
    let arr = match lookup_path_owned(raw.first()?.trim(), scope)? {
        Value::Array(a) => a,
        _ => return None,
    };
    let clo = raw.get(1)?.trim();
    let truthy = |v: &Value| !matches!(v, Value::Null | Value::Bool(false));
    // Evaluate the closure for one element; a closure error aborts
    // the whole builtin (returns `None`) — no partial results.
    let eval = |item: &Value| -> Option<Value> {
        frame.call_closure(clo, std::slice::from_ref(item))?.ok()
    };
    match name {
        "filter" | "reject" => {
            let want = name == "filter";
            let mut out = Vec::new();
            for item in &arr {
                if truthy(&eval(item)?) == want {
                    out.push(item.clone());
                }
            }
            Some(Value::Array(out))
        }
        "map" => {
            let mut out = Vec::with_capacity(arr.len());
            for item in &arr {
                out.push(eval(item)?);
            }
            Some(Value::Array(out))
        }
        "find" => {
            for item in &arr {
                if truthy(&eval(item)?) {
                    return Some(item.clone());
                }
            }
            Some(Value::Null)
        }
        "any" => {
            for item in &arr {
                if truthy(&eval(item)?) {
                    return Some(Value::Bool(true));
                }
            }
            Some(Value::Bool(false))
        }
        "all" => {
            for item in &arr {
                if !truthy(&eval(item)?) {
                    return Some(Value::Bool(false));
                }
            }
            Some(Value::Bool(true))
        }
        "sort_by" => {
            // Decorate-sort-undecorate: the closure runs once per
            // element (not per comparison).
            let mut keyed: Vec<(Value, Value)> = Vec::with_capacity(arr.len());
            for item in &arr {
                keyed.push((eval(item)?, item.clone()));
            }
            keyed.sort_by(|a, b| compare_values(Some(&a.0), Some(&b.0)));
            let desc = raw
                .get(2)
                .map(|s| s.trim().trim_matches(['\'', '"']))
                .is_some_and(|s| s == "desc");
            if desc {
                keyed.reverse();
            }
            Some(Value::Array(keyed.into_iter().map(|(_, v)| v).collect()))
        }
        "group_by" | "count_by" => {
            let counting = name == "count_by";
            let mut map = serde_json::Map::new();
            for item in &arr {
                let key = stringify_value(&eval(item)?);
                if counting {
                    let slot = map.entry(key).or_insert(Value::from(0));
                    let n = slot.as_i64().unwrap_or(0) + 1;
                    *slot = Value::from(n);
                } else if let Some(a) = map
                    .entry(key)
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                {
                    a.push(item.clone());
                }
            }
            Some(Value::Object(map))
        }
        _ => None,
    }
}

/// Try to resolve `body` as a functional-builtin call — optionally
/// followed by a dotted access path: `map(arr, "field")`,
/// `slice(rows, 0, 5)`, `find(rows, 'id', 2).label`, etc. Returns
/// `Some(value)` on a successful evaluation; `None` when the body
/// isn't a recognised call shape, so the caller falls through to
/// virtual segments and then the expression evaluator. Args are
/// parsed as one of: number literal, single- or double-quoted
/// string, `true`/`false`/`null`, or a bare path resolved via
/// [`lookup_path_owned`] (so calls nest).
fn try_call_owned(body: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    let body = body.trim();
    let open = body.find('(')?;
    let name = body[..open].trim();
    // **Wave F (`prui-luau-fusion.md` §7.9)** — colour helpers usable
    // from computed PRSS (`{ lua = "darken(tokens.colors.accent,
    // 0.1)" }`) and any expression slot. Native so the doc example
    // works without a `<script>`-defined helper; args resolve through
    // the same owned-value pipeline (so `tokens.colors.accent` is a
    // valid first arg).
    if matches!(name, "darken" | "lighten" | "alpha" | "mix") {
        let close = matching_close_paren(body, open)?;
        let args = parse_call_args(&body[open + 1..close], scope)?;
        let v = eval_color_call(name, &args)?;
        let tail = body[close + 1..].trim_start();
        return if tail.is_empty() {
            Some(v)
        } else {
            walk_dotted_path(&v, tail.strip_prefix('.')?)
        };
    }
    if !ARRAY_CALL_NAMES.contains(&name) {
        // **Wave A** — a `<script>` helper call
        // (`{priority_color(task.priority)}`). The call resolver
        // already parses args + the trailing dotted chain; we only
        // add a new dispatch arm for "the name is a harvested Luau
        // function". Args resolve through the same `parse_call_args`
        // path so nested builtin / binding args still work.
        #[cfg(feature = "luau")]
        if let Some(frame) = scope.luau_scope() {
            if frame.has_function(name) {
                let close = matching_close_paren(body, open)?;
                let args = parse_call_args(&body[open + 1..close], scope)?;
                let result = match frame.call(name, &args)? {
                    Ok(v) => v,
                    Err(_) => return None,
                };
                let tail = body[close + 1..].trim_start();
                if tail.is_empty() {
                    return Some(result);
                }
                let rest = tail.strip_prefix('.')?;
                return walk_dotted_path(&result, rest);
            }
        }
        return None;
    }
    // Match the closing paren that pairs with `open`, respecting
    // nested parens and quoted strings so `find(rows, 'id', 2)` and
    // `slice(filter(rows, 'k', 'v'), 0, 2)` both find their right
    // boundary cleanly.
    let close = matching_close_paren(body, open)?;
    let inside = &body[open + 1..close];
    // **Wave B** — closure-form builtin
    // (`filter(tasks, |t| t.priority == 'high')`). Checked before
    // `parse_call_args` so the closure literal isn't mangled by
    // argument evaluation. A closure-shaped arg with no Luau scope,
    // or a closure eval error, fails the call (returns `None`)
    // rather than silently falling into the field-name path.
    #[cfg(feature = "luau")]
    if CLOSURE_BUILTINS.contains(&name) {
        if let Some(raw) = split_top_level_args(inside) {
            if raw.iter().any(|a| looks_like_closure(a)) {
                let frame = scope.luau_scope()?;
                let v = eval_closure_builtin(name, &raw, scope, frame)?;
                let tail = body[close + 1..].trim_start();
                if tail.is_empty() {
                    return Some(v);
                }
                return walk_dotted_path(&v, tail.strip_prefix('.')?);
            }
        }
    }
    let args = parse_call_args(inside, scope)?;
    let call_value = eval_array_call(name, &args)?;
    let tail = body[close + 1..].trim_start();
    if tail.is_empty() {
        return Some(call_value);
    }
    // Trailing chain: must begin with `.` for the dotted-access
    // shape. Anything else (`(`, `[`, operator) is unsupported here —
    // the caller falls back to the full expression evaluator.
    let rest = tail.strip_prefix('.')?;
    walk_dotted_path(&call_value, rest)
}

/// Find the matching `)` for an opening paren at `open` in `body`.
/// Returns `None` when the body has unbalanced parens or unterminated
/// quoted strings — same defensive shape `parse_call_args` uses.
fn matching_close_paren(body: &str, open: usize) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut i = open;
    while i < bytes.len() {
        let ch = bytes[i];
        match (in_str, ch) {
            (Some(q), c) if c == q => in_str = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => in_str = Some(ch),
            (None, b'(') => depth += 1,
            (None, b')') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Walk a dotted-path expression (`label`, `user.email`, `tabs.0`)
/// against an owned JSON value. Returns `Some(child)` on a successful
/// walk; `None` on a missing segment / type mismatch. Virtual
/// trailing segments (`.length`, `.first`, `.last`) are recognised
/// terminally so `slice(rows, 0, 5).length` reads as expected.
fn walk_dotted_path(root: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let path = path.trim();
    if path.is_empty() {
        return Some(root.clone());
    }
    let mut cursor: serde_json::Value = root.clone();
    let segments: Vec<&str> = path.split('.').map(str::trim).collect();
    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            return None;
        }
        // Recognise a terminal virtual segment on the last position.
        if i == segments.len() - 1 {
            match (&cursor, *seg) {
                (serde_json::Value::Array(a), "length" | "size" | "count") => {
                    return Some(serde_json::Value::from(a.len() as i64));
                }
                (serde_json::Value::Object(m), "length" | "size" | "count") => {
                    return Some(serde_json::Value::from(m.len() as i64));
                }
                (serde_json::Value::String(s), "length" | "size" | "count") => {
                    return Some(serde_json::Value::from(s.chars().count() as i64));
                }
                (serde_json::Value::Array(a), "first") => {
                    return Some(a.first().cloned().unwrap_or(serde_json::Value::Null));
                }
                (serde_json::Value::Array(a), "last") => {
                    return Some(a.last().cloned().unwrap_or(serde_json::Value::Null));
                }
                _ => {}
            }
        }
        cursor = match cursor {
            serde_json::Value::Object(mut map) => map.remove(*seg)?,
            serde_json::Value::Array(arr) => {
                let idx: usize = seg.parse().ok()?;
                arr.into_iter().nth(idx)?
            }
            _ => return None,
        };
    }
    Some(cursor)
}

/// Split a call argument list on top-level commas (parens-depth
/// aware) and resolve each argument through the owned-value lookup
/// surface. Returns `None` when any unbalanced parens / quotes
/// surface, so a malformed call falls through to the next
/// resolution layer rather than silently producing wrong data.
fn parse_call_args(s: &str, scope: &LowerScope) -> Option<Vec<serde_json::Value>> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    for ch in trimmed.chars() {
        match (in_str, ch) {
            (Some(q), c) if c == q => {
                in_str = None;
                current.push(c);
            }
            (Some(_), c) => current.push(c),
            (None, '\'') | (None, '"') => {
                in_str = Some(ch);
                current.push(ch);
            }
            (None, '(' | '[') => {
                depth += 1;
                current.push(ch);
            }
            (None, ')' | ']') => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
                current.push(ch);
            }
            (None, ',') if depth == 0 => {
                args.push(eval_call_arg(current.trim(), scope)?);
                current.clear();
            }
            (None, c) => current.push(c),
        }
    }
    if depth != 0 || in_str.is_some() {
        return None;
    }
    if !current.trim().is_empty() {
        args.push(eval_call_arg(current.trim(), scope)?);
    }
    Some(args)
}

/// Resolve a single call argument to a typed JSON value. Tries, in
/// order: number literal, string literal (`'…'` / `"…"`),
/// `true`/`false`/`null`, then bare path / nested call via
/// [`lookup_path_owned`]. Returns `None` when nothing matches so
/// the caller fails the whole call cleanly.
fn eval_call_arg(s: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<i64>() {
        return Some(serde_json::Value::from(n));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Some(serde_json::Value::from(f));
    }
    if let (Some('\''), Some('\'')) = (s.chars().next(), s.chars().last()) {
        if s.len() >= 2 {
            return Some(serde_json::Value::from(&s[1..s.len() - 1]));
        }
    }
    if let (Some('"'), Some('"')) = (s.chars().next(), s.chars().last()) {
        if s.len() >= 2 {
            return Some(serde_json::Value::from(&s[1..s.len() - 1]));
        }
    }
    match s {
        "true" => return Some(serde_json::Value::Bool(true)),
        "false" => return Some(serde_json::Value::Bool(false)),
        "null" => return Some(serde_json::Value::Null),
        _ => {}
    }
    lookup_path_owned(s, scope)
}

/// Dispatch a parsed call to its implementation. Pure transformation
/// over `Vec<Value>` — no scope access here; everything resolves at
/// arg-parse time so the implementations stay test-friendly.
/// **Wave F** — parse `#rgb` / `#rrggbb` / `#rrggbbaa` into RGBA
/// (alpha defaults 255). Returns `None` for anything else (a token
/// reference that didn't resolve, a named colour) so the caller
/// fails the call cleanly.
fn parse_hex_rgba(s: &str) -> Option<(u8, u8, u8, u8)> {
    let h = s.trim().strip_prefix('#')?;
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|v| v * 17);
            Some((d(0)?, d(1)?, d(2)?, 255))
        }
        6 => Some((b(0)?, b(2)?, b(4)?, 255)),
        8 => Some((b(0)?, b(2)?, b(4)?, b(6)?)),
        _ => None,
    }
}

fn rgba_hex(r: u8, g: u8, b: u8, a: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

/// **Wave F (§7.9)** — colour math for computed PRSS / expression
/// slots. `amount`/`t` are clamped to `0.0..=1.0`; a non-colour
/// first arg returns `None`.
fn eval_color_call(name: &str, args: &[serde_json::Value]) -> Option<serde_json::Value> {
    let as_f = |v: &serde_json::Value| match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    };
    let as_hex = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => parse_hex_rgba(s),
        _ => None,
    };
    let lerp = |x: u8, y: u8, t: f64| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    match name {
        "darken" | "lighten" => {
            let (r, g, b, a) = as_hex(args.first()?)?;
            let amt = as_f(args.get(1)?)?.clamp(0.0, 1.0);
            let f = |c: u8| {
                if name == "darken" {
                    (c as f64 * (1.0 - amt)).round() as u8
                } else {
                    (c as f64 + (255.0 - c as f64) * amt).round() as u8
                }
            };
            Some(serde_json::Value::String(rgba_hex(f(r), f(g), f(b), a)))
        }
        "alpha" => {
            let (r, g, b, _) = as_hex(args.first()?)?;
            let a = (as_f(args.get(1)?)?.clamp(0.0, 1.0) * 255.0).round() as u8;
            Some(serde_json::Value::String(rgba_hex(r, g, b, a)))
        }
        "mix" => {
            let (r1, g1, b1, a1) = as_hex(args.first()?)?;
            let (r2, g2, b2, a2) = as_hex(args.get(1)?)?;
            let t = as_f(args.get(2)?).unwrap_or(0.5).clamp(0.0, 1.0);
            Some(serde_json::Value::String(rgba_hex(
                lerp(r1, r2, t),
                lerp(g1, g2, t),
                lerp(b1, b2, t),
                lerp(a1, a2, t),
            )))
        }
        _ => None,
    }
}

fn eval_array_call(name: &str, args: &[serde_json::Value]) -> Option<serde_json::Value> {
    use serde_json::Value;
    let arr_arg = |i: usize| match args.get(i) {
        Some(Value::Array(a)) => Some(a),
        _ => None,
    };
    let str_arg = |i: usize| match args.get(i) {
        Some(Value::String(s)) => Some(s.as_str()),
        _ => None,
    };
    let i64_arg = |i: usize| match args.get(i) {
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    };
    match name {
        "map" => {
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            Some(Value::Array(
                arr.iter()
                    .map(|item| match item {
                        Value::Object(map) => map.get(field).cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    })
                    .collect(),
            ))
        }
        "filter" => {
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let needle = args.get(2)?;
            let out: Vec<Value> = arr
                .iter()
                .filter(|item| match item {
                    Value::Object(map) => map
                        .get(field)
                        .map(|v| values_loose_eq(v, needle))
                        .unwrap_or(false),
                    _ => false,
                })
                .cloned()
                .collect();
            Some(Value::Array(out))
        }
        "find" => {
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let needle = args.get(2)?;
            for item in arr {
                if let Value::Object(map) = item {
                    if let Some(v) = map.get(field) {
                        if values_loose_eq(v, needle) {
                            return Some(item.clone());
                        }
                    }
                }
            }
            Some(Value::Null)
        }
        "reduce" => {
            // `reduce(arr, "op" [, "field"])` — op is one of:
            //   sum, product, min, max, count, avg.
            // Optional third arg names a field on object items; absent
            // means treat each item as a number directly.
            let arr = arr_arg(0)?;
            let op = str_arg(1)?;
            let field = str_arg(2);
            let extract = |item: &Value| -> Option<f64> {
                let target = match field {
                    Some(f) => match item {
                        Value::Object(map) => map.get(f)?,
                        _ => return None,
                    },
                    None => item,
                };
                match target {
                    Value::Number(n) => n.as_f64(),
                    Value::String(s) => s.parse::<f64>().ok(),
                    Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                    _ => None,
                }
            };
            let nums: Vec<f64> = arr.iter().filter_map(extract).collect();
            let n = match op {
                "sum" => nums.iter().sum::<f64>(),
                "product" => nums.iter().product::<f64>(),
                "min" => nums.iter().copied().fold(f64::INFINITY, f64::min),
                "max" => nums.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                "count" => nums.len() as f64,
                "avg" => {
                    if nums.is_empty() {
                        0.0
                    } else {
                        nums.iter().sum::<f64>() / nums.len() as f64
                    }
                }
                _ => return None,
            };
            if n.is_finite() && n.fract() == 0.0 && n.abs() <= i64::MAX as f64 {
                Some(Value::from(n as i64))
            } else {
                serde_json::Number::from_f64(n).map(Value::Number)
            }
        }
        "slice" => {
            let arr = arr_arg(0)?;
            let len = arr.len() as i64;
            let normalize = |n: i64| -> usize {
                if n < 0 {
                    ((len + n).max(0)) as usize
                } else {
                    (n.min(len)) as usize
                }
            };
            let start = normalize(i64_arg(1).unwrap_or(0));
            let end = normalize(i64_arg(2).unwrap_or(len));
            if start >= end {
                return Some(Value::Array(Vec::new()));
            }
            Some(Value::Array(arr[start..end].to_vec()))
        }
        "sort_by" => {
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let order = str_arg(2).unwrap_or("asc");
            let mut out: Vec<Value> = arr.clone();
            out.sort_by(|a, b| {
                let av = a.get(field);
                let bv = b.get(field);
                compare_values(av, bv)
            });
            if order == "desc" {
                out.reverse();
            }
            Some(Value::Array(out))
        }
        "unique" => {
            let arr = arr_arg(0)?;
            let mut seen: Vec<Value> = Vec::with_capacity(arr.len());
            for item in arr {
                if !seen.iter().any(|s| values_loose_eq(s, item)) {
                    seen.push(item.clone());
                }
            }
            Some(Value::Array(seen))
        }
        "reverse" => {
            // Single-arg array reverse — distinct from `reverse_arr`
            // and the `for=` `reverse` modifier; this returns a new
            // typed array suitable for downstream consumers (`{ first
            // = reverse(items).first }`).
            let arr = arr_arg(0)?;
            let mut out = arr.clone();
            out.reverse();
            Some(Value::Array(out))
        }
        "keys" => match args.first()? {
            Value::Object(map) => Some(Value::Array(
                map.keys().map(|k| Value::String(k.clone())).collect(),
            )),
            _ => None,
        },
        "values" => match args.first()? {
            Value::Object(map) => Some(Value::Array(map.values().cloned().collect())),
            _ => None,
        },
        "entries" => match args.first()? {
            Value::Object(map) => Some(Value::Array(
                map.iter()
                    .map(|(k, v)| {
                        let mut entry = serde_json::Map::new();
                        entry.insert("key".to_string(), Value::String(k.clone()));
                        entry.insert("value".to_string(), v.clone());
                        Value::Object(entry)
                    })
                    .collect(),
            )),
            _ => None,
        },
        "includes" => {
            let arr = arr_arg(0)?;
            let needle = args.get(1)?;
            Some(Value::Bool(arr.iter().any(|v| values_loose_eq(v, needle))))
        }
        "index_of" => {
            let arr = arr_arg(0)?;
            let needle = args.get(1)?;
            Some(Value::from(
                arr.iter()
                    .position(|v| values_loose_eq(v, needle))
                    .map(|i| i as i64)
                    .unwrap_or(-1),
            ))
        }
        "join" => {
            let arr = arr_arg(0)?;
            let sep = str_arg(1).unwrap_or(",");
            let s = arr
                .iter()
                .map(stringify_value)
                .collect::<Vec<_>>()
                .join(sep);
            Some(Value::String(s))
        }
        "concat_arr" => {
            let mut out: Vec<Value> = Vec::new();
            for a in args {
                if let Value::Array(items) = a {
                    out.extend(items.iter().cloned());
                }
            }
            Some(Value::Array(out))
        }
        "take" => {
            // `take(arr, n)` — first N items. `n <= 0` returns empty;
            // `n >= len` returns the whole array.
            let arr = arr_arg(0)?;
            let n = i64_arg(1).unwrap_or(0).max(0) as usize;
            Some(Value::Array(arr.iter().take(n).cloned().collect()))
        }
        "drop" => {
            // `drop(arr, n)` — every item AFTER the first N. Pair to
            // `take` for pagination patterns.
            let arr = arr_arg(0)?;
            let n = i64_arg(1).unwrap_or(0).max(0) as usize;
            Some(Value::Array(arr.iter().skip(n).cloned().collect()))
        }
        "pluck" => {
            // Alias for `map(arr, "field")` — same semantics, name
            // matches the lodash / underscore vocabulary so authors
            // coming from those libraries reach for the obvious word.
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            Some(Value::Array(
                arr.iter()
                    .map(|item| match item {
                        Value::Object(map) => map.get(field).cloned().unwrap_or(Value::Null),
                        _ => Value::Null,
                    })
                    .collect(),
            ))
        }
        "group_by" => {
            // `group_by(arr, "field")` — IndexMap-shaped object whose
            // keys are the distinct field values (in first-seen order)
            // and values are arrays of the items that share that key.
            // Round-trips through `for="entry in entries(group_by(…))"`
            // so authors can render section-per-group views.
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let mut groups: serde_json::Map<String, Value> = serde_json::Map::new();
            for item in arr {
                let key = match item.get(field) {
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => stringify_value(other),
                    None => String::new(),
                };
                groups
                    .entry(key)
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                    .unwrap()
                    .push(item.clone());
            }
            Some(Value::Object(groups))
        }
        "count_by" => {
            // `count_by(arr, "field")` — like `group_by` but values
            // are counts rather than item lists. Powers
            // `{"draft": 4, "active": 12}` summaries.
            let arr = arr_arg(0)?;
            let field = str_arg(1)?;
            let mut counts: serde_json::Map<String, Value> = serde_json::Map::new();
            for item in arr {
                let key = match item.get(field) {
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => stringify_value(other),
                    None => String::new(),
                };
                let entry = counts.entry(key).or_insert_with(|| Value::from(0i64));
                let next = entry.as_i64().unwrap_or(0) + 1;
                *entry = Value::from(next);
            }
            Some(Value::Object(counts))
        }
        "any" => {
            // `any(arr, "field", value)` — true iff at least one item
            // matches the field test. Single-arg form `any(arr)`
            // returns "does the array contain a truthy value" — useful
            // when an upstream stage already filtered.
            let arr = arr_arg(0)?;
            let field = str_arg(1);
            let needle = args.get(2);
            Some(Value::Bool(arr.iter().any(|item| match (field, needle) {
                (Some(f), Some(n)) => item.get(f).map(|v| values_loose_eq(v, n)).unwrap_or(false),
                _ => is_truthy_value(item),
            })))
        }
        "all" => {
            // Sibling to `any` — every item must match. Empty array →
            // true (vacuous truth, matches Rust `all`'s shape).
            let arr = arr_arg(0)?;
            let field = str_arg(1);
            let needle = args.get(2);
            Some(Value::Bool(arr.iter().all(|item| match (field, needle) {
                (Some(f), Some(n)) => item.get(f).map(|v| values_loose_eq(v, n)).unwrap_or(false),
                _ => is_truthy_value(item),
            })))
        }
        "chunk" => {
            // `chunk(arr, n)` — split into fixed-size sub-arrays. The
            // final chunk is short when `arr.len() % n != 0`. `n <= 0`
            // returns the whole array as a single chunk to mimic the
            // lodash shape and keep callers from accidentally producing
            // infinite iteration on a misconfigured size.
            let arr = arr_arg(0)?;
            let n = i64_arg(1).unwrap_or(1).max(1) as usize;
            let out: Vec<Value> = arr
                .chunks(n)
                .map(|slice| Value::Array(slice.to_vec()))
                .collect();
            Some(Value::Array(out))
        }
        "zip" => {
            // `zip(a, b, …)` — produce an array of N-tuples (as
            // arrays), one per index, up to the shortest input. Used
            // for "render rows from two parallel lists" patterns
            // (e.g. headers + values).
            let arrays: Vec<&Vec<Value>> = args
                .iter()
                .filter_map(|v| {
                    if let Value::Array(a) = v {
                        Some(a)
                    } else {
                        None
                    }
                })
                .collect();
            if arrays.is_empty() {
                return Some(Value::Array(Vec::new()));
            }
            let len = arrays.iter().map(|a| a.len()).min().unwrap_or(0);
            let zipped: Vec<Value> = (0..len)
                .map(|i| Value::Array(arrays.iter().map(|a| a[i].clone()).collect()))
                .collect();
            Some(Value::Array(zipped))
        }
        "range" => {
            // `range(n)` / `range(start, end)` / `range(start, end,
            // step)` — produces an integer array. The same `0..n`
            // numbers a `for=` clause natively supports, but as a
            // standalone array value so the result can be passed
            // around as data (`pluck`, `concat_arr`, etc.).
            let (start, end, step) = match args.len() {
                1 => (0, i64_arg(0)?, 1),
                2 => (i64_arg(0)?, i64_arg(1)?, 1),
                3 => (i64_arg(0)?, i64_arg(1)?, i64_arg(2)?.max(1)),
                _ => return None,
            };
            if start >= end {
                return Some(Value::Array(Vec::new()));
            }
            let nums: Vec<Value> = (start..end)
                .step_by(step as usize)
                .map(Value::from)
                .collect();
            Some(Value::Array(nums))
        }
        _ => None,
    }
}

/// Truthiness predicate over a raw JSON value — matches the
/// `eval_truthy` rule (null / false / 0 / "" / [] / {} → false;
/// anything else → true). Shared by `any` / `all` so a single-arg
/// call (`any(rows)`) reads through the same vocabulary `if=`
/// uses.
fn is_truthy_value(v: &serde_json::Value) -> bool {
    use serde_json::Value;
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Loose equality for typed JSON values — matches the expression
/// evaluator's `loose_eq` shape so `filter(rows, "id", "x")` reads
/// strings, `filter(rows, "count", 3)` reads numbers, and bool↔int
/// coercion stays consistent.
fn values_loose_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::String(x), Value::String(y)) => x == y,
        (Value::String(s), Value::Number(n)) | (Value::Number(n), Value::String(s)) => {
            s.parse::<f64>().ok() == n.as_f64()
        }
        (Value::Bool(b), Value::Number(n)) | (Value::Number(n), Value::Bool(b)) => {
            (if *b { 1.0 } else { 0.0 }) == n.as_f64().unwrap_or(0.0)
        }
        _ => stringify_value(a) == stringify_value(b),
    }
}

/// Order JSON values for `sort_by`. Numbers / strings sort naturally;
/// missing fields (`None`) sort last; mixed kinds fall back to
/// stringified comparison so iteration stays total.
fn compare_values(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
) -> std::cmp::Ordering {
    use serde_json::Value;
    use std::cmp::Ordering;
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, _) => Ordering::Greater,
        (_, None) => Ordering::Less,
        (Some(Value::Number(x)), Some(Value::Number(y))) => x
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&y.as_f64().unwrap_or(0.0))
            .unwrap_or(Ordering::Equal),
        (Some(Value::String(x)), Some(Value::String(y))) => x.cmp(y),
        (Some(Value::Bool(x)), Some(Value::Bool(y))) => x.cmp(y),
        (Some(x), Some(y)) => stringify_value(x).cmp(&stringify_value(y)),
    }
}

/// Truthy evaluator for `if=` / `else-if=`. Routes through the full
/// expression evaluator in [`evaluate_expression`] so authors get
/// ternary, boolean `||`/`&&`/`!`, comparisons, arithmetic, and dotted
/// paths uniformly. The bare-path fast path (`{row.selected}`) still
/// resolves through [`lookup_expression`] so a directly-bound JSON
/// `Value` keeps its native truthy rule (empty arrays / empty objects
/// are falsy — the expression coercion in [`ExprValue::to_boolean`]
/// would otherwise stringify them).
pub(super) fn eval_truthy(body: &str, scope: &LowerScope) -> bool {
    let body = body
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    if body.is_empty() {
        return false;
    }
    // Direct binding lookup — handles the `if="{row.selected}"` shape
    // where the bound value is a typed JSON value (array / object /
    // bool). Virtual `.length`/`.first`/`.last` segments resolve here
    // too, so `if="{items.length}"` is true iff non-empty without any
    // operator. Falls through for any operator-bearing expression.
    if let Some(v) = lookup_path_owned(body, scope) {
        return match v {
            serde_json::Value::Null => false,
            serde_json::Value::Bool(b) => b,
            serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
            serde_json::Value::String(s) => !s.is_empty(),
            serde_json::Value::Array(a) => !a.is_empty(),
            serde_json::Value::Object(o) => !o.is_empty(),
        };
    }
    // Operator-bearing expressions (`a == 'b'`, `enabled && !disabled`,
    // ternary heads, etc.) flow through the full Prism expression
    // evaluator. `None` means parse failure or unresolved operand —
    // treated as falsy, matching the JS rule.
    match evaluate_expression(body, scope) {
        Some(serde_json::Value::Null) | None => false,
        Some(serde_json::Value::Bool(b)) => b,
        Some(serde_json::Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        Some(serde_json::Value::Array(a)) => !a.is_empty(),
        Some(serde_json::Value::Object(o)) => !o.is_empty(),
    }
}

/// Evaluate a `{...}` expression body against the active scope and
/// return its computed JSON value. Wave 11.2 substrate: gives every
/// authored attribute access to ternary (`a ? b : c`), boolean
/// (`&& || !`), comparison (`== != < <= > >=`), arithmetic (`+ - * / %`),
/// dotted paths (`item.label`, `tabs.0.name`), and the Prism expression
/// builtins (`upper`, `len`, `min`, `concat`, …) — all in one pass
/// through the existing `prism_core::language::expression` parser +
/// evaluator. No hand-rolled regex or string-indexed parsing.
///
/// Returns `None` when the body is empty, the parser surfaces errors,
/// or the result coerces to a JSON null. Callers fall back to their
/// type-specific default (empty string for templates, false for
/// `if=`, etc.).
#[doc(hidden)]
pub fn evaluate_expression(body: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    use prism_core::language::expression::{evaluate, parse as parse_expr, ExprValue, ValueStore};

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = parse_expr(trimmed);
    if !parsed.errors.is_empty() {
        return None;
    }
    let node = parsed.node?;

    struct ScopeStore<'a> {
        scope: &'a LowerScope,
    }
    impl<'a> ValueStore for ScopeStore<'a> {
        fn resolve(&self, operand_type: &str, id: &str, subfield: Option<&str>) -> ExprValue {
            if operand_type != "field" {
                return ExprValue::String(String::new());
            }
            // Compose the dotted path the way `lookup_path_owned` reads
            // it, so virtual `.length` / `.first` / `.last` segments
            // resolve the same way they do in bare-path lookups —
            // `items.length > 0` is the symmetric operator-bearing form
            // of `if="{items.length}"`.
            let full_path = match subfield {
                Some(rest) => format!("{}.{}", id, rest),
                None => id.to_string(),
            };
            match lookup_path_owned(&full_path, self.scope) {
                Some(v) => json_to_expr_value(&v),
                // Missing path → Null. Authors write `value == null`
                // to detect missing-key absence (Wave 11.3 of
                // composable-builder-plan.md). Falsy in boolean
                // ladders, empty in string ladders, zero in numeric
                // ladders — same coercion table the JS `null` has.
                None => ExprValue::Null,
            }
        }
    }
    let store = ScopeStore { scope };
    Some(expr_value_to_json(evaluate(&node, &store)))
}

fn json_to_expr_value(v: &serde_json::Value) -> prism_core::language::expression::ExprValue {
    use prism_core::language::expression::ExprValue;
    match v {
        serde_json::Value::Bool(b) => ExprValue::Boolean(*b),
        serde_json::Value::Number(n) => ExprValue::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => ExprValue::String(s.clone()),
        serde_json::Value::Null => ExprValue::Null,
        // Arrays and objects don't participate in arithmetic / comparison
        // — fall through as their JSON-stringified form so authors who
        // accidentally compare an object stringify-compare instead of
        // crashing the render.
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            ExprValue::String(v.to_string())
        }
    }
}

fn expr_value_to_json(v: prism_core::language::expression::ExprValue) -> serde_json::Value {
    use prism_core::language::expression::ExprValue;
    match v {
        ExprValue::Boolean(b) => serde_json::Value::Bool(b),
        // Prefer the integer encoding when the value is exact — keeps
        // `Number(5)` stringifying as `"5"` rather than `"5.0"` so a
        // count derived through the evaluator surface (e.g.
        // `items.length + 1`) reads identically to one derived through
        // the direct `lookup_path_owned` surface.
        ExprValue::Number(n) => {
            if n.is_finite() && n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                serde_json::Value::Number(serde_json::Number::from(n as i64))
            } else {
                serde_json::Number::from_f64(n)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
        }
        ExprValue::String(s) => serde_json::Value::String(s),
        ExprValue::Null => serde_json::Value::Null,
    }
}

fn stringify_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Public-but-`#[doc(hidden)]` mirror of [`stringify_value`] for the
/// `prism-builder` resolver's templated-attribute path. Same shape:
/// strings pass through verbatim, nulls become empty, everything
/// else uses `Display`.
#[doc(hidden)]
pub fn stringify_value_for_template(value: &serde_json::Value) -> String {
    stringify_value(value)
}

/// Default pixel size used by `rem` and `em` unit suffixes when no
/// scope-driven base is available. Matches the browser default for
/// CSS root font size. Scope-aware call sites read
/// `tokens.typography.font-size-md` via [`LowerScope::rem_px`] and
/// pre-expand length values through [`expand_length_units`] before
/// dispatching to the parsers below.
const REM_PX: f32 = 16.0;

/// Pre-expand any `Nrem` / `Nem` segments in `value` to their
/// pixel-resolved numeric form, using the scope's
/// [`LowerScope::rem_px`] base. Multi-segment strings (`padding="1rem 2rem"`)
/// expand each whitespace-separated token independently so the
/// downstream `parse_padding_shorthand` consumer reads pixel numbers
/// uniformly.
///
/// When the scope's rem base equals the canonical 16px default, the
/// helper short-circuits and returns the value unchanged so the
/// hot path (no token override) avoids the alloc.
fn expand_length_units(value: &str, scope: &LowerScope) -> String {
    let rem_px = scope.rem_px();
    if (rem_px - REM_PX).abs() < f32::EPSILON {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut leading_ws_done = false;
    for token in value.split_whitespace() {
        if leading_ws_done {
            out.push(' ');
        } else {
            leading_ws_done = true;
        }
        // Order matters — `rem` ends in `em`. Try `rem` first.
        let expanded = if let Some(num) = token.strip_suffix("rem") {
            num.trim_end()
                .parse::<f32>()
                .ok()
                .map(|n| (n * rem_px).to_string())
        } else if let Some(num) = token.strip_suffix("em") {
            num.trim_end()
                .parse::<f32>()
                .ok()
                .map(|n| (n * rem_px).to_string())
        } else {
            None
        };
        match expanded {
            Some(s) => out.push_str(&s),
            None => out.push_str(token),
        }
    }
    out
}

/// Parse a length-valued string. Accepts:
///
/// | Form | Resolves to | Notes |
/// |---|---|---|
/// | `"14"` | `14.0` | Bare number = px (matches CSS) |
/// | `"14px"` | `14.0` | Explicit px |
/// | `"1rem"` | `16.0` | `n * REM_PX` |
/// | `"0.875rem"` | `14.0` | Decimal allowed |
/// | `"1em"` | `16.0` | Same as `rem` today — no parent-font-size scope threading yet |
///
/// Trailing whitespace tolerated. Anything else returns `None` —
/// caller drops the property silently (matching the existing
/// "unknown style value drops cleanly" discipline).
fn parse_f32(s: &str) -> Option<f32> {
    let s = s.trim();
    // Order matters: `rem` ends in `em`, so check `rem` first.
    if let Some(num) = s.strip_suffix("rem") {
        return num.trim_end().parse::<f32>().ok().map(|n| n * REM_PX);
    }
    if let Some(num) = s.strip_suffix("em") {
        return num.trim_end().parse::<f32>().ok().map(|n| n * REM_PX);
    }
    if let Some(num) = s.strip_suffix("px") {
        return num.trim_end().parse::<f32>().ok();
    }
    s.parse::<f32>().ok()
}

/// Apply one `style:<key>[:<state>]="<value>"` override onto a
/// [`ContainerProps`]. Single source of truth for the style-attribute
/// vocabulary: every consumer (`apply_container_attributes`'s
/// `AttributeNamespace::Style` branch, the resolver-side
/// parent-passes-style-to-child seam in `ui_resolver.rs`, future Luau
/// style writers) calls this so the keys stay in sync.
///
/// The `local` argument is the attribute's local part (`background`,
/// `radius:hovered`, etc.). [`split_state_suffix`] is consulted
/// internally to peel any trailing `:state` so callers don't need
/// to.
///
/// Unknown keys land as `data-style-<key>` / `data-style-<key>-<state>`
/// semantic attrs so author intent survives even when the runtime
/// doesn't have first-class support for the override yet — same
/// "data round-trips, behaviour follows" pattern Waves 9.2/9.4 use.
/// **PRSS / class toggles** — collect the active class-name list for
/// `el` against `scope`, in document order:
///
/// 1. Every name from a static `class="..."` attribute (multiple
///    classes whitespace-separated).
/// 2. Every `class:<name>="{cond}"` toggle whose value is truthy.
///
/// Toggles layer onto the static list so a later
/// `class:foo="{true}"` wins on conflicts the same way a literal
/// `class="… foo"` would. Boolean `class:foo` (no `=value`) reads as
/// `true` so authors can write the bare attribute as a synonym for
/// `class:foo="true"`. Result preserves source-order duplicates so
/// `apply_prss_class`'s left-to-right specificity rule observes
/// the same shape it would have without toggles.
fn active_class_names(el: &Element, scope: &LowerScope) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for attr in &el.attributes {
        match attr.name.namespace {
            AttributeNamespace::Identifier if attr.name.local == "class" => {
                if let Some(value) = resolved_attribute_string(&attr.value, scope) {
                    for name in value.split_whitespace() {
                        if !name.is_empty() {
                            out.push(name.to_string());
                        }
                    }
                }
            }
            AttributeNamespace::Class => {
                if attr.name.local.is_empty() {
                    continue;
                }
                if attr_value_is_truthy(&attr.value, scope) {
                    out.push(attr.name.local.clone());
                }
            }
            _ => {}
        }
    }
    out
}

/// Truthy evaluator for an `AttributeValue`. Mirrors the JS rule the
/// `if=` namespace already uses — bare `class:foo` (Empty) reads as
/// true, an Expression body flows through [`eval_truthy`], a
/// String/Template resolves and is parsed against the canonical
/// boolean spellings (`true` / `1` / non-empty arbitrary text → true;
/// `false` / `0` / empty → false). Lets authors write either
/// `class:active="{state.active}"` or `class:active` without
/// special-casing Empty downstream.
fn attr_value_is_truthy(value: &AttributeValue, scope: &LowerScope) -> bool {
    match value {
        // Boolean attribute (no `=` after the name) — the author's
        // intent is "always on", same shape as HTML's `disabled`.
        AttributeValue::Empty => true,
        // Expression bodies route through the full Prism truthy rule
        // so ternary / `&&` / dotted-path lookups read uniformly.
        AttributeValue::Expression(expr) => eval_truthy(&expr.body, scope),
        // String + Template values resolve to a string and then map
        // through the canonical boolean spellings. Authors writing
        // `class:foo="false"` get false; `class:foo="true"` true;
        // anything else is truthy if non-empty.
        AttributeValue::String { value, .. } => string_value_is_truthy(value),
        AttributeValue::Template { .. } => resolved_attribute_string(value, scope)
            .as_deref()
            .map(string_value_is_truthy)
            .unwrap_or(false),
    }
}

fn string_value_is_truthy(s: &str) -> bool {
    let trimmed = s.trim();
    !matches!(trimmed, "" | "false" | "0")
}

/// **PRSS** — resolve and apply one class name (with its `extends`
/// chain flattened parent-first) onto a [`ContainerProps`]. Property
/// values are interpolated against the active scope so a class can
/// reference `{tokens.colors.<name>}` and read through the same
/// expression evaluator inline `style:` uses.
///
/// Application order inside this function:
/// 1. Base properties from the flattened `extends` chain.
/// 2. State overrides — each `(state, key, value)` applied through
///    the existing `apply_style_override` state-suffix branch as
///    `<key>:<state>`.
///
/// Unknown class names are no-ops (a parent `extends` chain that
/// already surfaced a `missing-parent` diagnostic at parse time
/// drops cleanly at apply time too).
/// **Wave F (`prui-luau-fusion.md` §7.9)** — resolve a PRSS value
/// that may be a `{ lua = "…" }` computed expression. A
/// sentinel-prefixed value (encoded by the prism-core PRSS parser)
/// has its trailing expression evaluated through the same
/// owned-value pipeline class bindings use — so `tokens.*` lookups
/// and Luau-frame helpers (`darken(…)`) both resolve. A plain value
/// passes through untouched. Borrowed `Cow` on the common
/// (non-computed) path keeps the hot loop allocation-free.
fn prss_value_resolved<'a>(value: &'a str, scope: &LowerScope) -> std::borrow::Cow<'a, str> {
    match value.strip_prefix(prism_core::language::prss::LUA_VALUE_SENTINEL) {
        Some(expr) => {
            let resolved = lookup_path_owned(expr, scope)
                .or_else(|| evaluate_expression(expr, scope))
                .map(|v| stringify_value(&v))
                .unwrap_or_default();
            std::borrow::Cow::Owned(resolved)
        }
        None => std::borrow::Cow::Borrowed(value),
    }
}

fn apply_prss_class(
    sheet: &prism_core::language::prss::StyleSheet,
    name: &str,
    props: &mut ContainerProps,
    scope: &LowerScope,
) {
    let Some(resolved) = sheet.resolve(name) else {
        return;
    };
    for (key, value) in &resolved.properties {
        let computed = prss_value_resolved(value, scope);
        let interpolated = interpolate(&computed, scope);
        let resolved_short = resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
        let final_value = expand_length_units(&resolved_short, scope);
        apply_style_override(props, key, &final_value);
    }
    for (state, key, value) in &resolved.states {
        let computed = prss_value_resolved(value, scope);
        let interpolated = interpolate(&computed, scope);
        let resolved_short = resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
        let final_value = expand_length_units(&resolved_short, scope);
        let suffixed = format!("{}:{}", key, state);
        apply_style_override(props, &suffixed, &final_value);
    }
}

/// **PRSS descendant selectors** — walk every multi-segment class
/// in the sheet and apply the ones whose segment chain matches the
/// element's active class set + the ancestor class chain on
/// `scope`. Ordering: each matching selector's properties layer on
/// top of the flat-class application, so a more specific selector
/// (`.btn .icon`) overrides the flat (`.icon`) for keys it sets.
/// Selectors are visited in declaration order; later wins on key
/// conflicts (matching the §4.6 application-order rule).
fn apply_descendant_selectors(
    sheet: &prism_core::language::prss::StyleSheet,
    active: &[String],
    scope: &LowerScope,
    props: &mut ContainerProps,
) {
    let chain = scope.class_chain();
    for (_, segments, resolved) in sheet.descendant_selectors() {
        if !descendant_selector_matches(&segments, active, chain) {
            continue;
        }
        for (key, value) in &resolved.properties {
            let computed = prss_value_resolved(value, scope);
            let interpolated = interpolate(&computed, scope);
            let resolved_short =
                resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
            let final_value = expand_length_units(&resolved_short, scope);
            apply_style_override(props, key, &final_value);
        }
        for (state, key, value) in &resolved.states {
            let computed = prss_value_resolved(value, scope);
            let interpolated = interpolate(&computed, scope);
            let resolved_short =
                resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
            let final_value = expand_length_units(&resolved_short, scope);
            let suffixed = format!("{}:{}", key, state);
            apply_style_override(props, &suffixed, &final_value);
        }
    }
}

/// **Short-name token references** (`prss-reference.md` §6) — when a
/// PRSS class property or PRUI inline style value is a bare token
/// name (e.g. `radius = "md"` or `style:background="accent"`),
/// resolve it through the active `tokens.<bucket>.<name>` table on
/// `scope` to its underlying value (`8`, `"#7c3aed"`). Returns
/// `None` when the value isn't a bare token name (literal hex,
/// numeric with units, expression result, …) or when the looked-up
/// token isn't present — caller falls back to the raw value.
///
/// The bucket is derived from `key` (after a state-suffix split):
/// colors-typed keys (`background`, `color`, `border`) read from
/// `tokens.colors`; spacing-typed keys (`gap`, `padding`,
/// `padding-*`, `margin*`) read from `tokens.spacing`; `radius`
/// reads from `tokens.radius`; `font-size` / `line-height` read
/// from `tokens.typography` with the canonical `font-size-<short>`
/// / `line-height-<short>` key shape mirrored from
/// [`design_tokens_to_json`].
fn resolve_short_token(key: &str, value: &str, scope: &LowerScope) -> Option<String> {
    let trimmed = value.trim();
    if !is_bare_token_name(trimmed) {
        return None;
    }
    let (bucket, token_key) = short_token_bucket_for_key(key, trimmed)?;
    let tokens = scope.binding("tokens")?.as_object()?;
    let bucket_obj = tokens.get(bucket)?.as_object()?;
    let val = bucket_obj.get(&token_key)?;
    Some(stringify_value(val))
}

/// True when `s` matches the shape PRSS short-name token references
/// recognise: lowercase ASCII identifier characters (a-z), digits,
/// `-`, or `_`, with a non-digit first character. Filters out hex
/// colors (`#…`), numeric values (`8`, `1.5`, `1rem`), expression
/// remnants (`{…}`), and capitalised words. The shape mirrors the
/// design-token key spelling — `accent`, `text-primary`, `surface-elevated`,
/// `font-size-md`, `md`.
fn is_bare_token_name(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Map a PRSS / PRUI style key (after a state-suffix split) onto its
/// `(bucket, token_key)` lookup pair. `None` for keys that don't
/// participate in short-name resolution (`width`, `height`,
/// `direction`, `tag`, …).
fn short_token_bucket_for_key(key: &str, short: &str) -> Option<(&'static str, String)> {
    use prism_core::language::prism_ui::ast::split_state_suffix;
    let (bare_key, _) = split_state_suffix(key);
    match bare_key {
        "background" | "color" | "border" => Some(("colors", short.to_string())),
        "radius" => Some(("radius", short.to_string())),
        "gap" | "padding" | "padding-left" | "padding-right" | "padding-top" | "padding-bottom"
        | "margin" | "margin-left" | "margin-right" | "margin-top" | "margin-bottom" => {
            Some(("spacing", short.to_string()))
        }
        "font-size" => Some(("typography", format!("font-size-{}", short))),
        "line-height" => Some(("typography", format!("line-height-{}", short))),
        _ => None,
    }
}

/// CSS-style descendant matcher: the rightmost segment must match
/// a class in `current` (the element's active class set); each
/// preceding segment must match an ancestor's class set, in order
/// from innermost outward, with intermediate ancestors skipped if
/// they don't match.
///
/// `chain` is outermost-first as stored in [`LowerScope::class_chain`];
/// the match walks it from innermost (`chain.len()-1`) outward so the
/// nearest ancestor with the needed class is consumed first.
fn descendant_selector_matches(
    segments: &[&str],
    current: &[String],
    chain: &[Vec<String>],
) -> bool {
    if segments.is_empty() {
        return false;
    }
    let last = segments[segments.len() - 1];
    if !current.iter().any(|c| c == last) {
        return false;
    }
    let prefix = &segments[..segments.len() - 1];
    if prefix.is_empty() {
        // Single segment — caller handles flat application; we do
        // not apply here to avoid double-counting. Returning false
        // matches `descendant_selectors`'s `len() <= 1` filter; this
        // arm is defensive.
        return false;
    }
    let mut chain_idx = chain.len();
    // Walk prefix segments innermost-first. For each needle, scan
    // ancestors (innermost→outermost), consuming whichever one
    // contains it. Anything before that ancestor is still available
    // for outer prefix segments.
    for needle in prefix.iter().rev() {
        let mut found = false;
        while chain_idx > 0 {
            chain_idx -= 1;
            if chain[chain_idx].iter().any(|c| c == needle) {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

pub fn apply_style_override(props: &mut ContainerProps, local: &str, value: &str) {
    let (key, state) = split_state_suffix(local);
    match (key, state) {
        ("background", None) => {
            if let Some(c) = parse_color(value) {
                props.background = Some(c);
            }
        }
        ("radius", None) => {
            if let Some(v) = parse_f32(value) {
                props.radius = CornerRadius {
                    tl: v,
                    tr: v,
                    br: v,
                    bl: v,
                };
            }
        }
        ("padding", None) => {
            // CSS-shorthand: `padding="8"` (uniform), `padding="8 16"`
            // (vertical, horizontal), `padding="8 16 24"` (top, H,
            // bottom), `padding="8 16 24 32"` (TRBL — CSS top/right/
            // bottom/left order). Single-value path stays through
            // `Padding::all` for back-compat.
            if let Some(p) = parse_padding_shorthand(value) {
                props.padding = p;
            }
        }
        ("padding-left", None) => set_padding_side(&mut props.padding, Some(value), Side::Left),
        ("padding-right", None) => set_padding_side(&mut props.padding, Some(value), Side::Right),
        ("padding-top", None) => set_padding_side(&mut props.padding, Some(value), Side::Top),
        ("padding-bottom", None) => set_padding_side(&mut props.padding, Some(value), Side::Bottom),
        ("gap", None) => {
            if let Some(v) = parse_f32(value) {
                props.gap = v;
            }
        }
        ("width", None) => {
            if let Some(s) = parse_sizing(value) {
                props.width = s;
            }
        }
        ("height", None) => {
            if let Some(s) = parse_sizing(value) {
                props.height = s;
            }
        }
        // Wave 14.6 — `style:opacity="0.5"` lowers to the
        // `ContainerProps::opacity` field. Clamped to `[0.0, 1.0]`;
        // malformed values silently drop (same pattern as the rest
        // of this vocabulary). `None` (the default) means "fully
        // opaque" — set to a float to fade the container's own
        // paint and cascade into children at command-emit time.
        ("opacity", None) => {
            if let Some(v) = parse_f32(value) {
                props.opacity = Some(v.clamp(0.0, 1.0));
            }
        }
        ("background", Some("hovered")) => {
            if let Some(c) = parse_color(value) {
                props
                    .hover
                    .get_or_insert_with(HoverOverrides::default)
                    .background = Some(c);
            }
        }
        ("radius", Some("hovered")) => {
            if let Some(v) = parse_f32(value) {
                props
                    .hover
                    .get_or_insert_with(HoverOverrides::default)
                    .radius = Some(CornerRadius {
                    tl: v,
                    tr: v,
                    br: v,
                    bl: v,
                });
            }
        }
        (key, Some(state)) => {
            // Known state suffix (`:selected` / `:focused`), unknown
            // key — round-trip as a semantic attr so author intent
            // survives (Wave 9.2 pattern).
            props
                .semantic
                .attrs
                .push((format!("data-style-{}-{}", key, state), value.to_string()));
        }
        (_, None) => {
            // Unknown bare key (no recognized state suffix). Drop
            // silently — same shape as the pre-extraction
            // `apply_container_attributes` branch. Authors who want
            // arbitrary `data-*` payloads have the `data:` namespace.
        }
    }
}

fn parse_direction(s: &str) -> Direction {
    match s.trim() {
        "row" => Direction::Row,
        _ => Direction::Column,
    }
}

/// Parse a sizing value for `width` / `height`. In addition to
/// the length forms `parse_f32` accepts, this layer recognises:
///
/// | Form | Resolves to |
/// |---|---|
/// | `"grow"` | `Sizing::Grow` (`taffy::Dimension::Percent(1.0)`) |
/// | `"fit"` / `"auto"` | `Sizing::Fit` (`taffy::Dimension::Auto`) |
/// | `"50%"` | `Sizing::Percent(0.5)` (CSS-style; 0..1 clamped) |
/// | `"14"` / `"14px"` / `"1rem"` | `Sizing::Fixed(<px>)` via `parse_f32` |
fn parse_sizing(s: &str) -> Option<Sizing> {
    let s = s.trim();
    match s {
        "grow" => Some(Sizing::Grow),
        "fit" | "auto" => Some(Sizing::Fit),
        _ => {
            if let Some(num) = s.strip_suffix('%') {
                return num
                    .trim_end()
                    .parse::<f32>()
                    .ok()
                    .map(|n| Sizing::Percent(n / 100.0));
            }
            parse_f32(s).map(Sizing::Fixed)
        }
    }
}

fn parse_color(raw: &str) -> Option<Color> {
    let s = raw.trim();
    let hex = s.strip_prefix('#')?;
    let (r, g, b, a) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            255,
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            u8::from_str_radix(&hex[6..8], 16).ok()?,
        ),
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            (r * 17, g * 17, b * 17, 255)
        }
        _ => return None,
    };
    Some(Color { r, g, b, a })
}

/// Parse a CSS-shorthand padding value into a [`Padding`]. Accepts
/// 1, 2, 3, or 4 whitespace-separated lengths matching the standard
/// CSS shorthand order. Returns `None` if any token fails to parse;
/// caller drops the property cleanly.
///
/// | Tokens | Shape |
/// |---|---|
/// | 1 | All sides uniform |
/// | 2 | Vertical, horizontal |
/// | 3 | Top, horizontal, bottom |
/// | 4 | Top, right, bottom, left (CSS TRBL) |
fn parse_padding_shorthand(value: &str) -> Option<Padding> {
    let tokens: Vec<f32> = value
        .split_whitespace()
        .map(parse_f32)
        .collect::<Option<Vec<f32>>>()?;
    match tokens.len() {
        1 => Some(Padding::all(tokens[0])),
        2 => Some(Padding {
            top: tokens[0],
            right: tokens[1],
            bottom: tokens[0],
            left: tokens[1],
        }),
        3 => Some(Padding {
            top: tokens[0],
            right: tokens[1],
            bottom: tokens[2],
            left: tokens[1],
        }),
        4 => Some(Padding {
            top: tokens[0],
            right: tokens[1],
            bottom: tokens[2],
            left: tokens[3],
        }),
        _ => None,
    }
}

enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

fn set_padding_side(padding: &mut Padding, raw: Option<&str>, side: Side) {
    let Some(v) = raw.and_then(parse_f32) else {
        return;
    };
    match side {
        Side::Left => padding.left = v,
        Side::Right => padding.right = v,
        Side::Top => padding.top = v,
        Side::Bottom => padding.bottom = v,
    }
}

#[cfg(test)]
mod tests;
