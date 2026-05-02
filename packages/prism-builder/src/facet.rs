//! Facets — data-driven content generators.
//!
//! A `FacetDef` pairs a [`FacetTemplate`] (inline node subtree or
//! component reference) with a [`FacetKind`] that determines how data
//! is produced. Each kind resolves to `Vec<Value>`, which feeds into
//! template expansion (clone + bind per item).
//!
//! See `docs/dev/facets.md` for the full design rationale.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use prism_core::help::HelpEntry;
use prism_core::language::expression::{evaluate_expression, ExprValue};
use prism_core::language::visual::ScriptGraph;
use prism_core::widget::{get_json_field, json_sort_key, DataQuery, FilterOp, QueryFilter};

use crate::component::{Component, ComponentId, RenderError, RenderSlintContext};
use crate::document::{Node, NodeId};
use crate::html::Html;
use crate::html_block::{HtmlBlock, HtmlRenderContext};
use crate::prefab::{apply_prop_to_node, ExposedSlot, PrefabDef};
use crate::registry::{FieldKind, FieldSpec};
use crate::resource::ResourceId;
use crate::signal::{common_signals, SignalDef};
use crate::slint_source::SlintEmitter;

// ── Schema types ─────────────────────────────────────────────────────────────

pub type FacetSchemaId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetSchema {
    pub id: FacetSchemaId,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetRecord {
    pub id: String,
    #[serde(default)]
    pub fields: IndexMap<String, Value>,
}

impl FacetRecord {
    pub fn to_value(&self) -> Value {
        Value::Object(
            self.fields
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl FacetSchema {
    pub fn validate_record(&self, record: &FacetRecord) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for field in &self.fields {
            let val = record.fields.get(&field.key);
            if field.required {
                let missing = match val {
                    None | Some(Value::Null) => true,
                    Some(Value::String(s)) => s.is_empty(),
                    _ => false,
                };
                if missing {
                    errors.push(ValidationError {
                        field: field.key.clone(),
                        message: format!("{} is required", field.label),
                    });
                }
            }
            if let Some(val) = val {
                match &field.kind {
                    FieldKind::Number(bounds) => {
                        if let Some(n) = val.as_f64() {
                            if let Some(lo) = bounds.min {
                                if n < lo {
                                    errors.push(ValidationError {
                                        field: field.key.clone(),
                                        message: format!("must be >= {lo}"),
                                    });
                                }
                            }
                            if let Some(hi) = bounds.max {
                                if n > hi {
                                    errors.push(ValidationError {
                                        field: field.key.clone(),
                                        message: format!("must be <= {hi}"),
                                    });
                                }
                            }
                        }
                    }
                    FieldKind::Integer(bounds) => {
                        if let Some(n) = val.as_i64() {
                            if let Some(lo) = bounds.min {
                                if (n as f64) < lo {
                                    errors.push(ValidationError {
                                        field: field.key.clone(),
                                        message: format!("must be >= {lo}"),
                                    });
                                }
                            }
                            if let Some(hi) = bounds.max {
                                if (n as f64) > hi {
                                    errors.push(ValidationError {
                                        field: field.key.clone(),
                                        message: format!("must be <= {hi}"),
                                    });
                                }
                            }
                        }
                    }
                    FieldKind::Select(options) => {
                        if let Some(s) = val.as_str() {
                            if !s.is_empty() && !options.iter().any(|o| o.value == s) {
                                errors.push(ValidationError {
                                    field: field.key.clone(),
                                    message: format!("'{s}' is not a valid option"),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        errors
    }

    pub fn default_record(&self, id: impl Into<String>) -> FacetRecord {
        let mut fields = IndexMap::new();
        for field in &self.fields {
            if matches!(field.kind, FieldKind::Calculation { .. }) {
                continue;
            }
            let val = if field.default != Value::Null {
                field.default.clone()
            } else {
                default_for_kind(&field.kind)
            };
            fields.insert(field.key.clone(), val);
        }
        FacetRecord {
            id: id.into(),
            fields,
        }
    }
}

fn default_for_kind(kind: &FieldKind) -> Value {
    match kind {
        FieldKind::Text | FieldKind::TextArea | FieldKind::Date | FieldKind::DateTime => {
            Value::String(String::new())
        }
        FieldKind::Number(_) | FieldKind::Currency { .. } => Value::from(0.0),
        FieldKind::Integer(_) | FieldKind::Duration => Value::from(0),
        FieldKind::Boolean => Value::Bool(false),
        FieldKind::Color => Value::String("#000000".into()),
        FieldKind::File(_) => Value::Null,
        FieldKind::Select(options) => options
            .first()
            .map(|o| Value::String(o.value.clone()))
            .unwrap_or(Value::String(String::new())),
        FieldKind::Calculation { .. } => Value::Null,
    }
}

// ── Facet kinds ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FacetKind {
    #[default]
    List,
    ObjectQuery {
        #[serde(default)]
        query: DataQuery,
    },
    Script {
        #[serde(default)]
        source: String,
        #[serde(default)]
        language: ScriptLanguage,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<ScriptGraph>,
    },
    Aggregate {
        #[serde(default)]
        operation: AggregateOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
    },
    Lookup {
        #[serde(default)]
        source_entity: String,
        #[serde(default)]
        edge_type: String,
        #[serde(default)]
        target_entity: String,
    },
}

impl FacetKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::List => "List",
            Self::ObjectQuery { .. } => "Object Query",
            Self::Script { .. } => "Script",
            Self::Aggregate { .. } => "Aggregate",
            Self::Lookup { .. } => "Lookup",
        }
    }

    pub fn tag(&self) -> &'static str {
        match self {
            Self::List => "list",
            Self::ObjectQuery { .. } => "object-query",
            Self::Script { .. } => "script",
            Self::Aggregate { .. } => "aggregate",
            Self::Lookup { .. } => "lookup",
        }
    }

    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "object-query" => Self::ObjectQuery {
                query: DataQuery::default(),
            },
            "script" => Self::Script {
                source: String::new(),
                language: ScriptLanguage::default(),
                graph: None,
            },
            "aggregate" => Self::Aggregate {
                operation: AggregateOp::default(),
                field: None,
            },
            "lookup" => Self::Lookup {
                source_entity: String::new(),
                edge_type: String::new(),
                target_entity: String::new(),
            },
            _ => Self::List,
        }
    }

    /// Return a `DataQuery` for kinds that map onto one.
    /// `ObjectQuery` → its embedded query directly.
    /// `List` with `Query` source → not available here (lives on `FacetDataSource`).
    /// `Lookup`/`Script` → `None` (different resolution model).
    pub fn data_query(&self) -> Option<&DataQuery> {
        match self {
            Self::ObjectQuery { query } => Some(query),
            _ => None,
        }
    }
}

pub const FACET_KIND_TAGS: &[&str] = &["list", "object-query", "script", "aggregate", "lookup"];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ScriptLanguage {
    #[default]
    Luau,
    VisualGraph,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AggregateOp {
    #[default]
    Count,
    Sum,
    Min,
    Max,
    Avg,
    Join {
        #[serde(default)]
        separator: String,
    },
}

impl AggregateOp {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Sum => "sum",
            Self::Min => "min",
            Self::Max => "max",
            Self::Avg => "avg",
            Self::Join { .. } => "join",
        }
    }

    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "sum" => Self::Sum,
            "min" => Self::Min,
            "max" => Self::Max,
            "avg" => Self::Avg,
            "join" => Self::Join {
                separator: ", ".into(),
            },
            _ => Self::Count,
        }
    }
}

pub const AGGREGATE_OP_TAGS: &[&str] = &["count", "sum", "min", "max", "avg", "join"];

/// Apply an aggregate operation to a list of values.
pub fn apply_aggregate(items: &[Value], op: &AggregateOp, field: Option<&str>) -> Value {
    match op {
        AggregateOp::Count => Value::from(items.len()),
        AggregateOp::Sum => {
            let sum: f64 = items
                .iter()
                .filter_map(|item| {
                    field
                        .and_then(|f| get_json_field(item, f))
                        .and_then(|v| v.as_f64())
                })
                .sum();
            serde_json::json!(sum)
        }
        AggregateOp::Min => {
            let min = items
                .iter()
                .filter_map(|item| {
                    field
                        .and_then(|f| get_json_field(item, f))
                        .and_then(|v| v.as_f64())
                })
                .fold(f64::INFINITY, f64::min);
            if min.is_infinite() {
                Value::Null
            } else {
                serde_json::json!(min)
            }
        }
        AggregateOp::Max => {
            let max = items
                .iter()
                .filter_map(|item| {
                    field
                        .and_then(|f| get_json_field(item, f))
                        .and_then(|v| v.as_f64())
                })
                .fold(f64::NEG_INFINITY, f64::max);
            if max.is_infinite() {
                Value::Null
            } else {
                serde_json::json!(max)
            }
        }
        AggregateOp::Avg => {
            let mut count = 0usize;
            let sum: f64 = items
                .iter()
                .filter_map(|item| {
                    field
                        .and_then(|f| get_json_field(item, f))
                        .and_then(|v| v.as_f64())
                })
                .inspect(|_| count += 1)
                .sum();
            if count == 0 {
                Value::Null
            } else {
                serde_json::json!(sum / count as f64)
            }
        }
        AggregateOp::Join { separator } => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|item| {
                    field
                        .and_then(|f| get_json_field(item, f))
                        .map(|v| match v {
                            Value::String(s) => s,
                            other => other.to_string(),
                        })
                })
                .collect();
            Value::String(parts.join(separator))
        }
    }
}

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FacetDirection {
    Row,
    #[default]
    Column,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FacetLayout {
    #[serde(default)]
    pub direction: FacetDirection,
    #[serde(default)]
    pub gap: f32,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub columns: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FacetDataSource {
    /// Hand-authored inline array. Always available, no external dep.
    /// `records` holds structured data (when a schema is set); `items` holds
    /// legacy untyped JSON values. Both are resolved into `Vec<Value>`.
    Static {
        #[serde(default)]
        items: Vec<Value>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        records: Vec<FacetRecord>,
    },
    /// Reference to a `DataSource` resource whose `data` field is a JSON array.
    Resource { id: ResourceId },
    /// Filter and sort a resource array using structured `DataQuery` filters.
    Query {
        source: ResourceId,
        #[serde(default)]
        query: DataQuery,
    },
}

impl Default for FacetDataSource {
    fn default() -> Self {
        Self::Static {
            items: vec![],
            records: vec![],
        }
    }
}

impl FacetDataSource {
    pub fn resolve(
        &self,
        resources: &IndexMap<ResourceId, crate::resource::ResourceDef>,
    ) -> Vec<Value> {
        match self {
            FacetDataSource::Static { items, records } => {
                if !records.is_empty() {
                    records.iter().map(FacetRecord::to_value).collect()
                } else {
                    items.clone()
                }
            }
            FacetDataSource::Resource { id } => resources
                .get(id)
                .and_then(|r| r.data.as_array())
                .cloned()
                .unwrap_or_default(),
            FacetDataSource::Query { source, query } => {
                let mut items: Vec<Value> = resources
                    .get(source)
                    .and_then(|r| r.data.as_array())
                    .cloned()
                    .unwrap_or_default();

                query.apply(&mut items);
                items
            }
        }
    }
}

/// Maps one prefab exposed slot key to one dot-notation field path in a data item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetBinding {
    /// Key of the `ExposedSlot` on the referenced `PrefabDef`.
    pub slot_key: String,
    /// Dot-notation path into the data item JSON (e.g. `"meta.title"`).
    pub item_field: String,
}

/// Conditionally applies a variant axis value based on a data item field match.
/// When the item's `field` equals `value`, the prefab's `axis_key` prop is set
/// to `axis_value`, triggering the variant system's `apply_variant_defaults`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetVariantRule {
    pub field: String,
    pub value: String,
    pub axis_key: String,
    pub axis_value: String,
}

