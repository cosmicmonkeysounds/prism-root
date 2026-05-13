//! Wave 10 of `docs/dev/composable-builder-plan.md` — the
//! `PrimitiveRegistry`. Fourteen `BlockSpec`s declared here mirror the
//! shape of `starter::BUILTINS`: one row per primitive, each carrying
//! schema + lower + signals. The catalogue exists so:
//!
//! 1. Wave 11's `.prism-ui` self-hosting migration has a stable set of
//!    primitives to compose against (`<popover>`, `<list-picker>`,
//!    `<collapsible>`, `<split-handle>`, …).
//! 2. The inspector / properties panel can surface a primitive's prop
//!    schema like any other block — `Component::schema()` is the
//!    single seam.
//! 3. The Luau authoring surface (Wave 8) can register primitives the
//!    same way as components.
//!
//! Today's bodies are deliberately minimal — each primitive lowers to
//! a typed container with `data-role="<id>"` so the hit-test cache +
//! semantic emitter both have something to dispatch on. The full
//! interactive surface (text-input keystrokes, drag-scrub gestures,
//! popover positioning, color-picker HSL math, etc.) lands
//! incrementally as each primitive's first call site materialises —
//! see §11.5 of the plan.
//!
//! The schemas are the substantive part of this commit: an author
//! writing `<text-input value="…" placeholder="…"/>` against the
//! `prism-ui` parser sees the right field set today, and the
//! Inspector populates the matching prop rows on selection.

use prism_ui_runtime::command::CornerRadius;
use prism_ui_runtime::layout::{Node as UiNode, Semantic, Sizing, TextProps};
use serde_json::Value;

use crate::block::BlockSpec;
use crate::document::Node;
use crate::registry::{FieldSpec, NumericBounds};
use crate::style::StyleProperties;
use crate::ui_lower::{bare_container, LowerCtx};

// Every primitive's lower body shares the same shape: a single
// container with a `data-role` semantic attribute matching the
// primitive's id (minus the `prism.` namespace) so:
//
// - the hit-test cache can find it by role
// - the SSR / HTML backends can surface a typed tag
// - the inspector recognises the kind without a separate registry
//
// `primitive_lower(id, role)` is the shared constructor — adding a
// new primitive is one `BlockSpec` row, no per-primitive `lower_fn`
// boilerplate.
fn primitive_lower(
    role: &'static str,
) -> impl Fn(&LowerCtx<'_>, &Node, &StyleProperties) -> UiNode {
    move |_ctx, node, _style| {
        bare_container(node.id.clone(), Vec::new(), |p| {
            p.width = Sizing::Fit;
            p.height = Sizing::Fit;
            p.semantic = Semantic::tag("div")
                .with_attr("role", "group")
                .with_attr("data-role", role);
        })
    }
}

// Each primitive's `lower_fn` is a closure today. `BlockSpec` takes a
// `fn` pointer, not an `Fn`, so we hand-write a free function per
// primitive that calls the shared shape. The closures avoid the
// per-primitive impl-block boilerplate the explicit fn-shape would
// add (mirror of `synthetic_container` in `ui_lower::block`).

macro_rules! primitive_block {
    ($name:ident, $id:literal, $role:literal, $schema:ident) => {
        fn $name(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
            (primitive_lower($role))(ctx, node, style)
        }
    };
}

