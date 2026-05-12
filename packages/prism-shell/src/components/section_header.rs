//! `shell.section-header` — 36px collapsible-section header used in
//! sidebars, properties panels, and the inspector. Reads `label`,
//! `collapsed`, and `section-id` props; emits a single `section-toggled`
//! signal carrying the section id.
//!
//! Slint origin: `SectionHeader` in `ui/app.slint` (lines 524-566).
//!
//! Lowering: a 36px-tall row container with a chevron glyph + label,
//! a bottom hairline, and an optional "(default)" badge when collapsed.
//! The chevron source is *prop-conditional* (`chevron-left` when
//! collapsed, `chevron-down` when expanded) — that conditional resolves
//! at lower-time, no runtime state machinery needed.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, image_node, parse_color, LowerCtx},
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Sizing};
use serde_json::Value;

const ROW_HEIGHT: f32 = 36.0;
const CHEVRON_SIZE: f32 = 10.0;
const LABEL_FONT_SIZE: f32 = 11.0;
const BADGE_FONT_SIZE: f32 = 10.0;
const HAIRLINE_COLOR: &str = "#19000000";
const LABEL_COLOR_EXPANDED: &str = "#cc000000";
const LABEL_COLOR_COLLAPSED: &str = "#7f000000";
const BADGE_COLOR: &str = "#4c000000";

fn section_header_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("label", "Label").required(),
        FieldSpec::boolean("collapsed", "Collapsed").with_default(Value::Bool(false)),
        FieldSpec::text("section-id", "Section ID"),
    ]
}

fn section_header_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "section-toggled",
        "Fires when the header is clicked — payload carries the section id.",
    )
    .with_payload(vec![FieldSpec::text("section_id", "Section ID")])])
}

fn section_header_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let label = ctx.prop_str(node, "label");
    let collapsed = ctx.prop_bool(node, "collapsed", false);
    let section_id = ctx.prop_str(node, "section-id");

    let chevron_src = if collapsed {
        "icons/chevron-left.svg"
    } else {
        "icons/chevron-down.svg"
    };
    let chevron = image_node(
        format!("{}::chevron", node.id),
        chevron_src.into(),
        style,
        Sizing::Fixed(CHEVRON_SIZE),
        Sizing::Fixed(CHEVRON_SIZE),
    );

    let label_color = if collapsed {
        LABEL_COLOR_COLLAPSED
    } else {
        LABEL_COLOR_EXPANDED
    };
    let label_text = colored_text_node(
        format!("{}::label", node.id),
        label,
        style,
        LABEL_FONT_SIZE,
        label_color,
    );

    let mut row_children = vec![chevron, label_text];
    if collapsed {
        row_children.push(colored_text_node(
            format!("{}::badge", node.id),
            "(default)".into(),
            style,
            BADGE_FONT_SIZE,
            BADGE_COLOR,
        ));
    }

    let row = bare_container(format!("{}::row", node.id), row_children, |props| {
        props.direction = Direction::Row;
        props.gap = 6.0;
        props.padding = Padding {
            left: 4.0,
            right: 4.0,
            top: 0.0,
            bottom: 0.0,
        };
        props.height = Sizing::Fixed(ROW_HEIGHT - 1.0);
    });

    let hairline = bare_container(format!("{}::hairline", node.id), vec![], |props| {
        props.height = Sizing::Fixed(1.0);
        props.background = parse_color(HAIRLINE_COLOR);
    });

    // The outer container stacks (column) the row + hairline, and
    // declares `<header>` as the SSR semantic. Emitting the
    // section-id as a `data-section` attr keeps it discoverable for
    // CSS / scripted hosts without committing to a vocabulary the
    // walker has to know about.
    ctx.synthetic_container(node, style, vec![row, hairline], |props| {
        props.direction = Direction::Column;
        props.height = Sizing::Fixed(ROW_HEIGHT);
        let mut semantic = prism_ui_runtime::layout::Semantic::tag("header");
        if !section_id.is_empty() {
            semantic = semantic.with_attr("data-section", &section_id);
        }
        if collapsed {
            semantic = semantic.with_attr("data-collapsed", "true");
        }
        props.semantic = semantic;
    })
}

pub const SECTION_HEADER_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.section-header", section_header_schema)
        .lower(section_header_lower)
        .signals(section_header_signals);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::Block;
    use serde_json::json;

    fn lower_one(node: &BuilderNode) -> UiNode {
        lower_with(node, section_header_lower)
    }

    fn header(props: Value) -> BuilderNode {
        test_node("h", "shell.section-header", props)
    }

    fn first_child_image_source(ui: &UiNode) -> &str {
        let UiNode::Container { children, .. } = ui else {
            panic!("outer is not container")
        };
        let UiNode::Container {
            children: row_children,
            ..
        } = &children[0]
        else {
            panic!("row is not container")
        };
        let UiNode::Image { source, .. } = &row_children[0] else {
            panic!("first row child is not image")
        };
        source
    }

    #[test]
    fn expanded_header_shows_chevron_down_and_no_badge() {
        let ui = lower_one(&header(json!({
            "label": "Style",
            "collapsed": false,
            "section-id": "style"
        })));
        assert!(first_child_image_source(&ui).contains("chevron-down"));
        // 2 row children: chevron + label, no badge.
        if let UiNode::Container { children, .. } = &ui {
            if let UiNode::Container {
                children: row_kids, ..
            } = &children[0]
            {
                assert_eq!(row_kids.len(), 2);
            }
        }
    }

    #[test]
    fn collapsed_header_shows_chevron_left_and_default_badge() {
        let ui = lower_one(&header(json!({
            "label": "Style",
            "collapsed": true,
            "section-id": "style"
        })));
        assert!(first_child_image_source(&ui).contains("chevron-left"));
        if let UiNode::Container {
            children, props, ..
        } = &ui
        {
            if let UiNode::Container {
                children: row_kids, ..
            } = &children[0]
            {
                assert_eq!(row_kids.len(), 3); // chevron + label + badge
                if let UiNode::Text { content, .. } = &row_kids[2] {
                    assert_eq!(content, "(default)");
                }
            }
            assert!(props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "data-collapsed" && v == "true"));
        }
    }

    #[test]
    fn ssr_semantic_is_header_with_section_id() {
        let ui = lower_one(&header(json!({
            "label": "Style", "section-id": "style"
        })));
        if let UiNode::Container { props, .. } = ui {
            assert_eq!(props.semantic.tag.as_deref(), Some("header"));
            assert!(props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "data-section" && v == "style"));
        }
    }

    #[test]
    fn schema_declares_three_fields() {
        let block = prism_builder::SpecBlock::new(&super::SECTION_HEADER_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(keys, vec!["label", "collapsed", "section-id"]);
    }

    #[test]
    fn signals_include_section_toggled() {
        let block = prism_builder::SpecBlock::new(&super::SECTION_HEADER_SPEC);
        let names: Vec<String> = block.signals().into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"section-toggled".into()));
    }
}
