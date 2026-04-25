//! Parse marker-annotated `.slint` source back into a [`BuilderDocument`].
//!
//! This is the inverse of [`render_document_slint_source_mapped`]: given
//! `.slint` source with `// @node-start` / `// @node-end` markers, it
//! reconstructs the node tree with parsed property values.
//!
//! Unmarked regions compile and render but don't appear in the derived
//! document — they're "unmanaged" from the builder's perspective.

use serde_json::Value;

use crate::document::{BuilderDocument, Node};
use crate::layout::{FlowProps, GridPlacement, LayoutMode};
use crate::render::line_byte_offsets;
use crate::source_map::SourceMap;

/// Derive a [`BuilderDocument`] from marker-annotated `.slint` source.
///
/// Only marked nodes (those with `@node-start`/`@node-end` comments)
/// appear in the returned document. The source map provides the byte
/// ranges; this function parses the tree structure from marker nesting
/// and extracts property values from within each node's span.
pub fn derive_document_from_source(source: &str, _source_map: &SourceMap) -> BuilderDocument {
    let root = build_node_tree(source);
    BuilderDocument {
        root,
        ..Default::default()
    }
}

/// Reconstruct the node tree from `@node-start`/`@node-end` nesting.
fn build_node_tree(source: &str) -> Option<Node> {
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;

    let lines = line_byte_offsets(source);
    let mut prop_regions: Vec<(usize, usize)> = Vec::new();

    for (line_start, line) in &lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("// @node-start:") {
            let parts: Vec<&str> = rest.splitn(2, ':').collect();
            if parts.len() == 2 {
                prop_regions.push((*line_start + line.len() + 1, 0));
                stack.push(Node {
                    id: parts[0].to_string(),
                    component: parts[1].to_string(),
                    props: Value::Null,
                    children: Vec::new(),
                    ..Default::default()
                });
            }
        } else if let Some(rest) = trimmed.strip_prefix("// @grid:") {
            if let Some(node) = stack.last_mut() {
                if let Some((col_s, row_s)) = rest.split_once(',') {
                    if let (Ok(col), Ok(row)) =
                        (col_s.trim().parse::<i32>(), row_s.trim().parse::<i32>())
                    {
                        node.layout_mode = LayoutMode::Flow(FlowProps {
                            grid_column: GridPlacement::Line {
                                index: (col + 1) as i16,
                            },
                            grid_row: GridPlacement::Line {
                                index: (row + 1) as i16,
                            },
                            ..Default::default()
                        });
                    }
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("// @node-end:") {
            let node_id = rest.trim();
            if let Some(pos) = stack.iter().rposition(|n| n.id == node_id) {
                let mut completed = stack.remove(pos);
                let (prop_start, _) = prop_regions.remove(pos);
                let prop_end = *line_start;
                completed.props =
                    parse_props_from_region(source, prop_start, prop_end, &completed.children);
                translate_slint_keys_to_schema(&completed.component, &mut completed.props);
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(completed);
                } else {
                    root = Some(completed);
                }
            }
        }
    }
    root
}

/// Parse property bindings from a region of `.slint` source, skipping
/// child marker regions.
fn parse_props_from_region(
    source: &str,
    region_start: usize,
    region_end: usize,
    _children: &[Node],
) -> Value {
    let mut map = serde_json::Map::new();
    let clamped_end = region_end.min(source.len());
    let clamped_start = region_start.min(clamped_end);
    let mut child_depth: usize = 0;

    for (line_start, line) in line_byte_offsets(source) {
        if line_start >= clamped_end {
            break;
        }
        if line_start + line.len() < clamped_start {
            continue;
        }

        let trimmed = line.trim();

        if trimmed.starts_with("// @node-start:") {
            child_depth += 1;
            continue;
        }
        if trimmed.starts_with("// @node-end:") {
            child_depth = child_depth.saturating_sub(1);
            continue;
        }
        if child_depth > 0 {
            continue;
        }

        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.ends_with('{')
            || trimmed == "}"
        {
            continue;
        }

        if let Some((key, val)) = parse_property_line(trimmed) {
            map.insert(key, val);
        }
    }

    Value::Object(map)
}

/// Parse a single `key: value;` line.
fn parse_property_line(line: &str) -> Option<(String, Value)> {
    let colon = line.find(':')?;
    let semi = line.rfind(';')?;
    if semi <= colon {
        return None;
    }
    let key = line[..colon].trim();
    let value_str = line[colon + 1..semi].trim();
    Some((key.to_string(), parse_slint_value(value_str)))
}

/// Parse a Slint value literal into `serde_json::Value`.
pub fn parse_slint_value(val: &str) -> Value {
    if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
        let inner = &val[1..val.len() - 1];
        Value::String(unescape_slint_string(inner))
    } else if val == "true" {
        Value::Bool(true)
    } else if val == "false" {
        Value::Bool(false)
    } else if let Some(num_str) = val.strip_suffix("px") {
        parse_numeric(num_str.trim()).unwrap_or_else(|| Value::String(val.to_string()))
    } else if let Some(num_str) = val.strip_suffix('%') {
        parse_numeric(num_str.trim()).unwrap_or_else(|| Value::String(val.to_string()))
    } else if let Some(num) = parse_numeric(val) {
        num
    } else {
        Value::String(val.to_string())
    }
}

