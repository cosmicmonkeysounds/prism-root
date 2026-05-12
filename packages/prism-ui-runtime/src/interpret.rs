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
    lower_children(&document.nodes, scope)
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
    match el.tag.as_str() {
        "container" | "component" => {
            let mut props = ContainerProps::default();
            let mut id = String::new();
            apply_container_attributes(el, scope, &mut props, &mut id);
            let children = lower_children(&el.children, scope);
            vec![Node::Container {
                id,
                props,
                children,
            }]
        }
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
            out.push((node.clone(), None));
            continue;
        };
        let cf = control_flow_attr(el);
        match cf {
            None => {
                chain_taken = None;
                out.push((node.clone(), None));
            }
            Some(ControlFlow::If(cond)) => {
                let take = eval_truthy(&cond, scope);
                chain_taken = Some(take);
                if take {
                    out.push((node.clone(), None));
                }
            }
            Some(ControlFlow::ElseIf(cond)) => {
                let take = matches!(chain_taken, Some(false)) && eval_truthy(&cond, scope);
                if let Some(prev) = chain_taken.as_mut() {
                    *prev = *prev || take;
                }
                if take {
                    out.push((node.clone(), None));
                }
            }
            Some(ControlFlow::Else) => {
                let take = matches!(chain_taken, Some(false));
                chain_taken = None;
                if take {
                    out.push((node.clone(), None));
                }
            }
            Some(ControlFlow::For {
                var,
                index_var,
                source,
            }) => {
                chain_taken = None;
                let items = scope
                    .binding(&source)
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                for (idx, item) in items.into_iter().enumerate() {
                    let mut child_scope = scope.clone().with_binding(var.clone(), item);
                    // Optional iteration-index binding — `for="row, idx
                    // in rows"` exposes the index as a typed integer.
                    // Authors use it to build stable per-row ids
                    // (`id="row-{idx}"`) so hit-testing has a unique key
                    // per dispatched container.
                    if let Some(idx_name) = index_var.as_ref() {
                        child_scope = child_scope
                            .with_binding(idx_name.clone(), serde_json::Value::from(idx as i64));
                    }
                    out.push((node.clone(), Some(child_scope)));
                }
            }
        }
    }
    out
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
                .map(|(var, index_var, source)| ControlFlow::For {
                    var,
                    index_var,
                    source,
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
fn parse_for_clause(body: &str) -> Option<(String, Option<String>, String)> {
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
    let source = halves.next()?.trim().to_string();
    if source.is_empty() {
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
    // Original guard: reject trailing junk in the source.
    let mut parts = source.split_whitespace();
    let _ = parts.next();
    if parts.next().is_some() {
        return None;
    }
    Some((var, index_var, source))
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
        semantic: Semantic::default(),
        focused: false,
    }
}

fn apply_container_attributes(
    el: &Element,
    scope: &LowerScope,
    props: &mut ContainerProps,
    id: &mut String,
) {
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        let raw = resolved_attribute_string(&attr.value, scope);
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
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        props.padding = Padding::all(v);
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
            AttributeNamespace::On => {
                if let Some(action) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-on-{}", local), action));
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
            // Wave 12 follow-up: mirror the `data:` namespace's
            // empty-string filter so authors can use ternary
            // (`aria:level="{depth > 0 ? depth + 1 : ''}"`) to omit
            // the attribute conditionally. A literal `aria-foo=""` is
            // meaningless to every screen reader, so dropping it
            // matches user intent rather than HTML's serialisation
            // shape.
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
        let raw = resolved_attribute_string(&attr.value, scope);
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
                // Cheap bare-path lookup first (Wave 11.2 substrate);
                // operator-bearing bodies (ternary, `||`, `&&`, `==`)
                // fall through to the full evaluator so authors can
                // write `<text>{text ? text : status}</text>` against
                // the same vocabulary attribute interpolations use.
                let resolved = if let Some(v) = lookup_expression(&expr.body, scope) {
                    stringify_value(v)
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
        // Cheap path first — bare dotted-path binding lookup. Keeps
        // typed `Value::String("hi")` stringified verbatim (lookup
        // returns `&"hi"`, `stringify_value` strips the quotes) and
        // avoids paying the expression-parser cost for plain `{name}`
        // interpolations. Operator-bearing expressions fall through
        // to `evaluate_expression`.
        if let Some(v) = lookup_expression(body, scope) {
            return stringify_value(v);
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
            if let Some(v) = lookup_expression(body, scope) {
                out.push_str(&stringify_value(v));
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
    // bool). Falls through for any operator-bearing expression because
    // `lookup_expression` only walks bare dotted paths.
    if let Some(v) = lookup_expression(body, scope) {
        return match v {
            serde_json::Value::Null => false,
            serde_json::Value::Bool(b) => *b,
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
            let mut cursor = match self.scope.binding(id) {
                Some(v) => v,
                None => return ExprValue::String(String::new()),
            };
            if let Some(path) = subfield {
                for seg in path.split('.') {
                    let seg = seg.trim();
                    if seg.is_empty() {
                        return ExprValue::String(String::new());
                    }
                    cursor = match cursor {
                        serde_json::Value::Object(map) => match map.get(seg) {
                            Some(v) => v,
                            None => return ExprValue::String(String::new()),
                        },
                        serde_json::Value::Array(arr) => {
                            match seg.parse::<usize>().ok().and_then(|i| arr.get(i)) {
                                Some(v) => v,
                                None => return ExprValue::String(String::new()),
                            }
                        }
                        _ => return ExprValue::String(String::new()),
                    };
                }
            }
            json_to_expr_value(cursor)
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
        ExprValue::Number(n) => serde_json::Number::from_f64(n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
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

fn parse_f32(s: &str) -> Option<f32> {
    s.trim().trim_end_matches("px").parse::<f32>().ok()
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
            if let Some(v) = parse_f32(value) {
                props.padding = Padding::all(v);
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

fn parse_sizing(s: &str) -> Option<Sizing> {
    let s = s.trim();
    match s {
        "grow" => Some(Sizing::Grow),
        "fit" => Some(Sizing::Fit),
        _ => parse_f32(s).map(Sizing::Fixed),
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
}
