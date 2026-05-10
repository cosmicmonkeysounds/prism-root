//! `shell.dock-workspace` — recursive renderer for the active
//! [`prism_dock::DockState`] tree. Reads a `dock` JSON prop
//! (serialised [`prism_dock::DockNode`]) and emits nested containers:
//!
//! - `Split { axis, ratio, first, second }` → a `<container>` with
//!   `direction = row | column` whose two children are sized by the
//!   ratio (`first` gets `ratio`, `second` gets `1 - ratio`).
//! - `TabGroup { tabs, active }` → a `<shell.dock-panel>` with
//!   `panel-id` set to the active tab id; the dock-panel routes
//!   to the matching content tag via [`prism_dock::PanelKind::tag`].
//!
//! Smart pattern: pure recursion over a serialisable tree, no
//! per-panel knowledge, no router-arm match. Adding a new dockable
//! panel is one row in [`prism_dock::PanelKind::ALL`];
//! adding a new tree shape (a third dock variant) is one match arm
//! here. The "what visual lives in this leaf?" question is
//! answered exactly once, in the routing table.
//!
//! Replaces the legacy `push_dock_layout` walker that flattened the
//! tree into pixel rectangles for Slint to absolutely-position. The
//! new approach delegates layout to the runtime — `Sizing::Percent`
//! on the split children, `Sizing::Grow` on the leaves — so the
//! workspace re-flows on viewport resize without any host-side
//! geometry recomputation.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, LowerCtx},
};
use prism_dock::{Axis, DockNode};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};
use serde_json::{json, Value};

fn dock_workspace_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("dock", "Active DockNode (JSON)")]
}

fn dock_workspace_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let dock = node.props.get("dock").and_then(decode_dock_node);

    let body = match dock {
        Some(tree) => render_dock(ctx, &tree, &node.id),
        // Empty workspace — render a coherent (data-empty) frame
        // so the chrome never collapses to a zero-rect during a
        // partial migration / loading state.
        None => bare_container(format!("{}::empty", node.id), Vec::new(), |p| {
            p.width = Sizing::Grow;
            p.height = Sizing::Grow;
        }),
    };

    bare_container(node.id.clone(), vec![body], |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("div").with_attr("data-role", "dock-workspace");
    })
}

pub const DOCK_WORKSPACE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.dock-workspace", dock_workspace_schema)
        .lower(dock_workspace_lower);

/// Accept either a real `Object`-shaped JSON `Value` (the binding
/// path, post-`value_for` JSON-parse) or a `String` that still
/// carries the serialised form (defence-in-depth — older binding
/// codepaths or hand-authored attrs may pre-date the resolver fix).
fn decode_dock_node(v: &Value) -> Option<DockNode> {
    match v {
        Value::Object(_) => serde_json::from_value(v.clone()).ok(),
        Value::String(s) => serde_json::from_str(s).ok(),
        _ => None,
    }
}

fn render_dock(ctx: &LowerCtx<'_>, tree: &DockNode, base_id: &str) -> UiNode {
    match tree {
        DockNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let dir = match axis {
                Axis::Horizontal => Direction::Row,
                Axis::Vertical => Direction::Column,
            };
            let r = ratio.clamp(0.05, 0.95);
            let first_sz = sizing_for(*axis, r);
            let second_sz = sizing_for(*axis, 1.0 - r);
            let mut a = render_dock(ctx, first, &format!("{base_id}::a"));
            let mut b = render_dock(ctx, second, &format!("{base_id}::b"));
            apply_axis_sizing(&mut a, *axis, first_sz);
            apply_axis_sizing(&mut b, *axis, second_sz);
            bare_container(format!("{base_id}::split"), vec![a, b], |p| {
                p.direction = dir;
                p.width = Sizing::Grow;
                p.height = Sizing::Grow;
                p.semantic = Semantic::tag("div").with_attr("data-role", "dock-split");
            })
        }
        DockNode::TabGroup { tabs, active } => {
            let panel_id = tabs
                .get(*active)
                .cloned()
                .unwrap_or_else(|| tabs.first().cloned().unwrap_or_default());
            let derived_id = format!("{base_id}::{panel_id}");
            let mut props = json!({ "panel-id": panel_id });
            // Forward the tab list when there's more than one panel
            // in the leaf — the dock-panel renders a tab bar above
            // its body in that case (existing behaviour).
            if tabs.len() > 1 {
                let active_idx = *active;
                let entries: Vec<Value> = tabs
                    .iter()
                    .enumerate()
                    .map(|(i, t)| json!({ "tab-id": t, "label": t, "active": i == active_idx }))
                    .collect();
                props["tabs"] = Value::Array(entries);
            }
            ctx.lower_as("shell.dock-panel", derived_id.clone(), props)
                .unwrap_or_else(|| {
                    // Fallback for headless test contexts: render a
                    // labelled placeholder so the assertion paths can
                    // still inspect tree shape.
                    bare_container(derived_id, Vec::new(), |p| {
                        p.width = Sizing::Grow;
                        p.height = Sizing::Grow;
                        p.semantic = Semantic::tag("div")
                            .with_attr("data-role", "dock-leaf")
                            .with_attr("data-panel", panel_id.clone());
                    })
                })
        }
    }
}

