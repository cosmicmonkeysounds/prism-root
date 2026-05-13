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

// Wave 10.4-10.17 — every primitive ships a real `lower_fn` body
// (see the per-fn bodies below). The pre-Wave-10.4 shared
// `primitive_lower(role)` stub + `primitive_block!` macro retired
// once each primitive had something more meaningful than a bare
// `<div data-role="…"/>` to emit.

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
/// Wave 10.5 — `prism.drag-scrub`. The runtime emits a styled
/// container carrying `data-role="drag-scrub"` plus `data-value`
/// (current), `data-step`, `data-min` / `data-max` (when bounded),
/// and `data-bind-value` (when bound). The shell's pointer-routing
/// table recognises `drag-scrub` and routes drag deltas through
/// `nudge_focused_number` / `set_node_prop`. Multi-line scrubbing,
/// keyboard arrow handling, and shift-modifier coarse step all flow
/// through the existing `FieldFocusService` once focused.
fn drag_scrub_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let value = ctx.prop(node, "value");
    let step = ctx.prop(node, "step");
    let min = ctx.prop(node, "min");
    let max = ctx.prop(node, "max");
    let bind = {
        let v = ctx.prop_str(node, "bind-value");
        if v.is_empty() {
            ctx.prop_str(node, "bind")
        } else {
            v
        }
    };
    bare_container(node.id.clone(), Vec::new(), |p| {
        p.width = Sizing::Fit;
        p.height = Sizing::Fit;
        let mut s = Semantic::tag("div")
            .with_attr("role", "slider")
            .with_attr("data-role", "drag-scrub");
        if let Some(n) = value.as_f64() {
            s = s.with_attr("data-value", format_number(n));
        }
        if let Some(n) = step.as_f64() {
            s = s.with_attr("data-step", format_number(n));
        }
        if let Some(n) = min.as_f64() {
            s = s.with_attr("data-min", format_number(n));
            s = s.with_attr("aria-valuemin", format_number(n));
        }
        if let Some(n) = max.as_f64() {
            s = s.with_attr("data-max", format_number(n));
            s = s.with_attr("aria-valuemax", format_number(n));
        }
        if !bind.is_empty() {
            s = s.with_attr("data-bind-value", bind);
        }
        p.semantic = s;
    })
}

/// Wave 10.6 — `prism.popover`. Anchored overlay: when `open=true`,
/// emits the body with `data-role="popover"`, `data-placement` from
/// the prop, and `data-anchor-id` if set. The host paints the
/// arrow + positions the rect relative to the anchor's hit-test rect
/// at the next frame. When closed, the body collapses to a 0×0
/// `aria-hidden="true"` placeholder (matching the established
/// closed-overlay shape from §43 A2).
fn popover_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let open = ctx.prop_bool(node, "open", false);
    let anchor_id = ctx.prop_str(node, "anchor-id");
    let placement = {
        let raw = ctx.prop_str(node, "placement");
        if raw.is_empty() {
            "bottom".to_string()
        } else {
            raw
        }
    };
    let children = ctx.lower_children(&node.children);
    bare_container(node.id.clone(), children, |p| {
        if open {
            p.width = Sizing::Fit;
            p.height = Sizing::Fit;
        } else {
            p.width = Sizing::Fixed(0.0);
            p.height = Sizing::Fixed(0.0);
        }
        let mut s = Semantic::tag("div")
            .with_attr("role", "dialog")
            .with_attr("data-role", "popover")
            .with_attr("data-placement", placement)
            .with_attr("data-open", if open { "true" } else { "false" });
        if !anchor_id.is_empty() {
            s = s.with_attr("data-anchor-id", anchor_id);
        }
        if !open {
            s = s.with_attr("aria-hidden", "true");
        }
        p.semantic = s;
    })
}

/// Wave 10.7 — `prism.list-picker`. Emits a `data-role="list-picker"`
/// container with `data-options` (the raw JSON array string) +
/// `data-selected-id` so the shell's list-picker route can paint
/// rows + dispatch row-click. Children pass through verbatim so an
/// author can decorate the picker with a header / footer.
fn list_picker_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let options = ctx.prop(node, "options");
    let selected_id = ctx.prop_str(node, "selected-id");
    let children = ctx.lower_children(&node.children);
    bare_container(node.id.clone(), children, |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Fit;
        let mut s = Semantic::tag("div")
            .with_attr("role", "listbox")
            .with_attr("data-role", "list-picker");
        let opts_str = match &options {
            Value::String(s) if !s.is_empty() => s.clone(),
            Value::Array(_) => options.to_string(),
            _ => String::new(),
        };
        if !opts_str.is_empty() {
            s = s.with_attr("data-options", opts_str);
        }
        if !selected_id.is_empty() {
            s = s.with_attr("data-selected-id", selected_id);
            s = s.with_attr("aria-activedescendant", String::new());
        }
        p.semantic = s;
    })
}

