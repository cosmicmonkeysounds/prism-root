//! Prefabs — user-authored compound components.
//!
//! A `PrefabDef` captures a node subtree as a reusable template.
//! `ExposedSlot`s pin inner node props as instance-editable fields.
//! `PrefabComponent` wraps a def and implements `Component`, making
//! prefab instances indistinguishable from built-in components in the
//! registry and render walker.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::component::{Component, ComponentId, RenderError, RenderSlintContext};
use crate::document::{Node, NodeId};
use crate::registry::FieldSpec;
use crate::signal::SignalDef;
use crate::slint_source::SlintEmitter;
use crate::variant::VariantAxis;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposedSlot {
    pub key: String,
    pub target_node: NodeId,
    pub target_prop: String,
    pub spec: FieldSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabDef {
    pub id: ComponentId,
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub root: Node,
    #[serde(default)]
    pub exposed: Vec<ExposedSlot>,
    #[serde(default)]
    pub variants: Vec<VariantAxis>,
    #[serde(default)]
    pub thumbnail: Option<String>,
}

pub struct PrefabComponent {
    pub def: PrefabDef,
}

impl PrefabComponent {
    pub fn new(def: PrefabDef) -> Self {
        Self { def }
    }
}

impl Component for PrefabComponent {
    fn id(&self) -> &ComponentId {
        &self.def.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        self.def
            .exposed
            .iter()
            .map(|slot| slot.spec.clone())
            .collect()
    }

    fn signals(&self) -> Vec<SignalDef> {
        vec![]
    }

    fn variants(&self) -> Vec<VariantAxis> {
        self.def.variants.clone()
    }

    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let mut root = self.def.root.clone();
        for slot in &self.def.exposed {
            if let Some(val) = props.get(&slot.key) {
                apply_prop_to_node(&mut root, &slot.target_node, &slot.target_prop, val.clone());
            }
        }
        ctx.render_child(&root, out)
    }
}

fn apply_prop_to_node(node: &mut Node, target_id: &str, prop_key: &str, value: Value) {
    if node.id == target_id {
        if let Value::Object(ref mut map) = node.props {
            map.insert(prop_key.to_string(), value);
            return;
        }
        let mut map = serde_json::Map::new();
        map.insert(prop_key.to_string(), value);
        node.props = Value::Object(map);
        return;
    }
    for child in &mut node.children {
        apply_prop_to_node(child, target_id, prop_key, value.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hero_prefab() -> PrefabDef {
        PrefabDef {
            id: "prefab:hero".into(),
            label: "Hero Section".into(),
            description: "Full-width hero with heading and subtext.".into(),
            root: Node {
                id: "hero-root".into(),
                component: "container".into(),
                props: json!({ "spacing": 16 }),
                children: vec![
                    Node {
                        id: "hero-heading".into(),
                        component: "heading".into(),
                        props: json!({ "text": "Welcome", "level": 1 }),
                        children: vec![],
                        ..Default::default()
                    },
                    Node {
                        id: "hero-subtext".into(),
                        component: "text".into(),
                        props: json!({ "body": "Subtitle goes here" }),
                        children: vec![],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            exposed: vec![
                ExposedSlot {
                    key: "title".into(),
                    target_node: "hero-heading".into(),
                    target_prop: "text".into(),
                    spec: FieldSpec::text("title", "Hero Title").required(),
                },
                ExposedSlot {
                    key: "subtitle".into(),
                    target_node: "hero-subtext".into(),
                    target_prop: "body".into(),
                    spec: FieldSpec::text("subtitle", "Subtitle"),
                },
            ],
            variants: vec![],
            thumbnail: None,
        }
    }

    #[test]
    fn prefab_component_schema_from_exposed_slots() {
        let comp = PrefabComponent::new(hero_prefab());
        let schema = comp.schema();
        assert_eq!(schema.len(), 2);
        assert_eq!(schema[0].key, "title");
        assert!(schema[0].required);
        assert_eq!(schema[1].key, "subtitle");
    }

    #[test]
    fn prefab_component_id() {
        let comp = PrefabComponent::new(hero_prefab());
        assert_eq!(comp.id(), "prefab:hero");
    }

    #[test]
    fn apply_prop_to_node_updates_target() {
        let mut node = Node {
            id: "a".into(),
            component: "heading".into(),
            props: json!({ "text": "old" }),
            children: vec![],
            ..Default::default()
        };
        apply_prop_to_node(&mut node, "a", "text", json!("new"));
        assert_eq!(node.props["text"], "new");
    }

    #[test]
    fn apply_prop_to_node_finds_nested_target() {
        let mut node = Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![Node {
                id: "child".into(),
                component: "heading".into(),
                props: json!({ "text": "old" }),
                children: vec![],
                ..Default::default()
            }],
            ..Default::default()
        };
        apply_prop_to_node(&mut node, "child", "text", json!("new"));
        assert_eq!(node.children[0].props["text"], "new");
    }

    #[test]
    fn prefab_def_round_trips() {
        let def = hero_prefab();
        let json = serde_json::to_string(&def).unwrap();
        let back: PrefabDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "prefab:hero");
        assert_eq!(back.exposed.len(), 2);
    }
}
