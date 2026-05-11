//! `shell.field-editor` — kind-driven property-row used by every
//! property panel that edits a single value (boolean / select / color
//! / number / integer / text / file).
//!
//! Slint origin: `FieldEditor` in `ui/app.slint` (lines 819-1190).
//!
//! Smart pattern: the per-kind visual is dispatched through a
//! `kind_body` function returning a `Vec<UiNode>`, fed by a
//! `KindBuilder` fn-pointer table. The lowering body is one branch:
//! pick the builder, run it, wrap the result with the shared label +
//! padding chrome. Adding a new kind is one entry in `KIND_TABLE`
//! plus one body fn. Boilerplate (swatch sizing, dropdown header
//! shape, switch track colours) reuses [`super::chrome`] helpers and
//! the existing [`prism_builder::ui_lower`] primitives — no
//! per-kind hand-rolled `UiNode::Container { … }` literals.

use prism_builder::{
    document::Node,
    registry::{FieldSpec, NumericBounds, SelectOption},
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, prop_bool, prop_str,
        text_input_node, text_node, uniform_radius, LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use super::chrome::{drag_number_field_node, format_drag_value, DRAG_NUMBER_LABEL_COLOR};

const VSTACK_GAP: f32 = 4.0;
const FIELD_PAD: f32 = 4.0;
const LABEL_FONT_SIZE: f32 = 12.0;
const REQUIRED_MARK: &str = " *";

/// Accent ring painted around a focused field-editor row. Sits behind
/// the existing label + body, so the chrome reads as "this is the
/// field receiving keystrokes." Color matches the rest of the shell's
/// "active" accent (the same `#0060c0` family the inspector row uses).
const FOCUS_RING_BG: &str = "#180060c0";
const FOCUS_RING_RADIUS: f32 = 4.0;

const SWITCH_WIDTH: f32 = 36.0;
const SWITCH_HEIGHT: f32 = 18.0;
const SWITCH_RADIUS: f32 = 9.0;
const SWITCH_THUMB_SIZE: f32 = 14.0;
const SWITCH_ON_BG: &str = "#336699";
const SWITCH_OFF_BG: &str = "#33000000";
const SWITCH_THUMB: &str = "#ffffff";

const PILL_HEIGHT: f32 = 28.0;
const PILL_RADIUS: f32 = 4.0;
const PILL_BG: &str = "#08000000";
const PILL_HOVER_BG: &str = "#14000000";
const PICKER_GAP: f32 = 6.0;
const SWATCH_SIZE: f32 = 28.0;
const SWATCH_RADIUS: f32 = 4.0;

/// Closure-shape for kind-specific bodies. Receives the parent node
/// (for id namespacing + props) and returns the children that go
/// *below* the label inside the field-editor stack.
type KindBuilder = fn(&Node) -> Vec<UiNode>;

struct KindEntry {
    /// Match string from `node.props["kind"]`.
    kind: &'static str,
    /// Builds the kind-specific body (under the label).
    body: KindBuilder,
    /// SSR semantic role hint for the wrapper (`group` / `presentation`).
    aria_role: &'static str,
}

const KIND_TABLE: &[KindEntry] = &[
    KindEntry {
        kind: "boolean",
        body: build_boolean_body,
        aria_role: "group",
    },
    KindEntry {
        kind: "select",
        body: build_select_body,
        aria_role: "group",
    },
    KindEntry {
        kind: "color",
        body: build_color_body,
        aria_role: "group",
    },
    KindEntry {
        kind: "number",
        body: build_number_body,
        aria_role: "group",
    },
    KindEntry {
        kind: "integer",
        body: build_number_body,
        aria_role: "group",
    },
    KindEntry {
        kind: "file",
        body: build_text_body,
        aria_role: "group",
    },
    KindEntry {
        kind: "text",
        body: build_text_body,
        aria_role: "group",
    },
];

fn lookup_kind(kind: &str) -> &'static KindEntry {
    KIND_TABLE
        .iter()
        .find(|e| e.kind == kind)
        .unwrap_or(&KIND_TABLE[KIND_TABLE.len() - 1]) // fall through to text
}

