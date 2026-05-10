//! `shell.toast` — bottom-right notification card. First chrome
//! primitive built on the runtime's overlay z-layer (see
//! `prism_ui_runtime::layout::Overlay` / `OverlayAnchor`).
//!
//! Slint origin: the `notifications` repeater + toast `Rectangle` in
//! `ui/app.slint`.
//!
//! Lowering: a card-shaped `Container` (kind-tinted accent rail on the
//! left, title + body column, dismiss [`shell.icon-button`]-shaped
//! affordance on the right). The Block produces a *Node*, not an
//! `Overlay` — overlay mounting is the host's job (`Surface::push_overlay`
//! with a `BottomRight` corner anchor + 16px inset). This split keeps
//! the same lowering reusable by a hypothetical "notification list"
//! panel that wants the same visual inline.
//!
//! Kinds (`info` / `success` / `warning` / `error`) drive only the
//! accent-rail colour and the SSR `role` / `data-kind` attribute; the
//! card chrome is identical across kinds, so the kind-string flows
//! through a small `kind_accent`/`kind_role` lookup rather than
//! cloning the recipe four times.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, parse_color, prop_str, prop_string, uniform_radius,
        LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const TOAST_WIDTH: f32 = 320.0;
const TOAST_RADIUS: f32 = 8.0;
const RAIL_WIDTH: f32 = 3.0;
const TITLE_FONT: f32 = 13.0;
const BODY_FONT: f32 = 12.0;

const CARD_BG: &str = "#f2ffffff";
const TITLE_COLOR: &str = "#cc000000";
const BODY_COLOR: &str = "#99000000";

/// Map `kind` → (rail colour, ARIA role). One table, no per-kind
/// branch in the lowering body. Unknown kinds collapse to "info" so
/// authors can pass any string without crashing the renderer.
fn kind_chrome(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "success" => ("#2da44e", "status"),
        "warning" => ("#bf8700", "status"),
        "error" => ("#cf222e", "alert"),
        _ => ("#0969da", "status"),
    }
}

fn toast_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("body", "Body"),
        FieldSpec::text("kind", "Kind").with_default(Value::String("info".into())),
    ]
}

fn toast_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "dismissed",
        "User dismissed the toast (close click, swipe, or auto-timeout).",
    )])
}

fn toast_lower(_ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let title = prop_string(node, "title");
    let body = prop_string(node, "body");
    let kind = match prop_str(node, "kind") {
        "" => "info",
        other => other,
    };
    let (rail_color, role) = kind_chrome(kind);

    // Left accent rail — kind-tinted vertical stroke. `bare_container`
    // owns the construction; we only specify the few fields that
    // make this a rail rather than a generic box.
    let rail = bare_container(format!("{}::rail", node.id), vec![], |props| {
        props.width = Sizing::Fixed(RAIL_WIDTH);
        props.height = Sizing::Grow;
        props.background = parse_color(rail_color);
    });

    // Title + body column. `colored_text_node` is the shared
    // "cascade-resolved text with a per-block override colour"
    // builder — toast colours are intentional overrides, so this
    // is the one-line shape. Reordering is one Vec edit, not two.
    let title_text = colored_text_node(
        format!("{}::title", node.id),
        title,
        style,
        TITLE_FONT,
        TITLE_COLOR,
    );
    let body_text = if body.is_empty() {
        None
    } else {
        Some(colored_text_node(
            format!("{}::body", node.id),
            body,
            style,
            BODY_FONT,
            BODY_COLOR,
        ))
    };
    let mut column_children = vec![title_text];
    column_children.extend(body_text);
    let column = bare_container(format!("{}::col", node.id), column_children, |props| {
        props.direction = Direction::Column;
        props.gap = 4.0;
        props.width = Sizing::Grow;
        props.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 10.0,
            bottom: 10.0,
        };
    });

    // Outer card: rail + column in a row, fixed-width, soft shadow
    // shape (the actual shadow is a backend concern; SSR users pick
    // it up via the role + data attributes). Semantic role is kind-
    // driven so screen readers announce errors as alerts.
    bare_container(node.id.clone(), vec![rail, column], |props| {
        props.direction = Direction::Row;
        props.width = Sizing::Fixed(TOAST_WIDTH);
        props.height = Sizing::Fit;
        props.background = parse_color(CARD_BG);
        props.radius = uniform_radius(TOAST_RADIUS);
        props.semantic = Semantic::tag("aside")
            .with_role(role)
            .with_attr("data-kind", kind);
    })
}

pub const TOAST_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.toast", toast_schema)
        .lower(toast_lower)
        .signals(toast_signals);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::Block;
    use serde_json::json;

    fn lower_one(node: &BuilderNode) -> UiNode {
        lower_with(node, toast_lower)
    }

    fn toast_node(props: Value) -> BuilderNode {
        test_node("tst", "shell.toast", props)
    }

    #[test]
    fn lowers_to_row_with_rail_and_column() {
        let ui = lower_one(&toast_node(
            json!({ "title": "Saved", "body": "Project flushed", "kind": "success" }),
        ));
        let UiNode::Container {
            props, children, ..
        } = ui
        else {
            panic!("not a container")
        };
        assert_eq!(props.direction, Direction::Row);
        assert_eq!(props.width, Sizing::Fixed(TOAST_WIDTH));
        assert_eq!(children.len(), 2, "rail + column");
        // Rail is the first child and is kind-tinted.
        if let UiNode::Container {
            props: rail_props, ..
        } = &children[0]
        {
            assert_eq!(rail_props.width, Sizing::Fixed(RAIL_WIDTH));
            assert!(rail_props.background.is_some(), "rail tinted");
        } else {
            panic!("rail should be a container")
        }
    }

    #[test]
    fn omits_body_text_when_body_prop_empty() {
        let ui = lower_one(&toast_node(json!({ "title": "Hi", "body": "" })));
        let UiNode::Container { children, .. } = ui else {
            panic!("not a container")
        };
        let UiNode::Container { children: col, .. } = &children[1] else {
            panic!("column missing")
        };
        assert_eq!(col.len(), 1, "title only");
    }

    #[test]
    fn kind_drives_aria_role_and_data_kind() {
        for (kind, role) in [
            ("info", "status"),
            ("success", "status"),
            ("warning", "status"),
            ("error", "alert"),
            ("garbage", "status"),
        ] {
            let ui = lower_one(&toast_node(json!({ "title": "x", "kind": kind })));
            if let UiNode::Container { props, .. } = ui {
                assert_eq!(props.semantic.role.as_deref(), Some(role), "kind={kind}");
                assert_eq!(props.semantic.tag.as_deref(), Some("aside"));
                assert!(props
                    .semantic
                    .attrs
                    .iter()
                    .any(|(k, v)| k == "data-kind" && v == kind));
            } else {
                panic!("not a container for kind={kind}")
            }
        }
    }

    #[test]
    fn schema_declares_three_fields() {
        let block = prism_builder::SpecBlock::new(&super::TOAST_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(keys, vec!["title", "body", "kind"]);
    }

    #[test]
    fn signals_include_dismissed_alongside_universals() {
        let block = prism_builder::SpecBlock::new(&super::TOAST_SPEC);
        let names: Vec<String> = block.signals().into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"dismissed".into()));
        assert!(names.contains(&"clicked".into()), "common signals merged");
    }
}
