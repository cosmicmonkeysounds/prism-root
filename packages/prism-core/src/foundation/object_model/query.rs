//! `ObjectQuery` — typed query descriptor for filtering and
//! sorting objects. Port of `foundation/object-model/query.ts`.
//!
//! A single query type travels through the Loro store, the relay
//! REST endpoints, and daemon IPC calls so the Rust shape must
//! round-trip through serde exactly the same way the TS shape
//! round-trips through JSON.

use serde::{Deserialize, Serialize};

use super::types::{GraphObject, ObjectId};

// ── ObjectQuery ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ObjectQuery {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none", default)]
    pub type_name: Option<StringOrList>,
    #[serde(rename = "parentId", skip_serializing_if = "Option::is_none", default)]
    pub parent_id: Option<ParentFilter>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<StringOrList>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tags: Option<Vec<String>>,
    #[serde(rename = "dateAfter", skip_serializing_if = "Option::is_none", default)]
    pub date_after: Option<String>,
    #[serde(
        rename = "dateBefore",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub date_before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub offset: Option<usize>,
    #[serde(rename = "sortBy", skip_serializing_if = "Option::is_none", default)]
    pub sort_by: Option<SortField>,
    #[serde(rename = "sortDir", skip_serializing_if = "Option::is_none", default)]
    pub sort_dir: Option<SortDir>,
    #[serde(
        rename = "includeDeleted",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub include_deleted: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrList {
    One(String),
    Many(Vec<String>),
}

