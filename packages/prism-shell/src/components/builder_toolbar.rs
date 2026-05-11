//! `shell.builder-toolbar` — the 32px strip above the builder canvas
//! holding the alignment buttons, the Desktop/Tablet/Mobile device
//! cluster, the zoom indicator, and the node-count badge. §43 D2.
//!
//! Slint origin: the `BuilderCanvasToolbar` row above the canvas page
//! in `ui/app.slint`. The Slint version mixed dropdown chrome with
//! click-handlers; this port keeps the visual recipe and lets the
//! service layer own behaviour through declarative `data-role` /
//! `data-target` attrs (which the §43 follow-up hit-test surface
//! turns into actual mutations).
//!
//! Smart pattern: the row decomposes into four declarative clusters
//! built through the existing chrome helpers (`icon_button_node`
//! mostly), separated by `toolbar-separator` instances. Adding a
//! cluster is one cluster builder + one row in the assembly. No new
//! visual primitives, no new hover machinery.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, parse_color, prop_str, uniform_radius, LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use super::chrome::icon_button_node;

const TOOLBAR_HEIGHT: f32 = 32.0;
const TOOLBAR_BG: &str = "#f4000000";
const TOOLBAR_PAD_X: f32 = 8.0;
const TOOLBAR_GAP: f32 = 6.0;
const SEPARATOR_BG: &str = "#33000000";
const SEPARATOR_WIDTH: f32 = 1.0;
const SEPARATOR_HEIGHT: f32 = 18.0;
const PILL_BG: &str = "#0c000000";
const PILL_BG_ACTIVE: &str = "#260060c0";
const PILL_HEIGHT: f32 = 24.0;
const PILL_RADIUS: f32 = 3.0;
const PILL_PAD_X: f32 = 10.0;
const TEXT_COLOR: &str = "#000000";
const TEXT_COLOR_MUTED: &str = "#99000000";
const TEXT_SIZE: f32 = 11.0;

/// One alignment button in the leading cluster — `align-left`,
/// `align-center`, `align-right`. Per-row metadata is stored declaratively
/// in [`ALIGN_ENTRIES`] so adding a fourth button is one row.
struct AlignEntry {
    id: &'static str,
    icon: &'static str,
    tooltip: &'static str,
}

const ALIGN_ENTRIES: &[AlignEntry] = &[
    AlignEntry {
        id: "align-left",
        icon: "icons/align-left.svg",
        tooltip: "Align left",
    },
    AlignEntry {
        id: "align-center",
        icon: "icons/align-center.svg",
        tooltip: "Align center",
    },
    AlignEntry {
        id: "align-right",
        icon: "icons/align-right.svg",
        tooltip: "Align right",
    },
];

/// One device-mode pill in the centre cluster. The pill paints with
/// `PILL_BG_ACTIVE` when its id matches the toolbar's `device` prop.
struct DeviceEntry {
    id: &'static str,
    label: &'static str,
    tooltip: &'static str,
}

const DEVICE_ENTRIES: &[DeviceEntry] = &[
    DeviceEntry {
        id: "desktop",
        label: "Desktop",
        tooltip: "Desktop preview",
    },
    DeviceEntry {
        id: "tablet",
        label: "Tablet",
        tooltip: "Tablet preview",
    },
    DeviceEntry {
        id: "mobile",
        label: "Mobile",
        tooltip: "Mobile preview",
    },
];

fn builder_toolbar_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("device", "Active device preview (desktop|tablet|mobile)"),
        FieldSpec::number(
            "zoom",
            "Canvas zoom",
            prism_builder::registry::NumericBounds::min_max(0.1, 8.0),
        )
        .with_default(Value::from(1.0)),
        FieldSpec::number(
            "node-count",
            "Total node count in the active document",
            prism_builder::registry::NumericBounds::min(0.0),
        )
        .with_default(Value::from(0.0)),
        FieldSpec::text("tool", "Active tool (move|rotate|scale)"),
    ]
}

fn builder_toolbar_signals() -> Vec<SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "align-clicked",
            "User clicked one of the alignment buttons — payload (alignment).",
        ),
        SignalDef::new(
            "device-changed",
            "User picked a device mode — payload (device).",
        ),
        SignalDef::new("zoom-in", "User clicked the + zoom button."),
        SignalDef::new("zoom-out", "User clicked the - zoom button."),
        SignalDef::new("zoom-reset", "User clicked the 100% zoom label to reset."),
    ])
}