// ── Template + output types ──────────────────────────────────────────────────

/// How a facet renders each data item (or a single computed value).
///
/// `Inline` owns a `Node` subtree directly — `{{field}}` expressions in
/// its props are resolved against each data item at render time.
/// `ComponentRef` points to a registered component (backward-compatible
/// with the existing prefab pipeline).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FacetTemplate {
    /// Reference a registered component or prefab by ID.
    ComponentRef { component_id: ComponentId },
    /// Inline node subtree owned by this facet. Editable in context on
    /// the builder canvas. Bindings are `{{record.field}}` expressions
    /// in node props — no separate binding list needed.
    Inline { root: Box<Node> },
}

impl Default for FacetTemplate {
    fn default() -> Self {
        Self::ComponentRef {
            component_id: "card".into(),
        }
    }
}

/// Whether a facet repeats a template per data item or binds a single
/// scalar value directly to a target widget's prop.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FacetOutput {
    #[default]
    Repeated,
    Scalar {
        target_node: NodeId,
        target_prop: String,
    },
}

// ── FacetDef ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FacetDef {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub kind: FacetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<FacetSchemaId>,
    #[serde(default)]
    pub template: FacetTemplate,
    #[serde(default)]
    pub output: FacetOutput,
    #[serde(default)]
    pub data: FacetDataSource,
    #[serde(default)]
    pub bindings: Vec<FacetBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variant_rules: Vec<FacetVariantRule>,
    #[serde(default)]
    pub layout: FacetLayout,
    #[serde(skip)]
    pub resolved_data: Option<Vec<Value>>,
}

/// Result of resolving a facet's data. `Items` produces N prefab instances;
/// `Single` produces one instance with the aggregate value bound.
pub enum ResolvedFacetData {
    Items(Vec<Value>),
    Single(Value),
}

impl FacetDef {
    /// Return the component ID for `ComponentRef` templates, or `None` for inline.
    pub fn effective_component_id(&self) -> Option<&str> {
        match &self.template {
            FacetTemplate::ComponentRef { component_id } => Some(component_id),
            FacetTemplate::Inline { .. } => None,
        }
    }

    /// Mutably access the component ID inside a `ComponentRef` template.
    pub fn set_component_id(&mut self, id: &str) {
        if let FacetTemplate::ComponentRef { component_id } = &mut self.template {
            *component_id = id.to_string();
        }
    }

    /// Returns true if this facet uses an inline template.
    pub fn is_inline(&self) -> bool {
        matches!(self.template, FacetTemplate::Inline { .. })
    }

    /// Returns true if this facet produces scalar output.
    pub fn is_scalar(&self) -> bool {
        matches!(self.output, FacetOutput::Scalar { .. })
    }

    /// Resolve this facet's data items based on its kind.
    /// ObjectQuery, Script, and Lookup use `resolved_data` if the shell
    /// pre-populated it; otherwise they return empty.
    /// When `facet_schemas` is provided and this facet has a `schema_id`,
    /// `Calculation` fields in the schema are evaluated against each item.
    pub fn resolve_items(
        &self,
        resources: &IndexMap<ResourceId, crate::resource::ResourceDef>,
        facet_schemas: &IndexMap<FacetSchemaId, FacetSchema>,
    ) -> ResolvedFacetData {
        let schema = self.schema_id.as_ref().and_then(|id| facet_schemas.get(id));

        match &self.kind {
            FacetKind::List => {
                let mut items = self.data.resolve(resources);
                if let Some(s) = schema {
                    evaluate_calculations(&mut items, s);
                }
                ResolvedFacetData::Items(items)
            }
            FacetKind::ObjectQuery { .. } | FacetKind::Script { .. } | FacetKind::Lookup { .. } => {
                let mut items = self.resolved_data.clone().unwrap_or_default();
                if let Some(s) = schema {
                    evaluate_calculations(&mut items, s);
                }
                ResolvedFacetData::Items(items)
            }
            FacetKind::Aggregate { operation, field } => {
                let mut items = self.data.resolve(resources);
                if let Some(s) = schema {
                    evaluate_calculations(&mut items, s);
                }
                let result = apply_aggregate(&items, operation, field.as_deref());
                ResolvedFacetData::Single(result)
            }
        }
    }
}

