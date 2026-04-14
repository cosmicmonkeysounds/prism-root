//! Field resolver — computes values for formula, lookup, and
//! rollup fields.
//!
//! Port of `language/expression/field-resolver.ts`. Stores are
//! duck-typed via small traits so the resolver stays decoupled
//! from the collection-store / tree-model implementation.

use std::collections::HashMap;

use serde_json::Value;

use crate::foundation::object_model::types::{
    EntityFieldDef, EntityFieldType, GraphObject, ObjectEdge, ObjectId, RollupFunction,
};

use super::evaluator::{evaluate_expression, EvaluateResult};
use super::expression_types::ExprValue;

// ── Store traits ──────────────────────────────────────────────────

pub trait EdgeLookup {
    fn get_edges(&self, source_id: &ObjectId, relation: &str) -> Vec<ObjectEdge>;
}

pub trait ObjectLookup {
    fn get_object(&self, id: &ObjectId) -> Option<GraphObject>;
}

pub struct FieldResolverStores<'a> {
    pub edges: &'a dyn EdgeLookup,
    pub objects: &'a dyn ObjectLookup,
}

// ── Helpers ───────────────────────────────────────────────────────

/// Read a value out of a `GraphObject`, honouring a dot-path into
/// `data`. Top-level shell fields take precedence over `data.*`.
pub fn read_object_field(object: &GraphObject, path: &str) -> Option<Value> {
    if path.is_empty() {
        return None;
    }

    // Top-level shell fields (excluding "data" itself).
    if path != "data" {
        if let Some(v) = shell_field(object, path) {
            return Some(v);
        }
    }

    let mut segments = path.split('.');
    let first = segments.next()?;
    let mut current: Option<Value> = object.data.get(first).cloned();
    for seg in segments {
        match current {
            Some(Value::Object(ref map)) => current = map.get(seg).cloned(),
            _ => return None,
        }
    }
    current
}

fn shell_field(object: &GraphObject, path: &str) -> Option<Value> {
    match path {
        "id" => Some(Value::String(object.id.as_str().to_string())),
        "type" => Some(Value::String(object.type_name.clone())),
        "name" => Some(Value::String(object.name.clone())),
        "status" => object.status.clone().map(Value::String),
        "description" => Some(Value::String(object.description.clone())),
        "date" => object.date.clone().map(Value::String),
        "endDate" => object.end_date.clone().map(Value::String),
        "pinned" => Some(Value::Bool(object.pinned)),
        "parentId" => object
            .parent_id
            .as_ref()
            .map(|id| Value::String(id.as_str().to_string())),
        "position" => serde_json::Number::from_f64(object.position).map(Value::Number),
        "tags" => Some(Value::Array(
            object
                .tags
                .iter()
                .map(|t| Value::String(t.clone()))
                .collect(),
        )),
        _ => None,
    }
}

