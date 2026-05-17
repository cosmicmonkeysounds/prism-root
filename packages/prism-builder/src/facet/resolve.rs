//! `{{field}}` expression interpolation for facet templates.

use serde_json::Value;

use prism_core::widget::get_json_field;

use crate::document::Node;

/// Resolve `{{field}}` expressions in a node subtree against a data item.
///
/// Walks every string-valued prop on the node (and its children). Any
/// occurrence of `{{path}}` is replaced with the corresponding value
/// from the data item (via dot-notation `get_json_field`). If the entire
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(props: Value) -> Node {
        Node {
            id: "n".into(),
            component: "text".into(),
            props,
            ..Default::default()
        }
    }

    #[test]
    fn whole_value_expression_preserves_raw_type() {
        let mut n = node(json!({ "count": "{{n}}" }));
        resolve_template_expressions(&mut n, &json!({ "n": 7 }));
        assert_eq!(n.props["count"], json!(7));
    }

    #[test]
    fn mixed_text_interpolates_as_string() {
        let mut n = node(json!({ "body": "Hi {{name}}!" }));
        resolve_template_expressions(&mut n, &json!({ "name": "Ada" }));
        assert_eq!(n.props["body"], json!("Hi Ada!"));
    }

    #[test]
    fn record_prefix_is_stripped_and_children_recurse() {
        let mut n = node(json!({ "body": "{{record.title}}" }));
        n.children.push(node(json!({ "body": "{{record.title}}" })));
        resolve_template_expressions(&mut n, &json!({ "title": "X" }));
        assert_eq!(n.props["body"], json!("X"));
        assert_eq!(n.children[0].props["body"], json!("X"));
    }

    #[test]
    fn no_expression_is_a_noop() {
        let mut n = node(json!({ "body": "plain" }));
        resolve_template_expressions(&mut n, &json!({ "x": 1 }));
        assert_eq!(n.props["body"], json!("plain"));
    }
}
