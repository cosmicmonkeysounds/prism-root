//! Data sources, bindings, layout/direction, and variant rules.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use prism_core::widget::DataQuery;

use crate::resource::ResourceId;

use super::*;

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
