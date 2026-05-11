//! `shell.inspector-row` — 30px-tall row used by the inspector / outline
//! panel. One row per node-tree entry, with a `kind`-driven palette
//! (`node` / `row` / `empty`) controlling background tone, dot shape,
//! and label weight, plus optional move-up / move-down chevrons when
//! `selected` and an optional trash button when `show-delete` is set.
//!
//! Slint origin: `InspectorRow` in `ui/app.slint` (lines 1195-1267).
//!
//! Lowering smart pattern: a single `KindMetrics` lookup table
//! (`KIND_NODE` / `KIND_ROW` / `KIND_EMPTY`) holds every variation
//! across the three kinds — the lowering body never branches on `kind`
//! beyond picking the metrics row. Adding a new kind is one struct
//! literal; adding a new field on every kind is one field on the
//! struct. The chevron / trash buttons compose through
//! [`super::chrome::icon_button_node`], the same shared visual
//! recipe `IconButton` itself uses — embedded buttons inherit the
//! 28×28 / 16×16 / 6px-radius / hover-bg shape automatically.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, prop_bool, prop_str, prop_string,
        uniform_radius, LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use super::chrome::{icon_button_node, indent_dot};

const ROW_HEIGHT: f32 = 30.0;
const ROW_RADIUS: f32 = 4.0;
const INDENT_PX: f32 = 16.0;
const PAD_LEFT_BASE: f32 = 12.0;
const PAD_RIGHT: f32 = 8.0;
const ROW_GAP: f32 = 6.0;
const HOVER_BG: &str = "#0a000000";

/// Per-`kind` declarative table — every visual difference between
/// `node` / `row` / `empty` lives here; the lowering body never
/// branches on `kind` directly. Adding a fourth kind is one entry
/// in [`metrics_for_kind`].
struct KindMetrics {
    /// Resting background (`None` → transparent).
    bg: Option<&'static str>,
    /// Selected background — `node`-shaped rows tint when selected;
    /// `row` / `empty` ignore this field (they already paint a tone).
    selected_bg: Option<&'static str>,
    /// Indent-dot color when *not* selected.
    dot_color: &'static str,
    /// Indent-dot color when selected (only `node` honours this).
    dot_color_selected: &'static str,
    /// Indent-dot border radius (1px → square-ish, 3px → round).
    dot_radius_px: f32,
    /// Label font size.
    label_size: f32,
    /// Label color when not selected.
    label_color: &'static str,
    /// Label color when selected.
    label_color_selected: &'static str,
    /// Whether to render the secondary `node-id` text (monospaced).
    show_id_text: bool,
    /// Whether `selected` toggles move-up / move-down chevrons.
    show_move_buttons: bool,
    /// Whether `show-delete` toggles the trash button (e.g. on `row`
    /// kind hovered, the host flips this prop on enter / leave).
    allow_delete_button: bool,
    /// SSR `role` for accessibility ("treeitem" for nodes, "group"
    /// for row separators, "none" for empty placeholders).
    aria_role: &'static str,
}

const KIND_NODE: KindMetrics = KindMetrics {
    bg: None,
    selected_bg: Some("#26000000"),
    dot_color: "#99000000",
    dot_color_selected: "#000000ff",
    dot_radius_px: 3.0,
    label_size: 12.0,
    label_color: "#000000",
    label_color_selected: "#000000",
    show_id_text: true,
    show_move_buttons: true,
    allow_delete_button: false,
    aria_role: "treeitem",
};

const KIND_ROW: KindMetrics = KindMetrics {
    bg: Some("#0a000000"),
    selected_bg: None,
    dot_color: "#99000000",
    dot_color_selected: "#99000000",
    dot_radius_px: 1.0,
    label_size: 11.0,
    label_color: "#cc000000",
    label_color_selected: "#cc000000",
    show_id_text: false,
    show_move_buttons: false,
    allow_delete_button: true,
    aria_role: "group",
};