/// Pre-render pass: resolve scalar facets and inject values into target nodes.
///
/// For each facet with `FacetOutput::Scalar`, resolves the data to a single
/// value and sets `target_node.target_prop` on the document tree. Call this
/// before the render walk so scalar bindings are visible to the renderer.
pub fn apply_scalar_bindings(doc: &mut crate::document::BuilderDocument) {
    let pairs: Vec<(String, String, Value)> = doc
        .facets
        .values()
        .filter_map(|def| {
            if let FacetOutput::Scalar {
                target_node,
                target_prop,
            } = &def.output
            {
                if target_node.is_empty() || target_prop.is_empty() {
                    return None;
                }
                let resolved = def.resolve_items(&doc.resources, &doc.facet_schemas);
                let val = match resolved {
                    ResolvedFacetData::Single(v) => v,
                    ResolvedFacetData::Items(items) => Value::from(items.len() as u64),
                };
                Some((target_node.clone(), target_prop.clone(), val))
            } else {
                None
            }
        })
        .collect();

    if let Some(root) = &mut doc.root {
        for (node_id, prop_key, val) in pairs {
            if let Some(node) = root.find_mut(&node_id) {
                if let Value::Object(ref mut map) = node.props {
                    map.insert(prop_key, val);
                }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse a simple filter expression string into a [`QueryFilter`].
///
/// Supported forms:
/// - `"field == value"` → `QueryFilter { field, op: Eq, value }`
/// - `"field != value"` → `QueryFilter { field, op: Neq, value }`
/// - `"field"` (truthy) is not expressible as a single `QueryFilter`
///   and returns `None`.
pub fn parse_filter_expr(expr: &str) -> Option<QueryFilter> {
    let expr = expr.trim();
    if let Some((lhs, rhs)) = expr.split_once("!=") {
        let val_str = rhs.trim().trim_matches('\'').trim_matches('"');
        return Some(QueryFilter::new(
            lhs.trim(),
            FilterOp::Neq,
            Value::String(val_str.to_string()),
        ));
    }
    if let Some((lhs, rhs)) = expr.split_once("==") {
        let val_str = rhs.trim().trim_matches('\'').trim_matches('"');
        return Some(QueryFilter::new(
            lhs.trim(),
            FilterOp::Eq,
            Value::String(val_str.to_string()),
        ));
    }
    None
}

/// Apply all facet bindings to a cloned prefab root node.
fn apply_bindings(root: &mut Node, prefab: &PrefabDef, bindings: &[FacetBinding], item: &Value) {
    for binding in bindings {
        if let Some(slot) = prefab.exposed.iter().find(|s| s.key == binding.slot_key) {
            if let Some(value) = get_json_field(item, &binding.item_field) {
                apply_prop_to_node(root, &slot.target_node, &slot.target_prop, value);
            }
        }
    }
}

/// Apply variant rules to a cloned prefab root. For each matching rule,
/// sets the axis key prop so the variant system picks it up during render.
fn evaluate_variant_rules(root: &mut Node, rules: &[FacetVariantRule], item: &Value) {
    for rule in rules {
        let raw = get_json_field(item, &rule.field);
        let sort_key = raw.clone().map(json_sort_key).unwrap_or_default();
        let matches = sort_key == rule.value
            || raw
                .as_ref()
                .map(|v| v.to_string().trim_matches('"') == rule.value)
                .unwrap_or(false);
        if matches {
            if let Value::Object(ref mut map) = root.props {
                map.insert(
                    rule.axis_key.clone(),
                    Value::String(rule.axis_value.clone()),
                );
            }
        }
    }
}

// ── Inline template expression resolution ────────────────────────────────────

/// Resolve `{{field}}` expressions in a node subtree against a data item.
///
/// Walks every string-valued prop on the node (and its children). Any
/// occurrence of `{{path}}` is replaced with the corresponding value
/// from the data item (via dot-notation `get_field`). If the entire
/// prop value is a single `{{path}}` expression, the prop is set to the
/// raw JSON value (preserving numbers, booleans, etc.). Otherwise it's
/// interpolated as a string.
pub fn resolve_template_expressions(node: &mut Node, item: &Value) {
    if let Value::Object(ref mut map) = node.props {
        let keys: Vec<String> = map.keys().cloned().collect();
        for key in keys {
            if let Some(Value::String(s)) = map.get(&key) {
                if let Some(resolved) = resolve_expression_string(s, item) {
                    map.insert(key, resolved);
                }
            }
        }
    }
    for child in &mut node.children {
        resolve_template_expressions(child, item);
    }
}

/// Resolve a single string that may contain `{{field}}` expressions.
///
/// Returns `None` if the string contains no expressions (no-op).
/// If the entire string is a single `{{path}}`, returns the raw value.
/// If the string mixes text and expressions, returns a string with
/// expressions interpolated.
fn resolve_expression_string(s: &str, item: &Value) -> Option<Value> {
    if !s.contains("{{") {
        return None;
    }

    // Fast path: the entire value is one `{{path}}` expression.
    let trimmed = s.trim();
    if trimmed.starts_with("{{") && trimmed.ends_with("}}") && trimmed.matches("{{").count() == 1 {
        let path = trimmed[2..trimmed.len() - 2].trim();
        let path = path.strip_prefix("record.").unwrap_or(path);
        return Some(get_json_field(item, path).unwrap_or(Value::Null));
    }

    // Mixed: interpolate all expressions as strings.
    let mut result = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            let path = after[..end].trim();
            let path = path.strip_prefix("record.").unwrap_or(path);
            match get_json_field(item, path) {
                Some(Value::String(v)) => result.push_str(&v),
                Some(Value::Null) | None => {}
                Some(v) => result.push_str(&v.to_string()),
            }
            rest = &after[end + 2..];
        } else {
            result.push_str("{{");
            rest = after;
        }
    }
    result.push_str(rest);
    Some(Value::String(result))
}

/// Collect all `{{field}}` expression paths from a node subtree.
/// Used by component promotion to generate `ExposedSlot` entries.
pub fn collect_expression_fields(node: &Node) -> Vec<(NodeId, String, String)> {
    let mut results = Vec::new();
    if let Value::Object(ref map) = node.props {
        for (key, val) in map {
            if let Value::String(s) = val {
                for path in extract_expression_paths(s) {
                    results.push((node.id.clone(), key.clone(), path));
                }
            }
        }
    }
    for child in &node.children {
        results.extend(collect_expression_fields(child));
    }
    results
}

fn extract_expression_paths(s: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            let path = after[..end].trim();
            let path = path.strip_prefix("record.").unwrap_or(path);
            paths.push(path.to_string());
            rest = &after[end + 2..];
        } else {
            break;
        }
    }
    paths
}

// ── Component promotion ──────────────────────────────────────────────────────

/// Promote an inline template to a registered component (PrefabDef).
///
/// Extracts `{{field}}` expressions into `ExposedSlot` entries, creates
/// a `PrefabDef` from the template root, and returns it along with the
/// component ID the facet should switch to.
pub fn promote_inline_to_component(facet_id: &str, root: &Node) -> (PrefabDef, Vec<FacetBinding>) {
    let component_id = format!("user:{facet_id}");
    let fields = collect_expression_fields(root);

    let mut clean_root = root.clone();
    let mut exposed = Vec::new();
    let mut bindings = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();

    for (node_id, prop_key, field_path) in &fields {
        let slot_key = field_path.replace('.', "_");
        if seen_keys.insert(slot_key.clone()) {
            exposed.push(ExposedSlot {
                key: slot_key.clone(),
                target_node: node_id.clone(),
                target_prop: prop_key.clone(),
                spec: FieldSpec::text(&slot_key, &slot_key),
            });
            bindings.push(FacetBinding {
                slot_key: slot_key.clone(),
                item_field: field_path.clone(),
            });
        }
    }

    // Clear expression strings from the clean root so the prefab
    // has placeholder values instead of raw `{{field}}` text.
    clear_expressions(&mut clean_root);

    let prefab = PrefabDef {
        id: component_id,
        label: format!("From {}", facet_id),
        description: String::new(),
        root: clean_root,
        exposed,
        variants: vec![],
        thumbnail: None,
    };
    (prefab, bindings)
}

fn clear_expressions(node: &mut Node) {
    if let Value::Object(ref mut map) = node.props {
        for val in map.values_mut() {
            if let Value::String(s) = val {
                if s.contains("{{") {
                    *val = Value::String(String::new());
                }
            }
        }
    }
    for child in &mut node.children {
        clear_expressions(child);
    }
}

/// Evaluate `Calculation` fields in a schema against each data item.
///
/// For every `FieldKind::Calculation { formula }` field, builds an
/// expression context from the item's other fields and runs
/// `evaluate_expression`. The result is stored back into the item.
pub fn evaluate_calculations(items: &mut [Value], schema: &FacetSchema) {
    let calc_fields: Vec<(&str, &str)> = schema
        .fields
        .iter()
        .filter_map(|f| match &f.kind {
            FieldKind::Calculation { formula } if !formula.is_empty() => {
                Some((f.key.as_str(), formula.as_str()))
            }
            _ => None,
        })
        .collect();

    if calc_fields.is_empty() {
        return;
    }

    for item in items.iter_mut() {
        let obj = match item.as_object_mut() {
            Some(o) => o,
            None => continue,
        };

        let mut ctx: HashMap<String, ExprValue> = HashMap::new();
        for (key, val) in obj.iter() {
            let expr_val = match val {
                Value::Number(n) => ExprValue::Number(n.as_f64().unwrap_or(0.0)),
                Value::Bool(b) => ExprValue::Boolean(*b),
                Value::String(s) => {
                    if let Ok(n) = s.parse::<f64>() {
                        ExprValue::Number(n)
                    } else {
                        ExprValue::String(s.clone())
                    }
                }
                _ => ExprValue::String(val.to_string()),
            };
            ctx.insert(key.clone(), expr_val);
        }

        for (key, formula) in &calc_fields {
            let result = evaluate_expression(formula, &ctx);
            let json_val = match result.result {
                ExprValue::Number(n) => Value::from(n),
                ExprValue::Boolean(b) => Value::Bool(b),
                ExprValue::String(s) => Value::String(s),
            };
            obj.insert((*key).to_string(), json_val);
        }
    }
}