/// Wave 10.4 interactive body for `prism.text-input` — lowers to a
/// real `Node::TextInput` (not the generic `Container` placeholder)
/// with the `value` / `placeholder` props folded into the runtime
/// node + `data-role="text-input"` + an optional `data-bind-value`
/// semantic attr forwarded from the `bind` prop. When `bind` is set,
/// the shell's `route_bind_input_focus` opens a field-focus session
/// on pointer-down and the existing `FieldFocusService` routes
/// subsequent keystrokes into `set_node_prop` — closing the
/// Vue/Svelte `v-model` / `bind:value` two-way gap noted as Wave
/// 13.4 / 14.13 of `docs/dev/composable-builder-plan.md`.
///
/// The `disabled` prop adds `aria-disabled="true"`; the SSR walker
/// inherits both attrs verbatim. `multiline` is reserved (round-trips
/// as `data-multiline="true"` for the future textarea body) and
/// `max-length` rounds as `data-max-length` — both are author intent
/// carriers today, no runtime change.
fn text_input_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let value = ctx.prop_str(node, "value");
    let placeholder = ctx.prop_str(node, "placeholder");
    // Accept both `bind` (imperative / prefab author) and `bind-value`
    // (the DSL `bind:value="…"` form, forwarded by the resolver's
    // `AttributeNamespace::Bind` arm). Same destination data-attr.
    let bind = {
        let v = ctx.prop_str(node, "bind-value");
        if v.is_empty() {
            ctx.prop_str(node, "bind")
        } else {
            v
        }
    };
    let disabled = ctx.prop_bool(node, "disabled", false);
    let multiline = ctx.prop_bool(node, "multiline", false);
    let max_length = ctx.prop(node, "max-length");
    let mut semantic = Semantic::tag("input")
        .with_attr("type", "text")
        .with_attr("data-role", "text-input");
    if !bind.is_empty() {
        semantic = semantic.with_attr("data-bind-value", bind);
    }
    if !placeholder.is_empty() {
        semantic = semantic.with_attr("placeholder", placeholder.clone());
    }
    if disabled {
        semantic = semantic.with_attr("aria-disabled", "true");
    }
    if multiline {
        semantic = semantic.with_attr("data-multiline", "true");
    }
    if let Some(n) = max_length.as_f64() {
        if n > 0.0 {
            semantic = semantic.with_attr("data-max-length", (n as i64).to_string());
        }
    }
    UiNode::TextInput {
        id: node.id.clone(),
        value,
        placeholder,
        props: TextProps::default(),
        width: Sizing::Grow,
        height: Sizing::Fit,
        radius: CornerRadius {
            tl: 4.0,
            tr: 4.0,
            br: 4.0,
            bl: 4.0,
        },
        semantic,
        focused: false,
    }
}
primitive_block!(
    drag_scrub_lower,
    "prism.drag-scrub",
    "drag-scrub",
    drag_scrub_schema
);
primitive_block!(popover_lower, "prism.popover", "popover", popover_schema);
primitive_block!(
    list_picker_lower,
    "prism.list-picker",
    "list-picker",
    list_picker_schema
);
primitive_block!(
    collapsible_lower,
    "prism.collapsible",
    "collapsible",
    collapsible_schema
);
primitive_block!(
    split_handle_lower,
    "prism.split-handle",
    "split-handle",
    split_handle_schema
);
primitive_block!(
    timed_overlay_lower,
    "prism.timed-overlay",
    "timed-overlay",
    timed_overlay_schema
);
primitive_block!(
    focus_trap_lower,
    "prism.focus-trap",
    "focus-trap",
    focus_trap_schema
);
primitive_block!(select_lower, "prism.select", "select", select_schema);
primitive_block!(
    color_picker_lower,
    "prism.color-picker",
    "color-picker",
    color_picker_schema
);
primitive_block!(
    file_button_lower,
    "prism.file-button",
    "file-button",
    file_button_schema
);
primitive_block!(
    resize_edge_lower,
    "prism.resize-edge",
    "resize-edge",
    resize_edge_schema
);
primitive_block!(
    canvas_paint_lower,
    "prism.canvas-paint",
    "canvas-paint",
    canvas_paint_schema
);
primitive_block!(
    text_buffer_lower,
    "prism.text-buffer",
    "text-buffer",
    text_buffer_schema
);
/// Wave 11.4 — `<prism.builder-host/>` ships a real lower body, not
/// the shared `primitive_lower` stub. It takes the caller's
/// pre-lowered `host_children` (the `BuilderDocument` preview tree),
/// recursively annotates every container with `data-canvas-node="<id>"`,
/// the default hover tint, and (when the id matches `selection-id`)
/// a selection background. The output is one
/// `<container data-role="canvas-preview">` carrying the tagged tree.
/// Chrome (toolbar, grid overlay, selection outline, handles,
/// palette ghost) composes around it via the shell's
/// `shell.builder-canvas` DSL wrapper.
fn builder_host_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let selection_id_owned = ctx.prop_str(node, "selection-id");
    let selection_id = if selection_id_owned.is_empty() {
        None
    } else {
        Some(selection_id_owned.as_str())
    };
    let mut preview = ctx
        .host_children()
        .map(|s| s.to_vec())
        .unwrap_or_else(|| ctx.lower_children(&node.children));
    tag_canvas_subtree(&mut preview, selection_id);
    bare_container(node.id.clone(), preview, |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("div")
            .with_attr("role", "presentation")
            .with_attr("data-role", "canvas-preview");
    })
}

