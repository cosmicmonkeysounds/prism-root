//! Resolution-time helpers: scalar bindings, expression interpolation,
//! filter parsing, calculation evaluation.

use std::collections::HashMap;

use serde_json::Value;

use prism_core::language::expression::{evaluate_expression, ExprValue};
use prism_core::widget::{get_json_field, FilterOp, QueryFilter};

use crate::document::{Node, NodeId};
use crate::mutator::NodeMutator;
use crate::registry::FieldKind;

use super::*;

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
        let mutator = NodeMutator::new();
        for (node_id, prop_key, val) in pairs {
            mutator.write_at(root, &node_id, &prop_key, val);
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
                ExprValue::Null => Value::Null,
            };
            obj.insert((*key).to_string(), json_val);
        }
    }
}

// ── Slint component ───────────────────────────────────────────────────────────
