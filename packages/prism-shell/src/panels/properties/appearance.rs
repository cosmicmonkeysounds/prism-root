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
    /// Build appearance rows showing the cascade for a selected node.
    /// Node-level overrides come first, then inherited values (dimmed
    /// via a "inherited." key prefix so the Slint side can style them).
    pub(super) fn appearance_rows(node: &Node, app: Option<&PrismApp>) -> Vec<FieldRowData> {
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
}
