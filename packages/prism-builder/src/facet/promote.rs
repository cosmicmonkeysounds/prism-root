//! Promote inline facet templates into reusable prefab components.


use serde_json::Value;


use crate::document::Node;
use crate::prefab::{ExposedSlot, PrefabDef};
use crate::registry::FieldSpec;

use super::*;

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