fn parse_numeric(s: &str) -> Option<Value> {
    if let Ok(n) = s.parse::<i64>() {
        return Some(Value::Number(n.into()));
    }
    if let Ok(f) = s.parse::<f64>() {
        if f.fract() == 0.0 && f.is_finite() && f.abs() < i64::MAX as f64 {
            return Some(Value::Number((f as i64).into()));
        }
        return serde_json::Number::from_f64(f).map(Value::Number);
    }
    None
}

/// Format a `serde_json::Value` as a Slint property value literal.
pub fn format_slint_value(value: &Value, px_suffix: bool) -> String {
    match value {
        Value::String(s) => {
            format!("\"{}\"", crate::slint_source::escape_slint_string(s))
        }
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            if px_suffix {
                if let Some(i) = n.as_i64() {
                    format!("{i}px")
                } else if let Some(f) = n.as_f64() {
                    if f.fract() == 0.0 && f.is_finite() {
                        format!("{}px", f as i64)
                    } else {
                        format!("{f}px")
                    }
                } else {
                    n.to_string()
                }
            } else if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{}", f as i64)
                } else {
                    format!("{f}")
                }
            } else {
                n.to_string()
            }
        }
        Value::Null => "\"\"".to_string(),
        _ => format!("\"{}\"", value),
    }
}

/// Rename Slint-native property keys back to their schema equivalents
/// so the derived `BuilderDocument` uses the same keys `render_slint` reads.
fn translate_slint_keys_to_schema(component: &str, props: &mut Value) {
    if let Value::Object(map) = props {
        if component == "image" {
            if let Some(v) = map.remove("image-fit") {
                if !map.contains_key("fit") {
                    map.insert("fit".to_string(), v);
                }
            }
        }
    }
}

