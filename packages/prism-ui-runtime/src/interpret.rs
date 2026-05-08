//! Interpret path — parse a `.prism-ui` source string and lower the
//! resulting AST onto the runtime's typed [`layout::Node`] tree.
//!
//! This is the runtime half of the codegen story (Phase 2 tail, plan
//! §4.6 / §4.7). The DSL parser lives in
//! `prism_core::language::prism_ui`; here we walk its [`AstDocument`]
//! and emit `Node`s the layout engine already understands. The same
//! function powers compile-time validation in `prism-ui-build` and
//! the live-edit re-parse loop the shell will install in Phase 4.
//!
//! The supported surface is intentionally small — the v0 grammar
//! covers exactly the primitives the layout engine ships:
//! `<container>`, `<text>`, `<heading>`, `<spacer>`. Component
//! declarations (`<component name="...">`) round-trip as opaque
//! containers for now; the per-component instantiation pass lands in
//! Phase 3 with the unified component model.

use prism_core::language::prism_ui::{
    parse, AttributeNamespace, AttributeValue, Document as AstDocument, Element, Node as AstNode,
    ParseError,
};

use crate::command::{Color, CornerRadius};
use crate::layout::{ContainerProps, Direction, Node, Padding, Sizing, TextProps};

/// Parse + lower in one shot. Recoverable parse errors abort the
/// lowering — callers (build script, live-edit loop) surface them to
/// the editor.
pub fn interpret(source: &str) -> Result<Vec<Node>, Vec<ParseError>> {
    let (document, errors) = parse(source);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(lower_document(&document))
}

/// Lower an already-parsed AST document into a flat sequence of
/// runtime `Node`s. Children of unrecognised tags are flattened
/// upward so a host wrapping its scene in an unknown root still gets
/// a usable tree.
pub fn lower_document(document: &AstDocument) -> Vec<Node> {
    document.nodes.iter().flat_map(lower_node).collect()
}

fn lower_node(node: &AstNode) -> Vec<Node> {
    match node {
        AstNode::Element(el) => lower_element(el),
        AstNode::Text { value, .. } => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![Node::Text {
                    id: String::new(),
                    content: trimmed.to_string(),
                    props: TextProps::default(),
                }]
            }
        }
        AstNode::Interpolation(_) | AstNode::Comment { .. } => Vec::new(),
    }
}

fn lower_element(el: &Element) -> Vec<Node> {
    match el.tag.as_str() {
        "container" | "component" => {
            let mut props = ContainerProps::default();
            let mut id = String::new();
            apply_container_attributes(el, &mut props, &mut id);
            let children = el.children.iter().flat_map(lower_node).collect();
            vec![Node::Container {
                id,
                props,
                children,
            }]
        }
        "text" | "heading" => {
            let mut props = TextProps::default();
            let mut id = String::new();
            apply_text_attributes(el, &mut props, &mut id);
            if el.tag == "heading" && font_size_is_default(&props) {
                props.font_size = heading_font_size(el);
            }
            let content = collect_text_content(&el.children);
            vec![Node::Text { id, content, props }]
        }
        "spacer" => {
            let mut id = String::new();
            let mut width = 0.0;
            let mut height = 0.0;
            for attr in &el.attributes {
                if !matches!(attr.name.namespace, AttributeNamespace::Bare)
                    && !matches!(attr.name.namespace, AttributeNamespace::Identifier)
                {
                    continue;
                }
                let raw = attribute_string(&attr.value);
                match attr.name.local.as_str() {
                    "id" => id = raw.unwrap_or_default(),
                    "width" => width = raw.as_deref().and_then(parse_f32).unwrap_or(0.0),
                    "height" => height = raw.as_deref().and_then(parse_f32).unwrap_or(0.0),
                    _ => {}
                }
            }
            vec![Node::Spacer { id, width, height }]
        }
        // Unknown tag — drop the wrapping element, keep its children.
        // Lets a host nest a scene inside e.g. `<scene>` without
        // forcing the runtime to know about it.
        _ => el.children.iter().flat_map(lower_node).collect(),
    }
}

fn apply_container_attributes(el: &Element, props: &mut ContainerProps, id: &mut String) {
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        let raw = attribute_string(&attr.value);
        match attr.name.namespace {
            AttributeNamespace::Bare => match local {
                "direction" => {
                    if let Some(v) = raw.as_deref() {
                        props.direction = parse_direction(v);
                    }
                }
                "gap" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        props.gap = v;
                    }
                }
                "padding" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        props.padding = Padding::all(v);
                    }
                }
                "padding-left" => set_padding_side(&mut props.padding, raw.as_deref(), Side::Left),
                "padding-right" => {
                    set_padding_side(&mut props.padding, raw.as_deref(), Side::Right)
                }
                "padding-top" => set_padding_side(&mut props.padding, raw.as_deref(), Side::Top),
                "padding-bottom" => {
                    set_padding_side(&mut props.padding, raw.as_deref(), Side::Bottom)
                }
                "width" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        props.width = s;
                    }
                }
                "height" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        props.height = s;
                    }
                }
                _ => {}
            },
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    *id = v;
                }
            }
            AttributeNamespace::Style => match local {
                "background" => {
                    if let Some(c) = raw.as_deref().and_then(parse_color) {
                        props.background = Some(c);
                    }
                }
                "radius" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        props.radius = CornerRadius {
                            tl: v,
                            tr: v,
                            br: v,
                            bl: v,
                        };
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn apply_text_attributes(el: &Element, props: &mut TextProps, id: &mut String) {
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        let raw = attribute_string(&attr.value);
        match attr.name.namespace {
            AttributeNamespace::Bare if local == "font-size" => {
                if let Some(v) = raw.as_deref().and_then(parse_f32) {
                    props.font_size = v;
                }
            }
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    *id = v;
                }
            }
            AttributeNamespace::Style if local == "color" => {
                if let Some(c) = raw.as_deref().and_then(parse_color) {
                    props.color = c;
                }
            }
            _ => {}
        }
    }
}