fn field_editor_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("key", "Key"),
        FieldSpec::text("label", "Label"),
        FieldSpec::select(
            "kind",
            "Kind",
            vec![
                SelectOption::new("text", "Text"),
                SelectOption::new("number", "Number"),
                SelectOption::new("integer", "Integer"),
                SelectOption::new("boolean", "Boolean"),
                SelectOption::new("select", "Select"),
                SelectOption::new("color", "Color"),
                SelectOption::new("file", "File"),
            ],
        )
        .with_default(Value::from("text")),
        FieldSpec::text("value", "Value"),
        FieldSpec::boolean("required", "Required").with_default(Value::Bool(false)),
        FieldSpec::number("min", "Minimum", NumericBounds::default()),
        FieldSpec::number("max", "Maximum", NumericBounds::default()),
        // `options` is the select-kind dropdown contents; the field
        // editor copies it through to `data-options` so the click router
        // can cycle without re-resolving the schema. Number/integer rows
        // ignore it.
        FieldSpec::text("options", "Select options (JSON array of {value,label})"),
        // §43 C2: doc-node-id the edit applies to. Populated by
        // `derive_property_rows`; consumed by the hit-test router.
        FieldSpec::text("target-id", "Target node ID"),
        // B4: set by the properties-panel binding when `state.field_focus`
        // matches this row's `target-id + key`. The lowering paints an
        // accent outline so the user sees which field is "live."
        FieldSpec::boolean("focused", "Currently focused").with_default(Value::Bool(false)),
    ]
}

fn field_editor_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "field-edited",
            "Edit committed (text/select/color/file/boolean) — payload (key, text).",
        ),
        SignalDef::new(
            "field-edited-number",
            "Numeric drag tick — payload (key, value).",
        ),
        SignalDef::new(
            "file-browse-requested",
            "User clicked the browse button on a `file`-kind row.",
        ),
    ])
}

fn field_editor_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let kind = prop_str(node, "kind");
    let entry = lookup_kind(kind);
    let label_text = label_with_required(node);

    let mut stack: Vec<UiNode> = Vec::with_capacity(2);
    if !label_text.is_empty() {
        stack.push(text_node(
            format!("{}::label", node.id),
            label_text.clone(),
            &StyleProperties::default(),
            LABEL_FONT_SIZE,
        ));
    }
    stack.extend((entry.body)(node));

    let focused = prop_bool(node, "focused", false);
    bare_container(node.id.clone(), stack, |props| {
        props.direction = Direction::Column;
        props.gap = VSTACK_GAP;
        props.padding = Padding {
            left: FIELD_PAD,
            right: FIELD_PAD,
            top: 6.0,
            bottom: 6.0,
        };
        if focused {
            // Soft tint behind the row so the user sees which field
            // is receiving keystrokes; cleared the moment focus moves
            // away or the user hits Esc / Enter.
            props.background = parse_color(FOCUS_RING_BG);
            props.radius = uniform_radius(FOCUS_RING_RADIUS);
        }
        // §43 C2 routing keys — the hit-test surface reads
        // `data-role="field-edit"` + `data-target-id` + `data-key` +
        // `data-kind` to route a pointer-down on this row into a
        // `BuilderService::set_node_prop` call. `data-value` carries
        // the current value so the boolean / select toggle paths can
        // flip it without an additional lookup.
        let mut s = Semantic::tag("div")
            .with_attr("role", entry.aria_role)
            .with_attr("data-role", "field-edit");
        let key = prop_str(node, "key");
        if !key.is_empty() {
            s = s.with_attr("data-key", key);
        }
        s = s.with_attr("data-kind", entry.kind);
        let target_id = prop_str(node, "target-id");
        if !target_id.is_empty() {
            s = s.with_attr("data-target-id", target_id);
        }
        let value = prop_str(node, "value");
        if !value.is_empty() {
            s = s.with_attr("data-value", value);
        }
        // Kind-specific extras the click-cycle router reads:
        // `data-options` carries the select values (comma-joined, since
        // attrs are flat strings), and `data-min` / `data-max` clamp the
        // number / integer step. Absent attrs default to "no clamp" /
        // "no cycle" in the router.
        if entry.kind == "select" {
            if let Some(options) = node.props.get("options").and_then(|v| v.as_array()) {
                let joined: Vec<String> = options
                    .iter()
                    .filter_map(|o| o.get("value").and_then(|v| v.as_str()).map(String::from))
                    .collect();
                if !joined.is_empty() {
                    s = s.with_attr("data-options", joined.join(","));
                }
            }
        }
        if entry.kind == "number" || entry.kind == "integer" {
            if let Some(min) = node.props.get("min").and_then(|v| v.as_f64()) {
                s = s.with_attr("data-min", min.to_string());
            }
            if let Some(max) = node.props.get("max").and_then(|v| v.as_f64()) {
                s = s.with_attr("data-max", max.to_string());
            }
        }
        if focused {
            s = s.with_attr("data-focused", "true");
        }
        props.semantic = s;
    })
}

