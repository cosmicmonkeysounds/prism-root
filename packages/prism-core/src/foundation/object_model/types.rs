//! Shell/payload data types for the unified object graph. Port of
//! `foundation/object-model/types.ts`.
//!
//! Every node in the graph is a [`GraphObject`] — a universal
//! shell (id, type, name, parent, timestamps) plus an opaque
//! `data: serde_json::Value` payload interpreted by whatever
//! [`EntityDef`] is registered for the object's `type`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use std::collections::BTreeMap;

// ── Branded identity ───────────────────────────────────────────────

/// Typed wrapper around an object id string. `#[serde(transparent)]`
/// so it round-trips through the existing JSON shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(pub String);

impl ObjectId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ObjectId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ObjectId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EdgeId(pub String);

impl EdgeId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for EdgeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for EdgeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for EdgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Cast a raw string to [`ObjectId`] at a trust boundary (IPC
/// response, Loro import, etc.)
pub fn object_id(id: impl Into<String>) -> ObjectId {
    ObjectId(id.into())
}

pub fn edge_id(id: impl Into<String>) -> EdgeId {
    EdgeId(id.into())
}

// ── Entity field definitions ───────────────────────────────────────

/// The set of value types an entity field may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityFieldType {
    Bool,
    Int,
    Float,
    String,
    Text,
    Color,
    Enum,
    ObjectRef,
    Date,
    Datetime,
    Url,
    Lookup,
    Rollup,
}

/// Aggregation function used by rollup fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RollupFunction {
    Sum,
    Avg,
    Count,
    Min,
    Max,
    List,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiHints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
}

/// Defines one typed field in an entity's payload schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityFieldDef {
    pub id: String,
    #[serde(rename = "type")]
    pub field_type: EntityFieldType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "enumOptions",
        default
    )]
    pub enum_options: Option<Vec<EnumOption>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "refTypes", default)]
    pub ref_types: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "lookupRelation",
        default
    )]
    pub lookup_relation: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "lookupField",
        default
    )]
    pub lookup_field: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "rollupRelation",
        default
    )]
    pub rollup_relation: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "rollupField",
        default
    )]
    pub rollup_field: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "rollupFunction",
        default
    )]
    pub rollup_function: Option<RollupFunction>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ui: Option<UiHints>,
}

// ── Shell ──────────────────────────────────────────────────────────

/// The universal object shell. `data` is an opaque JSON payload
/// interpreted by whatever [`EntityDef`] is registered for
/// [`GraphObject::type_name`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphObject {
    pub id: ObjectId,
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    #[serde(rename = "parentId")]
    pub parent_id: Option<ObjectId>,
    pub position: f64,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub date: Option<String>,
    #[serde(rename = "endDate")]
    pub end_date: Option<String>,
    pub description: String,
    pub color: Option<String>,
    pub image: Option<String>,
    pub pinned: bool,
    pub data: BTreeMap<String, Value>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "deletedAt", skip_serializing_if = "Option::is_none", default)]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl GraphObject {
    /// Convenience constructor used throughout tests + the tree
    /// model. Fills every optional with an appropriate empty
    /// value and stamps the timestamps with the current time.
    pub fn new(
        id: impl Into<ObjectId>,
        type_name: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            type_name: type_name.into(),
            name: name.into(),
            parent_id: None,
            position: 0.0,
            status: None,
            tags: Vec::new(),
            date: None,
            end_date: None,
            description: String::new(),
            color: None,
            image: None,
            pinned: false,
            data: BTreeMap::new(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

// ── Graph Edges ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectEdge {
    pub id: EdgeId,
    #[serde(rename = "sourceId")]
    pub source_id: ObjectId,
    #[serde(rename = "targetId")]
    pub target_id: ObjectId,
    pub relation: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub position: Option<f64>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    pub data: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedEdge {
    #[serde(flatten)]
    pub edge: ObjectEdge,
    pub target: GraphObject,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub via: Option<ResolvedEdgeVia>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedEdgeVia {
    pub id: ObjectId,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

// ── Edge Type Definition ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeBehavior {
    Weak,
    Strong,
    Dependency,
    Membership,
    Assignment,
    Stream,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeScope {
    #[default]
    Local,
    Federated,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeCascade {
    None,
    #[default]
    DeleteEdge,
    DeleteTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeTypeDef {
    pub relation: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub nsid: Option<String>,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub behavior: Option<EdgeBehavior>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub undirected: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "allowMultiple",
        default
    )]
    pub allow_multiple: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cascade: Option<EdgeCascade>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "suggestInline",
        default
    )]
    pub suggest_inline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub color: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "sourceTypes",
        default
    )]
    pub source_types: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "sourceCategories",
        default
    )]
    pub source_categories: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "targetTypes",
        default
    )]
    pub target_types: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "targetCategories",
        default
    )]
    pub target_categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scope: Option<EdgeScope>,
}

impl EdgeTypeDef {
    pub fn allow_multiple(&self) -> bool {
        self.allow_multiple.unwrap_or(true)
    }

    pub fn is_undirected(&self) -> bool {
        self.undirected.unwrap_or(false)
    }
}

// ── Entity Type Definition ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultChildView {
    List,
    Kanban,
    Grid,
    Timeline,
    Graph,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabDefinition {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dynamic: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiOperation {
    List,
    Get,
    Create,
    Update,
    Delete,
    Restore,
    Move,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefaultSort {
    pub field: String,
    pub dir: SortDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ObjectTypeApiConfig {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub operations: Option<Vec<ApiOperation>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "softDelete",
        default
    )]
    pub soft_delete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub searchable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "filterBy", default)]
    pub filter_by: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "defaultSort",
        default
    )]
    pub default_sort: Option<DefaultSort>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "cascadeEdges",
        default
    )]
    pub cascade_edges: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hooks: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityDef {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub nsid: Option<String>,
    pub category: String,
    pub label: String,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "pluralLabel",
        default
    )]
    pub plural_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub color: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "defaultChildView",
        default
    )]
    pub default_child_view: Option<DefaultChildView>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tabs: Option<Vec<TabDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "childOnly", default)]
    pub child_only: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "extraChildTypes",
        default
    )]
    pub extra_child_types: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "extraParentTypes",
        default
    )]
    pub extra_parent_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fields: Option<Vec<EntityFieldDef>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub api: Option<ObjectTypeApiConfig>,
}

// ── Category Rules ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryRule {
    pub category: String,
    #[serde(rename = "canParent")]
    pub can_parent: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "canBeRoot", default)]
    pub can_be_root: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_object_round_trips() {
        let obj = GraphObject::new("abc", "task", "Buy milk");
        let json = serde_json::to_string(&obj).unwrap();
        let back: GraphObject = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, obj.id);
        assert_eq!(back.type_name, "task");
        assert_eq!(back.position, 0.0);
    }

    #[test]
    fn entity_def_supports_optional_fields() {
        let def = EntityDef {
            type_name: "task".into(),
            nsid: None,
            category: "record".into(),
            label: "Task".into(),
            plural_label: Some("Tasks".into()),
            description: None,
            color: None,
            default_child_view: Some(DefaultChildView::List),
            tabs: None,
            child_only: None,
            extra_child_types: None,
            extra_parent_types: None,
            fields: None,
            api: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("\"type\":\"task\""));
        assert!(json.contains("\"pluralLabel\":\"Tasks\""));
    }
}
