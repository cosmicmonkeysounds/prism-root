//! `ObjectRegistry` — runtime registry of entity/edge type
//! definitions, category rules, and Lens slot registrations. Port
//! of `foundation/object-model/registry.ts`.
//!
//! Unlike the TS original the Rust port does not parameterize over
//! an icon type. Icons are just strings (icon name / SVG id) and
//! any host that needs richer representations can keep a parallel
//! table keyed by type.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::types::{
    CategoryRule, EdgeTypeDef, EntityDef, EntityFieldDef, GraphObject, TabDefinition,
};

// ── Slot types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotDef {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tabs: Option<Vec<TabDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fields: Option<Vec<EntityFieldDef>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotRegistration {
    pub slot: SlotDef,
    #[serde(skip_serializing_if = "Option::is_none", rename = "forTypes", default)]
    pub for_types: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "forCategories",
        default
    )]
    pub for_categories: Option<Vec<String>>,
}

// ── TreeNode ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TreeNode {
    pub object: GraphObject,
    pub children: Vec<TreeNode>,
    pub depth: Option<usize>,
    pub weak_ref_children: Option<Vec<WeakRefChildNode>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeakRefChildNode {
    pub object: GraphObject,
    pub relation: String,
    pub edge_id: String,
    pub provider_id: String,
    pub provider_label: String,
}

// ── ObjectRegistry ─────────────────────────────────────────────────

/// Runtime registry of entity types, edge types, category rules
/// and lens slots. `IndexMap` is used for deterministic iteration
/// order to match the legacy JS `Map` insertion-order semantics.
#[derive(Debug, Default, Clone)]
pub struct ObjectRegistry {
    types: IndexMap<String, EntityDef>,
    rules: HashMap<String, CategoryRule>,
    edge_types: IndexMap<String, EdgeTypeDef>,
    slots: Vec<SlotRegistration>,
}

