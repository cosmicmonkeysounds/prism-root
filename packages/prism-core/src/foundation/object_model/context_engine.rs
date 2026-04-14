//! `ContextEngine` — registry-driven suggestion engine. Port of
//! `foundation/object-model/context-engine.ts`.
//!
//! Given a source object type (and optionally a target type) the
//! engine answers: what edges can you draw, what children can you
//! create, what goes in the context menu, and what should fire on
//! `[[...]]` inline autocomplete. All answers are derived from an
//! [`ObjectRegistry`]; nothing is hardcoded here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::registry::ObjectRegistry;
use super::types::{EdgeBehavior, EdgeTypeDef, EntityDef};

// ── Output types ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeOption {
    pub relation: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub behavior: Option<EdgeBehavior>,
    #[serde(rename = "isInline")]
    pub is_inline: bool,
    pub def: EdgeTypeDef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildOption {
    #[serde(rename = "type")]
    pub type_name: String,
    pub label: String,
    #[serde(rename = "pluralLabel")]
    pub plural_label: String,
    pub def: EntityDef,
}

// ── Context menu types ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContextMenuAction {
    CreateChild,
    CreateEdge,
    Delete,
    Duplicate,
    Move,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextMenuItem {
    pub id: String,
    pub label: String,
    pub action: ContextMenuAction,
    pub payload: BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub shortcut: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextMenuSection {
    pub id: String,
    pub title: String,
    pub items: Vec<ContextMenuItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutocompleteSuggestion {
    #[serde(rename = "edgeTypes")]
    pub edge_types: Vec<EdgeTypeDef>,
    #[serde(rename = "defaultRelation")]
    pub default_relation: Option<String>,
}

// ── Evaluation context ─────────────────────────────────────────────

/// Optional input context for future-style queries. Keeps the door
/// open for "currently-focused object"-style helpers the TS original
/// hinted at without pinning the shape yet.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EvaluationContext {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub target_type: Option<String>,
}

// ── ContextEngine ──────────────────────────────────────────────────

pub struct ContextEngine<'a> {
    registry: &'a ObjectRegistry,
}

impl<'a> ContextEngine<'a> {
    pub fn new(registry: &'a ObjectRegistry) -> Self {
        Self { registry }
    }

    pub fn get_edge_options(
        &self,
        source_type: &str,
        target_type: Option<&str>,
    ) -> Vec<EdgeOption> {
        let defs = match target_type {
            Some(t) => self.registry.get_edges_between(source_type, t),
            None => self.registry.get_edges_from(source_type),
        };
        defs.into_iter()
            .map(|def| EdgeOption {
                relation: def.relation.clone(),
                label: def.label.clone(),
                description: def.description.clone(),
                behavior: def.behavior,
                is_inline: def.suggest_inline.unwrap_or(false),
                def: def.clone(),
            })
            .collect()
    }

    pub fn get_inline_link_types(&self, source_type: &str) -> Vec<EdgeTypeDef> {
        self.registry
            .get_edges_from(source_type)
            .into_iter()
            .filter(|def| def.suggest_inline.unwrap_or(false))
            .cloned()
            .collect()
    }

    pub fn get_inline_edge_types(&self) -> Vec<EdgeTypeDef> {
        self.registry
            .all_edge_defs()
            .into_iter()
            .filter(|def| def.suggest_inline.unwrap_or(false))
            .cloned()
            .collect()
    }

    pub fn get_autocomplete_suggestions(&self, source_type: &str) -> AutocompleteSuggestion {
        let from_source = self.get_inline_link_types(source_type);
        let edge_types = if from_source.is_empty() {
            self.get_inline_edge_types()
        } else {
            from_source
        };
        let default_relation = edge_types.first().map(|d| d.relation.clone());
        AutocompleteSuggestion {
            edge_types,
            default_relation,
        }
    }

    pub fn get_child_options(&self, parent_type: &str) -> Vec<ChildOption> {
        self.registry
            .get_allowed_child_types(parent_type)
            .into_iter()
            .filter_map(|type_name| {
                let def = self.registry.get(&type_name)?.clone();
                let plural_label = def
                    .plural_label
                    .clone()
                    .unwrap_or_else(|| def.label.clone());
                Some(ChildOption {
                    type_name,
                    label: def.label.clone(),
                    plural_label,
                    def,
                })
            })
            .collect()
    }