/// Wave 10.8 — `prism.collapsible`. When `open=true`, children are
/// emitted; when closed, the body collapses to a header-only stub
/// with `data-open="false"`. The `data-role="collapsible"` lets a
/// host route header clicks to a toggle handler. The current title
/// rides as `aria-label` so screen readers carry the disclosure
/// identity even when the body is hidden.
fn collapsible_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let open = ctx.prop_bool(node, "open", true);
    let title = ctx.prop_str(node, "title");
    let children = if open {
        ctx.lower_children(&node.children)
    } else {
        Vec::new()
    };
    bare_container(node.id.clone(), children, |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Fit;
        let mut s = Semantic::tag("div")
            .with_attr("role", "group")
            .with_attr("data-role", "collapsible")
            .with_attr("data-open", if open { "true" } else { "false" })
            .with_attr("aria-expanded", if open { "true" } else { "false" });
        if !title.is_empty() {
            s = s.with_attr("aria-label", title);
        }
        p.semantic = s;
    })
}

/// Wave 10.9 — `prism.split-handle`. Emits a thin draggable strip
/// carrying `data-role="split-handle"` + `data-orientation` +
/// `data-position` (0..1 fraction). The shell routes drag deltas
/// through a panel-resize service so adjacent panes adopt the new
/// split ratio.
fn split_handle_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let orientation = {
        let raw = ctx.prop_str(node, "orientation");
        if raw.is_empty() {
            "horizontal".to_string()
        } else {
            raw
        }
    };
    let position = ctx.prop(node, "position").as_f64().unwrap_or(0.5);
    let bind = {
        let v = ctx.prop_str(node, "bind-value");
        if v.is_empty() {
            ctx.prop_str(node, "bind")
        } else {
            v
        }
    };
    bare_container(node.id.clone(), Vec::new(), |p| {
        if orientation == "vertical" {
            p.width = Sizing::Fixed(6.0);
            p.height = Sizing::Grow;
        } else {
            p.width = Sizing::Grow;
            p.height = Sizing::Fixed(6.0);
        }
        let mut s = Semantic::tag("div")
            .with_attr("role", "separator")
            .with_attr("data-role", "split-handle")
            .with_attr("data-orientation", orientation)
            .with_attr("data-position", format_number(position));
        if !bind.is_empty() {
            s = s.with_attr("data-bind-value", bind);
        }
        p.semantic = s;
    })
}

/// Wave 10.10 — `prism.timed-overlay`. Toast-style overlay that lives
/// for `duration-ms` then dismisses itself. The runtime body just
/// emits the data + a hidden/visible container; the auto-dismiss
/// timer is driven by an `Effect` an author wires up alongside the
/// usage site. Same `open` semantics as `popover`.
fn timed_overlay_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let open = ctx.prop_bool(node, "open", false);
    let duration = ctx.prop(node, "duration-ms").as_f64().unwrap_or(3000.0);
    let children = if open {
        ctx.lower_children(&node.children)
    } else {
        Vec::new()
    };
    bare_container(node.id.clone(), children, |p| {
        if open {
            p.width = Sizing::Fit;
            p.height = Sizing::Fit;
        } else {
            p.width = Sizing::Fixed(0.0);
            p.height = Sizing::Fixed(0.0);
        }
        let mut s = Semantic::tag("div")
            .with_attr("role", "status")
            .with_attr("data-role", "timed-overlay")
            .with_attr("data-open", if open { "true" } else { "false" })
            .with_attr("data-duration-ms", format_number(duration));
        if !open {
            s = s.with_attr("aria-hidden", "true");
        }
        p.semantic = s;
    })
}

