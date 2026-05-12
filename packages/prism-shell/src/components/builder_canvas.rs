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
    ui_lower::{bare_container, hover_bg, parse_color, uniform_radius, LowerCtx},
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::{json, Value};

const CANVAS_BG: &str = "#040000";
const PAGE_BG: &str = "#ffffff";
const GRID_LINE: &str = "#22000000";
const SELECTION_BORDER: &str = "#0060c0";
/// Wave 3.2 polish — fill colour of the palette-drag ghost pill
/// painted under the cursor while the user drags a palette item
/// across the canvas. Translucent blue so the underlying preview
/// stays legible.
const PALETTE_GHOST_BG: &str = "#660060c0";
const PALETTE_GHOST_BORDER: &str = "#0060c0";
/// Translucent blue tint painted as the background of the currently
/// selected canvas-document node. Visible against any underlying
/// content (white page bg, text glyphs, button fills) without
/// occluding it — gives the user immediate "I selected this"
/// feedback even before a proper bbox outline lands.
const SELECTION_TINT: &str = "#330060c0";
/// Resting hover tint painted as `props.hover.background` on every
/// canvas-document container. With `Surface::set_hovered` wired in
/// the shell, the cursor passing over a preview node now flashes
/// this tint — the canvas tree behaves like every other clickable
/// chrome surface.
const CANVAS_NODE_HOVER_BG: &str = "#1a0060c0";
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
        FieldSpec::text(
            "palette-drag",
            "Palette-drag ghost JSON {active, kind, x, y, drop-target}",
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
    let selection_id = ctx.prop_str(node, "selection-id");
    let selection_id = if selection_id.is_empty() {
        None
    } else {
        Some(selection_id)
    };
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
    //
    // The same walk also paints two visual affordances: a hover-bg
    // override (so every preview node tints when the cursor passes,
    // mirroring the chrome's clickability cue) and the selection
    // tint when the node id matches the currently selected document
    // node. Both run off `selection-id` from `builder_canvas_props`
    // — no separate "lookup the layout bbox" pass required.
    tag_canvas_subtree(&mut preview, selection_id.as_deref());

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
    let show_gizmo = ctx.prop_bool(node, "show-gizmo", false);
    let tool = ctx.prop_str(node, "tool");

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

    // Wave 3.2 polish: paint a translucent ghost pill at the cursor
    // while a palette-drag is in flight. The pill carries the
    // palette item's id as `data-kind` so the renderer can draw a
    // label or icon next to the rect when needed (the minimal fill
    // is enough for the user to track the cursor today).
    if let Some(ghost) = build_palette_drag_ghost(node) {
        layer_kids.push(ghost);
    }

    bare_container(format!("{}::overlay", node.id), layer_kids, |p| {
        p.semantic = Semantic::tag("div")
            .with_attr("role", "presentation")
            .with_attr("data-role", "canvas-overlay");
    })
}