/// Resting hover tint painted on every canvas-document container so
/// the cursor passing over a preview node flashes the same
/// affordance every chrome surface uses.
const CANVAS_NODE_HOVER_BG: &str = "#1a0060c0";
/// Translucent blue tint painted as the background of the currently
/// selected canvas-document node.
const CANVAS_SELECTION_TINT: &str = "#330060c0";

/// Walk the host_children subtree (recursively) and apply the three
/// canvas-preview affordances:
///   1. `data-canvas-node="<id>"` on every container with a non-empty
///      id — the pointer-down router distinguishes canvas-doc clicks
///      from chrome clicks via this attr.
///   2. A default hover-bg tint when none was authored.
///   3. The selection background + `data-selected="true"` when the
///      container's id matches `selection_id`.
///
/// Mirrors the legacy `builder_canvas::tag_canvas_subtree` body
/// verbatim. Lifted here so the imperative walk lives with the
/// primitive that needs it, not the shell-side chrome.
fn tag_canvas_subtree(nodes: &mut [UiNode], selection_id: Option<&str>) {
    use crate::ui_lower::{hover_bg, parse_color};
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
                if props.hover.is_none() {
                    props.hover = hover_bg(CANVAS_NODE_HOVER_BG);
                }
                if selection_id == Some(id.as_str()) {
                    if let Some(c) = parse_color(CANVAS_SELECTION_TINT) {
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

// ── schemas ─────────────────────────────────────────────────────────

fn text_input_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("value", "Value").required(),
        FieldSpec::text("placeholder", "Placeholder"),
        // Wave 10.4: when set, lowers to `data-bind-value="..."` on
        // the emitted input so the shell's bind-input-focus route
        // opens a field-focus session on pointer-down and subsequent
        // keystrokes write back to the bound `<node-id>.<key>`.
        // Authors typically write `bind:value="form.email"` in DSL,
        // which already lowers to this same data-attr via the
        // `AttributeNamespace::Bind` arm — the prop here is the
        // imperative-author counterpart for prefab / facet builds.
        FieldSpec::text("bind", "Bind to"),
        FieldSpec::boolean("multiline", "Multi-line").with_default(Value::Bool(false)),
        FieldSpec::boolean("disabled", "Disabled").with_default(Value::Bool(false)),
        FieldSpec::number("max-length", "Max length", NumericBounds::min(0.0)),
    ]
}

fn drag_scrub_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::number("value", "Value", NumericBounds::default()),
        FieldSpec::number("step", "Drag step", NumericBounds::min(0.0))
            .with_default(Value::from(1.0)),
        FieldSpec::number("min", "Min", NumericBounds::default()),
        FieldSpec::number("max", "Max", NumericBounds::default()),
    ]
}

fn popover_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::boolean("open", "Open").with_default(Value::Bool(false)),
        FieldSpec::text("anchor-id", "Anchor element id"),
        FieldSpec::text("placement", "Placement (top|bottom|left|right)")
            .with_default(Value::from("bottom")),
    ]
}

fn list_picker_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("options", "Options (JSON array of {id, label})"),
        FieldSpec::text("selected-id", "Selected option id"),
    ]
}

