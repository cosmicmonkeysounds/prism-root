//! Catalog of reusable `ObjectTemplate`s + instantiation engine.
//!
//! Port of `foundation/template/template-registry.ts`. Like
//! `TreeClipboard`, the registry borrows the tree / edges / undo
//! manager on each call rather than storing them, so it stays
//! ergonomic under Rust's borrow rules.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::Utc;
use serde_json::Value;

use crate::foundation::object_model::edge_model::{EdgeDraft, EdgeModel};
use crate::foundation::object_model::tree_model::{AddOptions, GraphObjectDraft, TreeModel};
use crate::foundation::object_model::types::{GraphObject, ObjectId};
use crate::foundation::object_model::ObjectModelError;
use crate::foundation::undo::manager::UndoRedoManager;
use crate::foundation::undo::types::ObjectSnapshot;

use super::types::{
    CreateFromObjectMeta, InstantiateOptions, InstantiateResult, ObjectTemplate, TemplateEdge,
    TemplateFilter, TemplateNode,
};

pub type TemplateIdGen = Box<dyn FnMut() -> String>;

pub struct TemplateRegistry {
    templates: HashMap<String, ObjectTemplate>,
    id_gen: TemplateIdGen,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template '{0}' not found")]
    TemplateNotFound(String),
    #[error("Object '{0}' not found")]
    ObjectNotFound(String),
    #[error(transparent)]
    ObjectModel(#[from] ObjectModelError),
}

fn default_id_generator() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{n:x}", Utc::now().timestamp_millis())
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            id_gen: Box::new(default_id_generator),
        }
    }

    pub fn with_id_gen(id_gen: TemplateIdGen) -> Self {
        Self {
            templates: HashMap::new(),
            id_gen,
        }
    }

    // ── Registration ─────────────────────────────────────────────

    pub fn register(&mut self, template: ObjectTemplate) {
        self.templates.insert(template.id.clone(), template);
    }

    pub fn unregister(&mut self, id: &str) -> bool {
        self.templates.remove(id).is_some()
    }

    pub fn get(&self, id: &str) -> Option<&ObjectTemplate> {
        self.templates.get(id)
    }

    pub fn has(&self, id: &str) -> bool {
        self.templates.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    pub fn list(&self, filter: Option<&TemplateFilter>) -> Vec<&ObjectTemplate> {
        let mut result: Vec<&ObjectTemplate> = self.templates.values().collect();
        let Some(filter) = filter else { return result };

        if let Some(cat) = &filter.category {
            result.retain(|t| t.category.as_deref() == Some(cat.as_str()));
        }
        if let Some(ty) = &filter.type_name {
            result.retain(|t| t.root.type_name == *ty);
        }
        if let Some(search) = &filter.search {
            let lower = search.to_lowercase();
            result.retain(|t| {
                t.name.to_lowercase().contains(&lower)
                    || t.description
                        .as_deref()
                        .is_some_and(|d| d.to_lowercase().contains(&lower))
            });
        }
        result
    }

    // ── Instantiation ────────────────────────────────────────────

    pub fn instantiate(
        &mut self,
        template_id: &str,
        tree: &mut TreeModel,
        edges: Option<&mut EdgeModel>,
        undo: Option<&mut UndoRedoManager>,
        options: InstantiateOptions,
    ) -> Result<InstantiateResult, TemplateError> {
        let template = self
            .templates
            .get(template_id)
            .ok_or_else(|| TemplateError::TemplateNotFound(template_id.to_string()))?
            .clone();

        let vars = &options.variables;
        let parent_id = options.parent_id.as_deref().map(ObjectId::new);
        let position = options.position;

        let mut id_map: HashMap<String, String> = HashMap::new();
        collect_placeholders(&template.root, &mut id_map, &mut self.id_gen);

        let mut created: Vec<GraphObject> = Vec::new();
        let mut snapshots: Vec<ObjectSnapshot> = Vec::new();

        instantiate_node(
            &template.root,
            parent_id.as_ref(),
            position,
            vars,
            &id_map,
            tree,
            &mut created,
            &mut snapshots,
        )?;

        let mut created_edges = Vec::new();
        if let (Some(edges_ref), Some(edge_templates)) = (edges, template.edges.as_ref()) {
            for edge_tpl in edge_templates {
                let source = id_map.get(&edge_tpl.source_placeholder_id);
                let target = id_map.get(&edge_tpl.target_placeholder_id);
                if let (Some(src), Some(tgt)) = (source, target) {
                    let edge = edges_ref.add(EdgeDraft {
                        id: None,
                        source_id: ObjectId::new(src.clone()),
                        target_id: ObjectId::new(tgt.clone()),
                        relation: edge_tpl.relation.clone(),
                        position: None,
                        data: edge_tpl
                            .data
                            .as_ref()
                            .map(|d| interpolate_data(d, vars))
                            .unwrap_or_default(),
                    })?;
                    created_edges.push(edge.clone());
                    snapshots.push(ObjectSnapshot::Edge {
                        before: None,
                        after: Some(edge),
                    });
                }
            }
        }

        if let Some(mgr) = undo {
            if !snapshots.is_empty() {
                mgr.push(
                    format!("Instantiate template \"{}\"", template.name),
                    snapshots,
                );
            }
        }

        Ok(InstantiateResult {
            created,
            created_edges,
            id_map,
        })
    }

    // ── Create from object ───────────────────────────────────────

    pub fn create_from_object(
        &self,
        root_object_id: &str,
        tree: &TreeModel,
        edges: Option<&EdgeModel>,
        meta: CreateFromObjectMeta,
    ) -> Result<ObjectTemplate, TemplateError> {
        if tree.get(root_object_id).is_none() {
            return Err(TemplateError::ObjectNotFound(root_object_id.to_string()));
        }

        let mut placeholder_map: HashMap<String, String> = HashMap::new();
        let mut counter: u32 = 0;

        let template_root = build_node(root_object_id, tree, &mut placeholder_map, &mut counter);

        let mut all_ids: HashSet<String> = HashSet::new();
        all_ids.insert(root_object_id.to_string());
        for d in tree.get_descendants(root_object_id) {
            all_ids.insert(d.id.as_str().to_string());
        }

        let mut template_edges: Vec<TemplateEdge> = Vec::new();
        if let Some(edges) = edges {
            for id in &all_ids {
                let from = edges.get_from(&ObjectId::new(id.clone()), None);
                for edge in from {
                    if all_ids.contains(edge.target_id.as_str()) {
                        let data = if edge.data.is_empty() {
                            None
                        } else {
                            Some(edge.data.clone())
                        };
                        template_edges.push(TemplateEdge {
                            source_placeholder_id: get_placeholder(
                                edge.source_id.as_str(),
                                &mut placeholder_map,
                                &mut counter,
                            ),
                            target_placeholder_id: get_placeholder(
                                edge.target_id.as_str(),
                                &mut placeholder_map,
                                &mut counter,
                            ),
                            relation: edge.relation.clone(),
                            data,
                        });
                    }
                }
            }
        }

        Ok(ObjectTemplate {
            id: meta.id,
            name: meta.name,
            description: meta.description,
            category: meta.category,
            root: template_root,
            edges: if template_edges.is_empty() {
                None
            } else {
                Some(template_edges)
            },
            variables: None,
            created_at: Utc::now().to_rfc3339(),
        })
    }
}

