//! Properties panel — unified section-based inspector for the
//! currently selected node. Inspired by Unity's Inspector:
//! every node is a "game object" with Transform, Component props,
//! Layout, and Appearance sections. Collapse state is automatic —
//! sections at their defaults collapse; modified sections expand.
//!
//! The Slint side receives a single flat `property-rows` model
//! where section headers and field rows are interleaved. One
//! unified edit callback routes by key prefix.

use prism_builder::layout::{
    AlignOption, Dimension, FlexDirection, FlowDisplay, GridPlacement, JustifyOption, LayoutMode,
};
use prism_builder::style::StyleProperties;
use prism_builder::FacetKind;
use prism_builder::{
    BuilderDocument, ComponentRegistry, FieldKind, FieldSpec, FieldValue, NodeId, PrismApp,
};
#[cfg(test)]
#[allow(unused_imports)]
use prism_builder::{FacetOutput, FacetTemplate};
use prism_core::foundation::spatial::Transform2D;
use prism_core::help::HelpEntry;
use serde_json::Value;

use super::Panel;

pub struct PropertiesPanel;

/// One row rendered by the Slint `FieldRowView` component. Mirrors
/// the `FieldRow` struct declared in `ui/app.slint`.
#[derive(Debug, Clone)]
pub struct FieldRowData {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub value: String,
    pub required: bool,
    pub min: f32,
    pub max: f32,
    pub has_bounds: bool,
    pub options: Vec<String>,
}

impl FieldRowData {
    pub fn text(
        key: impl Into<String>,
        label: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: "text".into(),
            value: value.into(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        }
    }

    pub fn number(
        key: impl Into<String>,
        label: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: "number".into(),
            value: value.into(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        }
    }

    pub fn boolean(key: impl Into<String>, label: impl Into<String>, value: bool) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: "boolean".into(),
            value: if value { "true" } else { "false" }.into(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        }
    }

    pub fn select(
        key: impl Into<String>,
        label: impl Into<String>,
        value: impl Into<String>,
        options: Vec<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: "select".into(),
            value: value.into(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options,
        }
    }

    pub fn color(
        key: impl Into<String>,
        label: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: "color".into(),
            value: value.into(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn bounds(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self.has_bounds = true;
        self
    }
}

/// A collapsible section in the properties panel. Collapse state
/// is computed from the data: sections at defaults auto-collapse,
/// modified sections auto-expand. No stored booleans.
#[derive(Debug, Clone)]
pub struct PropertySection {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub collapsed: bool,
    pub rows: Vec<FieldRowData>,
}

/// Origin of an appearance value in the cascade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleOrigin {
    Node,
    Page,
    App,
    Default,
}

mod appearance;
mod facets;
mod layout;

impl PropertiesPanel {
    pub const ID: i32 = 3;

    pub fn new() -> Self {
        Self
    }

    /// Find the component id of the currently selected node. Empty
    /// string when no node is selected or the id doesn't resolve.
    pub fn selected_component(doc: &BuilderDocument, selected: &Option<NodeId>) -> String {
        selected
            .as_ref()
            .and_then(|id| doc.root.as_ref().and_then(|n| n.find(id)))
            .map(|n| n.component.clone())
            .unwrap_or_default()
    }

    /// Compute all property sections for the selected node.
    /// Collapse state is automatic: sections at their default
    /// values collapse; sections with user-modified data expand.
    /// Returns an empty vec when nothing is selected.
    pub fn sections(
        doc: &BuilderDocument,
        registry: &ComponentRegistry,
        selected: &Option<NodeId>,
        app: Option<&PrismApp>,
    ) -> Vec<PropertySection> {
        let Some(selected_id) = selected else {
            return Self::page_sections(app);
        };
        let Some(node) = doc.root.as_ref().and_then(|n| n.find(selected_id)) else {
            return vec![];
        };
        let component = registry.get(&node.component);

        let mut sections = vec![];

        // ── Transform ──────────────────────────────────────────
        let transform_default = node.transform == Transform2D::default();
        sections.push(PropertySection {
            id: "transform".into(),
            label: "Transform".into(),
            icon: "move".into(),
            collapsed: transform_default,
            rows: Self::transform_rows(doc, selected),
        });

        // ── Component (schema-driven props) ────────────────────
        let component_rows = Self::rows(doc, registry, selected);
        let component_label = component
            .as_ref()
            .map(|c| {
                let id = c.id();
                let mut chars = id.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => id.to_string(),
                }
            })
            .unwrap_or_else(|| "Component".into());
        sections.push(PropertySection {
            id: "component".into(),
            label: component_label,
            icon: "sliders".into(),
            collapsed: false,
            rows: component_rows,
        });