fn font_size_is_default(props: &TextProps) -> bool {
    (props.font_size - TextProps::default().font_size).abs() < f32::EPSILON
}

/// `<heading level="N">` mirrors HTML — h1..h6 step down in 4pt
/// increments from a 28pt base, capped at the body default.
fn heading_font_size(el: &Element) -> f32 {
    let level = el
        .attributes
        .iter()
        .find(|a| a.name.raw == "level")
        .and_then(|a| attribute_string(&a.value))
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1);
    match level.clamp(1, 6) {
        1 => 28.0,
        2 => 24.0,
        3 => 20.0,
        4 => 18.0,
        5 => 16.0,
        _ => 14.0,
    }
}

fn collect_text_content(children: &[AstNode]) -> String {
    let mut out = String::new();
    for c in children {
        if let AstNode::Text { value, .. } = c {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(value.trim());
        }
    }
    out
}

fn attribute_string(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::String { value, .. } => Some(value.clone()),
        AttributeValue::Empty => None,
        AttributeValue::Expression(expr) => Some(format!("{{{}}}", expr.body)),
        AttributeValue::Template { parts, .. } => {
            let mut out = String::new();
            for part in parts {
                match part {
                    prism_core::language::prism_ui::ast::TemplatePart::Literal {
                        value, ..
                    } => out.push_str(value),
                    prism_core::language::prism_ui::ast::TemplatePart::Expression(e) => {
                        out.push('{');
                        out.push_str(&e.body);
                        out.push('}');
                    }
                }
            }
            Some(out)
        }
    }
}

fn parse_f32(s: &str) -> Option<f32> {
    s.trim().trim_end_matches("px").parse::<f32>().ok()
}

fn parse_direction(s: &str) -> Direction {
    match s.trim() {
        "row" => Direction::Row,
        _ => Direction::Column,
    }
}

fn parse_sizing(s: &str) -> Option<Sizing> {
    let s = s.trim();
    match s {
        "grow" => Some(Sizing::Grow),
        "fit" => Some(Sizing::Fit),
        _ => parse_f32(s).map(Sizing::Fixed),
    }
}

fn parse_color(raw: &str) -> Option<Color> {
    let s = raw.trim();
    let hex = s.strip_prefix('#')?;
    let (r, g, b, a) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            255,
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            u8::from_str_radix(&hex[6..8], 16).ok()?,
        ),
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            (r * 17, g * 17, b * 17, 255)
        }
        _ => return None,
    };
    Some(Color { r, g, b, a })
}

enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

fn set_padding_side(padding: &mut Padding, raw: Option<&str>, side: Side) {
    let Some(v) = raw.and_then(parse_f32) else {
        return;
    };
    match side {
        Side::Left => padding.left = v,
        Side::Right => padding.right = v,
        Side::Top => padding.top = v,
        Side::Bottom => padding.bottom = v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{compute, Viewport};

    const FIVE_ELEMENT_SOURCE: &str = r##"<container direction="column" gap="8" padding="16" width="grow" height="grow" style:background="#f0f0f0">
  <text id="title" font-size="24" style:color="#141414">Prism</text>
  <container id="row" direction="row" gap="8" width="grow" height="40" style:background="#ffffff">
    <text id="a">A</text>
    <spacer id="gap" width="16" height="0"/>
    <text id="b">B</text>
  </container>
</container>"##;

    #[test]
    fn parses_minimal_container() {
        let nodes = interpret("<container/>").unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(matches!(nodes[0], Node::Container { .. }));
    }

    #[test]
    fn parses_text_content() {
        let nodes = interpret(r#"<text>Hello</text>"#).unwrap();
        let Node::Text { content, .. } = &nodes[0] else {
            panic!("expected text");
        };
        assert_eq!(content, "Hello");
    }

    #[test]
    fn parses_color_and_sizing() {
        let nodes =
            interpret(r##"<container width="grow" height="40" style:background="#ff8800"/>"##)
                .unwrap();
        let Node::Container { props, .. } = &nodes[0] else {
            panic!()
        };
        assert!(matches!(props.width, Sizing::Grow));
        assert!(matches!(props.height, Sizing::Fixed(v) if (v - 40.0).abs() < f32::EPSILON));
        let bg = props.background.expect("background");
        assert_eq!((bg.r, bg.g, bg.b, bg.a), (0xff, 0x88, 0x00, 0xff));
    }

    #[test]
    fn heading_level_drives_font_size() {
        let nodes = interpret(r#"<heading level="3">Hi</heading>"#).unwrap();
        let Node::Text { props, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(props.font_size, 20.0);
    }

    /// Phase-2 round trip: source → AST → Rust nodes → render commands.
    /// The interpreted scene must match the hand-built five-element
    /// scene the layout snapshot test already pins.
    #[test]
    fn five_element_source_round_trips_to_layout() {
        let nodes = interpret(FIVE_ELEMENT_SOURCE).unwrap();
        assert_eq!(nodes.len(), 1);
        let cmds = compute(
            &nodes[0],
            Viewport {
                width: 800.0,
                height: 600.0,
            },
        );
        // Same five commands the hand-built scene produces:
        // root background + row background + 3 text leaves.
        assert_eq!(cmds.len(), 5);
    }

    #[test]
    fn parse_errors_propagate() {
        let err = interpret("<container>").unwrap_err();
        assert!(!err.is_empty());
    }
}
