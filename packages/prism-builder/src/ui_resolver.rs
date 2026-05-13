//! Registry-aware [`TagResolver`] — the bridge that lets `.prism-ui`
//! source reference any registered [`Component`] (`shell.icon-button`,
//! `app.foo`, user prefabs) by tag.
//!
//! This is the single seam between `prism-ui-runtime`'s engine-only
//! tag vocabulary (`container`, `text`, `heading`, `spacer`, `input`,
//! `slot`) and the open-ended set of host-supplied components. The
//! runtime asks the resolver about every tag it doesn't own; the
//! resolver looks the tag up in the [`ComponentRegistry`] and
//! delegates rendering to the block's own `Component::lower_ui`.
//!
//! ## Smart pattern: composition over a registration trait
//!
//! There is no parallel "runtime block" trait. The resolver re-uses
//! the same [`Component::lower_ui`] every block already implements,
//! so adding a new tag to the `.prism-ui` vocabulary is **zero
//! additional work** beyond the standard block registration. The
//! mapping is:
//!
//! ```text
//! <shell.icon-button icon="x.svg"/>
//!     │
//!     ├── ComponentRegistry.get("shell.icon-button")  → block
//!     ├── element_to_node()                           → builder Node
//!     └── block.lower_ui(ctx, &node, &style)          → runtime Node
//! ```
//!
//! ## Children
//!
//! Most chrome blocks consume their visual structure from props, not
//! children — IconButton, ToolbarSeparator, NavButton, DragNumberField,
//! TransformEditor, FieldEditor, etc. all synthesise their layout from
//! `node.props`. The resolver therefore passes `children: vec![]` by
//! default; tag-driven recursion is the resolver's job, not the
//! block's. Composition-style blocks that *do* recurse into builder
//! children (`AppWindow`) are out of scope for v0 — they take the
//! direct `Block::lower_ui(&node)` path, with the host constructing
//! a builder Node tree by hand. A follow-up extension can pre-lower
//! AST children through the runtime and inject them as a `<slot/>`
//! binding when a block opts in.

use std::collections::HashMap;
use std::sync::Arc;

use prism_core::language::prism_ui::{
    ast::TemplatePart, AttributeNamespace, AttributeValue, Element, Node as AstNode,
};
use prism_ui_runtime::interpret::{
    apply_style_override, evaluate_expression, lookup_path_owned_in_scope, lower_ast_children,
    stringify_value_for_template, LowerScope, TagResolver,
};
use prism_ui_runtime::layout::Node as UiNode;
use serde_json::{Map, Value};

use crate::document::Node as BuilderNode;
use crate::layout::LayoutMode;
use crate::registry::ComponentRegistry;
use crate::style::StyleProperties;
use crate::ui_lower::LowerCtx;
use prism_core::foundation::spatial::Transform2D;

/// `TagResolver` impl backed by a [`ComponentRegistry`]. Pass an
/// `Arc<ComponentRegistry>` (the shape `prism-shell` already keeps on
/// `ShellInner`) and hand the resulting `Arc<Self>` to
/// [`LowerScope::with_resolver`].
pub struct RegistryTagResolver {
    registry: Arc<ComponentRegistry>,
}

impl RegistryTagResolver {
    pub fn new(registry: Arc<ComponentRegistry>) -> Self {
        Self { registry }
    }

    /// Convenience constructor that wraps a borrowed registry into an
    /// `Arc` clone of its existing components — useful in tests where
    /// the registry is built locally.
    pub fn from_registry(registry: ComponentRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }
}

impl TagResolver for RegistryTagResolver {
    fn resolve(&self, element: &Element, scope: &LowerScope) -> Option<Vec<UiNode>> {
        // ── `<dispatch component="{expr}" props="{expr}"/>` —
        // dynamic dispatch by resolving the `component` attr against
        // scope and looking up the target block at render time. Closes
        // the Wave 11.2 substrate gap that blocked `properties-panel`'s
        // rows-with-component-field migration. The same path supports
        // any block that wants to materialise children whose component
        // ids only exist in data — recursive trees, plugin-authored
        // overlays, etc.
        let (component_id, dispatched_node) = if element.tag == "dispatch" {
            let target = dynamic_dispatch_target(element, scope)?;
            let component = self.registry.get(&target)?;
            (
                component,
                dispatch_element_to_builder_node(element, &target, scope),
            )
        } else {
            let component = self.registry.get(&element.tag)?;
            (component, element_to_builder_node(element, scope))
        };
        let component = component_id;
        let node = dispatched_node;
        let cascade = StyleProperties::default();
        // Host-injected children (binding-driven composition) win over
        // AST-pre-lowered children. The two paths cover disjoint cases
        // today — a host injects for tags whose live content is
        // computed (e.g. `shell.builder-canvas` rendering a
        // `BuilderDocument`), while AST children land on tags whose
        // source authors a literal subtree
        // (`<shell.app-window>…</shell.app-window>`). Both reach
        // `host_children` on `LowerCtx` through the same opt-in slot —
        // the block doesn't care which path produced them. See
        // `LowerScope::with_host_children_by_tag` for the host-side
        // injection seam.
        let host_supplied: Option<Vec<UiNode>> = scope
            .host_children_for(&element.tag)
            .map(|slice| slice.to_vec());
        // Wave 13.1 — partition AST children by their `slot="X"`
        // attribute. The default bucket (`""`) plus the legacy single-
        // slot contract land in `host_children`; each named bucket
        // lands in `host_children_by_slot` so `<slot name="X"/>` reads
        // pull from the right pile. When the host already supplied
        // pre-lowered children (`host_children_for(tag)` hit), the slot
        // map is empty — composition blocks that opt into named slots
        // use the AST path.
        let (pre_lowered, slot_map): (Vec<UiNode>, HashMap<String, Vec<UiNode>>) =
            if let Some(injected) = host_supplied {
                (injected, HashMap::new())
            } else if element.children.is_empty() {
                (Vec::new(), HashMap::new())
            } else {
                partition_children_by_slot(&element.children, scope)
            };
        // Thread the scope's tag-keyed emission snapshot into LowerCtx
        // so any `lower_as` call inside `component.lower_ui` (the
        // dock-panel routing path is the canonical caller) picks up
        // the same per-tag binding emission this resolver call sees
        // for host_children. Without this thread-through, routed
        // content tags (`shell.builder-canvas`, `shell.component-palette`,
        // `shell.properties-panel`) get empty props / zero children.
        // Wave 11.3 — treat empty pre_lowered as "no host children"
        // rather than "host children present and empty". Without this
        // filter, a DSL block's `<host-children>fallback</host-children>`
        // pattern always picks the empty path because the loader
        // installs `Some(empty Vec)` regardless. The lower_as path
        // already had this filter; the resolver path inherits it now
        // so the dock-panel migration's body fallback `<dispatch
        // component="{content-tag}"/>` actually fires when no caller
        // body was authored.
        let mut ctx = LowerCtx::new(Some(&self.registry), &cascade)
            .with_tag_emissions(scope.tag_emissions_arc());
        if !pre_lowered.is_empty() {
            ctx = ctx.with_host_children(&pre_lowered);
        }
        if !slot_map.is_empty() {
            ctx = ctx.with_host_children_by_slot(Arc::new(slot_map));
        }
        let mut lowered = component.lower_ui(&ctx, &node, &cascade);
        // §43 A1: any `on:<event>="<action>"` attribute on the source
        // element rides through to the lowered container as a
        // `data-on-<event>` semantic attr. The `element_to_builder_node`
        // helper deliberately drops the `On` namespace (per the
        // attribute table in its docstring) because the block doesn't
        // need it during render — the shell event router reads it
        // back from the resulting `HitRect.attrs` instead.
        attach_on_handlers(&mut lowered, element, scope);
        // Wave 12 — Vue/React-style style prop passing. `style:<k>="<v>"`
        // and `style="{obj}"` spread on the source element override
        // matching fields on the lowered container's `ContainerProps`.
        // Applied AFTER `lower_ui` so the block computes its natural
        // styling first; the caller's overrides win. Single seam —
        // `apply_style_override` in the runtime owns the vocabulary
        // (background / radius / padding / gap / width / height + the
        // `:hovered` overrides), and the resolver feeds keys through
        // it verbatim.
        attach_style_overrides(&mut lowered, element, scope);
        Some(vec![lowered])
    }
}