/// Wave 10.11 — `prism.focus-trap`. Marks a subtree as a focus trap
/// boundary. The shell's focus-routing service keeps tab order within
/// the children when `active=true`. Lowers transparently — children
/// pass through; only the semantic attrs change.
fn focus_trap_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let active = ctx.prop_bool(node, "active", false);
    let children = ctx.lower_children(&node.children);
    bare_container(node.id.clone(), children, |p| {
        p.width = Sizing::Fit;
        p.height = Sizing::Fit;
        p.semantic = Semantic::tag("div")
            .with_attr("role", "group")
            .with_attr("data-role", "focus-trap")
            .with_attr("data-active", if active { "true" } else { "false" });
    })
}

/// Wave 10.12 — `prism.select`. Emits a `<select>` shape with
/// `data-options` (JSON array string) + `data-value` (current) +
/// `data-bind-value` (when bound). The shell routes click to cycle
/// or open an anchored list-picker overlay (already wired).
fn select_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let options = ctx.prop(node, "options");
    let value = ctx.prop_str(node, "value");
    let placeholder = ctx.prop_str(node, "placeholder");
    let bind = {
        let v = ctx.prop_str(node, "bind-value");
        if v.is_empty() {
            ctx.prop_str(node, "bind")
        } else {
            v
        }
    };
    bare_container(node.id.clone(), Vec::new(), |p| {
        p.width = Sizing::Fit;
        p.height = Sizing::Fit;
        let mut s = Semantic::tag("select")
            .with_attr("role", "combobox")
            .with_attr("data-role", "select");
        let opts_str = match &options {
            Value::String(s) if !s.is_empty() => s.clone(),
            Value::Array(_) => options.to_string(),
            _ => String::new(),
        };
        if !opts_str.is_empty() {
            s = s.with_attr("data-options", opts_str);
        }
        if !value.is_empty() {
            s = s.with_attr("data-value", value);
        }
        if !placeholder.is_empty() {
            s = s.with_attr("placeholder", placeholder);
        }
        if !bind.is_empty() {
            s = s.with_attr("data-bind-value", bind);
        }
        p.semantic = s;
    })
}

/// Wave 10.13 — `prism.color-picker`. Emits a swatch + hex carrier
/// with `data-role="color-picker"`, `data-value` (the CSS hex/rgba),
/// `data-alpha` flag, and `data-bind-value` when bound. The shell's
/// color-picker overlay (already wired for `shell.color-picker`)
/// recognises the role and presents the HSL slider rig.
fn color_picker_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let value = ctx.prop_str(node, "value");
    let alpha = ctx.prop_bool(node, "alpha", true);
    let bind = {
        let v = ctx.prop_str(node, "bind-value");
        if v.is_empty() {
            ctx.prop_str(node, "bind")
        } else {
            v
        }
    };
    bare_container(node.id.clone(), Vec::new(), |p| {
        p.width = Sizing::Fit;
        p.height = Sizing::Fit;
        let mut s = Semantic::tag("div")
            .with_attr("role", "button")
            .with_attr("data-role", "color-picker")
            .with_attr("data-alpha", if alpha { "true" } else { "false" });
        if !value.is_empty() {
            s = s.with_attr("data-value", value);
        }
        if !bind.is_empty() {
            s = s.with_attr("data-bind-value", bind);
        }
        p.semantic = s;
    })
}

/// Wave 10.14 — `prism.file-button`. Emits a button labelled by
/// `label`, with `data-role="file-button"`, `data-accept` (MIME
/// filter), `data-multiple`. The shell's pointer-down routes it
/// through `handle_file_browse_click` which calls `Vfs::pick_file`
/// (already wired for the field-editor's File kind).
fn file_button_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let label = {
        let raw = ctx.prop_str(node, "label");
        if raw.is_empty() {
            "Browse…".to_string()
        } else {
            raw
        }
    };
    let accept = ctx.prop_str(node, "accept");
    let multiple = ctx.prop_bool(node, "multiple", false);
    let bind = {
        let v = ctx.prop_str(node, "bind-value");
        if v.is_empty() {
            ctx.prop_str(node, "bind")
        } else {
            v
        }
    };
    bare_container(node.id.clone(), Vec::new(), |p| {
        p.width = Sizing::Fit;
        p.height = Sizing::Fit;
        let mut s = Semantic::tag("button")
            .with_attr("role", "button")
            .with_attr("data-role", "file-button")
            .with_attr("data-label", label);
        if !accept.is_empty() {
            s = s.with_attr("data-accept", accept);
        }
        if multiple {
            s = s.with_attr("data-multiple", "true");
        }
        if !bind.is_empty() {
            s = s.with_attr("data-bind-value", bind);
        }
        p.semantic = s;
    })
}