// ── Variable interpolation ────────────────────────────────────────

fn interpolate_string(template: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = find_close(template, i + 2) {
                let name = &template[i + 2..end];
                if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    if let Some(v) = vars.get(name) {
                        out.push_str(v);
                        i = end + 2;
                        continue;
                    }
                }
            }
        }
        out.push(template[i..].chars().next().unwrap());
        i += template[i..].chars().next().unwrap().len_utf8();
    }
    out
}

fn find_close(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn interpolate_data(
    data: &BTreeMap<String, Value>,
    vars: &HashMap<String, String>,
) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for (k, v) in data {
        let new_val = match v {
            Value::String(s) => Value::String(interpolate_string(s, vars)),
            _ => v.clone(),
        };
        out.insert(k.clone(), new_val);
    }
    out
}

// ── Instantiation helpers ─────────────────────────────────────────

fn collect_placeholders(
    node: &TemplateNode,
    id_map: &mut HashMap<String, String>,
    id_gen: &mut TemplateIdGen,
) {
    id_map.insert(node.placeholder_id.clone(), id_gen());
    if let Some(children) = node.children.as_ref() {
        for child in children {
            collect_placeholders(child, id_map, id_gen);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn instantiate_node(
    node: &TemplateNode,
    parent_id: Option<&ObjectId>,
    position: Option<f64>,
    vars: &HashMap<String, String>,
    id_map: &HashMap<String, String>,
    tree: &mut TreeModel,
    created: &mut Vec<GraphObject>,
    snapshots: &mut Vec<ObjectSnapshot>,
) -> Result<(), TemplateError> {
    let real_id = id_map
        .get(&node.placeholder_id)
        .cloned()
        .unwrap_or_else(|| node.placeholder_id.clone());

    let draft = GraphObjectDraft {
        id: Some(real_id.clone()),
        type_name: node.type_name.clone(),
        name: interpolate_string(&node.name, vars),
        status: node.status.as_ref().map(|s| interpolate_string(s, vars)),
        tags: Some(node.tags.clone().unwrap_or_default()),
        date: None,
        end_date: None,
        description: Some(
            node.description
                .as_ref()
                .map(|d| interpolate_string(d, vars))
                .unwrap_or_default(),
        ),
        color: node.color.clone(),
        image: None,
        pinned: Some(node.pinned.unwrap_or(false)),
        data: Some(
            node.data
                .as_ref()
                .map(|d| interpolate_data(d, vars))
                .unwrap_or_default(),
        ),
    };

    let obj = tree.add(
        draft,
        AddOptions {
            parent_id: parent_id.cloned(),
            position,
        },
    )?;
    created.push(obj.clone());
    snapshots.push(ObjectSnapshot::Object {
        before: None,
        after: Some(obj),
    });

    if let Some(children) = node.children.as_ref() {
        let real_id_owned = ObjectId::new(real_id);
        for child in children {
            instantiate_node(
                child,
                Some(&real_id_owned),
                None,
                vars,
                id_map,
                tree,
                created,
                snapshots,
            )?;
        }
    }

    Ok(())
}

// ── createFromObject helpers ──────────────────────────────────────

fn build_node(
    object_id: &str,
    tree: &TreeModel,
    placeholder_map: &mut HashMap<String, String>,
    counter: &mut u32,
) -> TemplateNode {
    let obj = tree.get(object_id).expect("existence checked by caller");
    let ph = get_placeholder(object_id, placeholder_map, counter);
    let children = tree.get_children(Some(&obj.id));
    let child_nodes: Option<Vec<TemplateNode>> = if children.is_empty() {
        None
    } else {
        Some(
            children
                .iter()
                .map(|c| build_node(c.id.as_str(), tree, placeholder_map, counter))
                .collect(),
        )
    };
    TemplateNode {
        placeholder_id: ph,
        type_name: obj.type_name.clone(),
        name: obj.name.clone(),
        status: obj.status.clone(),
        tags: if obj.tags.is_empty() {
            None
        } else {
            Some(obj.tags.clone())
        },
        description: if obj.description.is_empty() {
            None
        } else {
            Some(obj.description.clone())
        },
        color: obj.color.clone(),
        pinned: if obj.pinned { Some(true) } else { None },
        data: if obj.data.is_empty() {
            None
        } else {
            Some(obj.data.clone())
        },
        children: child_nodes,
    }
}

fn get_placeholder(
    id: &str,
    placeholder_map: &mut HashMap<String, String>,
    counter: &mut u32,
) -> String {
    if let Some(existing) = placeholder_map.get(id) {
        return existing.clone();
    }
    *counter += 1;
    let ph = format!("placeholder-{counter}");
    placeholder_map.insert(id.to_string(), ph.clone());
    ph
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::edge_model::EdgeModelOptions;
    use crate::foundation::object_model::tree_model::TreeModelOptions;
    use crate::foundation::undo::manager::{UndoApplier, UndoRedoManager};
    use serde_json::json;

    fn seq_id_gen(prefix: &str) -> Box<dyn FnMut() -> String> {
        use std::sync::atomic::{AtomicU64, Ordering};
        let counter = std::sync::Arc::new(AtomicU64::new(0));
        let prefix = prefix.to_string();
        Box::new(move || {
            let n = counter.fetch_add(1, Ordering::Relaxed);
            format!("{prefix}-{}", n + 1)
        })
    }

    fn make_tree() -> TreeModel {
        TreeModel::with_options(TreeModelOptions {
            id_gen: Some(seq_id_gen("id")),
            ..Default::default()
        })
    }

    fn make_edges() -> EdgeModel {
        EdgeModel::with_options(EdgeModelOptions {
            id_gen: Some(seq_id_gen("id")),
            ..Default::default()
        })
    }

    fn make_undo() -> UndoRedoManager {
        let applier: UndoApplier = Box::new(|_s, _d| {});
        UndoRedoManager::new(applier)
    }

    fn make_registry() -> TemplateRegistry {
        TemplateRegistry::with_id_gen(seq_id_gen("new"))
    }

    fn make_template() -> ObjectTemplate {
        ObjectTemplate {
            id: "tpl-1".into(),
            name: "Task Template".into(),
            description: None,
            category: Some("productivity".into()),
            root: TemplateNode {
                placeholder_id: "p1".into(),
                type_name: "task".into(),
                name: "{{name}}".into(),
                description: Some("Created by template".into()),
                data: Some({
                    let mut m = BTreeMap::new();
                    m.insert("priority".into(), json!("{{priority}}"));
                    m
                }),
                ..Default::default()
            },
            edges: None,
            variables: None,
            created_at: "2026-04-01T00:00:00.000Z".into(),
        }
    }

    fn make_nested_template() -> ObjectTemplate {
        ObjectTemplate {
            id: "tpl-nested".into(),
            name: "Project Template".into(),
            description: None,
            category: Some("productivity".into()),
            root: TemplateNode {
                placeholder_id: "p-root".into(),
                type_name: "project".into(),
                name: "{{name}}".into(),
                children: Some(vec![
                    TemplateNode {
                        placeholder_id: "p-task-1".into(),
                        type_name: "task".into(),
                        name: "Task 1".into(),
                        data: Some({
                            let mut m = BTreeMap::new();
                            m.insert("assignee".into(), json!("{{lead}}"));
                            m
                        }),
                        ..Default::default()
                    },
                    TemplateNode {
                        placeholder_id: "p-task-2".into(),
                        type_name: "task".into(),
                        name: "Task 2".into(),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            },
            edges: Some(vec![TemplateEdge {
                source_placeholder_id: "p-task-2".into(),
                target_placeholder_id: "p-task-1".into(),
                relation: "depends-on".into(),
                data: None,
            }]),
            variables: None,
            created_at: "2026-04-01T00:00:00.000Z".into(),
        }
    }

    // ── registration ─────────────────────────────────────────────

    #[test]
    fn registers_and_retrieves_template() {
        let mut r = make_registry();
        r.register(make_template());
        assert!(r.has("tpl-1"));
        assert!(r.get("tpl-1").is_some());
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn overwrites_on_duplicate_id() {
        let mut r = make_registry();
        r.register(make_template());
        let mut updated = make_template();
        updated.name = "Updated".into();
        r.register(updated);
        assert_eq!(r.len(), 1);
        assert_eq!(r.get("tpl-1").unwrap().name, "Updated");
    }

    #[test]
    fn unregister_returns_bool() {
        let mut r = make_registry();
        r.register(make_template());
        assert!(r.unregister("tpl-1"));
        assert!(!r.has("tpl-1"));
        assert_eq!(r.len(), 0);
        assert!(!r.unregister("nope"));
    }

    // ── list/filter ──────────────────────────────────────────────

    #[test]
    fn list_and_filter() {
        let mut r = make_registry();
        r.register(ObjectTemplate {
            id: "t1".into(),
            name: "Task A".into(),
            category: Some("productivity".into()),
            ..make_template()
        });
        let mut monster = make_template();
        monster.id = "t2".into();
        monster.name = "Monster B".into();
        monster.category = Some("game".into());
        monster.root.type_name = "monster".into();
        r.register(monster);
        r.register(ObjectTemplate {
            id: "t3".into(),
            name: "Task C".into(),
            category: Some("productivity".into()),
            ..make_template()
        });

        assert_eq!(r.list(None).len(), 3);
        let prod = r.list(Some(&TemplateFilter {
            category: Some("productivity".into()),
            ..Default::default()
        }));
        assert_eq!(prod.len(), 2);
        let monster_filter = r.list(Some(&TemplateFilter {
            type_name: Some("monster".into()),
            ..Default::default()
        }));
        assert_eq!(monster_filter.len(), 1);
        assert_eq!(monster_filter[0].id, "t2");
        let search = r.list(Some(&TemplateFilter {
            search: Some("monster".into()),
            ..Default::default()
        }));
        assert_eq!(search.len(), 1);
        let combined = r.list(Some(&TemplateFilter {
            category: Some("productivity".into()),
            search: Some("task c".into()),
            ..Default::default()
        }));
        assert_eq!(combined.len(), 1);
        assert_eq!(combined[0].id, "t3");
    }

    // ── simple instantiate ───────────────────────────────────────

    #[test]
    fn instantiate_simple_template() {
        let mut r = make_registry();
        let mut tree = make_tree();
        r.register(make_template());
        let mut vars = HashMap::new();
        vars.insert("name".into(), "My Task".into());
        vars.insert("priority".into(), "high".into());
        let result = r
            .instantiate(
                "tpl-1",
                &mut tree,
                None,
                None,
                InstantiateOptions {
                    variables: vars,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.created.len(), 1);
        assert_eq!(result.created[0].name, "My Task");
        assert_eq!(result.created[0].type_name, "task");
        assert_eq!(
            result.created[0].data.get("priority").unwrap(),
            &json!("high")
        );
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn instantiate_leaves_unreplaced_variables_as_is() {
        let mut r = make_registry();
        let mut tree = make_tree();
        r.register(make_template());
        let result = r
            .instantiate(
                "tpl-1",
                &mut tree,
                None,
                None,
                InstantiateOptions::default(),
            )
            .unwrap();
        assert_eq!(result.created[0].name, "{{name}}");
        assert_eq!(
            result.created[0].data.get("priority").unwrap(),
            &json!("{{priority}}")
        );
    }

    #[test]
    fn instantiate_under_specified_parent() {
        let mut r = make_registry();
        let mut tree = make_tree();
        let folder = tree
            .add(GraphObjectDraft::new("folder", "F"), AddOptions::default())
            .unwrap();
        r.register(make_template());
        let result = r
            .instantiate(
                "tpl-1",
                &mut tree,
                None,
                None,
                InstantiateOptions {
                    parent_id: Some(folder.id.as_str().to_string()),
                    variables: {
                        let mut m = HashMap::new();
                        m.insert("name".into(), "A".into());
                        m.insert("priority".into(), "low".into());
                        m
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            result.created[0].parent_id.as_ref().map(|p| p.as_str()),
            Some(folder.id.as_str())
        );
    }

    #[test]
    fn instantiate_nonexistent_template_errors() {
        let mut r = make_registry();
        let mut tree = make_tree();
        let err = r
            .instantiate("nope", &mut tree, None, None, InstantiateOptions::default())
            .unwrap_err();
        assert!(matches!(err, TemplateError::TemplateNotFound(_)));
    }

    // ── nested instantiate ───────────────────────────────────────

    #[test]
    fn nested_instantiate_creates_root_and_children() {
        let mut r = make_registry();
        let mut tree = make_tree();
        r.register(make_nested_template());
        let mut vars = HashMap::new();
        vars.insert("name".into(), "Sprint 23".into());
        vars.insert("lead".into(), "Alice".into());
        let result = r
            .instantiate(
                "tpl-nested",
                &mut tree,
                None,
                None,
                InstantiateOptions {
                    variables: vars,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.created.len(), 3);
        assert_eq!(result.created[0].name, "Sprint 23");
        assert_eq!(result.created[0].type_name, "project");
        let children = tree.get_children(Some(&result.created[0].id));
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].data.get("assignee").unwrap(), &json!("Alice"));
    }

    #[test]
    fn nested_instantiate_creates_edges_with_remapped_ids() {
        let mut r = make_registry();
        let mut tree = make_tree();
        let mut edges = make_edges();
        r.register(make_nested_template());
        let mut vars = HashMap::new();
        vars.insert("name".into(), "S".into());
        vars.insert("lead".into(), "Bob".into());
        let result = r
            .instantiate(
                "tpl-nested",
                &mut tree,
                Some(&mut edges),
                None,
                InstantiateOptions {
                    variables: vars,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.created_edges.len(), 1);
        let edge = &result.created_edges[0];
        assert_eq!(edge.relation, "depends-on");
        let created_ids: HashSet<String> = result
            .created
            .iter()
            .map(|o| o.id.as_str().to_string())
            .collect();
        assert!(created_ids.contains(edge.source_id.as_str()));
        assert!(created_ids.contains(edge.target_id.as_str()));
    }

    #[test]
    fn nested_instantiate_provides_id_map() {
        let mut r = make_registry();
        let mut tree = make_tree();
        r.register(make_nested_template());
        let mut vars = HashMap::new();
        vars.insert("name".into(), "P".into());
        let result = r
            .instantiate(
                "tpl-nested",
                &mut tree,
                None,
                None,
                InstantiateOptions {
                    variables: vars,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.id_map.len(), 3);
        assert!(result.id_map.contains_key("p-root"));
        assert!(result.id_map.contains_key("p-task-1"));
        assert!(result.id_map.contains_key("p-task-2"));
    }

    // ── undo integration ─────────────────────────────────────────

    #[test]
    fn undo_pushes_single_entry() {
        let mut r = make_registry();
        let mut tree = make_tree();
        let mut undo = make_undo();
        r.register(make_nested_template());
        let mut vars = HashMap::new();
        vars.insert("name".into(), "P".into());
        r.instantiate(
            "tpl-nested",
            &mut tree,
            None,
            Some(&mut undo),
            InstantiateOptions {
                variables: vars,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(undo.history().len(), 1);
        assert!(undo.history()[0].description.contains("Project Template"));
    }

    // ── createFromObject ─────────────────────────────────────────

    #[test]
    fn create_from_object_captures_single_object() {
        let mut tree = make_tree();
        let obj = tree
            .add(
                GraphObjectDraft {
                    type_name: "task".into(),
                    name: "Original".into(),
                    status: Some("open".into()),
                    data: Some({
                        let mut m = BTreeMap::new();
                        m.insert("priority".into(), json!("high"));
                        m
                    }),
                    ..GraphObjectDraft::new("task", "Original")
                },
                AddOptions::default(),
            )
            .unwrap();
        let r = make_registry();
        let tpl = r
            .create_from_object(
                obj.id.as_str(),
                &tree,
                None,
                CreateFromObjectMeta {
                    id: "from-obj".into(),
                    name: "My Template".into(),
                    description: None,
                    category: Some("test".into()),
                },
            )
            .unwrap();
        assert_eq!(tpl.id, "from-obj");
        assert_eq!(tpl.name, "My Template");
        assert_eq!(tpl.root.type_name, "task");
        assert_eq!(tpl.root.name, "Original");
        assert_eq!(tpl.root.status.as_deref(), Some("open"));
        assert_eq!(
            tpl.root.data.as_ref().unwrap().get("priority").unwrap(),
            &json!("high")
        );
    }

    #[test]
    fn create_from_object_captures_descendants_as_children() {
        let mut tree = make_tree();
        let folder = tree
            .add(GraphObjectDraft::new("folder", "F"), AddOptions::default())
            .unwrap();
        tree.add(
            GraphObjectDraft::new("task", "A"),
            AddOptions {
                parent_id: Some(folder.id.clone()),
                position: None,
            },
        )
        .unwrap();
        tree.add(
            GraphObjectDraft::new("task", "B"),
            AddOptions {
                parent_id: Some(folder.id.clone()),
                position: None,
            },
        )
        .unwrap();
        let r = make_registry();
        let tpl = r
            .create_from_object(
                folder.id.as_str(),
                &tree,
                None,
                CreateFromObjectMeta {
                    id: "nested".into(),
                    name: "Folder Template".into(),
                    description: None,
                    category: None,
                },
            )
            .unwrap();
        let children = tpl.root.children.as_ref().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].name, "A");
        assert_eq!(children[1].name, "B");
    }

    #[test]
    fn create_from_object_captures_internal_edges() {
        let mut tree = make_tree();
        let mut edges = make_edges();
        let a = tree
            .add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let b = tree
            .add(GraphObjectDraft::new("task", "B"), AddOptions::default())
            .unwrap();
        let folder = tree
            .add(GraphObjectDraft::new("folder", "F"), AddOptions::default())
            .unwrap();
        tree.move_to(a.id.as_str(), Some(folder.id.clone()), None)
            .unwrap();
        tree.move_to(b.id.as_str(), Some(folder.id.clone()), None)
            .unwrap();
        edges
            .add(EdgeDraft {
                id: None,
                source_id: a.id.clone(),
                target_id: b.id.clone(),
                relation: "dep".into(),
                position: None,
                data: Default::default(),
            })
            .unwrap();
        let r = make_registry();
        let tpl = r
            .create_from_object(
                folder.id.as_str(),
                &tree,
                Some(&edges),
                CreateFromObjectMeta {
                    id: "edge-tpl".into(),
                    name: "With Edges".into(),
                    description: None,
                    category: None,
                },
            )
            .unwrap();
        assert_eq!(tpl.edges.as_ref().unwrap().len(), 1);
        assert_eq!(tpl.edges.as_ref().unwrap()[0].relation, "dep");
    }

    #[test]
    fn round_trip_create_then_instantiate() {
        let mut tree = make_tree();
        let folder = tree
            .add(GraphObjectDraft::new("folder", "F"), AddOptions::default())
            .unwrap();
        tree.add(
            GraphObjectDraft::new("task", "T1"),
            AddOptions {
                parent_id: Some(folder.id.clone()),
                position: None,
            },
        )
        .unwrap();
        tree.add(
            GraphObjectDraft::new("task", "T2"),
            AddOptions {
                parent_id: Some(folder.id.clone()),
                position: None,
            },
        )
        .unwrap();
        let mut r = make_registry();
        let tpl = r
            .create_from_object(
                folder.id.as_str(),
                &tree,
                None,
                CreateFromObjectMeta {
                    id: "round-trip".into(),
                    name: "RT".into(),
                    description: None,
                    category: None,
                },
            )
            .unwrap();
        r.register(tpl);
        let before = tree.len();
        let result = r
            .instantiate(
                "round-trip",
                &mut tree,
                None,
                None,
                InstantiateOptions::default(),
            )
            .unwrap();
        assert_eq!(result.created.len(), 3);
        assert_eq!(tree.len(), before + 3);
    }

    #[test]
    fn create_from_object_nonexistent_errors() {
        let tree = make_tree();
        let r = make_registry();
        let err = r
            .create_from_object(
                "nope",
                &tree,
                None,
                CreateFromObjectMeta {
                    id: "x".into(),
                    name: "X".into(),
                    description: None,
                    category: None,
                },
            )
            .unwrap_err();
        assert!(matches!(err, TemplateError::ObjectNotFound(_)));
    }

    // ── variable interpolation edge cases ────────────────────────

    #[test]
    fn interpolates_description_field() {
        let mut r = make_registry();
        let mut tree = make_tree();
        let mut tpl = make_template();
        tpl.root = TemplateNode {
            placeholder_id: "p1".into(),
            type_name: "note".into(),
            name: "Note".into(),
            description: Some("Written by {{author}} on {{date}}".into()),
            ..Default::default()
        };
        r.register(tpl);
        let mut vars = HashMap::new();
        vars.insert("author".into(), "Alice".into());
        vars.insert("date".into(), "2026-04-01".into());
        let result = r
            .instantiate(
                "tpl-1",
                &mut tree,
                None,
                None,
                InstantiateOptions {
                    variables: vars,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            result.created[0].description,
            "Written by Alice on 2026-04-01"
        );
    }

    #[test]
    fn interpolates_status_field() {
        let mut r = make_registry();
        let mut tree = make_tree();
        let mut tpl = make_template();
        tpl.root = TemplateNode {
            placeholder_id: "p1".into(),
            type_name: "task".into(),
            name: "T".into(),
            status: Some("{{initialStatus}}".into()),
            ..Default::default()
        };
        r.register(tpl);
        let mut vars = HashMap::new();
        vars.insert("initialStatus".into(), "in-progress".into());
        let result = r
            .instantiate(
                "tpl-1",
                &mut tree,
                None,
                None,
                InstantiateOptions {
                    variables: vars,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.created[0].status.as_deref(), Some("in-progress"));
    }

    #[test]
    fn does_not_interpolate_non_string_data_values() {
        let mut r = make_registry();
        let mut tree = make_tree();
        let mut tpl = make_template();
        tpl.root = TemplateNode {
            placeholder_id: "p1".into(),
            type_name: "task".into(),
            name: "T".into(),
            data: Some({
                let mut m = BTreeMap::new();
                m.insert("count".into(), json!(42));
                m.insert("label".into(), json!("{{tag}}"));
                m
            }),
            ..Default::default()
        };
        r.register(tpl);
        let mut vars = HashMap::new();
        vars.insert("tag".into(), "urgent".into());
        let result = r
            .instantiate(
                "tpl-1",
                &mut tree,
                None,
                None,
                InstantiateOptions {
                    variables: vars,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.created[0].data.get("count").unwrap(), &json!(42));
        assert_eq!(
            result.created[0].data.get("label").unwrap(),
            &json!("urgent")
        );
    }

    #[test]
    fn handles_multiple_variables_in_one_string() {
        let mut r = make_registry();
        let mut tree = make_tree();
        let mut tpl = make_template();
        tpl.root = TemplateNode {
            placeholder_id: "p1".into(),
            type_name: "task".into(),
            name: "{{prefix}}-{{suffix}}".into(),
            ..Default::default()
        };
        r.register(tpl);
        let mut vars = HashMap::new();
        vars.insert("prefix".into(), "PROJ".into());
        vars.insert("suffix".into(), "001".into());
        let result = r
            .instantiate(
                "tpl-1",
                &mut tree,
                None,
                None,
                InstantiateOptions {
                    variables: vars,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.created[0].name, "PROJ-001");
    }

    #[test]
    fn instantiate_at_position() {
        let mut r = make_registry();
        let mut tree = make_tree();
        tree.add(
            GraphObjectDraft::new("task", "Existing"),
            AddOptions::default(),
        )
        .unwrap();
        r.register(make_template());
        let mut vars = HashMap::new();
        vars.insert("name".into(), "New".into());
        vars.insert("priority".into(), "low".into());
        r.instantiate(
            "tpl-1",
            &mut tree,
            None,
            None,
            InstantiateOptions {
                variables: vars,
                position: Some(0.0),
                ..Default::default()
            },
        )
        .unwrap();
        let roots = tree.get_children(None);
        assert_eq!(roots[0].name, "New");
    }
}
