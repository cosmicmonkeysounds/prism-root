//! Object-collection query pipeline (filter → sort → group → limit).
//!
//! Pure functions over `&[GraphObject]`. The TS version returned
//! `unknown` from field access; here field values are
//! `serde_json::Value` so every operator can be expressed without
//! resorting to downcasts. Shell fields are exposed under their TS
//! names (`parentId`, `endDate`, `createdAt`, `deletedAt`,
//! `updatedAt`) so existing saved-query documents round-trip.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::foundation::object_model::types::GraphObject;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterOp {
    Eq,
    Neq,
    Contains,
    Starts,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Nin,
    Empty,
    Notempty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterConfig {
    pub field: String,
    pub op: FilterOp,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub value: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortConfig {
    pub field: String,
    pub dir: SortDir,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupConfig {
    pub field: String,
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupedResult {
    pub key: String,
    pub label: String,
    pub objects: Vec<GraphObject>,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Query {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<FilterConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sorts: Vec<SortConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// `None` = default-exclude soft-deleted rows (matches TS default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_deleted: Option<bool>,
}

/// Read a field by its TS-visible name. Shell fields first, then the
/// `data` payload. Returns `Value::Null` when the field is missing.
pub fn get_field_value(obj: &GraphObject, field: &str) -> Value {
    match field {
        "id" => Value::String(obj.id.as_str().to_string()),
        "type" => Value::String(obj.type_name.clone()),
        "name" => Value::String(obj.name.clone()),
        "parentId" => obj
            .parent_id
            .as_ref()
            .map(|p| Value::String(p.as_str().to_string()))
            .unwrap_or(Value::Null),
        "position" => serde_json::Number::from_f64(obj.position)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        "status" => obj
            .status
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
        "tags" => Value::Array(obj.tags.iter().cloned().map(Value::String).collect()),
        "date" => obj
            .date
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
        "endDate" => obj
            .end_date
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
        "description" => Value::String(obj.description.clone()),
        "color" => obj
            .color
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
        "image" => obj
            .image
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
        "pinned" => Value::Bool(obj.pinned),
        "createdAt" => Value::String(obj.created_at.to_rfc3339()),
        "updatedAt" => Value::String(obj.updated_at.to_rfc3339()),
        "deletedAt" => obj
            .deleted_at
            .map(|d| Value::String(d.to_rfc3339()))
            .unwrap_or(Value::Null),
        _ => obj.data.get(field).cloned().unwrap_or(Value::Null),
    }
}

fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

fn value_lt(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Null, Value::Null) => Some(std::cmp::Ordering::Equal),
        (Value::Null, _) => Some(std::cmp::Ordering::Less),
        (_, Value::Null) => Some(std::cmp::Ordering::Greater),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        (Value::Number(_), Value::Number(_)) => match (value_as_f64(a), value_as_f64(b)) {
            (Some(x), Some(y)) => x.partial_cmp(&y),
            _ => None,
        },
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

fn matches_filter(obj: &GraphObject, filter: &FilterConfig) -> bool {
    let actual = get_field_value(obj, &filter.field);
    let expected = &filter.value;

    match filter.op {
        FilterOp::Empty => is_empty_value(&actual),
        FilterOp::Notempty => !is_empty_value(&actual),
        FilterOp::Eq => actual == *expected,
        FilterOp::Neq => actual != *expected,
        FilterOp::Contains => match (&actual, expected) {
            (Value::String(a), Value::String(e)) => a.to_lowercase().contains(&e.to_lowercase()),
            (Value::Array(a), e) => a.contains(e),
            _ => false,
        },
        FilterOp::Starts => match (&actual, expected) {
            (Value::String(a), Value::String(e)) => a.to_lowercase().starts_with(&e.to_lowercase()),
            _ => false,
        },
        FilterOp::Gt | FilterOp::Gte | FilterOp::Lt | FilterOp::Lte => {
            if actual.is_null() || expected.is_null() {
                return false;
            }
            let Some(order) = value_lt(&actual, expected) else {
                return false;
            };
            match filter.op {
                FilterOp::Gt => order == std::cmp::Ordering::Greater,
                FilterOp::Gte => order != std::cmp::Ordering::Less,
                FilterOp::Lt => order == std::cmp::Ordering::Less,
                FilterOp::Lte => order != std::cmp::Ordering::Greater,
                _ => unreachable!(),
            }
        }
        FilterOp::In => match expected {
            Value::Array(items) => items.iter().any(|v| v == &actual),
            _ => false,
        },
        FilterOp::Nin => match expected {
            Value::Array(items) => !items.iter().any(|v| v == &actual),
            _ => true,
        },
    }
}

pub fn apply_filters(objects: &[GraphObject], filters: &[FilterConfig]) -> Vec<GraphObject> {
    if filters.is_empty() {
        return objects.to_vec();
    }
    objects
        .iter()
        .filter(|o| filters.iter().all(|f| matches_filter(o, f)))
        .cloned()
        .collect()
}

pub fn apply_sorts(objects: &[GraphObject], sorts: &[SortConfig]) -> Vec<GraphObject> {
    if sorts.is_empty() {
        return objects.to_vec();
    }
    let mut out = objects.to_vec();
    out.sort_by(|a, b| {
        for sort in sorts {
            let av = get_field_value(a, &sort.field);
            let bv = get_field_value(b, &sort.field);
            let ord = value_lt(&av, &bv).unwrap_or(std::cmp::Ordering::Equal);
            if ord == std::cmp::Ordering::Equal {
                continue;
            }
            return match sort.dir {
                SortDir::Asc => ord,
                SortDir::Desc => ord.reverse(),
            };
        }
        std::cmp::Ordering::Equal
    });
    out
}

pub fn apply_groups(objects: &[GraphObject], groups: &[GroupConfig]) -> Vec<GroupedResult> {
    if groups.is_empty() {
        return vec![GroupedResult {
            key: "__all__".into(),
            label: "All".into(),
            objects: objects.to_vec(),
            collapsed: false,
        }];
    }
    let group = &groups[0];
    let mut order: Vec<String> = Vec::new();
    let mut buckets: std::collections::HashMap<String, Vec<GraphObject>> =
        std::collections::HashMap::new();

    for obj in objects {
        let raw = get_field_value(obj, &group.field);
        let key = match raw {
            Value::Null => "__none__".to_string(),
            Value::String(s) => s,
            other => other.to_string(),
        };
        if !buckets.contains_key(&key) {
            order.push(key.clone());
        }
        buckets.entry(key).or_default().push(obj.clone());
    }

    order
        .into_iter()
        .map(|key| {
            let label = if key == "__none__" {
                "None".to_string()
            } else {
                key.clone()
            };
            let objects = buckets.remove(&key).unwrap_or_default();
            GroupedResult {
                key,
                label,
                objects,
                collapsed: group.collapsed,
            }
        })
        .collect()
}

pub fn apply_query(objects: &[GraphObject], query: &Query) -> Vec<GraphObject> {
    let mut result: Vec<GraphObject> = objects.to_vec();
    let exclude_deleted = query.exclude_deleted.unwrap_or(true);
    if exclude_deleted {
        result.retain(|o| o.deleted_at.is_none());
    }
    if !query.filters.is_empty() {
        result = apply_filters(&result, &query.filters);
    }
    if !query.sorts.is_empty() {
        result = apply_sorts(&result, &query.sorts);
    }
    if let Some(limit) = query.limit {
        result.truncate(limit);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::{GraphObject, ObjectId};
    use chrono::Utc;
    use serde_json::json;

    fn mk(id: &str, name: &str) -> GraphObject {
        GraphObject {
            id: ObjectId::new(id),
            type_name: "task".into(),
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
            data: Default::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        }
    }

    #[test]
    fn get_field_value_resolves_shell_fields() {
        let mut o = mk("1", "hello");
        o.status = Some("done".into());
        o.pinned = true;
        o.tags = vec!["a".into(), "b".into()];
        assert_eq!(get_field_value(&o, "name"), json!("hello"));
        assert_eq!(get_field_value(&o, "status"), json!("done"));
        assert_eq!(get_field_value(&o, "pinned"), json!(true));
        assert_eq!(get_field_value(&o, "tags"), json!(["a", "b"]));
        assert_eq!(get_field_value(&o, "parentId"), Value::Null);
    }

    #[test]
    fn get_field_value_falls_back_to_data_payload() {
        let mut o = mk("1", "task");
        o.data.insert("priority".into(), json!("high"));
        assert_eq!(get_field_value(&o, "priority"), json!("high"));
        assert_eq!(get_field_value(&o, "missing"), Value::Null);
    }

    #[test]
    fn eq_neq_filters() {
        let mut a = mk("1", "a");
        a.status = Some("done".into());
        let mut b = mk("2", "b");
        b.status = Some("todo".into());
        let objs = vec![a, b];
        let hit = apply_filters(
            &objs,
            &[FilterConfig {
                field: "status".into(),
                op: FilterOp::Eq,
                value: json!("done"),
            }],
        );
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].name, "a");

        let miss = apply_filters(
            &objs,
            &[FilterConfig {
                field: "status".into(),
                op: FilterOp::Neq,
                value: json!("done"),
            }],
        );
        assert_eq!(miss.len(), 1);
        assert_eq!(miss[0].name, "b");
    }

    #[test]
    fn contains_is_case_insensitive_for_strings() {
        let a = mk("1", "Hello World");
        let b = mk("2", "Goodbye");
        let hit = apply_filters(
            &[a, b],
            &[FilterConfig {
                field: "name".into(),
                op: FilterOp::Contains,
                value: json!("WORLD"),
            }],
        );
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].name, "Hello World");
    }

    #[test]
    fn contains_on_array_matches_any() {
        let mut a = mk("1", "a");
        a.tags = vec!["urgent".into(), "bug".into()];
        let b = mk("2", "b");
        let hit = apply_filters(
            &[a, b],
            &[FilterConfig {
                field: "tags".into(),
                op: FilterOp::Contains,
                value: json!("urgent"),
            }],
        );
        assert_eq!(hit.len(), 1);
    }

    #[test]
    fn empty_and_notempty_operators() {
        let mut with_status = mk("1", "a");
        with_status.status = Some("done".into());
        let bare = mk("2", "b");
        let objs = vec![with_status, bare];
        let empty = apply_filters(
            &objs,
            &[FilterConfig {
                field: "status".into(),
                op: FilterOp::Empty,
                value: Value::Null,
            }],
        );
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].name, "b");
        let notempty = apply_filters(
            &objs,
            &[FilterConfig {
                field: "status".into(),
                op: FilterOp::Notempty,
                value: Value::Null,
            }],
        );
        assert_eq!(notempty.len(), 1);
    }

    #[test]
    fn gt_gte_lt_lte_work_on_numbers() {
        let mut a = mk("1", "a");
        a.data.insert("score".into(), json!(10));
        let mut b = mk("2", "b");
        b.data.insert("score".into(), json!(20));
        let objs = vec![a, b];
        let hit = apply_filters(
            &objs,
            &[FilterConfig {
                field: "score".into(),
                op: FilterOp::Gt,
                value: json!(15),
            }],
        );
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].name, "b");
    }

    #[test]
    fn in_and_nin_operators() {
        let mut a = mk("1", "a");
        a.status = Some("todo".into());
        let mut b = mk("2", "b");
        b.status = Some("doing".into());
        let mut c = mk("3", "c");
        c.status = Some("done".into());
        let objs = vec![a, b, c];
        let hit = apply_filters(
            &objs,
            &[FilterConfig {
                field: "status".into(),
                op: FilterOp::In,
                value: json!(["todo", "done"]),
            }],
        );
        assert_eq!(hit.len(), 2);
        let miss = apply_filters(
            &objs,
            &[FilterConfig {
                field: "status".into(),
                op: FilterOp::Nin,
                value: json!(["todo", "done"]),
            }],
        );
        assert_eq!(miss.len(), 1);
        assert_eq!(miss[0].name, "b");
    }

    #[test]
    fn sort_ascending_and_descending() {
        let mut a = mk("1", "alpha");
        a.position = 3.0;
        let mut b = mk("2", "beta");
        b.position = 1.0;
        let mut c = mk("3", "gamma");
        c.position = 2.0;
        let objs = vec![a, b, c];
        let asc = apply_sorts(
            &objs,
            &[SortConfig {
                field: "position".into(),
                dir: SortDir::Asc,
            }],
        );
        assert_eq!(
            asc.iter().map(|o| o.name.clone()).collect::<Vec<_>>(),
            vec!["beta", "gamma", "alpha"]
        );
        let desc = apply_sorts(
            &objs,
            &[SortConfig {
                field: "position".into(),
                dir: SortDir::Desc,
            }],
        );
        assert_eq!(
            desc.iter().map(|o| o.name.clone()).collect::<Vec<_>>(),
            vec!["alpha", "gamma", "beta"]
        );
    }

    #[test]
    fn sort_multi_key_uses_stable_ordering() {
        let mut a = mk("1", "a");
        a.status = Some("done".into());
        a.position = 2.0;
        let mut b = mk("2", "b");
        b.status = Some("done".into());
        b.position = 1.0;
        let mut c = mk("3", "c");
        c.status = Some("todo".into());
        c.position = 3.0;
        let out = apply_sorts(
            &[a, b, c],
            &[
                SortConfig {
                    field: "status".into(),
                    dir: SortDir::Asc,
                },
                SortConfig {
                    field: "position".into(),
                    dir: SortDir::Asc,
                },
            ],
        );
        assert_eq!(
            out.iter().map(|o| o.name.clone()).collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );
    }

    #[test]
    fn group_by_field_partitions_in_first_occurrence_order() {
        let mut a = mk("1", "a");
        a.status = Some("done".into());
        let mut b = mk("2", "b");
        b.status = Some("todo".into());
        let mut c = mk("3", "c");
        c.status = Some("done".into());
        let groups = apply_groups(
            &[a, b, c],
            &[GroupConfig {
                field: "status".into(),
                collapsed: false,
            }],
        );
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "done");
        assert_eq!(groups[0].objects.len(), 2);
        assert_eq!(groups[1].key, "todo");
        assert_eq!(groups[1].objects.len(), 1);
    }

    #[test]
    fn group_maps_null_to_none_bucket() {
        let a = mk("1", "a");
        let mut b = mk("2", "b");
        b.status = Some("done".into());
        let groups = apply_groups(
            &[a, b],
            &[GroupConfig {
                field: "status".into(),
                collapsed: true,
            }],
        );
        let none_group = groups.iter().find(|g| g.key == "__none__").unwrap();
        assert_eq!(none_group.label, "None");
        assert!(none_group.collapsed);
    }

    #[test]
    fn group_with_empty_config_returns_single_all_bucket() {
        let a = mk("1", "a");
        let groups = apply_groups(&[a], &[]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key, "__all__");
        assert_eq!(groups[0].label, "All");
    }

    #[test]
    fn apply_query_pipeline_respects_exclude_deleted_and_limit() {
        let mut a = mk("1", "a");
        a.data.insert("score".into(), json!(10));
        let mut b = mk("2", "b");
        b.data.insert("score".into(), json!(30));
        b.deleted_at = Some(Utc::now());
        let mut c = mk("3", "c");
        c.data.insert("score".into(), json!(20));

        let query = Query {
            filters: vec![FilterConfig {
                field: "score".into(),
                op: FilterOp::Gt,
                value: json!(5),
            }],
            sorts: vec![SortConfig {
                field: "score".into(),
                dir: SortDir::Desc,
            }],
            limit: Some(1),
            ..Default::default()
        };
        let out = apply_query(&[a, b, c], &query);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "c"); // b is soft-deleted so out of the running
    }

    #[test]
    fn apply_query_exclude_deleted_false_includes_tombstones() {
        let mut a = mk("1", "a");
        a.deleted_at = Some(Utc::now());
        let query = Query {
            exclude_deleted: Some(false),
            ..Default::default()
        };
        let out = apply_query(&[a], &query);
        assert_eq!(out.len(), 1);
    }
}