/// Annotate the lowered runtime node with `data-on-<event>` semantic
/// attributes for every `on:*` attribute on the source element. The
/// surface contract is "containers carry handlers" — leaves (text,
/// spacer, image, text-input) don't contribute to `Surface::hit_test_at`
/// hits and so can't dispatch. Authors who want a clickable text node
/// today wrap it in a container; a follow-up can either tag the
/// inner leaf's outer container or grow hit-testable leaves.
/// Wave 13.1 — partition a dispatched element's AST children by their
/// `slot="X"` attribute (Vue / Web Components named-slot pattern).
/// Children with no `slot=` attribute land in the default bucket; the
/// rest land in a `HashMap<String, Vec<UiNode>>` keyed by slot name.
///
/// **Why partition before lowering**: each bucket lowers in the
/// *caller's* scope (parent context), not the dispatched component's
/// scope. That matches Vue / Svelte slot semantics — slot content
/// reads bindings from where it was authored, not where it was
/// emitted. Lowering each bucket separately is the cheapest seam.
///
/// `slot="X"` is a bare attribute, not a namespaced one, mirroring
/// the Web Components convention. Authors write
/// `<text slot="header">Title</text>` and the `slot` attribute is
/// consumed by the resolver — it never reaches the dispatched block
/// as a prop.
fn partition_children_by_slot(
    children: &[AstNode],
    scope: &LowerScope,
) -> (Vec<UiNode>, HashMap<String, Vec<UiNode>>) {
    let mut default_bucket: Vec<AstNode> = Vec::new();
    let mut named_buckets: HashMap<String, Vec<AstNode>> = HashMap::new();
    for node in children {
        match node {
            AstNode::Element(el) => {
                let slot_name = el.attributes.iter().find_map(|attr| {
                    if matches!(attr.name.namespace, AttributeNamespace::Bare)
                        && attr.name.local == "slot"
                    {
                        resolved_attribute_string(&attr.value, scope)
                    } else {
                        None
                    }
                });
                match slot_name {
                    Some(name) if !name.is_empty() => {
                        named_buckets.entry(name).or_default().push(node.clone())
                    }
                    _ => default_bucket.push(node.clone()),
                }
            }
            other => default_bucket.push(other.clone()),
        }
    }
    let default_ui = if default_bucket.is_empty() {
        Vec::new()
    } else {
        lower_ast_children(&default_bucket, scope)
    };
    let slot_map: HashMap<String, Vec<UiNode>> = named_buckets
        .into_iter()
        .map(|(name, asts)| (name, lower_ast_children(&asts, scope)))
        .collect();
    (default_ui, slot_map)
}

fn attach_on_handlers(node: &mut UiNode, element: &Element, scope: &LowerScope) {
    let mut on_attrs: Vec<(String, String)> = Vec::new();
    for attr in &element.attributes {
        if !matches!(attr.name.namespace, AttributeNamespace::On) {
            continue;
        }
        let Some(value) = resolved_attribute_string(&attr.value, scope) else {
            continue;
        };
        // Empty-string filter (PRUI ref §4) + dotted event-modifier
        // suffix flattening (Wave 14.3) — share the runtime's helper
        // so dispatched `<shell.*>` containers and source-authored
        // `<container>` elements emit identically.
        if value.is_empty() {
            continue;
        }
        let local_key = prism_ui_runtime::interpret::on_event_attr_key(&attr.name.local);
        on_attrs.push((format!("data-on-{}", local_key), value));
    }
    if on_attrs.is_empty() {
        return;
    }
    if let UiNode::Container { props, .. } = node {
        props.semantic.attrs.extend(on_attrs);
    }
}

/// Apply every `style:<key>[:<state>]` attribute and `style="{obj}"`
/// spread on the source element to the block's lowered container.
/// Runs AFTER `Component::lower_ui` so the block's natural styling
/// computes first; the parent's overrides win.
///
/// **Vue/React parallel:** in React you write
/// `<Button style={{background: 'red'}}/>`; here you write
/// `<shell.icon-button style:background="#ff0000"/>` or
/// `<shell.icon-button style="{theme.button}"/>`. Both reach the
/// same final container, both behave the same — the caller can
/// reshape any child's resting visual without the child opting in.
///
/// Leaves (text / spacer / image) silently ignore overrides — same
/// pattern `attach_on_handlers` uses; future leaf-level overrides
/// (e.g. text color) land as a sibling helper if needed.
fn attach_style_overrides(node: &mut UiNode, element: &Element, scope: &LowerScope) {
    let mut overrides: Vec<(String, String)> = Vec::new();
    for attr in &element.attributes {
        match attr.name.namespace {
            // `style:<key>="<value>"` — typed per-key override.
            AttributeNamespace::Style => {
                if let Some(value) = resolved_attribute_string(&attr.value, scope) {
                    overrides.push((attr.name.local.clone(), value));
                }
            }
            // `style="{obj}"` — spread a JSON object's entries as
            // style overrides. Mirrors the `props="{item}"` spread
            // pattern: each (k, v) becomes a per-key override, where
            // values are coerced to strings the same way authored
            // `style:k="v"` reaches `apply_style_override`. Non-object
            // resolutions are silently ignored — same shape as the
            // props spread fallback.
            AttributeNamespace::Bare if attr.name.local == "style" => {
                let resolved = resolved_attribute_value(&attr.value, scope);
                if let Value::Object(map) = resolved {
                    for (k, v) in map {
                        let s = match v {
                            Value::String(s) => s,
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            Value::Null => continue,
                            other => other.to_string(),
                        };
                        overrides.push((k, s));
                    }
                }
            }
            _ => {}
        }
    }
    if overrides.is_empty() {
        return;
    }
    if let UiNode::Container { props, .. } = node {
        for (key, value) in overrides {
            apply_style_override(props, &key, &value);
        }
    }
}