/// Wave 10.15 — `prism.resize-edge`. Emits a thin grab-handle with
/// `data-role="resize-edge"` + `data-direction` (whitelisted to
/// 8 octants) + `data-target-id`. Already wired into the shell's
/// `POINTER_ROUTES("resize-handle")` via the existing canvas gizmo
/// routing — Wave 10.15 adds the primitive-level surface so DSL
/// authors can compose resize affordances without hand-rolling the
/// data ladder.
fn resize_edge_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let direction = ctx.prop_str(node, "direction");
    let target_id = ctx.prop_str(node, "target-id");
    bare_container(node.id.clone(), Vec::new(), |p| {
        p.width = Sizing::Fit;
        p.height = Sizing::Fit;
        let mut s = Semantic::tag("div")
            .with_attr("role", "separator")
            .with_attr("data-role", "resize-handle")
            .with_attr("data-direction", direction);
        if !target_id.is_empty() {
            s = s.with_attr("data-target-id", target_id);
        }
        p.semantic = s;
    })
}

/// Wave 10.16 — `prism.canvas-paint`. Reserves a fixed-size paintable
/// rect. The host wires up an `Effect` that calls back into a custom
/// paint pass keyed off `data-canvas-paint-id` (matches the node's
/// id). Used for color-picker HSL gradient strips, gizmo arrow
/// arrowheads, etc.
fn canvas_paint_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let w = ctx.prop(node, "width").as_f64().unwrap_or(0.0);
    let h = ctx.prop(node, "height").as_f64().unwrap_or(0.0);
    bare_container(node.id.clone(), Vec::new(), |p| {
        if w > 0.0 {
            p.width = Sizing::Fixed(w as f32);
        }
        if h > 0.0 {
            p.height = Sizing::Fixed(h as f32);
        }
        let mut s = Semantic::tag("canvas")
            .with_attr("role", "img")
            .with_attr("data-role", "canvas-paint");
        if !node.id.is_empty() {
            s = s.with_attr("data-canvas-paint-id", node.id.clone());
        }
        if w > 0.0 {
            s = s.with_attr("width", format_number(w));
        }
        if h > 0.0 {
            s = s.with_attr("height", format_number(h));
        }
        p.semantic = s;
    })
}

