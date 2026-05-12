//! `shell.nav-graph` — visual graph of pages and their navigation
//! edges (href links + NavigateTo signal connections). Reads a
//! `pages` JSON array (positioned `{ x, y, label, is-active }`
//! entries) and an `edges` JSON array (`{ x1, y1, x2, y2, kind }`
//! entries). Renders each page as an absolutely-positioned card.
//! Edges are exposed as semantic `<line>` overlays — the actual
//! stroke pass is the renderer's job; this block only emits the
//! retained-mode tree.
//!
//! Slint origin: navigation panel graph header + canvas around
//! `ui/app.slint:3531/3538`.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, uniform_radius, LowerCtx,
    },
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const CARD_W: f32 = 140.0;
const CARD_H: f32 = 56.0;
const CARD_RADIUS: f32 = 6.0;
const CARD_BG: &str = "#0a000000";
const CARD_BG_ACTIVE: &str = "#160060c0";
const CARD_HOVER: &str = "#0f000000";
const TITLE_COLOR: &str = "#000000";
const ROUTE_COLOR: &str = "#80000000";
const EDGE_LAYER_BG: &str = "#06000000";

fn nav_graph_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Section title"),
        FieldSpec::text(
            "pages",
            "Pages (JSON array of {x, y, label, route, is-active})",
        ),
        FieldSpec::text("edges", "Edges (JSON array of {x1, y1, x2, y2, kind})"),
    ]
}

fn nav_graph_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let style = StyleProperties::default();
    let title = ctx.prop_str(node, "title");

    let edges_node = build_edge_layer(node);
    let cards = build_cards(node, &style);

    let mut canvas_kids = vec![edges_node];
    canvas_kids.extend(cards);

    let canvas = bare_container(format!("{}::canvas", node.id), canvas_kids, |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.background = parse_color(EDGE_LAYER_BG);
        p.radius = uniform_radius(6.0);
        p.semantic = Semantic::tag("div")
            .with_attr("role", "img")
            .with_attr("aria-label", "Page navigation graph")
            .with_attr("data-role", "nav-graph-canvas");
    });

    let mut kids: Vec<UiNode> = Vec::new();
    if !title.is_empty() {
        kids.push(colored_text_node(
            format!("{}::title", node.id),
            title,
            &style,
            12.0,
            ROUTE_COLOR,
        ));
    }
    kids.push(canvas);

    bare_container(node.id.clone(), kids, |p| {
        p.direction = Direction::Column;
        p.gap = 8.0;
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 12.0,
            bottom: 12.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("section")
            .with_attr("aria-label", "Navigation graph")
            .with_attr("data-role", "nav-graph");
    })
}

pub const NAV_GRAPH_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.nav-graph", nav_graph_schema).lower(nav_graph_lower);

fn build_edge_layer(node: &Node) -> UiNode {
    let edges: Vec<UiNode> = node
        .props
        .get("edges")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(idx, e)| build_edge(node, idx, e))
                .collect()
        })
        .unwrap_or_default();
    bare_container(format!("{}::edges", node.id), edges, |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("div")
            .with_attr("role", "presentation")
            .with_attr("data-role", "nav-graph-edges");
    })
}

fn build_edge(node: &Node, idx: usize, edge: &Value) -> UiNode {
    let kind = edge.get("kind").and_then(|v| v.as_str()).unwrap_or("href");
    let x1 = edge.get("x1").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y1 = edge.get("y1").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let x2 = edge.get("x2").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y2 = edge.get("y2").and_then(|v| v.as_f64()).unwrap_or(0.0);

    bare_container(format!("{}::edge::{}", node.id, idx), vec![], |p| {
        p.semantic = Semantic::tag("span")
            .with_attr("role", "presentation")
            .with_attr("data-role", "nav-graph-edge")
            .with_attr("data-kind", kind)
            .with_attr("data-x1", format_coord(x1))
            .with_attr("data-y1", format_coord(y1))
            .with_attr("data-x2", format_coord(x2))
            .with_attr("data-y2", format_coord(y2));
    })
}

fn build_cards(node: &Node, style: &StyleProperties) -> Vec<UiNode> {
    node.props
        .get("pages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(idx, item)| build_card(node, idx, item, style))
                .collect()
        })
        .unwrap_or_default()
}

fn build_card(node: &Node, idx: usize, item: &Value, style: &StyleProperties) -> UiNode {
    let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
    let route = item.get("route").and_then(|v| v.as_str()).unwrap_or("");
    let active = item
        .get("is-active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let x = item.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = item.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let mut kids: Vec<UiNode> = vec![colored_text_node(
        format!("{}::card::{}::label", node.id, idx),
        label.into(),
        style,
        12.0,
        TITLE_COLOR,
    )];
    if !route.is_empty() {
        kids.push(colored_text_node(
            format!("{}::card::{}::route", node.id, idx),
            route.into(),
            style,
            10.0,
            ROUTE_COLOR,
        ));
    }

    bare_container(format!("{}::card::{}", node.id, idx), kids, |p| {
        p.direction = Direction::Column;
        p.gap = 2.0;
        p.padding = Padding {
            left: 10.0,
            right: 10.0,
            top: 8.0,
            bottom: 8.0,
        };
        p.width = Sizing::Fixed(CARD_W);
        p.height = Sizing::Fixed(CARD_H);
        p.radius = uniform_radius(CARD_RADIUS);
        p.background = parse_color(if active { CARD_BG_ACTIVE } else { CARD_BG });
        p.hover = hover_bg(CARD_HOVER);
        let mut s = Semantic::tag("div")
            .with_attr("role", "button")
            .with_attr("data-role", "nav-graph-page")
            .with_attr("data-x", format_coord(x))
            .with_attr("data-y", format_coord(y));
        if active {
            s = s.with_attr("aria-current", "page");
        }
        p.semantic = s;
    })
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

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("ng", "shell.nav-graph", props);
        lower_with(&n, nav_graph_lower)
    }

    #[test]
    fn renders_pages_and_edges() {
        let ui = lower(json!({
            "title": "Pages",
            "pages": [
                { "label": "Home", "route": "/", "x": 0, "y": 0, "is-active": true },
                { "label": "About", "route": "/about", "x": 200, "y": 0 },
            ],
            "edges": [
                { "x1": 0, "y1": 0, "x2": 200, "y2": 0, "kind": "href" },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // title + canvas
        assert_eq!(children.len(), 2);
        let UiNode::Container {
            children: canvas, ..
        } = &children[1]
        else {
            panic!()
        };
        // edge layer + 2 cards
        assert_eq!(canvas.len(), 3);
        let UiNode::Container {
            children: edges, ..
        } = &canvas[0]
        else {
            panic!()
        };
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn empty_graph_has_canvas_only() {
        let ui = lower(json!({}));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 1, "no title → canvas only");
    }
}