fn sizing_for(axis: Axis, fraction: f32) -> Sizing {
    match axis {
        // Sizing::Percent expects a 0..1 fraction.
        Axis::Horizontal | Axis::Vertical => Sizing::Percent(fraction),
    }
}

fn apply_axis_sizing(node: &mut UiNode, axis: Axis, sz: Sizing) {
    if let UiNode::Container { props, .. } = node {
        match axis {
            Axis::Horizontal => props.width = sz,
            Axis::Vertical => props.height = sz,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use prism_dock::DockNode as PdNode;
    use serde_json::json;

    fn lower(props: Value, with_reg: bool) -> UiNode {
        let n = BuilderNode {
            id: "ws".into(),
            component: "shell.dock-workspace".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let owned;
        let ctx = if with_reg {
            let mut r = ShellComponentRegistry::new();
            register_shell_builtins(&mut r).expect("register");
            owned = r;
            LowerCtx::new(Some(owned.as_component_registry()), &cascade)
        } else {
            LowerCtx::new(None, &cascade)
        };
        dock_workspace_lower(&ctx, &n, &cascade)
    }

    #[test]
    fn empty_workspace_renders_coherent_frame() {
        let ui = lower(json!({}), false);
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "dock-workspace"));
    }

    #[test]
    fn single_leaf_dispatches_to_dock_panel_with_panel_id() {
        let dock = PdNode::TabGroup {
            tabs: vec!["builder".into()],
            active: 0,
        };
        let ui = lower(
            json!({ "dock": serde_json::to_value(&dock).unwrap() }),
            true,
        );
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // One leaf → one dock-panel
        let UiNode::Container { props, .. } = &children[0] else {
            panic!("leaf must be a container")
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-panel" && v == "builder"));
    }

    #[test]
    fn split_emits_two_children_with_correct_direction() {
        let dock = PdNode::Split {
            axis: Axis::Horizontal,
            ratio: 0.3,
            first: Box::new(PdNode::TabGroup {
                tabs: vec!["inspector".into()],
                active: 0,
            }),
            second: Box::new(PdNode::TabGroup {
                tabs: vec!["builder".into()],
                active: 0,
            }),
        };
        let ui = lower(
            json!({ "dock": serde_json::to_value(&dock).unwrap() }),
            true,
        );
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            props: split_props,
            children: split_kids,
            ..
        } = &children[0]
        else {
            panic!("expected split container")
        };
        assert_eq!(split_props.direction, Direction::Row);
        assert_eq!(split_kids.len(), 2);

        // First child sized to 30% width; second sized to 70%.
        let UiNode::Container {
            props: first_props, ..
        } = &split_kids[0]
        else {
            panic!()
        };
        match first_props.width {
            Sizing::Percent(p) => assert!((p - 0.3).abs() < 1e-3),
            other => panic!("expected percent, got {other:?}"),
        }
    }

    #[test]
    fn vertical_split_uses_column_direction_and_height_sizing() {
        let dock = PdNode::Split {
            axis: Axis::Vertical,
            ratio: 0.5,
            first: Box::new(PdNode::TabGroup {
                tabs: vec!["builder".into()],
                active: 0,
            }),
            second: Box::new(PdNode::TabGroup {
                tabs: vec!["code-editor".into()],
                active: 0,
            }),
        };
        let ui = lower(
            json!({ "dock": serde_json::to_value(&dock).unwrap() }),
            true,
        );
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            props: split_props,
            children: split_kids,
            ..
        } = &children[0]
        else {
            panic!()
        };
        assert_eq!(split_props.direction, Direction::Column);
        let UiNode::Container { props, .. } = &split_kids[0] else {
            panic!()
        };
        match props.height {
            Sizing::Percent(p) => assert!((p - 0.5).abs() < 1e-3),
            other => panic!("expected percent, got {other:?}"),
        }
    }

    #[test]
    fn multi_tab_leaf_forwards_tabs_array() {
        let dock = PdNode::TabGroup {
            tabs: vec!["builder".into(), "code-editor".into()],
            active: 1,
        };
        let ui = lower(
            json!({ "dock": serde_json::to_value(&dock).unwrap() }),
            true,
        );
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // dock-panel with a tab bar → its container has 2 sections
        // (tab-bar + body).
        let UiNode::Container {
            children: panel_kids,
            ..
        } = &children[0]
        else {
            panic!()
        };
        assert_eq!(panel_kids.len(), 2, "tab-bar + body");
    }

    #[test]
    fn string_serialised_dock_decodes() {
        // Defence-in-depth: an attribute that arrived as a raw string
        // (legacy authoring path) still parses.
        let dock = PdNode::TabGroup {
            tabs: vec!["builder".into()],
            active: 0,
        };
        let serialised = serde_json::to_string(&dock).unwrap();
        let ui = lower(json!({ "dock": serialised }), true);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert!(!children.is_empty());
    }
}