        // ── Layout ─────────────────────────────────────────────
        let layout_default = node.layout_mode == LayoutMode::default();
        let layout_rows = Self::layout_rows(doc, selected);
        sections.push(PropertySection {
            id: "layout".into(),
            label: "Layout".into(),
            icon: "layout".into(),
            collapsed: layout_default,
            rows: layout_rows,
        });

        // ── Appearance (cascade) ───────────────────────────────
        let node_style_default = node.style == StyleProperties::default();
        let appearance_rows = Self::appearance_rows(node, app);
        sections.push(PropertySection {
            id: "appearance".into(),
            label: "Appearance".into(),
            icon: "palette".into(),
            collapsed: node_style_default,
            rows: appearance_rows,
        });

        // ── Modifiers (only when non-empty) ────────────────────
        if !node.modifiers.is_empty() {
            let modifier_rows: Vec<FieldRowData> = node
                .modifiers
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    FieldRowData::text(
                        format!("modifier.{i}"),
                        format!("{:?}", m.kind),
                        format!("{:?}", m.kind),
                    )
                })
                .collect();
            sections.push(PropertySection {
                id: "modifiers".into(),
                label: "Modifiers".into(),
                icon: "layers".into(),
                collapsed: false,
                rows: modifier_rows,
            });
        }

        // ── Variants (only when component declares them) ───────
        if let Some(ref c) = component {
            let variant_axes = c.variants();
            if !variant_axes.is_empty() {
                let variant_rows: Vec<FieldRowData> = variant_axes
                    .iter()
                    .map(|axis| {
                        let current = node
                            .props
                            .get(&axis.key)
                            .and_then(|v| v.as_str())
                            .unwrap_or(
                                axis.options.first().map(|o| o.value.as_str()).unwrap_or(""),
                            );
                        FieldRowData::select(
                            axis.key.clone(),
                            axis.label.clone(),
                            current,
                            axis.options.iter().map(|o| o.value.clone()).collect(),
                        )
                    })
                    .collect();
                sections.push(PropertySection {
                    id: "variants".into(),
                    label: "Variants".into(),
                    icon: "layers".into(),
                    collapsed: false,
                    rows: variant_rows,
                });
            }
        }

        // ── Facet Data (only when component is "facet") ───────
        if node.component == "facet" {
            let facet_rows = Self::facet_rows(doc, node);
            sections.push(PropertySection {
                id: "facet-data".into(),
                label: "Facet Data".into(),
                icon: "layers".into(),
                collapsed: false,
                rows: facet_rows,
            });
        }

        sections
    }

    /// When nothing is selected, show page-level style editing.
    fn page_sections(app: Option<&PrismApp>) -> Vec<PropertySection> {
        let default_style = StyleProperties::default();
        let page_style = app
            .and_then(|a| a.pages.get(a.active_page))
            .map(|p| &p.style)
            .unwrap_or(&default_style);
        let page_default = *page_style == default_style;
        let rows = style_rows_from(page_style, "style");
        vec![PropertySection {
            id: "appearance".into(),
            label: "Page Styles".into(),
            icon: "palette".into(),
            collapsed: page_default,
            rows,
        }]
    }

    /// Flatten sections into a single row list for Slint consumption.
    /// Section headers are emitted as rows with `kind = "section"`.
    pub fn flatten_sections(sections: &[PropertySection]) -> Vec<FieldRowData> {
        let mut flat = Vec::new();
        for section in sections {
            flat.push(FieldRowData {
                key: section.id.clone(),
                label: section.label.clone(),
                kind: "section".into(),
                value: if section.collapsed {
                    "collapsed".into()
                } else {
                    "expanded".into()
                },
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![section.icon.clone()],
            });
            if !section.collapsed && section.id != "transform" {
                for row in &section.rows {
                    flat.push(row.clone());
                }
            }
        }
        flat
    }

    pub fn rows(
        doc: &BuilderDocument,
        registry: &ComponentRegistry,
        selected: &Option<NodeId>,
    ) -> Vec<FieldRowData> {
        let Some(selected_id) = selected else {
            return vec![];
        };
        let Some(node) = doc.root.as_ref().and_then(|n| n.find(selected_id)) else {
            return vec![];
        };
        let Some(component) = registry.get(&node.component) else {
            return vec![];
        };
        let mut rows: Vec<FieldRowData> = component
            .schema()
            .into_iter()
            .map(|spec| row_from_spec(&spec, &node.props))
            .collect();

        if node.component == "facet" {
            let facet_ids: Vec<String> = doc.facets.keys().cloned().collect();
            let facet_id = node
                .props
                .get("facet_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let is_list = doc
                .facets
                .get(facet_id)
                .map(|f| matches!(f.kind, FacetKind::List))
                .unwrap_or(false);

            for row in &mut rows {
                if row.key == "facet_id" {
                    row.kind = "select".into();
                    row.options = facet_ids.clone();
                }
            }
            if !is_list {
                rows.retain(|r| r.key != "max_items");
            }
        }

        rows
    }
}