const KIND_EMPTY: KindMetrics = KindMetrics {
    bg: None,
    selected_bg: None,
    dot_color: "#33000000",
    dot_color_selected: "#33000000",
    dot_radius_px: 3.0,
    label_size: 12.0,
    label_color: "#99000000",
    label_color_selected: "#99000000",
    show_id_text: false,
    show_move_buttons: false,
    allow_delete_button: false,
    aria_role: "none",
};

fn metrics_for_kind(kind: &str) -> &'static KindMetrics {
    match kind {
        "row" => &KIND_ROW,
        "empty" => &KIND_EMPTY,
        // Default: anything else is treated as `node`.
        _ => &KIND_NODE,
    }
}

fn inspector_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("node-id", "Node ID"),
        FieldSpec::text("component-id", "Component ID"),
        FieldSpec::text("kind", "Kind").with_default(Value::from("node")),
        FieldSpec::number(
            "depth",
            "Depth",
            prism_builder::registry::NumericBounds::min(0.0),
        )
        .with_default(Value::from(0.0)),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::boolean("show-delete", "Show delete (host-driven hover)")
            .with_default(Value::Bool(false)),
    ]
}

fn inspector_row_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "row-clicked",
            "Row was clicked — host selects the bound node-id.",
        ),
        SignalDef::new(
            "row-right-clicked",
            "Row was right-clicked — host opens a context menu at the (x, y).",
        ),
        SignalDef::new(
            "move-up",
            "Move-up chevron clicked (selected node rows only).",
        ),
        SignalDef::new(
            "move-down",
            "Move-down chevron clicked (selected node rows only).",
        ),
        SignalDef::new(
            "delete-track",
            "Trash clicked (row-kind rows with `show-delete=true`).",
        ),
    ])
}

fn inspector_row_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let kind = prop_str(node, "kind");
    let m = metrics_for_kind(kind);
    let selected = prop_bool(node, "selected", false);
    let show_delete = prop_bool(node, "show-delete", false);
    let depth = node
        .props
        .get("depth")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .max(0.0) as f32;
    let component_label = prop_str(node, "component-id");
    let node_id_text = prop_string(node, "node-id");

    // Style scaffold for child text — the kind table picks color
    // and size, `colored_text_node` clones a fresh StyleProperties
    // with the override colour applied, no shared mutable state.
    let style = StyleProperties::default();

    // Left cluster: indent dot + label (+ optional id text).
    let dot_color = if selected {
        m.dot_color_selected
    } else {
        m.dot_color
    };
    let label_color = if selected {
        m.label_color_selected
    } else {
        m.label_color
    };

    let mut left_children: Vec<UiNode> = Vec::with_capacity(if m.show_id_text { 3 } else { 2 });
    left_children.push(indent_dot(
        format!("{}::dot", node.id),
        dot_color,
        m.dot_radius_px,
    ));
    left_children.push(colored_text_node(
        format!("{}::label", node.id),
        component_label.into(),
        &style,
        m.label_size,
        label_color,
    ));
    if m.show_id_text && !node_id_text.is_empty() {
        left_children.push(colored_text_node(
            format!("{}::id", node.id),
            node_id_text.clone(),
            &style,
            10.0,
            "#80000000",
        ));
    }

    let left = bare_container(format!("{}::left", node.id), left_children, |p| {
        p.direction = Direction::Row;
        p.gap = ROW_GAP;
        p.height = Sizing::Grow;
    });

    // Right cluster: chevrons (selected node) or trash (row +
    // show-delete). Built via the shared `icon_button_node` recipe
    // — same 28×28 / 16×16 / 6px-radius shape as IconButton itself.
    let right = build_right_cluster(node, m, selected, show_delete);

    let mut row_children: Vec<UiNode> = Vec::with_capacity(2);
    row_children.push(left);
    if let Some(right_cluster) = right {
        row_children.push(right_cluster);
    }

    // Outer 30px row with kind- and selection-driven background.
    let resting_bg = if selected {
        m.selected_bg.or(m.bg)
    } else {
        m.bg
    };

    bare_container(node.id.clone(), row_children, |props| {
        props.height = Sizing::Fixed(ROW_HEIGHT);
        props.radius = uniform_radius(ROW_RADIUS);
        props.background = resting_bg.and_then(parse_color);
        // Hover swap is the cheapest visual feedback; rows declare
        // it unconditionally (disabled rows are not a thing here).
        props.hover = hover_bg(HOVER_BG);
        props.padding = Padding {
            left: PAD_LEFT_BASE + depth * INDENT_PX,
            right: PAD_RIGHT,
            top: 0.0,
            bottom: 0.0,
        };
        props.direction = Direction::Row;
        props.gap = 0.0;

        // SSR semantic — outline-tree row is a `<div role="…">`
        // with `aria-selected` driven by the prop. Flat structure
        // is fine for a flat tree; deep tree rendering would carry
        // `aria-level={depth + 1}` but we skip that until needed.
        //
        // §43 C3 routing keys — `data-role="inspector-row"` plus the
        // doc-node id under `data-target-id` so the hit-test surface
        // can route clicks back to `SelectionService` without the
        // shell having to walk the rendered tree itself.
        let mut s = Semantic::tag("div")
            .with_attr("role", m.aria_role)
            .with_attr("data-role", "inspector-row");
        if !node_id_text.is_empty() {
            s = s.with_attr("data-target-id", node_id_text.clone());
        }
        if selected && m.aria_role == "treeitem" {
            s = s.with_attr("aria-selected", "true");
        }
        if depth > 0.0 {
            s = s.with_attr("aria-level", ((depth as i64) + 1).to_string());
        }
        props.semantic = s;
    })
}