    pub fn get_context_menu(
        &self,
        object_type: &str,
        target_type: Option<&str>,
    ) -> Vec<ContextMenuSection> {
        let mut sections: Vec<ContextMenuSection> = Vec::new();

        let child_opts = self.get_child_options(object_type);
        if !child_opts.is_empty() {
            sections.push(ContextMenuSection {
                id: "create".into(),
                title: "Create".into(),
                items: child_opts
                    .iter()
                    .map(|opt| {
                        let mut payload = BTreeMap::new();
                        payload.insert(
                            "childType".into(),
                            serde_json::Value::String(opt.type_name.clone()),
                        );
                        ContextMenuItem {
                            id: format!("create-child:{}", opt.type_name),
                            label: format!("New {}", opt.label),
                            action: ContextMenuAction::CreateChild,
                            payload,
                            shortcut: None,
                        }
                    })
                    .collect(),
            });
        }

        let edge_opts: Vec<EdgeOption> = self
            .get_edge_options(object_type, target_type)
            .into_iter()
            .filter(|o| !o.is_inline)
            .collect();

        if !edge_opts.is_empty() {
            sections.push(ContextMenuSection {
                id: "connect".into(),
                title: "Connect".into(),
                items: edge_opts
                    .iter()
                    .map(|opt| {
                        let mut payload = BTreeMap::new();
                        payload.insert(
                            "relation".into(),
                            serde_json::Value::String(opt.relation.clone()),
                        );
                        ContextMenuItem {
                            id: format!("create-edge:{}", opt.relation),
                            label: format!("{}…", opt.label),
                            action: ContextMenuAction::CreateEdge,
                            payload,
                            shortcut: None,
                        }
                    })
                    .collect(),
            });
        }

        sections.push(ContextMenuSection {
            id: "object".into(),
            title: "Object".into(),
            items: vec![
                ContextMenuItem {
                    id: "duplicate".into(),
                    label: "Duplicate".into(),
                    action: ContextMenuAction::Duplicate,
                    payload: BTreeMap::new(),
                    shortcut: None,
                },
                ContextMenuItem {
                    id: "delete".into(),
                    label: "Delete".into(),
                    action: ContextMenuAction::Delete,
                    payload: BTreeMap::new(),
                    shortcut: None,
                },
            ],
        });

        sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::{CategoryRule, EdgeTypeDef, EntityDef};

    fn registry_with_project_and_task() -> ObjectRegistry {
        let mut reg = ObjectRegistry::with_category_rules([
            CategoryRule {
                category: "container".into(),
                can_parent: vec!["record".into()],
                can_be_root: Some(true),
            },
            CategoryRule {
                category: "record".into(),
                can_parent: Vec::new(),
                can_be_root: Some(true),
            },
        ]);
        reg.register(EntityDef {
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
        });
        reg.register(EntityDef {
            type_name: "task".into(),
            nsid: None,
            category: "record".into(),
            label: "Task".into(),
            plural_label: Some("Tasks".into()),
            description: None,
            color: None,
            default_child_view: None,
            tabs: None,
            child_only: None,
            extra_child_types: None,
            extra_parent_types: None,
            fields: None,
            api: None,
        });
        reg.register_edge(EdgeTypeDef {
            relation: "blocks".into(),
            nsid: None,
            label: "Blocks".into(),
            description: None,
            behavior: Some(EdgeBehavior::Dependency),
            undirected: None,
            allow_multiple: None,
            cascade: None,
            suggest_inline: Some(true),
            color: None,
            source_types: Some(vec!["task".into()]),
            source_categories: None,
            target_types: Some(vec!["task".into()]),
            target_categories: None,
            scope: None,
        });
        reg
    }

    #[test]
    fn child_options_follow_registry() {
        let reg = registry_with_project_and_task();
        let engine = ContextEngine::new(&reg);
        let opts = engine.get_child_options("project");
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].type_name, "task");
    }

    #[test]
    fn autocomplete_prefers_source_type_inlines() {
        let reg = registry_with_project_and_task();
        let engine = ContextEngine::new(&reg);
        let sug = engine.get_autocomplete_suggestions("task");
        assert_eq!(sug.default_relation.as_deref(), Some("blocks"));
    }

    #[test]
    fn context_menu_has_object_section() {
        let reg = registry_with_project_and_task();
        let engine = ContextEngine::new(&reg);
        let sections = engine.get_context_menu("project", None);
        assert!(sections.iter().any(|s| s.id == "object"));
    }
}
