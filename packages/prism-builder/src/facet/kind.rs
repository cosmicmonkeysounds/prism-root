//! Facet kind enums and aggregation ops.


use serde::{Deserialize, Serialize};
use serde_json::Value;

use prism_core::language::visual::ScriptGraph;
use prism_core::widget::{get_json_field, DataQuery};



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