fn collapsible_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::boolean("open", "Open").with_default(Value::Bool(true)),
        FieldSpec::text("title", "Section title"),
    ]
}

fn split_handle_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("orientation", "Orientation (horizontal|vertical)")
            .with_default(Value::from("horizontal")),
        FieldSpec::number(
            "position",
            "Position (0..1)",
            NumericBounds::min_max(0.0, 1.0),
        )
        .with_default(Value::from(0.5)),
    ]
}

fn timed_overlay_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::boolean("open", "Open").with_default(Value::Bool(false)),
        FieldSpec::number(
            "duration-ms",
            "Auto-dismiss duration (ms)",
            NumericBounds::min(0.0),
        )
        .with_default(Value::from(3000.0)),
    ]
}

fn focus_trap_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::boolean("active", "Active").with_default(Value::Bool(false))]
}

fn select_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("options", "Options (JSON array of {id, label})"),
        FieldSpec::text("value", "Selected id"),
        FieldSpec::text("placeholder", "Placeholder"),
    ]
}

fn color_picker_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("value", "Color (CSS hex or rgba)"),
        FieldSpec::boolean("alpha", "Alpha channel").with_default(Value::Bool(true)),
    ]
}

fn file_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("label", "Button label").with_default(Value::from("Browse…")),
        FieldSpec::text("accept", "Accept (MIME or extension list)"),
        FieldSpec::boolean("multiple", "Multi-select").with_default(Value::Bool(false)),
    ]
}

fn resize_edge_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("direction", "Edge direction (tl|t|tr|r|br|b|bl|l)").required(),
        FieldSpec::text("target-id", "Target node id"),
    ]
}

fn canvas_paint_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::number("width", "Width", NumericBounds::min(0.0)),
        FieldSpec::number("height", "Height", NumericBounds::min(0.0)),
        // Hook the host can read at lowering time to dispatch a
        // custom paint pass; today the renderer paints a placeholder
        // rect.
        FieldSpec::text("paint-handler", "Paint handler id"),
    ]
}

fn text_buffer_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("source", "Source text"),
        FieldSpec::text("language", "Language id"),
        FieldSpec::number("caret", "Caret (byte offset)", NumericBounds::min(0.0)),
    ]
}

/// Wave 11.4 — `<prism.builder-host/>` is the runtime surface that
/// hosts a builder document inside a `.prism-ui` shell. The shell's
/// `shell.builder-canvas` block is the consumer wrapper today; this
/// primitive lifts the abstraction so any `.prism-ui` document can
/// embed a builder canvas without depending on the shell's chrome
/// catalog. Props mirror the canvas's authored surface: a binding
/// path for the hosted `BuilderDocument`, a viewport hint, and a
/// `show-selection` toggle the inspector can wire to a selection
/// signal.
fn builder_host_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("document", "Document binding path").required(),
        FieldSpec::text("viewport", "Viewport hint (desktop|tablet|mobile|custom)")
            .with_default(Value::from("desktop")),
        FieldSpec::boolean("show-selection", "Show selection overlay")
            .with_default(Value::Bool(true)),
        FieldSpec::boolean("read-only", "Read-only").with_default(Value::Bool(false)),
        // Wave 11.4 — the id of the currently selected canvas
        // document node. The lower body walks host_children and
        // paints the selection tint on the container with this id.
        // Empty / unset → no selection paint (cursor still hovers
        // through the default tint).
        FieldSpec::text("selection-id", "Selected canvas-document node id"),
    ]
}

// ── specs ───────────────────────────────────────────────────────────

pub const TEXT_INPUT_SPEC: BlockSpec =
    BlockSpec::new("prism.text-input", text_input_schema).lower(text_input_lower);
pub const DRAG_SCRUB_SPEC: BlockSpec =
    BlockSpec::new("prism.drag-scrub", drag_scrub_schema).lower(drag_scrub_lower);
pub const POPOVER_SPEC: BlockSpec =
    BlockSpec::new("prism.popover", popover_schema).lower(popover_lower);
