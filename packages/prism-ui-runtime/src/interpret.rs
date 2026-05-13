//! Interpret path — parse a `.prism-ui` source string and lower the
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

// ---------------------------------------------------------------------------
// Tag resolver — DI hook for unknown tags
// ---------------------------------------------------------------------------

/// Resolve a `.prism-ui` element whose tag the runtime doesn't own
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
}

/// **Wave 14.3** — per-element memo cache keyed by `id`. Hosts that
/// want stable v-memo semantics across renders construct one and
/// thread it into [`LowerScope::with_memo_cache`]; the same handle
/// can live for the lifetime of the surface (frames, panel swaps,
/// hot reloads — anything short of a document replacement).
#[derive(Debug, Default)]
pub struct MemoCache {
    entries: HashMap<String, (Vec<serde_json::Value>, Vec<Node>)>,
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
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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

    pub fn binding(&self, name: &str) -> Option<&serde_json::Value> {
        self.bindings.get(name)
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
    /// element should emit. The shell's `.prism-ui` loader sets this
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

    /// **Wave 14.1** — seed the design-token table as a `tokens`
    /// binding. Every migrated `.prism-ui` file authors visual
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
/// `.prism-ui` source). Re-uses the same control-flow + slot +
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
fn lower_children(nodes: &[AstNode], scope: &LowerScope) -> Vec<Node> {
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
    }
    lower_element_body(el, scope)
}

/// **Wave 14.3** — extract the memo dep tuple and the resolved id
/// of an element in one pass. Returns `None` when *either* the
/// element has no `memo=` attribute or its id resolves to an empty
/// string — both are required for the cache to key cleanly.
fn extract_memo_and_id(
    el: &Element,
    scope: &LowerScope,
) -> Option<(Vec<serde_json::Value>, String)> {
    let mut memo: Option<Vec<serde_json::Value>> = None;
    let mut id: Option<String> = None;
    for attr in &el.attributes {
        match attr.name.namespace {
            AttributeNamespace::Bare if attr.name.local == "memo" => {
                let body = attribute_string(&attr.value).unwrap_or_default();
                memo = Some(eval_memo_deps(&body, scope));
            }
            AttributeNamespace::Identifier if attr.name.local == "id" => {
                id = resolved_attribute_string(&attr.value, scope).filter(|s| !s.is_empty());
            }
            _ => {}
        }
    }
    Some((memo?, id?))
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
        // The shell's `.prism-ui` loader installs the children via
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
            if let Some(resolver) = scope.resolver() {
                if let Some(nodes) = resolver.resolve(el, scope) {
                    return nodes;
                }
            }
            lower_children(&el.children, scope)
        }
    }
}

// ---------------------------------------------------------------------------
// Control-flow expansion
// ---------------------------------------------------------------------------

/// Pre-pass that walks a sibling list and produces a flat
/// `Vec<(node, optional-child-scope)>` of nodes that survive the
/// control-flow gates. `else-if`/`else` chains are evaluated against
/// the most recent `if` predicate; `for` clones the element per item
/// with the iteration variable bound in a fork of the parent scope.
///
/// Returning a per-element optional scope (rather than rewriting the
/// AST) lets the iteration variable bind without cloning the entire
/// tree: the lowering pass picks up the `for`-bound scope only for
/// the element it belongs to, then descends back into the parent
/// scope for unrelated siblings.
fn expand_control_flow(
    nodes: &[AstNode],
    scope: &LowerScope,
) -> Vec<(AstNode, Option<LowerScope>)> {
    let mut out: Vec<(AstNode, Option<LowerScope>)> = Vec::with_capacity(nodes.len());
    // Tracks whether the current `if`/`else-if`/`else` chain has
    // already taken a branch. A non-element sibling resets the chain
    // — same rule HTMX / Svelte use.
    let mut chain_taken: Option<bool> = None;
    // **Wave 14.2** — accumulating scope for sibling-level `<let
    // name="X" value="{…}"/>` bindings. Starts as `None` (subsequent
    // siblings see the parent scope unchanged); each `<let/>` clones
    // the running scope, evaluates its expression, and seeds the
    // binding so every subsequent sibling inherits it. Matches
    // Svelte's `{@const}` lexical scope: declaration onward, within
    // the same sibling list.
    let mut let_scope: Option<LowerScope> = None;

    for node in nodes {
        let AstNode::Element(el) = node else {
            // Whitespace-only text between tags is the parser's way of
            // round-tripping source layout — it must not break a
            // sibling `if`/`else-if`/`else` chain. Real text content
            // (or interpolations / comments-with-content) does break
            // the chain, matching the HTMX/Svelte rule that an
            // intervening render node ends conditional grouping.
            let breaks_chain = match node {
                AstNode::Text { value, .. } => !value.trim().is_empty(),
                _ => true,
            };
            if breaks_chain {
                chain_taken = None;
            }
            out.push((node.clone(), let_scope.clone()));
            continue;
        };
        // **Wave 14.2** — `<let name="X" value="{expr}"/>` evaluated
        // and bound into the running let_scope; doesn't render. Lives
        // before `control_flow_attr` so a `<let if="…"/>` doesn't
        // accidentally fire when the predicate is false (let-bindings
        // are unconditional by design — guard with a sibling `<if/>`
        // wrapper if conditional state is wanted). Typed resolution
        // through [`evaluate_bare_attr_typed`] preserves number /
        // bool / object shape, so a downstream `padding="{total}"`
        // reads through `parse_f32` cleanly.
        if el.tag == "let" {
            let active = let_scope.as_ref().unwrap_or(scope);
            let name = bare_attr_value(el, "name", active);
            let value = evaluate_bare_attr_typed(el, "value", active);
            if let (Some(name), Some(value)) = (name, value) {
                let next = let_scope
                    .clone()
                    .unwrap_or_else(|| scope.clone())
                    .with_binding(name, value);
                let_scope = Some(next);
            }
            continue;
        }
        // **Wave 14.2** — every code path that reads bindings now
        // consults the running `let_scope` first (sibling-level
        // shadowing of the parent scope). The original `scope`
        // parameter is the fallback when no `<let/>` preceded this
        // sibling.
        let active = let_scope.as_ref().unwrap_or(scope);
        let cf = control_flow_attr(el);
        match cf {
            None => {
                chain_taken = None;
                out.push((node.clone(), let_scope.clone()));
            }
            Some(ControlFlow::If(cond)) => {
                let take = eval_truthy(&cond, active);
                chain_taken = Some(take);
                if take {
                    out.push((node.clone(), let_scope.clone()));
                }
            }
            Some(ControlFlow::ElseIf(cond)) => {
                let take = matches!(chain_taken, Some(false)) && eval_truthy(&cond, active);
                if let Some(prev) = chain_taken.as_mut() {
                    *prev = *prev || take;
                }
                if take {
                    out.push((node.clone(), let_scope.clone()));
                }
            }
            Some(ControlFlow::Else) => {
                let take = matches!(chain_taken, Some(false));
                chain_taken = None;
                if take {
                    out.push((node.clone(), let_scope.clone()));
                }
            }
            Some(ControlFlow::For {
                var,
                index_var,
                source,
                step,
                reverse,
            }) => {
                // **Wave 15.2** — numeric range `0..n` / `0..=n` short-circuits the
                // binding-lookup path before falling back to **Wave 15.3** object
                // iteration on a JSON object (each LHS-pair element binds
                // `(value, key)`) and the original array iteration.
                // Post-Wave-15 modifiers: `step N` (range only) skips by `N`
                // between emitted endpoints; `reverse` flips the final order
                // of every iteration shape.
                let iter = resolve_for_iteration(&source, step, reverse, active);
                // **Wave 15.1** — empty iteration sets `chain_taken =
                // Some(false)` so a subsequent `else` (Svelte's
                // `{:each}{:else}` shape) runs as the empty-state fallback.
                // Non-empty iteration sets `Some(true)` to suppress the
                // else branch, matching the if-chain rule. The previous
                // unconditional reset-to-`None` becomes the "no for at
                // all" baseline up at `None` of `match cf`.
                chain_taken = Some(!iter.is_empty());
                for (key, item) in iter {
                    let mut child_scope = active.clone().with_binding(var.clone(), item);
                    // Optional iteration-index / object-key binding —
                    // `for="row, idx in rows"` exposes the index as a
                    // typed integer for arrays / ranges; `for="value, key
                    // in obj"` exposes the string key for objects. Same
                    // LHS slot, type depends on the source shape (Wave
                    // 15.3).
                    if let Some(idx_name) = index_var.as_ref() {
                        child_scope = child_scope.with_binding(idx_name.clone(), key);
                    }
                    out.push((node.clone(), Some(child_scope)));
                }
            }
        }
    }
    out
}

