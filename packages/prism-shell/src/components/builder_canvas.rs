//! `shell.builder-canvas` — the WYSIWYG editing surface. The canvas
//! itself is a deterministic frame that exposes the page rect plus
//! retained-mode children for: rendered preview nodes (forwarded as
//! `host_children` from `.prism-ui` source), grid-cell overlays
//! (from a `grid-cells` JSON array), the selected node's gizmo
//! (dispatched to one of `shell.gizmo-{move,rotate,scale}` via
//! the `tool` prop), and an 8-row resize-handle ring (when
//! `selection-rect` is present).
//!
//! Slint origin: `BuilderCanvas` body around `ui/app.slint:1981`.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, prop_bool, prop_string, uniform_radius, LowerCtx},
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::{json, Value};

const CANVAS_BG: &str = "#040000";
const PAGE_BG: &str = "#ffffff";
const GRID_LINE: &str = "#22000000";
const SELECTION_BORDER: &str = "#0060c0";
const HANDLE_DIRECTIONS: [&str; 8] = ["tl", "t", "tr", "r", "br", "b", "bl", "l"];

fn builder_canvas_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::number(
            "page-width",
            "Page width (px)",
            prism_builder::registry::NumericBounds::min(1.0),
        )
        .with_default(Value::from(1280.0)),
        FieldSpec::number(
            "page-height",
            "Page height (px)",
            prism_builder::registry::NumericBounds::min(1.0),
        )
        .with_default(Value::from(800.0)),
        FieldSpec::number(
            "zoom",
            "Canvas zoom",
            prism_builder::registry::NumericBounds::min_max(0.1, 8.0),
        )
        .with_default(Value::from(1.0)),
        FieldSpec::text("tool", "Active tool (move|rotate|scale|none)"),
        FieldSpec::boolean("show-gizmo", "Show gizmo overlay").with_default(Value::Bool(false)),
        FieldSpec::text(
            "selection-rect",
            "Selection rect JSON {x, y, width, height} for handles",
        ),
        FieldSpec::text(
            "grid-cells",
            "Grid cells JSON array of {x, y, width, height, occupied}",
        ),
    ]
}

fn builder_canvas_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new("canvas-clicked", "Canvas background clicked."),
        SignalDef::new("cell-clicked", "Grid cell clicked."),
        SignalDef::new("selection-dragged", "Selection drag delta."),
    ])
}

fn builder_canvas_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let zoom = node
        .props
        .get("zoom")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
        .max(0.01) as f32;
    let page_w = node
        .props
        .get("page-width")
        .and_then(|v| v.as_f64())
        .unwrap_or(1280.0) as f32
        * zoom;
    let page_h = node
        .props
        .get("page-height")
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0) as f32
        * zoom;

    // §43 D2: toolbar strip prepended above the page rect. The
    // canvas binding forwards the toolbar's data (device, zoom,
    // node-count, tool) through this node's own props bag so the
    // single host binding row populates both. Falls through cleanly
    // when no registry is available — `lower_as` returns None and
    // the canvas paints without the toolbar (headless render paths).
    let toolbar_props = json!({
        "device": node
            .props
            .get("device")
            .and_then(|v| v.as_str())
            .unwrap_or("desktop"),
        "zoom": zoom,
        "node-count": node
            .props
            .get("node-count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        "tool": node
            .props
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("move"),
    });
    let toolbar = ctx.lower_as(
        "shell.builder-toolbar",
        format!("{}::toolbar", node.id),
        toolbar_props,
    );

    // Author-driven preview tree (the lowered nodes of the active
    // BuilderDocument's component tree). We accept either explicit
    // `host_children` (when `.prism-ui` author embeds them) or the
    // recursed children path.
    let mut preview = ctx
        .host_children()
        .map(|s| s.to_vec())
        .unwrap_or_else(|| ctx.lower_children(&node.children));

    // §43 B5: tag every container in the preview subtree with
    // `data-canvas-node="<id>"` so pointer-down routing can
    // distinguish a click on a canvas document node from a click on
    // a chrome container that happens to share an id (the most
    // notable collision is `root` — both `<shell.app-window
    // id="root">` and `BuilderDocument::page_shell()` use it).
    tag_canvas_subtree(&mut preview);

    let preview_layer = bare_container(format!("{}::preview", node.id), preview, |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("div")
            .with_attr("role", "presentation")
            .with_attr("data-role", "canvas-preview");
    });

    let grid_layer = build_grid_layer(node);
    let selection_layer = build_selection_layer(ctx, node);

    let page = bare_container(
        format!("{}::page", node.id),
        vec![preview_layer, grid_layer, selection_layer],
        |p| {
            p.width = Sizing::Fixed(page_w);
            p.height = Sizing::Fixed(page_h);
            p.background = parse_color(PAGE_BG);
            p.radius = uniform_radius(2.0);
            p.semantic = Semantic::tag("div")
                .with_attr("role", "img")
                .with_attr("aria-label", "Page surface")
                .with_attr("data-role", "canvas-page");
        },
    );

    let mut frame_children: Vec<UiNode> = Vec::with_capacity(2);
    if let Some(t) = toolbar {
        frame_children.push(t);
    }
    frame_children.push(page);

    bare_container(node.id.clone(), frame_children, |p| {
        p.direction = Direction::Column;
        p.gap = 12.0;
        p.padding = Padding {
            left: 24.0,
            right: 24.0,
            top: 24.0,
            bottom: 24.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.background = parse_color(CANVAS_BG);
        p.semantic = Semantic::tag("section")
            .with_attr("aria-label", "Builder canvas")
            .with_attr("data-role", "builder-canvas");
    })
}

