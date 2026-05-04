#![allow(unused_imports)]

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

use super::{
    dimension_slider, dimension_unit, format_align, format_anchor, format_display, format_f32,
    format_flex_direction, format_justify, format_number, layout_number, layout_select,
    placement_slider, placement_type, row_from_spec, style_rows_from, FieldRowData,
    PropertiesPanel, PropertySection, StyleOrigin,
};

impl PropertiesPanel {
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
}