impl ObjectRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_category_rules(rules: impl IntoIterator<Item = CategoryRule>) -> Self {
        let mut reg = Self::default();
        for rule in rules {
            reg.rules.insert(rule.category.clone(), rule);
        }
        reg
    }

    // ── Entity Type Registration ──────────────────────────────────

    pub fn register(&mut self, def: EntityDef) -> &mut Self {
        self.types.insert(def.type_name.clone(), def);
        self
    }

    pub fn register_all<I>(&mut self, defs: I) -> &mut Self
    where
        I: IntoIterator<Item = EntityDef>,
    {
        for def in defs {
            self.register(def);
        }
        self
    }

    pub fn remove(&mut self, type_name: &str) -> bool {
        self.types.shift_remove(type_name).is_some()
    }

    pub fn add_category_rule(&mut self, rule: CategoryRule) -> &mut Self {
        self.rules.insert(rule.category.clone(), rule);
        self
    }

    // ── Slot Registration ─────────────────────────────────────────

    pub fn register_slot(&mut self, registration: SlotRegistration) -> &mut Self {
        self.slots.push(registration);
        self
    }

    pub fn get_slots(&self, type_name: &str) -> Vec<&SlotDef> {
        let category = self.get_category(type_name);
        self.slots
            .iter()
            .filter(|reg| {
                let matches_type = reg
                    .for_types
                    .as_ref()
                    .map(|ts| ts.iter().any(|t| t == type_name))
                    .unwrap_or(false);
                let matches_cat = reg
                    .for_categories
                    .as_ref()
                    .map(|cs| cs.iter().any(|c| c == category))
                    .unwrap_or(false);
                matches_type || matches_cat
            })
            .map(|reg| &reg.slot)
            .collect()
    }

    pub fn get_effective_tabs(&self, type_name: &str) -> Vec<TabDefinition> {
        let base: Vec<TabDefinition> = self
            .types
            .get(type_name)
            .and_then(|d| d.tabs.clone())
            .unwrap_or_else(default_tabs);
        let mut seen: std::collections::HashSet<String> =
            base.iter().map(|t| t.id.clone()).collect();
        let mut merged = base;
        for slot in self.get_slots(type_name) {
            if let Some(tabs) = slot.tabs.as_ref() {
                for t in tabs {
                    if !seen.contains(&t.id) {
                        seen.insert(t.id.clone());
                        merged.push(t.clone());
                    }
                }
            }
        }
        merged
    }

    pub fn get_entity_fields(&self, type_name: &str) -> Vec<EntityFieldDef> {
        let base: Vec<EntityFieldDef> = self
            .types
            .get(type_name)
            .and_then(|d| d.fields.clone())
            .unwrap_or_default();
        let mut seen: std::collections::HashSet<String> =
            base.iter().map(|f| f.id.clone()).collect();
        let mut merged = base;
        for slot in self.get_slots(type_name) {
            if let Some(fields) = slot.fields.as_ref() {
                for f in fields {
                    if !seen.contains(&f.id) {
                        seen.insert(f.id.clone());
                        merged.push(f.clone());
                    }
                }
            }
        }
        merged
    }

    // ── Edge Type Registration ────────────────────────────────────

    pub fn register_edge(&mut self, def: EdgeTypeDef) -> &mut Self {
        self.edge_types.insert(def.relation.clone(), def);
        self
    }

    pub fn register_edges<I>(&mut self, defs: I) -> &mut Self
    where
        I: IntoIterator<Item = EdgeTypeDef>,
    {
        for def in defs {
            self.register_edge(def);
        }
        self
    }

    pub fn remove_edge(&mut self, relation: &str) -> bool {
        self.edge_types.shift_remove(relation).is_some()
    }

    pub fn get_edge_type(&self, relation: &str) -> Option<&EdgeTypeDef> {
        self.edge_types.get(relation)
    }

    pub fn get_edge_label<'a>(&'a self, relation: &'a str) -> &'a str {
        self.edge_types
            .get(relation)
            .map(|d| d.label.as_str())
            .unwrap_or(relation)
    }

    pub fn all_edge_types(&self) -> Vec<&str> {
        self.edge_types.keys().map(String::as_str).collect()
    }

    pub fn all_edge_defs(&self) -> Vec<&EdgeTypeDef> {
        self.edge_types.values().collect()
    }

    pub fn can_connect(&self, relation: &str, source_type: &str, target_type: &str) -> bool {
        let Some(def) = self.edge_types.get(relation) else {
            return true;
        };

        if def.source_types.is_some() || def.source_categories.is_some() {
            let src_cat = self.get_category(source_type);
            let by_type = def
                .source_types
                .as_ref()
                .map(|ts| ts.iter().any(|t| t == source_type))
                .unwrap_or(false);
            let by_cat = def
                .source_categories
                .as_ref()
                .map(|cs| cs.iter().any(|c| c == src_cat))
                .unwrap_or(false);
            if !by_type && !by_cat {
                return false;
            }
        }

        if def.target_types.is_some() || def.target_categories.is_some() {
            let tgt_cat = self.get_category(target_type);
            let by_type = def
                .target_types
                .as_ref()
                .map(|ts| ts.iter().any(|t| t == target_type))
                .unwrap_or(false);
            let by_cat = def
                .target_categories
                .as_ref()
                .map(|cs| cs.iter().any(|c| c == tgt_cat))
                .unwrap_or(false);
            if !by_type && !by_cat {
                return false;
            }
        }

        true
    }

    pub fn get_edges_from(&self, source_type: &str) -> Vec<&EdgeTypeDef> {
        let cat = self.get_category(source_type).to_string();
        self.all_edge_defs()
            .into_iter()
            .filter(|def| {
                if def.source_types.is_none() && def.source_categories.is_none() {
                    return true;
                }
                let by_type = def
                    .source_types
                    .as_ref()
                    .map(|ts| ts.iter().any(|t| t == source_type))
                    .unwrap_or(false);
                let by_cat = def
                    .source_categories
                    .as_ref()
                    .map(|cs| cs.iter().any(|c| c == &cat))
                    .unwrap_or(false);
                by_type || by_cat
            })
            .collect()
    }

    pub fn get_edges_to(&self, target_type: &str) -> Vec<&EdgeTypeDef> {
        let cat = self.get_category(target_type).to_string();
        self.all_edge_defs()
            .into_iter()
            .filter(|def| {
                if def.target_types.is_none() && def.target_categories.is_none() {
                    return true;
                }
                let by_type = def
                    .target_types
                    .as_ref()
                    .map(|ts| ts.iter().any(|t| t == target_type))
                    .unwrap_or(false);
                let by_cat = def
                    .target_categories
                    .as_ref()
                    .map(|cs| cs.iter().any(|c| c == &cat))
                    .unwrap_or(false);
                by_type || by_cat
            })
            .collect()
    }

    pub fn get_edges_between(&self, source_type: &str, target_type: &str) -> Vec<&EdgeTypeDef> {
        let relations: Vec<String> = self
            .edge_types
            .values()
            .filter(|def| self.can_connect(&def.relation, source_type, target_type))
            .map(|def| def.relation.clone())
            .collect();
        relations
            .into_iter()
            .filter_map(|r| self.edge_types.get(&r))
            .collect()
    }

    // ── Lookup ────────────────────────────────────────────────────

    pub fn get(&self, type_name: &str) -> Option<&EntityDef> {
        self.types.get(type_name)
    }

    pub fn has(&self, type_name: &str) -> bool {
        self.types.contains_key(type_name)
    }

    pub fn all_types(&self) -> Vec<&str> {
        self.types.keys().map(String::as_str).collect()
    }

    pub fn all_defs(&self) -> Vec<&EntityDef> {
        self.types.values().collect()
    }

    pub fn get_label<'a>(&'a self, type_name: &'a str) -> &'a str {
        self.types
            .get(type_name)
            .map(|d| d.label.as_str())
            .unwrap_or(type_name)
    }

    pub fn get_plural_label<'a>(&'a self, type_name: &'a str) -> &'a str {
        let def = self.types.get(type_name);
        def.and_then(|d| d.plural_label.as_deref())
            .or_else(|| def.map(|d| d.label.as_str()))
            .unwrap_or(type_name)
    }

    pub fn get_color(&self, type_name: &str) -> &str {
        self.types
            .get(type_name)
            .and_then(|d| d.color.as_deref())
            .unwrap_or("#888888")
    }

    pub fn get_category(&self, type_name: &str) -> &str {
        self.types
            .get(type_name)
            .map(|d| d.category.as_str())
            .unwrap_or("")
    }

    pub fn get_tabs(&self, type_name: &str) -> Vec<TabDefinition> {
        self.types
            .get(type_name)
            .and_then(|d| d.tabs.clone())
            .unwrap_or_else(default_tabs)
    }

    // ── Containment ───────────────────────────────────────────────

    pub fn can_be_child_of(&self, child_type: &str, parent_type: &str) -> bool {
        let Some(child) = self.types.get(child_type) else {
            return false;
        };
        let Some(parent) = self.types.get(parent_type) else {
            return false;
        };

        if let Some(extras) = parent.extra_child_types.as_ref() {
            if extras.iter().any(|t| t == child_type) {
                return true;
            }
        }
        if let Some(extras) = child.extra_parent_types.as_ref() {
            if extras.iter().any(|t| t == parent_type) {
                return true;
            }
        }

        if let Some(rule) = self.rules.get(&parent.category) {
            if rule.can_parent.iter().any(|c| c == &child.category) {
                return true;
            }
        }

        false
    }

    pub fn can_be_root(&self, type_name: &str) -> bool {
        let Some(def) = self.types.get(type_name) else {
            return false;
        };
        if def.child_only.unwrap_or(false) {
            return false;
        }
        if let Some(rule) = self.rules.get(&def.category) {
            if rule.can_be_root == Some(false) {
                return false;
            }
        }
        true
    }

    pub fn can_have_children(&self, type_name: &str) -> bool {
        let Some(def) = self.types.get(type_name) else {
            return false;
        };
        if def.child_only.unwrap_or(false) {
            return false;
        }
        if def
            .extra_child_types
            .as_ref()
            .map(|v| !v.is_empty())
            .unwrap_or(false)
        {
            return true;
        }
        self.rules
            .get(&def.category)
            .map(|r| !r.can_parent.is_empty())
            .unwrap_or(false)
    }

    pub fn get_allowed_child_types(&self, parent_type: &str) -> Vec<String> {
        self.types
            .keys()
            .filter(|t| self.can_be_child_of(t, parent_type))
            .cloned()
            .collect()
    }

    // ── Tree Utilities ────────────────────────────────────────────

    pub fn build_tree(&self, objects: &[GraphObject]) -> Vec<TreeNode> {
        // Two-pass assembly: first index every object by id, then
        // recursively materialise each subtree starting from the
        // detected roots. This avoids the order-sensitivity that
        // bit the earlier single-pass version.
        let by_id: HashMap<&str, &GraphObject> =
            objects.iter().map(|o| (o.id.as_str(), o)).collect();

        let mut children_of: HashMap<&str, Vec<&GraphObject>> = HashMap::new();
        let mut root_ids: Vec<&str> = Vec::new();
        for obj in objects {
            match obj.parent_id.as_ref().map(|p| p.as_str()) {
                Some(pid) if by_id.contains_key(pid) => {
                    children_of.entry(pid).or_default().push(obj);
                }
                _ => root_ids.push(obj.id.as_str()),
            }
        }

        fn build<'a>(
            obj: &'a GraphObject,
            children_of: &HashMap<&'a str, Vec<&'a GraphObject>>,
        ) -> TreeNode {
            let kids = children_of
                .get(obj.id.as_str())
                .map(|v| v.iter().map(|o| build(o, children_of)).collect())
                .unwrap_or_default();
            TreeNode {
                object: obj.clone(),
                children: kids,
                depth: None,
                weak_ref_children: None,
            }
        }

        let mut roots: Vec<TreeNode> = root_ids
            .into_iter()
            .filter_map(|id| by_id.get(id).map(|obj| build(obj, &children_of)))
            .collect();

        sort_by_position(&mut roots);
        roots
    }

    pub fn get_ancestors(
        &self,
        id: &str,
        object_map: &HashMap<String, GraphObject>,
    ) -> Vec<GraphObject> {
        let mut ancestors = Vec::new();
        let mut current = object_map.get(id);
        while let Some(obj) = current {
            if let Some(parent_id) = obj.parent_id.as_ref() {
                if let Some(parent) = object_map.get(parent_id.as_str()) {
                    ancestors.push(parent.clone());
                    current = Some(parent);
                    continue;
                }
            }
            break;
        }
        ancestors
    }
}

