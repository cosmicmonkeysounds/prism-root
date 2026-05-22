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
    parse, Document as AstDocument, Element, Node as AstNode, ParseError,
};

use crate::layout::Node;

pub(crate) mod color;
mod control_flow;

mod style;
pub use style::apply_style_override;

mod expression;
#[cfg(test)]
use expression::rewrite_pipes;
pub use expression::{
    evaluate_expression, lookup_expression_in_scope, lookup_path_owned_in_scope,
    stringify_value_for_template,
};

mod document;
use document::{collect_imports, collect_inline_stylesheets, collect_teleports};
#[cfg(feature = "luau")]
use document::{
    collect_script_bodies, document_uses_dialect, document_uses_luau_expr, resolve_require_graph,
};

mod elements;
pub use elements::lower_ast_children;
use elements::lower_children;

mod components;
pub(crate) use components::splice_mixin_into_children;
pub use components::{
    harvest_components, harvest_declarations, instantiate_component, is_pascal_case_tag,
    ComponentDef, HarvestedDeclarations, MixinDef, ParamDef, TraitDef,
};

mod trait_registry;
pub use trait_registry::{TraitRegistry, TraitTarget};

mod macros;
pub use macros::{expand_macros, harvest_macros, MacroDef, MAX_EXPANSION_DEPTH};

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
    /// Wave 11.2 — default-slot injection point for a DSL-authored
    /// shell component composing its caller's pre-lowered children.
    /// The loader's [`crate::interpret::lower_document_with_scope`]
    /// caller stuffs the calling `LowerCtx::host_children()` here at
    /// invocation time; the runtime's unnamed `<slot/>` emits the
    /// UiNodes verbatim (Phase 5 collapsed `<host-children/>` into
    /// this single slot surface). Distinct from
    /// `host_children_by_tag` (resolver-side, tag-keyed pre-injection)
    /// and from [`SlotBindings`] (AST-level `<slot/>` expansion via
    /// template binding). `None` outside the loader's seam.
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
    /// **Phase 7 — `component` declarations.** Map of name →
    /// resolved [`ComponentDef`] for every top-level
    /// `<component name="X">` harvested from the document and from
    /// every `<import component="./foo.prui"/>` projection (Phase
    /// 8). Populated once per document by
    /// [`lower_document_with_scope`]; carried through scope clones
    /// for child-scope dispatch.
    local_components: Arc<HashMap<String, Arc<ComponentDef>>>,
    /// **Phase 8 — `trait` declarations.** Companion table to
    /// [`Self::local_components`] for `<trait name="X">` decls
    /// (typed shapes / contracts per §7.3). Carried through scope
    /// clones identically — empty for documents that declare none.
    local_traits: Arc<HashMap<String, Arc<TraitDef>>>,
    /// **Phase 10 — `mixin` declarations.** Sibling table to
    /// [`Self::local_components`] / [`Self::local_traits`]. The
    /// splicer (`components::splice_mixins`) reads it to inline
    /// mixin bodies on `use` / `derive=` / `with=` sites. Empty for
    /// documents that declare no mixins (the common case so far).
    pub(crate) local_mixins: Arc<HashMap<String, Arc<MixinDef>>>,
    /// **Phase 11 — `macro` declarations.** Round-tripped through
    /// scope for tooling consumers (LSP, lint). The macro expansion
    /// itself runs in a *pre-pass* over the AST before
    /// [`lower_document_with_scope`] starts the main walk, so the
    /// lowering paths below never see a macro invocation directly.
    /// Empty for documents that declare none (the common case).
    pub(crate) local_macros: Arc<HashMap<String, Arc<MacroDef>>>,
    /// **Phase 9 — trait registry.** Open registry of trait names
    /// to attribute method sets, consulted at parse / lowering time
    /// by `trait.method=value` attribute dispatch (§7.4). Seeded
    /// with the four built-in traits (`layout` / `style` / `pointer`
    /// / `a11y`); user code extends it via Luau / Rust registration.
    /// Distinct from [`Self::local_traits`] — `local_traits` holds
    /// document-author-declared `<trait>` shapes; this is the
    /// runtime's attribute-dispatch registry.
    trait_registry: Arc<TraitRegistry>,
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

    /// Wave 11.2 — install the pre-lowered children that the runtime's
    /// unnamed `<slot/>` should emit. The shell's `.prui` loader sets
    /// this before invoking [`lower_document_with_scope`] so a
    /// DSL-authored wrapper component (toast-stack, launchpad)
    /// consumes its caller's children via one declarative element
    /// instead of a Rust `ctx.host_children()` call. (Phase 5
    /// collapsed `<host-children/>` into the unnamed slot.)
    pub fn with_host_children_ui(mut self, children: Vec<Node>) -> Self {
        self.host_children_ui = Some(Arc::new(children));
        self
    }

    /// The pre-lowered children currently bound to the unnamed
    /// `<slot/>` (Phase-5 default-slot seam). `None` outside the
    /// loader's seam.
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

    /// **Phase 7** — install the local component table harvested from
    /// the document. PascalCase tags inside `lower_element_body`
    /// resolve through this table before falling through to the
    /// host's `TagResolver`. Carrying as `Arc<HashMap<…>>` keeps
    /// scope clones cheap during control-flow / slot expansion.
    pub fn with_local_components(
        mut self,
        components: Arc<HashMap<String, Arc<ComponentDef>>>,
    ) -> Self {
        self.local_components = components;
        self
    }

    /// **Phase 7** — look up a locally-declared component by name.
    /// Returns `None` when the document declared no component of
    /// that name (the common case — most lowering paths skip the
    /// PascalCase dispatch entirely).
    pub fn local_component(&self, name: &str) -> Option<&Arc<ComponentDef>> {
        self.local_components.get(name)
    }

    /// **Phase 7** — is there at least one local component
    /// registered? Cheap pre-flight check so the per-element
    /// PascalCase dispatch can skip the map probe on documents that
    /// declared none.
    pub fn has_local_components(&self) -> bool {
        !self.local_components.is_empty()
    }

    /// **Phase 8** — install the local trait table harvested from
    /// the document and from `<import component="…"/>` projections.
    /// Empty on documents that declare none (the common case).
    pub fn with_local_traits(mut self, traits: Arc<HashMap<String, Arc<TraitDef>>>) -> Self {
        self.local_traits = traits;
        self
    }

    /// **Phase 8** — look up a locally-declared trait by name. The
    /// Phase 9 attribute-classifier will read this to resolve
    /// `Focusable.focused` against an author-declared `<trait>` shape;
    /// today it round-trips for documentation + LSP consumers.
    pub fn local_trait(&self, name: &str) -> Option<&Arc<TraitDef>> {
        self.local_traits.get(name)
    }

    /// **Phase 10** — install the local mixin table harvested from
    /// the document and from `<import component="…"/>` projections.
    /// Empty on documents that declare none.
    pub fn with_local_mixins(mut self, mixins: Arc<HashMap<String, Arc<MixinDef>>>) -> Self {
        self.local_mixins = mixins;
        self
    }

    /// **Phase 10** — look up a locally-declared mixin by name.
    /// Returns `None` for the common case (no mixins on the scope).
    pub fn local_mixin(&self, name: &str) -> Option<&Arc<MixinDef>> {
        self.local_mixins.get(name)
    }

    /// **Phase 10** — is there at least one local mixin registered?
    /// Cheap pre-flight check so the per-element `with=` dispatch
    /// can skip the map probe on documents that declared none.
    pub fn has_local_mixins(&self) -> bool {
        !self.local_mixins.is_empty()
    }

    /// **Phase 11** — install the local macro table harvested from
    /// the document and from `<import component="…"/>` projections
    /// of files that declared macros. Empty on documents that
    /// declare none.
    pub fn with_local_macros(mut self, macros: Arc<HashMap<String, Arc<MacroDef>>>) -> Self {
        self.local_macros = macros;
        self
    }

    /// **Phase 11** — look up a locally-declared macro by name.
    /// Tooling consumers (LSP, lint) reach for this — the runtime's
    /// own expansion pass owns the registry locally during the
    /// pre-pass, then deposits it here for downstream consumers.
    pub fn local_macro(&self, name: &str) -> Option<&Arc<MacroDef>> {
        self.local_macros.get(name)
    }

    /// **Phase 11** — is there at least one local macro registered?
    pub fn has_local_macros(&self) -> bool {
        !self.local_macros.is_empty()
    }

    /// **Phase 9** — install the trait registry (the four built-in
    /// `layout` / `style` / `pointer` / `a11y` traits plus any
    /// user-registered traits). The default is
    /// [`TraitRegistry::builtin`], so the scope carries it
    /// transparently and call sites that don't extend the registry
    /// pay nothing.
    pub fn with_trait_registry(mut self, registry: TraitRegistry) -> Self {
        self.trait_registry = Arc::new(registry);
        self
    }

    /// **Phase 9** — borrow the active trait registry. Used by the
    /// attribute classifier (Phase 9+ parser work) to resolve dotted
    /// `trait.method=` syntax against the open registry.
    pub fn trait_registry(&self) -> &TraitRegistry {
        &self.trait_registry
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
    // **Phase 11 — macro expansion pre-pass.** Macros (`<macro
    // Name>…</macro>` per §7.8) are pattern-and-expand rewrites that
    // run *before* every other lowering pass. We harvest them off
    // the raw document, run [`macros::expand_macros`] over the
    // entire AST, then thread the rewritten nodes through the rest
    // of the pipeline as if the author had written the expanded
    // form directly. Documents without macros pay nothing — the
    // empty-registry early-out in `expand_macros` makes the pass a
    // no-op clone.
    let macro_registry = macros::harvest_macros(&document.nodes, None);
    let expanded_doc_owned;
    let document: &AstDocument = if macro_registry.is_empty() {
        document
    } else {
        let expanded_nodes = macros::expand_macros(&document.nodes, &macro_registry, 0);
        expanded_doc_owned = AstDocument {
            nodes: expanded_nodes,
        };
        &expanded_doc_owned
    };

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

    // **Phase 7 + 8** — harvest top-level `<component name="X">` and
    // `<trait name="X">` declarations from the active document, then
    // resolve every `<import component="./path.prui"/>` projection
    // (Phase 8 §7.11) by re-parsing the imported source through the
    // host's `ImportResolver` and merging its declarations into the
    // local tables. The file-declared `<namespace name="Ns"/>` of
    // each imported file is honoured automatically; an `as=` alias
    // on the import overrides it (Q11). Documents that declare no
    // components AND have no `component` imports skip both clones.
    let local_decls_owned;
    let scope: &LowerScope = {
        let mut declarations = harvest_declarations(&document.nodes, None);
        if let Some(res) = scope.import_resolver() {
            for imp in collect_imports(&document.nodes) {
                if imp.kind != "component" {
                    continue;
                }
                if let Some(src) = res.resolve_import("component", &imp.path) {
                    let (imported_doc, _errs) = parse(&src);
                    let imported = harvest_declarations(&imported_doc.nodes, imp.alias.as_deref());
                    declarations.merge(imported);
                }
            }
        }
        if declarations.components.is_empty()
            && declarations.traits.is_empty()
            && declarations.mixins.is_empty()
            && macro_registry.is_empty()
        {
            scope
        } else {
            local_decls_owned = scope
                .clone()
                .with_local_components(Arc::new(declarations.components))
                .with_local_traits(Arc::new(declarations.traits))
                .with_local_mixins(Arc::new(declarations.mixins))
                .with_local_macros(Arc::new(macro_registry.clone()));
            &local_decls_owned
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

#[cfg(test)]
mod tests;