pub const FIELD_EDITOR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.field-editor", field_editor_schema)
        .lower(field_editor_lower)
        .signals(field_editor_signals);

fn label_with_required(node: &Node) -> String {
    let label = prop_str(node, "label");
    if label.is_empty() {
        return String::new();
    }
    if prop_bool(node, "required", false) {
        format!("{label}{REQUIRED_MARK}")
    } else {
        label.into()
    }
}

// ── kind bodies ──────────────────────────────────────────────────────

fn build_boolean_body(node: &Node) -> Vec<UiNode> {
    let on = prop_str(node, "value") == "true";
    // Track + thumb. Thumb's left padding flips by state; this is the
    // declarative equivalent of the original Slint `Switch`.
    let track_bg = if on { SWITCH_ON_BG } else { SWITCH_OFF_BG };
    let thumb = bare_container(format!("{}::thumb", node.id), vec![], |p| {
        p.width = Sizing::Fixed(SWITCH_THUMB_SIZE);
        p.height = Sizing::Fixed(SWITCH_THUMB_SIZE);
        p.radius = uniform_radius(SWITCH_THUMB_SIZE / 2.0);
        p.background = parse_color(SWITCH_THUMB);
    });
    let track_pad_left = if on {
        SWITCH_WIDTH - SWITCH_THUMB_SIZE - 2.0
    } else {
        2.0
    };
    let track = bare_container(format!("{}::switch", node.id), vec![thumb], |p| {
        p.width = Sizing::Fixed(SWITCH_WIDTH);
        p.height = Sizing::Fixed(SWITCH_HEIGHT);
        p.radius = uniform_radius(SWITCH_RADIUS);
        p.background = parse_color(track_bg);
        p.padding = Padding {
            left: track_pad_left,
            right: 0.0,
            top: 2.0,
            bottom: 2.0,
        };
        p.semantic = Semantic::tag("input")
            .with_attr("type", "checkbox")
            .with_attr_if(on, "checked", "checked");
    });
    vec![track]
}

fn build_select_body(node: &Node) -> Vec<UiNode> {
    let value = prop_str(node, "value");
    vec![pill_with_chevron(
        format!("{}::select", node.id),
        value,
        false,
    )]
}