pub const BUILDER_CANVAS_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.builder-canvas", builder_canvas_schema)
        .lower(builder_canvas_lower)
        .signals(builder_canvas_signals);

fn build_grid_layer(node: &Node) -> UiNode {
    let cells: Vec<UiNode> = node
        .props
        .get("grid-cells")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(idx, item)| build_grid_cell(node, idx, item))
                .collect()
        })
        .unwrap_or_default();

    bare_container(format!("{}::grid", node.id), cells, |p| {
        p.semantic = Semantic::tag("div")
            .with_attr("role", "grid")
            .with_attr("aria-label", "Page grid")
            .with_attr("data-role", "canvas-grid");
    })
}

fn build_grid_cell(node: &Node, idx: usize, item: &Value) -> UiNode {
    let x = item.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = item.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let w = item.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let h = item.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let occupied = item
        .get("occupied")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    bare_container(format!("{}::cell::{}", node.id, idx), vec![], |p| {
        p.width = Sizing::Fixed(w as f32);
        p.height = Sizing::Fixed(h as f32);
        p.background = parse_color(if occupied { "#0c0060c0" } else { "#04000000" });
        let mut s = Semantic::tag("div")
            .with_attr("role", "gridcell")
            .with_attr("data-role", "canvas-cell")
            .with_attr("data-x", format_coord(x))
            .with_attr("data-y", format_coord(y));
        if occupied {
            s = s.with_attr("data-occupied", "true");
        }
        p.semantic = s;
        p.radius = uniform_radius(1.0);
        // The gridline ghost is drawn by the renderer using
        // `data-stroke`; the layout block only emits the AABB.
        p.hover = prism_builder::ui_lower::hover_bg("#0a0060c0");
        let _ = GRID_LINE; // referenced in renderer; constant kept for parity with Slint
    })
}

fn build_selection_layer(ctx: &LowerCtx<'_>, node: &Node) -> UiNode {
    let selection_rect = node.props.get("selection-rect");
    let show_gizmo = prop_bool(node, "show-gizmo", false);
    let tool = prop_string(node, "tool");

    let mut layer_kids: Vec<UiNode> = Vec::new();

    if let Some(rect) = selection_rect {
        let x = rect.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let y = rect.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let w = rect.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let h = rect.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);

        // Selection outline (transparent fill, renderer paints stroke).
        layer_kids.push(bare_container(
            format!("{}::sel-outline", node.id),
            vec![],
            |p| {
                p.width = Sizing::Fixed(w as f32);
                p.height = Sizing::Fixed(h as f32);
                p.semantic = Semantic::tag("div")
                    .with_attr("role", "presentation")
                    .with_attr("data-role", "selection-outline")
                    .with_attr("data-x", format_coord(x))
                    .with_attr("data-y", format_coord(y))
                    .with_attr("data-stroke", SELECTION_BORDER);
            },
        ));

        // 8-row resize handle ring dispatched through the registry
        // when available; falls through to a flat handles container
        // when the resolver isn't wired up.
        for (idx, dir) in HANDLE_DIRECTIONS.iter().enumerate() {
            let handle_id = format!("{}::handle::{}", node.id, idx);
            let handle = ctx
                .lower_as(
                    "shell.resize-handle",
                    handle_id.clone(),
                    json!({ "direction": dir }),
                )
                .unwrap_or_else(|| {
                    bare_container(handle_id, vec![], |p| {
                        p.width = Sizing::Fixed(8.0);
                        p.height = Sizing::Fixed(8.0);
                        p.semantic = Semantic::tag("span")
                            .with_attr("role", "button")
                            .with_attr("data-role", "resize-handle")
                            .with_attr("data-direction", dir.to_string());
                    })
                });
            layer_kids.push(handle);
        }
    }

    if show_gizmo {
        let gizmo_tag = match tool.as_str() {
            "rotate" => "shell.gizmo-rotate",
            "scale" => "shell.gizmo-scale",
            _ => "shell.gizmo-move",
        };
        if let Some(g) = ctx.lower_as(gizmo_tag, format!("{}::gizmo", node.id), json!({})) {
            layer_kids.push(g);
        }
    }

    bare_container(format!("{}::overlay", node.id), layer_kids, |p| {
        p.semantic = Semantic::tag("div")
            .with_attr("role", "presentation")
            .with_attr("data-role", "canvas-overlay");
    })
}

