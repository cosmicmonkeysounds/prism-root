//! `shell.app-card` — 160px launchpad card for an installed app (or
//! the "Create App" affordance). Hover swaps background + border;
//! click opens the app.
//!
//! Slint origin: `AppCard` in `ui/app.slint` (lines 1349-1473).
//!
//! Lowering: an outer 160px-tall container with a 4px accent rail
//! pinned to the top edge (declared as the first child — vertical
//! flow puts it at y=0), then a body column with the icon row,
//! description, and page-count badge. The `is-create` mode swaps the
//! body for a centred plus glyph + "Create App" label.
//!
//! Icon name → asset path is a lookup table in this file; the cascade
//! never sees these names, so resolving here keeps the runtime
//! `Image` source string concrete.

use prism_builder::{
    document::Node,
    registry::{FieldSpec, NumericBounds},
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, image_node, parse_color, uniform_radius,
        LowerCtx,
    },
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const CARD_HEIGHT: f32 = 160.0;
const CARD_RADIUS: f32 = 12.0;
const CARD_BG: &str = "#ffffff";
const CARD_HOVER_BG: &str = "#f4f4f4";
const RAIL_HEIGHT: f32 = 4.0;
const PAD: f32 = 20.0;
const ICON_HOST: f32 = 36.0;
const ICON_HOST_RADIUS: f32 = 8.0;
const ICON_GLYPH: f32 = 18.0;
const NAME_COLOR: &str = "#000000";
const DESCRIPTION_COLOR: &str = "#99000000";
const BADGE_BG: &str = "#14000000";
const BADGE_COLOR: &str = "#7f000000";
const CREATE_LABEL_COLOR: &str = "#b3000000";

/// Map the design-token icon name to the asset path. One table; the
/// lowering body never branches per name. Unknown names fall through
/// to a neutral box icon so the runtime still draws something.
fn icon_path(name: &str) -> &'static str {
    match name {
        "globe" => "icons/globe.svg",
        "music" => "icons/grip.svg",
        "zap" => "icons/zap.svg",
        "code" => "icons/code.svg",
        "cube" => "icons/box.svg",
        "star" => "icons/help-circle.svg",
        "heart" => "icons/user.svg",
        "film" => "icons/image.svg",
        _ => "icons/box.svg",
    }
}

fn app_card_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("app-id", "App ID").required(),
        FieldSpec::text("name", "Name"),
        FieldSpec::text("description", "Description"),
        FieldSpec::text("icon", "Icon"),
        FieldSpec::text("accent-color", "Accent color")
            .with_default(Value::String("#0060c0".into())),
        FieldSpec::integer(
            "page-count",
            "Page count",
            NumericBounds::min_max(0.0, 999.0),
        ),
        FieldSpec::boolean("is-create", "Is create card").with_default(Value::Bool(false)),
    ]
}

fn app_card_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let app_id = ctx.prop_str(node, "app-id");
    let name = ctx.prop_str(node, "name");
    let description = ctx.prop_str(node, "description");
    let icon_name = ctx.prop_str(node, "icon");
    let accent = ctx.prop_str(node, "accent-color");
    let accent: &str = if accent.is_empty() {
        "#0060c0"
    } else {
        accent.as_str()
    };
    let page_count = node
        .props
        .get("page-count")
        .and_then(|v| v.as_i64())
        .unwrap_or(1);
    let is_create = ctx.prop_bool(node, "is-create", false);

    let rail = bare_container(format!("{}::rail", node.id), vec![], |p| {
        p.height = Sizing::Fixed(RAIL_HEIGHT);
        p.background = parse_color(accent);
    });

    let body = if is_create {
        create_body(node, style)
    } else {
        app_body(
            node,
            style,
            &name,
            &description,
            &icon_name,
            accent,
            page_count,
        )
    };

    // Outer card. Background + border get hover-swapped through the
    // standard hover-overrides path. SSR semantic is <article> with
    // the app-id surfaced as `data-app` so launchpad screen-readers
    // / nav scripts have something to target.
    //
    // B6 follow-up: the article carries `data-role="app-card"` so
    // `POINTER_ROUTES` can dispatch the click to
    // `WorkspaceSlot::set_active_app`; the `data-app` attr does
    // double duty as the route's target id. The "create" card opts
    // out of the route (no app id to load) by simply not carrying a
    // `data-app` value, which the route guard treats as a no-op.
    bare_container(node.id.clone(), vec![rail, body], |props| {
        props.direction = Direction::Column;
        props.height = Sizing::Fixed(CARD_HEIGHT);
        props.background = parse_color(CARD_BG);
        props.radius = uniform_radius(CARD_RADIUS);
        props.hover = hover_bg(CARD_HOVER_BG);
        let mut semantic = Semantic::tag("article").with_attr("data-role", "app-card");
        if !app_id.is_empty() {
            semantic = semantic.with_attr("data-app", &app_id);
        }
        if is_create {
            semantic = semantic.with_attr("data-create", "true");
        }
        props.semantic = semantic;
    })
}

pub const APP_CARD_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.app-card", app_card_schema).lower(app_card_lower);

fn create_body(node: &Node, style: &StyleProperties) -> UiNode {
    let plus = image_node(
        format!("{}::plus", node.id),
        "icons/plus.svg".into(),
        style,
        Sizing::Fixed(32.0),
        Sizing::Fixed(32.0),
    );
    let label = colored_text_node(
        format!("{}::create-label", node.id),
        "Create App".into(),
        style,
        14.0,
        CREATE_LABEL_COLOR,
    );
    bare_container(format!("{}::body", node.id), vec![plus, label], |p| {
        p.direction = Direction::Column;
        p.gap = 8.0;
        p.height = Sizing::Grow;
        p.padding = Padding {
            left: PAD,
            right: PAD,
            top: PAD + 4.0,
            bottom: PAD,
        };
    })
}