/// **Wave 15.2 / 15.3** — resolve the right-hand side of a `for=`
/// attribute into an ordered list of `(second-LHS, primary-LHS)`
/// pairs. The primary is the item bound to `var`; the second is the
/// value bound to `index_var` when present.
///
/// Three source shapes, in priority order:
/// 1. **Numeric range** `start..end` / `start..=end` (Wave 15.2).
///    Both endpoints resolve through the full expression evaluator
///    so `for="i in 0..items.length"` works as well as `for="i in
///    0..10"`. Reverse ranges (start > end) yield an empty
///    iteration — Rust `Range`'s shape.
/// 2. **JSON object** (Wave 15.3). When the source resolves to a
///    `Value::Object`, iterate `(key, value)` entries in insertion
///    order. The second-LHS binding receives the string key.
/// 3. **JSON array** (the pre-Wave-15 path). Iterate values; the
///    second-LHS binding receives the integer index.
fn resolve_for_iteration(
    source: &str,
    step: Option<i64>,
    reverse: bool,
    scope: &LowerScope,
) -> Vec<(serde_json::Value, serde_json::Value)> {
    let trimmed = source.trim();
    // Wave 15.2 — range form. `..=` parsed before `..` so the
    // inclusive form isn't swallowed by the exclusive split.
    if let Some((start_raw, end_raw, inclusive)) = split_range(trimmed) {
        let start = resolve_to_i64(start_raw, scope);
        let end = resolve_to_i64(end_raw, scope);
        if let (Some(start), Some(end)) = (start, end) {
            let last = if inclusive { end } else { end - 1 };
            if start > last {
                return Vec::new();
            }
            let stride = step.unwrap_or(1).max(1) as usize;
            let mut values: Vec<i64> = (start..=last).step_by(stride).collect();
            if reverse {
                values.reverse();
            }
            return values
                .into_iter()
                .map(|n| (serde_json::Value::from(n), serde_json::Value::from(n)))
                .collect();
        }
        return Vec::new();
    }
    // Resolve the source as an arbitrary expression so a dotted-path
    // (`item.children`) and functional-helper calls (`map(rows,
    // 'label')`, `filter(rows, 'status', 'active')`, `slice(rows, 0,
    // 5)`) resolve through the same owned-value vocabulary as
    // attribute / text interpolations. Bare identifiers and virtual
    // segments still hit the cheap path inside `lookup_path_owned`.
    //
    // Note: `step` is range-only; arrays and objects always emit every
    // entry. `reverse` flips the final order for either shape.
    let resolved =
        lookup_path_owned(trimmed, scope).or_else(|| evaluate_expression(trimmed, scope));
    let mut entries: Vec<(serde_json::Value, serde_json::Value)> = match resolved {
        Some(serde_json::Value::Object(map)) => map
            .into_iter()
            .map(|(k, v)| (serde_json::Value::String(k), v))
            .collect(),
        Some(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .enumerate()
            .map(|(i, v)| (serde_json::Value::from(i as i64), v))
            .collect(),
        _ => Vec::new(),
    };
    if reverse {
        entries.reverse();
    }
    entries
}

/// Split `start..end` / `start..=end` into `(start, end, inclusive)`.
/// Returns `None` when no `..` appears, so the caller falls through to
/// the value-binding lookup. Inclusive (`..=`) wins over exclusive
/// (`..`) so an author writing `0..=10` doesn't accidentally end up
/// with `0..` + `=10`.
fn split_range(body: &str) -> Option<(&str, &str, bool)> {
    if let Some((start, end)) = body.split_once("..=") {
        return Some((start.trim(), end.trim(), true));
    }
    if let Some((start, end)) = body.split_once("..") {
        return Some((start.trim(), end.trim(), false));
    }
    None
}

/// Resolve a range endpoint to an integer. Tries the cheap parse
/// first (so `0..10` doesn't pay the expression-parser cost), then
/// falls through to the full evaluator (so `0..items.length` works
/// when an array's length is exposed via a bound `length` field on
/// scope) and finally the bare-binding lookup.
fn resolve_to_i64(s: &str, scope: &LowerScope) -> Option<i64> {
    if let Ok(n) = s.parse::<i64>() {
        return Some(n);
    }
    let value = lookup_path_owned(s, scope).or_else(|| evaluate_expression(s, scope))?;
    match value {
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        serde_json::Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

#[derive(Debug, Clone)]
enum ControlFlow {
    If(String),
    ElseIf(String),
    Else,
    For {
        var: String,
        index_var: Option<String>,
        source: String,
        /// Iteration step. Only meaningful for numeric ranges
        /// (`for="i in 0..100 step 10"`). `None` defaults to step 1.
        /// Non-range sources ignore this — array / object iteration
        /// always emits every entry. Parser guarantees `>= 1` when
        /// `Some`.
        step: Option<i64>,
        /// Reverse iteration order. Applies after collecting all
        /// entries (range, array, or object). `for="item in items reverse"`,
        /// `for="i in 0..10 reverse"`, `for="i in 0..100 step 10 reverse"`
        /// all valid.
        reverse: bool,
    },
}

fn control_flow_attr(el: &Element) -> Option<ControlFlow> {
    for attr in &el.attributes {
        if !matches!(attr.name.namespace, AttributeNamespace::ControlFlow) {
            continue;
        }
        let body = attribute_string(&attr.value).unwrap_or_default();
        return Some(match attr.name.local.as_str() {
            "if" => ControlFlow::If(body),
            "else-if" => ControlFlow::ElseIf(body),
            "else" => ControlFlow::Else,
            "for" => parse_for_clause(&body)
                .map(|c| ControlFlow::For {
                    var: c.var,
                    index_var: c.index_var,
                    source: c.source,
                    step: c.step,
                    reverse: c.reverse,
                })
                .unwrap_or_else(|| ControlFlow::If("false".into())),
            _ => return None,
        });
    }
    None
}

/// `"post in posts"` → `("post", None, "posts")`.
/// `"post, idx in posts"` → `("post", Some("idx"), "posts")` — the
/// optional iteration-index variable lets authors build per-row stable
/// ids (`<x id="row-{idx}"/>`), critical for hit-testing dispatched
/// containers via `<dispatch for="row, idx in rows" id="props-{idx}"/>`.
/// Whitespace tolerant; any other shape returns `None` and the caller
/// treats the element as dropped (`if false`).
/// Parsed `for=` clause. Each field captures one part of the
/// `var [, index_var] in source [step N] [reverse]` shape.
struct ForClause {
    var: String,
    index_var: Option<String>,
    source: String,
    /// Iteration step. Range-only; `None` defaults to 1. Parser
    /// guarantees `>= 1` when `Some`.
    step: Option<i64>,
    /// Reverse iteration order after collection.
    reverse: bool,
}

/// `"post in posts"` → `ForClause { var: "post", index_var: None,
/// source: "posts", step: None, reverse: false }`.
/// `"post, idx in posts"` adds `index_var: Some("idx")`.
/// `"i in 0..100 step 10"` adds `step: Some(10)`.
/// `"item in items reverse"` adds `reverse: true`.
/// `"i in 0..100 step 10 reverse"` adds both.
///
/// The source identifier or range is the first whitespace-delimited
/// token after `in`; any trailing `step N` / `reverse` modifiers apply
/// in either order. Step `<= 0`, duplicate modifiers, and trailing
/// junk all return `None`, which the caller treats as `if false` (the
/// element is dropped).
fn parse_for_clause(body: &str) -> Option<ForClause> {
    let body = body
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    // Split on `in` first so the iteration-variable side can carry an
    // optional comma-separated index name without ambiguity with the
    // `in` keyword.
    let mut halves = body.splitn(2, " in ");
    let lhs = halves.next()?.trim();
    let rhs = halves.next()?.trim();
    if rhs.is_empty() {
        return None;
    }
    // LHS shapes: `var` or `var, idx`. Reject anything else.
    let mut lhs_parts = lhs.split(',').map(|s| s.trim());
    let var = lhs_parts.next()?.to_string();
    if var.is_empty() {
        return None;
    }
    let index_var = lhs_parts
        .next()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    if lhs_parts.next().is_some() {
        return None;
    }
    // RHS shape: `<source> [step N] [reverse]` in either order. The
    // source can be a bare identifier, a dotted-path, a numeric range
    // (`0..n` / `0..=n`), OR a functional-helper call (`map(rows,
    // 'label')`, `slice(filter(rows, …), 0, 5)`) that may itself
    // contain whitespace inside its argument list. Modifiers are
    // recognised at the tail; the source extends from the start of
    // the RHS to the boundary before the first `step N` / `reverse`
    // token at paren-depth 0.
    let (source, after) = split_for_source(rhs)?;
    let source = source.trim().to_string();
    if source.is_empty() {
        return None;
    }
    let mut tokens = after.split_whitespace();
    let mut step: Option<i64> = None;
    let mut reverse = false;
    while let Some(tok) = tokens.next() {
        match tok {
            "reverse" => {
                if reverse {
                    return None;
                }
                reverse = true;
            }
            "step" => {
                if step.is_some() {
                    return None;
                }
                let v = tokens.next()?.parse::<i64>().ok()?;
                if v < 1 {
                    return None;
                }
                step = Some(v);
            }
            _ => return None,
        }
    }
    Some(ForClause {
        var,
        index_var,
        source,
        step,
        reverse,
    })
}

/// Split a `for=` right-hand-side into the source expression and the
/// trailing-modifier slice. Walks left-to-right tracking paren / quote
/// depth so a call-form source (`map(rows, 'label') reverse step 2`)
/// extends through the comma + nested parens cleanly. The boundary
/// fires when we see a whitespace-delimited `step` or `reverse` token
/// at paren-depth 0 outside any quoted string.
fn split_for_source(rhs: &str) -> Option<(String, String)> {
    let bytes = rhs.as_bytes();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut i = 0usize;
    let mut last_ws_end: Option<usize> = None;
    while i < bytes.len() {
        let ch = bytes[i];
        match (in_str, ch) {
            (Some(q), c) if c == q => in_str = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => in_str = Some(ch),
            (None, b'(' | b'[') => depth += 1,
            (None, b')' | b']') => depth -= 1,
            (None, b' ' | b'\t') if depth == 0 => {
                // We're between tokens at depth 0. Peek the next
                // non-whitespace word and check whether it's a
                // modifier. If yes, this whitespace is the boundary.
                let mut j = i + 1;
                while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                    j += 1;
                }
                let mut k = j;
                while k < bytes.len() && !matches!(bytes[k], b' ' | b'\t') {
                    k += 1;
                }
                let tok = &rhs[j..k];
                if matches!(tok, "step" | "reverse") {
                    let source = rhs[..i].to_string();
                    let tail = rhs[j..].to_string();
                    return Some((source, tail));
                }
                last_ws_end = Some(k);
                i = k;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    let _ = last_ws_end;
    Some((rhs.to_string(), String::new()))
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
/// from migrating to `.prism-ui` source. Same attr vocabulary the
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
    let mut focused = false;
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
    Node::TextInput {
        id,
        value,
        placeholder,
        props,
        width,
        height,
        radius: CornerRadius::default(),
        semantic,
        focused,
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
                // Wave 11.2 — Semantic surface for `.prism-ui`-authored
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
            // `.prism-ui` authors write
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
                if let Some(value) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-transition-{}", local), value));
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
            _ => {}
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

fn bare_attr_value(el: &Element, name: &str, scope: &LowerScope) -> Option<String> {
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
fn evaluate_bare_attr_typed(
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
fn attribute_string(value: &AttributeValue) -> Option<String> {
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
fn lookup_path_owned(body: &str, scope: &LowerScope) -> Option<serde_json::Value> {
    if let Some(v) = lookup_expression(body, scope) {
        return Some(v.clone());
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
];

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
    if !ARRAY_CALL_NAMES.contains(&name) {
        return None;
    }
    // Match the closing paren that pairs with `open`, respecting
    // nested parens and quoted strings so `find(rows, 'id', 2)` and
    // `slice(filter(rows, 'k', 'v'), 0, 2)` both find their right
    // boundary cleanly.
    let close = matching_close_paren(body, open)?;
    let inside = &body[open + 1..close];
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
fn eval_truthy(body: &str, scope: &LowerScope) -> bool {
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
                None => ExprValue::String(String::new()),
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
        serde_json::Value::Null => ExprValue::String(String::new()),
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
        let interpolated = interpolate(value, scope);
        let resolved_short = resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
        let final_value = expand_length_units(&resolved_short, scope);
        apply_style_override(props, key, &final_value);
    }
    for (state, key, value) in &resolved.states {
        let interpolated = interpolate(value, scope);
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
            let interpolated = interpolate(value, scope);
            let resolved_short =
                resolve_short_token(key, &interpolated, scope).unwrap_or(interpolated);
            let final_value = expand_length_units(&resolved_short, scope);
            apply_style_override(props, key, &final_value);
        }
        for (state, key, value) in &resolved.states {
            let interpolated = interpolate(value, scope);
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
mod tests {
    use super::*;
    use crate::layout::{compute, Viewport};
    use serde_json::json;

    const FIVE_ELEMENT_SOURCE: &str = r##"<container direction="column" gap="8" padding="16" width="grow" height="grow" style:background="#f0f0f0">
  <text id="title" font-size="24" style:color="#141414">Prism</text>
  <container id="row" direction="row" gap="8" width="grow" height="40" style:background="#ffffff">
    <text id="a">A</text>
    <spacer id="gap" width="16" height="0"/>
    <text id="b">B</text>
  </container>
</container>"##;

    #[test]
    fn parses_minimal_container() {
        let nodes = interpret("<container/>").unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(matches!(nodes[0], Node::Container { .. }));
    }

    #[test]
    fn parses_text_content() {
        let nodes = interpret(r#"<text>Hello</text>"#).unwrap();
        let Node::Text { content, .. } = &nodes[0] else {
            panic!("expected text");
        };
        assert_eq!(content, "Hello");
    }

    #[test]
    fn parses_color_and_sizing() {
        let nodes =
            interpret(r##"<container width="grow" height="40" style:background="#ff8800"/>"##)
                .unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(matches!(props.width, Sizing::Grow));
        assert!(matches!(props.height, Sizing::Fixed(v) if (v - 40.0).abs() < f32::EPSILON));
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b, bg.a), (0xff, 0x88, 0x00, 0xff));
    }

    #[test]
    fn heading_level_drives_font_size() {
        let nodes = interpret(r#"<heading level="3">Hi</heading>"#).unwrap();
        let Node::Text { props, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(props.font_size, 20.0);
    }

    #[test]
    fn five_element_source_round_trips_to_layout() {
        let nodes = interpret(FIVE_ELEMENT_SOURCE).unwrap();
        assert_eq!(nodes.len(), 1);
        let cmds = compute(
            &nodes[0],
            Viewport {
                width: 800.0,
                height: 600.0,
            },
        );
        assert_eq!(cmds.len(), 5);
    }

    #[test]
    fn parse_errors_propagate() {
        let err = interpret("<container>").unwrap_err();
        assert!(!err.is_empty());
    }

    // ---------- Slots ----------

    #[test]
    fn slot_falls_back_to_default_children_when_no_binding() {
        let nodes =
            interpret(r#"<container><slot><text>fallback</text></slot></container>"#).unwrap();
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "fallback");
    }

    #[test]
    fn slot_resolves_default_binding_from_scope() {
        let (doc, errs) = parse(r#"<container><slot/></container>"#);
        assert!(errs.is_empty());
        // Caller injects a `<text>injected</text>` AST node as the
        // default slot — same shape a Phase-3 component-instantiation
        // pass would feed in.
        let (injected, errs) = parse(r#"<text>injected</text>"#);
        assert!(errs.is_empty());
        let scope =
            LowerScope::default().with_slots(SlotBindings::default().with_default(injected.nodes));
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "injected");
    }

    #[test]
    fn slot_named_falls_through_to_host_children_by_slot_map() {
        // Wave 13.1 — when no AST-level SlotBindings carries `header`,
        // the runtime falls through to `host_children_by_slot["header"]`.
        // The resolver populates this from a dispatched element's
        // `slot="X"` AST children; here we set it directly.
        let (doc, errs) = parse(r#"<container><slot name="header"/></container>"#);
        assert!(errs.is_empty());
        let mut map: HashMap<String, Vec<Node>> = HashMap::new();
        map.insert(
            "header".into(),
            vec![Node::Text {
                id: "hdr".into(),
                content: "FROM-HOST".into(),
                props: TextProps::default(),
            }],
        );
        let scope = LowerScope::default().with_host_children_by_slot(Arc::new(map));
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "FROM-HOST");
    }

    #[test]
    fn slot_unknown_name_falls_back_to_fallback_children() {
        // No binding for `nonexistent` — the element's own children
        // (the fallback) render instead.
        let (doc, _) = parse(
            r#"<container><slot name="nonexistent"><text>FALLBACK</text></slot></container>"#,
        );
        let scope = LowerScope::default();
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "FALLBACK");
    }

    #[test]
    fn slot_named_binding_isolates_from_default() {
        let (doc, _) = parse(
            r#"<container>
                <slot name="header"/>
                <slot/>
            </container>"#,
        );
        let (header, _) = parse(r#"<text>HEAD</text>"#);
        let (default, _) = parse(r#"<text>BODY</text>"#);
        let scope = LowerScope::default().with_slots(
            SlotBindings::default()
                .with_named("header", header.nodes)
                .with_default(default.nodes),
        );
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 2);
        if let (Node::Text { content: a, .. }, Node::Text { content: b, .. }) =
            (&children[0], &children[1])
        {
            assert_eq!(a, "HEAD");
            assert_eq!(b, "BODY");
        } else {
            panic!("expected two text children")
        }
    }

    // ---------- Control flow ----------

    #[test]
    fn if_drops_subtree_when_binding_falsy() {
        let (doc, _) = parse(r#"<container><text if="show">visible</text></container>"#);
        let scope = LowerScope::default().with_binding("show", json!(false));
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert!(children.is_empty(), "if=false drops the element");
    }

    #[test]
    fn if_keeps_subtree_when_binding_truthy() {
        let (doc, _) = parse(r#"<container><text if="show">visible</text></container>"#);
        let scope = LowerScope::default().with_binding("show", json!(true));
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn else_if_chain_picks_first_truthy_branch() {
        let (doc, _) = parse(
            r#"<container>
                <text if="a">A</text>
                <text else-if="b">B</text>
                <text else>fallback</text>
            </container>"#,
        );
        let scope = LowerScope::default()
            .with_binding("a", json!(false))
            .with_binding("b", json!(true));
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "B");
    }

    #[test]
    fn else_falls_through_when_all_predicates_false() {
        let (doc, _) = parse(
            r#"<container>
                <text if="a">A</text>
                <text else>fallback</text>
            </container>"#,
        );
        let scope = LowerScope::default().with_binding("a", json!(false));
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "fallback");
    }

    #[test]
    fn for_loop_supports_dotted_field_access_on_object_items() {
        let scope = LowerScope::default().with_binding(
            "items",
            serde_json::json!([
                { "label": "Alpha", "depth": 0 },
                { "label": "Beta", "depth": 2 },
            ]),
        );
        let (doc, errs) = parse(
            r#"<container><text for="item in items">{item.label}={item.depth}</text></container>"#,
        );
        assert!(errs.is_empty());
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 2);
        let Node::Text { content: a, .. } = &children[0] else {
            panic!()
        };
        let Node::Text { content: b, .. } = &children[1] else {
            panic!()
        };
        assert!(a.contains("Alpha") && a.contains('0'));
        assert!(b.contains("Beta") && b.contains('2'));
    }

    #[test]
    fn for_clones_children_with_iteration_binding() {
        let (doc, _) = parse(
            r#"<container>
                <text for="post in posts">{post}</text>
            </container>"#,
        );
        let scope = LowerScope::default().with_binding("posts", json!(["alpha", "beta", "gamma"]));
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 3);
        let contents: Vec<&str> = children
            .iter()
            .map(|c| {
                if let Node::Text { content, .. } = c {
                    content.as_str()
                } else {
                    ""
                }
            })
            .collect();
        assert_eq!(contents, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn for_with_missing_binding_yields_no_children() {
        let (doc, _) = parse(r#"<container><text for="x in missing">{x}</text></container>"#);
        let nodes = lower_document_with_scope(&doc, &LowerScope::default());
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert!(children.is_empty());
    }

    // ---------- Teleport (Wave 14.3) ----------

    /// `<teleport to="overlay-root">` moves its children to the
    /// container with `id="overlay-root"`. At the source position
    /// the teleport emits nothing.
    #[test]
    fn teleport_routes_children_to_target_id() {
        let nodes = interpret(
            r#"<container>
                 <container id="overlay-root"/>
                 <container>
                   <teleport to="overlay-root">
                     <text>routed</text>
                   </teleport>
                 </container>
               </container>"#,
        )
        .unwrap();
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        // Two children: the overlay root + the sibling that hosted
        // the teleport. The teleport itself emits nothing — its
        // child container has zero children.
        assert_eq!(children.len(), 2);
        let Node::Container {
            id: overlay_id,
            children: overlay_children,
            ..
        } = &children[0]
        else {
            panic!("expected overlay-root container")
        };
        assert_eq!(overlay_id, "overlay-root");
        assert_eq!(
            overlay_children.len(),
            1,
            "teleport payload landed at the target"
        );
        let Node::Text { content, .. } = &overlay_children[0] else {
            panic!("expected text payload")
        };
        assert_eq!(content, "routed");

        // The teleport's source sibling has no children — its
        // `<teleport>` body materialised at the target, not here.
        let Node::Container {
            children: source_children,
            ..
        } = &children[1]
        else {
            panic!("expected source-side container")
        };
        assert!(source_children.is_empty());
    }

    /// A teleport with no matching target id silently drops its
    /// payload — matches Vue's behaviour.
    #[test]
    fn teleport_with_missing_target_drops_payload() {
        let nodes = interpret(
            r#"<container>
                 <teleport to="nowhere"><text>lost</text></teleport>
               </container>"#,
        )
        .unwrap();
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert!(children.is_empty());
    }

    /// Multiple teleports targeting the same id stack their payloads
    /// in source order.
    #[test]
    fn teleport_multiple_targets_stack_in_source_order() {
        let nodes = interpret(
            r#"<container>
                 <container id="stack"/>
                 <teleport to="stack"><text>first</text></teleport>
                 <teleport to="stack"><text>second</text></teleport>
               </container>"#,
        )
        .unwrap();
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        let Node::Container {
            children: stack_children,
            ..
        } = &children[0]
        else {
            panic!("expected stack target")
        };
        let labels: Vec<&str> = stack_children
            .iter()
            .filter_map(|n| match n {
                Node::Text { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, vec!["first", "second"]);
    }

    // ---------- Memo (Wave 14.3) ----------

    /// `memo="…"` is a no-op when no cache is installed — the
    /// element lowers normally (proves the round-trip-through-cache
    /// path doesn't depend on host wiring for default behaviour).
    #[test]
    fn memo_without_cache_lowers_element_normally() {
        let (doc, _) = parse(r#"<container id="x" memo="count"><text>hi</text></container>"#);
        let scope = LowerScope::default().with_binding("count", json!(1));
        let nodes = lower_document_with_scope(&doc, &scope);
        assert_eq!(nodes.len(), 1);
        if let Node::Container { children, .. } = &nodes[0] {
            assert_eq!(children.len(), 1);
        } else {
            panic!("expected container")
        }
    }

    /// With a cache installed, the second lowering call with the
    /// same dep tuple returns the cached subtree verbatim and does
    /// **not** re-evaluate the element body — proven here by
    /// flipping the underlying binding the *body* reads. The cached
    /// subtree shows the old text because the memo gate keeps the
    /// re-evaluation from happening.
    #[test]
    fn memo_with_cache_returns_cached_subtree_when_deps_unchanged() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let (doc, _) = parse(r#"<container id="x" memo="count"><text>{label}</text></container>"#);
        let cache = Rc::new(RefCell::new(MemoCache::new()));

        let first = lower_document_with_scope(
            &doc,
            &LowerScope::default()
                .with_binding("count", json!(1))
                .with_binding("label", json!("first"))
                .with_memo_cache(Rc::clone(&cache)),
        );
        let Node::Container { children, .. } = &first[0] else {
            panic!()
        };
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "first");
        assert_eq!(cache.borrow().len(), 1);

        // Same dep, different label: memo gate should bypass the
        // re-lower, so the rendered text stays "first".
        let second = lower_document_with_scope(
            &doc,
            &LowerScope::default()
                .with_binding("count", json!(1))
                .with_binding("label", json!("second"))
                .with_memo_cache(Rc::clone(&cache)),
        );
        let Node::Container { children, .. } = &second[0] else {
            panic!()
        };
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "first", "memo cache hit preserved old subtree");
    }

    /// When any dep moves, the cache invalidates and the body
    /// re-evaluates against the fresh scope.
    #[test]
    fn memo_with_cache_re_evaluates_when_a_dep_changes() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let (doc, _) = parse(r#"<container id="x" memo="count"><text>{label}</text></container>"#);
        let cache = Rc::new(RefCell::new(MemoCache::new()));

        let _ = lower_document_with_scope(
            &doc,
            &LowerScope::default()
                .with_binding("count", json!(1))
                .with_binding("label", json!("first"))
                .with_memo_cache(Rc::clone(&cache)),
        );

        let second = lower_document_with_scope(
            &doc,
            &LowerScope::default()
                .with_binding("count", json!(2))
                .with_binding("label", json!("second"))
                .with_memo_cache(Rc::clone(&cache)),
        );
        let Node::Container { children, .. } = &second[0] else {
            panic!()
        };
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "second", "dep change invalidated the cache");
    }

    /// `memo=` without an id never enters the cache — there'd be
    /// no stable key.
    #[test]
    fn memo_without_id_is_a_noop() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let (doc, _) = parse(r#"<container memo="count"><text>{label}</text></container>"#);
        let cache = Rc::new(RefCell::new(MemoCache::new()));
        let _ = lower_document_with_scope(
            &doc,
            &LowerScope::default()
                .with_binding("count", json!(1))
                .with_binding("label", json!("first"))
                .with_memo_cache(Rc::clone(&cache)),
        );
        assert!(cache.borrow().is_empty());
    }

    // ---------- TextInput ----------

    #[test]
    fn input_lowers_to_text_input_node_with_value() {
        let nodes = interpret(r#"<input value="hello" placeholder="search..."/>"#).unwrap();
        assert_eq!(nodes.len(), 1);
        let Node::TextInput {
            value, placeholder, ..
        } = &nodes[0]
        else {
            panic!("expected TextInput, got {:?}", nodes[0])
        };
        assert_eq!(value, "hello");
        assert_eq!(placeholder, "search...");
    }

    #[test]
    fn image_lowers_to_image_node_with_source_and_sizing() {
        let nodes = interpret(
            r##"<image src="icons/chevron-down.svg" width="10" height="10"
                       style:radius="2" style:tint="#cc000000" aria-label="open"/>"##,
        )
        .unwrap();
        assert_eq!(nodes.len(), 1);
        let Node::Image {
            source,
            width,
            height,
            radius,
            tint,
            semantic,
            ..
        } = &nodes[0]
        else {
            panic!("expected Image, got {:?}", nodes[0])
        };
        assert_eq!(source, "icons/chevron-down.svg");
        assert!(matches!(width, Sizing::Fixed(v) if (v - 10.0).abs() < f32::EPSILON));
        assert!(matches!(height, Sizing::Fixed(v) if (v - 10.0).abs() < f32::EPSILON));
        assert!((radius.tl - 2.0).abs() < f32::EPSILON);
        assert!(tint.is_some());
        assert_eq!(semantic.aria_label.as_deref(), Some("open"));
    }

    #[test]
    fn image_data_and_aria_namespaces_round_trip_on_semantic() {
        let nodes =
            interpret(r##"<image src="icons/x.svg" data:role="close-icon" aria:hidden="true"/>"##)
                .unwrap();
        let Node::Image { semantic, .. } = &nodes[0] else {
            panic!("expected Image")
        };
        assert!(semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "close-icon"));
        assert!(semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-hidden" && v == "true"));
    }

    #[test]
    fn input_kind_string_is_stable() {
        let nodes = interpret(r#"<input value="x"/>"#).unwrap();
        assert_eq!(nodes[0].kind(), "text-input");
    }

    /// Wave 14.3 — `bind:value="<node-id>.<key>"` on `<input>` lowers
    /// to a `data-bind-value` semantic attr on the input's node. The
    /// shell event router reads it back at pointer-down time to open
    /// a field-focus session against the bound source.
    #[test]
    fn input_bind_value_lowers_to_data_bind_value_attr() {
        let nodes = interpret(r#"<input bind:value="form.email"/>"#).unwrap();
        let Node::TextInput { semantic, .. } = &nodes[0] else {
            panic!("expected TextInput")
        };
        assert!(
            semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "data-bind-value" && v == "form.email"),
            "bind:value should round-trip onto the input's semantic.attrs"
        );
    }

    /// `aria:*` / `data:*` on inputs round-trip onto `semantic.attrs`
    /// so SSR + hit-test routing work the same as they do on
    /// containers.
    #[test]
    fn input_data_and_aria_namespaces_round_trip_on_semantic() {
        let nodes =
            interpret(r#"<input value="x" data:role="email-field" aria:label="Email"/>"#).unwrap();
        let Node::TextInput { semantic, .. } = &nodes[0] else {
            panic!("expected TextInput")
        };
        assert!(semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "email-field"));
        assert!(semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-label" && v == "Email"));
    }

    #[test]
    fn input_round_trips_through_layout_and_emits_three_commands() {
        let nodes = interpret(r#"<input value="hi" width="200" height="32"/>"#).unwrap();
        let cmds = compute(
            &nodes[0],
            Viewport {
                width: 800.0,
                height: 600.0,
            },
        );
        // Background rectangle + border + text = 3 commands.
        assert_eq!(cmds.len(), 3);
    }

    // ---------- TagResolver ----------

    /// Stand-in resolver for the unit tests — turns `<my.box>` into a
    /// fixed-size container, leaves every other tag untouched. The
    /// real resolver lives in `prism-builder` and dispatches through
    /// `ComponentRegistry`.
    struct FakeResolver;
    impl TagResolver for FakeResolver {
        fn resolve(&self, element: &Element, _scope: &LowerScope) -> Option<Vec<Node>> {
            if element.tag != "my.box" {
                return None;
            }
            Some(vec![Node::Container {
                id: "from-resolver".into(),
                props: ContainerProps {
                    width: Sizing::Fixed(40.0),
                    height: Sizing::Fixed(40.0),
                    ..Default::default()
                },
                children: vec![],
            }])
        }
    }

    #[test]
    fn resolver_handles_unknown_tag_when_returning_some() {
        let (doc, errs) = parse(r#"<container><my.box/></container>"#);
        assert!(errs.is_empty());
        let scope = LowerScope::default().with_resolver(Arc::new(FakeResolver));
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        let Node::Container {
            id, props: cprops, ..
        } = &children[0]
        else {
            panic!("resolver did not produce container")
        };
        assert_eq!(id, "from-resolver");
        assert_eq!(cprops.width, Sizing::Fixed(40.0));
    }

    #[test]
    fn resolver_returning_none_falls_back_to_default_unknown_tag() {
        // `<scene>` is not handled by FakeResolver, so it falls through
        // to the runtime's default "drop the wrapper, keep children"
        // behaviour — same shape as the no-resolver case.
        let (doc, _) = parse(r#"<scene><text>kept</text></scene>"#);
        let scope = LowerScope::default().with_resolver(Arc::new(FakeResolver));
        let nodes = lower_document_with_scope(&doc, &scope);
        assert_eq!(nodes.len(), 1);
        let Node::Text { content, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(content, "kept");
    }

    #[test]
    fn host_children_by_tag_round_trips_through_scope_getter() {
        // Pin the new injection seam: a host-supplied map keyed by tag
        // surfaces through `host_children_for(tag)` and clones cheaply
        // through scope forks (the `Arc` discipline).
        let mut map: HashMap<String, Vec<Node>> = HashMap::new();
        map.insert(
            "my.canvas".into(),
            vec![Node::Text {
                id: "leaf".into(),
                content: "from-host".into(),
                props: TextProps::default(),
            }],
        );
        let scope = LowerScope::default().with_host_children_by_tag(map);
        let supplied = scope.host_children_for("my.canvas").expect("entry");
        assert_eq!(supplied.len(), 1);
        assert!(scope.host_children_for("absent").is_none());
        // Clone propagates the Arc — every fork sees the same entries
        // without re-cloning the underlying Vec<Node>.
        let forked = scope.clone();
        assert!(forked.host_children_for("my.canvas").is_some());
    }

    #[test]
    fn resolver_propagates_through_for_loop_child_scopes() {
        let (doc, _) = parse(r#"<container><my.box for="x in items"/></container>"#);
        let scope = LowerScope::default()
            .with_binding("items", json!([1, 2, 3]))
            .with_resolver(Arc::new(FakeResolver));
        let nodes = lower_document_with_scope(&doc, &scope);
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 3, "resolver fired once per iteration");
    }

    /// §43 A1: `on:<event>="<action>"` on a bare `<container>`
    /// lowers to a `data-on-<event>` semantic attribute that the
    /// shell event router reads at pointer-down time. The runtime
    /// is intentionally action-grammar-agnostic — it preserves the
    /// raw string and lets the host parse it via
    /// `prism_builder::signal::parse_action`.
    #[test]
    fn on_event_attribute_lowers_to_data_on_attr_on_container() {
        let nodes =
            interpret(r#"<container id="btn" on:click="emit save" on:hover="cmd help.show"/>"#)
                .unwrap();
        let crate::layout::Node::Container { id, props, .. } = &nodes[0] else {
            panic!("expected container, got {:?}", nodes[0])
        };
        assert_eq!(id, "btn");
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            attrs.get("data-on-click").map(String::as_str),
            Some("emit save"),
        );
        assert_eq!(
            attrs.get("data-on-hover").map(String::as_str),
            Some("cmd help.show"),
        );
    }

    /// Wave 9.1 — `route:<key>="<value>"` lowers to a `data-<key>`
    /// semantic attribute the hit-test cache reads off. The
    /// namespace is sugar — `route:role="x"` and `data:role="x"`
    /// emit the same `data-role="x"`.
    #[test]
    fn route_namespace_lowers_to_data_dash_attr_on_container() {
        let nodes =
            interpret(r#"<container id="btn" route:role="resize-handle" route:direction="br"/>"#)
                .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!("expected container, got {:?}", nodes[0])
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            attrs.get("data-role").map(String::as_str),
            Some("resize-handle")
        );
        assert_eq!(attrs.get("data-direction").map(String::as_str), Some("br"));
    }

    /// `data:<key>="<value>"` pass-through stays equivalent to the
    /// `route:` namespace — same lowered shape, different
    /// authoring vocabulary (data: is the bare pass-through,
    /// route: is sugar for the hit-test conventions). Either form
    /// reaches the hit cache.
    #[test]
    fn data_namespace_lowers_to_data_dash_attr_on_container() {
        let nodes =
            interpret(r#"<container data:role="palette-item" data:target-id="text"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            attrs.get("data-role").map(String::as_str),
            Some("palette-item")
        );
        assert_eq!(
            attrs.get("data-target-id").map(String::as_str),
            Some("text")
        );
    }

    /// `aria:<role>="<value>"` lowers to an `aria-<role>` semantic
    /// attribute — the SSR + HTML backends pick it up verbatim,
    /// the runtime hit-test cache does not gate on aria-* attrs
    /// today but the data round-trips so the convention stays
    /// addressable.
    #[test]
    fn aria_namespace_lowers_to_aria_dash_attr_on_container() {
        let nodes = interpret(r#"<container aria:label="Resize" aria:hidden="false"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(attrs.get("aria-label").map(String::as_str), Some("Resize"));
        assert_eq!(attrs.get("aria-hidden").map(String::as_str), Some("false"));
    }

    /// Wave 11.2 — bare `tag` / `role` / `aria-label` attrs on
    /// `<container>` set the dedicated `Semantic` fields directly.
    /// Hand-rolled Rust shell components build these via
    /// `Semantic::tag(..).with_role(..).with_aria_label(..)`; the
    /// DSL needs the same vocabulary for the `.prism-ui`-authored
    /// shell-component migration. The `attrs` vec used by `aria:` /
    /// `data:` namespaces is independent — these three set the
    /// typed fields the HTML emitter reads at the same seam it
    /// always did.
    #[test]
    fn bare_semantic_attrs_set_dedicated_fields_on_container() {
        let nodes = interpret(
            r#"<container tag="section" role="separator" aria-label="Toolbar divider"/>"#,
        )
        .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("section"));
        assert_eq!(props.semantic.role.as_deref(), Some("separator"));
        assert_eq!(
            props.semantic.aria_label.as_deref(),
            Some("Toolbar divider")
        );
        // The dedicated fields don't double-write into `attrs`.
        assert!(props.semantic.attrs.is_empty());
    }

    /// Wave 9.4 — `transition:<prop>="<duration>"` lowers to a
    /// `data-transition-<prop>` semantic attribute the
    /// `Effect`-driven animator (follow-up) consumes.
    #[test]
    fn transition_namespace_lowers_to_data_transition_attr() {
        let nodes =
            interpret(r#"<container transition:opacity="200ms" transition:transform="120ms"/>"#)
                .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            attrs.get("data-transition-opacity").map(String::as_str),
            Some("200ms")
        );
        assert_eq!(
            attrs.get("data-transition-transform").map(String::as_str),
            Some("120ms")
        );
    }

    /// Wave 13.3 — `use:<modifier-id>[="<value>"]` directive lowers
    /// to a `data-use-<id>` semantic attribute. Authors write
    /// `<container use:hover use:tooltip="Click to save"/>` instead
    /// of hand-emitting the `data-use-` ladder; runtime modifier-fold
    /// integration is a follow-up.
    #[test]
    fn use_namespace_lowers_to_data_use_attr() {
        let nodes = interpret(r#"<container use:hover use:tooltip="Click to save"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // Empty-bodied `use:hover` materialises as `data-use-hover="true"`
        // so SSR / hit-test caches see a non-empty value to dispatch on.
        assert_eq!(
            attrs.get("data-use-hover").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            attrs.get("data-use-tooltip").map(String::as_str),
            Some("Click to save")
        );
    }

    /// `bind:<key>="<source>"` lowers to a `data-bind-<key>`
    /// semantic attribute carrying the source path verbatim. The
    /// reactive-binding installer (Phase 4 of the dioxus plan)
    /// reads these off when the document loads.
    #[test]
    fn bind_namespace_lowers_to_data_bind_attr() {
        let nodes = interpret(r#"<container bind:title="$selection.name"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            attrs.get("data-bind-title").map(String::as_str),
            Some("$selection.name")
        );
    }

    /// Wave 9.2 — `style:background:hovered="<color>"` folds into
    /// the container's `hover` override bundle. The plain
    /// `style:background` still lands on `props.background`; the
    /// `:hovered` variant only contributes to `HoverOverrides`.
    #[test]
    fn style_state_namespace_hovered_lowers_into_hover_overrides() {
        let nodes = interpret(
            r##"<container style:background="#000000" style:background:hovered="#3366ff"/>"##,
        )
        .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("resting background set");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x00, 0x00));
        let hover = props.hover.as_ref().expect("hover bundle populated");
        let hbg = hover.background.expect("hover background set");
        assert_eq!((hbg.r, hbg.g, hbg.b), (0x33, 0x66, 0xff));
    }

    /// Wave 9.2 — `style:radius:hovered="<px>"` rounds all four
    /// corners on hover, mirroring the resting-radius shape.
    #[test]
    fn style_state_namespace_hovered_lowers_radius_to_hover_overrides() {
        let nodes = interpret(r#"<container style:radius:hovered="8"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let hover = props.hover.as_ref().expect("hover bundle populated");
        let r = hover.radius.expect("hover radius set");
        assert!((r.tl - 8.0).abs() < f32::EPSILON);
        assert!((r.br - 8.0).abs() < f32::EPSILON);
    }

    /// Wave 9.2 — `:selected` / `:focused` states have no
    /// container-level runtime infra yet, so the override
    /// round-trips as `data-style-<key>-<state>` semantic attrs.
    /// Same shape as Wave 9.4 transitions — data carries author
    /// intent, runtime hookup is the follow-up.
    #[test]
    fn style_state_namespace_selected_and_focused_round_trip_as_data_attrs() {
        let nodes = interpret(
            r##"<container style:background:selected="#ff0000" style:background:focused="#00ff00"/>"##,
        )
        .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            attrs
                .get("data-style-background-selected")
                .map(String::as_str),
            Some("#ff0000")
        );
        assert_eq!(
            attrs
                .get("data-style-background-focused")
                .map(String::as_str),
            Some("#00ff00")
        );
        assert!(
            props.hover.is_none(),
            "non-hover states should not populate hover"
        );
    }

    /// Wave 9.2 — an unrecognized state suffix (`:active`) is
    /// treated as part of the key (no split), so the lookup
    /// against the bare key/state pair falls through cleanly
    /// without touching `props.background` / `props.hover` /
    /// `props.semantic.attrs`.
    #[test]
    fn style_state_namespace_unknown_suffix_falls_through_cleanly() {
        let nodes = interpret(r##"<container style:background:active="#abcdef"/>"##).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(props.background.is_none());
        assert!(props.hover.is_none());
        assert!(props.semantic.attrs.is_empty());
    }

    #[test]
    fn input_paints_placeholder_when_value_empty() {
        let nodes =
            interpret(r#"<input placeholder="type here" width="200" height="32"/>"#).unwrap();
        let cmds = compute(
            &nodes[0],
            Viewport {
                width: 800.0,
                height: 600.0,
            },
        );
        let text_cmd = cmds
            .iter()
            .find_map(|c| {
                if let crate::command::RenderCommand::Text { content, .. } = c {
                    Some(content.as_str())
                } else {
                    None
                }
            })
            .expect("text command emitted");
        assert_eq!(text_cmd, "type here");
    }

    // ─── Wave 11.2 substrate — expression evaluator across attrs ───

    /// Templated attribute with a ternary head picks the matching
    /// branch and stringifies — exactly what the toast / row-variant
    /// migrations need to map a discriminant (`kind`, `selected`) to
    /// a colour or label without a Rust seam.
    #[test]
    fn templated_attribute_resolves_ternary_against_scope() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding("kind", json!("error"));
        let doc =
            parse(r##"<container style:background="{kind == 'error' ? '#cf222e' : '#0969da'}"/>"##)
                .0;
        let nodes = lower_document_with_scope(&doc, &scope);
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        // Container's `style:background` lowered into `props.background`;
        // assert the resolved hex landed on the typed slot.
        let bg = props.background.expect("background set");
        assert_eq!(bg.r, 0xcf);
        assert_eq!(bg.g, 0x22);
        assert_eq!(bg.b, 0x2e);
    }

    /// `if=` on a control-flow attribute runs through the full
    /// expression evaluator so authors can compose boolean expressions
    /// (`enabled && !disabled`) without a Rust normalisation step.
    #[test]
    fn if_attribute_evaluates_boolean_expression() {
        use serde_json::json;
        let scope = LowerScope::default()
            .with_binding("enabled", json!(true))
            .with_binding("disabled", json!(false));
        let doc = parse(
            r#"<container>
                <text if="{enabled && !disabled}">visible</text>
                <text if="{!enabled || disabled}">hidden</text>
            </container>"#,
        )
        .0;
        let nodes = lower_document_with_scope(&doc, &scope);
        let crate::layout::Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        // Exactly one of the two text nodes survives the conditional.
        assert_eq!(children.len(), 1, "if= evaluator should drop the false arm");
    }

    /// `if=` with a comparison against a string literal — the row-variant
    /// migration shape (`if="{kind == 'error'}"`) used across the toast,
    /// signal-connection-row, schema-row, etc.
    #[test]
    fn if_attribute_supports_string_equality_comparison() {
        use serde_json::json;
        let render_with_kind = |k: &str| {
            let scope = LowerScope::default().with_binding("kind", json!(k));
            let doc = parse(
                r#"<container>
                    <text if="{kind == 'error'}">oops</text>
                    <text if="{kind == 'success'}">ok</text>
                </container>"#,
            )
            .0;
            let nodes = lower_document_with_scope(&doc, &scope);
            let crate::layout::Node::Container { children, .. } = &nodes[0] else {
                panic!()
            };
            children.len()
        };
        assert_eq!(render_with_kind("error"), 1);
        assert_eq!(render_with_kind("success"), 1);
        assert_eq!(render_with_kind("info"), 0);
    }

    /// `for="row, idx in rows"` exposes the iteration index as a typed
    /// number binding so the DSL author can synthesise stable per-row
    /// ids — critical for hit-testing dispatched containers, which
    /// only enter the hit cache when their `id` is non-empty.
    #[test]
    fn for_loop_exposes_optional_iteration_index() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding(
            "rows",
            json!([
                { "label": "Body" },
                { "label": "Level" },
                { "label": "Link URL" },
            ]),
        );
        // `<container id="row-{idx}">` produces a unique id per item.
        let doc = parse(
            r#"<container>
                <container for="row, idx in rows" id="row-{idx}">
                    <text>{row.label}</text>
                </container>
            </container>"#,
        )
        .0;
        let nodes = lower_document_with_scope(&doc, &scope);
        let crate::layout::Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        let ids: Vec<&str> = children
            .iter()
            .filter_map(|c| match c {
                crate::layout::Node::Container { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["row-0", "row-1", "row-2"]);
    }

    /// Dotted-path lookups in expression position still resolve to the
    /// underlying JSON value, so a `for="row in rows"` loop addressing
    /// `{row.label}` continues to work after the parser switch.
    #[test]
    fn expression_evaluator_walks_dotted_path_into_object() {
        use serde_json::json;
        let scope =
            LowerScope::default().with_binding("row", json!({ "label": "Hi", "kind": "error" }));
        // Ternary referencing dotted field — exercises both substrate
        // additions in one expression.
        assert_eq!(
            evaluate_expression("row.kind == 'error' ? row.label : 'fallback'", &scope),
            Some(serde_json::Value::String("Hi".to_string()))
        );
    }

    // ── Wave 14 — substrate from HTMX / CSS / SwiftUI / Compose ──

    /// **Wave 14.1** — the `tokens` scope binding round-trips
    /// the design-token table through the existing dotted-path
    /// resolver, so an author can write
    /// `style:background="{tokens.colors.accent}"` and get the same
    /// `ContainerProps.background` the equivalent hex literal would
    /// produce. The colour serialisation uses the standard `#rrggbbaa`
    /// shape `parse_color` consumes.
    #[test]
    fn tokens_binding_resolves_color_in_style_namespace() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(
            r#"<container id="t" style:background="{tokens.colors.accent}"/>"#,
            &scope,
        )
        .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!("expected container, got {:?}", nodes[0]);
        };
        let accent = &prism_core::design_tokens::DEFAULT_TOKENS.colors.accent;
        let bg = props.background.expect("background should resolve");
        assert_eq!(bg.r, accent.r);
        assert_eq!(bg.g, accent.g);
        assert_eq!(bg.b, accent.b);
        assert_eq!(bg.a, accent.a);
    }

    /// **Wave 14.1** — numeric tokens (spacing / radius / typography)
    /// resolve through the same dotted-path lookup; `parse_f32` reads
    /// the number verbatim through the JSON-number value.
    #[test]
    fn tokens_binding_resolves_nested_path_through_style() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(
            r#"<container id="t" padding="{tokens.spacing.md}" style:radius="{tokens.radius.lg}"/>"#,
            &scope,
        )
        .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!("expected container, got {:?}", nodes[0]);
        };
        let md = prism_core::design_tokens::DEFAULT_TOKENS.spacing.md as f32;
        let lg = prism_core::design_tokens::DEFAULT_TOKENS.radius.lg as f32;
        assert!((props.padding.left - md).abs() < f32::EPSILON);
        assert!((props.padding.top - md).abs() < f32::EPSILON);
        assert!((props.radius.tl - lg).abs() < f32::EPSILON);
        assert!((props.radius.br - lg).abs() < f32::EPSILON);
    }

    /// **Wave 14.2** — a sibling `<let name="total" value="{items.length}"/>`
    /// seeds a `total` binding into every subsequent sibling's scope
    /// without consuming a render slot. Today no `items.length`
    /// built-in exists, so the test uses a primitive value pulled
    /// from the parent scope to exercise the propagation rule.
    #[test]
    fn let_binding_propagates_to_subsequent_siblings() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding("base", json!(8));
        let source = r#"
            <let name="doubled" value="{base * 2}"/>
            <container id="t" padding="{doubled}"/>
        "#;
        let nodes = interpret_with_scope(source, &scope).unwrap();
        // Single rendered sibling — the `<let/>` should not emit.
        assert_eq!(nodes.len(), 1, "let should not render: {nodes:?}");
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!("expected container, got {:?}", nodes[0]);
        };
        assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
    }

    /// **Wave 14.2** — a `<let/>` binding overrides a same-named
    /// parent-scope binding in subsequent siblings (lexical
    /// shadowing); the parent-scope value remains untouched outside
    /// the let-scope. Matches Svelte `{@const}` semantics.
    #[test]
    fn let_binding_shadows_parent_scope_for_subsequent_siblings() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding("size", json!(4));
        let source = r#"
            <container id="a" padding="{size}"/>
            <let name="size" value="{20}"/>
            <container id="b" padding="{size}"/>
        "#;
        let nodes = interpret_with_scope(source, &scope).unwrap();
        assert_eq!(nodes.len(), 2);
        let crate::layout::Node::Container {
            id: id_a, props: a, ..
        } = &nodes[0]
        else {
            panic!()
        };
        let crate::layout::Node::Container {
            id: id_b, props: b, ..
        } = &nodes[1]
        else {
            panic!()
        };
        assert_eq!(id_a, "a");
        assert_eq!(id_b, "b");
        assert!((a.padding.left - 4.0).abs() < f32::EPSILON);
        assert!((b.padding.left - 20.0).abs() < f32::EPSILON);
    }

    // ── Wave 15 — iteration / loop patterns ──

    /// **Wave 15.1** — Svelte-style `{:each}{:else}` shape: a `for`
    /// that iterates zero items lets a following `else` sibling
    /// render as the empty-state fallback. Non-empty iteration
    /// suppresses the branch.
    #[test]
    fn else_after_empty_for_renders_fallback() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding("items", json!([]));
        let source = r#"
            <container id="row" for="item in items"/>
            <text else>No items.</text>
        "#;
        let nodes = interpret_with_scope(source, &scope).unwrap();
        // For-loop emitted zero containers; the else-text replaces it.
        assert_eq!(nodes.len(), 1);
        let crate::layout::Node::Text { content, .. } = &nodes[0] else {
            panic!("expected fallback text, got {:?}", nodes[0]);
        };
        assert_eq!(content, "No items.");
    }

    #[test]
    fn else_after_non_empty_for_is_suppressed() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding("items", json!(["a", "b"]));
        let source = r#"
            <container id="row" for="item in items"/>
            <text else>No items.</text>
        "#;
        let nodes = interpret_with_scope(source, &scope).unwrap();
        // Two for-rows emitted; the else-text is suppressed.
        assert_eq!(nodes.len(), 2);
        for n in &nodes {
            assert!(matches!(n, crate::layout::Node::Container { .. }));
        }
    }

    #[test]
    fn else_if_after_empty_for_evaluates_predicate() {
        use serde_json::json;
        let scope = LowerScope::default()
            .with_binding("items", json!([]))
            .with_binding("show", json!(true));
        let source = r#"
            <container id="row" for="item in items"/>
            <text else-if="{show}">Else-if branch.</text>
            <text else>Fallback.</text>
        "#;
        let nodes = interpret_with_scope(source, &scope).unwrap();
        assert_eq!(nodes.len(), 1);
        let crate::layout::Node::Text { content, .. } = &nodes[0] else {
            panic!("expected else-if branch text, got {:?}", nodes[0]);
        };
        assert_eq!(content, "Else-if branch.");
    }

    /// **Wave 15.2** — `for="i in 0..3"` iterates `i ∈ [0, 3)`,
    /// matching Rust's `Range` / Python's `range(3)` shape.
    #[test]
    fn range_exclusive_iterates_start_through_end_minus_one() {
        let source = r#"<container id="row-{i}" for="i in 0..3" padding="{i}"/>"#;
        let nodes = interpret(source).unwrap();
        assert_eq!(nodes.len(), 3);
        for (idx, n) in nodes.iter().enumerate() {
            let crate::layout::Node::Container { id, props, .. } = n else {
                panic!()
            };
            assert_eq!(id, &format!("row-{idx}"));
            assert!((props.padding.left - idx as f32).abs() < f32::EPSILON);
        }
    }

    /// **Wave 15.2** — `..=` inclusive variant matches Rust's
    /// `RangeInclusive` shape.
    #[test]
    fn range_inclusive_iterates_start_through_end() {
        let source = r#"<container id="row-{i}" for="i in 0..=3"/>"#;
        let nodes = interpret(source).unwrap();
        assert_eq!(nodes.len(), 4, "0..=3 includes 0,1,2,3");
    }

    /// **Wave 15.2** — start > end yields zero items, no panic.
    #[test]
    fn range_with_descending_endpoints_yields_zero_items() {
        let source = r#"<container id="row-{i}" for="i in 5..3"/>"#;
        let nodes = interpret(source).unwrap();
        assert!(nodes.is_empty());
    }

    /// **Wave 15.2** — range endpoints resolve through the scope
    /// binding map. `for="i in 0..n"` with `n` bound to 4 iterates
    /// `0..4`.
    #[test]
    fn range_endpoints_resolve_through_scope_bindings() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding("n", json!(4));
        let nodes =
            interpret_with_scope(r#"<container id="r-{i}" for="i in 0..n"/>"#, &scope).unwrap();
        assert_eq!(nodes.len(), 4);
    }

    /// **Wave 15.3** — iterating over a JSON object yields one entry
    /// per `(key, value)` pair in insertion order. The primary LHS
    /// variable binds the value; the optional second LHS variable
    /// binds the string key (mirroring the index-variable shape for
    /// arrays).
    #[test]
    fn for_loop_over_object_iterates_entries_in_insertion_order() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding(
            "props",
            json!({ "alpha": "first", "beta": "second", "gamma": "third" }),
        );
        let source = r#"<text for="value in props">{value}</text>"#;
        let nodes = interpret_with_scope(source, &scope).unwrap();
        let contents: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                crate::layout::Node::Text { content, .. } => content.clone(),
                _ => panic!("expected text node"),
            })
            .collect();
        assert_eq!(contents, vec!["first", "second", "third"]);
    }

    #[test]
    fn for_loop_over_object_binds_key_to_second_lhs_variable() {
        use serde_json::json;
        let scope = LowerScope::default()
            .with_binding("props", json!({ "alpha": "first", "beta": "second" }));
        // `<text>{a} {b}</text>` interpolations are space-joined by
        // `collect_text_content` — keep the assertion on the joined
        // shape rather than fighting the runtime convention.
        let source = r#"<text for="value, key in props">{key} {value}</text>"#;
        let nodes = interpret_with_scope(source, &scope).unwrap();
        let contents: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                crate::layout::Node::Text { content, .. } => content.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(contents, vec!["alpha first", "beta second"]);
    }

    /// **Regression pin** — the Wave 15.3 dispatch on source shape
    /// must not break the existing array + index path. `for="x, idx
    /// in arr"` still binds `idx` to the integer index.
    #[test]
    fn for_loop_index_variable_still_works_on_arrays() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding("xs", json!(["a", "b"]));
        let source = r#"<text for="x, i in xs">{i} {x}</text>"#;
        let nodes = interpret_with_scope(source, &scope).unwrap();
        let contents: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                crate::layout::Node::Text { content, .. } => content.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(contents, vec!["0 a", "1 b"]);
    }

    /// **Wave 14.3** — `on:click.once` lowers to `data-on-click-once`
    /// so the modifier suffix round-trips through the same `data-on-*`
    /// hit-test cache. Today no consumer reads the suffix; data
    /// carries author intent for future runtime wiring.
    #[test]
    fn on_namespace_modifier_suffix_round_trips_as_data_attr() {
        let nodes = interpret(
            r#"<container id="b" on:click.once="cmd confirm" on:click.stop="cmd noop"/>"#,
        )
        .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!("expected container, got {:?}", nodes[0]);
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            attrs.get("data-on-click-once").map(String::as_str),
            Some("cmd confirm")
        );
        assert_eq!(
            attrs.get("data-on-click-stop").map(String::as_str),
            Some("cmd noop")
        );
    }

    /// **Wave 15.4 (step)** — `for="i in 0..10 step 2"` iterates by
    /// `2` between endpoints. Matches Python `range(0, 10, 2)` /
    /// Rust `(0..10).step_by(2)` / SwiftUI `stride(from:to:by:)`.
    #[test]
    fn range_with_step_iterates_by_increment() {
        let source = r#"<container id="r-{i}" for="i in 0..10 step 2"/>"#;
        let nodes = interpret(source).unwrap();
        let ids: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                crate::layout::Node::Container { id, .. } => id.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(ids, vec!["r-0", "r-2", "r-4", "r-6", "r-8"]);
    }

    /// **Wave 15.4 (step)** — `step` composes with `..=` inclusive
    /// ranges.
    #[test]
    fn inclusive_range_with_step_includes_upper_endpoint_when_aligned() {
        let source = r#"<container id="r-{i}" for="i in 0..=10 step 5"/>"#;
        let nodes = interpret(source).unwrap();
        let ids: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                crate::layout::Node::Container { id, .. } => id.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(ids, vec!["r-0", "r-5", "r-10"]);
    }

    /// **Wave 15.4 (step)** — step ≤ 0 rejects the whole `for=`
    /// (treated as `if false`), so the element drops cleanly.
    #[test]
    fn range_with_zero_or_negative_step_drops_element() {
        let zero = interpret(r#"<container id="r-{i}" for="i in 0..10 step 0"/>"#).unwrap();
        assert!(zero.is_empty());
        let neg = interpret(r#"<container id="r-{i}" for="i in 0..10 step -2"/>"#).unwrap();
        assert!(neg.is_empty());
    }

    /// **Wave 15.4 (reverse)** — `for="i in 0..5 reverse"` iterates
    /// `4, 3, 2, 1, 0`. The endpoint shape is identical to forward
    /// iteration; only the emitted order flips.
    #[test]
    fn range_with_reverse_iterates_descending() {
        let source = r#"<container id="r-{i}" for="i in 0..5 reverse"/>"#;
        let nodes = interpret(source).unwrap();
        let ids: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                crate::layout::Node::Container { id, .. } => id.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(ids, vec!["r-4", "r-3", "r-2", "r-1", "r-0"]);
    }

    /// **Wave 15.4 (reverse)** — arrays reverse to last-first order.
    #[test]
    fn array_with_reverse_iterates_last_first() {
        use serde_json::json;
        let scope = LowerScope::default().with_binding("xs", json!(["a", "b", "c"]));
        let nodes =
            interpret_with_scope(r#"<text for="x in xs reverse">{x}</text>"#, &scope).unwrap();
        let contents: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                crate::layout::Node::Text { content, .. } => content.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(contents, vec!["c", "b", "a"]);
    }

    /// **Wave 15.4 (reverse)** — objects reverse insertion order too.
    /// The `<text>` body's joined shape inherits the runtime's
    /// space-between-runs convention from `collect_text_content`.
    #[test]
    fn object_with_reverse_iterates_last_entry_first() {
        use serde_json::json;
        let scope =
            LowerScope::default().with_binding("o", json!({ "x": "1", "y": "2", "z": "3" }));
        let nodes = interpret_with_scope(r#"<text for="v, k in o reverse">{k} {v}</text>"#, &scope)
            .unwrap();
        let contents: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                crate::layout::Node::Text { content, .. } => content.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(contents, vec!["z 3", "y 2", "x 1"]);
    }

    /// **Wave 15.4 (step + reverse)** — modifiers compose. `step`
    /// applies first (filters the range to every Nth element), then
    /// `reverse` flips the filtered sequence.
    #[test]
    fn range_step_and_reverse_compose() {
        let source = r#"<container id="r-{i}" for="i in 0..10 step 2 reverse"/>"#;
        let nodes = interpret(source).unwrap();
        let ids: Vec<String> = nodes
            .iter()
            .map(|n| match n {
                crate::layout::Node::Container { id, .. } => id.clone(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(ids, vec!["r-8", "r-6", "r-4", "r-2", "r-0"]);
    }

    /// **Wave 15.4** — modifiers tolerate either order on the `for`
    /// clause: `reverse step N` parses identically to `step N reverse`.
    #[test]
    fn for_modifiers_accept_either_order() {
        let a = interpret(r#"<container id="r-{i}" for="i in 0..10 reverse step 2"/>"#).unwrap();
        let b = interpret(r#"<container id="r-{i}" for="i in 0..10 step 2 reverse"/>"#).unwrap();
        assert_eq!(a.len(), b.len());
        for (na, nb) in a.iter().zip(b.iter()) {
            match (na, nb) {
                (
                    crate::layout::Node::Container { id: ida, .. },
                    crate::layout::Node::Container { id: idb, .. },
                ) => assert_eq!(ida, idb),
                _ => panic!(),
            }
        }
    }

    /// **Wave 15.4** — repeating a modifier is rejected (drops the
    /// element) so an author who writes `reverse reverse` sees the
    /// missing output and fixes the typo.
    #[test]
    fn for_modifier_duplicate_drops_element() {
        let nodes = interpret(r#"<container for="i in 0..3 reverse reverse"/>"#).unwrap();
        assert!(nodes.is_empty());
        let nodes = interpret(r#"<container for="i in 0..3 step 1 step 2"/>"#).unwrap();
        assert!(nodes.is_empty());
    }

    /// **Wave 15.5** — `key="X"` on a `<container>` lowers to a
    /// `data-key="X"` semantic attr. Reconciliation hint round-trip;
    /// the runtime tree-diff that would consume it is the unblock.
    #[test]
    fn key_attr_lowers_to_data_key_semantic_attr() {
        let nodes = interpret(r#"<container id="row" key="row-42"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(attrs.get("data-key").map(String::as_str), Some("row-42"));
    }

    /// **Wave 15.5** — `key=""` drops cleanly so a ternary that
    /// resolves to the empty string omits the attr (Wave 13 `data:` /
    /// `aria:` empty-string filter pattern).
    #[test]
    fn key_attr_empty_string_drops_cleanly() {
        let nodes = interpret(r#"<container id="row" key=""/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(props.semantic.attrs.iter().all(|(k, _)| k != "data-key"));
    }

    /// **Numeric units** — `parse_f32` accepts CSS-style length
    /// suffixes (`px`, `rem`, `em`). Bare numbers continue to mean
    /// pixels (matches CSS bare-number-is-px convention).
    #[test]
    fn parse_f32_handles_px_rem_em_suffixes() {
        assert_eq!(parse_f32("14"), Some(14.0));
        assert_eq!(parse_f32("14px"), Some(14.0));
        assert_eq!(parse_f32("1rem"), Some(16.0));
        assert_eq!(parse_f32("0.5rem"), Some(8.0));
        assert_eq!(parse_f32("0.875rem"), Some(14.0));
        assert_eq!(parse_f32("1em"), Some(16.0));
        // Trailing whitespace inside the value is tolerated so a
        // ternary that produces `"14 px"` doesn't silently drop.
        assert_eq!(parse_f32("14 px"), Some(14.0));
    }

    #[test]
    fn parse_f32_rejects_unknown_suffixes() {
        assert_eq!(parse_f32("14pt"), None);
        assert_eq!(parse_f32("14vh"), None);
        assert_eq!(parse_f32("nonsense"), None);
    }

    /// **Numeric units** — `parse_sizing` adds `%` and `auto` to
    /// the vocabulary `parse_f32` accepts.
    #[test]
    fn parse_sizing_handles_percent_grow_fit_auto() {
        use crate::layout::Sizing;
        assert!(matches!(parse_sizing("grow"), Some(Sizing::Grow)));
        assert!(matches!(parse_sizing("fit"), Some(Sizing::Fit)));
        assert!(matches!(parse_sizing("auto"), Some(Sizing::Fit)));
        let Some(Sizing::Percent(p)) = parse_sizing("50%") else {
            panic!()
        };
        assert!((p - 0.5).abs() < f32::EPSILON);
        let Some(Sizing::Percent(p)) = parse_sizing("100%") else {
            panic!()
        };
        assert!((p - 1.0).abs() < f32::EPSILON);
    }

    /// **Numeric units (integration)** — `padding="1rem"` lowers
    /// to 16px on a container. Same code path every other
    /// length-valued attribute uses.
    #[test]
    fn padding_in_rem_resolves_to_pixels() {
        let nodes = interpret(r#"<container padding="1rem"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
        assert!((props.padding.right - 16.0).abs() < f32::EPSILON);
    }

    /// **Numeric units (integration)** — `width="50%"` lowers to
    /// `Sizing::Percent(0.5)`.
    #[test]
    fn width_50_percent_lowers_to_sizing_percent() {
        use crate::layout::Sizing;
        let nodes = interpret(r#"<container width="50%" height="100%"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let Sizing::Percent(w) = props.width else {
            panic!("expected percent width, got {:?}", props.width)
        };
        let Sizing::Percent(h) = props.height else {
            panic!("expected percent height, got {:?}", props.height)
        };
        assert!((w - 0.5).abs() < f32::EPSILON);
        assert!((h - 1.0).abs() < f32::EPSILON);
    }

    /// **Numeric units (integration)** — `font-size="0.875rem"`
    /// on a text element lowers to 14px (the typography token
    /// shape PRSS exposes by default).
    #[test]
    fn font_size_in_rem_resolves_to_pixels() {
        let nodes = interpret(r#"<text font-size="0.875rem">Hi</text>"#).unwrap();
        let crate::layout::Node::Text { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!((props.font_size - 14.0).abs() < f32::EPSILON);
    }

    /// **Wave 15.5** — `key="{expr}"` interpolates through scope
    /// so `for="row in rows"` + `key="{row.id}"` works.
    #[test]
    fn key_attr_interpolates_through_scope() {
        use serde_json::json;
        let scope =
            LowerScope::default().with_binding("rows", json!([{"id": "alpha"}, {"id": "beta"}]));
        let nodes = interpret_with_scope(
            r#"<container for="row in rows" id="r-{row.id}" key="{row.id}"/>"#,
            &scope,
        )
        .unwrap();
        let keys: Vec<String> = nodes
            .iter()
            .filter_map(|n| match n {
                crate::layout::Node::Container { props, .. } => props
                    .semantic
                    .attrs
                    .iter()
                    .find(|(k, _)| k == "data-key")
                    .map(|(_, v)| v.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(keys, vec!["alpha", "beta"]);
    }

    /// **PRSS integration** — a `class="…"` attribute on a
    /// `<container>` resolves each named class through the
    /// installed stylesheet and applies its properties via
    /// `apply_style_override`.
    #[test]
    fn prss_class_applies_background_and_radius() {
        use std::sync::Arc;
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.btn]
            background = "#0060c0"
            radius = 8
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("class supplied background");
        assert_eq!((bg.r, bg.g, bg.b, bg.a), (0x00, 0x60, 0xc0, 0xff));
        assert!((props.radius.tl - 8.0).abs() < f32::EPSILON);
    }

    /// **PRSS integration** — `extends` flattens parent properties
    /// first; child overrides win.
    #[test]
    fn prss_extends_inherits_then_overrides() {
        use std::sync::Arc;
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.btn]
            background = "#ffffff"
            radius = 8
            padding = 12

            [class.btn-primary]
            extends = "btn"
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class="btn-primary"/>"#, &scope).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("class supplied background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
        assert!((props.radius.tl - 8.0).abs() < f32::EPSILON);
        assert!((props.padding.left - 12.0).abs() < f32::EPSILON);
    }

    /// **PRSS integration** — multiple classes apply left to
    /// right; the rightmost wins on conflict.
    #[test]
    fn prss_multiple_classes_apply_left_to_right() {
        use std::sync::Arc;
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.btn]
            background = "#ffffff"

            [class.accent]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class="btn accent"/>"#, &scope).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("class supplied background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// **PRSS integration** — inline `style:` always wins over
    /// classes. Application-order rule from §4.6 of the PRSS
    /// reference.
    #[test]
    fn prss_inline_style_overrides_class_property() {
        use std::sync::Arc;
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.btn]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r##"<container class="btn" style:background="#ff0000"/>"##,
            &scope,
        )
        .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("inline wins");
        assert_eq!((bg.r, bg.g, bg.b), (0xff, 0x00, 0x00));
    }

    /// **PRSS integration** — `[class.btn.hovered]` lands in
    /// `ContainerProps.hover` through the existing state-suffix
    /// pathway.
    #[test]
    fn prss_state_variant_lowers_into_hover_overrides() {
        use std::sync::Arc;
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.btn]
            background = "#ffffff"

            [class.btn.hovered]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let hover = props.hover.as_ref().expect("hover override installed");
        let bg = hover.background.expect("hover background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// **PRSS integration** — token overrides merge into the
    /// `tokens` binding so `{tokens.colors.X}` in PRUI resolves
    /// through the override.
    #[test]
    fn prss_tokens_merge_into_tokens_binding() {
        use std::sync::Arc;
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [tokens.colors]
            accent = "#7c3aed"

            [tokens.spacing]
            md = 16
            "##,
        );
        let scope = LowerScope::default()
            .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r##"<container padding="{tokens.spacing.md}" style:background="{tokens.colors.accent}"/>"##,
            &scope,
        )
        .unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("token reads through");
        assert_eq!((bg.r, bg.g, bg.b), (0x7c, 0x3a, 0xed));
        assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
    }

    /// **PRSS integration** — class property values may reference
    /// tokens via `{expr}` interpolation. Same expression evaluator
    /// inline `style:` uses.
    #[test]
    fn prss_class_value_interpolates_token_reference() {
        use std::sync::Arc;
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [tokens.colors]
            brand = "#5b21b6"

            [class.btn]
            background = "{tokens.colors.brand}"
            "##,
        );
        let scope = LowerScope::default()
            .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("interpolated token");
        assert_eq!((bg.r, bg.g, bg.b), (0x5b, 0x21, 0xb6));
    }

    /// **PRSS integration** — without a stylesheet installed,
    /// `class="…"` is a styling no-op. The inline `style:` still
    /// applies.
    #[test]
    fn prss_no_stylesheet_means_class_is_a_noop() {
        let nodes = interpret(r##"<container class="btn" style:background="#abcdef"/>"##).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("inline style: applied");
        assert_eq!((bg.r, bg.g, bg.b), (0xab, 0xcd, 0xef));
    }

    /// **Sugar (`@event`)** — `@click="cmd save"` parses
    /// identically to `on:click="cmd save"` (Vue shorthand). Both
    /// lower to a `data-on-click` semantic attr.
    #[test]
    fn at_prefix_is_alias_for_on_namespace() {
        let nodes = interpret(r#"<container @click="cmd save"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attr = props
            .semantic
            .attrs
            .iter()
            .find(|(k, _)| k == "data-on-click")
            .map(|(_, v)| v.as_str());
        assert_eq!(attr, Some("cmd save"));
    }

    /// **Sugar (`:prop`)** — `:value="form.email"` parses
    /// identically to `bind:value="form.email"` (Vue shorthand).
    #[test]
    fn colon_prefix_is_alias_for_bind_namespace() {
        let nodes = interpret(r#"<container :value="form.email"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attr = props
            .semantic
            .attrs
            .iter()
            .find(|(k, _)| k == "data-bind-value")
            .map(|(_, v)| v.as_str());
        assert_eq!(attr, Some("form.email"));
    }

    /// **Sugar (`<fragment>`)** — emits children verbatim with no
    /// wrapping container.
    #[test]
    fn fragment_element_emits_children_unwrapped() {
        let nodes = interpret(r#"<fragment><text>A</text><text>B</text></fragment>"#).unwrap();
        assert_eq!(nodes.len(), 2);
        for n in &nodes {
            assert!(matches!(n, crate::layout::Node::Text { .. }));
        }
    }

    /// **Sugar (`<fragment>`)** — `if=` on the fragment gates its
    /// whole body. Sibling-level control-flow applies before
    /// element lowering.
    #[test]
    fn fragment_with_if_attribute_gates_children() {
        let nodes = interpret(r#"<fragment if="{false}"><text>hidden</text></fragment>"#).unwrap();
        assert!(nodes.is_empty());
        let nodes = interpret(r#"<fragment if="{true}"><text>shown</text></fragment>"#).unwrap();
        assert_eq!(nodes.len(), 1);
    }

    /// **Sugar (padding shorthand)** — `padding="8 16"` is
    /// vertical/horizontal.
    #[test]
    fn padding_shorthand_two_values_is_vertical_horizontal() {
        let nodes = interpret(r#"<container padding="8 16"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!((props.padding.top - 8.0).abs() < f32::EPSILON);
        assert!((props.padding.bottom - 8.0).abs() < f32::EPSILON);
        assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
        assert!((props.padding.right - 16.0).abs() < f32::EPSILON);
    }

    /// **Sugar (padding shorthand)** — three values: top, H, bottom.
    #[test]
    fn padding_shorthand_three_values_is_top_horizontal_bottom() {
        let nodes = interpret(r#"<container padding="4 8 12"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!((props.padding.top - 4.0).abs() < f32::EPSILON);
        assert!((props.padding.left - 8.0).abs() < f32::EPSILON);
        assert!((props.padding.right - 8.0).abs() < f32::EPSILON);
        assert!((props.padding.bottom - 12.0).abs() < f32::EPSILON);
    }

    /// **Sugar (padding shorthand)** — four values: CSS TRBL order.
    #[test]
    fn padding_shorthand_four_values_is_css_trbl() {
        let nodes = interpret(r#"<container padding="1 2 3 4"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!((props.padding.top - 1.0).abs() < f32::EPSILON);
        assert!((props.padding.right - 2.0).abs() < f32::EPSILON);
        assert!((props.padding.bottom - 3.0).abs() < f32::EPSILON);
        assert!((props.padding.left - 4.0).abs() < f32::EPSILON);
    }

    /// **Sugar (padding shorthand + units)** — each token in the
    /// shorthand goes through `parse_f32`, so `rem` / `px` /
    /// `em` suffixes all work per-token.
    #[test]
    fn padding_shorthand_with_units() {
        let nodes = interpret(r#"<container padding="1rem 8px"/>"#).unwrap();
        let crate::layout::Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!((props.padding.top - 16.0).abs() < f32::EPSILON); // 1rem
        assert!((props.padding.left - 8.0).abs() < f32::EPSILON); // 8px
    }

    // ─── class:foo="{cond}" reactive class toggle ──────────────

    /// Truthy `class:active` applies the named PRSS class as if it
    /// were part of `class="…"`.
    #[test]
    fn class_toggle_truthy_applies_prss_class() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"[class.active]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default()
            .with_binding("on", serde_json::json!(true))
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class:active="{on}"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// Falsy `class:active` does not apply the class — the runtime
    /// renders as if `class:active` were absent.
    #[test]
    fn class_toggle_falsy_skips_prss_class() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"[class.active]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default()
            .with_binding("on", serde_json::json!(false))
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class:active="{on}"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(props.background.is_none());
    }

    /// Boolean attribute form (`class:active` with no `=value`)
    /// reads as truthy — Vue / Svelte parity.
    #[test]
    fn class_toggle_boolean_form_is_truthy() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"[class.active]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class:active/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(props.background.is_some());
    }

    /// Class round-trip — even with no stylesheet loaded, the active
    /// class set lands on `Semantic::class` so the SSR / semantic-HTML
    /// emitter prints `<div class="btn icon">` verbatim. Author intent
    /// survives independently of PRSS lookup.
    #[test]
    fn class_attribute_round_trips_through_semantic_class() {
        let nodes = interpret(r#"<container class="btn icon"/>"#).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(props.semantic.class.as_deref(), Some("btn icon"));
    }

    /// Class toggles append to the static class list and round-trip
    /// through `Semantic::class` together. No-stylesheet path; the
    /// HTML backend would emit `<div class="btn primary">`.
    #[test]
    fn class_toggle_extends_semantic_class_list() {
        let scope = LowerScope::default().with_binding("on", serde_json::json!(true));
        let nodes =
            interpret_with_scope(r#"<container class="btn" class:primary="{on}"/>"#, &scope)
                .unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(props.semantic.class.as_deref(), Some("btn primary"));
    }

    /// Falsy `class:foo` keeps the static list intact in
    /// `Semantic::class` — only truthy toggles contribute.
    #[test]
    fn falsy_class_toggle_does_not_appear_in_semantic_class() {
        let scope = LowerScope::default().with_binding("on", serde_json::json!(false));
        let nodes =
            interpret_with_scope(r#"<container class="btn" class:primary="{on}"/>"#, &scope)
                .unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(props.semantic.class.as_deref(), Some("btn"));
    }

    /// SSR backend reads `Semantic::class` — verify a class attribute
    /// authored on a PRUI container actually emits in the HTML.
    #[test]
    fn ssr_backend_emits_class_attribute_for_authored_class() {
        let nodes = interpret(r#"<container class="btn primary"/>"#).unwrap();
        let html = crate::backends::semantic_html::lower(&nodes[0]);
        assert!(
            html.contains("class=\"btn primary\""),
            "SSR HTML missing class attr: {html}"
        );
    }

    /// `class:foo` layers on top of static `class="…"` — both lists
    /// participate in PRSS application; later toggles win on key
    /// conflicts.
    #[test]
    fn class_toggle_layers_on_static_class_attribute() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.btn]
            background = "#ffffff"

            [class.primary]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default()
            .with_binding("primary", serde_json::json!(true))
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="btn" class:primary="{primary}"/>"#,
            &scope,
        )
        .unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("background");
        // `primary` declared after `btn` in attribute order; later wins.
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    // ─── PRSS descendant selectors ─────────────────────────────

    /// `.btn .icon` matches an `<icon>` (well, container with
    /// class="icon") nested under a container with class="btn",
    /// even with intermediate ancestors.
    #[test]
    fn prss_descendant_selector_matches_through_ancestor_chain() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.btn]
            background = "#fff"

            [class.icon]
            background = "#aaa"

            [class.".btn .icon"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="btn">
                <container class="wrap">
                    <container id="leaf" class="icon"/>
                </container>
            </container>"#,
            &scope,
        )
        .unwrap();
        // Walk to the deepest container.
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// Without a matching ancestor `.btn`, the descendant selector
    /// does not match — the leaf falls back to its flat `.icon`
    /// styling.
    #[test]
    fn prss_descendant_selector_misses_without_matching_ancestor() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.icon]
            background = "#aaa"

            [class.".btn .icon"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container>
                <container id="leaf" class="icon"/>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0xaa, 0xaa, 0xaa));
    }

    fn find_container_by_id<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
        for n in nodes {
            if let Node::Container {
                id: cid, children, ..
            } = n
            {
                if cid == id {
                    return Some(n);
                }
                if let Some(found) = find_container_by_id(children, id) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Three-segment descendant chain (`.a .b .c`) walks two
    /// ancestors before landing on the current element.
    #[test]
    fn prss_three_segment_descendant_selector_matches() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".a .b .c"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="a">
                <container class="b">
                    <container id="leaf" class="c"/>
                </container>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// Three-segment chain skips intermediate ancestors that don't
    /// match — `.a .c` finds `c` even if there's a non-matching `.x`
    /// between `a` and `c`.
    #[test]
    fn prss_descendant_skips_intermediate_non_matching_ancestors() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".a .c"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="a">
                <container class="x">
                    <container class="y">
                        <container id="leaf" class="c"/>
                    </container>
                </container>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// Reverse-order ancestors do not match — `.a .b` requires `a`
    /// strictly outside `b`.
    #[test]
    fn prss_descendant_selector_requires_outer_to_inner_order() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".btn .icon"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        // `.icon` outside, `.btn` inside — selector doesn't match.
        let nodes = interpret_with_scope(
            r#"<container class="icon">
                <container id="leaf" class="btn"/>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        assert!(props.background.is_none());
    }

    /// Descendant selector with state variant: `[class.".btn .icon".hovered]`
    /// applies a hover override on the matched leaf. The state lands
    /// on `ContainerProps::hover` (the same path inline
    /// `style:background:hovered` takes).
    #[test]
    fn prss_descendant_selector_with_state_variant_lowers_into_hover_overrides() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".btn .icon"]
            background = "#0060c0"

            [class.".btn .icon".hovered]
            background = "#a78bfa"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="btn">
                <container id="leaf" class="icon"/>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let hover = props.hover.as_ref().expect("hover overrides");
        let bg = hover.background.expect("hover background");
        assert_eq!((bg.r, bg.g, bg.b), (0xa7, 0x8b, 0xfa));
    }

    /// More-specific descendant selector overrides flat-class on the
    /// same key — CSS specificity ordering: `.btn .icon` (specificity
    /// 0,0,2,0) wins over `.icon` (0,0,1,0).
    #[test]
    fn prss_descendant_selector_overrides_flat_class_for_same_key() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.icon]
            background = "#aaaaaa"

            [class.".btn .icon"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="btn">
                <container id="leaf" class="icon"/>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        // Descendant selector wins.
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// Multiple descendant selectors that all match contribute their
    /// disjoint properties — `.a .x` sets background, `.b .x` sets
    /// radius; both apply on a leaf nested under both ancestors.
    #[test]
    fn prss_multiple_descendant_selectors_apply_disjoint_keys() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".a .x"]
            background = "#0060c0"

            [class.".b .x"]
            radius = 8
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="a">
                <container class="b">
                    <container id="leaf" class="x"/>
                </container>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
        assert!((props.radius.tl - 8.0).abs() < f32::EPSILON);
    }

    /// Conflicting descendant selectors resolve by declaration
    /// order — later wins, mirroring `extends` chain ordering.
    #[test]
    fn prss_conflicting_descendant_selectors_resolve_by_declaration_order() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".a .x"]
            background = "#aaaaaa"

            [class.".b .x"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="a">
                <container class="b">
                    <container id="leaf" class="x"/>
                </container>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        // `.b .x` declared second → wins.
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// Descendant selector inside a sibling subtree must not "leak"
    /// — once we leave the matching ancestor's subtree, the
    /// chain unwinds.
    #[test]
    fn prss_descendant_selector_does_not_leak_into_sibling_subtree() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".btn .icon"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container>
                <container class="btn">
                    <container id="inner" class="icon"/>
                </container>
                <container id="sibling" class="icon"/>
            </container>"#,
            &scope,
        )
        .unwrap();
        let inner = find_container_by_id(&nodes, "inner").expect("inner");
        let sibling = find_container_by_id(&nodes, "sibling").expect("sibling");
        let Node::Container { props: ip, .. } = inner else {
            panic!()
        };
        let Node::Container { props: sp, .. } = sibling else {
            panic!()
        };
        let inner_bg = ip.background.expect("inner background");
        assert_eq!((inner_bg.r, inner_bg.g, inner_bg.b), (0x00, 0x60, 0xc0));
        // Sibling has no `.btn` ancestor — selector misses.
        assert!(sp.background.is_none());
    }

    /// Descendant selector with class toggle on the leaf — the
    /// truthy `class:icon="{cond}"` toggle still feeds the
    /// rightmost-segment match.
    #[test]
    fn prss_descendant_selector_with_class_toggle_on_leaf() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".btn .icon"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default()
            .with_binding("show", serde_json::json!(true))
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="btn">
                <container id="leaf" class:icon="{show}"/>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// Descendant selector with class toggle on the *ancestor* — the
    /// truthy `class:btn="{cond}"` toggle on an outer container
    /// participates in the chain match.
    #[test]
    fn prss_descendant_selector_with_class_toggle_on_ancestor() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".btn .icon"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default()
            .with_binding("primary", serde_json::json!(true))
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class:btn="{primary}">
                <container id="leaf" class="icon"/>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// Same class on multiple ancestors doesn't break the matcher —
    /// the inner ancestor consumes the segment, the outer one is
    /// available for further matches if needed.
    #[test]
    fn prss_descendant_selector_handles_repeated_class_in_chain() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.".btn .btn .icon"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="btn">
                <container class="btn">
                    <container id="leaf" class="icon"/>
                </container>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    /// Bare-key (unquoted) descendant selector also parses through
    /// `selector_segments` — `[class."btn icon"]` matches
    /// `[class.".btn .icon"]` because both decompose to `["btn", "icon"]`.
    #[test]
    fn prss_descendant_selector_accepts_bare_segments_without_dots() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class."btn icon"]
            background = "#0060c0"
            "##,
        );
        let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(
            r#"<container class="btn">
                <container id="leaf" class="icon"/>
            </container>"#,
            &scope,
        )
        .unwrap();
        let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
        let Node::Container { props, .. } = leaf else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    }

    // ─── Short-name token references ───────────────────────────

    /// PRSS `radius = "md"` resolves through the active token table
    /// to the `tokens.radius.md` value.
    #[test]
    fn prss_short_name_radius_resolves_through_tokens() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"[class.btn]
            radius = "md"
            "##,
        );
        let scope = LowerScope::default()
            .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let expected = prism_core::design_tokens::DEFAULT_TOKENS.radius.md as f32;
        assert!((props.radius.tl - expected).abs() < f32::EPSILON);
    }

    /// Inline `style:background="accent"` resolves through
    /// `tokens.colors.accent`.
    #[test]
    fn inline_style_short_name_color_resolves_through_tokens() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes =
            interpret_with_scope(r#"<container style:background="accent"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let expected = &prism_core::design_tokens::DEFAULT_TOKENS.colors.accent;
        let bg = props.background.expect("background");
        assert_eq!(bg.r, expected.r);
        assert_eq!(bg.g, expected.g);
        assert_eq!(bg.b, expected.b);
    }

    /// Bare `padding="md"` resolves through `tokens.spacing.md`.
    #[test]
    fn bare_padding_short_name_resolves_through_spacing_tokens() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(r#"<container padding="md"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let expected = prism_core::design_tokens::DEFAULT_TOKENS.spacing.md as f32;
        assert!((props.padding.left - expected).abs() < f32::EPSILON);
    }

    /// Unknown short names drop silently — the value passes through
    /// to the parser, which fails to interpret and leaves the prop
    /// at its default. Matches PRSS's "unknown drops cleanly" rule.
    #[test]
    fn short_name_lookup_miss_falls_through_to_parser() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes =
            interpret_with_scope(r#"<container padding="not-a-token-name"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        // Default Padding::all(0) survives.
        assert!((props.padding.left - 0.0).abs() < f32::EPSILON);
    }

    /// Custom token (PRSS-defined `tokens.colors.brand-purple`)
    /// resolves through the merged token table on the scope.
    #[test]
    fn short_name_resolves_through_custom_prss_token() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [tokens.colors]
            brand-purple = "#5b21b6"

            [class.btn]
            background = "brand-purple"
            "##,
        );
        let scope = LowerScope::default()
            .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x5b, 0x21, 0xb6));
    }

    /// `font-size = "lg"` resolves to `tokens.typography.font-size-lg`
    /// — the typography bucket's `font-size-<short>` key shape.
    #[test]
    fn short_name_resolves_font_size_lg_through_typography_table() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(r#"<text font-size="lg">Hi</text>"#, &scope).unwrap();
        let Node::Text { props, .. } = &nodes[0] else {
            panic!()
        };
        let expected = prism_core::design_tokens::DEFAULT_TOKENS
            .typography
            .font_size_lg as f32;
        assert!((props.font_size - expected).abs() < f32::EPSILON);
    }

    /// Per-side padding short names also resolve — `padding-left = "md"`
    /// reads through `tokens.spacing.md`.
    #[test]
    fn short_name_resolves_per_side_padding_through_spacing() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(
            r#"<container padding-left="md" padding-right="lg"/>"#,
            &scope,
        )
        .unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let expected_md = prism_core::design_tokens::DEFAULT_TOKENS.spacing.md as f32;
        let expected_lg = prism_core::design_tokens::DEFAULT_TOKENS.spacing.lg as f32;
        assert!((props.padding.left - expected_md).abs() < f32::EPSILON);
        assert!((props.padding.right - expected_lg).abs() < f32::EPSILON);
    }

    /// `gap = "sm"` resolves through `tokens.spacing.sm`.
    #[test]
    fn short_name_resolves_gap_through_spacing() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(r#"<container gap="sm"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let expected = prism_core::design_tokens::DEFAULT_TOKENS.spacing.sm as f32;
        assert!((props.gap - expected).abs() < f32::EPSILON);
    }

    /// Sizing keywords (`grow`, `fit`, `auto`) on `width`/`height`
    /// must NOT be resolved as short tokens — these keys aren't in
    /// the bucket table, so the keyword passes through to
    /// `parse_sizing` unchanged.
    #[test]
    fn short_name_resolution_does_not_eat_sizing_keywords() {
        use crate::layout::Sizing;
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes =
            interpret_with_scope(r#"<container width="grow" height="fit"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(matches!(props.width, Sizing::Grow));
        assert!(matches!(props.height, Sizing::Fit));
    }

    /// `direction = "row"` is a known direction keyword — even
    /// though "row" looks like a token name, the `direction` key
    /// isn't in the short-name bucket map so resolution is skipped
    /// and the keyword reaches `parse_direction` intact.
    #[test]
    fn short_name_resolution_does_not_eat_direction_keyword() {
        use crate::layout::Direction;
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(r#"<container direction="row"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(matches!(props.direction, Direction::Row));
    }

    /// Hex colors (`#…`) must NOT be resolved as short tokens —
    /// the leading `#` rejects them at `is_bare_token_name`.
    #[test]
    fn short_name_resolution_skips_hex_color_values() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes =
            interpret_with_scope(r##"<container style:background="#7c3aed"/>"##, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x7c, 0x3a, 0xed));
    }

    /// Numeric values with units (e.g. `"1rem"`, `"50%"`,
    /// `"12px"`, `"8"`) must NOT be resolved — they have a leading
    /// digit so `is_bare_token_name` rejects them. The `1rem` here
    /// resolves through rem-expansion using the scope's
    /// `tokens.typography.font-size-md` base (14 in DEFAULT_TOKENS),
    /// not through `tokens.spacing.1rem` lookup.
    #[test]
    fn short_name_resolution_skips_numeric_and_unit_values() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(r#"<container padding="1rem"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let expected = prism_core::design_tokens::DEFAULT_TOKENS
            .typography
            .font_size_md as f32;
        assert!((props.padding.left - expected).abs() < f32::EPSILON);
    }

    /// `data:` namespace attrs are NOT eligible for short-name
    /// resolution — `data:role="md"` round-trips verbatim.
    #[test]
    fn short_name_resolution_skips_data_namespace() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(r#"<container data:role="md"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        // The data attribute carries through as a literal "md",
        // not the resolved `tokens.spacing.md` value.
        assert!(
            props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "data-role" && v == "md"),
            "data:role should round-trip verbatim, got {:?}",
            props.semantic.attrs
        );
    }

    /// `aria:` namespace attrs are NOT eligible for short-name
    /// resolution — `aria:level="md"` round-trips verbatim.
    #[test]
    fn short_name_resolution_skips_aria_namespace() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes = interpret_with_scope(r#"<container aria:level="md"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(
            props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "aria-level" && v == "md"),
            "aria:level should round-trip verbatim, got {:?}",
            props.semantic.attrs
        );
    }

    /// Short-name resolution still fires inside a state-suffixed
    /// style override — `style:background:hovered="accent"` resolves
    /// through `tokens.colors.accent` then writes to the hover bundle.
    #[test]
    fn short_name_resolves_inside_style_state_override() {
        let scope =
            LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
        let nodes =
            interpret_with_scope(r#"<container style:background:hovered="accent"/>"#, &scope)
                .unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let hover = props.hover.as_ref().expect("hover overrides");
        let bg = hover.background.expect("hover background");
        let expected = &prism_core::design_tokens::DEFAULT_TOKENS.colors.accent;
        assert_eq!(bg.r, expected.r);
    }

    /// Compound state on a PRSS class — `[class.btn.hovered]` with
    /// short-name `background = "accent-muted"` resolves through
    /// the typography path correctly.
    #[test]
    fn short_name_resolves_inside_prss_class_state_variant() {
        let (sheet, _) = prism_core::language::prss::parse(
            r##"
            [class.btn]
            background = "surface"

            [class.btn.hovered]
            background = "accent-muted"
            "##,
        );
        let scope = LowerScope::default()
            .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
            .with_stylesheet(Arc::new(sheet));
        let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        // Base: background = surface
        let surface = &prism_core::design_tokens::DEFAULT_TOKENS.colors.surface;
        let bg = props.background.expect("background");
        assert_eq!(bg.r, surface.r);
        // Hover: accent-muted
        let hover = props.hover.as_ref().expect("hover");
        let muted = &prism_core::design_tokens::DEFAULT_TOKENS
            .colors
            .accent_muted;
        let hover_bg = hover.background.expect("hover background");
        assert_eq!(hover_bg.r, muted.r);
    }

    /// Without a `tokens` binding installed, short-name resolution
    /// is a no-op — the literal value passes through to the parser.
    #[test]
    fn short_name_resolution_no_op_without_tokens() {
        let scope = LowerScope::default(); // no tokens
        let nodes = interpret_with_scope(r#"<container padding="md"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        // No resolution happened; "md" failed `parse_padding_shorthand`
        // and the default Padding::all(0) survived.
        assert!((props.padding.left - 0.0).abs() < f32::EPSILON);
    }

    // ─── Token-driven rem base ─────────────────────────────────

    /// `1rem` reads through `tokens.typography.font-size-md` when
    /// the scope has a token table installed. Double the base
    /// → double the resolved padding.
    #[test]
    fn rem_base_reads_from_typography_font_size_md() {
        let mut tokens = prism_core::design_tokens::DEFAULT_TOKENS;
        tokens.typography.font_size_md = 32; // 32px base instead of 16
        let scope = LowerScope::default().with_design_tokens(&tokens);
        let nodes = interpret_with_scope(r#"<container padding="1rem"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!((props.padding.left - 32.0).abs() < f32::EPSILON);
    }

    /// Without a token binding, `rem_px` falls back to the canonical
    /// 16px so `1rem` reads as 16 — preserving existing behaviour.
    #[test]
    fn rem_base_defaults_to_sixteen_without_tokens() {
        let scope = LowerScope::default();
        let nodes = interpret_with_scope(r#"<container padding="1rem"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
    }

    /// Multi-segment shorthand expansion (`padding="1rem 2rem"`)
    /// honours the scope-driven rem base across every segment.
    #[test]
    fn rem_base_applies_to_each_padding_shorthand_token() {
        let mut tokens = prism_core::design_tokens::DEFAULT_TOKENS;
        tokens.typography.font_size_md = 20;
        let scope = LowerScope::default().with_design_tokens(&tokens);
        let nodes = interpret_with_scope(r#"<container padding="1rem 2rem"/>"#, &scope).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        // top = 1rem = 20, left = 2rem = 40
        assert!((props.padding.top - 20.0).abs() < f32::EPSILON);
        assert!((props.padding.left - 40.0).abs() < f32::EPSILON);
    }

    // ---------- `<dispatch tag="{expr}"/>` runtime-tag dispatch ----------

    /// A literal `tag="container"` rewrites the dispatch element into
    /// a closed-set primitive and lowers it through the container arm
    /// — every container attribute (`gap`, `style:*`, …) flows through
    /// the synthesised element verbatim.
    #[test]
    fn dispatch_tag_literal_routes_to_primitive_container() {
        let nodes =
            interpret(r##"<dispatch tag="container" gap="12" style:background="#aabbcc"/>"##)
                .unwrap();
        assert_eq!(nodes.len(), 1);
        let Node::Container { props, .. } = &nodes[0] else {
            panic!("expected container, got {:?}", nodes[0]);
        };
        assert!((props.gap - 12.0).abs() < f32::EPSILON);
        let bg = props.background.expect("bg");
        assert_eq!((bg.r, bg.g, bg.b), (0xaa, 0xbb, 0xcc));
    }

    /// `tag="{binding}"` resolves against scope and re-dispatches.
    /// Closes the §15 "data-driven tag" authoring gap for the
    /// primitive vocabulary.
    #[test]
    fn dispatch_tag_resolves_through_scope_binding() {
        let scope = LowerScope::default().with_binding("kind", json!("text"));
        let nodes =
            interpret_with_scope(r#"<dispatch tag="{kind}">hello</dispatch>"#, &scope).unwrap();
        let Node::Text { content, .. } = &nodes[0] else {
            panic!("expected text");
        };
        assert_eq!(content, "hello");
    }

    /// Unknown resolved tags fall through to the resolver — no resolver
    /// means the "drop wrapper, keep children" default fires, exactly
    /// as for any author-written unknown tag.
    #[test]
    fn dispatch_tag_unknown_falls_through_to_default_unknown_tag() {
        let nodes =
            interpret(r#"<dispatch tag="my.widget"><text>inner</text></dispatch>"#).unwrap();
        assert_eq!(nodes.len(), 1);
        let Node::Text { content, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(content, "inner");
    }

    /// Empty/missing `tag=` leaves the element as bare `<dispatch>`,
    /// which falls through to the resolver path (legacy `component=`
    /// form). Without a resolver, the unknown-tag default fires.
    #[test]
    fn dispatch_with_empty_tag_attribute_falls_through() {
        let nodes = interpret(r#"<dispatch tag=""><text>fallback</text></dispatch>"#).unwrap();
        let Node::Text { content, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(content, "fallback");
    }

    /// The synthesised element retains every non-`tag` attribute so a
    /// dispatched primitive sees the same prop shape an authored
    /// element would.
    #[test]
    fn dispatch_tag_drops_tag_attribute_but_keeps_others() {
        let nodes =
            interpret(r##"<dispatch tag="text" font-size="22" id="dyn-title">Title</dispatch>"##)
                .unwrap();
        let Node::Text { id, content, props } = &nodes[0] else {
            panic!()
        };
        assert_eq!(id, "dyn-title");
        assert_eq!(content, "Title");
        assert!((props.font_size - 22.0).abs() < f32::EPSILON);
    }

    /// Dispatch composes with `for=` — typical use case is rendering
    /// a row whose primitive varies by data.
    #[test]
    fn dispatch_tag_inside_for_loop_emits_one_node_per_item() {
        let scope = LowerScope::default().with_binding(
            "rows",
            json!([
                {"kind": "text", "body": "A"},
                {"kind": "text", "body": "B"},
                {"kind": "spacer"},
            ]),
        );
        let nodes = interpret_with_scope(
            r#"<container>
                <dispatch for="row in rows" tag="{row.kind}">{row.body}</dispatch>
               </container>"#,
            &scope,
        )
        .unwrap();
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 3);
        assert!(matches!(children[0], Node::Text { .. }));
        assert!(matches!(children[1], Node::Text { .. }));
        assert!(matches!(children[2], Node::Spacer { .. }));
    }

    // ---------- Virtual `.length` / `.first` / `.last` segments ----------

    /// `items.length` reads as the array length from a bare-path
    /// lookup — closes the `for="i in 0..items.length"` authoring
    /// gap the PRUI reference promises.
    #[test]
    fn array_length_virtual_segment_reads_as_count() {
        let scope = LowerScope::default().with_binding("rows", json!(["a", "b", "c", "d"]));
        let nodes = interpret_with_scope(
            r#"<container>
                <text for="i in 0..rows.length">{i}</text>
               </container>"#,
            &scope,
        )
        .unwrap();
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 4);
    }

    /// `obj.length` reads as the number of keys.
    #[test]
    fn object_length_virtual_segment_reads_as_key_count() {
        let scope = LowerScope::default()
            .with_binding("form", json!({"name": "x", "email": "y", "age": 1}));
        let nodes = interpret_with_scope(r#"<text>{form.length}</text>"#, &scope).unwrap();
        let Node::Text { content, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(content, "3");
    }

    /// `items.first` / `items.last` resolve to the first/last
    /// element value.
    #[test]
    fn array_first_and_last_virtual_segments_resolve_to_endpoints() {
        let scope = LowerScope::default().with_binding("rows", json!(["alpha", "beta", "gamma"]));
        let nodes = interpret_with_scope(
            r#"<container>
                <text>{rows.first}</text>
                <text>{rows.last}</text>
               </container>"#,
            &scope,
        )
        .unwrap();
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        let Node::Text { content: first, .. } = &children[0] else {
            panic!()
        };
        let Node::Text { content: last, .. } = &children[1] else {
            panic!()
        };
        assert_eq!(first, "alpha");
        assert_eq!(last, "gamma");
    }

    /// Empty array → first/last return Null which stringifies to ""
    /// (the empty-string filter elsewhere already absorbs this).
    #[test]
    fn empty_array_first_returns_empty_string() {
        let scope = LowerScope::default().with_binding("rows", json!([]));
        let nodes = interpret_with_scope(r#"<text>{rows.first}</text>"#, &scope).unwrap();
        let Node::Text { content, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(content, "");
    }

    /// `if="{items.length}"` is truthy when non-empty, falsy when
    /// empty — matches the JS array-truthiness rule authors expect.
    #[test]
    fn if_with_length_is_falsy_on_empty_array() {
        let scope = LowerScope::default().with_binding("rows", json!([]));
        let nodes =
            interpret_with_scope(r#"<text if="{rows.length}">visible</text>"#, &scope).unwrap();
        assert!(nodes.is_empty());
    }

    /// `if="{items.length > 0}"` flows through the full evaluator and
    /// sees the same synthesized length value.
    #[test]
    fn comparison_against_length_in_evaluator_works() {
        let scope = LowerScope::default().with_binding("rows", json!(["a", "b", "c"]));
        let nodes = interpret_with_scope(
            r#"<container>
                <text if="{rows.length > 2}">many</text>
                <text else>few</text>
               </container>"#,
            &scope,
        )
        .unwrap();
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "many");
    }

    /// Strings also expose `.length` / `.first` / `.last`, matching
    /// the surface promise that the same virtual segments work on
    /// every container-shape.
    #[test]
    fn string_length_first_last_resolve_through_virtual_segments() {
        let scope = LowerScope::default().with_binding("word", json!("hello"));
        let nodes = interpret_with_scope(
            r#"<container>
                <text>{word.length}</text>
                <text>{word.first}</text>
                <text>{word.last}</text>
               </container>"#,
            &scope,
        )
        .unwrap();
        let Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        let texts: Vec<String> = children
            .iter()
            .map(|c| match c {
                Node::Text { content, .. } => content.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(
            texts,
            vec!["5".to_string(), "h".to_string(), "o".to_string()]
        );
    }

    // ---------- on:event empty-string filter + modifier flattening ----------

    /// `on:click=""` drops cleanly so a ternary that resolves to ""
    /// omits the handler, matching the `data:`/`aria:` rule.
    #[test]
    fn on_event_empty_string_drops_the_attribute() {
        let nodes = interpret(r#"<container on:click=""/>"#).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(!props
            .semantic
            .attrs
            .iter()
            .any(|(k, _)| k == "data-on-click"));
    }

    /// `on:click.once.stop` flattens to `data-on-click-once-stop` —
    /// the dotted suffix is the author surface; consumers read the
    /// dash-joined wire form.
    #[test]
    fn on_event_modifier_dot_suffix_flattens_to_dash_in_data_attr() {
        let nodes = interpret(r#"<container on:click.once.stop="cmd save"/>"#).unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let attr = props
            .semantic
            .attrs
            .iter()
            .find(|(k, _)| k == "data-on-click-once-stop");
        assert!(attr.is_some(), "{:?}", props.semantic.attrs);
        assert_eq!(attr.unwrap().1, "cmd save");
    }

    /// `on:event_key` is the shared canonical-key helper — sanity
    /// check it directly so a future refactor doesn't drift the wire
    /// shape silently.
    #[test]
    fn on_event_attr_key_replaces_dots_with_dashes() {
        assert_eq!(on_event_attr_key("click"), "click");
        assert_eq!(on_event_attr_key("click.once"), "click-once");
        assert_eq!(on_event_attr_key("click.once.stop"), "click-once-stop");
    }
}