fn to_number_any(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn to_expr_value(v: &Value) -> ExprValue {
    match v {
        Value::Number(n) => ExprValue::Number(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => ExprValue::Boolean(*b),
        Value::String(s) => ExprValue::String(s.clone()),
        Value::Null => ExprValue::Number(0.0),
        _ => ExprValue::String(v.to_string()),
    }
}

/// Build an expression context (shell + data flattened) from a
/// `GraphObject`.
pub fn build_formula_context(object: &GraphObject) -> HashMap<String, ExprValue> {
    let mut ctx: HashMap<String, ExprValue> = HashMap::new();
    ctx.insert(
        "id".into(),
        ExprValue::String(object.id.as_str().to_string()),
    );
    ctx.insert("type".into(), ExprValue::String(object.type_name.clone()));
    ctx.insert("name".into(), ExprValue::String(object.name.clone()));
    ctx.insert(
        "status".into(),
        ExprValue::String(object.status.clone().unwrap_or_default()),
    );
    ctx.insert(
        "description".into(),
        ExprValue::String(object.description.clone()),
    );
    ctx.insert(
        "date".into(),
        ExprValue::String(object.date.clone().unwrap_or_default()),
    );
    ctx.insert(
        "endDate".into(),
        ExprValue::String(object.end_date.clone().unwrap_or_default()),
    );
    ctx.insert("pinned".into(), ExprValue::Boolean(object.pinned));

    for (k, v) in &object.data {
        ctx.insert(k.clone(), to_expr_value(v));
    }
    ctx
}

// ── Resolvers ─────────────────────────────────────────────────────

pub fn resolve_formula_field(object: &GraphObject, field_def: &EntityFieldDef) -> ExprValue {
    let Some(expression) = field_def.expression.as_ref() else {
        return ExprValue::Number(0.0);
    };
    let ctx = build_formula_context(object);
    let EvaluateResult { result, .. } = evaluate_expression(expression, &ctx);
    result
}

pub fn resolve_lookup_field(
    object: &GraphObject,
    field_def: &EntityFieldDef,
    stores: &FieldResolverStores<'_>,
) -> Option<Value> {
    let relation = field_def.lookup_relation.as_deref()?;
    let lookup_field = field_def.lookup_field.as_deref()?;
    let edges = stores.edges.get_edges(&object.id, relation);
    let first_edge = edges.into_iter().next()?;
    let first_target = stores.objects.get_object(&first_edge.target_id)?;
    read_object_field(&first_target, lookup_field)
}

pub fn aggregate(values: &[Option<Value>], fn_: RollupFunction) -> ExprValue {
    if fn_ == RollupFunction::Count {
        return ExprValue::Number(values.len() as f64);
    }
    if fn_ == RollupFunction::List {
        let joined = values
            .iter()
            .map(|v| match v {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        return ExprValue::String(joined);
    }
    if values.is_empty() {
        return ExprValue::Number(0.0);
    }
    let nums: Vec<f64> = values
        .iter()
        .map(|v| v.as_ref().map(to_number_any).unwrap_or(0.0))
        .collect();
    match fn_ {
        RollupFunction::Sum => ExprValue::Number(nums.iter().sum()),
        RollupFunction::Avg => {
            let sum: f64 = nums.iter().sum();
            ExprValue::Number(sum / nums.len() as f64)
        }
        RollupFunction::Min => {
            ExprValue::Number(nums.iter().copied().fold(f64::INFINITY, f64::min))
        }
        RollupFunction::Max => {
            ExprValue::Number(nums.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        }
        RollupFunction::Count | RollupFunction::List => unreachable!("handled above"),
    }
}

pub fn resolve_rollup_field(
    object: &GraphObject,
    field_def: &EntityFieldDef,
    stores: &FieldResolverStores<'_>,
) -> ExprValue {
    let Some(relation) = field_def.rollup_relation.as_deref() else {
        return ExprValue::Number(0.0);
    };
    let Some(rollup_field) = field_def.rollup_field.as_deref() else {
        return ExprValue::Number(0.0);
    };
    let fn_ = field_def.rollup_function.unwrap_or(RollupFunction::Count);
    let edges = stores.edges.get_edges(&object.id, relation);
    let mut values: Vec<Option<Value>> = Vec::with_capacity(edges.len());
    for edge in edges {
        match stores.objects.get_object(&edge.target_id) {
            Some(target) => values.push(read_object_field(&target, rollup_field)),
            None => continue,
        }
    }
    aggregate(&values, fn_)
}

/// Dispatcher — returns `None` if the field isn't a computed type
/// (caller should read the raw stored value instead).
pub fn resolve_computed_field(
    object: &GraphObject,
    field_def: &EntityFieldDef,
    stores: &FieldResolverStores<'_>,
) -> Option<ExprValue> {
    if field_def.expression.is_some() {
        return Some(resolve_formula_field(object, field_def));
    }
    match field_def.field_type {
        EntityFieldType::Lookup => {
            let value = resolve_lookup_field(object, field_def, stores);
            Some(
                value
                    .as_ref()
                    .map(to_expr_value)
                    .unwrap_or(ExprValue::Number(0.0)),
            )
        }
        EntityFieldType::Rollup => Some(resolve_rollup_field(object, field_def, stores)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::{EdgeId, GraphObject, ObjectEdge, ObjectId};
    use serde_json::json;

    struct StaticEdges(Vec<ObjectEdge>);
    impl EdgeLookup for StaticEdges {
        fn get_edges(&self, source_id: &ObjectId, relation: &str) -> Vec<ObjectEdge> {
            self.0
                .iter()
                .filter(|e| &e.source_id == source_id && e.relation == relation)
                .cloned()
                .collect()
        }
    }

    struct StaticObjects(HashMap<String, GraphObject>);
    impl ObjectLookup for StaticObjects {
        fn get_object(&self, id: &ObjectId) -> Option<GraphObject> {
            self.0.get(id.as_str()).cloned()
        }
    }

    fn make_obj(id: &str, name: &str) -> GraphObject {
        GraphObject::new(id, "task", name)
    }

    #[test]
    fn read_shell_field_and_data_path() {
        let mut obj = make_obj("1", "Task");
        obj.data.insert("nested".into(), json!({ "value": 42 }));
        assert_eq!(
            read_object_field(&obj, "name"),
            Some(Value::String("Task".into()))
        );
        assert_eq!(read_object_field(&obj, "nested.value"), Some(json!(42)));
    }

    #[test]
    fn build_formula_context_flattens_data() {
        let mut obj = make_obj("1", "Task");
        obj.status = Some("done".into());
        obj.data.insert("priority".into(), json!(5));
        let ctx = build_formula_context(&obj);
        assert_eq!(ctx.get("status"), Some(&ExprValue::String("done".into())));
        assert_eq!(ctx.get("priority"), Some(&ExprValue::Number(5.0)));
    }

    #[test]
    fn resolve_formula_field_evaluates_expression() {
        let mut obj = make_obj("1", "Task");
        obj.data.insert("a".into(), json!(3));
        obj.data.insert("b".into(), json!(4));
        let field = EntityFieldDef {
            id: "total".into(),
            field_type: EntityFieldType::Float,
            label: None,
            description: None,
            required: None,
            default: None,
            expression: Some("a + b".into()),
            enum_options: None,
            ref_types: None,
            lookup_relation: None,
            lookup_field: None,
            rollup_relation: None,
            rollup_field: None,
            rollup_function: None,
            ui: None,
        };
        let v = resolve_formula_field(&obj, &field);
        assert_eq!(v, ExprValue::Number(7.0));
    }

    #[test]
    fn aggregate_sum_and_count() {
        let vals = vec![Some(json!(1)), Some(json!(2)), Some(json!(3))];
        assert_eq!(
            aggregate(&vals, RollupFunction::Sum),
            ExprValue::Number(6.0)
        );
        assert_eq!(
            aggregate(&vals, RollupFunction::Count),
            ExprValue::Number(3.0)
        );
        assert_eq!(
            aggregate(&vals, RollupFunction::Avg),
            ExprValue::Number(2.0)
        );
    }

    #[test]
    fn aggregate_list_joins_with_comma() {
        let vals = vec![
            Some(Value::String("a".into())),
            Some(Value::String("b".into())),
        ];
        assert_eq!(
            aggregate(&vals, RollupFunction::List),
            ExprValue::String("a, b".into())
        );
    }

    #[test]
    fn resolve_lookup_field_returns_first_target_value() {
        let parent = make_obj("p", "Parent");
        let mut child = make_obj("c", "Child");
        child.data.insert("name_alias".into(), json!("hello"));
        let edge = ObjectEdge {
            id: EdgeId::new("e"),
            source_id: ObjectId::new("p"),
            target_id: ObjectId::new("c"),
            relation: "ref".to_string(),
            position: None,
            created_at: chrono::Utc::now(),
            data: Default::default(),
        };
        let edges = StaticEdges(vec![edge]);
        let mut objs = HashMap::new();
        objs.insert("c".to_string(), child);
        let stores = FieldResolverStores {
            edges: &edges,
            objects: &StaticObjects(objs),
        };
        let field = EntityFieldDef {
            id: "alias".into(),
            field_type: EntityFieldType::Lookup,
            label: None,
            description: None,
            required: None,
            default: None,
            expression: None,
            enum_options: None,
            ref_types: None,
            lookup_relation: Some("ref".into()),
            lookup_field: Some("name_alias".into()),
            rollup_relation: None,
            rollup_field: None,
            rollup_function: None,
            ui: None,
        };
        let v = resolve_lookup_field(&parent, &field, &stores);
        assert_eq!(v, Some(Value::String("hello".into())));
    }
}
