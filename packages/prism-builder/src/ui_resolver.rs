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

use std::sync::Arc;

use prism_core::language::prism_ui::{AttributeNamespace, AttributeValue, Element};
use prism_ui_runtime::interpret::{lower_ast_children, LowerScope, TagResolver};
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
        let component = self.registry.get(&element.tag)?;
        let node = element_to_builder_node(element);
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
        let pre_lowered: Vec<UiNode> = if let Some(injected) = host_supplied {
            injected
        } else if element.children.is_empty() {
            Vec::new()
        } else {
            lower_ast_children(&element.children, scope)
        };
        // Thread the scope's tag-keyed emission snapshot into LowerCtx
        // so any `lower_as` call inside `component.lower_ui` (the
        // dock-panel routing path is the canonical caller) picks up
        // the same per-tag binding emission this resolver call sees
        // for host_children. Without this thread-through, routed
        // content tags (`shell.builder-canvas`, `shell.component-palette`,
        // `shell.properties-panel`) get empty props / zero children.
        let ctx = LowerCtx::new(Some(&self.registry), &cascade)
            .with_host_children(&pre_lowered)
            .with_tag_emissions(scope.tag_emissions_arc());
        let mut lowered = component.lower_ui(&ctx, &node, &cascade);
        // §43 A1: any `on:<event>="<action>"` attribute on the source
        // element rides through to the lowered container as a
        // `data-on-<event>` semantic attr. The `element_to_builder_node`
        // helper deliberately drops the `On` namespace (per the
        // attribute table in its docstring) because the block doesn't
        // need it during render — the shell event router reads it
        // back from the resulting `HitRect.attrs` instead.
        attach_on_handlers(&mut lowered, element);
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
fn attach_on_handlers(node: &mut UiNode, element: &Element) {
    let mut on_attrs: Vec<(String, String)> = Vec::new();
    for attr in &element.attributes {
        if !matches!(attr.name.namespace, AttributeNamespace::On) {
            continue;
        }
        let Some(value) = literal_attribute_value(&attr.value) else {
            continue;
        };
        on_attrs.push((format!("data-on-{}", attr.name.local), value));
    }
    if on_attrs.is_empty() {
        return;
    }
    if let UiNode::Container { props, .. } = node {
        props.semantic.attrs.extend(on_attrs);
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
fn element_to_builder_node(element: &Element) -> BuilderNode {
    let mut id = String::new();
    let mut props: Map<String, Value> = Map::new();
    for attr in &element.attributes {
        let local = attr.name.local.as_str();
        let raw = literal_attribute_value(&attr.value);
        match attr.name.namespace {
            AttributeNamespace::Identifier if local == "id" => {
                id = raw.unwrap_or_default();
            }
            AttributeNamespace::Bare => {
                props.insert(local.to_string(), value_for(raw));
            }
            AttributeNamespace::Data => {
                props.insert(local.to_string(), value_for(raw));
            }
            AttributeNamespace::Aria => {
                props.insert(format!("aria-{local}"), value_for(raw));
            }
            // Styling, signals, bindings, facets, control-flow keywords
            // are not block-prop carriers — the cascade handles styles,
            // signals/bindings flow through their own dispatch paths,
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

/// Pull a literal string out of an attribute value. Skips
/// interpolation segments — those would need scope-aware resolution,
/// and chrome props are typically literal strings/numbers in source.
/// Templates with embedded `{expr}` parts collapse to their literal
/// segments concatenated; downstream blocks that need full interpolation
/// can opt in once the expression evaluator lands.
fn literal_attribute_value(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::String { value, .. } => Some(value.clone()),
        AttributeValue::Empty => None,
        AttributeValue::Expression(expr) => Some(format!("{{{}}}", expr.body)),
        AttributeValue::Template { parts, .. } => {
            let mut out = String::new();
            for part in parts {
                if let prism_core::language::prism_ui::ast::TemplatePart::Literal {
                    value, ..
                } = part
                {
                    out.push_str(value);
                }
            }
            Some(out)
        }
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
        let bn = element_to_builder_node(n);
        assert_eq!(bn.props["disabled"], Value::Bool(true));
    }

    #[test]
    fn numeric_attribute_coerces_to_number() {
        let (doc, _) = parse(r#"<demo.box count="3" ratio="0.5"/>"#);
        let n = match &doc.nodes[0] {
            prism_core::language::prism_ui::Node::Element(e) => e,
            _ => panic!(),
        };
        let bn = element_to_builder_node(n);
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
        let bn = element_to_builder_node(n);
        assert_eq!(bn.props["aria-label"], Value::String("Close".into()));
    }
}