// ── Slint component ───────────────────────────────────────────────────────────

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
            "Programmatic list: expands a prefab template once per item in a data source.",
        ))
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }

    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let facet_id = props.get("facet_id").and_then(|v| v.as_str()).unwrap_or("");

        let facet = match ctx.facets.get(facet_id) {
            Some(f) => f,
            None => {
                let label = if facet_id.is_empty() {
                    "(no facet_id set)".to_string()
                } else {
                    format!("Facet: {facet_id} (not found)")
                };
                return out.block("Rectangle", |out| {
                    out.prop_px("preferred-height", 40.0);
                    out.line("background: #f0f0f0;");
                    out.block("Text", |out| {
                        out.prop_string("text", &label);
                        Ok(())
                    })
                });
            }
        };

        // ── Scalar output: no render, value binding handled at document level ──
        if let FacetOutput::Scalar { .. } = &facet.output {
            return Ok(());
        }

        let resolved = facet.resolve_items(ctx.resources, ctx.facet_schemas);

        match &facet.template {
            FacetTemplate::Inline { root: template } => {
                let items = match resolved {
                    ResolvedFacetData::Items(mut items) => {
                        if let Some(max) = props.get("max_items").and_then(|v| v.as_u64()) {
                            items.truncate(max as usize);
                        }
                        items
                    }
                    ResolvedFacetData::Single(val) => {
                        let mut root = template.clone();
                        resolve_template_expressions(&mut root, &val);
                        return ctx.render_child(&root, out);
                    }
                };
                let layout_tag = match facet.layout.direction {
                    FacetDirection::Row => "HorizontalLayout",
                    FacetDirection::Column => "VerticalLayout",
                };
                out.block(layout_tag, |out| {
                    if facet.layout.gap > 0.0 {
                        out.prop_px("spacing", facet.layout.gap as f64);
                    }
                    out.line("alignment: start;");
                    for item in &items {
                        let mut root = template.clone();
                        resolve_template_expressions(&mut root, item);
                        evaluate_variant_rules(&mut root, &facet.variant_rules, item);
                        ctx.render_child(&root, out)?;
                    }
                    Ok(())
                })
            }
            FacetTemplate::ComponentRef { component_id } => {
                let prefab = ctx.prefabs.get(component_id.as_str()).ok_or_else(|| {
                    RenderError::Failed(format!("prefab '{component_id}' not found"))
                })?;
                let items = match resolved {
                    ResolvedFacetData::Items(mut items) => {
                        if let Some(max) = props.get("max_items").and_then(|v| v.as_u64()) {
                            items.truncate(max as usize);
                        }
                        items
                    }
                    ResolvedFacetData::Single(val) => {
                        let mut root = prefab.root.clone();
                        if let Some(first_binding) = facet.bindings.first() {
                            if let Some(slot) = prefab
                                .exposed
                                .iter()
                                .find(|s| s.key == first_binding.slot_key)
                            {
                                apply_prop_to_node(
                                    &mut root,
                                    &slot.target_node,
                                    &slot.target_prop,
                                    val,
                                );
                            }
                        }
                        return ctx.render_child(&root, out);
                    }
                };
                let layout_tag = match facet.layout.direction {
                    FacetDirection::Row => "HorizontalLayout",
                    FacetDirection::Column => "VerticalLayout",
                };
                out.block(layout_tag, |out| {
                    if facet.layout.gap > 0.0 {
                        out.prop_px("spacing", facet.layout.gap as f64);
                    }
                    out.line("alignment: start;");
                    for item in &items {
                        let mut root = prefab.root.clone();
                        apply_bindings(&mut root, prefab, &facet.bindings, item);
                        evaluate_variant_rules(&mut root, &facet.variant_rules, item);
                        ctx.render_child(&root, out)?;
                    }
                    Ok(())
                })
            }
        }
    }
}

// ── HTML block ────────────────────────────────────────────────────────────────

pub struct FacetHtmlBlock {
    pub id: ComponentId,
}

impl FacetHtmlBlock {
    pub fn new() -> Self {
        Self { id: "facet".into() }
    }
}

