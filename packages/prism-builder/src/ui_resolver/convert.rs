//! Element → builder-node conversion + dynamic dispatch — the
//! attribute-resolution half of the registry tag resolver, split out
//! of `ui_resolver/mod.rs` (Phase B.5). `mod.rs` owns the
//! `TagResolver` impl (tag dispatch); this module owns turning a
//! resolved `<tag …/>` element into the `BuilderNode` a block's
//! `Component::lower_ui` consumes, plus the `style:` / `on:` post-lower
//! annotation passes and the `<dispatch/>` dynamic-target path.

use std::collections::HashMap;

use prism_core::language::prism_ui::{
    ast::TemplatePart, AttributeNamespace, AttributeValue, Element, Node as AstNode,
};
use prism_ui_runtime::interpret::{
    apply_style_override, evaluate_expression, lookup_path_owned_in_scope, lower_ast_children,
    stringify_value_for_template, LowerScope,
};
use prism_ui_runtime::layout::Node as UiNode;
use serde_json::{Map, Value};

use crate::document::Node as BuilderNode;
use crate::layout::LayoutMode;
use crate::style::StyleProperties;
use prism_core::foundation::spatial::Transform2D;

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
pub(crate) fn partition_children_by_slot(
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

pub(crate) fn attach_on_handlers(node: &mut UiNode, element: &Element, scope: &LowerScope) {
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
pub(crate) fn attach_style_overrides(node: &mut UiNode, element: &Element, scope: &LowerScope) {
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
/// | `on:*` / `bind:*` / `sig:*` / control-flow | ignored (handled separately upstream) |
///
/// Boolean attributes (`<el disabled>`) become `Bool(true)`.
/// Strings stay strings; the block's schema does the typed coercion.
///
/// Interpolated attributes resolve through `scope`: a pure
/// `key="{expr}"` returns the underlying JSON Value verbatim (so a
/// `for="item in items"` over `Vec<Object>` can spread fields directly
/// onto the dispatched block via `prop="{item.field}"`), while a
/// templated `key="prefix-{expr}"` resolves to its expanded string.
pub(crate) fn element_to_builder_node(element: &Element, scope: &LowerScope) -> BuilderNode {
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
            // Styling, signals, control-flow keywords are not
            // block-prop carriers — the cascade handles styles,
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
pub(crate) fn is_bare_path_expression(body: &str) -> bool {
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
pub(crate) fn resolved_attribute_string(
    value: &AttributeValue,
    scope: &LowerScope,
) -> Option<String> {
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
pub(crate) fn resolved_attribute_value(value: &AttributeValue, scope: &LowerScope) -> Value {
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
pub(crate) fn dynamic_dispatch_target(element: &Element, scope: &LowerScope) -> Option<String> {
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
pub(crate) fn dispatch_element_to_builder_node(
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
pub(crate) fn value_for(raw: Option<String>) -> Value {
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