fn row_from_spec(spec: &FieldSpec, props: &Value) -> FieldRowData {
    let (kind_label, value, min, max, has_bounds, options) = match &spec.kind {
        FieldKind::Text => (
            "text",
            FieldValue::read_string(props, spec).to_string(),
            0.0,
            0.0,
            false,
            vec![],
        ),
        FieldKind::TextArea => (
            "textarea",
            FieldValue::read_string(props, spec).to_string(),
            0.0,
            0.0,
            false,
            vec![],
        ),
        FieldKind::Number(bounds) => (
            "number",
            format_number(FieldValue::read_number(props, spec)),
            bounds.min.unwrap_or(0.0) as f32,
            bounds.max.unwrap_or(100.0) as f32,
            bounds.min.is_some() && bounds.max.is_some(),
            vec![],
        ),
        FieldKind::Integer(bounds) => (
            "integer",
            FieldValue::read_integer(props, spec).to_string(),
            bounds.min.unwrap_or(0.0) as f32,
            bounds.max.unwrap_or(100.0) as f32,
            bounds.min.is_some() && bounds.max.is_some(),
            vec![],
        ),
        FieldKind::Boolean => (
            "boolean",
            FieldValue::read_boolean(props, spec).to_string(),
            0.0,
            0.0,
            false,
            vec![],
        ),
        FieldKind::Select(opts) => (
            "select",
            FieldValue::read_string(props, spec).to_string(),
            0.0,
            0.0,
            false,
            opts.iter().map(|o| o.value.clone()).collect(),
        ),
        FieldKind::Color => (
            "color",
            FieldValue::read_string(props, spec).to_string(),
            0.0,
            0.0,
            false,
            vec![],
        ),
        FieldKind::File(_) => {
            let display = props
                .get(&spec.key)
                .and_then(prism_builder::AssetSource::from_prop)
                .map(|s| s.display_name().to_string())
                .unwrap_or_default();
            ("file", display, 0.0, 0.0, false, vec![])
        }
        FieldKind::Date | FieldKind::DateTime => (
            "text",
            FieldValue::read_string(props, spec).to_string(),
            0.0,
            0.0,
            false,
            vec![],
        ),
        FieldKind::Duration => (
            "number",
            format_number(FieldValue::read_number(props, spec)),
            0.0,
            0.0,
            false,
            vec![],
        ),
        FieldKind::Currency { .. } => (
            "number",
            format_number(FieldValue::read_number(props, spec)),
            0.0,
            0.0,
            false,
            vec![],
        ),
        FieldKind::Calculation { .. } => (
            "text",
            FieldValue::read_string(props, spec).to_string(),
            0.0,
            0.0,
            false,
            vec![],
        ),
        FieldKind::Custom { .. } => (
            "text",
            FieldValue::read_string(props, spec).to_string(),
            0.0,
            0.0,
            false,
            vec![],
        ),
    };
    FieldRowData {
        key: spec.key.clone(),
        label: spec.label.clone(),
        kind: kind_label.into(),
        value,
        required: spec.required,
        min,
        max,
        has_bounds,
        options,
    }
}

fn layout_number(key: &str, label: &str, value: String, min: f32, max: f32) -> FieldRowData {
    FieldRowData {
        key: key.into(),
        label: label.into(),
        kind: "number".into(),
        value,
        required: false,
        min,
        max,
        has_bounds: true,
        options: vec![],
    }
}

fn layout_select(key: &str, label: &str, value: String, options: Vec<&str>) -> FieldRowData {
    FieldRowData {
        key: key.into(),
        label: label.into(),
        kind: "select".into(),
        value,
        required: false,
        min: 0.0,
        max: 0.0,
        has_bounds: false,
        options: options.into_iter().map(String::from).collect(),
    }
}

fn dimension_unit(d: Dimension) -> String {
    match d {
        Dimension::Auto => "auto",
        Dimension::Px { .. } => "px",
        Dimension::Percent { .. } => "%",
    }
    .into()
}