impl StringOrList {
    pub fn contains(&self, value: &str) -> bool {
        match self {
            Self::One(s) => s == value,
            Self::Many(v) => v.iter().any(|s| s == value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParentFilter {
    Root,
    Id(ObjectId),
}

// Legacy `parentId: null` is represented by `ParentFilter::Root`.
// Deserialization from `null` is handled manually because serde's
// default untagged resolution picks `Id("")` for `""` vs `null`.
impl ParentFilter {
    pub fn matches(&self, value: Option<&ObjectId>) -> bool {
        match (self, value) {
            (Self::Root, None) => true,
            (Self::Root, Some(_)) => false,
            (Self::Id(target), Some(v)) => target == v,
            (Self::Id(_), None) => false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortField {
    Name,
    Date,
    CreatedAt,
    UpdatedAt,
    #[default]
    Position,
    Status,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

// ── Filter operators (shell level) ─────────────────────────────────
// The legacy persistence layer also exposes a richer `ObjectFilter`
// for materialised view queries. The shell-level version keeps the
// same name so downstream code can keep importing the symbol.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectFilter {
    pub field: String,
    pub op: ObjectFilterOp,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObjectFilterOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
    In,
    NotIn,
    Contains,
    StartsWith,
    EndsWith,
    Exists,
}

// ── Matching + sorting ─────────────────────────────────────────────

pub fn matches_query(obj: &GraphObject, query: &ObjectQuery) -> bool {
    if !query.include_deleted.unwrap_or(false) && obj.deleted_at.is_some() {
        return false;
    }

    if let Some(type_filter) = query.type_name.as_ref() {
        if !type_filter.contains(&obj.type_name) {
            return false;
        }
    }

    if let Some(parent_filter) = query.parent_id.as_ref() {
        if !parent_filter.matches(obj.parent_id.as_ref()) {
            return false;
        }
    }

    if let Some(status_filter) = query.status.as_ref() {
        match obj.status.as_ref() {
            None => return false,
            Some(s) if !status_filter.contains(s) => return false,
            _ => {}
        }
    }

    if let Some(tag_filter) = query.tags.as_ref() {
        if !tag_filter.iter().all(|t| obj.tags.contains(t)) {
            return false;
        }
    }

    if let Some(pinned) = query.pinned {
        if obj.pinned != pinned {
            return false;
        }
    }

    if let Some(after) = query.date_after.as_ref() {
        if let Some(d) = obj.date.as_ref() {
            if d.as_str() < after.as_str() {
                return false;
            }
        }
    }
    if let Some(before) = query.date_before.as_ref() {
        if let Some(d) = obj.date.as_ref() {
            if d.as_str() > before.as_str() {
                return false;
            }
        }
    }

    if let Some(search) = query.search.as_ref() {
        let needle = search.to_lowercase();
        let in_name = obj.name.to_lowercase().contains(&needle);
        let in_desc = obj.description.to_lowercase().contains(&needle);
        if !in_name && !in_desc {
            return false;
        }
    }

    true
}

pub fn matches_filter(obj: &GraphObject, filter: &ObjectFilter) -> bool {
    // Shell field lookup only — payload fields can be addressed
    // via `data.*` in a follow-up.
    let value = match filter.field.as_str() {
        "id" => serde_json::Value::String(obj.id.as_str().to_string()),
        "type" => serde_json::Value::String(obj.type_name.clone()),
        "name" => serde_json::Value::String(obj.name.clone()),
        "status" => obj
            .status
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
        "pinned" => serde_json::Value::Bool(obj.pinned),
        "description" => serde_json::Value::String(obj.description.clone()),
        _ => serde_json::Value::Null,
    };

    use ObjectFilterOp::*;
    match filter.op {
        Eq => value == filter.value,
        Ne => value != filter.value,
        Lt | Lte | Gt | Gte => compare_numeric(&value, &filter.value, filter.op),
        In => filter
            .value
            .as_array()
            .map(|arr| arr.contains(&value))
            .unwrap_or(false),
        NotIn => !filter
            .value
            .as_array()
            .map(|arr| arr.contains(&value))
            .unwrap_or(false),
        Contains => filter
            .value
            .as_str()
            .and_then(|needle| value.as_str().map(|hay| hay.contains(needle)))
            .unwrap_or(false),
        StartsWith => filter
            .value
            .as_str()
            .and_then(|needle| value.as_str().map(|hay| hay.starts_with(needle)))
            .unwrap_or(false),
        EndsWith => filter
            .value
            .as_str()
            .and_then(|needle| value.as_str().map(|hay| hay.ends_with(needle)))
            .unwrap_or(false),
        Exists => !value.is_null(),
    }
}

fn compare_numeric(a: &serde_json::Value, b: &serde_json::Value, op: ObjectFilterOp) -> bool {
    let (Some(av), Some(bv)) = (a.as_f64(), b.as_f64()) else {
        return false;
    };
    match op {
        ObjectFilterOp::Lt => av < bv,
        ObjectFilterOp::Lte => av <= bv,
        ObjectFilterOp::Gt => av > bv,
        ObjectFilterOp::Gte => av >= bv,
        _ => false,
    }
}

pub fn apply_filters<'a, I>(objects: I, filters: &[ObjectFilter]) -> Vec<GraphObject>
where
    I: IntoIterator<Item = &'a GraphObject>,
{
    objects
        .into_iter()
        .filter(|obj| filters.iter().all(|f| matches_filter(obj, f)))
        .cloned()
        .collect()
}

pub fn sort_objects(objects: &mut [GraphObject], query: &ObjectQuery) {
    let field = query.sort_by.unwrap_or_default();
    let asc = query.sort_dir.unwrap_or_default() == SortDir::Asc;
    objects.sort_by(|a, b| {
        let ord = compare_field(a, b, field);
        if asc {
            ord
        } else {
            ord.reverse()
        }
    });
}

fn compare_field(a: &GraphObject, b: &GraphObject, field: SortField) -> std::cmp::Ordering {
    use SortField::*;
    match field {
        Name => a.name.cmp(&b.name),
        Date => a.date.cmp(&b.date),
        CreatedAt => a.created_at.cmp(&b.created_at),
        UpdatedAt => a.updated_at.cmp(&b.updated_at),
        Position => a
            .position
            .partial_cmp(&b.position)
            .unwrap_or(std::cmp::Ordering::Equal),
        Status => a.status.cmp(&b.status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::ObjectId;

    fn obj(id: &str, type_name: &str, name: &str) -> GraphObject {
        GraphObject::new(id, type_name, name)
    }

    #[test]
    fn matches_by_type() {
        let a = obj("1", "task", "A");
        let q = ObjectQuery {
            type_name: Some(StringOrList::One("task".into())),
            ..Default::default()
        };
        assert!(matches_query(&a, &q));
    }

    #[test]
    fn matches_parent_root() {
        let a = obj("1", "task", "A");
        let q = ObjectQuery {
            parent_id: Some(ParentFilter::Root),
            ..Default::default()
        };
        assert!(matches_query(&a, &q));

        let mut b = obj("2", "task", "B");
        b.parent_id = Some(ObjectId::new("root"));
        assert!(!matches_query(&b, &q));
    }

    #[test]
    fn search_matches_name_or_description() {
        let mut a = obj("1", "task", "Buy milk");
        a.description = "for dinner".into();
        let q = ObjectQuery {
            search: Some("dinner".into()),
            ..Default::default()
        };
        assert!(matches_query(&a, &q));
    }

    #[test]
    fn sort_objects_by_name_desc() {
        let mut items = vec![
            obj("1", "task", "B"),
            obj("2", "task", "A"),
            obj("3", "task", "C"),
        ];
        let q = ObjectQuery {
            sort_by: Some(SortField::Name),
            sort_dir: Some(SortDir::Desc),
            ..Default::default()
        };
        sort_objects(&mut items, &q);
        assert_eq!(items[0].name, "C");
        assert_eq!(items[2].name, "A");
    }
}
