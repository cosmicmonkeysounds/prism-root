//! Properties panel — unified section-based inspector for the
//! currently selected node. Inspired by Unity's Inspector:
//! every node is a "game object" with Transform, Component props,
//! Layout, and Appearance sections. Collapse state is automatic —
//! sections at their defaults collapse; modified sections expand.
//!
//! The Slint side receives a single flat `property-rows` model
//! where section headers and field rows are interleaved. One
//! unified edit callback routes by key prefix.

use prism_builder::card_prefab_def;
use prism_builder::layout::{
    AlignOption, Dimension, FlexDirection, FlowDisplay, GridPlacement, JustifyOption, LayoutMode,
};
use prism_builder::style::StyleProperties;
use prism_builder::{
    AggregateOp, FacetDataSource, FacetDirection, FacetKind, FacetOutput, FacetTemplate,
    AGGREGATE_OP_TAGS, FACET_KIND_TAGS,
};
use prism_builder::{
    BuilderDocument, ComponentRegistry, FieldKind, FieldSpec, FieldValue, Node, NodeId, PrismApp,
};
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
                .map(|(i, m)| FieldRowData {
                    key: format!("modifier.{i}"),
                    label: format!("{:?}", m.kind),
                    kind: "text".into(),
                    value: format!("{:?}", m.kind),
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
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
                        FieldRowData {
                            key: axis.key.clone(),
                            label: axis.label.clone(),
                            kind: "select".into(),
                            value: current.to_string(),
                            required: false,
                            min: 0.0,
                            max: 0.0,
                            has_bounds: false,
                            options: axis.options.iter().map(|o| o.value.clone()).collect(),
                        }
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

    /// Build appearance rows showing the cascade for a selected node.
    /// Node-level overrides come first, then inherited values (dimmed
    /// via a "inherited." key prefix so the Slint side can style them).
    fn appearance_rows(node: &Node, app: Option<&PrismApp>) -> Vec<FieldRowData> {
        let default_style = StyleProperties::default();
        let app_style = app.map(|a| &a.style).unwrap_or(&default_style);
        let page_style = app
            .and_then(|a| a.pages.get(a.active_page))
            .map(|p| &p.style)
            .unwrap_or(&default_style);

        let mut rows = Vec::new();

        struct CascadeField<'a> {
            key: &'a str,
            label: &'a str,
            kind: &'a str,
            node_val: Option<String>,
            page_val: Option<String>,
            app_val: Option<String>,
            min: f32,
            max: f32,
        }

        let fields = [
            CascadeField {
                key: "font_family",
                label: "Font family",
                kind: "text",
                node_val: node.style.font_family.clone(),
                page_val: page_style.font_family.clone(),
                app_val: app_style.font_family.clone(),
                min: 0.0,
                max: 0.0,
            },
            CascadeField {
                key: "font_size",
                label: "Font size",
                kind: "number",
                node_val: node.style.font_size.map(|v| format!("{v}")),
                page_val: page_style.font_size.map(|v| format!("{v}")),
                app_val: app_style.font_size.map(|v| format!("{v}")),
                min: 6.0,
                max: 120.0,
            },
            CascadeField {
                key: "font_weight",
                label: "Font weight",
                kind: "number",
                node_val: node.style.font_weight.map(|v| format!("{v}")),
                page_val: page_style.font_weight.map(|v| format!("{v}")),
                app_val: app_style.font_weight.map(|v| format!("{v}")),
                min: 100.0,
                max: 900.0,
            },
            CascadeField {
                key: "line_height",
                label: "Line height",
                kind: "number",
                node_val: node.style.line_height.map(|v| format!("{v}")),
                page_val: page_style.line_height.map(|v| format!("{v}")),
                app_val: app_style.line_height.map(|v| format!("{v}")),
                min: 0.5,
                max: 4.0,
            },
            CascadeField {
                key: "color",
                label: "Text color",
                kind: "color",
                node_val: node.style.color.clone(),
                page_val: page_style.color.clone(),
                app_val: app_style.color.clone(),
                min: 0.0,
                max: 0.0,
            },
            CascadeField {
                key: "background",
                label: "Background",
                kind: "color",
                node_val: node.style.background.clone(),
                page_val: page_style.background.clone(),
                app_val: app_style.background.clone(),
                min: 0.0,
                max: 0.0,
            },
            CascadeField {
                key: "accent",
                label: "Accent",
                kind: "color",
                node_val: node.style.accent.clone(),
                page_val: page_style.accent.clone(),
                app_val: app_style.accent.clone(),
                min: 0.0,
                max: 0.0,
            },
            CascadeField {
                key: "base_spacing",
                label: "Spacing",
                kind: "number",
                node_val: node.style.base_spacing.map(|v| format!("{v}")),
                page_val: page_style.base_spacing.map(|v| format!("{v}")),
                app_val: app_style.base_spacing.map(|v| format!("{v}")),
                min: 0.0,
                max: 64.0,
            },
            CascadeField {
                key: "border_radius",
                label: "Radius",
                kind: "number",
                node_val: node.style.border_radius.map(|v| format!("{v}")),
                page_val: page_style.border_radius.map(|v| format!("{v}")),
                app_val: app_style.border_radius.map(|v| format!("{v}")),
                min: 0.0,
                max: 64.0,
            },
        ];

        for f in &fields {
            let (resolved, origin) = if let Some(ref v) = f.node_val {
                (v.clone(), StyleOrigin::Node)
            } else if let Some(ref v) = f.page_val {
                (v.clone(), StyleOrigin::Page)
            } else if let Some(ref v) = f.app_val {
                (v.clone(), StyleOrigin::App)
            } else {
                (String::new(), StyleOrigin::Default)
            };

            let key_prefix = match origin {
                StyleOrigin::Node => "style.",
                _ => "inherited.style.",
            };

            rows.push(FieldRowData {
                key: format!("{key_prefix}{}", f.key),
                label: match origin {
                    StyleOrigin::Node => f.label.to_string(),
                    StyleOrigin::Page => format!("{} (page)", f.label),
                    StyleOrigin::App => format!("{} (app)", f.label),
                    StyleOrigin::Default => format!("{} (—)", f.label),
                },
                kind: f.kind.into(),
                value: resolved,
                required: false,
                min: f.min,
                max: f.max,
                has_bounds: f.kind == "number",
                options: vec![],
            });
        }

        rows
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
                    vec![
                        "block", "flex", "grid", "none", "absolute", "relative", "free",
                    ],
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
                    vec![
                        "free", "absolute", "relative", "block", "flex", "grid", "none",
                    ],
                )]
            }
            LayoutMode::Absolute(abs) => {
                let mut rows = vec![layout_select(
                    "layout.display",
                    "Display",
                    "absolute".into(),
                    vec![
                        "absolute", "relative", "free", "block", "flex", "grid", "none",
                    ],
                )];
                rows.push(layout_select(
                    "layout.width_unit",
                    "Width Unit",
                    dimension_unit(abs.width),
                    vec!["auto", "px", "%"],
                ));
                if let Some((val, lo, hi)) = dimension_slider(abs.width) {
                    rows.push(layout_number(
                        "layout.width_value",
                        "Width",
                        format_f32(val),
                        lo,
                        hi,
                    ));
                }
                rows.push(layout_select(
                    "layout.height_unit",
                    "Height Unit",
                    dimension_unit(abs.height),
                    vec!["auto", "px", "%"],
                ));
                if let Some((val, lo, hi)) = dimension_slider(abs.height) {
                    rows.push(layout_number(
                        "layout.height_value",
                        "Height",
                        format_f32(val),
                        lo,
                        hi,
                    ));
                }
                rows
            }
            LayoutMode::Relative(flow) => {
                let mut rows = vec![layout_select(
                    "layout.display",
                    "Display",
                    "relative".into(),
                    vec![
                        "relative", "absolute", "free", "block", "flex", "grid", "none",
                    ],
                )];
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
                rows.push(layout_number(
                    "layout.gap",
                    "Gap",
                    format_f32(flow.gap),
                    0.0,
                    128.0,
                ));
                rows
            }
        }
    }

    /// Produce transform rows (position, rotation, scale, anchor) for
    /// the selected node. Godot-style: every node has a transform.
    pub fn transform_rows(doc: &BuilderDocument, selected: &Option<NodeId>) -> Vec<FieldRowData> {
        let Some(selected_id) = selected else {
            return vec![];
        };
        let Some(node) = doc.root.as_ref().and_then(|n| n.find(selected_id)) else {
            return vec![];
        };
        let t = &node.transform;
        vec![
            layout_number(
                "transform.x",
                "Position X",
                format_f32(t.position[0]),
                -4000.0,
                4000.0,
            ),
            layout_number(
                "transform.y",
                "Position Y",
                format_f32(t.position[1]),
                -4000.0,
                4000.0,
            ),
            layout_number(
                "transform.rotation",
                "Rotation",
                format_f32(t.rotation.to_degrees()),
                -360.0,
                360.0,
            ),
            layout_number(
                "transform.scale_x",
                "Scale X",
                format_f32(t.scale[0]),
                0.01,
                10.0,
            ),
            layout_number(
                "transform.scale_y",
                "Scale Y",
                format_f32(t.scale[1]),
                0.01,
                10.0,
            ),
            layout_select(
                "transform.anchor",
                "Anchor",
                format_anchor(t.anchor),
                vec![
                    "top-left",
                    "top-center",
                    "top-right",
                    "center-left",
                    "center",
                    "center-right",
                    "bottom-left",
                    "bottom-center",
                    "bottom-right",
                    "stretch",
                ],
            ),
        ]
    }

    fn facet_rows(doc: &BuilderDocument, node: &Node) -> Vec<FieldRowData> {
        let facet_id = node
            .props
            .get("facet_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let facet_ids: Vec<String> = doc.facets.keys().cloned().collect();

        let Some(def) = doc.facets.get(facet_id) else {
            return vec![FieldRowData {
                key: "facet.facet_id".into(),
                label: "Facet ID".into(),
                kind: "select".into(),
                value: facet_id.to_string(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: facet_ids,
            }];
        };

        let schema = def
            .schema_id
            .as_ref()
            .and_then(|sid| doc.facet_schemas.get(sid));

        // Schema selection dropdown
        let mut schema_options: Vec<String> = vec!["(none)".into()];
        for sid in doc.facet_schemas.keys() {
            schema_options.push(sid.clone());
        }
        let schema_value = def.schema_id.as_deref().unwrap_or("(none)").to_string();

        // Prefab dropdown (for ComponentRef template)
        let mut prefab_options: Vec<String> = vec!["card".into()];
        for id in doc.prefabs.keys() {
            if id != "card" {
                prefab_options.push(id.clone());
            }
        }

        // Template type
        let template_value = match &def.template {
            FacetTemplate::ComponentRef { .. } => "component-ref",
            FacetTemplate::Inline { .. } => "inline",
        };

        // Output type
        let output_value = match &def.output {
            FacetOutput::Repeated => "repeated",
            FacetOutput::Scalar { .. } => "scalar",
        };

        // ── Common header (all kinds) ─────────────────────────
        let mut rows = vec![
            FieldRowData {
                key: "facet.kind".into(),
                label: "Kind".into(),
                kind: "select".into(),
                value: def.kind.tag().to_string(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: FACET_KIND_TAGS.iter().map(|s| (*s).into()).collect(),
            },
            FieldRowData {
                key: "facet.schema_id".into(),
                label: "Schema".into(),
                kind: "select".into(),
                value: schema_value,
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: schema_options,
            },
            FieldRowData {
                key: "facet.template_type".into(),
                label: "Template".into(),
                kind: "select".into(),
                value: template_value.to_string(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec!["component-ref".into(), "inline".into()],
            },
        ];

        // Show prefab selector only for ComponentRef templates
        if matches!(def.template, FacetTemplate::ComponentRef { .. }) {
            rows.push(FieldRowData {
                key: "facet.component_id".into(),
                label: "Component template".into(),
                kind: "select".into(),
                value: def.effective_component_id().unwrap_or("card").to_string(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: prefab_options,
            });
        }

        // Output type selector
        rows.push(FieldRowData {
            key: "facet.output_type".into(),
            label: "Output".into(),
            kind: "select".into(),
            value: output_value.to_string(),
            required: true,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec!["repeated".into(), "scalar".into()],
        });

        // Scalar target fields
        if let FacetOutput::Scalar {
            target_node,
            target_prop,
        } = &def.output
        {
            rows.push(FieldRowData {
                key: "facet.scalar_target_node".into(),
                label: "Target node".into(),
                kind: "text".into(),
                value: target_node.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: "facet.scalar_target_prop".into(),
                label: "Target prop".into(),
                kind: "text".into(),
                value: target_prop.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
        }

        // ── Kind-specific sections ────────────────────────────
        match &def.kind {
            FacetKind::List => {
                Self::push_data_source_rows(&mut rows, def);
                Self::push_layout_rows(&mut rows, def);
            }
            FacetKind::ObjectQuery { query } => {
                let entity_type = query.object_type.as_deref().unwrap_or("");
                let filter_str = query
                    .filters
                    .first()
                    .map(|f| {
                        let op_str = match f.op {
                            prism_core::widget::FilterOp::Eq => "==",
                            prism_core::widget::FilterOp::Neq => "!=",
                            _ => "==",
                        };
                        let val = match &f.value {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        format!("{} {} {}", f.field, op_str, val)
                    })
                    .unwrap_or_default();
                let sort_str = query
                    .sort
                    .first()
                    .map(|s| {
                        if s.descending {
                            format!("-{}", s.field)
                        } else {
                            s.field.clone()
                        }
                    })
                    .unwrap_or_default();
                rows.push(FieldRowData {
                    key: "facet.entity_type".into(),
                    label: "Entity type".into(),
                    kind: "text".into(),
                    value: entity_type.to_string(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.oq_filter".into(),
                    label: "Filter".into(),
                    kind: "text".into(),
                    value: filter_str,
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.oq_sort_by".into(),
                    label: "Sort by".into(),
                    kind: "text".into(),
                    value: sort_str,
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.oq_limit".into(),
                    label: "Limit".into(),
                    kind: "integer".into(),
                    value: query.limit.map(|l| l.to_string()).unwrap_or_default(),
                    required: false,
                    min: 0.0,
                    max: 10000.0,
                    has_bounds: true,
                    options: vec![],
                });
                Self::push_layout_rows(&mut rows, def);
            }
            FacetKind::Script {
                source,
                language,
                graph,
            } => {
                rows.push(FieldRowData {
                    key: "facet.script_language".into(),
                    label: "Language".into(),
                    kind: "select".into(),
                    value: match language {
                        prism_builder::ScriptLanguage::Luau => "luau".to_string(),
                        prism_builder::ScriptLanguage::VisualGraph => "visual-graph".to_string(),
                    },
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec!["luau".into(), "visual-graph".into()],
                });
                match language {
                    prism_builder::ScriptLanguage::Luau => {
                        rows.push(FieldRowData {
                            key: "facet.script_source".into(),
                            label: "Luau script".into(),
                            kind: "textarea".into(),
                            value: source.clone(),
                            required: false,
                            min: 0.0,
                            max: 0.0,
                            has_bounds: false,
                            options: vec![],
                        });
                    }
                    prism_builder::ScriptLanguage::VisualGraph => {
                        let node_count = graph.as_ref().map(|g| g.nodes.len()).unwrap_or(0);
                        let edge_count = graph.as_ref().map(|g| g.edges.len()).unwrap_or(0);
                        rows.push(FieldRowData {
                            key: "facet.graph_info".into(),
                            label: "Graph".into(),
                            kind: "text".into(),
                            value: format!("{node_count} nodes, {edge_count} edges"),
                            required: false,
                            min: 0.0,
                            max: 0.0,
                            has_bounds: false,
                            options: vec![],
                        });
                        if !source.is_empty() {
                            rows.push(FieldRowData {
                                key: "facet.graph_source_preview".into(),
                                label: "Compiled source".into(),
                                kind: "textarea".into(),
                                value: source.clone(),
                                required: false,
                                min: 0.0,
                                max: 0.0,
                                has_bounds: false,
                                options: vec![],
                            });
                        }
                    }
                }
                Self::push_layout_rows(&mut rows, def);
            }
            FacetKind::Aggregate { operation, field } => {
                Self::push_data_source_rows(&mut rows, def);
                rows.push(FieldRowData {
                    key: "facet.agg_operation".into(),
                    label: "Operation".into(),
                    kind: "select".into(),
                    value: operation.tag().to_string(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: AGGREGATE_OP_TAGS.iter().map(|s| (*s).into()).collect(),
                });
                if !matches!(operation, AggregateOp::Count) {
                    rows.push(FieldRowData {
                        key: "facet.agg_field".into(),
                        label: "Field".into(),
                        kind: "text".into(),
                        value: field.as_deref().unwrap_or("").into(),
                        required: true,
                        min: 0.0,
                        max: 0.0,
                        has_bounds: false,
                        options: vec![],
                    });
                }
                if let AggregateOp::Join { separator } = operation {
                    rows.push(FieldRowData {
                        key: "facet.agg_separator".into(),
                        label: "Separator".into(),
                        kind: "text".into(),
                        value: separator.clone(),
                        required: false,
                        min: 0.0,
                        max: 0.0,
                        has_bounds: false,
                        options: vec![],
                    });
                }
            }
            FacetKind::Lookup {
                source_entity,
                edge_type,
                target_entity,
            } => {
                rows.push(FieldRowData {
                    key: "facet.lookup_source".into(),
                    label: "Source entity".into(),
                    kind: "text".into(),
                    value: source_entity.clone(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.lookup_edge".into(),
                    label: "Edge type".into(),
                    kind: "text".into(),
                    value: edge_type.clone(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                rows.push(FieldRowData {
                    key: "facet.lookup_target".into(),
                    label: "Target entity".into(),
                    kind: "text".into(),
                    value: target_entity.clone(),
                    required: true,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
                Self::push_layout_rows(&mut rows, def);
            }
        }

        // ── Bindings (all kinds) ──────────────────────────────
        Self::push_binding_rows(&mut rows, doc, def, schema);

        // ── Variant Rules ────────────────────────────────────
        Self::push_variant_rule_rows(&mut rows, def);

        // ── Records (List + static source + schema) ───────────
        if matches!(def.kind, FacetKind::List) {
            Self::push_record_rows(&mut rows, def, schema);
        }

        rows
    }

    fn push_data_source_rows(rows: &mut Vec<FieldRowData>, def: &prism_builder::FacetDef) {
        let (source_kind, item_count_label, source_id, filter_val, sort_val) = match &def.data {
            FacetDataSource::Static { items, records } => {
                let count = if records.is_empty() {
                    items.len()
                } else {
                    records.len()
                };
                (
                    "static",
                    format!("{count} items"),
                    String::new(),
                    String::new(),
                    String::new(),
                )
            }
            FacetDataSource::Resource { id } => (
                "resource",
                "resource".into(),
                id.clone(),
                String::new(),
                String::new(),
            ),
            FacetDataSource::Query { source, query } => {
                let f = query
                    .filters
                    .first()
                    .map(|qf| {
                        let op_str = match qf.op {
                            prism_core::widget::FilterOp::Eq => "==",
                            prism_core::widget::FilterOp::Neq => "!=",
                            _ => "==",
                        };
                        let val = match &qf.value {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        format!("{} {} {}", qf.field, op_str, val)
                    })
                    .unwrap_or_default();
                let s = query
                    .sort
                    .first()
                    .map(|qs| {
                        if qs.descending {
                            format!("-{}", qs.field)
                        } else {
                            qs.field.clone()
                        }
                    })
                    .unwrap_or_default();
                ("query", "query".into(), source.clone(), f, s)
            }
        };

        rows.push(FieldRowData {
            key: "facet.source_kind".into(),
            label: "Data source".into(),
            kind: "select".into(),
            value: source_kind.to_string(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec!["static".into(), "resource".into(), "query".into()],
        });

        if source_kind != "static" {
            rows.push(FieldRowData {
                key: "facet.source_id".into(),
                label: "Source resource ID".into(),
                kind: "text".into(),
                value: source_id,
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
        }

        if source_kind == "query" {
            rows.push(FieldRowData {
                key: "facet.filter".into(),
                label: "Filter expression".into(),
                kind: "text".into(),
                value: filter_val,
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: "facet.sort_by".into(),
                label: "Sort by field".into(),
                kind: "text".into(),
                value: sort_val,
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
        }

        rows.push(FieldRowData {
            key: "facet.item_count".into(),
            label: "Items".into(),
            kind: "text".into(),
            value: item_count_label,
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });
    }

    fn push_layout_rows(rows: &mut Vec<FieldRowData>, def: &prism_builder::FacetDef) {
        let direction_label = match def.layout.direction {
            FacetDirection::Row => "row",
            FacetDirection::Column => "column",
        };
        rows.extend([
            FieldRowData {
                key: "facet.direction".into(),
                label: "Direction".into(),
                kind: "select".into(),
                value: direction_label.to_string(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec!["column".into(), "row".into()],
            },
            FieldRowData {
                key: "facet.gap".into(),
                label: "Gap (px)".into(),
                kind: "integer".into(),
                value: format!("{}", def.layout.gap as u32),
                required: false,
                min: 0.0,
                max: 128.0,
                has_bounds: true,
                options: vec![],
            },
            FieldRowData {
                key: "facet.wrap".into(),
                label: "Wrap".into(),
                kind: "boolean".into(),
                value: def.layout.wrap.to_string(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            },
            FieldRowData {
                key: "facet.columns".into(),
                label: "Columns".into(),
                kind: "integer".into(),
                value: def.layout.columns.map(|c| c.to_string()).unwrap_or_default(),
                required: false,
                min: 0.0,
                max: 12.0,
                has_bounds: true,
                options: vec![],
            },
        ]);
    }

    fn push_binding_rows(
        rows: &mut Vec<FieldRowData>,
        doc: &BuilderDocument,
        def: &prism_builder::FacetDef,
        schema: Option<&prism_builder::FacetSchema>,
    ) {
        let cid = def.effective_component_id().unwrap_or("card");
        let prefab_exposed = if cid == "card" {
            doc.prefabs
                .get("card")
                .map(|p| p.exposed.clone())
                .unwrap_or_else(|| card_prefab_def().exposed)
        } else {
            doc.prefabs
                .get(cid)
                .map(|p| p.exposed.clone())
                .unwrap_or_default()
        };
        if prefab_exposed.is_empty() {
            return;
        }

        rows.push(FieldRowData {
            key: "facet.bindings_header".into(),
            label: "── Bindings ──".into(),
            kind: "text".into(),
            value: String::new(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });

        let schema_field_options: Vec<String> = if let Some(s) = schema {
            let mut opts = vec!["".into()];
            opts.extend(s.fields.iter().map(|f| f.key.clone()));
            opts
        } else {
            vec![]
        };

        for slot in &prefab_exposed {
            let bound_field = def
                .bindings
                .iter()
                .find(|b| b.slot_key == slot.key)
                .map(|b| b.item_field.clone())
                .unwrap_or_default();

            if schema_field_options.is_empty() {
                rows.push(FieldRowData {
                    key: format!("facet.binding.{}", slot.key),
                    label: format!("Bind: {}", slot.key),
                    kind: "text".into(),
                    value: bound_field,
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: vec![],
                });
            } else {
                rows.push(FieldRowData {
                    key: format!("facet.binding.{}", slot.key),
                    label: format!("Bind: {}", slot.key),
                    kind: "select".into(),
                    value: bound_field,
                    required: false,
                    min: 0.0,
                    max: 0.0,
                    has_bounds: false,
                    options: schema_field_options.clone(),
                });
            }
        }
    }

    fn push_variant_rule_rows(rows: &mut Vec<FieldRowData>, def: &prism_builder::FacetDef) {
        if def.variant_rules.is_empty() {
            rows.push(FieldRowData {
                key: "facet.add_variant_rule".into(),
                label: "+ Add variant rule".into(),
                kind: "text".into(),
                value: String::new(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            return;
        }

        rows.push(FieldRowData {
            key: "facet.variant_rules_header".into(),
            label: "── Variant Rules ──".into(),
            kind: "text".into(),
            value: String::new(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });

        for (i, rule) in def.variant_rules.iter().enumerate() {
            rows.push(FieldRowData {
                key: format!("facet.variant_rule.{i}.field"),
                label: "When field".into(),
                kind: "text".into(),
                value: rule.field.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("facet.variant_rule.{i}.value"),
                label: "equals".into(),
                kind: "text".into(),
                value: rule.value.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("facet.variant_rule.{i}.axis_key"),
                label: "set axis".into(),
                kind: "text".into(),
                value: rule.axis_key.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("facet.variant_rule.{i}.axis_value"),
                label: "to".into(),
                kind: "text".into(),
                value: rule.axis_value.clone(),
                required: true,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            rows.push(FieldRowData {
                key: format!("facet.remove_variant_rule.{i}"),
                label: "Remove rule".into(),
                kind: "text".into(),
                value: String::new(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
        }

        rows.push(FieldRowData {
            key: "facet.add_variant_rule".into(),
            label: "+ Add variant rule".into(),
            kind: "text".into(),
            value: String::new(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });
    }

    fn push_record_rows(
        rows: &mut Vec<FieldRowData>,
        def: &prism_builder::FacetDef,
        schema: Option<&prism_builder::FacetSchema>,
    ) {
        let FacetDataSource::Static { records, .. } = &def.data else {
            return;
        };
        let Some(s) = schema else {
            return;
        };
        if records.is_empty() {
            return;
        }

        rows.push(FieldRowData {
            key: "facet.records_header".into(),
            label: "── Records ──".into(),
            kind: "text".into(),
            value: String::new(),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: vec![],
        });
        for (ri, rec) in records.iter().enumerate() {
            rows.push(FieldRowData {
                key: format!("facet.record_header.{ri}"),
                label: format!("Record {} ({})", ri + 1, rec.id),
                kind: "text".into(),
                value: String::new(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            });
            for field in &s.fields {
                if matches!(field.kind, FieldKind::Calculation { .. }) {
                    continue;
                }
                let val = rec
                    .fields
                    .get(&field.key)
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Null => String::new(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();

                let (kind, opts, min, max, has_bounds) = match &field.kind {
                    FieldKind::Text
                    | FieldKind::TextArea
                    | FieldKind::Date
                    | FieldKind::DateTime
                    | FieldKind::File(_) => ("text", vec![], 0.0, 0.0, false),
                    FieldKind::Number(b) => (
                        "number",
                        vec![],
                        b.min.unwrap_or(0.0) as f32,
                        b.max.unwrap_or(0.0) as f32,
                        b.min.is_some() || b.max.is_some(),
                    ),
                    FieldKind::Currency { .. } => ("number", vec![], 0.0, 0.0, false),
                    FieldKind::Integer(b) => (
                        "integer",
                        vec![],
                        b.min.unwrap_or(0.0) as f32,
                        b.max.unwrap_or(0.0) as f32,
                        b.min.is_some() || b.max.is_some(),
                    ),
                    FieldKind::Duration => ("integer", vec![], 0.0, 0.0, false),
                    FieldKind::Boolean => ("boolean", vec![], 0.0, 0.0, false),
                    FieldKind::Color => ("color", vec![], 0.0, 0.0, false),
                    FieldKind::Select(options) => (
                        "select",
                        options.iter().map(|o| o.value.clone()).collect(),
                        0.0,
                        0.0,
                        false,
                    ),
                    FieldKind::Calculation { .. } => unreachable!(),
                };

                rows.push(FieldRowData {
                    key: format!("facet.record.{}.{}", ri, field.key),
                    label: field.label.clone(),
                    kind: kind.into(),
                    value: val,
                    required: field.required,
                    min,
                    max,
                    has_bounds,
                    options: opts,
                });
            }
        }
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