fn dimension_slider(d: Dimension) -> Option<(f32, f32, f32)> {
    match d {
        Dimension::Auto => None,
        Dimension::Px { value } => Some((value, 0.0, 2000.0)),
        Dimension::Percent { value } => Some((value, 0.0, 100.0)),
    }
}

fn placement_type(p: GridPlacement) -> String {
    match p {
        GridPlacement::Auto => "auto",
        GridPlacement::Line { .. } => "line",
        GridPlacement::Span { .. } => "span",
    }
    .into()
}

fn placement_slider(p: GridPlacement) -> Option<(f32, f32)> {
    match p {
        GridPlacement::Auto => None,
        GridPlacement::Line { index } => Some((index as f32, 24.0)),
        GridPlacement::Span { count } => Some((count as f32, 24.0)),
    }
}

fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn format_display(d: FlowDisplay) -> String {
    match d {
        FlowDisplay::Block => "block",
        FlowDisplay::Flex => "flex",
        FlowDisplay::Grid => "grid",
        FlowDisplay::None => "none",
    }
    .into()
}

fn format_flex_direction(d: FlexDirection) -> String {
    match d {
        FlexDirection::Row => "row",
        FlexDirection::Column => "column",
        FlexDirection::RowReverse => "row-reverse",
        FlexDirection::ColumnReverse => "column-reverse",
    }
    .into()
}

fn format_align(a: AlignOption) -> String {
    match a {
        AlignOption::Auto => "auto",
        AlignOption::Start => "start",
        AlignOption::End => "end",
        AlignOption::Center => "center",
        AlignOption::Stretch => "stretch",
        AlignOption::Baseline => "baseline",
    }
    .into()
}

fn format_justify(j: JustifyOption) -> String {
    match j {
        JustifyOption::Start => "start",
        JustifyOption::End => "end",
        JustifyOption::Center => "center",
        JustifyOption::SpaceBetween => "space-between",
        JustifyOption::SpaceAround => "space-around",
        JustifyOption::SpaceEvenly => "space-evenly",
        JustifyOption::Stretch => "stretch",
    }
    .into()
}

pub fn format_anchor(a: prism_core::foundation::spatial::Anchor) -> String {
    use prism_core::foundation::spatial::Anchor;
    match a {
        Anchor::TopLeft => "top-left",
        Anchor::TopCenter => "top-center",
        Anchor::TopRight => "top-right",
        Anchor::CenterLeft => "center-left",
        Anchor::Center => "center",
        Anchor::CenterRight => "center-right",
        Anchor::BottomLeft => "bottom-left",
        Anchor::BottomCenter => "bottom-center",
        Anchor::BottomRight => "bottom-right",
        Anchor::Stretch => "stretch",
    }
    .into()
}

fn format_f32(v: f32) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn style_rows_from(style: &StyleProperties, prefix: &str) -> Vec<FieldRowData> {
    let mut rows = Vec::new();
    let text = |key: &str, label: &str, val: &Option<String>| FieldRowData {
        key: format!("{prefix}.{key}"),
        label: label.into(),
        kind: "text".into(),
        value: val.as_deref().unwrap_or("").into(),
        required: false,
        min: 0.0,
        max: 0.0,
        has_bounds: false,
        options: vec![],
    };
    let number = |key: &str, label: &str, val: &Option<f32>, min: f32, max: f32| FieldRowData {
        key: format!("{prefix}.{key}"),
        label: label.into(),
        kind: "number".into(),
        value: val.map(|v| format!("{v}")).unwrap_or_default(),
        required: false,
        min,
        max,
        has_bounds: true,
        options: vec![],
    };
    let color = |key: &str, label: &str, val: &Option<String>| FieldRowData {
        key: format!("{prefix}.{key}"),
        label: label.into(),
        kind: "color".into(),
        value: val.as_deref().unwrap_or("#000000").into(),
        required: false,
        min: 0.0,
        max: 0.0,
        has_bounds: false,
        options: vec![],
    };
    rows.push(text("font_family", "Font family", &style.font_family));
    rows.push(number(
        "font_size",
        "Font size",
        &style.font_size,
        6.0,
        120.0,
    ));
    rows.push(number(
        "font_weight",
        "Font weight",
        &style.font_weight.map(|w| w as f32),
        100.0,
        900.0,
    ));
    rows.push(number(
        "line_height",
        "Line height",
        &style.line_height,
        0.5,
        4.0,
    ));
    rows.push(color("color", "Text color", &style.color));
    rows.push(color("background", "Background", &style.background));
    rows.push(color("accent", "Accent", &style.accent));
    rows.push(number(
        "base_spacing",
        "Spacing",
        &style.base_spacing,
        0.0,
        64.0,
    ));
    rows.push(number(
        "border_radius",
        "Radius",
        &style.border_radius,
        0.0,
        64.0,
    ));
    rows
}