/// Wave 3.2 polish — render the palette-drag ghost as an absolute-
/// positioned rect inside the canvas overlay. Reads `palette-drag`
/// from the canvas's prop bag (populated by `props.rs` from
/// `CatalogSlot::palette_drag`). Returns `None` when no drag is
/// active so the overlay layer stays untouched on the common path.
fn build_palette_drag_ghost(node: &Node) -> Option<UiNode> {
    let drag = node.props.get("palette-drag")?;
    if !drag
        .get("active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    let x = drag.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = drag.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let kind = drag
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let drop_target = drag
        .get("drop-target")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(bare_container(
        format!("{}::palette-ghost", node.id),
        vec![],
        |p| {
            p.width = Sizing::Fixed(96.0);
            p.height = Sizing::Fixed(28.0);
            p.background = parse_color(PALETTE_GHOST_BG);
            p.radius = uniform_radius(4.0);
            let mut s = Semantic::tag("div")
                .with_attr("role", "presentation")
                .with_attr("data-role", "palette-ghost")
                .with_attr("data-kind", kind)
                // Cursor anchor — top-left of the ghost paints at
                // the pointer. The renderer reads `data-x`/`data-y`
                // the same way it does for `selection-outline` so
                // the two overlay variants share the absolute-
                // positioning convention.
                .with_attr("data-x", format_coord(x))
                .with_attr("data-y", format_coord(y))
                .with_attr("data-stroke", PALETTE_GHOST_BORDER);
            if !drop_target.is_empty() {
                s = s.with_attr("data-drop-target", drop_target);
            }
            p.semantic = s;
        },
    ))
}

/// Annotate every container in `nodes` (recursively) with the three
/// canvas-preview affordances:
///
/// 1. `data-canvas-node="<id>"` semantic attr — pointer-down routing
///    uses this to recognise canvas-doc descendants without false
///    positives on chrome containers that share an id (the canonical
///    collision is `root` — both `<shell.app-window id="root">` and
///    `BuilderDocument::page_shell()` use it).
/// 2. A `props.hover` background tint — so the cursor passing over a
///    preview node tints it, mirroring how chrome surfaces signal
///    clickability. The shell's hover dispatch (`Surface::set_hovered`
///    on every PointerMove) drives the paint swap.
/// 3. A selection background tint when `selection_id` matches the
///    container's id — the user's "I selected this" feedback before
///    a proper bbox outline lands. Pre-existing `background` settings
///    are preserved by *only* overriding when the resting bg is `None`
///    on hover, and unconditionally tinting on selection (the selection
///    paint wins over any base bg colour by design).
///
/// Only Container variants contribute to `Surface::hit_test_at` hits;
/// only they receive the decoration — leaves never carry hover state.
fn tag_canvas_subtree(nodes: &mut [UiNode], selection_id: Option<&str>) {
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
                // Universal hover affordance — every preview node
                // flashes the tint as the cursor passes. Authors who
                // declared their own `props.hover` in a custom block
                // keep it (we only fill the slot when it's empty).
                if props.hover.is_none() {
                    props.hover = hover_bg(CANVAS_NODE_HOVER_BG);
                }
                if selection_id == Some(id.as_str()) {
                    if let Some(c) = parse_color(SELECTION_TINT) {
                        props.background = Some(c);
                    }
                    props
                        .semantic
                        .attrs
                        .push(("data-selected".into(), "true".into()));
                }
            }
            tag_canvas_subtree(children, selection_id);
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

    /// Walk a lowered tree and return the first container whose
    /// `data-canvas-node` attr matches `id`. Encapsulates the
    /// preview-layer search so the selection / hover tests stay
    /// readable.
    fn find_canvas_node<'a>(root: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        let UiNode::Container {
            props, children, ..
        } = root
        else {
            return None;
        };
        let tagged = props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-canvas-node" && v == id);
        if tagged {
            return Some(root);
        }
        children.iter().find_map(|c| find_canvas_node(c, id))
    }

    fn lower_with_preview(props: Value, doc_nodes: Vec<prism_builder::Node>) -> UiNode {
        // Build a canvas node carrying a synthetic preview child tree —
        // the lower path forwards `node.children` through
        // `ctx.lower_children` when no `host_children` are injected,
        // which is exactly the shape we need to exercise
        // `tag_canvas_subtree`.
        let mut n = test_node("bc", "shell.builder-canvas", props);
        n.children = doc_nodes;
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register shell");
        // The preview nodes are `container` / `text` etc. — register
        // the builder builtins so `lower_children` can resolve them.
        let mut comp_reg = prism_builder::ComponentRegistry::new();
        prism_builder::starter::register_builtins(&mut comp_reg)
            .expect("register builder builtins");
        // Merge: re-register every shell-side spec onto the builder reg
        // — the lower path takes ONE registry. The test only needs to
        // resolve the document's own component ids, so the builder reg
        // alone is enough.
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&comp_reg), &cascade);
        builder_canvas_lower(&ctx, &n, &cascade)
    }

    #[test]
    fn selected_canvas_node_paints_tint_and_carries_data_selected_attr() {
        // The §B5 click route mutates `state.canvas.selection`; the
        // canvas binding forwards it as `selection-id`; this lower
        // pass must show the user *something* changed by tinting
        // the matching preview container.
        // Use the `container` block so the lowered preview entry is a
        // `UiNode::Container` (the only variant `tag_canvas_subtree`
        // walks into). `text` lowers to a `UiNode::Text` leaf, which
        // would never carry `data-canvas-node` regardless of the
        // tagging pass.
        let doc = vec![prism_builder::Node {
            id: "demo-heading".into(),
            component: prism_builder::ComponentId::from("container"),
            props: json!({}),
            ..Default::default()
        }];
        let ui = lower_with_preview(json!({ "selection-id": "demo-heading" }), doc);
        let found = find_canvas_node(&ui, "demo-heading").expect("preview node");
        let UiNode::Container { props, .. } = found else {
            panic!()
        };
        assert!(
            props.background.is_some(),
            "selected canvas node paints the SELECTION_TINT background"
        );
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-selected" && v == "true"));
    }

    #[test]
    fn unselected_canvas_node_does_not_carry_data_selected_attr() {
        // Use the `container` block so the lowered preview entry is a
        // `UiNode::Container` (the only variant `tag_canvas_subtree`
        // walks into). `text` lowers to a `UiNode::Text` leaf, which
        // would never carry `data-canvas-node` regardless of the
        // tagging pass.
        let doc = vec![prism_builder::Node {
            id: "demo-heading".into(),
            component: prism_builder::ComponentId::from("container"),
            props: json!({}),
            ..Default::default()
        }];
        let ui = lower_with_preview(json!({ "selection-id": "" }), doc);
        let found = find_canvas_node(&ui, "demo-heading").expect("preview node");
        let UiNode::Container { props, .. } = found else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .all(|(k, _)| k != "data-selected"));
    }

    #[test]
    fn every_canvas_node_declares_hover_bg_for_set_hovered_paint() {
        // The shell's `Surface::set_hovered` wiring lights up
        // `props.hover` on the container under the cursor. Without
        // this declaration on each preview container, hovering a
        // canvas node would do nothing — defeating the visual
        // affordance the rest of the chrome already carries.
        // Use the `container` block so the lowered preview entry is a
        // `UiNode::Container` (the only variant `tag_canvas_subtree`
        // walks into). `text` lowers to a `UiNode::Text` leaf, which
        // would never carry `data-canvas-node` regardless of the
        // tagging pass.
        let doc = vec![prism_builder::Node {
            id: "demo-heading".into(),
            component: prism_builder::ComponentId::from("container"),
            props: json!({}),
            ..Default::default()
        }];
        let ui = lower_with_preview(json!({}), doc);
        let found = find_canvas_node(&ui, "demo-heading").expect("preview node");
        let UiNode::Container { props, .. } = found else {
            panic!()
        };
        assert!(
            props.hover.is_some(),
            "every canvas preview container declares hover_bg"
        );
    }

    #[test]
    fn palette_drag_ghost_paints_inside_canvas_overlay_when_active() {
        // Wave 3.2 polish: the canvas overlay layer carries a
        // `data-role="palette-ghost"` container at the cursor
        // position whenever a palette drag is in flight.
        let ui = lower(json!({
            "palette-drag": {
                "active": true,
                "kind": "text",
                "x": 120.0,
                "y": 240.0,
                "drop-target": "demo-heading",
            },
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
        let ghost = overlay
            .iter()
            .find_map(|c| {
                if let UiNode::Container { props, .. } = c {
                    if props
                        .semantic
                        .attrs
                        .iter()
                        .any(|(k, v)| k == "data-role" && v == "palette-ghost")
                    {
                        return Some(props);
                    }
                }
                None
            })
            .expect("palette ghost present");
        assert!(ghost
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-kind" && v == "text"));
        assert!(ghost
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-x" && v == "120"));
        assert!(ghost
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-y" && v == "240"));
        assert!(ghost
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-drop-target" && v == "demo-heading"));
    }

    #[test]
    fn palette_drag_ghost_absent_when_inactive() {
        let ui = lower(json!({
            "palette-drag": { "active": false },
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
        assert!(
            overlay.iter().all(|c| {
                let UiNode::Container { props, .. } = c else {
                    return true;
                };
                props
                    .semantic
                    .attrs
                    .iter()
                    .all(|(k, v)| !(k == "data-role" && v == "palette-ghost"))
            }),
            "no ghost while inactive",
        );
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
