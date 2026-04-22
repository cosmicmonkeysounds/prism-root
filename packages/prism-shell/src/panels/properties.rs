//! Properties panel — the schema-driven field-row editor for the
//! currently selected node. Walks the component's [`FieldSpec`]
//! list and emits one field row per entry, reading the current
//! value straight out of the node's `props` via [`FieldValue`].
//!
//! The Slint side paints each row as a label + value + hint in a
//! vertical list; editing is wired up when the store grows a
//! mutate-prop action in Phase 4. Read-only for now is fine — the
//! important thing for Phase 3 is that the schema walks end-to-end.

use prism_builder::layout::{
    AlignOption, Dimension, FlexDirection, FlowDisplay, GridPlacement, JustifyOption, LayoutMode,
};
use prism_builder::{BuilderDocument, ComponentRegistry, FieldKind, FieldSpec, FieldValue, NodeId};
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

    /// Produce layout-specific [`FieldRowData`] items for the
    /// selected node's `LayoutMode` / `FlowProps`.
    pub fn layout_rows(doc: &BuilderDocument, selected: &Option<NodeId>) -> Vec<FieldRowData> {
        let Some(selected_id) = selected else {
            return vec![];
        };
        let Some(node) = doc.root.as_ref().and_then(|n| n.find(selected_id)) else {
            return vec![];
        };
        match &node.layout_mode {
            LayoutMode::Flow(flow) => {
                let mut rows = vec![layout_select(
                    "layout.display",
                    "Display",
                    format_display(flow.display),
                    vec!["block", "flex", "grid", "none"],
                )];

                // Width — select for unit + slider for value
                rows.push(layout_select(
                    "layout.width_unit",
                    "Width Unit",
                    dimension_unit(flow.width),
                    vec!["auto", "px", "%"],
                ));
                if let Some((val, lo, hi)) = dimension_slider(flow.width) {
                    rows.push(layout_number(
                        "layout.width_value",
                        "Width",
                        format_f32(val),
                        lo,
                        hi,
                    ));
                }

                // Height — select for unit + slider for value
                rows.push(layout_select(
                    "layout.height_unit",
                    "Height Unit",
                    dimension_unit(flow.height),
                    vec!["auto", "px", "%"],
                ));
                if let Some((val, lo, hi)) = dimension_slider(flow.height) {
                    rows.push(layout_number(
                        "layout.height_value",
                        "Height",
                        format_f32(val),
                        lo,
                        hi,
                    ));
                }

                // Padding — 4 individual sliders
                if flow.padding != prism_core::foundation::geometry::Edges::ZERO {
                    rows.push(layout_number(
                        "layout.padding_top",
                        "Padding Top",
                        format_f32(flow.padding.top),
                        0.0,
                        256.0,
                    ));
                    rows.push(layout_number(
                        "layout.padding_right",
                        "Padding Right",
                        format_f32(flow.padding.right),
                        0.0,
                        256.0,
                    ));
                    rows.push(layout_number(
                        "layout.padding_bottom",
                        "Padding Bottom",
                        format_f32(flow.padding.bottom),
                        0.0,
                        256.0,
                    ));
                    rows.push(layout_number(
                        "layout.padding_left",
                        "Padding Left",
                        format_f32(flow.padding.left),
                        0.0,
                        256.0,
                    ));
                }

                // Margin — 4 individual sliders
                if flow.margin != prism_core::foundation::geometry::Edges::ZERO {
                    rows.push(layout_number(
                        "layout.margin_top",
                        "Margin Top",
                        format_f32(flow.margin.top),
                        0.0,
                        256.0,
                    ));
                    rows.push(layout_number(
                        "layout.margin_right",
                        "Margin Right",
                        format_f32(flow.margin.right),
                        0.0,
                        256.0,
                    ));
                    rows.push(layout_number(
                        "layout.margin_bottom",
                        "Margin Bottom",
                        format_f32(flow.margin.bottom),
                        0.0,
                        256.0,
                    ));
                    rows.push(layout_number(
                        "layout.margin_left",
                        "Margin Left",
                        format_f32(flow.margin.left),
                        0.0,
                        256.0,
                    ));
                }

                rows.push(layout_number(
                    "layout.gap",
                    "Gap",
                    format_f32(flow.gap),
                    0.0,
                    128.0,
                ));

                if flow.display == FlowDisplay::Flex {
                    rows.push(layout_select(
                        "layout.flex_direction",
                        "Direction",
                        format_flex_direction(flow.flex_direction),
                        vec!["row", "column", "row-reverse", "column-reverse"],
                    ));
                    rows.push(layout_number(
                        "layout.flex_grow",
                        "Flex Grow",
                        format_f32(flow.flex_grow),
                        0.0,
                        10.0,
                    ));
                    rows.push(layout_number(
                        "layout.flex_shrink",
                        "Flex Shrink",
                        format_f32(flow.flex_shrink),
                        0.0,
                        10.0,
                    ));
                }

                rows.push(layout_select(
                    "layout.align_items",
                    "Align Items",
                    format_align(flow.align_items),
                    vec!["auto", "start", "end", "center", "stretch", "baseline"],
                ));

                rows.push(layout_select(
                    "layout.justify_content",
                    "Justify",
                    format_justify(flow.justify_content),
                    vec![
                        "start",
                        "end",
                        "center",
                        "space-between",
                        "space-around",
                        "space-evenly",
                        "stretch",
                    ],
                ));

                if flow.display == FlowDisplay::Grid
                    || !matches!(flow.grid_column, GridPlacement::Auto)
                {
                    rows.push(layout_select(
                        "layout.grid_column_type",
                        "Grid Col Type",
                        placement_type(flow.grid_column),
                        vec!["auto", "line", "span"],
                    ));
                    if let Some((val, hi)) = placement_slider(flow.grid_column) {
                        rows.push(layout_number(
                            "layout.grid_column_value",
                            "Grid Col",
                            format_f32(val),
                            1.0,
                            hi,
                        ));
                    }
                }

                if flow.display == FlowDisplay::Grid
                    || !matches!(flow.grid_row, GridPlacement::Auto)
                {
                    rows.push(layout_select(
                        "layout.grid_row_type",
                        "Grid Row Type",
                        placement_type(flow.grid_row),
                        vec!["auto", "line", "span"],
                    ));
                    if let Some((val, hi)) = placement_slider(flow.grid_row) {
                        rows.push(layout_number(
                            "layout.grid_row_value",
                            "Grid Row",
                            format_f32(val),
                            1.0,
                            hi,
                        ));
                    }
                }

                rows
            }
            LayoutMode::Free => {
                vec![layout_select(
                    "layout.display",
                    "Display",
                    "free".into(),
                    vec!["free", "block", "flex", "grid", "none"],
                )]
            }
        }
    }

    /// Produce one [`FieldRowData`] per entry in the selected
    /// component's schema. Returns an empty list if nothing is
    /// selected or the component is missing from the registry.
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
        component
            .schema()
            .into_iter()
            .map(|spec| row_from_spec(&spec, &node.props))
            .collect()
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