fn default_tabs() -> Vec<TabDefinition> {
    vec![
        TabDefinition {
            id: "overview".into(),
            label: "Overview".into(),
            dynamic: None,
        },
        TabDefinition {
            id: "children".into(),
            label: "Children".into(),
            dynamic: Some(true),
        },
        TabDefinition {
            id: "linked".into(),
            label: "Linked".into(),
            dynamic: Some(true),
        },
    ]
}

fn sort_by_position(nodes: &mut Vec<TreeNode>) {
    nodes.sort_by(|a, b| {
        a.object
            .position
            .partial_cmp(&b.object.position)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for n in nodes {
        sort_by_position(&mut n.children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::{DefaultChildView, ObjectId};

    fn task_def() -> EntityDef {
        EntityDef {
            type_name: "task".into(),
            nsid: None,
            category: "record".into(),
            label: "Task".into(),
            plural_label: Some("Tasks".into()),
            description: None,
            color: Some("#ff0000".into()),
            default_child_view: Some(DefaultChildView::List),
            tabs: None,
            child_only: None,
            extra_child_types: None,
            extra_parent_types: None,
            fields: None,
            api: None,
        }
    }

    fn project_def() -> EntityDef {
        EntityDef {
            type_name: "project".into(),
            nsid: None,
            category: "container".into(),
            label: "Project".into(),
            plural_label: Some("Projects".into()),
            description: None,
            color: None,
            default_child_view: None,
            tabs: None,
            child_only: None,
            extra_child_types: None,
            extra_parent_types: None,
            fields: None,
            api: None,
        }
    }

    fn container_rule() -> CategoryRule {
        CategoryRule {
            category: "container".into(),
            can_parent: vec!["container".into(), "record".into()],
            can_be_root: Some(true),
        }
    }

    fn record_rule() -> CategoryRule {
        CategoryRule {
            category: "record".into(),
            can_parent: Vec::new(),
            can_be_root: Some(true),
        }
    }

    #[test]
    fn containment_rules_respect_categories() {
        let mut reg = ObjectRegistry::with_category_rules([container_rule(), record_rule()]);
        reg.register(project_def()).register(task_def());
        assert!(reg.can_be_child_of("task", "project"));
        assert!(!reg.can_be_child_of("project", "task"));
        assert!(reg.can_be_root("project"));
        assert!(reg.can_be_root("task"));
    }

    #[test]
    fn build_tree_sorts_by_position() {
        let reg = ObjectRegistry::new();
        let mut a = GraphObject::new("a", "project", "A");
        a.position = 1.0;
        let mut b = GraphObject::new("b", "task", "B");
        b.parent_id = Some(ObjectId::new("a"));
        b.position = 0.0;
        let mut c = GraphObject::new("c", "task", "C");
        c.parent_id = Some(ObjectId::new("a"));
        c.position = 1.0;
        let tree = reg.build_tree(&[c.clone(), a.clone(), b.clone()]);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].object.id.as_str(), "a");
        assert_eq!(tree[0].children[0].object.id.as_str(), "b");
        assert_eq!(tree[0].children[1].object.id.as_str(), "c");
    }
}
