//! Resources — typed, shareable data objects referenced by ID.
//!
//! Instead of inlining values into every node's props, nodes can
//! reference a shared `ResourceDef` via `{ "$ref": "resource:<id>" }`.
//! The render walker resolves these references before passing props to
//! the component, so components are unaware of the indirection.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type ResourceId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    StylePreset,
    ColorPalette,
    TypographyScale,
    AnimationCurve,
    DataSource,
    MediaAsset,
    IconSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDef {
    pub id: ResourceId,
    pub kind: ResourceKind,
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub data: Value,
}

/// Walk a props tree and replace `{ "$ref": "resource:<id>" }` objects
/// with the referenced resource's data. Non-matching values pass
/// through unchanged.
pub fn resolve_resource_refs(
    props: &Value,
    resources: &IndexMap<ResourceId, ResourceDef>,
) -> Value {
    match props {
        Value::Object(map) => {
            if let Some(ref_val) = map.get("$ref").and_then(|v| v.as_str()) {
                if let Some(id) = ref_val.strip_prefix("resource:") {
                    if let Some(resource) = resources.get(id) {
                        return resource.data.clone();
                    }
                }
            }
            Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), resolve_resource_refs(v, resources)))
                    .collect(),
            )
        }
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| resolve_resource_refs(v, resources))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_resources() -> IndexMap<ResourceId, ResourceDef> {
        let mut m = IndexMap::new();
        m.insert(
            "brand".into(),
            ResourceDef {
                id: "brand".into(),
                kind: ResourceKind::ColorPalette,
                label: "Brand Colors".into(),
                description: String::new(),
                data: json!({ "primary": "#3b82f6", "secondary": "#6366f1" }),
            },
        );
        m.insert(
            "heading-style".into(),
            ResourceDef {
                id: "heading-style".into(),
                kind: ResourceKind::StylePreset,
                label: "Heading Style".into(),
                description: String::new(),
                data: json!({ "font-size": 32, "font-weight": 700 }),
            },
        );
        m
    }

    #[test]
    fn resolves_top_level_ref() {
        let resources = sample_resources();
        let props = json!({ "$ref": "resource:brand" });
        let resolved = resolve_resource_refs(&props, &resources);
        assert_eq!(
            resolved,
            json!({ "primary": "#3b82f6", "secondary": "#6366f1" })
        );
    }

    #[test]
    fn resolves_nested_ref() {
        let resources = sample_resources();
        let props = json!({
            "text": "Hello",
            "style": { "$ref": "resource:heading-style" }
        });
        let resolved = resolve_resource_refs(&props, &resources);
        assert_eq!(resolved["text"], "Hello");
        assert_eq!(resolved["style"]["font-size"], 32);
    }

    #[test]
    fn passes_through_unknown_ref() {
        let resources = sample_resources();
        let props = json!({ "$ref": "resource:missing" });
        let resolved = resolve_resource_refs(&props, &resources);
        assert_eq!(resolved, json!({ "$ref": "resource:missing" }));
    }

    #[test]
    fn passes_through_non_resource_ref() {
        let resources = sample_resources();
        let props = json!({ "$ref": "other:thing" });
        let resolved = resolve_resource_refs(&props, &resources);
        assert_eq!(resolved, json!({ "$ref": "other:thing" }));
    }

    #[test]
    fn passes_through_plain_props() {
        let resources = sample_resources();
        let props = json!({ "text": "hello", "level": 1 });
        let resolved = resolve_resource_refs(&props, &resources);
        assert_eq!(resolved, props);
    }

    #[test]
    fn resolves_in_array() {
        let resources = sample_resources();
        let props = json!([{ "$ref": "resource:brand" }, "plain"]);
        let resolved = resolve_resource_refs(&props, &resources);
        assert_eq!(resolved[0]["primary"], "#3b82f6");
        assert_eq!(resolved[1], "plain");
    }

    #[test]
    fn resource_def_round_trips_through_serde() {
        let def = ResourceDef {
            id: "test".into(),
            kind: ResourceKind::AnimationCurve,
            label: "Test".into(),
            description: "A test resource".into(),
            data: json!({ "easing": "ease-in-out" }),
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: ResourceDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "test");
        assert_eq!(back.kind, ResourceKind::AnimationCurve);
    }
}