fn builder_toolbar_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let device = prop_str(node, "device");
    let zoom = node
        .props
        .get("zoom")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;
    let node_count = node
        .props
        .get("node-count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let mut clusters: Vec<UiNode> = Vec::with_capacity(8);
    clusters.push(build_align_cluster(node));
    clusters.push(separator(format!("{}::sep-1", node.id)));
    clusters.push(build_device_cluster(node, device));
    clusters.push(separator(format!("{}::sep-2", node.id)));
    clusters.push(build_zoom_cluster(node, zoom));
    clusters.push(separator(format!("{}::sep-3", node.id)));
    clusters.push(build_node_count_badge(node, node_count));

    bare_container(node.id.clone(), clusters, |p| {
        p.direction = Direction::Row;
        p.gap = TOOLBAR_GAP;
        p.width = Sizing::Grow;
        p.height = Sizing::Fixed(TOOLBAR_HEIGHT);
        p.padding = Padding {
            left: TOOLBAR_PAD_X,
            right: TOOLBAR_PAD_X,
            top: 0.0,
            bottom: 0.0,
        };
        p.background = parse_color(TOOLBAR_BG);
        p.semantic = Semantic::tag("nav")
            .with_attr("role", "toolbar")
            .with_attr("aria-label", "Builder canvas toolbar")
            .with_attr("data-role", "builder-toolbar");
    })
}

pub const BUILDER_TOOLBAR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.builder-toolbar", builder_toolbar_schema)
        .lower(builder_toolbar_lower)
        .signals(builder_toolbar_signals);

fn build_align_cluster(node: &Node) -> UiNode {
    let buttons: Vec<UiNode> = ALIGN_ENTRIES
        .iter()
        .map(|entry| {
            icon_button_node(
                format!("{}::align::{}", node.id, entry.id),
                entry.icon,
                true,
                Some(entry.tooltip),
            )
        })
        .collect();
    bare_container(format!("{}::align", node.id), buttons, |p| {
        p.direction = Direction::Row;
        p.gap = 2.0;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("div")
            .with_attr("role", "group")
            .with_attr("aria-label", "Alignment")
            .with_attr("data-role", "toolbar-align-cluster");
    })
}

fn build_device_cluster(node: &Node, active_device: &str) -> UiNode {
    let pills: Vec<UiNode> = DEVICE_ENTRIES
        .iter()
        .map(|entry| device_pill(node, entry, entry.id == active_device))
        .collect();
    bare_container(format!("{}::device", node.id), pills, |p| {
        p.direction = Direction::Row;
        p.gap = 4.0;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("div")
            .with_attr("role", "radiogroup")
            .with_attr("aria-label", "Device preview")
            .with_attr("data-role", "toolbar-device-cluster");
    })
}

fn device_pill(node: &Node, entry: &DeviceEntry, active: bool) -> UiNode {
    let bg = if active { PILL_BG_ACTIVE } else { PILL_BG };
    let label_color = if active { TEXT_COLOR } else { TEXT_COLOR_MUTED };
    let cascade = StyleProperties::default();
    let label = colored_text_node(
        format!("{}::device::{}::label", node.id, entry.id),
        entry.label.into(),
        &cascade,
        TEXT_SIZE,
        label_color,
    );
    bare_container(
        format!("{}::device::{}", node.id, entry.id),
        vec![label],
        |p| {
            p.direction = Direction::Row;
            p.height = Sizing::Fixed(PILL_HEIGHT);
            p.padding = Padding {
                left: PILL_PAD_X,
                right: PILL_PAD_X,
                top: 0.0,
                bottom: 0.0,
            };
            p.background = parse_color(bg);
            p.radius = uniform_radius(PILL_RADIUS);
            let mut s = Semantic::tag("button")
                .with_attr("type", "button")
                .with_attr("role", "radio")
                .with_attr("aria-label", entry.tooltip)
                .with_attr("data-role", "toolbar-device-pill")
                .with_attr("data-device", entry.id);
            if active {
                s = s.with_attr("aria-checked", "true");
            }
            p.semantic = s;
        },
    )
}

fn build_zoom_cluster(node: &Node, zoom: f32) -> UiNode {
    let minus = icon_button_node(
        format!("{}::zoom::out", node.id),
        "icons/minus.svg",
        true,
        Some("Zoom out"),
    );
    let plus = icon_button_node(
        format!("{}::zoom::in", node.id),
        "icons/plus.svg",
        true,
        Some("Zoom in"),
    );
    let percent = format!("{}%", (zoom * 100.0).round() as i32);
    let cascade = StyleProperties::default();
    let label = colored_text_node(
        format!("{}::zoom::label", node.id),
        percent,
        &cascade,
        TEXT_SIZE,
        TEXT_COLOR,
    );
    let label_pill = bare_container(format!("{}::zoom::pill", node.id), vec![label], |p| {
        p.direction = Direction::Row;
        p.height = Sizing::Fixed(PILL_HEIGHT);
        p.padding = Padding {
            left: PILL_PAD_X,
            right: PILL_PAD_X,
            top: 0.0,
            bottom: 0.0,
        };
        p.background = parse_color(PILL_BG);
        p.radius = uniform_radius(PILL_RADIUS);
        p.semantic = Semantic::tag("button")
            .with_attr("type", "button")
            .with_attr("aria-label", "Reset zoom to 100%")
            .with_attr("data-role", "toolbar-zoom-reset");
    });
    bare_container(
        format!("{}::zoom", node.id),
        vec![minus, label_pill, plus],
        |p| {
            p.direction = Direction::Row;
            p.gap = 4.0;
            p.height = Sizing::Grow;
            p.semantic = Semantic::tag("div")
                .with_attr("role", "group")
                .with_attr("aria-label", "Zoom")
                .with_attr("data-role", "toolbar-zoom-cluster");
        },
    )
}