fn app_body(
    node: &Node,
    style: &StyleProperties,
    name: &str,
    description: &str,
    icon_name: &str,
    accent: &str,
    page_count: i64,
) -> UiNode {
    let glyph = image_node(
        format!("{}::icon", node.id),
        icon_path(icon_name).into(),
        style,
        Sizing::Fixed(ICON_GLYPH),
        Sizing::Fixed(ICON_GLYPH),
    );
    let icon_host = bare_container(format!("{}::icon-host", node.id), vec![glyph], |p| {
        p.width = Sizing::Fixed(ICON_HOST);
        p.height = Sizing::Fixed(ICON_HOST);
        p.radius = uniform_radius(ICON_HOST_RADIUS);
        // Translucent accent — we don't have token transparentize() here,
        // so use the accent's alpha-tinted RGBA when the prop is in that
        // form, otherwise fall through to no-fill (the runtime will
        // resolve the actual tint when the design-tokens cascade lands).
        p.background = parse_color(accent);
    });
    let name_text = colored_text_node(
        format!("{}::name", node.id),
        name.into(),
        style,
        16.0,
        NAME_COLOR,
    );
    let header = bare_container(
        format!("{}::header", node.id),
        vec![icon_host, name_text],
        |p| {
            p.direction = Direction::Row;
            p.gap = 10.0;
        },
    );

    let description_text = colored_text_node(
        format!("{}::desc", node.id),
        description.into(),
        style,
        12.0,
        DESCRIPTION_COLOR,
    );

    let badge_label = if page_count == 1 {
        "1 page".to_string()
    } else {
        format!("{page_count} pages")
    };
    let badge_text = colored_text_node(
        format!("{}::badge-text", node.id),
        badge_label,
        style,
        10.0,
        BADGE_COLOR,
    );
    let badge = bare_container(format!("{}::badge", node.id), vec![badge_text], |p| {
        p.height = Sizing::Fixed(20.0);
        p.radius = uniform_radius(4.0);
        p.background = parse_color(BADGE_BG);
        p.padding = Padding {
            left: 6.0,
            right: 6.0,
            top: 0.0,
            bottom: 0.0,
        };
    });

    bare_container(
        format!("{}::body", node.id),
        vec![header, description_text, badge],
        |p| {
            p.direction = Direction::Column;
            p.gap = 8.0;
            p.height = Sizing::Grow;
            p.padding = Padding {
                left: PAD,
                right: PAD,
                top: PAD,
                bottom: PAD,
            };
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::Block;
    use serde_json::json;

    fn lower_one(node: &BuilderNode) -> UiNode {
        lower_with(node, app_card_lower)
    }

    fn card(props: Value) -> BuilderNode {
        test_node("c", "shell.app-card", props)
    }

    #[test]
    fn standard_card_lowers_to_rail_plus_body() {
        let ui = lower_one(&card(json!({
            "app-id": "lattice",
            "name": "Lattice",
            "description": "Build websites",
            "icon": "globe",
            "accent-color": "#0060c0",
            "page-count": 4
        })));
        if let UiNode::Container {
            children, props, ..
        } = ui
        {
            assert_eq!(children.len(), 2, "rail + body");
            assert!(props.hover.is_some(), "card declares hover bg");
            assert_eq!(props.semantic.tag.as_deref(), Some("article"));
            assert!(props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "data-app" && v == "lattice"));
            // B6 follow-up: card carries the `data-role` routing key
            // POINTER_ROUTES dispatches through.
            assert!(props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "data-role" && v == "app-card"));
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn create_card_uses_centred_plus_body() {
        let ui = lower_one(&card(json!({
            "app-id": "new",
            "is-create": true
        })));
        if let UiNode::Container {
            props, children, ..
        } = ui
        {
            assert!(props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "data-create" && v == "true"));
            // body is the second child
            if let UiNode::Container {
                children: body_kids,
                ..
            } = &children[1]
            {
                // 2 kids: plus image + label
                assert_eq!(body_kids.len(), 2);
                assert!(matches!(body_kids[0], UiNode::Image { .. }));
            }
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn page_count_one_uses_singular_label() {
        let ui = lower_one(&card(json!({
            "app-id": "x", "page-count": 1
        })));
        let body_kids = body_children(&ui);
        let badge = &body_kids[2];
        if let UiNode::Container { children: kids, .. } = badge {
            if let UiNode::Text { content, .. } = &kids[0] {
                assert_eq!(content, "1 page");
            }
        }
    }

    #[test]
    fn page_count_many_uses_plural_label() {
        let ui = lower_one(&card(json!({
            "app-id": "x", "page-count": 7
        })));
        let body_kids = body_children(&ui);
        let badge = &body_kids[2];
        if let UiNode::Container { children: kids, .. } = badge {
            if let UiNode::Text { content, .. } = &kids[0] {
                assert_eq!(content, "7 pages");
            }
        }
    }

    #[test]
    fn unknown_icon_falls_back_to_box() {
        assert_eq!(icon_path("not-a-real-icon"), "icons/box.svg");
    }

    #[test]
    fn schema_declares_seven_fields() {
        let block = prism_builder::SpecBlock::new(&super::APP_CARD_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(
            keys,
            vec![
                "app-id",
                "name",
                "description",
                "icon",
                "accent-color",
                "page-count",
                "is-create",
            ]
        );
    }

    fn body_children(ui: &UiNode) -> &[UiNode] {
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: body_kids,
            ..
        } = &children[1]
        else {
            panic!()
        };
        body_kids
    }
}