fn format_f32(v: f32) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
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
                    component: "heading".into(),
                    props: json!({ "text": "Hi", "level": 2 }),
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
    fn heading_schema_produces_two_rows() {
        let (doc, reg) = setup();
        let rows = PropertiesPanel::rows(&doc, &reg, &Some("h".into()));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].key, "text");
        assert_eq!(rows[0].value, "Hi");
        assert!(rows[0].required);
        assert_eq!(rows[1].key, "level");
        assert_eq!(rows[1].value, "2");
    }

    #[test]
    fn container_spacing_row_has_number_kind() {
        let (doc, reg) = setup();
        let rows = PropertiesPanel::rows(&doc, &reg, &Some("root".into()));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "spacing");
        assert_eq!(rows[0].kind, "integer");
        assert_eq!(rows[0].value, "16");
        assert!(rows[0].has_bounds);
        assert!((rows[0].min - 0.0).abs() < f32::EPSILON);
        assert!((rows[0].max - 64.0).abs() < f32::EPSILON);
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
    fn selected_component_resolves_through_registry() {
        let (doc, reg) = setup();
        let _ = reg;
        assert_eq!(
            PropertiesPanel::selected_component(&doc, &Some("h".into())),
            "heading"
        );
        assert_eq!(
            PropertiesPanel::selected_component(&doc, &Some("root".into())),
            "container"
        );
        assert_eq!(PropertiesPanel::selected_component(&doc, &None), "");
    }
}
