//! Facets — programmatic list generators backed by a prefab template.
//!
//! A `FacetDef` pairs a [`PrefabDef`] template with a data source and a
//! set of bindings that map item fields to prefab exposed slots. At render
//! time the facet expands into one prefab instance per data item.
//!
//! See `docs/dev/facets.md` for the full design rationale.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use prism_core::help::HelpEntry;

use crate::component::{Component, ComponentId, RenderError, RenderSlintContext};
use crate::document::Node;
use crate::html::Html;
use crate::html_block::{HtmlBlock, HtmlRenderContext};
use crate::prefab::{apply_prop_to_node, PrefabDef};
use crate::registry::{FieldSpec, NumericBounds};
use crate::resource::ResourceId;
use crate::signal::{common_signals, SignalDef};
use crate::slint_source::SlintEmitter;

// ── Data types ────────────────────────────────────────────────────────────────

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
    /// Reserved for Phase 2: fixed column count for grid layouts.
    #[serde(default)]
    pub columns: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FacetDataSource {
    /// Hand-authored inline array. Always available, no external dep.
    Static { items: Vec<Value> },
    /// Reference to a `DataSource` resource whose `data` field is a JSON array.
    Resource { id: ResourceId },
    /// Filter and sort a resource array without mutating the underlying resource.
    /// `filter` is an optional expression: `"field_path == value"`,
    /// `"field_path != value"`, or just `"field_path"` (truthy check).
    /// `sort_by` is an optional dot-notation field path; sort is ascending
    /// string order. Prepend `-` (e.g. `"-date"`) for descending.
    Query {
        source: ResourceId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sort_by: Option<String>,
    },
}

impl Default for FacetDataSource {
    fn default() -> Self {
        Self::Static { items: vec![] }
    }
}

impl FacetDataSource {
    pub fn resolve(
        &self,
        resources: &IndexMap<ResourceId, crate::resource::ResourceDef>,
    ) -> Vec<Value> {
        match self {
            FacetDataSource::Static { items } => items.clone(),
            FacetDataSource::Resource { id } => resources
                .get(id)
                .and_then(|r| r.data.as_array())
                .cloned()
                .unwrap_or_default(),
            FacetDataSource::Query {
                source,
                filter,
                sort_by,
            } => {
                let mut items: Vec<Value> = resources
                    .get(source)
                    .and_then(|r| r.data.as_array())
                    .cloned()
                    .unwrap_or_default();

                if let Some(expr) = filter {
                    items.retain(|item| evaluate_filter(item, expr));
                }

                if let Some(key) = sort_by {
                    let (descending, path) = if let Some(stripped) = key.strip_prefix('-') {
                        (true, stripped)
                    } else {
                        (false, key.as_str())
                    };
                    items.sort_by(|a, b| {
                        let va = get_field(a, path).map(value_sort_key).unwrap_or_default();
                        let vb = get_field(b, path).map(value_sort_key).unwrap_or_default();
                        if descending {
                            vb.cmp(&va)
                        } else {
                            va.cmp(&vb)
                        }
                    });
                }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// ID of the `PrefabDef` used as the item template.
    pub prefab_id: ComponentId,
    #[serde(default)]
    pub data: FacetDataSource,
    #[serde(default)]
    pub bindings: Vec<FacetBinding>,
    #[serde(default)]
    pub layout: FacetLayout,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Dot-notation field access into a JSON value (e.g. `"meta.title"`).
fn get_field(item: &Value, path: &str) -> Option<Value> {
    let mut current = item;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current.clone())
}

/// Stringify a JSON value for stable string-based sorting.
fn value_sort_key(v: Value) -> String {
    match v {
        Value::String(s) => s,
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                format!("{f:020.6}")
            } else {
                n.to_string()
            }
        }
        Value::Bool(b) => (if b { "1" } else { "0" }).into(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Evaluate a simple filter expression against a data item.
///
/// Supported forms:
/// - `"field"` — truthy check (non-null, non-empty string, non-false, non-zero)
/// - `"field == value"` — equality (string comparison after serialising both sides)
/// - `"field != value"` — inequality
fn evaluate_filter(item: &Value, expr: &str) -> bool {
    let expr = expr.trim();
    if let Some((lhs, rhs)) = expr.split_once("!=") {
        let field_val = get_field(item, lhs.trim())
            .map(value_sort_key)
            .unwrap_or_default();
        let rhs_str = rhs.trim().trim_matches('\'').trim_matches('"');
        return field_val != rhs_str;
    }
    if let Some((lhs, rhs)) = expr.split_once("==") {
        let field_val = get_field(item, lhs.trim())
            .map(value_sort_key)
            .unwrap_or_default();
        let rhs_str = rhs.trim().trim_matches('\'').trim_matches('"');
        return field_val == rhs_str;
    }
    // Truthy check
    match get_field(item, expr) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// Apply all facet bindings to a cloned prefab root node.
fn apply_bindings(root: &mut Node, prefab: &PrefabDef, bindings: &[FacetBinding], item: &Value) {
    for binding in bindings {
        if let Some(slot) = prefab.exposed.iter().find(|s| s.key == binding.slot_key) {
            if let Some(value) = get_field(item, &binding.item_field) {
                apply_prop_to_node(root, &slot.target_node, &slot.target_prop, value);
            }
        }
    }
}

// ── Slint component ───────────────────────────────────────────────────────────

pub struct FacetComponent {
    pub id: ComponentId,
}

impl FacetComponent {
    pub fn new() -> Self {
        Self { id: "facet".into() }
    }
}

impl Default for FacetComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for FacetComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        crate::schemas::facet()
    }

    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "builder.components.facet",
            "Facet",
            "Programmatic list: expands a prefab template once per item in a data source.",
        ))
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }

    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let facet_id = props.get("facet_id").and_then(|v| v.as_str()).unwrap_or("");