impl Default for FacetHtmlBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl HtmlBlock for FacetHtmlBlock {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        crate::schemas::facet()
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }

    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let facet_id = props.get("facet_id").and_then(|v| v.as_str()).unwrap_or("");

        let facet = ctx
            .facets
            .get(facet_id)
            .ok_or_else(|| RenderError::Failed(format!("facet '{facet_id}' not found")))?;

        // Scalar output: no rendered HTML from the facet itself.
        if let FacetOutput::Scalar { .. } = &facet.output {
            return Ok(());
        }

        let resolved = facet.resolve_items(ctx.resources, ctx.facet_schemas);

        let style = match facet.layout.direction {
            FacetDirection::Row => format!(
                "display:flex;flex-direction:row;gap:{}px",
                facet.layout.gap as u32
            ),
            FacetDirection::Column => format!(
                "display:flex;flex-direction:column;gap:{}px",
                facet.layout.gap as u32
            ),
        };

        match &facet.template {
            FacetTemplate::Inline { root: template } => {
                let items = match resolved {
                    ResolvedFacetData::Items(mut items) => {
                        if let Some(max) = props.get("max_items").and_then(|v| v.as_u64()) {
                            items.truncate(max as usize);
                        }
                        items
                    }
                    ResolvedFacetData::Single(val) => {
                        let mut root = template.clone();
                        resolve_template_expressions(&mut root, &val);
                        out.open_attrs("div", &[("data-facet", facet_id)]);
                        ctx.render_child(&root, out)?;
                        out.close("div");
                        return Ok(());
                    }
                };
                out.open_attrs(
                    "div",
                    &[("style", style.as_str()), ("data-facet", facet_id)],
                );
                for item in &items {
                    let mut root = template.clone();
                    resolve_template_expressions(&mut root, item);
                    evaluate_variant_rules(&mut root, &facet.variant_rules, item);
                    ctx.render_child(&root, out)?;
                }
                out.close("div");
                Ok(())
            }
            FacetTemplate::ComponentRef { component_id } => {
                let prefab = ctx.prefabs.get(component_id.as_str()).ok_or_else(|| {
                    RenderError::Failed(format!("prefab '{component_id}' not found"))
                })?;
                let items = match resolved {
                    ResolvedFacetData::Items(mut items) => {
                        if let Some(max) = props.get("max_items").and_then(|v| v.as_u64()) {
                            items.truncate(max as usize);
                        }
                        items
                    }
                    ResolvedFacetData::Single(val) => {
                        let mut root = prefab.root.clone();
                        if let Some(first_binding) = facet.bindings.first() {
                            if let Some(slot) = prefab
                                .exposed
                                .iter()
                                .find(|s| s.key == first_binding.slot_key)
                            {
                                apply_prop_to_node(
                                    &mut root,
                                    &slot.target_node,
                                    &slot.target_prop,
                                    val,
                                );
                            }
                        }
                        out.open_attrs("div", &[("data-facet", facet_id)]);
                        ctx.render_child(&root, out)?;
                        out.close("div");
                        return Ok(());
                    }
                };
                out.open_attrs(
                    "div",
                    &[("style", style.as_str()), ("data-facet", facet_id)],
                );
                for item in &items {
                    let mut root = prefab.root.clone();
                    apply_bindings(&mut root, prefab, &facet.bindings, item);
                    evaluate_variant_rules(&mut root, &facet.variant_rules, item);
                    ctx.render_child(&root, out)?;
                }
                out.close("div");
                Ok(())
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::json;

    use crate::prefab::{ExposedSlot, PrefabDef};
    use crate::registry::{FieldSpec, NumericBounds, SelectOption};

    fn hero_prefab() -> PrefabDef {
        PrefabDef {
            id: "prefab:hero".into(),
            label: "Hero".into(),
            description: String::new(),
            root: Node {
                id: "hero-root".into(),
                component: "text".into(),
                props: json!({ "body": "default" }),
                children: vec![],
                ..Default::default()
            },
            exposed: vec![ExposedSlot {
                key: "title".into(),
                target_node: "hero-root".into(),
                target_prop: "body".into(),
                spec: FieldSpec::text("title", "Title"),
            }],
            variants: vec![],
            thumbnail: None,
        }
    }

    fn sample_facet() -> FacetDef {
        FacetDef {
            id: "facet:heroes".into(),
            label: "Heroes".into(),
            description: String::new(),
            kind: FacetKind::List,
            schema_id: None,
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            data: FacetDataSource::Static {
                items: vec![json!({ "name": "Alpha" }), json!({ "name": "Beta" })],
                records: vec![],
            },
            bindings: vec![FacetBinding {
                slot_key: "title".into(),
                item_field: "name".into(),
            }],
            variant_rules: vec![],
            layout: FacetLayout {
                direction: FacetDirection::Column,
                gap: 8.0,
                ..Default::default()
            },
            resolved_data: None,
        }
    }

    #[test]
    fn json_field_flat() {
        let item = json!({ "name": "Alpha" });
        assert_eq!(get_json_field(&item, "name"), Some(json!("Alpha")));
    }

    #[test]
    fn json_field_nested() {
        let item = json!({ "meta": { "title": "Deep" } });
        assert_eq!(get_json_field(&item, "meta.title"), Some(json!("Deep")));
    }

    #[test]
    fn json_field_missing_returns_none() {
        let item = json!({ "a": 1 });
        assert!(get_json_field(&item, "b").is_none());
        assert!(get_json_field(&item, "a.nested").is_none());
    }

    #[test]
    fn static_source_resolves_to_items() {
        let resources = IndexMap::new();
        let src = FacetDataSource::Static {
            items: vec![json!("a"), json!("b")],
            records: vec![],
        };
        let resolved = src.resolve(&resources);
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn resource_source_resolves_from_registry() {
        use crate::resource::{ResourceDef, ResourceKind};
        let mut resources = IndexMap::new();
        resources.insert(
            "items".into(),
            ResourceDef {
                id: "items".into(),
                kind: ResourceKind::DataSource,
                label: "Items".into(),
                description: String::new(),
                data: json!([{ "name": "X" }, { "name": "Y" }]),
            },
        );
        let src = FacetDataSource::Resource { id: "items".into() };
        let resolved = src.resolve(&resources);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0]["name"], "X");
    }

    #[test]
    fn resource_source_missing_returns_empty() {
        let resources = IndexMap::new();
        let src = FacetDataSource::Resource {
            id: "missing".into(),
        };
        assert!(src.resolve(&resources).is_empty());
    }

    #[test]
    fn apply_bindings_injects_values() {
        let prefab = hero_prefab();
        let item = json!({ "name": "TestTitle" });
        let bindings = vec![FacetBinding {
            slot_key: "title".into(),
            item_field: "name".into(),
        }];
        let mut root = prefab.root.clone();
        apply_bindings(&mut root, &prefab, &bindings, &item);
        assert_eq!(root.props["body"], "TestTitle");
    }

    #[test]
    fn apply_bindings_skips_missing_slot() {
        let prefab = hero_prefab();
        let item = json!({ "name": "TestTitle" });
        let bindings = vec![FacetBinding {
            slot_key: "nonexistent".into(),
            item_field: "name".into(),
        }];
        let mut root = prefab.root.clone();
        apply_bindings(&mut root, &prefab, &bindings, &item);
        // default value should be unchanged
        assert_eq!(root.props["body"], "default");
    }

    #[test]
    fn facet_def_round_trips_serde() {
        let def = sample_facet();
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "facet:heroes");
        assert_eq!(back.bindings.len(), 1);
        assert_eq!(back.bindings[0].slot_key, "title");
    }

    #[test]
    fn facet_data_source_serde_static() {
        let src = FacetDataSource::Static {
            items: vec![json!({"a": 1})],
            records: vec![],
        };
        let json = serde_json::to_string(&src).unwrap();
        let back: FacetDataSource = serde_json::from_str(&json).unwrap();
        match back {
            FacetDataSource::Static { items, .. } => assert_eq!(items.len(), 1),
            _ => panic!("expected Static"),
        }
    }

    #[test]
    fn facet_data_source_serde_resource() {
        let src = FacetDataSource::Resource {
            id: "my-data".into(),
        };
        let json = serde_json::to_string(&src).unwrap();
        let back: FacetDataSource = serde_json::from_str(&json).unwrap();
        match back {
            FacetDataSource::Resource { id } => assert_eq!(id, "my-data"),
            _ => panic!("expected Resource"),
        }
    }

    #[test]
    fn facet_schema_has_required_facet_id() {
        let schema = crate::schemas::facet();
        let facet_id_spec = schema.iter().find(|s| s.key == "facet_id").unwrap();
        assert!(facet_id_spec.required);
    }

    fn sample_resources() -> IndexMap<String, crate::resource::ResourceDef> {
        use crate::resource::{ResourceDef, ResourceKind};
        let mut resources = IndexMap::new();
        resources.insert(
            "products".into(),
            ResourceDef {
                id: "products".into(),
                kind: ResourceKind::DataSource,
                label: "Products".into(),
                description: String::new(),
                data: json!([
                    { "name": "Apple", "status": "active", "price": 1.5 },
                    { "name": "Banana", "status": "inactive", "price": 0.5 },
                    { "name": "Cherry", "status": "active", "price": 3.0 },
                ]),
            },
        );
        resources
    }

    #[test]
    fn query_source_no_filter_no_sort_returns_all() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery::default(),
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn query_source_equality_filter() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                filters: vec![QueryFilter::new("status", FilterOp::Eq, json!("active"))],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i["status"] == "active"));
    }

    #[test]
    fn query_source_inequality_filter() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                filters: vec![QueryFilter::new("status", FilterOp::Neq, json!("active"))],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "Banana");
    }

    #[test]
    fn query_source_sort_ascending() {
        use prism_core::widget::QuerySort;
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                sort: vec![QuerySort {
                    field: "name".into(),
                    descending: false,
                }],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items[0]["name"], "Apple");
        assert_eq!(items[1]["name"], "Banana");
        assert_eq!(items[2]["name"], "Cherry");
    }

    #[test]
    fn query_source_sort_descending() {
        use prism_core::widget::QuerySort;
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                sort: vec![QuerySort {
                    field: "name".into(),
                    descending: true,
                }],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items[0]["name"], "Cherry");
        assert_eq!(items[2]["name"], "Apple");
    }

    #[test]
    fn query_source_filter_and_sort() {
        use prism_core::widget::QuerySort;
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                filters: vec![QueryFilter::new("status", FilterOp::Eq, json!("active"))],
                sort: vec![QuerySort {
                    field: "price".into(),
                    descending: true,
                }],
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"], "Cherry");
        assert_eq!(items[1]["name"], "Apple");
    }

    #[test]
    fn query_source_with_limit() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            query: DataQuery {
                limit: Some(2),
                ..Default::default()
            },
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn query_source_serde_round_trip() {
        use prism_core::widget::QuerySort;
        let src = FacetDataSource::Query {
            source: "data".into(),
            query: DataQuery {
                filters: vec![QueryFilter::new("active", FilterOp::Eq, json!(true))],
                sort: vec![QuerySort {
                    field: "name".into(),
                    descending: false,
                }],
                ..Default::default()
            },
        };
        let json_str = serde_json::to_string(&src).unwrap();
        let back: FacetDataSource = serde_json::from_str(&json_str).unwrap();
        match back {
            FacetDataSource::Query { source, query } => {
                assert_eq!(source, "data");
                assert_eq!(query.filters.len(), 1);
                assert_eq!(query.sort.len(), 1);
            }
            _ => panic!("expected Query"),
        }
    }

    // ── Schema tests ─────────────────────────────────────────────

    fn test_schema() -> FacetSchema {
        FacetSchema {
            id: "schema:test".into(),
            label: "Test Schema".into(),
            description: String::new(),
            fields: vec![
                FieldSpec::text("title", "Title").required(),
                FieldSpec::integer("count", "Count", NumericBounds::min_max(0.0, 100.0))
                    .with_default(json!(0)),
                FieldSpec::select(
                    "status",
                    "Status",
                    vec![
                        SelectOption::new("active", "Active"),
                        SelectOption::new("archived", "Archived"),
                    ],
                ),
            ],
        }
    }

    #[test]
    fn schema_serde_round_trip() {
        let schema = test_schema();
        let json = serde_json::to_string(&schema).unwrap();
        let back: FacetSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "schema:test");
        assert_eq!(back.fields.len(), 3);
    }

    #[test]
    fn schema_default_record_has_all_fields() {
        let schema = test_schema();
        let rec = schema.default_record("rec:1");
        assert_eq!(rec.id, "rec:1");
        assert_eq!(rec.fields.len(), 3);
        assert_eq!(rec.fields["title"], json!(""));
        assert_eq!(rec.fields["count"], json!(0));
        assert_eq!(rec.fields["status"], json!("active"));
    }

    #[test]
    fn schema_default_record_excludes_calculation_fields() {
        let schema = FacetSchema {
            id: "s:calc".into(),
            label: "With Calc".into(),
            description: String::new(),
            fields: vec![
                FieldSpec::number("price", "Price", NumericBounds::unbounded()),
                FieldSpec::calculation("total", "Total", "price * 2"),
            ],
        };
        let rec = schema.default_record("rec:1");
        assert_eq!(rec.fields.len(), 1);
        assert!(rec.fields.contains_key("price"));
        assert!(!rec.fields.contains_key("total"));
    }

    #[test]
    fn schema_validate_record_catches_missing_required() {
        let schema = test_schema();
        let rec = FacetRecord {
            id: "rec:1".into(),
            fields: IndexMap::new(),
        };
        let errors = schema.validate_record(&rec);
        assert!(errors.iter().any(|e| e.field == "title"));
    }

    #[test]
    fn schema_validate_record_catches_out_of_bounds() {
        let schema = test_schema();
        let mut rec = schema.default_record("rec:1");
        rec.fields.insert("count".into(), json!(200));
        let errors = schema.validate_record(&rec);
        assert!(errors.iter().any(|e| e.field == "count"));
    }

    #[test]
    fn schema_validate_record_catches_invalid_select() {
        let schema = test_schema();
        let mut rec = schema.default_record("rec:1");
        rec.fields.insert("title".into(), json!("ok"));
        rec.fields.insert("status".into(), json!("nonexistent"));
        let errors = schema.validate_record(&rec);
        assert!(errors.iter().any(|e| e.field == "status"));
    }

    #[test]
    fn schema_validate_record_passes_valid() {
        let schema = test_schema();
        let mut rec = schema.default_record("rec:1");
        rec.fields.insert("title".into(), json!("My Item"));
        rec.fields.insert("count".into(), json!(42));
        rec.fields.insert("status".into(), json!("active"));
        let errors = schema.validate_record(&rec);
        assert!(errors.is_empty());
    }

    #[test]
    fn facet_record_to_value() {
        let mut fields = IndexMap::new();
        fields.insert("name".into(), json!("Alpha"));
        fields.insert("age".into(), json!(25));
        let rec = FacetRecord {
            id: "rec:1".into(),
            fields,
        };
        let val = rec.to_value();
        assert_eq!(val["name"], "Alpha");
        assert_eq!(val["age"], 25);
    }

    #[test]
    fn static_source_prefers_records_over_items() {
        let resources = IndexMap::new();
        let mut fields = IndexMap::new();
        fields.insert("name".into(), json!("FromRecord"));
        let src = FacetDataSource::Static {
            items: vec![json!({"name": "FromItem"})],
            records: vec![FacetRecord {
                id: "r1".into(),
                fields,
            }],
        };
        let resolved = src.resolve(&resources);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0]["name"], "FromRecord");
    }

    #[test]
    fn facet_def_schema_id_round_trips() {
        let def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::List,
            schema_id: Some("schema:projects".into()),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            resolved_data: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_id, Some("schema:projects".into()));
    }

    #[test]
    fn facet_def_schema_id_none_omitted_in_json() {
        let def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::List,
            schema_id: None,
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            resolved_data: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(!json.contains("schema_id"));
    }

    #[test]
    fn facet_kind_default_is_list() {
        let kind = FacetKind::default();
        assert!(matches!(kind, FacetKind::List));
    }

    #[test]
    fn facet_kind_serde_list() {
        let def = sample_facet();
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.kind, FacetKind::List));
    }

    #[test]
    fn facet_kind_serde_object_query() {
        use prism_core::widget::QuerySort;
        let mut def = sample_facet();
        def.kind = FacetKind::ObjectQuery {
            query: DataQuery {
                object_type: Some("BlogPost".into()),
                filters: vec![QueryFilter::new("status", FilterOp::Eq, json!("published"))],
                sort: vec![QuerySort {
                    field: "created_at".into(),
                    descending: true,
                }],
                limit: Some(10),
            },
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::ObjectQuery { query } => {
                assert_eq!(query.object_type.as_deref(), Some("BlogPost"));
                assert_eq!(query.filters.len(), 1);
                assert!(query.sort[0].descending);
                assert_eq!(query.limit, Some(10));
            }
            _ => panic!("expected ObjectQuery"),
        }
    }

    #[test]
    fn facet_kind_serde_script() {
        let mut def = sample_facet();
        def.kind = FacetKind::Script {
            source: "return {}".into(),
            language: ScriptLanguage::Luau,
            graph: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::Script {
                source, language, ..
            } => {
                assert_eq!(source, "return {}");
                assert_eq!(*language, ScriptLanguage::Luau);
            }
            _ => panic!("expected Script"),
        }
    }

    #[test]
    fn facet_kind_serde_aggregate() {
        let mut def = sample_facet();
        def.kind = FacetKind::Aggregate {
            operation: AggregateOp::Sum,
            field: Some("price".into()),
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::Aggregate { operation, field } => {
                assert!(matches!(operation, AggregateOp::Sum));
                assert_eq!(field.as_deref(), Some("price"));
            }
            _ => panic!("expected Aggregate"),
        }
    }

    #[test]
    fn facet_kind_serde_lookup() {
        let mut def = sample_facet();
        def.kind = FacetKind::Lookup {
            source_entity: "Project".into(),
            edge_type: "has_member".into(),
            target_entity: "User".into(),
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::Lookup {
                source_entity,
                edge_type,
                target_entity,
            } => {
                assert_eq!(source_entity, "Project");
                assert_eq!(edge_type, "has_member");
                assert_eq!(target_entity, "User");
            }
            _ => panic!("expected Lookup"),
        }
    }

    #[test]
    fn facet_kind_backward_compat_missing_kind_defaults_to_list() {
        let json = r#"{
            "id": "facet:old",
            "label": "Old Facet",
            "data": { "kind": "static", "items": [] },
            "bindings": [],
            "layout": {}
        }"#;
        let def: FacetDef = serde_json::from_str(json).unwrap();
        assert!(matches!(def.kind, FacetKind::List));
        assert_eq!(def.effective_component_id(), Some("card"));
    }

    #[test]
    fn facet_kind_tag_round_trip() {
        for tag in FACET_KIND_TAGS {
            let kind = FacetKind::from_tag(tag);
            assert_eq!(kind.tag(), *tag);
        }
    }

    #[test]
    fn aggregate_count() {
        let items = vec![json!({"x": 1}), json!({"x": 2}), json!({"x": 3})];
        let result = apply_aggregate(&items, &AggregateOp::Count, None);
        assert_eq!(result, json!(3));
    }

    #[test]
    fn aggregate_sum() {
        let items = vec![json!({"x": 10}), json!({"x": 20}), json!({"x": 30})];
        let result = apply_aggregate(&items, &AggregateOp::Sum, Some("x"));
        assert_eq!(result, json!(60.0));
    }

    #[test]
    fn aggregate_min_max() {
        let items = vec![json!({"v": 5}), json!({"v": 2}), json!({"v": 8})];
        assert_eq!(
            apply_aggregate(&items, &AggregateOp::Min, Some("v")),
            json!(2.0)
        );
        assert_eq!(
            apply_aggregate(&items, &AggregateOp::Max, Some("v")),
            json!(8.0)
        );
    }

    #[test]
    fn aggregate_avg() {
        let items = vec![json!({"v": 10}), json!({"v": 20}), json!({"v": 30})];
        let result = apply_aggregate(&items, &AggregateOp::Avg, Some("v"));
        assert_eq!(result, json!(20.0));
    }

    #[test]
    fn aggregate_join() {
        let items = vec![
            json!({"name": "Alice"}),
            json!({"name": "Bob"}),
            json!({"name": "Carol"}),
        ];
        let result = apply_aggregate(
            &items,
            &AggregateOp::Join {
                separator: ", ".into(),
            },
            Some("name"),
        );
        assert_eq!(result, json!("Alice, Bob, Carol"));
    }

    #[test]
    fn aggregate_empty_items() {
        let items: Vec<Value> = vec![];
        assert_eq!(apply_aggregate(&items, &AggregateOp::Count, None), json!(0));
        assert_eq!(
            apply_aggregate(&items, &AggregateOp::Avg, Some("x")),
            Value::Null
        );
        assert_eq!(
            apply_aggregate(&items, &AggregateOp::Min, Some("x")),
            Value::Null
        );
    }

    #[test]
    fn aggregate_op_tag_round_trip() {
        for tag in AGGREGATE_OP_TAGS {
            let op = AggregateOp::from_tag(tag);
            assert_eq!(op.tag(), *tag);
        }
    }

    #[test]
    fn resolve_items_list_kind() {
        let def = sample_facet();
        let resources = IndexMap::new();
        let schemas = IndexMap::new();
        match def.resolve_items(&resources, &schemas) {
            ResolvedFacetData::Items(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected Items"),
        }
    }

    #[test]
    fn resolve_items_aggregate_kind() {
        let mut def = sample_facet();
        def.kind = FacetKind::Aggregate {
            operation: AggregateOp::Count,
            field: None,
        };
        let resources = IndexMap::new();
        let schemas = IndexMap::new();
        match def.resolve_items(&resources, &schemas) {
            ResolvedFacetData::Single(val) => assert_eq!(val, json!(2)),
            _ => panic!("expected Single"),
        }
    }

    #[test]
    fn field_kind_text_is_default_builder() {
        let spec = FieldSpec::text("test", "Test");
        assert!(matches!(spec.kind, FieldKind::Text));
    }

    // ── Calculation field tests ──────────────────────────────────────

    #[test]
    fn evaluate_calculations_basic_arithmetic() {
        let schema = FacetSchema {
            id: "s1".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![
                FieldSpec::number("price", "Price", NumericBounds::unbounded()),
                FieldSpec::integer("qty", "Quantity", NumericBounds::unbounded()),
                FieldSpec::calculation("total", "Total", "price * qty"),
            ],
        };

        let mut items = vec![
            json!({"price": 10.0, "qty": 3}),
            json!({"price": 5.5, "qty": 2}),
        ];

        evaluate_calculations(&mut items, &schema);

        assert_eq!(items[0]["total"], json!(30.0));
        assert_eq!(items[1]["total"], json!(11.0));
    }

    #[test]
    fn evaluate_calculations_string_concat() {
        let schema = FacetSchema {
            id: "s2".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![
                FieldSpec::text("first", "First"),
                FieldSpec::text("last", "Last"),
                FieldSpec::calculation("full", "Full Name", "concat(first, \" \", last)"),
            ],
        };

        let mut items = vec![json!({"first": "Alice", "last": "Smith"})];
        evaluate_calculations(&mut items, &schema);
        assert_eq!(items[0]["full"], json!("Alice Smith"));
    }

    #[test]
    fn evaluate_calculations_empty_formula_skipped() {
        let schema = FacetSchema {
            id: "s3".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![FieldSpec::calculation("calc", "Calc", String::new())],
        };

        let mut items = vec![json!({"x": 1})];
        evaluate_calculations(&mut items, &schema);
        assert!(items[0].get("calc").is_none());
    }

    #[test]
    fn evaluate_calculations_no_calc_fields_is_noop() {
        let schema = FacetSchema {
            id: "s4".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![FieldSpec::text("name", "Name")],
        };

        let mut items = vec![json!({"name": "x"})];
        let original = items.clone();
        evaluate_calculations(&mut items, &schema);
        assert_eq!(items, original);
    }

    #[test]
    fn evaluate_calculations_non_object_items_skipped() {
        let schema = FacetSchema {
            id: "s5".into(),
            label: "Test".into(),
            description: String::new(),
            fields: vec![FieldSpec::calculation("calc", "Calc", "1 + 1")],
        };

        let mut items = vec![json!(42), json!("hello"), json!(null)];
        let original = items.clone();
        evaluate_calculations(&mut items, &schema);
        assert_eq!(items, original);
    }

    #[test]
    fn resolve_items_with_calculations() {
        let mut def = sample_facet();
        def.schema_id = Some("s1".into());
        def.data = FacetDataSource::Static {
            items: vec![
                json!({"name": "A", "price": 10.0, "qty": 2}),
                json!({"name": "B", "price": 5.0, "qty": 4}),
            ],
            records: vec![],
        };

        let resources = IndexMap::new();
        let mut schemas = IndexMap::new();
        schemas.insert(
            "s1".to_string(),
            FacetSchema {
                id: "s1".into(),
                label: "Products".into(),
                description: String::new(),
                fields: vec![FieldSpec::calculation("total", "Total", "price * qty")],
            },
        );

        match def.resolve_items(&resources, &schemas) {
            ResolvedFacetData::Items(items) => {
                assert_eq!(items[0]["total"], json!(20.0));
                assert_eq!(items[1]["total"], json!(20.0));
            }
            _ => panic!("expected Items"),
        }
    }

    #[test]
    fn resolve_items_aggregate_with_calculations() {
        let mut def = sample_facet();
        def.kind = FacetKind::Aggregate {
            operation: AggregateOp::Sum,
            field: Some("total".into()),
        };
        def.schema_id = Some("s1".into());
        def.data = FacetDataSource::Static {
            items: vec![
                json!({"price": 10.0, "qty": 2}),
                json!({"price": 5.0, "qty": 3}),
            ],
            records: vec![],
        };

        let resources = IndexMap::new();
        let mut schemas = IndexMap::new();
        schemas.insert(
            "s1".to_string(),
            FacetSchema {
                id: "s1".into(),
                label: "Products".into(),
                description: String::new(),
                fields: vec![FieldSpec::calculation("total", "Total", "price * qty")],
            },
        );

        match def.resolve_items(&resources, &schemas) {
            ResolvedFacetData::Single(val) => {
                assert_eq!(val, json!(35.0));
            }
            _ => panic!("expected Single"),
        }
    }

    // ── Visual graph / ScriptLanguage tests ──────────────────────

    #[test]
    fn script_language_visual_graph_serde() {
        let lang = ScriptLanguage::VisualGraph;
        let json = serde_json::to_string(&lang).unwrap();
        assert_eq!(json, "\"visual-graph\"");
        let back: ScriptLanguage = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ScriptLanguage::VisualGraph);
    }

    #[test]
    fn script_kind_with_graph_serde() {
        use prism_core::language::visual::{ScriptGraph, ScriptNode, ScriptNodeKind};

        let mut graph = ScriptGraph::new("facet-graph", "Facet Script");
        graph.add_node(ScriptNode::new("entry", ScriptNodeKind::Entry, "Entry"));
        graph.add_node(ScriptNode::new("ret", ScriptNodeKind::Return, "return {}"));

        let mut def = sample_facet();
        def.kind = FacetKind::Script {
            source: String::new(),
            language: ScriptLanguage::VisualGraph,
            graph: Some(graph),
        };

        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        match &back.kind {
            FacetKind::Script {
                language, graph, ..
            } => {
                assert_eq!(*language, ScriptLanguage::VisualGraph);
                let g = graph.as_ref().unwrap();
                assert_eq!(g.nodes.len(), 2);
                assert_eq!(g.id, "facet-graph");
            }
            _ => panic!("expected Script"),
        }
    }

    #[test]
    fn script_kind_graph_none_omitted_in_json() {
        let mut def = sample_facet();
        def.kind = FacetKind::Script {
            source: "return {}".into(),
            language: ScriptLanguage::Luau,
            graph: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(!json.contains("\"graph\""));
    }

    #[test]
    fn script_kind_backward_compat_missing_graph() {
        let json = r#"{
            "id": "facet:old-script",
            "label": "Old Script",
            "kind": { "type": "script", "source": "return {}", "language": "luau" },
            "data": { "kind": "static", "items": [] },
            "bindings": [],
            "layout": {}
        }"#;
        let def: FacetDef = serde_json::from_str(json).unwrap();
        match &def.kind {
            FacetKind::Script { graph, .. } => assert!(graph.is_none()),
            _ => panic!("expected Script"),
        }
    }

    // ── Variant rule tests ──────────────────────────────────────

    #[test]
    fn evaluate_variant_rules_sets_axis_prop() {
        let mut root = Node {
            id: "r".into(),
            component: "text".into(),
            props: json!({}),
            children: vec![],
            ..Default::default()
        };
        let rules = vec![FacetVariantRule {
            field: "featured".into(),
            value: "true".into(),
            axis_key: "variant".into(),
            axis_value: "highlight".into(),
        }];
        let item = json!({"featured": true});
        evaluate_variant_rules(&mut root, &rules, &item);
        assert_eq!(root.props["variant"], "highlight");
    }

    #[test]
    fn evaluate_variant_rules_no_match_no_change() {
        let mut root = Node {
            id: "r".into(),
            component: "text".into(),
            props: json!({"variant": "default"}),
            children: vec![],
            ..Default::default()
        };
        let rules = vec![FacetVariantRule {
            field: "featured".into(),
            value: "true".into(),
            axis_key: "variant".into(),
            axis_value: "highlight".into(),
        }];
        let item = json!({"featured": false});
        evaluate_variant_rules(&mut root, &rules, &item);
        assert_eq!(root.props["variant"], "default");
    }

    #[test]
    fn evaluate_variant_rules_multiple_rules() {
        let mut root = Node {
            id: "r".into(),
            component: "text".into(),
            props: json!({}),
            children: vec![],
            ..Default::default()
        };
        let rules = vec![
            FacetVariantRule {
                field: "status".into(),
                value: "active".into(),
                axis_key: "variant".into(),
                axis_value: "primary".into(),
            },
            FacetVariantRule {
                field: "size".into(),
                value: "large".into(),
                axis_key: "size".into(),
                axis_value: "lg".into(),
            },
        ];
        let item = json!({"status": "active", "size": "large"});
        evaluate_variant_rules(&mut root, &rules, &item);
        assert_eq!(root.props["variant"], "primary");
        assert_eq!(root.props["size"], "lg");
    }

    #[test]
    fn facet_def_variant_rules_serde() {
        let mut def = sample_facet();
        def.variant_rules = vec![FacetVariantRule {
            field: "featured".into(),
            value: "true".into(),
            axis_key: "variant".into(),
            axis_value: "highlight".into(),
        }];
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.variant_rules.len(), 1);
        assert_eq!(back.variant_rules[0].field, "featured");
        assert_eq!(back.variant_rules[0].axis_value, "highlight");
    }

    #[test]
    fn facet_def_variant_rules_omitted_when_empty() {
        let def = sample_facet();
        let json = serde_json::to_string(&def).unwrap();
        assert!(!json.contains("variant_rules"));
    }

    // ── FacetTemplate tests ─────────────────────────────────────

    #[test]
    fn facet_template_default_is_component_ref() {
        let t = FacetTemplate::default();
        match t {
            FacetTemplate::ComponentRef { component_id } => assert_eq!(component_id, "card"),
            _ => panic!("expected ComponentRef"),
        }
    }

    #[test]
    fn facet_template_inline_serde() {
        let t = FacetTemplate::Inline {
            root: Box::new(Node {
                id: "tpl".into(),
                component: "text".into(),
                props: json!({"body": "{{title}}"}),
                children: vec![],
                ..Default::default()
            }),
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: FacetTemplate = serde_json::from_str(&json).unwrap();
        match back {
            FacetTemplate::Inline { root } => {
                assert_eq!(root.id, "tpl");
                assert_eq!(root.props["body"], "{{title}}");
            }
            _ => panic!("expected Inline"),
        }
    }

    #[test]
    fn facet_template_component_ref_serde() {
        let t = FacetTemplate::ComponentRef {
            component_id: "my-card".into(),
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: FacetTemplate = serde_json::from_str(&json).unwrap();
        match back {
            FacetTemplate::ComponentRef { component_id } => {
                assert_eq!(component_id, "my-card");
            }
            _ => panic!("expected ComponentRef"),
        }
    }

    #[test]
    fn facet_output_default_is_repeated() {
        let o = FacetOutput::default();
        assert!(matches!(o, FacetOutput::Repeated));
    }

    #[test]
    fn facet_output_scalar_serde() {
        let o = FacetOutput::Scalar {
            target_node: "label-1".into(),
            target_prop: "body".into(),
        };
        let json = serde_json::to_string(&o).unwrap();
        let back: FacetOutput = serde_json::from_str(&json).unwrap();
        match back {
            FacetOutput::Scalar {
                target_node,
                target_prop,
            } => {
                assert_eq!(target_node, "label-1");
                assert_eq!(target_prop, "body");
            }
            _ => panic!("expected Scalar"),
        }
    }

    #[test]
    fn facet_def_backward_compat_no_template_or_output() {
        let json = r#"{
            "id": "facet:old",
            "label": "Old Facet",
            "data": { "kind": "static", "items": [] },
            "bindings": [],
            "layout": {}
        }"#;
        let def: FacetDef = serde_json::from_str(json).unwrap();
        assert!(matches!(def.template, FacetTemplate::ComponentRef { .. }));
        assert!(matches!(def.output, FacetOutput::Repeated));
    }

    // ── Expression resolution tests ─────────────────────────────

    #[test]
    fn resolve_expression_single_field() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "{{title}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"title": "Hello World"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "Hello World");
    }

    #[test]
    fn resolve_expression_preserves_number() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"count": "{{total}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"total": 42});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["count"], json!(42));
    }

    #[test]
    fn resolve_expression_record_prefix_stripped() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "{{record.name}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"name": "Test"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "Test");
    }

    #[test]
    fn resolve_expression_mixed_text() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "Hello, {{name}}! You have {{count}} items."}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"name": "Alice", "count": 3});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "Hello, Alice! You have 3 items.");
    }

    #[test]
    fn resolve_expression_nested_field() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "{{meta.title}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"meta": {"title": "Deep"}});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "Deep");
    }

    #[test]
    fn resolve_expression_missing_field_is_null() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "{{nonexistent}}"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"name": "x"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], Value::Null);
    }

    #[test]
    fn resolve_expression_no_expressions_is_noop() {
        let mut node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({"body": "plain text"}),
            children: vec![],
            ..Default::default()
        };
        let item = json!({"name": "x"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.props["body"], "plain text");
    }

    #[test]
    fn resolve_expression_children() {
        let mut node = Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![Node {
                id: "child".into(),
                component: "text".into(),
                props: json!({"body": "{{title}}"}),
                children: vec![],
                ..Default::default()
            }],
            ..Default::default()
        };
        let item = json!({"title": "Child Title"});
        resolve_template_expressions(&mut node, &item);
        assert_eq!(node.children[0].props["body"], "Child Title");
    }

    #[test]
    fn collect_expression_fields_finds_all() {
        let node = Node {
            id: "root".into(),
            component: "card".into(),
            props: json!({"title": "{{name}}", "body": "{{description}}"}),
            children: vec![Node {
                id: "img".into(),
                component: "image".into(),
                props: json!({"src": "{{thumbnail}}"}),
                children: vec![],
                ..Default::default()
            }],
            ..Default::default()
        };
        let fields = collect_expression_fields(&node);
        assert_eq!(fields.len(), 3);
        assert!(fields.iter().any(|(_, _, f)| f == "name"));
        assert!(fields.iter().any(|(_, _, f)| f == "description"));
        assert!(fields
            .iter()
            .any(|(id, _, f)| id == "img" && f == "thumbnail"));
    }

    // ── Component promotion tests ───────────────────────────────

    #[test]
    fn promote_inline_creates_prefab_and_bindings() {
        let root = Node {
            id: "tpl-root".into(),
            component: "card".into(),
            props: json!({"title": "{{name}}", "body": "{{desc}}"}),
            children: vec![],
            ..Default::default()
        };
        let (prefab, bindings) = promote_inline_to_component("facet:test", &root);
        assert_eq!(prefab.id, "user:facet:test");
        assert_eq!(prefab.exposed.len(), 2);
        assert_eq!(bindings.len(), 2);
        assert_eq!(prefab.root.props["title"], "");
        assert_eq!(prefab.root.props["body"], "");
    }

    // ── FacetDef helper tests ───────────────────────────────────

    #[test]
    fn effective_component_id_for_component_ref() {
        let def = sample_facet();
        assert_eq!(def.effective_component_id(), Some("card"));
    }

    #[test]
    fn effective_component_id_for_inline() {
        let mut def = sample_facet();
        def.template = FacetTemplate::Inline {
            root: Box::new(Node {
                id: "tpl".into(),
                component: "text".into(),
                props: json!({}),
                children: vec![],
                ..Default::default()
            }),
        };
        assert_eq!(def.effective_component_id(), None);
        assert!(def.is_inline());
    }

    #[test]
    fn is_scalar_output() {
        let mut def = sample_facet();
        assert!(!def.is_scalar());
        def.output = FacetOutput::Scalar {
            target_node: "n1".into(),
            target_prop: "body".into(),
        };
        assert!(def.is_scalar());
    }

    #[test]
    fn apply_scalar_bindings_injects_aggregate_value() {
        use crate::document::BuilderDocument;
        let mut doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                children: vec![Node {
                    id: "label".into(),
                    component: "text".into(),
                    props: json!({"body": "placeholder"}),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        doc.facets.insert(
            "facet:sum".into(),
            FacetDef {
                id: "facet:sum".into(),
                label: "Sum".into(),
                kind: FacetKind::Aggregate {
                    operation: AggregateOp::Sum,
                    field: Some("amount".into()),
                },
                output: FacetOutput::Scalar {
                    target_node: "label".into(),
                    target_prop: "body".into(),
                },
                data: FacetDataSource::Static {
                    items: vec![json!({"amount": 10}), json!({"amount": 20})],
                    records: vec![],
                },
                ..Default::default()
            },
        );
        apply_scalar_bindings(&mut doc);
        let label = doc.root.as_ref().unwrap().find("label").unwrap();
        assert_eq!(label.props["body"], json!(30.0));
    }

    #[test]
    fn apply_scalar_bindings_skips_empty_target() {
        use crate::document::BuilderDocument;
        let mut doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "text".into(),
                props: json!({"body": "original"}),
                ..Default::default()
            }),
            ..Default::default()
        };
        doc.facets.insert(
            "facet:x".into(),
            FacetDef {
                id: "facet:x".into(),
                kind: FacetKind::Aggregate {
                    operation: AggregateOp::Count,
                    field: None,
                },
                output: FacetOutput::Scalar {
                    target_node: String::new(),
                    target_prop: String::new(),
                },
                data: FacetDataSource::Static {
                    items: vec![json!({}), json!({})],
                    records: vec![],
                },
                ..Default::default()
            },
        );
        apply_scalar_bindings(&mut doc);
        assert_eq!(doc.root.as_ref().unwrap().props["body"], json!("original"));
    }

    #[test]
    fn set_component_id_updates_component_ref() {
        let mut def = sample_facet();
        def.set_component_id("hero");
        assert_eq!(def.effective_component_id(), Some("hero"));
    }

    #[test]
    fn set_component_id_noop_for_inline() {
        let mut def = sample_facet();
        def.template = FacetTemplate::Inline {
            root: Box::new(Node {
                id: "tpl".into(),
                component: "text".into(),
                ..Default::default()
            }),
        };
        def.set_component_id("hero");
        assert!(def.is_inline());
    }
}