pub const LIST_PICKER_SPEC: BlockSpec =
    BlockSpec::new("prism.list-picker", list_picker_schema).lower(list_picker_lower);
pub const COLLAPSIBLE_SPEC: BlockSpec =
    BlockSpec::new("prism.collapsible", collapsible_schema).lower(collapsible_lower);
pub const SPLIT_HANDLE_SPEC: BlockSpec =
    BlockSpec::new("prism.split-handle", split_handle_schema).lower(split_handle_lower);
pub const TIMED_OVERLAY_SPEC: BlockSpec =
    BlockSpec::new("prism.timed-overlay", timed_overlay_schema).lower(timed_overlay_lower);
pub const FOCUS_TRAP_SPEC: BlockSpec =
    BlockSpec::new("prism.focus-trap", focus_trap_schema).lower(focus_trap_lower);
pub const SELECT_SPEC: BlockSpec =
    BlockSpec::new("prism.select", select_schema).lower(select_lower);
pub const COLOR_PICKER_SPEC: BlockSpec =
    BlockSpec::new("prism.color-picker", color_picker_schema).lower(color_picker_lower);
pub const FILE_BUTTON_SPEC: BlockSpec =
    BlockSpec::new("prism.file-button", file_button_schema).lower(file_button_lower);
pub const RESIZE_EDGE_SPEC: BlockSpec =
    BlockSpec::new("prism.resize-edge", resize_edge_schema).lower(resize_edge_lower);
pub const CANVAS_PAINT_SPEC: BlockSpec =
    BlockSpec::new("prism.canvas-paint", canvas_paint_schema).lower(canvas_paint_lower);
pub const TEXT_BUFFER_SPEC: BlockSpec =
    BlockSpec::new("prism.text-buffer", text_buffer_schema).lower(text_buffer_lower);
/// Wave 11.4 — Tier-3 primitive that hosts a builder document. The
/// shell's `shell.builder-canvas` block is the chrome wrapper that
/// composes against it.
pub const BUILDER_HOST_SPEC: BlockSpec =
    BlockSpec::new("prism.builder-host", builder_host_schema).lower(builder_host_lower);

/// The 15-row primitive catalogue — Waves 10 + 11.4 of the plan. The
/// list is closed and ordered for stable iteration (snapshot tests,
/// name completion). Each entry is referenced by name through the
/// `prism.<id>` tag in `.prism-ui` source.
pub const PRIMITIVES: &[&BlockSpec] = &[
    &TEXT_INPUT_SPEC,
    &DRAG_SCRUB_SPEC,
    &POPOVER_SPEC,
    &LIST_PICKER_SPEC,
    &COLLAPSIBLE_SPEC,
    &SPLIT_HANDLE_SPEC,
    &TIMED_OVERLAY_SPEC,
    &FOCUS_TRAP_SPEC,
    &SELECT_SPEC,
    &COLOR_PICKER_SPEC,
    &FILE_BUTTON_SPEC,
    &RESIZE_EDGE_SPEC,
    &CANVAS_PAINT_SPEC,
    &TEXT_BUFFER_SPEC,
    &BUILDER_HOST_SPEC,
];

