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
    parse, AttributeNamespace, AttributeValue, Document as AstDocument, Element, Node as AstNode,
    ParseError,
};

use crate::command::{Color, CornerRadius};
use crate::layout::{ContainerProps, Direction, Node, Padding, Semantic, Sizing, TextProps};

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
        // `<slot/>` and `<slot name="x"/>` resolve to whatever the
        // caller injected. Default slot uses the unnamed binding;
        // named slots match by `name`. If the caller didn't bind a
        // matching slot, the slot element's own children render as
        // fallback content — the same semantics every component DSL
        // converged on. See `LowerScope`/`SlotBindings`.
        "slot" => {
            let name = bare_attr_value(el, "name", scope);
            match scope.slots.resolve(name.as_deref()) {
                Some(injected) => lower_children(injected, scope),
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
            Some(ControlFlow::For { var, source }) => {
                chain_taken = None;
                let items = scope
                    .binding(&source)
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                for item in items {
                    let child_scope = scope.clone().with_binding(var.clone(), item);
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
    For { var: String, source: String },
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
                .map(|(var, source)| ControlFlow::For { var, source })
                .unwrap_or_else(|| ControlFlow::If("false".into())),
            _ => return None,
        });
    }
    None
}

/// `"post in posts"` → `("post", "posts")`. Whitespace tolerant; any
/// other shape returns `None` and the caller treats the element as
/// dropped (`if false`).
fn parse_for_clause(body: &str) -> Option<(String, String)> {
    let body = body
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    let mut parts = body.split_whitespace();
    let var = parts.next()?.to_string();
    let kw = parts.next()?;
    if kw != "in" {
        return None;
    }
    let source = parts.next()?.to_string();
    if parts.next().is_some() {
        return None;
    }
    Some((var, source))
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
                _ => {}
            },
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    *id = v;
                }
            }
            AttributeNamespace::Style => match local {
                "background" => {
                    if let Some(c) = raw.as_deref().and_then(parse_color) {
                        props.background = Some(c);
                    }
                }
                "radius" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        props.radius = CornerRadius {
                            tl: v,
                            tr: v,
                            br: v,
                            bl: v,
                        };
                    }
                }
                _ => {}
            },
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
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&interpolate(value.trim(), scope));
            }
            AstNode::Interpolation(expr) => {
                if let Some(v) = lookup_expression(&expr.body, scope) {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(&stringify_value(v));
                }
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
    match value {
        AttributeValue::String { value, .. } => Some(value.clone()),
        AttributeValue::Empty => None,
        AttributeValue::Expression(expr) => Some(
            lookup_expression(&expr.body, scope)
                .map(stringify_value)
                .unwrap_or_default(),
        ),
        AttributeValue::Template { parts, .. } => {
            let mut out = String::new();
            for part in parts {
                match part {
                    prism_core::language::prism_ui::ast::TemplatePart::Literal {
                        value, ..
                    } => out.push_str(value),
                    prism_core::language::prism_ui::ast::TemplatePart::Expression(e) => {
                        if let Some(v) = lookup_expression(&e.body, scope) {
                            out.push_str(&stringify_value(v));
                        }
                    }
                }
            }
            Some(out)
        }
    }
}

/// Scan a literal text run for `{ident}` segments and resolve them
/// against the scope. Cheap, single-pass — no expression parser, just
/// identifier lookup. Lifts to a real expression evaluator (Prism
/// Syntax) in Phase 2 tail per plan §4.7.
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
            if let Some(v) = lookup_expression(body.trim(), scope) {
                out.push_str(&stringify_value(v));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Resolve a `{...}`-style expression body against the scope. Phase-2
/// minimum: bare identifiers + the literals `true`/`false`/numbers.
/// Anything else lowers to `None` and the caller falls back to an
/// empty string. The full expression evaluator lives in
/// `prism_core::language::expression` and lands here behind the same
/// seam when component instantiation grows past identifiers.
fn lookup_expression<'a>(body: &str, scope: &'a LowerScope) -> Option<&'a serde_json::Value> {
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    scope.binding(body)
}

/// Truthy evaluator for `if=` / `else-if=`. Mirrors the JS rule:
/// missing/empty = false; literal `false`/`0` = false; otherwise true
/// when the binding resolves to a non-empty value. Keeps the
/// vocabulary small and predictable for v0; richer predicates land
/// when the expression evaluator does.
fn eval_truthy(body: &str, scope: &LowerScope) -> bool {
    let body = body
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    if body.is_empty() {
        return false;
    }
    if body.eq_ignore_ascii_case("true") {
        return true;
    }
    if body.eq_ignore_ascii_case("false") {
        return false;
    }
    if let Ok(n) = body.parse::<f64>() {
        return n != 0.0;
    }
    match scope.binding(body) {
        None => false,
        Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        Some(serde_json::Value::Array(a)) => !a.is_empty(),
        Some(serde_json::Value::Object(o)) => !o.is_empty(),
    }
}

fn stringify_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn parse_f32(s: &str) -> Option<f32> {
    s.trim().trim_end_matches("px").parse::<f32>().ok()
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
}