        let facet = match ctx.facets.get(facet_id) {
            Some(f) => f,
            None => {
                let label = if facet_id.is_empty() {
                    "(no facet_id set)".to_string()
                } else {
                    format!("Facet: {facet_id} (not found)")
                };
                return out.block("Rectangle", |out| {
                    out.prop_px("preferred-height", 40.0);
                    out.line("background: #f0f0f0;");
                    out.block("Text", |out| {
                        out.prop_string("text", &label);
                        Ok(())
                    })
                });
            }
        };

        let prefab = ctx.prefabs.get(&facet.prefab_id).ok_or_else(|| {
            RenderError::Failed(format!("prefab '{}' not found", facet.prefab_id))
        })?;

        let mut items = facet.data.resolve(ctx.resources);
        if let Some(max) = props.get("max_items").and_then(|v| v.as_u64()) {
            items.truncate(max as usize);
        }

        let layout_tag = match facet.layout.direction {
            FacetDirection::Row => "HorizontalLayout",
            FacetDirection::Column => "VerticalLayout",
        };

        out.block(layout_tag, |out| {
            if facet.layout.gap > 0.0 {
                out.prop_px("spacing", facet.layout.gap as f64);
            }
            out.line("alignment: start;");
            for item in &items {
                let mut root = prefab.root.clone();
                apply_bindings(&mut root, prefab, &facet.bindings, item);
                ctx.render_child(&root, out)?;
            }
            Ok(())
        })
    }
}

// ── HTML block ────────────────────────────────────────────────────────────────

pub struct FacetHtmlBlock {
    pub id: ComponentId,
}

impl FacetHtmlBlock {
    pub fn new() -> Self {
        Self { id: "facet".into() }
    }
}

impl Default for FacetHtmlBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl HtmlBlock for FacetHtmlBlock {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        crate::schemas::facet()
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }

    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let facet_id = props.get("facet_id").and_then(|v| v.as_str()).unwrap_or("");

        let facet = ctx
            .facets
            .get(facet_id)
            .ok_or_else(|| RenderError::Failed(format!("facet '{facet_id}' not found")))?;

        let prefab = ctx.prefabs.get(&facet.prefab_id).ok_or_else(|| {
            RenderError::Failed(format!("prefab '{}' not found", facet.prefab_id))
        })?;

        let mut items = facet.data.resolve(ctx.resources);
        if let Some(max) = props.get("max_items").and_then(|v| v.as_u64()) {
            items.truncate(max as usize);
        }

        let tag = match facet.layout.direction {
            FacetDirection::Row => "div",
            FacetDirection::Column => "div",
        };
        let style = match facet.layout.direction {
            FacetDirection::Row => format!(
                "display:flex;flex-direction:row;gap:{}px",
                facet.layout.gap as u32
            ),
            FacetDirection::Column => format!(
                "display:flex;flex-direction:column;gap:{}px",
                facet.layout.gap as u32
            ),
        };

        out.open_attrs(tag, &[("style", style.as_str()), ("data-facet", facet_id)]);
        for item in &items {
            let mut root = prefab.root.clone();
            apply_bindings(&mut root, prefab, &facet.bindings, item);
            ctx.render_child(&root, out)?;
        }
        out.close(tag);
        Ok(())
    }
}

