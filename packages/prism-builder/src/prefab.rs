//! Prefabs — user-authored compound components.
//!
//! A [`PrefabDef`] captures a node subtree as a reusable template.
//! Each [`ExposedSlot`] pins one inner-node prop as an instance-editable
//! field; the slot key shows up in the property panel, the user types
//! a value, and at render time that value is written into the template
//! before the subtree is lowered through the unified `lower_ui` pipeline.
//!
//! [`PrefabComponent`] implements [`crate::block::Block`] (which gives
//! it a `Component` impl via the blanket impl in `block.rs`) so prefab
//! instances live in the [`crate::registry::ComponentRegistry`]
//! alongside built-ins. There is no "prefab walker" — rendering goes
//! through `Block::lower_ui` like every other registered block.

use serde::{Deserialize, Serialize};

use prism_core::help::HelpEntry;

use crate::block::Block;
use crate::component::ComponentId;
use crate::document::{Node, NodeId};
use crate::mutator::NodeMutator;
use crate::registry::FieldSpec;
use crate::signal::{common_signals, SignalDef};
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

impl Block for PrefabComponent {
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

    fn help_entry(&self) -> Option<HelpEntry> {
        if self.def.description.is_empty() {
            return None;
        }
        Some(HelpEntry::new(
            format!("builder.components.{}", self.def.id),
            &self.def.label,
            &self.def.description,
        ))
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }

    fn variants(&self) -> Vec<VariantAxis> {
        self.def.variants.clone()
    }

    /// Materialise the prefab's `def.root` against the host node's
    /// props (one write per [`ExposedSlot`]) and lower the result
    /// through the unified `lower_ui` pipeline. Internal node ids are
    /// prefixed with the host node's id so multiple prefab instances
    /// on the same page don't collide.
    fn lower_ui(
        &self,
        ctx: &crate::ui_lower::LowerCtx<'_>,
        node: &Node,
        _style: &crate::style::StyleProperties,
    ) -> prism_ui_runtime::layout::Node {
        // 1. Clone the template, namespacing every id under the host
        //    node's id (so `card-title` becomes `nXX::card-title`).
        let mut materialised = clone_with_id_prefix(&self.def.root, &node.id);

        // 2. Apply each ExposedSlot through the unified mutation seam:
        //    read `key` from the host props, write into
        //    `target_node.props[target_prop]`. Skip slots whose key
        //    isn't present — the inner template's authored default
        //    stands.
        let mutator = match ctx.bindings() {
            Some(b) => NodeMutator::with_bindings(b),
            None => NodeMutator::new(),
        };
        for slot in &self.def.exposed {
            let Some(value) = node.props.get(&slot.key) else {
                continue;
            };
            let prefixed_target = format!("{}::{}", node.id, slot.target_node);
            mutator.write_at(
                &mut materialised,
                &prefixed_target,
                &slot.target_prop,
                value.clone(),
            );
        }

        // 3. Hand off to the runtime walker. This recurses into every
        //    inner block via the host's existing `ComponentRegistry` —
        //    no prefab-specific render path.
        ctx.lower(&materialised)
    }
}

/// Deep-clone a node tree, prefixing every id with `{prefix}::`. The
/// prefix isolates inner ids per host instance so two `<card/>`s on
/// the same page produce two non-colliding subtrees.
fn clone_with_id_prefix(node: &Node, prefix: &str) -> Node {
    Node {
        id: format!("{prefix}::{}", node.id),
        component: node.component.clone(),
        props: node.props.clone(),
        children: node
            .children
            .iter()
            .map(|c| clone_with_id_prefix(c, prefix))
            .collect(),
        style: node.style.clone(),
        layout_mode: node.layout_mode.clone(),
        transform: node.transform.clone(),
        modifiers: node.modifiers.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
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
                        component: "text".into(),
                        props: json!({ "body": "Welcome", "level": "h1" }),
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
                    target_prop: "body".into(),
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
        let schema = Component::schema(&comp);
        assert_eq!(schema.len(), 2);
        assert_eq!(schema[0].key, "title");
        assert!(schema[0].required);
        assert_eq!(schema[1].key, "subtitle");
    }

    #[test]
    fn prefab_component_id() {
        let comp = PrefabComponent::new(hero_prefab());
        assert_eq!(Component::id(&comp), "prefab:hero");
    }

    #[test]
    fn prefab_def_round_trips() {
        let def = hero_prefab();
        let json = serde_json::to_string(&def).unwrap();
        let back: PrefabDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "prefab:hero");
        assert_eq!(back.exposed.len(), 2);
    }

    #[test]
    fn clone_with_prefix_namespaces_ids() {
        let n = Node {
            id: "outer".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![Node {
                id: "inner".into(),
                component: "text".into(),
                props: json!({}),
                children: vec![],
                ..Default::default()
            }],
            ..Default::default()
        };
        let cloned = clone_with_id_prefix(&n, "host");
        assert_eq!(cloned.id, "host::outer");
        assert_eq!(cloned.children[0].id, "host::inner");
    }
}