fn build_node_count_badge(node: &Node, count: usize) -> UiNode {
    let count_word = if count == 1 { "node" } else { "nodes" };
    let label_text = format!("{count} {count_word}");
    let cascade = StyleProperties::default();
    let label = colored_text_node(
        format!("{}::count::label", node.id),
        label_text,
        &cascade,
        TEXT_SIZE,
        TEXT_COLOR_MUTED,
    );
    bare_container(format!("{}::count", node.id), vec![label], |p| {
        p.direction = Direction::Row;
        p.height = Sizing::Fixed(PILL_HEIGHT);
        p.padding = Padding {
            left: PILL_PAD_X,
            right: PILL_PAD_X,
            top: 0.0,
            bottom: 0.0,
        };
        p.background = parse_color(PILL_BG);
        p.radius = uniform_radius(PILL_RADIUS);
        p.semantic = Semantic::tag("div")
            .with_attr("role", "status")
            .with_attr("aria-label", "Node count")
            .with_attr("data-role", "toolbar-node-count");
    })
}

fn separator(id: String) -> UiNode {
    bare_container(id, vec![], |p| {
        p.width = Sizing::Fixed(SEPARATOR_WIDTH);
        p.height = Sizing::Fixed(SEPARATOR_HEIGHT);
        p.background = parse_color(SEPARATOR_BG);
        p.semantic = Semantic::tag("span")
            .with_attr("role", "separator")
            .with_attr("aria-orientation", "vertical");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::Block;
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        lower_with(
            &test_node("tb", "shell.builder-toolbar", props),
            builder_toolbar_lower,
        )
    }

    #[test]
    fn toolbar_lowers_with_seven_clusters() {
        let ui = lower(json!({}));
        let UiNode::Container {
            props, children, ..
        } = ui
        else {
            panic!()
        };
        assert_eq!(props.height, Sizing::Fixed(TOOLBAR_HEIGHT));
        // align + sep + device + sep + zoom + sep + count
        assert_eq!(children.len(), 7);
    }

    #[test]
    fn align_cluster_emits_three_icon_buttons() {
        let ui = lower(json!({}));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: align_kids,
            props: align_props,
            ..
        } = &children[0]
        else {
            panic!()
        };
        assert_eq!(align_kids.len(), 3);
        assert!(align_props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "toolbar-align-cluster"));
    }

    #[test]
    fn device_cluster_marks_active_pill_with_aria_checked() {
        let ui = lower(json!({ "device": "tablet" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // [align, sep, device, ...] — device cluster is index 2.
        let UiNode::Container {
            children: device_kids,
            ..
        } = &children[2]
        else {
            panic!()
        };
        assert_eq!(device_kids.len(), 3);
        // Tablet is the second device.
        let UiNode::Container {
            props: tablet_props,
            ..
        } = &device_kids[1]
        else {
            panic!()
        };
        assert!(tablet_props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-checked" && v == "true"));
        assert!(tablet_props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-device" && v == "tablet"));
        let UiNode::Container {
            props: desktop_props,
            ..
        } = &device_kids[0]
        else {
            panic!()
        };
        assert!(!desktop_props
            .semantic
            .attrs
            .iter()
            .any(|(k, _)| k == "aria-checked"));
    }

    #[test]
    fn zoom_cluster_shows_percent_label() {
        let ui = lower(json!({ "zoom": 1.5 }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: zoom_kids,
            ..
        } = &children[4]
        else {
            panic!()
        };
        // [minus, pill, plus]
        assert_eq!(zoom_kids.len(), 3);
        let UiNode::Container {
            children: pill_kids,
            ..
        } = &zoom_kids[1]
        else {
            panic!()
        };
        let UiNode::Text { content, .. } = &pill_kids[0] else {
            panic!()
        };
        assert_eq!(content, "150%");
    }

    #[test]
    fn node_count_badge_pluralises() {
        let ui = lower(json!({ "node-count": 3 }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: count_kids,
            ..
        } = &children[6]
        else {
            panic!()
        };
        let UiNode::Text { content, .. } = &count_kids[0] else {
            panic!()
        };
        assert_eq!(content, "3 nodes");

        let ui_one = lower(json!({ "node-count": 1 }));
        let UiNode::Container {
            children: kids_one, ..
        } = ui_one
        else {
            panic!()
        };
        let UiNode::Container {
            children: cnt_one_kids,
            ..
        } = &kids_one[6]
        else {
            panic!()
        };
        let UiNode::Text { content: c_one, .. } = &cnt_one_kids[0] else {
            panic!()
        };
        assert_eq!(c_one, "1 node");
    }

    #[test]
    fn schema_declares_four_fields() {
        let block = prism_builder::SpecBlock::new(&super::BUILDER_TOOLBAR_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(keys, vec!["device", "zoom", "node-count", "tool"]);
    }
}
