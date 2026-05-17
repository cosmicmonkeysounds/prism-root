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

use prism_core::language::prism_ui::Element;
use prism_ui_runtime::interpret::{LowerScope, TagResolver};
use prism_ui_runtime::layout::Node as UiNode;

use crate::registry::ComponentRegistry;
use crate::style::StyleProperties;
use crate::ui_lower::LowerCtx;

mod convert;
use convert::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{register_block, Block};
    use crate::document::Node as BuilderNode;
    use crate::registry::FieldSpec;
    use prism_core::language::prism_ui::parse;
    use prism_ui_runtime::interpret::lower_document_with_scope;
    use serde_json::Value;

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
