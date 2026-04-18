//! `network::server::openapi` — OpenAPI 3.0 spec generation from ObjectRegistry.
//!
//! `generate_openapi` produces an `OpenApiSpec` (serializable to
//! JSON/YAML) from an `ObjectRegistry`, including schema objects
//! derived from `EntityFieldDef` field lists and path items from
//! the route spec generator.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::foundation::object_model::registry::ObjectRegistry;
use crate::foundation::object_model::types::{EntityFieldDef, EntityFieldType};

use super::route_spec::{generate_route_specs, HttpMethod};

// ── OpenAPI types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: OpenApiInfo,
    pub paths: BTreeMap<String, BTreeMap<String, OpenApiOperation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<OpenApiComponents>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiInfo {
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiOperation {
    pub operation_id: String,
    pub summary: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub parameters: Vec<OpenApiParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<OpenApiRequestBody>,
    pub responses: BTreeMap<String, OpenApiResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiParameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: String,
    pub required: bool,
    pub schema: BTreeMap<String, JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiRequestBody {
    pub required: bool,
    pub content: BTreeMap<String, OpenApiMediaType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiMediaType {
    pub schema: BTreeMap<String, JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiResponse {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiComponents {
    pub schemas: BTreeMap<String, JsonValue>,
}

// ── Generator ──────────────────────────────────────────────────────

pub fn generate_openapi(registry: &ObjectRegistry, title: &str, version: &str) -> OpenApiSpec {
    let route_specs = generate_route_specs(registry);
    let mut paths: BTreeMap<String, BTreeMap<String, OpenApiOperation>> = BTreeMap::new();
    let mut schemas: BTreeMap<String, JsonValue> = BTreeMap::new();

    // Generate schemas from entity field definitions
    for def in registry.all_defs() {
        let fields = registry.get_entity_fields(&def.type_name);
        if !fields.is_empty() {
            let schema = entity_schema(&def.label, &fields);
            schemas.insert(def.label.clone(), schema);
        }
    }

    // Generate path items from route specs
    for spec in &route_specs {
        let method_key = match spec.method {
            HttpMethod::Get => "get",
            HttpMethod::Post => "post",
            HttpMethod::Put => "put",
            HttpMethod::Patch => "patch",
            HttpMethod::Delete => "delete",
        };

        let parameters: Vec<OpenApiParameter> = spec
            .params
            .iter()
            .map(|p| OpenApiParameter {
                name: p.name.clone(),
                location: "path".to_string(),
                required: p.required,
                schema: BTreeMap::from([("type".to_string(), JsonValue::from("string"))]),
            })
            .collect();

        let request_body = if matches!(
            spec.method,
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
        ) {
            let schema_ref = spec
                .entity_type
                .as_ref()
                .and_then(|t| registry.get(t))
                .map(|d| {
                    BTreeMap::from([(
                        "$ref".to_string(),
                        JsonValue::from(format!("#/components/schemas/{}", d.label)),
                    )])
                })
                .unwrap_or_else(|| {
                    BTreeMap::from([("type".to_string(), JsonValue::from("object"))])
                });
            Some(OpenApiRequestBody {
                required: true,
                content: BTreeMap::from([(
                    "application/json".to_string(),
                    OpenApiMediaType { schema: schema_ref },
                )]),
            })
        } else {
            None
        };

        let mut responses = BTreeMap::new();
        responses.insert(
            "200".to_string(),
            OpenApiResponse {
                description: "Success".to_string(),
            },
        );
        if matches!(spec.method, HttpMethod::Post) {
            responses.insert(
                "201".to_string(),
                OpenApiResponse {
                    description: "Created".to_string(),
                },
            );
        }
        if !spec.params.is_empty() {
            responses.insert(
                "404".to_string(),
                OpenApiResponse {
                    description: "Not found".to_string(),
                },
            );
        }

        let operation = OpenApiOperation {
            operation_id: spec.operation_id.clone(),
            summary: spec.summary.clone(),
            tags: spec.tags.clone(),
            parameters,
            request_body,
            responses,
        };

        // OpenAPI uses {param} not {{param}}
        let openapi_path = spec.path.replace("{{", "{").replace("}}", "}");
        paths
            .entry(openapi_path)
            .or_default()
            .insert(method_key.to_string(), operation);
    }

    let components = if schemas.is_empty() {
        None
    } else {
        Some(OpenApiComponents { schemas })
    };

    OpenApiSpec {
        openapi: "3.0.3".to_string(),
        info: OpenApiInfo {
            title: title.to_string(),
            version: version.to_string(),
            description: Some("Auto-generated from ObjectRegistry".to_string()),
        },
        paths,
        components,
    }
}

fn field_type_to_openapi(field_type: EntityFieldType) -> (&'static str, Option<&'static str>) {
    match field_type {
        EntityFieldType::Bool => ("boolean", None),
        EntityFieldType::Int => ("integer", None),
        EntityFieldType::Float => ("number", Some("double")),
        EntityFieldType::String | EntityFieldType::Text | EntityFieldType::Color => {
            ("string", None)
        }
        EntityFieldType::Enum => ("string", None),
        EntityFieldType::ObjectRef => ("string", None),
        EntityFieldType::Date | EntityFieldType::Datetime => ("string", Some("date-time")),
        EntityFieldType::Url => ("string", Some("uri")),
        EntityFieldType::Lookup | EntityFieldType::Rollup => ("string", None),
    }
}

fn entity_schema(label: &str, fields: &[EntityFieldDef]) -> JsonValue {
    let mut properties = BTreeMap::new();
    let mut required_fields = Vec::new();

    // Shell fields
    properties.insert("id".to_string(), serde_json::json!({"type": "string"}));
    properties.insert("name".to_string(), serde_json::json!({"type": "string"}));
    required_fields.push("name".to_string());

    for field in fields {
        let (type_str, format) = field_type_to_openapi(field.field_type);
        let mut prop = BTreeMap::new();
        prop.insert("type".to_string(), JsonValue::from(type_str));
        if let Some(fmt) = format {
            prop.insert("format".to_string(), JsonValue::from(fmt));
        }
        if let Some(desc) = &field.description {
            prop.insert("description".to_string(), JsonValue::from(desc.as_str()));
        }
        if let Some(enum_opts) = &field.enum_options {
            let values: Vec<JsonValue> = enum_opts
                .iter()
                .map(|o| JsonValue::from(o.value.as_str()))
                .collect();
            prop.insert("enum".to_string(), JsonValue::Array(values));
        }
        properties.insert(
            field.id.clone(),
            JsonValue::Object(prop.into_iter().collect()),
        );

        if field.required == Some(true) {
            required_fields.push(field.id.clone());
        }
    }

    let mut schema = serde_json::json!({
        "type": "object",
        "description": format!("{label} entity"),
        "properties": properties,
    });
    if !required_fields.is_empty() {
        schema["required"] =
            JsonValue::Array(required_fields.into_iter().map(JsonValue::from).collect());
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::{
        EdgeTypeDef, EntityDef, EntityFieldDef, EntityFieldType, EnumOption,
    };

    fn task_def_with_fields() -> EntityDef {
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
            fields: Some(vec![
                EntityFieldDef {
                    id: "status".into(),
                    field_type: EntityFieldType::Enum,
                    label: Some("Status".into()),
                    description: Some("Current task status".into()),
                    required: Some(true),
                    default: None,
                    expression: None,
                    enum_options: Some(vec![
                        EnumOption {
                            value: "todo".into(),
                            label: "To Do".into(),
                        },
                        EnumOption {
                            value: "done".into(),
                            label: "Done".into(),
                        },
                    ]),
                    ref_types: None,
                    lookup_relation: None,
                    lookup_field: None,
                    rollup_relation: None,
                    rollup_field: None,
                    rollup_function: None,
                    ui: None,
                },
                EntityFieldDef {
                    id: "priority".into(),
                    field_type: EntityFieldType::Int,
                    label: Some("Priority".into()),
                    description: None,
                    required: None,
                    default: None,
                    expression: None,
                    enum_options: None,
                    ref_types: None,
                    lookup_relation: None,
                    lookup_field: None,
                    rollup_relation: None,
                    rollup_field: None,
                    rollup_function: None,
                    ui: None,
                },
                EntityFieldDef {
                    id: "due_date".into(),
                    field_type: EntityFieldType::Date,
                    label: Some("Due Date".into()),
                    description: None,
                    required: None,
                    default: None,
                    expression: None,
                    enum_options: None,
                    ref_types: None,
                    lookup_relation: None,
                    lookup_field: None,
                    rollup_relation: None,
                    rollup_field: None,
                    rollup_function: None,
                    ui: None,
                },
            ]),
            api: None,
        }
    }

    fn simple_def() -> EntityDef {
        EntityDef {
            type_name: "note".into(),
            nsid: None,
            category: "record".into(),
            label: "Note".into(),
            plural_label: Some("Notes".into()),
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
    fn generates_valid_openapi_spec() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields());
        let spec = generate_openapi(&reg, "Prism API", "1.0.0");
        assert_eq!(spec.openapi, "3.0.3");
        assert_eq!(spec.info.title, "Prism API");
        assert!(!spec.paths.is_empty());
    }

    #[test]
    fn paths_include_entity_crud() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields());
        let spec = generate_openapi(&reg, "Test", "1.0");
        assert!(spec.paths.contains_key("/api/tasks"));
        assert!(spec.paths.contains_key("/api/tasks/{id}"));
        let tasks_path = &spec.paths["/api/tasks"];
        assert!(tasks_path.contains_key("get"));
        assert!(tasks_path.contains_key("post"));
    }

    #[test]
    fn paths_include_edge_routes() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields())
            .register_edge(blocks_edge());
        let spec = generate_openapi(&reg, "Test", "1.0");
        assert!(spec.paths.contains_key("/api/edges/blocks"));
    }

    #[test]
    fn schemas_generated_from_fields() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields());
        let spec = generate_openapi(&reg, "Test", "1.0");
        let components = spec.components.unwrap();
        let task_schema = &components.schemas["Task"];
        let props = task_schema["properties"].as_object().unwrap();
        assert!(props.contains_key("status"));
        assert!(props.contains_key("priority"));
        assert!(props.contains_key("due_date"));
        assert!(props.contains_key("id"));
        assert!(props.contains_key("name"));
    }

    #[test]
    fn enum_field_produces_enum_values() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields());
        let spec = generate_openapi(&reg, "Test", "1.0");
        let components = spec.components.unwrap();
        let status_prop = &components.schemas["Task"]["properties"]["status"];
        let enum_values = status_prop["enum"].as_array().unwrap();
        assert_eq!(enum_values.len(), 2);
        assert_eq!(enum_values[0], "todo");
    }

    #[test]
    fn required_fields_listed() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields());
        let spec = generate_openapi(&reg, "Test", "1.0");
        let components = spec.components.unwrap();
        let required = components.schemas["Task"]["required"].as_array().unwrap();
        assert!(required.contains(&JsonValue::from("status")));
        assert!(required.contains(&JsonValue::from("name")));
    }

    #[test]
    fn no_components_when_no_fields() {
        let mut reg = ObjectRegistry::new();
        reg.register(simple_def());
        let spec = generate_openapi(&reg, "Test", "1.0");
        assert!(spec.components.is_none());
    }

    #[test]
    fn date_field_has_format() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields());
        let spec = generate_openapi(&reg, "Test", "1.0");
        let components = spec.components.unwrap();
        let due_date = &components.schemas["Task"]["properties"]["due_date"];
        assert_eq!(due_date["format"], "date-time");
    }

    #[test]
    fn post_routes_have_request_body() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields());
        let spec = generate_openapi(&reg, "Test", "1.0");
        let post_op = &spec.paths["/api/tasks"]["post"];
        assert!(post_op.request_body.is_some());
    }

    #[test]
    fn get_routes_have_no_request_body() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields());
        let spec = generate_openapi(&reg, "Test", "1.0");
        let get_op = &spec.paths["/api/tasks"]["get"];
        assert!(get_op.request_body.is_none());
    }

    #[test]
    fn spec_serializes_to_json() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields())
            .register_edge(blocks_edge());
        let spec = generate_openapi(&reg, "Prism", "0.1.0");
        let json = serde_json::to_string_pretty(&spec).unwrap();
        assert!(json.contains("\"openapi\""));
        assert!(json.contains("\"3.0.3\""));
    }

    #[test]
    fn path_parameters_on_get_by_id() {
        let mut reg = ObjectRegistry::new();
        reg.register(task_def_with_fields());
        let spec = generate_openapi(&reg, "Test", "1.0");
        let get_by_id = &spec.paths["/api/tasks/{id}"]["get"];
        assert_eq!(get_by_id.parameters.len(), 1);
        assert_eq!(get_by_id.parameters[0].name, "id");
        assert_eq!(get_by_id.parameters[0].location, "path");
    }

    #[test]
    fn field_type_mapping() {
        assert_eq!(
            field_type_to_openapi(EntityFieldType::Bool),
            ("boolean", None)
        );
        assert_eq!(
            field_type_to_openapi(EntityFieldType::Int),
            ("integer", None)
        );
        assert_eq!(
            field_type_to_openapi(EntityFieldType::Float),
            ("number", Some("double"))
        );
        assert_eq!(
            field_type_to_openapi(EntityFieldType::Url),
            ("string", Some("uri"))
        );
    }
}
