//! Template data types — reusable blueprints for GraphObject subtrees.
//!
//! Port of `foundation/template/template-types.ts`.

use std::collections::BTreeMap;
use std::collections::HashMap;

use serde_json::Value;

use crate::foundation::object_model::types::{GraphObject, ObjectEdge};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TemplateVariable {
    pub name: String,
    pub label: Option<String>,
    pub default_value: Option<String>,
    pub required: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TemplateNode {
    pub placeholder_id: String,
    pub type_name: String,
    pub name: String,
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub pinned: Option<bool>,
    pub data: Option<BTreeMap<String, Value>>,
    pub children: Option<Vec<TemplateNode>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TemplateEdge {
    pub source_placeholder_id: String,
    pub target_placeholder_id: String,
    pub relation: String,
    pub data: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub root: TemplateNode,
    pub edges: Option<Vec<TemplateEdge>>,
    pub variables: Option<Vec<TemplateVariable>>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct InstantiateOptions {
    pub variables: HashMap<String, String>,
    pub parent_id: Option<String>,
    pub position: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct InstantiateResult {
    pub created: Vec<GraphObject>,
    pub created_edges: Vec<ObjectEdge>,
    pub id_map: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct TemplateFilter {
    pub category: Option<String>,
    pub type_name: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateFromObjectMeta {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
}
