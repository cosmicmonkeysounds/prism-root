//! `FacetComponent` — the data-repeat block.
//!
//! A facet lowers its **child subtree** (the template) once per item in
//! its data source, interpolating `{{field}}` expressions against each
//! item. The template is ordinary `node.children` and the data source is
//! read from `node.props`, so the whole thing is reachable from the
//! standard `Component::lower_ui` contract — no `BuilderDocument`
//! side-table. With no data (design-time / unbound) the template renders
//! once, as-is, so the canvas shows and edits it like any other node.

use prism_core::help::HelpEntry;
use serde_json::Value;

use crate::component::{Component, ComponentId};
use crate::document::Node;
use crate::facet::resolve_template_expressions;
use crate::registry::FieldSpec;
use crate::signal::{common_signals, SignalDef};
use crate::style::StyleProperties;
use crate::ui_lower::LowerCtx;

use prism_ui_runtime::layout::Node as UiNode;

pub struct FacetComponent {
    pub id: ComponentId,
}

impl FacetComponent {
    pub fn new() -> Self {
        Self { id: "facet".into() }
    }
}

impl Default for FacetComponent {
    fn default() -> Self {
        Self::new()
    }
}

/// Make every id in a cloned template instance unique to its item so the
/// canvas / hit-test / reactive cache never see duplicate ids.
fn prefix_ids(node: &mut Node, prefix: &str) {
    node.id = format!("{prefix}/{}", node.id);
    for child in &mut node.children {
        prefix_ids(child, prefix);
    }
}

impl Component for FacetComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        crate::schemas::facet()
    }

    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.facet",
            "Facet",
            "Repeats its child template once per item in its data source.",
        ))
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
        let items: Vec<Value> = match ctx.prop(node, "items") {
            Value::Array(a) => a,
            _ => Vec::new(),
        };

        // No bound data: render the template subtree once, as-is. This is
        // the design-time shape — the children are real nodes the normal
        // inspector / property / click paths already handle.
        if items.is_empty() {
            return ctx.default_container(node, style);
        }

        let take = match ctx.prop(node, "max_items") {
            Value::Number(n) => n.as_u64().map(|v| v as usize),
            _ => None,
        }
        .unwrap_or(items.len())
        .min(items.len());

        let mut children = Vec::new();
        for (idx, item) in items.iter().take(take).enumerate() {
            for tmpl in &node.children {
                let mut inst = tmpl.clone();
                prefix_ids(&mut inst, &format!("{}::{idx}", node.id));
                resolve_template_expressions(&mut inst, item);
                children.push(ctx.lower(&inst));
            }
        }
        ctx.synthetic_container(node, style, children, |_| {})
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ComponentRegistry;
    use crate::starter::register_builtins;
    use serde_json::json;

    fn text_child(id: &str, body: &str) -> Node {
        Node {
            id: id.into(),
            component: "text".into(),
            props: json!({ "body": body }),
            ..Default::default()
        }
    }

    fn facet_node(props: Value, children: Vec<Node>) -> Node {
        Node {
            id: "f1".into(),
            component: "facet".into(),
            props,
            children,
            ..Default::default()
        }
    }

    fn child_texts(n: &UiNode) -> Vec<String> {
        let UiNode::Container { children, .. } = n else {
            panic!("expected container");
        };
        // `text_lower` wraps every Text leaf in a selectable container,
        // so each facet child is now a Container holding the Text leaf.
        children
            .iter()
            .map(|c| match c {
                UiNode::Text { content, .. } => content.clone(),
                UiNode::Container {
                    children: inner, ..
                } => match inner.first() {
                    Some(UiNode::Text { content, .. }) => content.clone(),
                    Some(other) => format!("{other:?}"),
                    None => String::new(),
                },
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn repeats_template_once_per_item_with_interpolation() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).expect("register builtins");
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade);

        let node = facet_node(
            json!({ "items": [ { "name": "Alpha" }, { "name": "Beta" } ] }),
            vec![text_child("t", "{{name}}")],
        );
        let out = FacetComponent::new().lower_ui(&ctx, &node, &cascade);
        assert_eq!(child_texts(&out), vec!["Alpha", "Beta"]);
    }

    #[test]
    fn no_data_renders_template_once_as_is() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).expect("register builtins");
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade);

        let node = facet_node(json!({}), vec![text_child("t", "static")]);
        let out = FacetComponent::new().lower_ui(&ctx, &node, &cascade);
        // Falls back to default_container over the real children.
        assert_eq!(child_texts(&out), vec!["static"]);
    }

    #[test]
    fn max_items_caps_the_repeat() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).expect("register builtins");
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade);

        let node = facet_node(
            json!({
                "items": [ { "name": "a" }, { "name": "b" }, { "name": "c" } ],
                "max_items": 2
            }),
            vec![text_child("t", "{{name}}")],
        );
        let out = FacetComponent::new().lower_ui(&ctx, &node, &cascade);
        assert_eq!(child_texts(&out), vec!["a", "b"]);
    }
}