fn build_color_body(node: &Node) -> Vec<UiNode> {
    let value = prop_str(node, "value");
    let style = StyleProperties::default();

    let swatch = bare_container(format!("{}::swatch", node.id), vec![], |p| {
        p.width = Sizing::Fixed(SWATCH_SIZE);
        p.height = Sizing::Fixed(SWATCH_SIZE);
        p.radius = uniform_radius(SWATCH_RADIUS);
        p.background = parse_color(value);
        p.semantic = Semantic::tag("button")
            .with_attr("type", "button")
            .with_attr("data-role", "color-swatch");
    });

    let hex = text_input_node(
        format!("{}::hex", node.id),
        value.into(),
        "#000000".into(),
        &style,
        Sizing::Grow,
        Sizing::Fixed(PILL_HEIGHT),
        12.0,
    );

    vec![bare_container(
        format!("{}::color-row", node.id),
        vec![swatch, hex],
        |p| {
            p.direction = Direction::Row;
            p.gap = PICKER_GAP;
            p.height = Sizing::Fixed(PILL_HEIGHT);
        },
    )]
}

fn build_number_body(node: &Node) -> Vec<UiNode> {
    let key = prop_str(node, "key");
    let value = node
        .props
        .get("value")
        .and_then(|v| v.as_f64())
        .or_else(|| {
            node.props
                .get("value")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0.0);
    vec![drag_number_field_node(
        format!("{}::drag", node.id),
        "",
        DRAG_NUMBER_LABEL_COLOR,
        format_drag_value(value),
        key,
    )]
}

fn build_text_body(node: &Node) -> Vec<UiNode> {
    let value = prop_str(node, "value");
    let style = StyleProperties::default();
    vec![text_input_node(
        format!("{}::input", node.id),
        value.into(),
        String::new(),
        &style,
        Sizing::Grow,
        Sizing::Fixed(PILL_HEIGHT),
        12.0,
    )]
}

/// Reusable "pill with right-side chevron" shape — the dropdown
/// trigger for `select` and (in the original Slint) the anchor picker.
/// Lives here rather than in `chrome.rs` because it's currently a
/// single-consumer helper; promote when a second caller arrives.
fn pill_with_chevron(id: impl Into<String>, value: &str, open: bool) -> UiNode {
    let id = id.into();
    let style = StyleProperties::default();
    let label = colored_text_node(
        format!("{id}::label"),
        value.into(),
        &style,
        12.0,
        "#000000",
    );
    let chevron_glyph = if open {
        "icons/chevron-down.svg"
    } else {
        "icons/chevron-left.svg"
    };
    let chevron = prism_builder::ui_lower::image_node(
        format!("{id}::chevron"),
        chevron_glyph.into(),
        &style,
        Sizing::Fixed(10.0),
        Sizing::Fixed(10.0),
    );
    let row = bare_container(
        format!("{id}::row"),
        vec![
            bare_container(format!("{id}::label-cell"), vec![label], |p| {
                p.height = Sizing::Grow;
                p.width = Sizing::Grow;
            }),
            chevron,
        ],
        |p| {
            p.direction = Direction::Row;
            p.gap = 6.0;
            p.padding = Padding {
                left: 8.0,
                right: 8.0,
                top: 0.0,
                bottom: 0.0,
            };
            p.height = Sizing::Grow;
        },
    );
    bare_container(id, vec![row], |p| {
        p.height = Sizing::Fixed(PILL_HEIGHT);
        p.radius = uniform_radius(PILL_RADIUS);
        p.background = parse_color(PILL_BG);
        p.hover = hover_bg(PILL_HOVER_BG);
        p.semantic = Semantic::tag("button")
            .with_attr("type", "button")
            .with_attr_if(open, "aria-expanded", "true");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::Block;
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("fe", "shell.field-editor", props);
        lower_with(&n, field_editor_lower)
    }

    #[test]
    fn boolean_kind_lowers_to_label_plus_switch() {
        let ui = lower(json!({ "kind": "boolean", "label": "On", "value": "true" }));
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-kind" && v == "boolean"));
        assert_eq!(children.len(), 2, "label + switch");
    }

    #[test]
    fn select_kind_lowers_to_pill_with_chevron() {
        let ui = lower(json!({ "kind": "select", "label": "Mode", "value": "auto" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // label + pill
        assert_eq!(children.len(), 2);
        let UiNode::Container { props, .. } = &children[1] else {
            panic!("pill not a container")
        };
        assert_eq!(props.height, Sizing::Fixed(PILL_HEIGHT));
        assert_eq!(props.semantic.tag.as_deref(), Some("button"));
    }

    #[test]
    fn color_kind_includes_swatch_and_input() {
        let ui = lower(json!({ "kind": "color", "label": "Bg", "value": "#ff0000" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: row_kids, ..
        } = &children[1]
        else {
            panic!()
        };
        assert_eq!(row_kids.len(), 2, "swatch + hex input");
        let UiNode::Container { props, .. } = &row_kids[0] else {
            panic!()
        };
        assert!(props.background.is_some(), "swatch has color");
        assert!(matches!(&row_kids[1], UiNode::TextInput { .. }));
    }

    #[test]
    fn number_kind_uses_drag_field_helper() {
        let ui = lower(json!({ "kind": "number", "label": "X", "key": "x", "value": 3.5 }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // label + drag-number container
        let UiNode::Container { props, .. } = &children[1] else {
            panic!()
        };
        assert_eq!(
            props.height,
            Sizing::Fixed(super::super::chrome::DRAG_NUMBER_HEIGHT)
        );
    }

    #[test]
    fn unknown_kind_falls_through_to_text() {
        let ui = lower(json!({ "kind": "weird", "value": "hi" }));
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!()
        };
        // No label set, so just the body; semantic data-kind reflects fallback.
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-kind" && v == "text"));
        assert!(matches!(&children[0], UiNode::TextInput { .. }));
    }

    #[test]
    fn required_marks_label_with_asterisk() {
        let ui = lower(json!({ "kind": "text", "label": "Name", "required": true, "value": "" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert!(content.ends_with(REQUIRED_MARK));
    }

    #[test]
    fn schema_declares_ten_fields() {
        let block = prism_builder::SpecBlock::new(&super::FIELD_EDITOR_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(
            keys,
            vec![
                "key",
                "label",
                "kind",
                "value",
                "required",
                "min",
                "max",
                "options",
                "target-id",
                "focused",
            ]
        );
    }

    #[test]
    fn focused_prop_paints_focus_ring_and_data_focused_attr() {
        let ui = lower(json!({
            "kind": "text",
            "label": "Body",
            "value": "hello",
            "key": "body",
            "target-id": "demo-heading",
            "focused": true,
        }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props.background.is_some(), "focused row must paint a tint");
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-focused" && v == "true"));
    }

    #[test]
    fn lowered_field_carries_routing_attrs() {
        // §43 C2: every field-editor row exposes the keys the hit-test
        // surface needs (`data-role`, `data-target-id`, `data-key`,
        // `data-kind`, `data-value`).
        let ui = lower(json!({
            "kind": "boolean",
            "label": "Visible",
            "key": "visible",
            "value": "true",
            "target-id": "demo-heading",
        }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        let s = &props.semantic;
        assert!(s
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "field-edit"));
        assert!(s
            .attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "demo-heading"));
        assert!(s
            .attrs
            .iter()
            .any(|(k, v)| k == "data-key" && v == "visible"));
        assert!(s
            .attrs
            .iter()
            .any(|(k, v)| k == "data-kind" && v == "boolean"));
        assert!(s
            .attrs
            .iter()
            .any(|(k, v)| k == "data-value" && v == "true"));
    }

    #[test]
    fn signals_include_three_field_callbacks() {
        let block = prism_builder::SpecBlock::new(&super::FIELD_EDITOR_SPEC);
        let names: Vec<String> = block.signals().into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"field-edited".into()));
        assert!(names.contains(&"field-edited-number".into()));
        assert!(names.contains(&"file-browse-requested".into()));
    }
}