// ── Schema helper (also registered in schemas.rs) ────────────────────────────

pub fn facet_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("facet_id", "Facet ID").required(),
        FieldSpec::integer(
            "max_items",
            "Max items",
            NumericBounds::min_max(1.0, 10_000.0),
        ),
    ]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::json;

    use crate::prefab::{ExposedSlot, PrefabDef};
    use crate::registry::FieldSpec;

    fn hero_prefab() -> PrefabDef {
        PrefabDef {
            id: "prefab:hero".into(),
            label: "Hero".into(),
            description: String::new(),
            root: Node {
                id: "hero-root".into(),
                component: "text".into(),
                props: json!({ "body": "default" }),
                children: vec![],
                ..Default::default()
            },
            exposed: vec![ExposedSlot {
                key: "title".into(),
                target_node: "hero-root".into(),
                target_prop: "body".into(),
                spec: FieldSpec::text("title", "Title"),
            }],
            variants: vec![],
            thumbnail: None,
        }
    }

    fn sample_facet() -> FacetDef {
        FacetDef {
            id: "facet:heroes".into(),
            label: "Heroes".into(),
            description: String::new(),
            prefab_id: "prefab:hero".into(),
            data: FacetDataSource::Static {
                items: vec![json!({ "name": "Alpha" }), json!({ "name": "Beta" })],
            },
            bindings: vec![FacetBinding {
                slot_key: "title".into(),
                item_field: "name".into(),
            }],
            layout: FacetLayout {
                direction: FacetDirection::Column,
                gap: 8.0,
                ..Default::default()
            },
        }
    }

    #[test]
    fn get_field_flat() {
        let item = json!({ "name": "Alpha" });
        assert_eq!(get_field(&item, "name"), Some(json!("Alpha")));
    }

    #[test]
    fn get_field_nested() {
        let item = json!({ "meta": { "title": "Deep" } });
        assert_eq!(get_field(&item, "meta.title"), Some(json!("Deep")));
    }

    #[test]
    fn get_field_missing_returns_none() {
        let item = json!({ "a": 1 });
        assert!(get_field(&item, "b").is_none());
        assert!(get_field(&item, "a.nested").is_none());
    }

    #[test]
    fn static_source_resolves_to_items() {
        let resources = IndexMap::new();
        let src = FacetDataSource::Static {
            items: vec![json!("a"), json!("b")],
        };
        let resolved = src.resolve(&resources);
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn resource_source_resolves_from_registry() {
        use crate::resource::{ResourceDef, ResourceKind};
        let mut resources = IndexMap::new();
        resources.insert(
            "items".into(),
            ResourceDef {
                id: "items".into(),
                kind: ResourceKind::DataSource,
                label: "Items".into(),
                description: String::new(),
                data: json!([{ "name": "X" }, { "name": "Y" }]),
            },
        );
        let src = FacetDataSource::Resource { id: "items".into() };
        let resolved = src.resolve(&resources);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0]["name"], "X");
    }

    #[test]
    fn resource_source_missing_returns_empty() {
        let resources = IndexMap::new();
        let src = FacetDataSource::Resource {
            id: "missing".into(),
        };
        assert!(src.resolve(&resources).is_empty());
    }

    #[test]
    fn apply_bindings_injects_values() {
        let prefab = hero_prefab();
        let item = json!({ "name": "TestTitle" });
        let bindings = vec![FacetBinding {
            slot_key: "title".into(),
            item_field: "name".into(),
        }];
        let mut root = prefab.root.clone();
        apply_bindings(&mut root, &prefab, &bindings, &item);
        assert_eq!(root.props["body"], "TestTitle");
    }

    #[test]
    fn apply_bindings_skips_missing_slot() {
        let prefab = hero_prefab();
        let item = json!({ "name": "TestTitle" });
        let bindings = vec![FacetBinding {
            slot_key: "nonexistent".into(),
            item_field: "name".into(),
        }];
        let mut root = prefab.root.clone();
        apply_bindings(&mut root, &prefab, &bindings, &item);
        // default value should be unchanged
        assert_eq!(root.props["body"], "default");
    }

    #[test]
    fn facet_def_round_trips_serde() {
        let def = sample_facet();
        let json = serde_json::to_string(&def).unwrap();
        let back: FacetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "facet:heroes");
        assert_eq!(back.bindings.len(), 1);
        assert_eq!(back.bindings[0].slot_key, "title");
    }

    #[test]
    fn facet_data_source_serde_static() {
        let src = FacetDataSource::Static {
            items: vec![json!({"a": 1})],
        };
        let json = serde_json::to_string(&src).unwrap();
        let back: FacetDataSource = serde_json::from_str(&json).unwrap();
        match back {
            FacetDataSource::Static { items } => assert_eq!(items.len(), 1),
            _ => panic!("expected Static"),
        }
    }

    #[test]
    fn facet_data_source_serde_resource() {
        let src = FacetDataSource::Resource {
            id: "my-data".into(),
        };
        let json = serde_json::to_string(&src).unwrap();
        let back: FacetDataSource = serde_json::from_str(&json).unwrap();
        match back {
            FacetDataSource::Resource { id } => assert_eq!(id, "my-data"),
            _ => panic!("expected Resource"),
        }
    }

    #[test]
    fn facet_schema_has_required_facet_id() {
        let schema = facet_schema();
        let facet_id_spec = schema.iter().find(|s| s.key == "facet_id").unwrap();
        assert!(facet_id_spec.required);
    }

    fn sample_resources() -> IndexMap<String, crate::resource::ResourceDef> {
        use crate::resource::{ResourceDef, ResourceKind};
        let mut resources = IndexMap::new();
        resources.insert(
            "products".into(),
            ResourceDef {
                id: "products".into(),
                kind: ResourceKind::DataSource,
                label: "Products".into(),
                description: String::new(),
                data: json!([
                    { "name": "Apple", "status": "active", "price": 1.5 },
                    { "name": "Banana", "status": "inactive", "price": 0.5 },
                    { "name": "Cherry", "status": "active", "price": 3.0 },
                ]),
            },
        );
        resources
    }

    #[test]
    fn query_source_no_filter_no_sort_returns_all() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            filter: None,
            sort_by: None,
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn query_source_equality_filter() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            filter: Some("status == active".into()),
            sort_by: None,
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i["status"] == "active"));
    }

    #[test]
    fn query_source_inequality_filter() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            filter: Some("status != active".into()),
            sort_by: None,
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "Banana");
    }

    #[test]
    fn query_source_truthy_filter() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            filter: Some("status".into()),
            sort_by: None,
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn query_source_sort_ascending() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            filter: None,
            sort_by: Some("name".into()),
        };
        let items = src.resolve(&resources);
        assert_eq!(items[0]["name"], "Apple");
        assert_eq!(items[1]["name"], "Banana");
        assert_eq!(items[2]["name"], "Cherry");
    }

    #[test]
    fn query_source_sort_descending() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            filter: None,
            sort_by: Some("-name".into()),
        };
        let items = src.resolve(&resources);
        assert_eq!(items[0]["name"], "Cherry");
        assert_eq!(items[2]["name"], "Apple");
    }

    #[test]
    fn query_source_filter_and_sort() {
        let resources = sample_resources();
        let src = FacetDataSource::Query {
            source: "products".into(),
            filter: Some("status == active".into()),
            sort_by: Some("-price".into()),
        };
        let items = src.resolve(&resources);
        assert_eq!(items.len(), 2);
        // Cherry (3.0) before Apple (1.5) for descending price
        assert_eq!(items[0]["name"], "Cherry");
        assert_eq!(items[1]["name"], "Apple");
    }

    #[test]
    fn query_source_serde_round_trip() {
        let src = FacetDataSource::Query {
            source: "data".into(),
            filter: Some("active == true".into()),
            sort_by: Some("name".into()),
        };
        let json = serde_json::to_string(&src).unwrap();
        let back: FacetDataSource = serde_json::from_str(&json).unwrap();
        match back {
            FacetDataSource::Query {
                source,
                filter,
                sort_by,
            } => {
                assert_eq!(source, "data");
                assert_eq!(filter, Some("active == true".into()));
                assert_eq!(sort_by, Some("name".into()));
            }
            _ => panic!("expected Query"),
        }
    }

    #[test]
    fn evaluate_filter_truthy_empty_string_is_false() {
        let item = json!({ "name": "" });
        assert!(!evaluate_filter(&item, "name"));
    }

    #[test]
    fn evaluate_filter_truthy_null_is_false() {
        let item = json!({ "name": null });
        assert!(!evaluate_filter(&item, "name"));
    }

    #[test]
    fn evaluate_filter_truthy_missing_field_is_false() {
        let item = json!({ "other": 1 });
        assert!(!evaluate_filter(&item, "name"));
    }
}