/// Translate an AST [`Element`] into a builder [`BuilderNode`] the
/// block's `Component::lower_ui` can consume.
///
/// Mapping (single source of truth — every namespace handled here, not
/// in callers):
///
/// | Attribute namespace | Lands in           |
/// | ------------------- | ------------------ |
/// | `id="foo"`          | `node.id`          |
/// | bare `key="value"`  | `node.props[key]`  |
/// | `data:k="v"`        | `node.props[k]`    |
/// | `aria:k="v"`        | `node.props["aria-{k}"]` |
/// | `style:*`           | ignored (cascade comes from `parent_style`) |
/// | `on:*` / `bind:*` / `sig:*` / `fct:*` / control-flow | ignored (handled separately upstream) |
///
/// Boolean attributes (`<el disabled>`) become `Bool(true)`.
/// Strings stay strings; the block's schema does the typed coercion.
///
/// Interpolated attributes resolve through `scope`: a pure
/// `key="{expr}"` returns the underlying JSON Value verbatim (so a
/// `for="item in items"` over `Vec<Object>` can spread fields directly
/// onto the dispatched block via `prop="{item.field}"`), while a
/// templated `key="prefix-{expr}"` resolves to its expanded string.
fn element_to_builder_node(element: &Element, scope: &LowerScope) -> BuilderNode {
    let mut id = String::new();
    let mut props: Map<String, Value> = Map::new();
    for attr in &element.attributes {
        let local = attr.name.local.as_str();
        match attr.name.namespace {
            AttributeNamespace::Identifier if local == "id" => {
                id = resolved_attribute_string(&attr.value, scope).unwrap_or_default();
            }
            // `props="{expr}"` spread: when `expr` resolves to a JSON
            // object, every (k, v) becomes a prop on the dispatched
            // node. Subsequent bare attrs in the same element override
            // matching keys. Wave 11.2 enabler for list-binding rows
            // (`shell.nav-page-list` / `shell.explorer` /
            // `shell.signals-panel`) — `<shell.nav-page-row props="{item}"/>`
            // spreads the entire item object onto the row without
            // enumerating every schema key in the DSL.
            AttributeNamespace::Bare if local == "props" => {
                if let Value::Object(map) = resolved_attribute_value(&attr.value, scope) {
                    for (k, v) in map {
                        props.insert(k, v);
                    }
                }
            }
            // Wave 12 — `style="{obj}"` is a styling spread consumed
            // by `attach_style_overrides` post-lower; it is NOT a
            // block prop. Swallow it here so blocks don't see a stray
            // `style` key in their props bag.
            AttributeNamespace::Bare if local == "style" => {}
            AttributeNamespace::Bare => {
                props.insert(
                    local.to_string(),
                    resolved_attribute_value(&attr.value, scope),
                );
            }
            AttributeNamespace::Data => {
                props.insert(
                    local.to_string(),
                    resolved_attribute_value(&attr.value, scope),
                );
            }
            AttributeNamespace::Aria => {
                props.insert(
                    format!("aria-{local}"),
                    resolved_attribute_value(&attr.value, scope),
                );
            }
            // Wave 10.4 / Wave 13.4: forward `bind:KEY="X"` through
            // registered component tags as `props["bind-KEY"] = "X"`,
            // mirroring the `Bind` arm in
            // `dispatch_element_to_builder_node`. The receiving block
            // (e.g. `prism.text-input`) then re-emits the carrier as a
            // `data-bind-value` semantic attr that the shell's
            // input-focus route consumes for two-way writeback.
            AttributeNamespace::Bind => {
                props.insert(
                    format!("bind-{local}"),
                    resolved_attribute_value(&attr.value, scope),
                );
            }
            // Styling, signals, facets, control-flow keywords
            // are not block-prop carriers — the cascade handles styles,
            // signals flow through their own dispatch paths,
            // and control-flow attrs were consumed by the runtime's
            // pre-pass before the resolver was called.
            _ => {}
        }
    }
    BuilderNode {
        id,
        component: element.tag.clone(),
        props: Value::Object(props),
        children: Vec::new(),
        layout_mode: LayoutMode::default(),
        transform: Transform2D::default(),
        modifiers: Vec::new(),
        style: StyleProperties::default(),
    }
}

/// True when the expression body is a bare dotted path / identifier
/// (no operators, no comparisons, no function calls). Used to gate
/// the evaluator fall-through: bare paths use `lookup_expression_in_scope`
/// (returns Null on miss — the prop reader's expected fallback);
/// operator-bearing expressions invoke the full evaluator.
fn is_bare_path_expression(body: &str) -> bool {
    let t = body.trim();
    if t.is_empty() {
        return false;
    }
    t.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Resolve an attribute value to a string under `scope`. Handles
/// pure literals, single `{expr}` interpolations, and templated mixes.
/// Bare dotted-path lookups go through [`lookup_expression_in_scope`]
/// (cheap, returns a `&Value`); operator-bearing expressions fall
/// through to [`evaluate_expression`] so ternary, boolean, comparison,
/// and arithmetic work uniformly in DSL attributes.
///
/// Returns `None` for `AttributeValue::Empty` (boolean attrs).
fn resolved_attribute_string(value: &AttributeValue, scope: &LowerScope) -> Option<String> {
    fn resolve_one(body: &str, scope: &LowerScope) -> String {
        // Owned lookup so virtual `.length`/`.first`/`.last` segments
        // resolve through the same vocabulary as the runtime's
        // attribute paths.
        if let Some(v) = lookup_path_owned_in_scope(body, scope) {
            return stringify_value_for_template(&v);
        }
        if is_bare_path_expression(body) {
            return String::new();
        }
        match evaluate_expression(body, scope) {
            Some(v) => stringify_value_for_template(&v),
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
                    TemplatePart::Literal { value, .. } => out.push_str(value),
                    TemplatePart::Expression(expr) => {
                        out.push_str(&resolve_one(&expr.body, scope));
                    }
                }
            }
            Some(out)
        }
    }
}