impl Default for PropertiesPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel for PropertiesPanel {
    fn id(&self) -> i32 {
        Self::ID
    }
    fn label(&self) -> &'static str {
        "Properties"
    }
    fn title(&self) -> &'static str {
        "Properties"
    }
    fn hint(&self) -> &'static str {
        "Schema-driven editor for the selected node."
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "shell.panels.properties",
            "Properties",
            "Property editor for the selected component. Fields are type-aware: text, numbers, booleans, selects, and colors.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::{starter::register_builtins, BuilderDocument, ComponentRegistry, Node};
    use serde_json::json;

    fn setup() -> (BuilderDocument, ComponentRegistry) {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({ "spacing": 16 }),
                children: vec![Node {
                    id: "h".into(),
                    component: "text".into(),
                    props: json!({ "body": "Hi", "level": "h2" }),
                    children: vec![],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        (doc, reg)
    }

    #[test]
    fn empty_selection_yields_no_rows() {
        let (doc, reg) = setup();
        assert!(PropertiesPanel::rows(&doc, &reg, &None).is_empty());
    }

    #[test]
    fn text_schema_produces_three_rows() {
        let (doc, reg) = setup();
        let rows = PropertiesPanel::rows(&doc, &reg, &Some("h".into()));
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].key, "body");
        assert_eq!(rows[0].value, "Hi");
        assert_eq!(rows[1].key, "level");
        assert_eq!(rows[1].value, "h2");
        assert_eq!(rows[2].key, "href");
        assert_eq!(rows[2].value, "");
    }

    #[test]
    fn container_spacing_row_has_number_kind() {
        let (doc, reg) = setup();
        let rows = PropertiesPanel::rows(&doc, &reg, &Some("root".into()));
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].key, "spacing");
        assert_eq!(rows[0].kind, "integer");
        assert_eq!(rows[0].value, "16");
        assert!(rows[0].has_bounds);
        assert!((rows[0].min - 0.0).abs() < f32::EPSILON);
        assert!((rows[0].max - 64.0).abs() < f32::EPSILON);
        assert_eq!(rows[1].key, "padding");
        assert_eq!(rows[2].key, "border_width");
        assert_eq!(rows[3].key, "border_color");
    }

    #[test]
    fn layout_rows_use_typed_controls() {
        use prism_builder::layout::{FlowProps, LayoutMode};

        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({}),
                layout_mode: LayoutMode::Flow(FlowProps::default()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let rows = PropertiesPanel::layout_rows(&doc, &Some("root".into()));
        let kinds: Vec<(&str, &str)> = rows
            .iter()
            .map(|r| (r.key.as_str(), r.kind.as_str()))
            .collect();
        assert!(kinds.contains(&("layout.display", "select")));
        assert!(kinds.contains(&("layout.width_unit", "select")));
        assert!(kinds.contains(&("layout.height_unit", "select")));
        assert!(kinds.contains(&("layout.gap", "number")));
        assert!(kinds.contains(&("layout.align_items", "select")));
        assert!(kinds.contains(&("layout.justify_content", "select")));
        for row in &rows {
            assert_ne!(
                row.kind, "text",
                "no layout row should use plain text kind (found key={})",
                row.key
            );
        }
    }

    #[test]
    fn layout_padding_splits_into_four_sliders() {
        use prism_builder::layout::{FlowProps, LayoutMode};

        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({}),
                layout_mode: LayoutMode::Flow(FlowProps {
                    padding: prism_core::foundation::geometry::Edges::all(8.0),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let rows = PropertiesPanel::layout_rows(&doc, &Some("root".into()));
        let pad_rows: Vec<_> = rows
            .iter()
            .filter(|r| r.key.starts_with("layout.padding_"))
            .collect();
        assert_eq!(pad_rows.len(), 4);
        for r in &pad_rows {
            assert_eq!(r.kind, "number");
            assert!(r.has_bounds);
            assert_eq!(r.value, "8");
        }
    }

    #[test]
    fn layout_width_px_shows_slider() {
        use prism_builder::layout::{Dimension, FlowProps, LayoutMode};

        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({}),
                layout_mode: LayoutMode::Flow(FlowProps {
                    width: Dimension::Px { value: 200.0 },
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let rows = PropertiesPanel::layout_rows(&doc, &Some("root".into()));
        let unit_row = rows.iter().find(|r| r.key == "layout.width_unit").unwrap();
        assert_eq!(unit_row.kind, "select");
        assert_eq!(unit_row.value, "px");

        let val_row = rows.iter().find(|r| r.key == "layout.width_value").unwrap();
        assert_eq!(val_row.kind, "number");
        assert!(val_row.has_bounds);
        assert_eq!(val_row.value, "200");
    }

    #[test]
    fn transform_rows_default_identity() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({}),
                ..Default::default()
            }),
            ..Default::default()
        };

        let rows = PropertiesPanel::transform_rows(&doc, &Some("root".into()));
        assert_eq!(rows.len(), 6);
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "transform.x",
                "transform.y",
                "transform.rotation",
                "transform.scale_x",
                "transform.scale_y",
                "transform.anchor"
            ]
        );
        assert_eq!(rows[0].value, "0");
        assert_eq!(rows[1].value, "0");
        assert_eq!(rows[2].value, "0");
        assert_eq!(rows[3].value, "1");
        assert_eq!(rows[4].value, "1");
        assert_eq!(rows[5].value, "top-left");
        assert_eq!(rows[5].kind, "select");
    }

    #[test]
    fn transform_rows_with_offset() {
        use prism_core::foundation::spatial::{Anchor, Transform2D};

        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({}),
                transform: Transform2D {
                    position: [120.0, 45.0],
                    anchor: Anchor::Center,
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };

        let rows = PropertiesPanel::transform_rows(&doc, &Some("root".into()));
        assert_eq!(rows[0].value, "120");
        assert_eq!(rows[1].value, "45");
        assert_eq!(rows[5].value, "center");
    }

    #[test]
    fn transform_rows_empty_on_no_selection() {
        let (doc, _) = setup();
        assert!(PropertiesPanel::transform_rows(&doc, &None).is_empty());
    }

    #[test]
    fn selected_component_resolves_through_registry() {
        let (doc, reg) = setup();
        let _ = reg;
        assert_eq!(
            PropertiesPanel::selected_component(&doc, &Some("h".into())),
            "text"
        );
        assert_eq!(
            PropertiesPanel::selected_component(&doc, &Some("root".into())),
            "container"
        );
        assert_eq!(PropertiesPanel::selected_component(&doc, &None), "");
    }

    #[test]
    fn sections_default_node_collapses_transform_layout_appearance() {
        let (doc, reg) = setup();
        let sections = PropertiesPanel::sections(&doc, &reg, &Some("h".into()), None);
        assert!(sections.iter().any(|s| s.id == "transform"));
        assert!(sections.iter().any(|s| s.id == "component"));
        assert!(sections.iter().any(|s| s.id == "layout"));
        assert!(sections.iter().any(|s| s.id == "appearance"));
        let transform = sections.iter().find(|s| s.id == "transform").unwrap();
        let component = sections.iter().find(|s| s.id == "component").unwrap();
        let layout = sections.iter().find(|s| s.id == "layout").unwrap();
        let appearance = sections.iter().find(|s| s.id == "appearance").unwrap();
        assert!(
            transform.collapsed,
            "transform should auto-collapse at defaults"
        );
        assert!(!component.collapsed, "component should always expand");
        assert!(layout.collapsed, "layout should auto-collapse at defaults");
        assert!(
            appearance.collapsed,
            "appearance should auto-collapse at defaults"
        );
    }

    #[test]
    fn sections_non_default_transform_expands() {
        use prism_core::foundation::spatial::Transform2D;
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n".into(),
                component: "text".into(),
                props: json!({ "body": "Hello" }),
                transform: Transform2D {
                    position: [50.0, 0.0],
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let sections = PropertiesPanel::sections(&doc, &reg, &Some("n".into()), None);
        assert!(
            !sections[0].collapsed,
            "non-default transform should expand"
        );
    }

    #[test]
    fn sections_non_default_layout_expands() {
        use prism_builder::layout::{FlowDisplay, FlowProps, LayoutMode};
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n".into(),
                component: "container".into(),
                props: json!({}),
                layout_mode: LayoutMode::Flow(FlowProps {
                    display: FlowDisplay::Flex,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let sections = PropertiesPanel::sections(&doc, &reg, &Some("n".into()), None);
        let layout_section = sections.iter().find(|s| s.id == "layout").unwrap();
        assert!(!layout_section.collapsed, "flex layout should expand");
    }

    #[test]
    fn sections_non_default_style_expands_appearance() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n".into(),
                component: "text".into(),
                props: json!({}),
                style: prism_builder::style::StyleProperties {
                    color: Some("#ff0000".into()),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let sections = PropertiesPanel::sections(&doc, &reg, &Some("n".into()), None);
        let appearance = sections.iter().find(|s| s.id == "appearance").unwrap();
        assert!(
            !appearance.collapsed,
            "node with style override should expand appearance"
        );
    }

    #[test]
    fn sections_no_selection_shows_page_styles() {
        let sections = PropertiesPanel::sections(
            &BuilderDocument::default(),
            &ComponentRegistry::new(),
            &None,
            None,
        );
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].id, "appearance");
        assert_eq!(sections[0].label, "Page Styles");
    }

    #[test]
    fn sections_component_label_is_capitalized() {
        let (doc, reg) = setup();
        let sections = PropertiesPanel::sections(&doc, &reg, &Some("h".into()), None);
        let component = sections.iter().find(|s| s.id == "component").unwrap();
        assert_eq!(component.label, "Text");
    }

    #[test]
    fn flatten_sections_interleaves_headers_and_fields() {
        let (doc, reg) = setup();
        let sections = PropertiesPanel::sections(&doc, &reg, &Some("h".into()), None);
        let flat = PropertiesPanel::flatten_sections(&sections);
        let section_headers: Vec<&str> = flat
            .iter()
            .filter(|r| r.kind == "section")
            .map(|r| r.label.as_str())
            .collect();
        assert!(section_headers.contains(&"Transform"));
        assert!(section_headers.contains(&"Text"));
        assert!(section_headers.contains(&"Layout"));
        assert!(section_headers.contains(&"Appearance"));
        let expanded_headers: Vec<&str> = flat
            .iter()
            .filter(|r| r.kind == "section" && r.value == "expanded")
            .map(|r| r.label.as_str())
            .collect();
        assert!(expanded_headers.contains(&"Text"));
    }

    #[test]
    fn flatten_sections_collapsed_sections_have_no_field_rows() {
        let (doc, reg) = setup();
        let sections = PropertiesPanel::sections(&doc, &reg, &Some("h".into()), None);
        let flat = PropertiesPanel::flatten_sections(&sections);
        let after_transform: Vec<&str> = flat
            .iter()
            .skip_while(|r| !(r.kind == "section" && r.label == "Transform"))
            .skip(1)
            .take_while(|r| r.kind != "section")
            .map(|r| r.key.as_str())
            .collect();
        assert!(
            after_transform.is_empty(),
            "collapsed Transform should have no field rows after its header"
        );
    }

    #[test]
    fn appearance_rows_show_origin_in_label() {
        use prism_builder::PrismApp;
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let app = PrismApp {
            id: "test".into(),
            name: "Test".into(),
            description: String::new(),
            icon: prism_builder::AppIcon::Cube,
            pages: vec![],
            active_page: 0,
            navigation: prism_builder::NavigationConfig::default(),
            style: StyleProperties {
                font_family: Some("Inter".into()),
                color: Some("#000".into()),
                ..Default::default()
            },
        };
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n".into(),
                component: "text".into(),
                props: json!({}),
                style: StyleProperties {
                    font_size: Some(24.0),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let sections = PropertiesPanel::sections(&doc, &reg, &Some("n".into()), Some(&app));
        let appearance = sections.iter().find(|s| s.id == "appearance").unwrap();
        let font_family_row = appearance
            .rows
            .iter()
            .find(|r| r.key.contains("font_family"))
            .unwrap();
        assert!(
            font_family_row.label.contains("(app)"),
            "inherited from app should show origin"
        );
        assert!(
            font_family_row.key.starts_with("inherited."),
            "inherited keys should have inherited prefix"
        );
        let font_size_row = appearance
            .rows
            .iter()
            .find(|r| r.key.contains("font_size"))
            .unwrap();
        assert!(
            !font_size_row.label.contains("("),
            "node-level override should not show origin"
        );
        assert!(
            font_size_row.key.starts_with("style."),
            "node overrides should use style. prefix"
        );
    }

    #[test]
    fn button_sections_include_variants() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "btn".into(),
                component: "button".into(),
                props: json!({ "text": "Click" }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let sections = PropertiesPanel::sections(&doc, &reg, &Some("btn".into()), None);
        let ids: Vec<&str> = sections.iter().map(|s| s.id.as_str()).collect();
        assert!(
            ids.contains(&"variants"),
            "button should have variants section"
        );
        assert!(
            !ids.contains(&"signals"),
            "signals belong in the dedicated Signals panel, not Properties"
        );
    }

    #[test]
    fn facet_rows_show_prefab_select_and_bindings() {
        use prism_builder::{FacetDataSource, FacetDef, FacetKind, FacetLayout};

        let mut doc = BuilderDocument::default();
        let facet_id = "facet:f1".to_string();
        // Use card prefab (add it to doc.prefabs so look-up works)
        doc.prefabs
            .insert("card".into(), prism_builder::card_prefab_def());
        doc.facets.insert(
            facet_id.clone(),
            FacetDef {
                id: facet_id.clone(),
                label: "Test".into(),
                description: String::new(),
                kind: FacetKind::List,
                schema_id: None,
                data: FacetDataSource::Static {
                    items: vec![],
                    records: vec![],
                },
                bindings: vec![],
                variant_rules: vec![],
                layout: FacetLayout::default(),
                template: FacetTemplate::default(),
                output: FacetOutput::default(),
                resolved_data: None,
            },
        );
        let node = Node {
            id: "n1".into(),
            component: "facet".into(),
            props: serde_json::json!({ "facet_id": facet_id }),
            ..Default::default()
        };
        let rows = PropertiesPanel::facet_rows(&doc, &node);
        let prefab_row = rows.iter().find(|r| r.key == "facet.component_id").unwrap();
        assert_eq!(prefab_row.kind, "select");
        assert!(prefab_row.options.contains(&"card".to_string()));
        // Should have binding rows for "title" and "body" slots
        let binding_rows: Vec<_> = rows
            .iter()
            .filter(|r| r.key.starts_with("facet.binding."))
            .collect();
        assert_eq!(binding_rows.len(), 2);
        assert!(binding_rows.iter().any(|r| r.key == "facet.binding.title"));
        assert!(binding_rows.iter().any(|r| r.key == "facet.binding.body"));
    }

    #[test]
    fn facet_component_rows_use_select_for_facet_id() {
        use prism_builder::{FacetDataSource, FacetDef, FacetKind, FacetLayout};

        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let facet_id = "facet:f1".to_string();
        let mut doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "facet".into(),
                props: json!({ "facet_id": &facet_id }),
                ..Default::default()
            }),
            ..Default::default()
        };
        doc.facets.insert(
            facet_id.clone(),
            FacetDef {
                id: facet_id.clone(),
                label: "Test".into(),
                description: String::new(),
                kind: FacetKind::List,
                schema_id: None,
                data: FacetDataSource::Static {
                    items: vec![],
                    records: vec![],
                },
                bindings: vec![],
                variant_rules: vec![],
                layout: FacetLayout::default(),
                template: FacetTemplate::default(),
                output: FacetOutput::default(),
                resolved_data: None,
            },
        );
        let rows = PropertiesPanel::rows(&doc, &reg, &Some("n1".into()));
        let fid_row = rows.iter().find(|r| r.key == "facet_id").unwrap();
        assert_eq!(fid_row.kind, "select");
        assert!(fid_row.options.contains(&facet_id));
        assert!(
            rows.iter().any(|r| r.key == "max_items"),
            "List facets should show max_items"
        );
    }

    #[test]
    fn facet_component_rows_hide_max_items_for_non_list() {
        use prism_builder::{AggregateOp, FacetDef, FacetKind};

        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let facet_id = "facet:agg".to_string();
        let mut doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "facet".into(),
                props: json!({ "facet_id": &facet_id }),
                ..Default::default()
            }),
            ..Default::default()
        };
        doc.facets.insert(
            facet_id.clone(),
            FacetDef {
                id: facet_id.clone(),
                label: "Sum".into(),
                kind: FacetKind::Aggregate {
                    operation: AggregateOp::Count,
                    field: None,
                },
                ..Default::default()
            },
        );
        let rows = PropertiesPanel::rows(&doc, &reg, &Some("n1".into()));
        assert!(
            !rows.iter().any(|r| r.key == "max_items"),
            "non-List facets should not show max_items"
        );
        let fid_row = rows.iter().find(|r| r.key == "facet_id").unwrap();
        assert_eq!(fid_row.kind, "select");
    }

    #[test]
    fn facet_data_fallback_uses_select() {
        let doc = BuilderDocument::default();
        let node = Node {
            id: "n1".into(),
            component: "facet".into(),
            props: json!({ "facet_id": "missing" }),
            ..Default::default()
        };
        let rows = PropertiesPanel::facet_rows(&doc, &node);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "select");
        assert_eq!(rows[0].key, "facet.facet_id");
    }
}