pub const INSPECTOR_ROW_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.inspector-row", inspector_row_schema)
        .lower(inspector_row_lower)
        .signals(inspector_row_signals);

fn build_right_cluster(
    node: &Node,
    m: &KindMetrics,
    selected: bool,
    show_delete: bool,
) -> Option<UiNode> {
    let mut buttons: Vec<UiNode> = Vec::new();
    if selected && m.show_move_buttons {
        buttons.push(icon_button_node(
            format!("{}::move-up", node.id),
            "icons/chevron-up.svg",
            true,
            Some("Move up"),
        ));
        buttons.push(icon_button_node(
            format!("{}::move-down", node.id),
            "icons/chevron-down.svg",
            true,
            Some("Move down"),
        ));
    }
    if show_delete && m.allow_delete_button {
        buttons.push(icon_button_node(
            format!("{}::delete", node.id),
            "icons/trash.svg",
            true,
            Some("Delete"),
        ));
    }
    if buttons.is_empty() {
        return None;
    }
    Some(bare_container(
        format!("{}::right", node.id),
        buttons,
        |p| {
            p.direction = Direction::Row;
            p.gap = 2.0;
            p.height = Sizing::Grow;
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::Block;
    use serde_json::json;

    fn lower(node: &BuilderNode) -> UiNode {
        lower_with(node, inspector_row_lower)
    }

    fn row(props: Value) -> BuilderNode {
        test_node("ir", "shell.inspector-row", props)
    }

    fn assert_container(n: &UiNode) -> (&prism_ui_runtime::layout::ContainerProps, &Vec<UiNode>) {
        match n {
            UiNode::Container {
                props, children, ..
            } => (props, children),
            other => panic!("expected container, got {other:?}"),
        }
    }

    #[test]
    fn unselected_node_row_has_left_cluster_only() {
        let ui = lower(&row(
            json!({ "kind": "node", "component-id": "Heading", "node-id": "n42" }),
        ));
        let (props, children) = assert_container(&ui);
        assert_eq!(props.height, Sizing::Fixed(ROW_HEIGHT));
        assert_eq!(children.len(), 1, "no chevrons unless selected");
    }

    #[test]
    fn selected_node_row_grows_a_right_cluster_with_two_chevrons() {
        let ui = lower(&row(json!({
            "kind": "node",
            "component-id": "Heading",
            "node-id": "n42",
            "selected": true,
        })));
        let (_, children) = assert_container(&ui);
        assert_eq!(children.len(), 2, "left + right");
        let (right_props, right_kids) = assert_container(&children[1]);
        assert_eq!(right_props.gap, 2.0);
        assert_eq!(right_kids.len(), 2, "move-up + move-down");
        // Each is the shared icon-button shape.
        let (b1_props, _) = assert_container(&right_kids[0]);
        assert_eq!(b1_props.width, Sizing::Fixed(28.0));
        assert_eq!(b1_props.semantic.tag.as_deref(), Some("button"));
    }

    #[test]
    fn row_kind_paints_resting_bg_and_hides_chevrons_even_when_selected() {
        let ui = lower(&row(json!({
            "kind": "row",
            "component-id": "Section",
            "selected": true,
        })));
        let (props, children) = assert_container(&ui);
        assert!(props.background.is_some(), "row kind tints resting bg");
        // Selected on row-kind doesn't force chevrons to appear.
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn row_kind_with_show_delete_renders_trash_button() {
        let ui = lower(&row(json!({
            "kind": "row",
            "component-id": "Section",
            "show-delete": true,
        })));
        let (_, children) = assert_container(&ui);
        assert_eq!(children.len(), 2);
        let (_, right_kids) = assert_container(&children[1]);
        assert_eq!(right_kids.len(), 1, "single trash button");
    }

    #[test]
    fn empty_kind_uses_low_contrast_palette_and_omits_id_text() {
        let ui = lower(&row(json!({
            "kind": "empty",
            "component-id": "(no children)",
            "node-id": "n0",
        })));
        let (_, children) = assert_container(&ui);
        let (_, left_kids) = assert_container(&children[0]);
        // dot + label only — id text suppressed for empty-kind even
        // when a node-id is present.
        assert_eq!(left_kids.len(), 2);
    }

    #[test]
    fn padding_left_grows_with_depth_for_tree_indent() {
        let depth_zero = lower(&row(json!({ "kind": "node", "depth": 0 })));
        let depth_three = lower(&row(json!({ "kind": "node", "depth": 3 })));
        let (p0, _) = assert_container(&depth_zero);
        let (p3, _) = assert_container(&depth_three);
        assert_eq!(p0.padding.left, PAD_LEFT_BASE);
        assert_eq!(p3.padding.left, PAD_LEFT_BASE + 3.0 * INDENT_PX);
    }

    #[test]
    fn semantic_carries_role_and_aria_selected() {
        let ui = lower(&row(json!({
            "kind": "node",
            "selected": true,
            "depth": 2,
            "node-id": "demo-heading",
        })));
        let (props, _) = assert_container(&ui);
        let s = &props.semantic;
        assert_eq!(s.tag.as_deref(), Some("div"));
        assert!(s.attrs.iter().any(|(k, v)| k == "role" && v == "treeitem"));
        assert!(s
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-selected" && v == "true"));
        assert!(s.attrs.iter().any(|(k, v)| k == "aria-level" && v == "3"));
        // §43 C3: routing keys for the hit-test surface.
        assert!(s
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "inspector-row"));
        assert!(s
            .attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "demo-heading"));
    }

    #[test]
    fn row_kind_emits_role_group() {
        let ui = lower(&row(json!({ "kind": "row" })));
        let (props, _) = assert_container(&ui);
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "group"));
    }

    #[test]
    fn schema_declares_six_fields_and_signals_cover_row_actions() {
        let block = prism_builder::SpecBlock::new(&super::INSPECTOR_ROW_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(
            keys,
            vec![
                "node-id",
                "component-id",
                "kind",
                "depth",
                "selected",
                "show-delete"
            ]
        );
        let names: Vec<String> = block.signals().into_iter().map(|s| s.name).collect();
        for s in [
            "row-clicked",
            "row-right-clicked",
            "move-up",
            "move-down",
            "delete-track",
        ] {
            assert!(names.contains(&s.into()), "missing signal {s}");
        }
    }

    #[test]
    fn unknown_kind_falls_through_to_node_palette() {
        let ui = lower(&row(json!({ "kind": "weird-future-kind" })));
        let (props, _) = assert_container(&ui);
        // Same role as KIND_NODE.
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "treeitem"));
    }
}
