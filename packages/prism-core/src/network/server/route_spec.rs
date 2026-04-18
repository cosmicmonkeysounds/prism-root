//! `network::server::route_spec` — REST route descriptors from ObjectRegistry.
//!
//! `generate_route_specs` walks an `ObjectRegistry` and produces a
//! `Vec<RouteSpec>` — one CRUD set per entity type, plus relationship
//! routes for edge types. Host crates map these specs to their HTTP
//! framework of choice.

use serde::{Deserialize, Serialize};

use crate::foundation::object_model::registry::ObjectRegistry;

// ── Route spec types ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Patch => write!(f, "PATCH"),
            Self::Delete => write!(f, "DELETE"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteParam {
    pub name: String,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSpec {
    pub method: HttpMethod,
    pub path: String,
    pub operation_id: String,
    pub summary: String,
    pub entity_type: Option<String>,
    pub params: Vec<RouteParam>,
    pub tags: Vec<String>,
}

// ── Generator ──────────────────────────────────────────────────────

pub fn generate_route_specs(registry: &ObjectRegistry) -> Vec<RouteSpec> {
    let mut specs = Vec::new();

    for def in registry.all_defs() {
        let type_name = &def.type_name;
        let label = &def.label;
        let plural = def.plural_label.as_deref().unwrap_or(label);
        let path_segment = type_name.replace('_', "-");
        let plural_segment = format!("{path_segment}s");

        // List
        specs.push(RouteSpec {
            method: HttpMethod::Get,
            path: format!("/api/{plural_segment}"),
            operation_id: format!("list_{type_name}s"),
            summary: format!("List all {plural}"),
            entity_type: Some(type_name.clone()),
            params: Vec::new(),
            tags: vec![label.clone()],
        });

        // Create
        specs.push(RouteSpec {
            method: HttpMethod::Post,
            path: format!("/api/{plural_segment}"),
            operation_id: format!("create_{type_name}"),
            summary: format!("Create a new {label}"),
            entity_type: Some(type_name.clone()),
            params: Vec::new(),
            tags: vec![label.clone()],
        });

        // Get by ID
        specs.push(RouteSpec {
            method: HttpMethod::Get,
            path: format!("/api/{plural_segment}/{{id}}"),
            operation_id: format!("get_{type_name}"),
            summary: format!("Get a {label} by ID"),
            entity_type: Some(type_name.clone()),
            params: vec![RouteParam {
                name: "id".to_string(),
                required: true,
                description: format!("{label} ID"),
            }],
            tags: vec![label.clone()],
        });

        // Update
        specs.push(RouteSpec {
            method: HttpMethod::Patch,
            path: format!("/api/{plural_segment}/{{id}}"),
            operation_id: format!("update_{type_name}"),
            summary: format!("Update a {label}"),
            entity_type: Some(type_name.clone()),
            params: vec![RouteParam {
                name: "id".to_string(),
                required: true,
                description: format!("{label} ID"),
            }],
            tags: vec![label.clone()],
        });

        // Delete
        specs.push(RouteSpec {
            method: HttpMethod::Delete,
            path: format!("/api/{plural_segment}/{{id}}"),
            operation_id: format!("delete_{type_name}"),
            summary: format!("Delete a {label}"),
            entity_type: Some(type_name.clone()),
            params: vec![RouteParam {
                name: "id".to_string(),
                required: true,
                description: format!("{label} ID"),
            }],
            tags: vec![label.clone()],
        });
    }

    // Edge / relationship routes
    for edge_def in registry.all_edge_defs() {
        let relation = &edge_def.relation;
        let label = &edge_def.label;
        let path_segment = relation.replace('_', "-");

        // List relationships
        specs.push(RouteSpec {
            method: HttpMethod::Get,
            path: format!("/api/edges/{path_segment}"),
            operation_id: format!("list_{relation}_edges"),
            summary: format!("List {label} relationships"),
            entity_type: None,
            params: Vec::new(),
            tags: vec!["Edges".to_string(), label.clone()],
        });

        // Create relationship
        specs.push(RouteSpec {
            method: HttpMethod::Post,
            path: format!("/api/edges/{path_segment}"),
            operation_id: format!("create_{relation}_edge"),
            summary: format!("Create a {label} relationship"),
            entity_type: None,
            params: Vec::new(),
            tags: vec!["Edges".to_string(), label.clone()],
        });

        // Delete relationship
        specs.push(RouteSpec {
            method: HttpMethod::Delete,
            path: format!("/api/edges/{path_segment}/{{edge_id}}"),
            operation_id: format!("delete_{relation}_edge"),
            summary: format!("Delete a {label} relationship"),
            entity_type: None,
            params: vec![RouteParam {
                name: "edge_id".to_string(),
                required: true,
                description: "Edge ID".to_string(),
            }],
            tags: vec!["Edges".to_string(), label.clone()],
        });
    }

    specs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::{EdgeTypeDef, EntityDef};

    fn task_def() -> EntityDef {
        EntityDef {
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

    fn blocks_edge() -> EdgeTypeDef {
        EdgeTypeDef {
            relation: "blocks".into(),
            nsid: None,
            label: "Blocks".into(),
            description: None,
            behavior: None,
            undirected: None,
            allow_multiple: None,
            cascade: None,
            suggest_inline: None,
            color: None,
            source_types: None,
            source_categories: None,
            target_types: None,
            target_categories: None,
            scope: None,
        }
    }

    #[test]
    fn generates_crud_routes_per_entity() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def());
        let specs = generate_route_specs(&reg);
        assert_eq!(specs.len(), 5);
        let methods: Vec<HttpMethod> = specs.iter().map(|s| s.method).collect();
        assert!(methods.contains(&HttpMethod::Get));
        assert!(methods.contains(&HttpMethod::Post));
        assert!(methods.contains(&HttpMethod::Patch));
        assert!(methods.contains(&HttpMethod::Delete));
    }

    #[test]
    fn route_paths_use_plural_kebab_case() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def());
        let specs = generate_route_specs(&reg);
        assert!(specs.iter().any(|s| s.path == "/api/tasks"));
        assert!(specs.iter().any(|s| s.path == "/api/tasks/{id}"));
    }

    #[test]
    fn multiple_entity_types() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def()).register(project_def());
        let specs = generate_route_specs(&reg);
        assert_eq!(specs.len(), 10); // 5 per entity
    }

    #[test]
    fn edge_routes() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def()).register_edge(blocks_edge());
        let specs = generate_route_specs(&reg);
        let edge_specs: Vec<_> = specs
            .iter()
            .filter(|s| s.path.contains("/edges/"))
            .collect();
        assert_eq!(edge_specs.len(), 3);
        assert!(edge_specs.iter().any(|s| s.method == HttpMethod::Get));
        assert!(edge_specs.iter().any(|s| s.method == HttpMethod::Post));
        assert!(edge_specs.iter().any(|s| s.method == HttpMethod::Delete));
    }

    #[test]
    fn operation_ids_are_unique() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def())
            .register(project_def())
            .register_edge(blocks_edge());
        let specs = generate_route_specs(&reg);
        let mut op_ids: Vec<&str> = specs.iter().map(|s| s.operation_id.as_str()).collect();
        let len_before = op_ids.len();
        op_ids.sort();
        op_ids.dedup();
        assert_eq!(op_ids.len(), len_before);
    }

    #[test]
    fn empty_registry_produces_no_routes() {
        let reg = ObjectRegistry::new();
        let specs = generate_route_specs(&reg);
        assert!(specs.is_empty());
    }

    #[test]
    fn tags_contain_entity_label() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def());
        let specs = generate_route_specs(&reg);
        for spec in &specs {
            assert!(spec.tags.contains(&"Task".to_string()));
        }
    }

    #[test]
    fn params_on_get_by_id() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def());
        let specs = generate_route_specs(&reg);
        let get_by_id = specs.iter().find(|s| s.operation_id == "get_task").unwrap();
        assert_eq!(get_by_id.params.len(), 1);
        assert_eq!(get_by_id.params[0].name, "id");
        assert!(get_by_id.params[0].required);
    }

    #[test]
    fn entity_type_set_on_crud_routes() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def());
        let specs = generate_route_specs(&reg);
        for spec in &specs {
            if spec.path.contains("/api/tasks") {
                assert_eq!(spec.entity_type.as_deref(), Some("task"));
            }
        }
    }
}