/// One-line fan-out — register every primitive into a `ComponentRegistry`.
/// Wave 10 hooks this into both the shell's `ShellComponentRegistry`
/// (so the inspector / picker surface them) and the builder's
/// document registry (so authored documents resolve `prism.<id>`
/// tags through the same `lower_template` dispatch).
pub fn register_primitives(
    registry: &mut crate::ComponentRegistry,
) -> Result<(), crate::RegistryError> {
    crate::block::register_specs(registry, PRIMITIVES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentRegistry;

    #[test]
    fn primitive_count_is_fifteen() {
        // Wave 10 listed 14 primitives; Wave 11.4 added
        // `prism.builder-host`. The catalogue is a closed set;
        // further changes need a plan update.
        assert_eq!(PRIMITIVES.len(), 15);
    }

    #[test]
    fn builder_host_primitive_is_registered_with_required_document_field() {
        let mut reg = ComponentRegistry::new();
        register_primitives(&mut reg).expect("register");
        let comp = reg
            .get("prism.builder-host")
            .expect("builder-host registered");
        let schema = comp.schema();
        let document_field = schema
            .iter()
            .find(|f| f.key == "document")
            .expect("document field present");
        assert!(
            document_field.required,
            "the `document` field must be required so the inspector flags missing bindings"
        );
    }

    #[test]
    fn every_primitive_id_starts_with_prism_namespace() {
        for spec in PRIMITIVES {
            assert!(
                spec.id.starts_with("prism."),
                "primitive id `{}` must be in the `prism.` namespace",
                spec.id
            );
        }
    }

    #[test]
    fn primitive_ids_are_unique() {
        let mut ids: Vec<&str> = PRIMITIVES.iter().map(|s| s.id).collect();
        let len = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), len, "duplicate primitive ids");
    }

    #[test]
    fn register_primitives_lands_all_specs_in_the_registry() {
        let mut reg = ComponentRegistry::new();
        register_primitives(&mut reg).expect("register");
        for spec in PRIMITIVES {
            assert!(
                reg.get(spec.id).is_some(),
                "expected `{}` in registry after register_primitives",
                spec.id
            );
        }
    }

    #[test]
    fn primitive_lower_emits_data_role_matching_tag_local_part() {
        let mut reg = ComponentRegistry::new();
        register_primitives(&mut reg).expect("register");
        let node = Node {
            id: "p1".into(),
            component: "prism.popover".into(),
            ..Default::default()
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(&reg), &cascade);
        let UiNode::Container { props, .. } = popover_lower(&ctx, &node, &cascade) else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "popover"));
    }

    /// Wave 10.4: `prism.text-input` lowers to a real `TextInput`
    /// node carrying every prop the schema declares. The `bind`
    /// prop forwards as `data-bind-value` so the shell's
    /// `route_bind_input_focus` opens a field-focus session on
    /// pointer-down (closing the two-way `bind:value` deferral).
    #[test]
    fn text_input_lower_emits_typed_node_with_props_folded_in() {
        let node = Node {
            id: "in1".into(),
            component: "prism.text-input".into(),
            props: serde_json::json!({
                "value": "hello",
                "placeholder": "type…",
                "bind": "form.email",
                "disabled": true,
                "max-length": 80,
            }),
            ..Default::default()
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        let lowered = text_input_lower(&ctx, &node, &cascade);
        let UiNode::TextInput {
            id,
            value,
            placeholder,
            semantic,
            ..
        } = lowered
        else {
            panic!("expected TextInput, got {lowered:?}");
        };
        assert_eq!(id, "in1");
        assert_eq!(value, "hello");
        assert_eq!(placeholder, "type…");
        let attr = |k: &str| {
            semantic
                .attrs
                .iter()
                .find_map(|(name, v)| (name == k).then(|| v.clone()))
        };
        assert_eq!(attr("data-role"), Some("text-input".into()));
        assert_eq!(attr("data-bind-value"), Some("form.email".into()));
        assert_eq!(attr("aria-disabled"), Some("true".into()));
        assert_eq!(attr("data-max-length"), Some("80".into()));
        assert_eq!(attr("placeholder"), Some("type…".into()));
    }

    #[test]
    fn text_input_lower_skips_optional_attrs_when_unset() {
        let node = Node {
            id: "in1".into(),
            component: "prism.text-input".into(),
            props: serde_json::json!({ "value": "" }),
            ..Default::default()
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        let UiNode::TextInput { semantic, .. } = text_input_lower(&ctx, &node, &cascade) else {
            panic!();
        };
        let has = |k: &str| semantic.attrs.iter().any(|(n, _)| n == k);
        assert!(has("data-role"));
        assert!(!has("data-bind-value"));
        assert!(!has("aria-disabled"));
        assert!(!has("data-max-length"));
        assert!(!has("placeholder"));
    }
}