/// Annotate every container in `nodes` (recursively) with
/// `data-canvas-node="<id>"`. Only Container variants contribute to
/// `Surface::hit_test_at` hits; tagging them is enough for the
/// pointer-routing path to recognise canvas-doc descendants without
/// false positives on chrome containers that share an id.
fn tag_canvas_subtree(nodes: &mut [UiNode]) {
    for n in nodes {
        if let UiNode::Container {
            id,
            props,
            children,
        } = n
        {
            if !id.is_empty() {
                props
                    .semantic
                    .attrs
                    .push(("data-canvas-node".into(), id.clone()));
            }
            tag_canvas_subtree(children);
        }
    }
}

fn format_coord(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use crate::components::testing::test_node;

    fn lower(props: Value) -> UiNode {
        let n = test_node("bc", "shell.builder-canvas", props);
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);
        builder_canvas_lower(&ctx, &n, &cascade)
    }

    /// §43 D2: the canvas frame now prepends a toolbar above the page
    /// when a registry is available. This helper pulls the page rect
    /// out of `[toolbar, page]` so the rest of the tests stay readable.
    fn page_of(children: &[UiNode]) -> &Vec<UiNode> {
        // [toolbar, page] in the registry-attached path; falls back to
        // [page] when the resolver couldn't dispatch the toolbar.
        let page_idx = children.len() - 1;
        let UiNode::Container { children: page, .. } = &children[page_idx] else {
            panic!("page slot not a container")
        };
        page
    }

    #[test]
    fn renders_page_with_three_layers() {
        let ui = lower(json!({}));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // preview + grid + overlay
        assert_eq!(page_of(&children).len(), 3);
    }

    #[test]
    fn page_size_scales_with_zoom() {
        let ui = lower(json!({
            "page-width": 800,
            "page-height": 600,
            "zoom": 2.0,
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let page_idx = children.len() - 1;
        let UiNode::Container { props, .. } = &children[page_idx] else {
            panic!()
        };
        assert_eq!(props.width, Sizing::Fixed(1600.0));
        assert_eq!(props.height, Sizing::Fixed(1200.0));
    }

    #[test]
    fn selection_rect_yields_outline_plus_eight_handles() {
        let ui = lower(json!({
            "selection-rect": { "x": 10, "y": 20, "width": 100, "height": 50 },
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: overlay, ..
        } = &page_of(&children)[2]
        else {
            panic!()
        };
        assert_eq!(overlay.len(), 9, "outline + 8 handles");
    }

    #[test]
    fn gizmo_dispatch_picks_rotate() {
        let ui = lower(json!({
            "show-gizmo": true,
            "tool": "rotate",
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: overlay, ..
        } = &page_of(&children)[2]
        else {
            panic!()
        };
        // No selection → just the gizmo
        assert_eq!(overlay.len(), 1);
        let UiNode::Container { props, .. } = &overlay[0] else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-tool" && v == "rotate"));
    }

    #[test]
    fn grid_cells_render_with_aria_grid_role() {
        let ui = lower(json!({
            "grid-cells": [
                { "x": 0, "y": 0, "width": 200, "height": 100, "occupied": false },
                { "x": 200, "y": 0, "width": 200, "height": 100, "occupied": true },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: cells,
            props,
            ..
        } = &page_of(&children)[1]
        else {
            panic!()
        };
        assert_eq!(cells.len(), 2);
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "grid"));
    }

    #[test]
    fn canvas_prepends_toolbar_above_page_when_registry_present() {
        // §43 D2: the canvas frame's first child is the toolbar; the
        // page rect is the second child. Without a registry the toolbar
        // is dropped silently — the existing `lower` helper attaches a
        // real registry, so the toolbar always appears here.
        let ui = lower(json!({}));
        let UiNode::Container {
            props, children, ..
        } = ui
        else {
            panic!()
        };
        assert_eq!(children.len(), 2, "[toolbar, page]");
        let UiNode::Container {
            props: toolbar_props,
            ..
        } = &children[0]
        else {
            panic!("toolbar not a container")
        };
        assert!(toolbar_props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "builder-toolbar"));
        // The page rect is the second child.
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "builder-canvas"));
    }
}