fn unescape_slint_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::build_source_map_from_markers;

    #[test]
    fn parse_slint_string_value() {
        assert_eq!(
            parse_slint_value(r#""Hello""#),
            Value::String("Hello".into())
        );
    }

    #[test]
    fn parse_slint_escaped_string() {
        assert_eq!(
            parse_slint_value(r#""line1\nline2""#),
            Value::String("line1\nline2".into())
        );
    }

    #[test]
    fn parse_slint_bool_values() {
        assert_eq!(parse_slint_value("true"), Value::Bool(true));
        assert_eq!(parse_slint_value("false"), Value::Bool(false));
    }

    #[test]
    fn parse_slint_px_value() {
        assert_eq!(parse_slint_value("24px"), Value::Number(24.into()));
    }

    #[test]
    fn parse_slint_integer() {
        assert_eq!(parse_slint_value("42"), Value::Number(42.into()));
    }

    #[test]
    fn parse_slint_float() {
        let val = parse_slint_value("0.5");
        assert_eq!(val.as_f64(), Some(0.5));
    }

    #[test]
    fn parse_slint_color_passthrough() {
        assert_eq!(
            parse_slint_value("#ff6600ff"),
            Value::String("#ff6600ff".into())
        );
    }

    #[test]
    fn format_string_value() {
        let val = Value::String("Hello".into());
        assert_eq!(format_slint_value(&val, false), r#""Hello""#);
    }

    #[test]
    fn format_number_with_px() {
        let val = serde_json::json!(24);
        assert_eq!(format_slint_value(&val, true), "24px");
    }

    #[test]
    fn format_bool_value() {
        assert_eq!(format_slint_value(&Value::Bool(true), false), "true");
    }

    #[test]
    fn round_trip_string() {
        let original = "Hello \"World\"\nSecond line";
        let formatted = format_slint_value(&Value::String(original.into()), false);
        let parsed = parse_slint_value(&formatted);
        assert_eq!(parsed, Value::String(original.into()));
    }

    #[test]
    fn derive_single_node() {
        let source = r#"// Auto-generated by prism_builder — live bidirectional source (ADR-006).

export component BuilderRoot inherits Window {
    preferred-width: 1280px;
    preferred-height: 800px;

    VerticalLayout {
        // @node-start:n1:heading
        Text {
            text: "Hello";
            font-size: 24px;
        }
        // @node-end:n1
    }
}"#;
        let map = build_source_map_from_markers(source);
        let doc = derive_document_from_source(source, &map);

        let root = doc.root.as_ref().expect("should have root node");
        assert_eq!(root.id, "n1");
        assert_eq!(root.component, "heading");
        assert_eq!(
            root.props.get("text").and_then(|v| v.as_str()),
            Some("Hello")
        );
        assert_eq!(
            root.props.get("font-size").and_then(|v| v.as_i64()),
            Some(24)
        );
    }

    #[test]
    fn derive_nested_nodes() {
        let source = r#"export component BuilderRoot inherits Window {
    VerticalLayout {
        // @node-start:s1:section
        VerticalLayout {
            spacing: 12px;
            // @node-start:h1:heading
            Text {
                text: "A";
                font-size: 24px;
            }
            // @node-end:h1
            // @node-start:h2:heading
            Text {
                text: "B";
                font-size: 24px;
            }
            // @node-end:h2
        }
        // @node-end:s1
    }
}"#;
        let map = build_source_map_from_markers(source);
        let doc = derive_document_from_source(source, &map);

        let root = doc.root.as_ref().expect("should have root");
        assert_eq!(root.id, "s1");
        assert_eq!(root.component, "section");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].id, "h1");
        assert_eq!(
            root.children[0].props.get("text").and_then(|v| v.as_str()),
            Some("A")
        );
        assert_eq!(root.children[1].id, "h2");
    }

    #[test]
    fn derive_preserves_parent_props_not_children() {
        let source = r#"export component BuilderRoot inherits Window {
    // @node-start:s1:section
    VerticalLayout {
        spacing: 12px;
        // @node-start:h1:heading
        Text {
            text: "Child";
            font-size: 32px;
        }
        // @node-end:h1
    }
    // @node-end:s1
}"#;
        let map = build_source_map_from_markers(source);
        let doc = derive_document_from_source(source, &map);

        let root = doc.root.as_ref().unwrap();
        assert_eq!(root.props.get("spacing").and_then(|v| v.as_i64()), Some(12));
        assert!(root.props.get("text").is_none());
        assert!(root.props.get("font-size").is_none());
    }

    #[test]
    fn derive_empty_source_returns_empty_document() {
        let source = "export component BuilderRoot inherits Window { }";
        let map = build_source_map_from_markers(source);
        let doc = derive_document_from_source(source, &map);
        assert!(doc.root.is_none());
    }
}
