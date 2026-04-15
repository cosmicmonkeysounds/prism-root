//! `automation::condition` — pure condition tree evaluator + template
//! interpolation + object-trigger matching.
//!
//! Port of `kernel/automation/condition-evaluator.ts`. Works on
//! `serde_json::Value` instead of a TS `unknown`; keeps the 10-operator
//! `compare` surface and the `{{dot.path}}` template interpolator.

use regex::Regex;
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::types::{
    AutomationCondition, AutomationContext, FieldOperator, ObjectTriggerFilter, TagMode,
};

/// Resolve a dot-path against an arbitrary JSON value.
///
/// Returns `None` when any intermediate key is missing or non-object.
pub fn get_path<'a>(value: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    let mut cur = value;
    for key in path.split('.') {
        cur = match cur {
            JsonValue::Object(map) => map.get(key)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Build the JSON view of the context used by [`get_path`] + [`interpolate`].
pub fn context_to_json(ctx: &AutomationContext) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert(
        "automationId".into(),
        JsonValue::String(ctx.automation_id.clone()),
    );
    map.insert(
        "triggeredAt".into(),
        JsonValue::String(ctx.triggered_at.clone()),
    );
    map.insert(
        "triggerType".into(),
        JsonValue::String(ctx.trigger_type.clone()),
    );
    if let Some(obj) = &ctx.object {
        map.insert("object".into(), JsonValue::Object(obj.clone()));
    }
    if let Some(prev) = &ctx.previous_object {
        map.insert("previousObject".into(), JsonValue::Object(prev.clone()));
    }
    if let Some(extra) = &ctx.extra {
        map.insert("extra".into(), JsonValue::Object(extra.clone()));
    }
    JsonValue::Object(map)
}

/// Compare two JSON values with the given operator. Mirrors the loose
/// semantics of the TS port — numeric comparisons fall back to `0` on
/// non-number operands, stringy operators coerce via `to_display_string`.
pub fn compare(actual: Option<&JsonValue>, op: FieldOperator, expected: &JsonValue) -> bool {
    match op {
        FieldOperator::Eq => match actual {
            Some(v) => v == expected,
            None => expected.is_null(),
        },
        FieldOperator::Neq => match actual {
            Some(v) => v != expected,
            None => !expected.is_null(),
        },
        FieldOperator::Gt => numeric(actual) > numeric(Some(expected)),
        FieldOperator::Gte => numeric(actual) >= numeric(Some(expected)),
        FieldOperator::Lt => numeric(actual) < numeric(Some(expected)),
        FieldOperator::Lte => numeric(actual) <= numeric(Some(expected)),
        FieldOperator::Contains => {
            to_display_string(actual).contains(&to_display_string(Some(expected)))
        }
        FieldOperator::StartsWith => {
            to_display_string(actual).starts_with(&to_display_string(Some(expected)))
        }
        FieldOperator::EndsWith => {
            to_display_string(actual).ends_with(&to_display_string(Some(expected)))
        }
        FieldOperator::Matches => {
            let pattern = to_display_string(Some(expected));
            match Regex::new(&pattern) {
                Ok(re) => re.is_match(&to_display_string(actual)),
                Err(_) => false,
            }
        }
    }
}

fn numeric(v: Option<&JsonValue>) -> f64 {
    match v {
        Some(JsonValue::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(JsonValue::String(s)) => s.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn to_display_string(v: Option<&JsonValue>) -> String {
    match v {
        None | Some(JsonValue::Null) => String::new(),
        Some(JsonValue::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// Evaluate a condition tree against a context.
pub fn evaluate_condition(cond: &AutomationCondition, ctx: &AutomationContext) -> bool {
    let ctx_json = context_to_json(ctx);
    evaluate_inner(cond, &ctx_json, ctx)
}

fn evaluate_inner(
    cond: &AutomationCondition,
    ctx_json: &JsonValue,
    ctx: &AutomationContext,
) -> bool {
    match cond {
        AutomationCondition::Field {
            path,
            operator,
            value,
        } => {
            let actual = get_path(ctx_json, path);
            compare(actual, *operator, value)
        }
        AutomationCondition::Type { object_type } => ctx
            .object
            .as_ref()
            .and_then(|o| o.get("type"))
            .and_then(|v| v.as_str())
            .map(|s| s == object_type)
            .unwrap_or(false),
        AutomationCondition::Tags { tags, mode } => {
            let obj_tags: Vec<&str> = ctx
                .object
                .as_ref()
                .and_then(|o| o.get("tags"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|t| t.as_str()).collect())
                .unwrap_or_default();
            match mode {
                TagMode::All => tags.iter().all(|t| obj_tags.iter().any(|o| o == t)),
                TagMode::Any => tags.iter().any(|t| obj_tags.iter().any(|o| o == t)),
            }
        }
        AutomationCondition::And { conditions } => {
            conditions.iter().all(|c| evaluate_inner(c, ctx_json, ctx))
        }
        AutomationCondition::Or { conditions } => {
            conditions.iter().any(|c| evaluate_inner(c, ctx_json, ctx))
        }
        AutomationCondition::Not { condition } => !evaluate_inner(condition, ctx_json, ctx),
    }
}

/// Replace `{{dot.path}}` placeholders in a JSON template with values
/// pulled from `ctx`. Matches the TS port: objects and arrays recurse,
/// strings get pattern-replaced, everything else passes through.
pub fn interpolate(template: &JsonValue, ctx: &AutomationContext) -> JsonValue {
    let ctx_json = context_to_json(ctx);
    interpolate_inner(template, &ctx_json)
}

fn interpolate_inner(value: &JsonValue, ctx_json: &JsonValue) -> JsonValue {
    match value {
        JsonValue::String(s) => JsonValue::String(interpolate_string(s, ctx_json)),
        JsonValue::Array(arr) => {
            JsonValue::Array(arr.iter().map(|v| interpolate_inner(v, ctx_json)).collect())
        }
        JsonValue::Object(map) => {
            let mut out = JsonMap::new();
            for (k, v) in map {
                out.insert(k.clone(), interpolate_inner(v, ctx_json));
            }
            JsonValue::Object(out)
        }
        other => other.clone(),
    }
}

fn interpolate_string(src: &str, ctx_json: &JsonValue) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = find_close(&bytes[i + 2..]) {
                let path = &src[i + 2..i + 2 + end];
                if path
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
                {
                    let resolved = get_path(ctx_json, path)
                        .map(value_to_text)
                        .unwrap_or_default();
                    out.push_str(&resolved);
                    i += 2 + end + 2;
                    continue;
                }
            }
        }
        let ch = src[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn find_close(rest: &[u8]) -> Option<usize> {
    (0..rest.len().saturating_sub(1)).find(|&i| rest[i] == b'}' && rest[i + 1] == b'}')
}

fn value_to_text(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => String::new(),
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Check if an object event's payload passes an object-trigger filter.
pub fn matches_object_trigger(
    filter: &ObjectTriggerFilter,
    object: &JsonMap<String, JsonValue>,
) -> bool {
    if let Some(types) = filter.object_types.as_ref().filter(|t| !t.is_empty()) {
        let obj_type = object.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !types.iter().any(|t| t == obj_type) {
            return false;
        }
    }
    if let Some(tags) = filter.tags.as_ref().filter(|t| !t.is_empty()) {
        let obj_tags: Vec<&str> = object
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|t| t.as_str()).collect())
            .unwrap_or_default();
        if !tags.iter().all(|t| obj_tags.iter().any(|o| o == t)) {
            return false;
        }
    }
    if let Some(fields) = &filter.field_match {
        for (k, expected) in fields {
            if object.get(k) != Some(expected) {
                return false;
            }
        }
    }
    true
}