/// Wave 10.17 — `prism.text-buffer`. Multi-line text buffer with line
/// and caret reporting. The runtime currently lowers it as a
/// `TextInput` with `multiline=true` so keystroke routing reuses the
/// single-line path; richer features (selection, IME, scroll) land
/// alongside the future textarea body.
fn text_buffer_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let value = ctx.prop_str(node, "value");
    let placeholder = ctx.prop_str(node, "placeholder");
    let bind = {
        let v = ctx.prop_str(node, "bind-value");
        if v.is_empty() {
            ctx.prop_str(node, "bind")
        } else {
            v
        }
    };
    let mut semantic = Semantic::tag("textarea")
        .with_attr("data-role", "text-buffer")
        .with_attr("data-multiline", "true");
    if !bind.is_empty() {
        semantic = semantic.with_attr("data-bind-value", bind);
    }
    if !placeholder.is_empty() {
        semantic = semantic.with_attr("placeholder", placeholder.clone());
    }
    UiNode::TextInput {
        id: node.id.clone(),
        value,
        placeholder,
        props: TextProps::default(),
        width: Sizing::Grow,
        height: Sizing::Grow,
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

/// Format an `f64` for round-trip into a semantic attribute value.
/// Integer-valued doubles serialise without a trailing `.0` so authors
/// reading `data-step="1"` don't see `data-step="1.0"`.
fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e16 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
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

    fn lower(
        component: &str,
        lower_fn: fn(&LowerCtx, &Node, &StyleProperties) -> UiNode,
        props: serde_json::Value,
    ) -> UiNode {
        let node = Node {
            id: "n1".into(),
            component: component.into(),
            props,
            ..Default::default()
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        lower_fn(&ctx, &node, &cascade)
    }

    fn semantic_attr(node: &UiNode, key: &str) -> Option<String> {
        match node {
            UiNode::Container { props, .. } => props
                .semantic
                .attrs
                .iter()
                .find_map(|(k, v)| (k == key).then(|| v.clone())),
            UiNode::TextInput { semantic, .. } => semantic
                .attrs
                .iter()
                .find_map(|(k, v)| (k == key).then(|| v.clone())),
            _ => None,
        }
    }

    /// Wave 10.5 — drag-scrub fold of value / step / min / max + bind.
    #[test]
    fn drag_scrub_lower_emits_value_step_bounds_and_bind() {
        let n = lower(
            "prism.drag-scrub",
            drag_scrub_lower,
            serde_json::json!({
                "value": 5.5, "step": 0.25, "min": 0, "max": 10, "bind-value": "form.amount",
            }),
        );
        assert_eq!(
            semantic_attr(&n, "data-role").as_deref(),
            Some("drag-scrub")
        );
        assert_eq!(semantic_attr(&n, "data-value").as_deref(), Some("5.5"));
        assert_eq!(semantic_attr(&n, "data-step").as_deref(), Some("0.25"));
        assert_eq!(semantic_attr(&n, "data-min").as_deref(), Some("0"));
        assert_eq!(semantic_attr(&n, "data-max").as_deref(), Some("10"));
        assert_eq!(semantic_attr(&n, "aria-valuemin").as_deref(), Some("0"));
        assert_eq!(
            semantic_attr(&n, "data-bind-value").as_deref(),
            Some("form.amount")
        );
    }

    /// Wave 10.6 — popover collapses when closed, expands when open.
    #[test]
    fn popover_lower_open_emits_data_attrs() {
        let n = lower(
            "prism.popover",
            popover_lower,
            serde_json::json!({
                "open": true, "anchor-id": "swatch-1", "placement": "top",
            }),
        );
        assert_eq!(semantic_attr(&n, "data-role").as_deref(), Some("popover"));
        assert_eq!(semantic_attr(&n, "data-open").as_deref(), Some("true"));
        assert_eq!(semantic_attr(&n, "data-placement").as_deref(), Some("top"));
        assert_eq!(
            semantic_attr(&n, "data-anchor-id").as_deref(),
            Some("swatch-1")
        );
        assert!(semantic_attr(&n, "aria-hidden").is_none());
    }

    #[test]
    fn popover_lower_closed_collapses_and_marks_hidden() {
        let n = lower(
            "prism.popover",
            popover_lower,
            serde_json::json!({ "open": false }),
        );
        let UiNode::Container { props, .. } = &n else {
            panic!()
        };
        assert!(matches!(props.width, Sizing::Fixed(0.0)));
        assert_eq!(semantic_attr(&n, "data-open").as_deref(), Some("false"));
        assert_eq!(semantic_attr(&n, "aria-hidden").as_deref(), Some("true"));
    }

    /// Wave 10.7 — list-picker carries options + selected-id.
    #[test]
    fn list_picker_lower_emits_options_and_selected_id() {
        let n = lower(
            "prism.list-picker",
            list_picker_lower,
            serde_json::json!({
                "options": [{"id": "a", "label": "A"}, {"id": "b", "label": "B"}],
                "selected-id": "b",
            }),
        );
        assert_eq!(
            semantic_attr(&n, "data-role").as_deref(),
            Some("list-picker")
        );
        assert_eq!(semantic_attr(&n, "data-selected-id").as_deref(), Some("b"));
        assert!(semantic_attr(&n, "data-options")
            .unwrap()
            .contains("\"id\":\"a\""));
    }

    /// Wave 10.8 — collapsible swallows children when closed.
    #[test]
    fn collapsible_lower_open_includes_aria_attrs() {
        let n = lower(
            "prism.collapsible",
            collapsible_lower,
            serde_json::json!({
                "open": true, "title": "Details",
            }),
        );
        assert_eq!(semantic_attr(&n, "data-open").as_deref(), Some("true"));
        assert_eq!(semantic_attr(&n, "aria-expanded").as_deref(), Some("true"));
        assert_eq!(semantic_attr(&n, "aria-label").as_deref(), Some("Details"));
    }

    /// Wave 10.9 — split-handle orientation toggles sizing axes.
    #[test]
    fn split_handle_lower_vertical_picks_fixed_width_and_grow_height() {
        let n = lower(
            "prism.split-handle",
            split_handle_lower,
            serde_json::json!({
                "orientation": "vertical", "position": 0.6,
            }),
        );
        let UiNode::Container { props, .. } = &n else {
            panic!()
        };
        assert!(matches!(props.width, Sizing::Fixed(6.0)));
        assert!(matches!(props.height, Sizing::Grow));
        assert_eq!(
            semantic_attr(&n, "data-orientation").as_deref(),
            Some("vertical")
        );
        assert_eq!(semantic_attr(&n, "data-position").as_deref(), Some("0.6"));
    }

    /// Wave 10.12 — select carries options + value + bind.
    #[test]
    fn select_lower_emits_combobox_data_attrs() {
        let n = lower(
            "prism.select",
            select_lower,
            serde_json::json!({
                "options": [{"id": "x", "label": "X"}],
                "value": "x",
                "bind-value": "form.choice",
            }),
        );
        assert_eq!(semantic_attr(&n, "data-role").as_deref(), Some("select"));
        assert_eq!(semantic_attr(&n, "data-value").as_deref(), Some("x"));
        assert_eq!(
            semantic_attr(&n, "data-bind-value").as_deref(),
            Some("form.choice")
        );
    }

    /// Wave 10.13 — color-picker swatch carries value + alpha flag.
    #[test]
    fn color_picker_lower_emits_value_and_alpha() {
        let n = lower(
            "prism.color-picker",
            color_picker_lower,
            serde_json::json!({
                "value": "#ff0080", "alpha": false,
            }),
        );
        assert_eq!(
            semantic_attr(&n, "data-role").as_deref(),
            Some("color-picker")
        );
        assert_eq!(semantic_attr(&n, "data-value").as_deref(), Some("#ff0080"));
        assert_eq!(semantic_attr(&n, "data-alpha").as_deref(), Some("false"));
    }

    /// Wave 10.14 — file-button has default label + carries accept.
    #[test]
    fn file_button_lower_emits_default_label_and_accept() {
        let n = lower(
            "prism.file-button",
            file_button_lower,
            serde_json::json!({
                "accept": "image/png", "multiple": true,
            }),
        );
        assert_eq!(
            semantic_attr(&n, "data-role").as_deref(),
            Some("file-button")
        );
        assert_eq!(semantic_attr(&n, "data-label").as_deref(), Some("Browse…"));
        assert_eq!(
            semantic_attr(&n, "data-accept").as_deref(),
            Some("image/png")
        );
        assert_eq!(semantic_attr(&n, "data-multiple").as_deref(), Some("true"));
    }

    /// Wave 10.15 — resize-edge carries direction + target-id.
    #[test]
    fn resize_edge_lower_emits_direction_and_target() {
        let n = lower(
            "prism.resize-edge",
            resize_edge_lower,
            serde_json::json!({
                "direction": "br", "target-id": "n42",
            }),
        );
        assert_eq!(
            semantic_attr(&n, "data-role").as_deref(),
            Some("resize-handle")
        );
        assert_eq!(semantic_attr(&n, "data-direction").as_deref(), Some("br"));
        assert_eq!(semantic_attr(&n, "data-target-id").as_deref(), Some("n42"));
    }

    /// Wave 10.16 — canvas-paint reserves a fixed-size rect carrying
    /// the node id so paint callbacks can target it.
    #[test]
    fn canvas_paint_lower_emits_canvas_id_and_dimensions() {
        let n = lower(
            "prism.canvas-paint",
            canvas_paint_lower,
            serde_json::json!({
                "width": 200, "height": 16,
            }),
        );
        let UiNode::Container { props, .. } = &n else {
            panic!()
        };
        assert!(matches!(props.width, Sizing::Fixed(200.0)));
        assert!(matches!(props.height, Sizing::Fixed(16.0)));
        assert_eq!(
            semantic_attr(&n, "data-canvas-paint-id").as_deref(),
            Some("n1")
        );
        assert_eq!(semantic_attr(&n, "width").as_deref(), Some("200"));
    }

    /// Wave 10.17 — text-buffer lowers as a multi-line TextInput.
    #[test]
    fn text_buffer_lower_emits_multiline_text_input() {
        let n = lower(
            "prism.text-buffer",
            text_buffer_lower,
            serde_json::json!({
                "value": "line1\nline2", "bind-value": "doc.body",
            }),
        );
        let UiNode::TextInput {
            value, semantic, ..
        } = &n
        else {
            panic!()
        };
        assert_eq!(value, "line1\nline2");
        let attr = |k: &str| {
            semantic
                .attrs
                .iter()
                .find_map(|(name, v)| (name == k).then(|| v.clone()))
        };
        assert_eq!(attr("data-role").as_deref(), Some("text-buffer"));
        assert_eq!(attr("data-multiline").as_deref(), Some("true"));
        assert_eq!(attr("data-bind-value").as_deref(), Some("doc.body"));
    }
}