/// Resolve an attribute value to a typed JSON [`Value`] under `scope`.
/// A pure `{expr}` returns the bound JSON value verbatim — arrays,
/// objects, numbers, and bools survive the dispatch seam — so
/// `for="item in items"` over `Vec<Object>` can spread typed fields
/// onto the dispatched block. Operator-bearing expressions fall through
/// to the full evaluator, returning a computed `Value`. Everything
/// else (literal strings, templates) is coerced through [`value_for`].
fn resolved_attribute_value(value: &AttributeValue, scope: &LowerScope) -> Value {
    match value {
        AttributeValue::Empty => Value::Bool(true),
        AttributeValue::Expression(expr) => {
            // Bare-path lookup first (virtual segments included) — this
            // is the typed-prop pass-through that keeps arrays / objects
            // intact for `props="{item}"` spread. Operator-bearing
            // expressions still flow through the evaluator.
            if let Some(v) = lookup_path_owned_in_scope(&expr.body, scope) {
                return v;
            }
            if is_bare_path_expression(&expr.body) {
                return Value::Null;
            }
            evaluate_expression(&expr.body, scope).unwrap_or(Value::Null)
        }
        _ => value_for(resolved_attribute_string(value, scope)),
    }
}

/// Resolve the `component` attribute of a `<dispatch …/>` element to
/// a registered component id. The attribute is interpolated against
/// the active scope so `<dispatch component="{item.component}"/>`
/// works inside a `for` loop. Returns `None` when the attribute is
/// missing or resolves to an empty string.
fn dynamic_dispatch_target(element: &Element, scope: &LowerScope) -> Option<String> {
    for attr in &element.attributes {
        if matches!(attr.name.namespace, AttributeNamespace::Bare) && attr.name.local == "component"
        {
            let s = resolved_attribute_string(&attr.value, scope)?;
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Build the [`BuilderNode`] for a `<dispatch>` element. The
/// `component` attribute is swallowed (consumed for routing); every
/// other attribute flows through the same lane as a normal element so
/// `id="…"`, `props="{…}"` spread, and bare attrs all work. The
/// resulting node carries the *resolved* component id, not "dispatch".
fn dispatch_element_to_builder_node(
    element: &Element,
    target: &str,
    scope: &LowerScope,
) -> BuilderNode {
    let mut id = String::new();
    // Seed props with the live binding emission for the target tag
    // (when one exists). This mirrors `LowerCtx::lower_as` so a
    // `<dispatch component="shell.component-palette"/>` from a DSL
    // block picks up the same `items` / `selected-id` the static
    // `<shell.component-palette/>` form would. Author-supplied
    // attrs on the `<dispatch/>` element win on key collision
    // (same precedence rule `lower_as` uses).
    let mut props: Map<String, Value> = match scope.tag_emission_for(target) {
        Some(emission) => match &emission.props {
            Value::Object(map) => map.clone(),
            _ => Map::new(),
        },
        None => Map::new(),
    };
    for attr in &element.attributes {
        let local = attr.name.local.as_str();
        match attr.name.namespace {
            AttributeNamespace::Identifier if local == "id" => {
                id = resolved_attribute_string(&attr.value, scope).unwrap_or_default();
            }
            // Swallow the `component` routing attr; everything else
            // follows the same per-namespace mapping as
            // `element_to_builder_node`.
            AttributeNamespace::Bare if local == "component" => {}
            // Wave 12 — `style="{obj}"` is consumed post-lower by
            // `attach_style_overrides`, not as a prop. Swallow.
            AttributeNamespace::Bare if local == "style" => {}
            AttributeNamespace::Bare if local == "props" => {
                if let Value::Object(map) = resolved_attribute_value(&attr.value, scope) {
                    for (k, v) in map {
                        props.insert(k, v);
                    }
                }
            }
            AttributeNamespace::Bare => {
                props.insert(
                    local.to_string(),
                    resolved_attribute_value(&attr.value, scope),
                );
            }
            AttributeNamespace::Data => {
                props.insert(
                    local.to_string(),
                    resolved_attribute_value(&attr.value, scope),
                );
            }
            AttributeNamespace::Aria => {
                props.insert(
                    format!("aria-{local}"),
                    resolved_attribute_value(&attr.value, scope),
                );
            }
            // Wave 10.4 / Wave 13.4 two-way `bind:value="<src>"` —
            // forward bind-namespaced attrs through dispatched
            // component tags as `props["bind-<key>"]` so the resolved
            // block reads them via the same `ctx.prop_str` path it
            // uses for any other prop. `<prism.text-input bind:value="form.email"/>`
            // lands as `props["bind-value"] = "form.email"`; the
            // text-input lower body then forwards it as a
            // `data-bind-value` semantic attr on the emitted input
            // so the shell's `route_bind_input_focus` opens a
            // field-focus session and keystrokes write back through
            // `set_node_prop`. Closes the deferred Wave 13.4 /
            // 14.13 gap (two-way `v-model`-style binding).
            AttributeNamespace::Bind => {
                props.insert(
                    format!("bind-{local}"),
                    resolved_attribute_value(&attr.value, scope),
                );
            }
            _ => {}
        }
    }
    BuilderNode {
        id,
        component: target.into(),
        props: Value::Object(props),
        children: Vec::new(),
        layout_mode: LayoutMode::default(),
        transform: Transform2D::default(),
        modifiers: Vec::new(),
        style: StyleProperties::default(),
    }
}

/// Coerce a captured attribute string into the JSON value the block's
/// schema expects. Numbers and bools auto-coerce; strings whose first
/// non-whitespace character is `[` or `{` are parsed as JSON so blocks
/// can read array / object props directly via `.as_array()` /
/// `.as_object()` regardless of whether they arrived from authored
/// source (`tabs="[…]"`) or from a binding emission (which serialises
/// JSON arrays / objects through the same string carrier). Everything
/// else stays a string. Empty (`<el disabled>`) becomes `Bool(true)`,
/// matching HTML's "boolean attribute" convention.
fn value_for(raw: Option<String>) -> Value {
    let Some(s) = raw else {
        return Value::Bool(true);
    };
    if let Ok(b) = s.parse::<bool>() {
        return Value::Bool(b);
    }
    if let Ok(n) = s.parse::<i64>() {
        return Value::from(n);
    }
    if let Ok(f) = s.parse::<f64>() {
        return Value::from(f);
    }
    if matches!(s.trim_start().chars().next(), Some('[') | Some('{')) {
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            return v;
        }
    }
    Value::String(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{register_block, Block};
    use crate::registry::FieldSpec;
    use prism_core::language::prism_ui::parse;
    use prism_ui_runtime::interpret::lower_document_with_scope;

    /// Minimal block: `<demo.box .../>` lowers to a fixed-size
    /// container whose colour comes from the `tint` prop. Stand-in
    /// for a real chrome block; exercises the resolver end-to-end
    /// without dragging the shell crate in.
    struct DemoBox {
        id: crate::ComponentId,
    }
    impl Default for DemoBox {
        fn default() -> Self {
            Self {
                id: "demo.box".into(),
            }
        }
    }
    impl Block for DemoBox {
        fn id(&self) -> &crate::ComponentId {
            &self.id
        }
        fn schema(&self) -> Vec<FieldSpec> {
            vec![FieldSpec::text("tint", "Tint")]
        }
        fn lower_ui(
            &self,
            _ctx: &LowerCtx<'_>,
            node: &BuilderNode,
            _style: &StyleProperties,
        ) -> UiNode {
            use prism_ui_runtime::layout::{ContainerProps, Sizing};
            let bg = node
                .props
                .get("tint")
                .and_then(|v| v.as_str())
                .and_then(crate::ui_lower::parse_color);
            UiNode::Container {
                id: node.id.clone(),
                props: ContainerProps {
                    width: Sizing::Fixed(40.0),
                    height: Sizing::Fixed(40.0),
                    background: bg,
                    ..Default::default()
                },
                children: vec![],
            }
        }
    }

    fn registry_with_demo() -> Arc<ComponentRegistry> {
        let mut reg = ComponentRegistry::new();
        register_block(&mut reg, Arc::new(DemoBox::default())).unwrap();
        Arc::new(reg)
    }

    #[test]
    fn registered_tag_lowers_through_block() {
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let (doc, errs) = parse(r##"<container><demo.box id="b" tint="#ff0000"/></container>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        let UiNode::Container { id, props, .. } = &children[0] else {
            panic!("resolver did not produce a container")
        };
        assert_eq!(id, "b");
        let bg = props.background.expect("tint propagated as background");
        assert_eq!((bg.r, bg.g, bg.b), (0xff, 0x00, 0x00));
    }

    #[test]
    fn unregistered_tag_falls_through_to_default() {
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let (doc, _) = parse(r#"<scene><text>kept</text></scene>"#);
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        // Unknown tag drops the wrapper, keeps children.
        assert_eq!(nodes.len(), 1);
        assert!(matches!(nodes[0], UiNode::Text { .. }));
    }

    #[test]
    fn boolean_attribute_coerces_to_true() {
        let (doc, _) = parse(r#"<demo.box disabled/>"#);
        let n = doc
            .nodes
            .first()
            .and_then(|n| match n {
                prism_core::language::prism_ui::Node::Element(e) => Some(e),
                _ => None,
            })
            .unwrap();
        let bn = element_to_builder_node(n, &LowerScope::default());
        assert_eq!(bn.props["disabled"], Value::Bool(true));
    }

    #[test]
    fn numeric_attribute_coerces_to_number() {
        let (doc, _) = parse(r#"<demo.box count="3" ratio="0.5"/>"#);
        let n = match &doc.nodes[0] {
            prism_core::language::prism_ui::Node::Element(e) => e,
            _ => panic!(),
        };
        let bn = element_to_builder_node(n, &LowerScope::default());
        assert_eq!(bn.props["count"], Value::from(3i64));
        assert_eq!(bn.props["ratio"], Value::from(0.5));
    }

    /// Composition-style block that *consumes* `ctx.host_children()`.
    /// Mirrors the `shell.app-window` shape without dragging the shell
    /// crate into the builder's test surface.
    struct DemoHost {
        id: crate::ComponentId,
    }
    impl Default for DemoHost {
        fn default() -> Self {
            Self {
                id: "demo.host".into(),
            }
        }
    }
    impl Block for DemoHost {
        fn id(&self) -> &crate::ComponentId {
            &self.id
        }
        fn schema(&self) -> Vec<FieldSpec> {
            vec![]
        }
        fn lower_ui(
            &self,
            ctx: &LowerCtx<'_>,
            node: &BuilderNode,
            _style: &StyleProperties,
        ) -> UiNode {
            use prism_ui_runtime::layout::ContainerProps;
            let kids = ctx
                .host_children()
                .map(|s| s.to_vec())
                .unwrap_or_else(|| ctx.lower_children(&node.children));
            UiNode::Container {
                id: node.id.clone(),
                props: ContainerProps::default(),
                children: kids,
            }
        }
    }

    #[test]
    fn resolver_pre_lowers_ast_children_into_host_children_slot() {
        // Composition block reached from source — the inner `<text>` is
        // pre-lowered through the runtime by the resolver, then handed
        // to `DemoHost::lower_ui` via `ctx.host_children()`. No
        // `<slot/>` declaration needed; the block opts in by reading
        // the LowerCtx slot.
        let mut reg = ComponentRegistry::new();
        register_block(&mut reg, Arc::new(DemoHost::default())).unwrap();
        let resolver = Arc::new(RegistryTagResolver::new(Arc::new(reg)));
        let (doc, errs) = parse(r#"<demo.host id="h"><text>hi</text></demo.host>"#);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { id, children, .. } = &nodes[0] else {
            panic!("expected DemoHost container, got {:?}", nodes[0])
        };
        assert_eq!(id, "h");
        assert_eq!(children.len(), 1);
        assert!(matches!(children[0], UiNode::Text { .. }));
    }

    #[test]
    fn resolver_host_children_is_empty_when_source_has_none() {
        // Self-closing tag → empty AST children list → `host_children`
        // returns `Some(&[])`. Block sees an empty pre-lowered slice
        // (still wins over the builder-Node walk, but produces zero
        // children).
        let mut reg = ComponentRegistry::new();
        register_block(&mut reg, Arc::new(DemoHost::default())).unwrap();
        let resolver = Arc::new(RegistryTagResolver::new(Arc::new(reg)));
        let (doc, _) = parse(r#"<demo.host id="h"/>"#);
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert!(children.is_empty());
    }

    #[test]
    fn resolver_host_children_does_not_propagate_to_recursive_lower() {
        // Nested resolver dispatch: the outer block reads
        // `host_children`, but its lowering MUST NOT pollute the inner
        // block's context with the same slot — `host_children` belongs
        // to one block only, set by its resolver call. Verifies the
        // `lower()` recursion intentionally drops it.
        let mut reg = ComponentRegistry::new();
        register_block(&mut reg, Arc::new(DemoHost::default())).unwrap();
        register_block(&mut reg, Arc::new(DemoBox::default())).unwrap();
        let resolver = Arc::new(RegistryTagResolver::new(Arc::new(reg)));
        let (doc, _) = parse(
            r#"<demo.host id="h"><demo.host id="inner"><text>x</text></demo.host></demo.host>"#,
        );
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        // Outer host adopts the inner host-as-pre-lowered-child;
        // that inner host in turn adopted its own pre-lowered text.
        assert_eq!(children.len(), 1);
        let UiNode::Container {
            id: inner_id,
            children: inner_kids,
            ..
        } = &children[0]
        else {
            panic!()
        };
        assert_eq!(inner_id, "inner");
        assert_eq!(inner_kids.len(), 1);
        assert!(matches!(inner_kids[0], UiNode::Text { .. }));
    }

    #[test]
    fn resolver_prefers_scope_injected_children_over_ast() {
        // §43 B2: when the host injects a tag-keyed pre-lowered children
        // slice via `LowerScope::with_host_children_by_tag`, those
        // children win over any AST children the resolver would
        // otherwise pre-lower. This is the seam binding-driven
        // composition uses (canvas hosting a `BuilderDocument`).
        let mut reg = ComponentRegistry::new();
        register_block(&mut reg, Arc::new(DemoHost::default())).unwrap();
        let resolver = Arc::new(RegistryTagResolver::new(Arc::new(reg)));
        // Source authors an `<text>ast</text>` child — but the host
        // injects an alternate text node for this tag, which must
        // override.
        let (doc, _) = parse(r#"<demo.host id="h"><text>ast</text></demo.host>"#);
        let mut map: std::collections::HashMap<String, Vec<UiNode>> =
            std::collections::HashMap::new();
        map.insert(
            "demo.host".into(),
            vec![UiNode::Text {
                id: "from-host".into(),
                content: "injected".into(),
                props: prism_ui_runtime::layout::TextProps::default(),
            }],
        );
        let scope = LowerScope::default()
            .with_resolver(resolver)
            .with_host_children_by_tag(map);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        let UiNode::Text { content, .. } = &children[0] else {
            panic!("expected the host-injected text node")
        };
        assert_eq!(
            content, "injected",
            "host-injected children must override AST pre-lowering"
        );
    }

    #[test]
    fn on_click_attribute_attaches_data_on_click_to_lowered_container() {
        // §43 A1: `on:click="emit save"` on a registered tag rides
        // through to the lowered container as `data-on-click`. The
        // shell event router reads this attr at pointer-down time
        // and dispatches through `signal::parse_action`.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let (doc, errs) =
            parse(r##"<demo.box id="b" on:click="emit save" on:hover="cmd help.show"/>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!("DemoBox should lower to a container")
        };
        let attrs: std::collections::HashMap<_, _> = props
            .semantic
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            attrs.get("data-on-click").map(String::as_str),
            Some("emit save")
        );
        assert_eq!(
            attrs.get("data-on-hover").map(String::as_str),
            Some("cmd help.show")
        );
    }

    #[test]
    fn on_attributes_are_skipped_when_their_value_is_empty() {
        // Defensive: an `on:click` with no value should not produce
        // a `data-on-click` attr — handlers without an action body
        // are meaningless. Matches the lowering rule in the runtime's
        // `apply_container_attributes` (no `raw` → skip).
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let (doc, _) = parse(r##"<demo.box id="b" on:click/>"##);
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(!props
            .semantic
            .attrs
            .iter()
            .any(|(k, _)| k == "data-on-click"));
    }

    #[test]
    fn aria_attribute_lands_with_aria_prefix() {
        let (doc, _) = parse(r#"<demo.box aria:label="Close"/>"#);
        let n = match &doc.nodes[0] {
            prism_core::language::prism_ui::Node::Element(e) => e,
            _ => panic!(),
        };
        let bn = element_to_builder_node(n, &LowerScope::default());
        assert_eq!(bn.props["aria-label"], Value::String("Close".into()));
    }

    #[test]
    fn interpolated_attribute_resolves_from_scope_as_typed_json() {
        // Pure `{expr}` attrs return the underlying JSON value verbatim,
        // so a `for="item in items"` loop over Vec<Object> can spread
        // typed fields onto a dispatched block. Templates with literals
        // resolve to strings (the old behaviour, but interpolation-aware).
        let scope = LowerScope::default()
            .with_binding("count", Value::from(42i64))
            .with_binding("label", Value::String("Hello".into()))
            .with_binding("row", serde_json::json!({ "name": "Beta", "depth": 1 }));
        let (doc, _) = parse(
            r#"<demo.box count="{count}" label="{label}" name="{row.name}" depth="{row.depth}" prefixed="d={row.depth}"/>"#,
        );
        let n = match &doc.nodes[0] {
            prism_core::language::prism_ui::Node::Element(e) => e,
            _ => panic!(),
        };
        let bn = element_to_builder_node(n, &scope);
        assert_eq!(bn.props["count"], Value::from(42i64));
        assert_eq!(bn.props["label"], Value::String("Hello".into()));
        assert_eq!(bn.props["name"], Value::String("Beta".into()));
        assert_eq!(bn.props["depth"], Value::from(1i64));
        assert_eq!(bn.props["prefixed"], Value::String("d=1".into()));
    }

    #[test]
    fn props_spread_attribute_unpacks_object_into_node_props() {
        // `<el props="{item}"/>` spreads a JSON object onto the dispatched
        // node. Wave 11.2 enabler for list-binding migrations
        // (shell.nav-page-list, shell.explorer, shell.signals-panel)
        // that today rely on `ctx.lower_as(tag, id, item.clone())` to
        // forward whole-row props.
        let scope = LowerScope::default().with_binding(
            "row",
            serde_json::json!({ "page-title": "Home", "route": "/", "selected": true }),
        );
        let (doc, _) = parse(r#"<demo.box props="{row}"/>"#);
        let n = match &doc.nodes[0] {
            prism_core::language::prism_ui::Node::Element(e) => e,
            _ => panic!(),
        };
        let bn = element_to_builder_node(n, &scope);
        assert_eq!(bn.props["page-title"], Value::String("Home".into()));
        assert_eq!(bn.props["route"], Value::String("/".into()));
        assert_eq!(bn.props["selected"], Value::Bool(true));
    }

    #[test]
    fn props_spread_ignores_non_object_values() {
        let scope = LowerScope::default().with_binding("v", Value::from(7i64));
        let (doc, _) = parse(r#"<demo.box props="{v}" label="kept"/>"#);
        let n = match &doc.nodes[0] {
            prism_core::language::prism_ui::Node::Element(e) => e,
            _ => panic!(),
        };
        let bn = element_to_builder_node(n, &scope);
        // The spread of a non-object is a no-op; the sibling `label`
        // attribute still lands.
        assert!(bn.props.get("v").is_none());
        assert_eq!(bn.props["label"], Value::String("kept".into()));
    }

    #[test]
    fn dispatch_element_dynamically_routes_to_resolved_component() {
        // Wave 11.2 substrate: `<dispatch component="{row.component}"
        // props="{row.props}"/>` looks up the target tag at render time
        // and dispatches as if the source had named it directly.
        // Closes the long-standing properties-panel migration block
        // (rows-with-component-field).
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default().with_resolver(resolver).with_binding(
            "row",
            serde_json::json!({
                "component": "demo.box",
                "props": { "tint": "#ff0000" },
            }),
        );
        let (doc, errs) = parse(r#"<dispatch component="{row.component}" props="{row.props}"/>"#);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        assert_eq!(nodes.len(), 1, "exactly one node from the dispatch");
        // DemoBox lowers to a 40×40 container — confirm we hit it, not
        // some `dispatch` fallback.
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!("dispatch should produce demo.box's container")
        };
        assert_eq!(
            props.width,
            prism_ui_runtime::layout::Sizing::Fixed(40.0),
            "demo.box's 40×40 shape must surface — the dispatch routed correctly"
        );
        // Tint from `row.props.tint` flowed through the `props=` spread.
        let bg = props.background.expect("dispatch should pass tint through");
        assert_eq!(bg.r, 0xff);
    }

    #[test]
    fn dispatch_tag_routes_to_registered_block() {
        // Symmetric form of the `component=` dispatch: `<dispatch
        // tag="demo.box"/>` resolves the tag attribute at the runtime
        // layer (rewrite-to-synthetic-element), which then falls
        // through to the resolver — which sees a `<demo.box/>`-shaped
        // element and dispatches normally.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default().with_resolver(resolver);
        let (doc, errs) = parse(r##"<dispatch tag="demo.box" id="b" tint="#00ff00"/>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        assert_eq!(nodes.len(), 1, "exactly one node");
        let UiNode::Container { id, props, .. } = &nodes[0] else {
            panic!("expected demo.box container, got {:?}", nodes[0])
        };
        assert_eq!(id, "b");
        let bg = props.background.expect("tint propagated through dispatch");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0xff, 0x00));
    }

    #[test]
    fn dispatch_tag_resolves_from_scope_and_routes_to_block() {
        // The interesting case: `tag="{kind}"` resolves through scope
        // before being rewritten. Closes the §15 PRUI-ref gap for
        // data-driven registered-tag dispatch.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default()
            .with_resolver(resolver)
            .with_binding("kind", serde_json::json!("demo.box"));
        let (doc, errs) = parse(r##"<dispatch tag="{kind}" id="b" tint="#0000ff"/>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { id, props, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(id, "b");
        let bg = props.background.expect("tint must flow through dispatch");
        assert_eq!(bg.b, 0xff);
    }

    #[test]
    fn dispatch_tag_to_primitive_short_circuits_resolver() {
        // `tag="container"` rewrites to a runtime-primitive
        // `<container/>` and never hits the resolver. The DemoBox
        // 40×40 sizing must NOT surface — confirming we routed to
        // the primitive arm, not a registered-component fallback.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default().with_resolver(resolver);
        let (doc, errs) =
            parse(r##"<dispatch tag="container" id="root" gap="4" style:background="#112233"/>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { id, props, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(id, "root");
        assert!((props.gap - 4.0).abs() < f32::EPSILON);
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b), (0x11, 0x22, 0x33));
        // No fixed 40×40 sizing — that's DemoBox's signature, which
        // must be absent when routing to a primitive.
        assert!(!matches!(
            props.width,
            prism_ui_runtime::layout::Sizing::Fixed(40.0)
        ));
    }

    #[test]
    fn dispatch_tag_takes_precedence_over_component_attr() {
        // If both `tag=` and `component=` are present, the runtime's
        // rewrite happens first — the synthesised element no longer
        // sees `component=` as routing because it's no longer a
        // `<dispatch>` tag. Pin the precedence so authoring stays
        // unambiguous.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default().with_resolver(resolver);
        let (doc, errs) = parse(
            r##"<dispatch tag="container" component="demo.box" id="root" style:background="#abcdef"/>"##,
        );
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { id, props, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(id, "root");
        let bg = props.background.expect("bg");
        assert_eq!((bg.r, bg.g, bg.b), (0xab, 0xcd, 0xef));
    }

    #[test]
    fn dispatch_with_unresolved_component_attr_returns_none() {
        // Missing `component` attr means the dispatch can't find a
        // target — the resolver returns None, leaving the unknown-tag
        // fallback to surface the wrapper's children.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default().with_resolver(resolver);
        let (doc, _) = parse(r#"<dispatch/>"#);
        let nodes = lower_document_with_scope(&doc, &scope);
        assert!(
            nodes.is_empty(),
            "dispatch with no component attr must not produce a node, got {nodes:?}"
        );
    }

    #[test]
    fn interpolated_attribute_with_missing_binding_returns_null() {
        let (doc, _) = parse(r#"<demo.box value="{missing}"/>"#);
        let n = match &doc.nodes[0] {
            prism_core::language::prism_ui::Node::Element(e) => e,
            _ => panic!(),
        };
        let bn = element_to_builder_node(n, &LowerScope::default());
        assert_eq!(bn.props["value"], Value::Null);
    }

    #[test]
    fn style_namespace_overrides_lowered_container_background() {
        // Wave 12 — Vue/React-style style prop passing.
        // `style:background="#…"` on the source element wins over the
        // block's natural background after `lower_ui` runs.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        // DemoBox's natural background comes from its `tint` prop.
        let (doc, errs) =
            parse(r##"<demo.box id="b" tint="#ff0000" style:background="#00ff00"/>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("override should set bg");
        assert_eq!(
            (bg.r, bg.g, bg.b),
            (0x00, 0xff, 0x00),
            "style:background must win over the block's tint-derived bg",
        );
    }

    #[test]
    fn style_spread_object_unpacks_each_key_as_override() {
        // `style="{obj}"` spread parallels `props="{item}"`.
        // Each key in the resolved object becomes a style override
        // applied post-lower, exactly like an authored
        // `style:k="v"`.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default().with_resolver(resolver).with_binding(
            "theme",
            serde_json::json!({ "background": "#0000ff", "radius": 8 }),
        );
        let (doc, errs) = parse(r##"<demo.box id="b" tint="#ff0000" style="{theme}"/>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("spread should set bg");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x00, 0xff));
        // CornerRadius is uniform → all four corners equal.
        assert_eq!(props.radius.tl, 8.0);
        assert_eq!(props.radius.tr, 8.0);
        assert_eq!(props.radius.br, 8.0);
        assert_eq!(props.radius.bl, 8.0);
    }

    #[test]
    fn style_namespace_overrides_with_state_suffix_route_to_hover() {
        // `style:background:hovered="#…"` lands on
        // `ContainerProps.hover.background` — same vocabulary the
        // runtime's `apply_container_attributes` uses, lifted through
        // the resolver seam.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let (doc, errs) = parse(r##"<demo.box id="b" style:background:hovered="#102030"/>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let hover_bg = props
            .hover
            .as_ref()
            .and_then(|h| h.background)
            .expect("hover background should be set");
        assert_eq!(
            (hover_bg.r, hover_bg.g, hover_bg.b),
            (0x10, 0x20, 0x30),
            "style:background:hovered must populate the hover override",
        );
    }

    #[test]
    fn style_spread_value_is_not_visible_as_block_prop() {
        // The `style="{obj}"` spread is consumed by the post-lower
        // override pipeline, not as a block prop. Blocks that read
        // `node.props["style"]` would see the raw JSON if we didn't
        // swallow it; pin the contract so a future refactor doesn't
        // accidentally re-expose the key.
        let scope = LowerScope::default()
            .with_binding("theme", serde_json::json!({ "background": "#0000ff" }));
        let (doc, _) = parse(r#"<demo.box style="{theme}" label="kept"/>"#);
        let n = match &doc.nodes[0] {
            prism_core::language::prism_ui::Node::Element(e) => e,
            _ => panic!(),
        };
        let bn = element_to_builder_node(n, &scope);
        assert!(
            bn.props.get("style").is_none(),
            "style spread must not appear in node.props",
        );
        assert_eq!(bn.props["label"], Value::String("kept".into()));
    }

    #[test]
    fn style_overrides_unknown_bare_key_drops_silently() {
        // Unknown bare-key style attrs (no `:state` suffix) drop
        // silently — mirroring the pre-Wave-12 behavior of
        // `apply_container_attributes`. Authors who want arbitrary
        // `data-*` payloads use the `data:` namespace. Future
        // expansions of the known-key vocabulary in
        // `apply_style_override` light the key up uniformly across
        // every consumer (direct authoring + resolver pass-through).
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let (doc, _) = parse(r##"<demo.box style:tint="#beadee"/>"##);
        let scope = LowerScope::default().with_resolver(resolver);
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(
            !props
                .semantic
                .attrs
                .iter()
                .any(|(k, _)| k.starts_with("data-style-tint")),
            "unknown bare-key style attr must drop silently; got attrs {:?}",
            props.semantic.attrs,
        );
    }

    #[test]
    fn style_overrides_apply_through_dynamic_dispatch() {
        // `<dispatch component="{…}" style:background="#…"/>` — style
        // overrides must apply to the resolved target's lowered
        // container just like a directly-named tag would.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default().with_resolver(resolver).with_binding(
            "row",
            serde_json::json!({ "component": "demo.box", "props": { "tint": "#ff0000" } }),
        );
        let (doc, errs) = parse(
            r##"<dispatch component="{row.component}" props="{row.props}" style:background="#00ff00"/>"##,
        );
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("override should set bg");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0xff, 0x00));
    }

    // ---------- Functional-helper calls round-trip through the
    // resolver's attribute paths (props=, attribute interpolation,
    // typed-attribute spread). The runtime's `lookup_path_owned` is
    // the shared seam, so a `props="{find(rows, 'id', target).props}"`
    // shape should pass the typed object through to the dispatched
    // block. ----------

    #[test]
    fn functional_call_in_dispatch_props_spread() {
        // `<dispatch component="demo.box" props="{find(rows, 'id',
        // active).props}"/>` — pull the row whose `id` matches a
        // selection cursor, then spread its `props` onto the
        // dispatched block. Validates that the resolver's
        // `resolved_attribute_value` consumes a call result through
        // `lookup_path_owned_in_scope`.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default()
            .with_resolver(resolver)
            .with_binding(
                "rows",
                serde_json::json!([
                    {"id": 1, "props": {"tint": "#aa0000"}},
                    {"id": 2, "props": {"tint": "#00aa00"}},
                ]),
            )
            .with_binding("active", serde_json::json!(2));
        let (doc, errs) =
            parse(r##"<dispatch component="demo.box" props="{find(rows, 'id', active).props}"/>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("tint flowed through");
        assert_eq!((bg.r, bg.g, bg.b), (0x00, 0xaa, 0x00));
    }

    #[test]
    fn functional_call_in_attribute_interpolation_lowers_to_string() {
        // `<demo.box tint="{first(map(rows, 'tint'))}"/>` — composed
        // call (`first` over a `map` projection). The resolver's
        // attribute path stringifies the result and the block reads
        // it as a normal prop.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default().with_resolver(resolver).with_binding(
            "rows",
            serde_json::json!([
                {"tint": "#3366ff"},
                {"tint": "#ff9933"},
            ]),
        );
        // `map(rows, 'tint').first` reads through the virtual segment
        // chain → string "#3366ff" → tint prop on the block.
        let (doc, errs) = parse(r##"<demo.box tint="{map(rows, 'tint').first}"/>"##);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        let bg = props.background.expect("tint resolved");
        assert_eq!((bg.r, bg.g, bg.b), (0x33, 0x66, 0xff));
    }

    #[test]
    fn for_loop_with_filter_call_drives_repeated_dispatch() {
        // The headline case: a `for=` whose source is a filtered
        // array, dispatching one block per matching row. Round-trips
        // through the resolver + runtime + functional helpers in
        // one flow.
        let resolver = Arc::new(RegistryTagResolver::new(registry_with_demo()));
        let scope = LowerScope::default().with_resolver(resolver).with_binding(
            "rows",
            serde_json::json!([
                {"status": "active",   "tint": "#aa0000"},
                {"status": "archived", "tint": "#666666"},
                {"status": "active",   "tint": "#00aa00"},
                {"status": "active",   "tint": "#0000aa"},
            ]),
        );
        let (doc, errs) = parse(
            r##"<container>
                <demo.box for="row in filter(rows, 'status', 'active')" tint="{row.tint}"/>
               </container>"##,
        );
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(children.len(), 3, "three active rows survived the filter");
        // Each child is a demo.box-shaped container with its tint
        // background applied.
        let tints: Vec<(u8, u8, u8)> = children
            .iter()
            .filter_map(|c| match c {
                UiNode::Container { props, .. } => props.background.map(|c| (c.r, c.g, c.b)),
                _ => None,
            })
            .collect();
        assert_eq!(
            tints,
            vec![(0xaa, 0x00, 0x00), (0x00, 0xaa, 0x00), (0x00, 0x00, 0xaa)],
        );
    }
}
