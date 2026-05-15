//! `dispatch_event` — single router from `prism_ui_runtime::event::Event`
//! into `ShellInner` mutations. Replaces the deleted `app/callbacks/`
//! directory (6 files of Slint-callback wiring).
//!
//! Returns `true` when the next frame must re-render. The supervisor
//! in `Shell::run` drives this: every event arm either mutates the
//! store (re-render) or is a no-op (skip the redraw).
//!
//! The contract is *one match arm per runtime variant*. No nested
//! per-block dispatch — every block reads its data from the next
//! `bindings.snapshot(ctx)`, so an event handler's only job is to
//! mutate `ShellInner` (or the `AppState` it carries) and signal
//! the redraw.
//!
//! See `docs/dev/clay-migration-plan.md` §17.

use std::cell::RefCell;
use std::rc::Rc;

use prism_ui_runtime::event::Event;
use prism_ui_runtime::layout::HitRect;

use crate::services::EventOutcome;
use crate::shell::ShellInner;

/// Router entry. `hit` carries the runtime's hit-test result for a
/// pointer event (the topmost container under the cursor). Callers
/// that don't yet have a Surface (every test in this module) pass
/// `None`; the §22 pointer arms run unchanged in that path. Callers
/// that *do* have a Surface (the `Shell::run` handler) pass
/// `Some(hit)` so the §43 C2 / C3 routing arms can fire.
pub fn dispatch_event(
    inner: &Rc<RefCell<ShellInner>>,
    event: &Event,
    hit: Option<HitRect>,
) -> bool {
    // Phase 8 open-Q resolution (`docs/dev/dioxus-inspiration.md`):
    // every event is one logical transaction from the reactive graph's
    // point of view. A pointer-down may fan through hit-routing, a
    // signal cascade, a field-focus blur, and a property write all in
    // one shot — wrap the whole dispatch in `ReactiveContext::batch` so
    // every dependent `Effect` / `Memo` wakes exactly once per event.
    prism_core::reactive::ReactiveContext::batch(|| dispatch_event_inner(inner, event, hit))
}

fn dispatch_event_inner(
    inner: &Rc<RefCell<ShellInner>>,
    event: &Event,
    hit: Option<HitRect>,
) -> bool {
    match event {
        Event::Resize { width, height } => {
            let mut guard = inner.borrow_mut();
            guard.viewport.width = *width as f32;
            guard.viewport.height = *height as f32;
            true
        }
        // Canvas pointer arms (§22). Three forwarders, no per-tool
        // awareness: the slot resolves what `(phase, position)` means
        // under the active tool / drag-target. Adding a new tool mode
        // (e.g. `Skew`) doesn't touch this router.
        //
        // §43 C2 / C3: a pointer-down also consults the hit-test
        // result; when the topmost container carries a routing
        // `data-role`, the matching shell handler fires *before* the
        // canvas gizmo capture runs. This lets clicks on inspector
        // rows / field-editor toggles mutate state without poking
        // through the canvas-tool dispatch.
        Event::PointerDown {
            x,
            y,
            button,
            modifiers: pointer_modifiers,
        } => {
            // B4: a pointer-down that *isn't* on a text-input field
            // commits any active field-focus session before any other
            // routing runs. Clicking a chrome button, a second field
            // row, or the canvas all reach this branch — none should
            // leave the focus "stuck" on the previous field.
            let blurred = pre_route_field_focus_blur(inner, hit.as_ref());
            // Wave 3.4: a right-click on a canvas-resident hit opens
            // the context menu and short-circuits the rest of the
            // pointer chain. Chrome surfaces stay on primary-only
            // routes; the secondary button is a canvas concern.
            if *button == prism_ui_runtime::event::PointerButton::Secondary {
                let opened = hit
                    .as_ref()
                    .map(|h| route_context_menu_open(inner, h, *x, *y))
                    .unwrap_or(false);
                return blurred || opened;
            }
            // Wave 2.4 HSL — slider press needs the exact pointer-x
            // (in viewport-space) to compute the channel fraction;
            // route it *before* `route_pointer_down` (whose handler
            // signature has no event x/y) so the table-driven path
            // never sees the hit.
            let slider = hit
                .as_ref()
                .map(|h| route_color_slider_press(inner, h, *x))
                .unwrap_or(false);
            // The code-editor body needs the pointer-x / pointer-y
            // (in viewport-space) and the modifier state to resolve
            // a byte offset + handle shift-click extend + the multi-
            // click cascade. Route it *before* the table-driven
            // `route_pointer_down` so the handler can take focus +
            // reposition the caret in one pass.
            let code_editor_press = !slider
                && hit
                    .as_ref()
                    .map(|h| route_code_editor_body_press(inner, h, *x, *y, *pointer_modifiers))
                    .unwrap_or(false);
            let routed = slider
                || code_editor_press
                || hit
                    .as_ref()
                    .map(|h| route_pointer_down(inner, h))
                    .unwrap_or(false);
            // §43 A1: an authored `on:click="<action>"` attribute lands
            // on the hit as `data-on-click`. Dispatch through the action
            // grammar parser → `fire_signal` / command table. Sits
            // *after* the `data-role` chrome routes so existing
            // inspector-row / field-edit clicks keep their short-path
            // semantics — they're not author-overridable.
            let acted = !routed
                && hit
                    .as_ref()
                    .map(|h| route_on_click(inner, h))
                    .unwrap_or(false);
            // Wave 14.3 — `bind:value="<node-id>.<key>"` on an
            // `<input>` lowers to a `data-bind-value` semantic attr.
            // When the click lands on a hit carrying that attr (and
            // no chrome route already claimed it), start a field-focus
            // session against the bound source. The existing
            // `FieldFocusService` then routes subsequent text / key
            // events into `set_node_prop` so typing in the input
            // writes back to the doc-node prop.
            let bound = !routed
                && hit
                    .as_ref()
                    .map(|h| route_bind_input_focus(inner, h))
                    .unwrap_or(false);
            // Wave 3.4: any primary click outside the context menu
            // dismisses it. Sits before the canvas-node-select route
            // so clicking on a node behind the menu re-selects rather
            // than leaving a stale overlay.
            let dismissed = !routed
                && !acted
                && !bound
                && hit
                    .as_ref()
                    .map(|h| route_context_menu_dismiss(inner, h))
                    .unwrap_or(false);
            // Wave 3.2: when a palette item is armed, a primary click
            // on the canvas (page, preview, or any tagged canvas
            // node) starts a palette drag. Takes precedence over
            // `route_canvas_node_select` so picking a palette item
            // and clicking an existing node parents the new node
            // under it rather than re-selecting it.
            let palette_armed = !routed
                && !acted
                && !bound
                && !dismissed
                && hit
                    .as_ref()
                    .map(|h| route_palette_drag_begin(inner, h, *x, *y))
                    .unwrap_or(false);
            // §43 B5: a click on a rendered canvas document node
            // (tagged `data-canvas-node="<id>"` by `builder_canvas`)
            // routes to `select_node` *before* the canvas-tool drag
            // capture runs. Chrome clicks that already matched a
            // `data-role` route (inspector-row / field-edit) skip
            // this — `routed` short-circuits the chain.
            let selected = !routed
                && !acted
                && !bound
                && !dismissed
                && !palette_armed
                && hit
                    .as_ref()
                    .map(|h| route_canvas_node_select(inner, h))
                    .unwrap_or(false);
            // The gizmo / handle drag capture stays the last fallback.
            // Skipped when a palette drag claimed the press so the
            // canvas doesn't also try to grab the same down event.
            let captured = !palette_armed && inner.borrow_mut().state.canvas.pointer_down(*x, *y);
            blurred
                || routed
                || acted
                || bound
                || dismissed
                || palette_armed
                || selected
                || captured
        }
        Event::PointerMove { x, y, modifiers: _ } => {
            // B4: a property-row number-scrub session intercepts
            // pointer-move before the canvas gets a chance — they're
            // disjoint surfaces (right rail vs canvas) and the canvas
            // shouldn't see pointer activity that's actually scrubbing
            // a number field.
            let scrubbed = {
                let mut guard = inner.borrow_mut();
                if guard.state.number_drag.is_some() {
                    let g = &mut *guard;
                    let registry = g.registry.as_component_registry();
                    g.state.update_number_drag(*x, Some(registry))
                } else {
                    false
                }
            };
            // Wave 3.2: while a palette drag is in flight every
            // pointer-move updates the ghost's pointer position and
            // tracks the canvas node under the cursor. Falls through
            // cleanly when no drag is active.
            let palette_moved = {
                let active = inner.borrow().state.catalog.palette_drag.is_some();
                if active {
                    let target = hit
                        .as_ref()
                        .and_then(|h| attr_value(h, "data-canvas-node").map(str::to_string));
                    inner
                        .borrow_mut()
                        .state
                        .update_palette_drag(*x, *y, target.as_deref())
                } else {
                    false
                }
            };
            // Wave 3.3: a resize-handle drag intercepts pointer-move
            // before the generic canvas gizmo arm. The mutator returns
            // false when no session is active so this falls through
            // cleanly.
            let resized = inner.borrow_mut().state.update_resize_drag(*x, *y);
            // Wave 2.4 HSL — a color-slider drag updates whichever
            // channel was captured on pointer-down. Cheap-checks the
            // optional drag state up-front so non-drag pointer-moves
            // pay nothing.
            let slid = if inner
                .borrow()
                .state
                .overlay
                .color_picker
                .slider_drag
                .is_some()
            {
                apply_color_slider_at(inner, *x)
            } else {
                false
            };
            // Code-editor drag-select intercepts pointer-move when a
            // press anchored a drag. Sits before the canvas tool so a
            // drag that began inside the editor body can't bleed into
            // the canvas pointer arm. No-op when no drag is active.
            let editor_dragged = route_code_editor_body_drag(inner, *x, *y);
            // Editor hover — resolve the token under the pointer to
            // a help entry and surface it through the help-tooltip
            // slot. Runs after drag so a drag in progress still
            // wins; no-op when the hit isn't a code-editor body.
            route_code_editor_body_hover(inner, hit.as_ref(), *x, *y);
            scrubbed
                || palette_moved
                || resized
                || slid
                || editor_dragged
                || inner.borrow_mut().state.canvas.pointer_move(*x, *y)
        }
        Event::PointerUp { x, y, .. } => {
            // B4: end the number-scrub session if one was active. If
            // the user never moved past the threshold, treat the
            // release as a click → `+1` step (the legacy behaviour
            // before the drag scrubber landed).
            // If a scrub session was active and never crossed the
            // threshold, treat the release as a click → +1 step.
            // Otherwise (no session, or dragged): fall through to
            // canvas.
            let was_click_release = {
                let mut guard = inner.borrow_mut();
                matches!(guard.state.end_number_drag(), Some(false))
            };
            let stepped = was_click_release
                && hit
                    .as_ref()
                    .map(|h| step_number_on_click(inner, h))
                    .unwrap_or(false);
            // Wave 3.2: commit the palette drag at release. The
            // drop-target captured by the last move (or the initial
            // down) decides parent; the new node moves the
            // selection so the inspector / properties refresh.
            let dropped = {
                let active = inner.borrow().state.catalog.palette_drag.is_some();
                if active {
                    let mut guard = inner.borrow_mut();
                    let g = &mut *guard;
                    let registry = g.registry.as_component_registry();
                    g.state.end_palette_drag(Some(registry)).is_some()
                } else {
                    false
                }
            };
            // Wave 3.3: commit the resize drag at release. The
            // transform mutations from `update_resize_drag` are
            // already live in the document; this just drops the
            // session so the next click can start a new one.
            let resize_committed = inner.borrow_mut().state.end_resize_drag();
            // Wave 2.4 HSL — release any captured color-slider drag.
            // `set_color_picker_value` already wrote the live channel
            // through `set_node_prop`; this just clears the capture
            // slot so the next press starts a fresh session.
            let slider_released = {
                let mut guard = inner.borrow_mut();
                guard
                    .state
                    .overlay
                    .color_picker
                    .slider_drag
                    .take()
                    .is_some()
            };
            // Editor drag release — drop any in-flight selection
            // drag before the canvas pointer-up runs so the canvas
            // tool can't accidentally pick up the release event.
            let editor_released = route_code_editor_body_release(inner);
            stepped
                || dropped
                || resize_committed
                || slider_released
                || editor_released
                || inner.borrow_mut().state.canvas.pointer_up(*x, *y)
        }
        // §24: every other event variant fans out through the service
        // registry. Services declare their interest via `on_event`;
        // the first to return `Handled` short-circuits. Adding a new
        // feature *does not touch this match*.
        Event::Wheel { .. }
        | Event::Key { .. }
        | Event::Text { .. }
        | Event::Focus { .. }
        | Event::ImePreedit { .. }
        | Event::ImeCommit { .. }
        | Event::ImeEnabled
        | Event::ImeDisabled => {
            let mut guard = inner.borrow_mut();
            // Split-borrow: we need `&services` and `&mut MutCtx{state, undo, viewport}`
            // simultaneously. Re-borrow the fields explicitly so the
            // borrow checker sees the disjoint slices.
            let g = &mut *guard;
            let viewport = g.viewport;
            let services = &g.services;
            let registry = g.registry.as_component_registry();
            let modifier_registry: &prism_builder::ModifierRegistry = g.modifier_registry.as_ref();
            let mut ctx = crate::services::MutCtx {
                state: &mut g.state,
                viewport,
                undo: &mut g.undo,
                vfs: g.vfs.as_mut(),
                luau: g.luau.as_mut(),
                clipboard: &mut g.clipboard,
                registry: Some(registry),
                modifier_registry: Some(modifier_registry),
            };
            matches!(services.fan_out(event, &mut ctx), EventOutcome::Handled)
        }
    }
}

/// One declarative table — `data-role` → handler. Adding a new
/// click-routable shell primitive is one row plus one handler fn.
type PointerHandler = fn(&Rc<RefCell<ShellInner>>, &HitRect) -> bool;

const POINTER_ROUTES: &[(&str, PointerHandler)] = &[
    ("inspector-row", handle_inspector_row_click),
    ("field-edit", handle_field_edit_click),
    ("workflow-page-button", handle_workflow_page_button_click),
    ("dock-tab", handle_dock_tab_click),
    ("palette-item", handle_palette_item_click),
    ("nav-page-row", handle_nav_page_row_click),
    ("toolbar-device-pill", handle_toolbar_device_pill_click),
    ("toolbar-zoom-reset", handle_toolbar_zoom_reset_click),
    ("app-card", handle_app_card_click),
    ("schema-row", handle_schema_row_click),
    ("signal-connection-row", handle_signal_connection_row_click),
    ("nav-button", handle_nav_button_click),
    ("menu-pill", handle_menu_pill_click),
    // Wave 3.3: pointer-down on a `shell.resize-handle` carries
    // `data-direction` (one of `tl|t|tr|r|br|b|bl|l`); the handler
    // captures the direction + snapshot for the active selection so
    // the next pointer-move applies the delta against a pristine
    // transform (no integration drift across the drag).
    ("resize-handle", handle_resize_handle_press),
    // Wave 1.6 of `docs/dev/composable-builder-plan.md` — composable
    // inspector. Each row in the modifier-header strip is a separate
    // route; the picker overlay's open / select pair completes the
    // attach flow.
    ("modifier-toggle", handle_modifier_toggle_click),
    ("modifier-remove", handle_modifier_remove_click),
    ("modifier-reorder", handle_modifier_reorder_click),
    ("add-modifier-open", handle_add_modifier_open_click),
    (
        "modifier-picker-select",
        handle_modifier_picker_select_click,
    ),
    // Wave 4.3 — connection picker. The form has three field rows
    // (`data-field` ∈ source-signal | action-kind | target-label)
    // and two action buttons. The field handler dispatches to the
    // matching mutator; today only `action-kind` actually cycles,
    // source / target await Wave 10's text-input primitive but
    // surface their hover affordance immediately.
    (
        "connection-picker-field",
        handle_connection_picker_field_click,
    ),
    ("connection-picker-add", handle_connection_picker_add_click),
    (
        "connection-picker-cancel",
        handle_connection_picker_cancel_click,
    ),
    // Wave 2.5 — file-kind property row's "Browse…" button. Invokes
    // `Vfs::pick_file` on the live shell `ShellInner::vfs`; the
    // first picked path commits via `set_node_prop`. On Cancelled /
    // Unsupported / Err the handler is a noop (no toast — the user
    // *chose* to dismiss the dialog).
    ("file-browse", handle_file_browse_click),
    // Wave 2.4 — color-kind property row's swatch + `shell.color-picker`
    // overlay rows. Swatch click opens the picker, preset rows
    // commit through `set_node_prop`, and the close button dismisses
    // the overlay without committing.
    ("color-swatch", handle_color_swatch_click),
    ("color-preset-select", handle_color_preset_select_click),
    ("color-picker-close", handle_color_picker_close_click),
    // Wave 2.3 — select-dropdown overlay. Option rows commit through
    // `set_node_prop` and dismiss the overlay; the close button is
    // explicit so a future "click-outside dismisses" wiring (a Wave
    // 11 follow-up around `OverlaySlot::dismiss_all_on_outside_click`)
    // can sit alongside without touching the dropdown's own routes.
    (
        "select-dropdown-option",
        handle_select_dropdown_option_click,
    ),
    ("select-dropdown-close", handle_select_dropdown_close_click),
    // Editor tabs — click switches active, close button removes,
    // new-tab button opens a fresh Untitled. Each handler reads
    // `data-tab-index` to know which tab the click targets.
    ("editor-tab", handle_editor_tab_click),
    ("editor-tab-close", handle_editor_tab_close_click),
    ("editor-tab-new", handle_editor_tab_new_click),
    // Wave 2.4 HSL — the slider press path is routed *before*
    // `route_pointer_down` because the channel commit needs the
    // exact pointer-x coordinate (the in-table handler signature
    // can't access it). See `route_color_slider_press` for the
    // pointer-y / move / up wiring.
];

fn route_pointer_down(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let role = match attr_value(hit, "data-role") {
        Some(r) => r,
        None => return false,
    };
    for (k, handler) in POINTER_ROUTES {
        if *k == role {
            return handler(inner, hit);
        }
    }
    false
}

/// §43 C3: a click on an inspector row sets the canvas selection to
/// the row's `data-target-id` and runs one resync pass.
fn handle_inspector_row_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    g.state.select_node(target, Some(registry))
}

/// §43 C2: a click on a field-editor row mutates the bound
/// property. Click semantics fan out by kind:
///
/// * `boolean` — toggle the current value.
/// * `select` — cycle to the next option declared in `data-options`
///   (comma-joined value list from the spec). Wraps at the end.
/// * `text` / `color` / `file` — set a text-input focus session
///   (`AppState::field_focus`). The `FieldFocusService` then routes
///   subsequent `Text` / `Key` events into the bound prop until Esc
///   (cancel + restore) or Enter (commit). Clicking elsewhere also
///   commits via the no-target-id fall-through in
///   [`pre_route_field_focus_blur`].
/// * `number` / `integer` — fall through to the drag-scrubber path.
///   The pointer-down here only *initialises* the scrub session;
///   pointer-move + pointer-up do the actual mutation. Tiny drags
///   below the threshold count as a click → `+1` step on release.
fn handle_field_edit_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let Some(key) = attr_value(hit, "data-key") else {
        return false;
    };
    let kind = attr_value(hit, "data-kind").unwrap_or("");
    // Wave 2.3 — `select` kind opens an anchored dropdown overlay
    // instead of cycling on click. The cycle behaviour stays
    // available through the keyboard path (arrow keys via field-
    // focus session) for the same dropdown's open state. Falls
    // through cleanly when no options are bound.
    if kind == "select" {
        let Some(options) = attr_value(hit, "data-options") else {
            return false;
        };
        let pairs: Vec<&str> = options.split(',').filter(|s| !s.is_empty()).collect();
        if pairs.is_empty() {
            return false;
        }
        let current = attr_value(hit, "data-value").unwrap_or("").to_string();
        // Each comma-separated entry is `<value>:<label>` (the
        // field-editor's `data-options` shape). When the colon is
        // missing we treat the bare token as both value and label.
        let opts: Vec<serde_json::Value> = pairs
            .iter()
            .map(|tok| {
                let (val, label) = match tok.find(':') {
                    Some(idx) => (&tok[..idx], &tok[idx + 1..]),
                    None => (*tok, *tok),
                };
                serde_json::json!({ "value": val, "label": label })
            })
            .collect();
        let mut guard = inner.borrow_mut();
        return guard
            .state
            .open_select_dropdown(target, key, &current, opts);
    }
    // Kind: boolean — flip on every click. The mutation runs
    // through `set_node_prop`.
    let toggle_value: Option<serde_json::Value> = match kind {
        "boolean" => {
            let cur = attr_value(hit, "data-value").unwrap_or("false") == "true";
            Some(serde_json::Value::Bool(!cur))
        }
        _ => None,
    };
    if let Some(value) = toggle_value {
        let mut guard = inner.borrow_mut();
        let g = &mut *guard;
        return write_field_value(g, hit, target, key, value);
    }
    // Number / integer: open a drag-scrub session. Pointer-move
    // delivers the actual mutation; click (no drag) falls through
    // to the `+1 step` on pointer-up.
    if kind == "number" || kind == "integer" {
        let cur = attr_value(hit, "data-value")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let min = attr_value(hit, "data-min").and_then(|s| s.parse::<f64>().ok());
        let max = attr_value(hit, "data-max").and_then(|s| s.parse::<f64>().ok());
        let start_x = hit.bounds.x + hit.bounds.width * 0.5;
        let mut guard = inner.borrow_mut();
        guard.state.begin_number_drag(crate::state::NumberDragInit {
            target_id: target,
            key,
            kind,
            start_x,
            start_value: cur,
            min,
            max,
        });
        // Wave 2.2: ALSO open a `field_focus` session so arrow keys
        // (handled by `FieldFocusService`) nudge the value ±1 / ±10
        // without the user needing to drag. The drag and the focus
        // coexist: pointer-move runs the scrub; arrow keys nudge;
        // Esc / Enter / clicking elsewhere closes the focus; the
        // drag ends on pointer-up regardless.
        guard.state.begin_field_focus(target, key, kind);
        return true;
    }
    // Text-editing kinds: open a focus session. Subsequent Text/Key
    // events route through `FieldFocusService` until commit / cancel.
    // Wave 2.1 of `docs/dev/composable-builder-plan.md`: `textarea`
    // shares the same focus session shape as `text` — the only
    // difference is multi-line commit semantics, which `FieldFocusService`
    // distinguishes on `field_focus.kind` at Enter time.
    if matches!(kind, "text" | "textarea" | "color" | "file") {
        let mut guard = inner.borrow_mut();
        return guard.state.begin_field_focus(target, key, kind);
    }
    false
}

/// Click on a workflow-page tab in the bottom bar — switches the
/// active workflow page. The DockWorkspace mutation triggers a
/// re-render of the dock tree on the next frame.
fn handle_workflow_page_button_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    guard.state.workspace.workspace.switch_page_by_id(target)
}

/// Click on a dock-panel tab — activates that tab inside its
/// TabGroup. `navigate_to_panel` handles both same-page activation
/// and cross-page navigation; the active dock state mutates and
/// the dock-workspace block re-renders against the new tree.
fn handle_dock_tab_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    guard.state.workspace.workspace.navigate_to_panel(target)
}

/// Click on a component-palette row — selects the item by id. The
/// palette block re-paints with the highlighted pill on the next
/// frame; downstream the canvas slot reads `palette_selected` to
/// decide whether a canvas-cell click should drop a new node.
fn handle_palette_item_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let current = guard.state.catalog.palette_selected.as_deref();
    if current == Some(target) {
        return false;
    }
    guard.state.catalog.palette_selected = Some(target.to_string());
    true
}

/// Click on a navigation-panel page row — moves both the chevron
/// cursor (`selected_page`) and the active-page flag onto the
/// clicked row. The cursor drives the `selected` / `show-delete`
/// props the row reads; the active flag drives the workspace's
/// current page. Returns `true` when either side moved.
fn handle_nav_page_row_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let cursor_moved = guard.state.navigation.select_row(target);
    let active_moved = guard.state.navigation.select_page_by_id(target);
    cursor_moved || active_moved
}

/// Click on a builder-toolbar device pill — switches the canvas's
/// responsive preview target. The pill carries `data-device` with
/// the kebab-case id (`desktop` / `tablet` / `mobile`); unknown
/// strings fall through cleanly without mutating state.
fn handle_toolbar_device_pill_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(id) = attr_value(hit, "data-device") else {
        return false;
    };
    let Some(device) = crate::state::Device::from_id(id) else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    if guard.state.canvas.device == device {
        return false;
    }
    guard.state.canvas.device = device;
    true
}

/// Click on a Launchpad `shell.app-card` — switches the workspace's
/// `active_app` cursor to the card's `data-app` id via the full
/// ADR-009 + ADR-010 swap chain (skeleton + service factories +
/// render dirty). Cards without a `data-app` value (the "create"
/// affordance) fall through cleanly. The D4 follow-up referenced in
/// `ui-migration-followups.md` lands here: launchpad clicks now flow
/// through `Shell::switch_active_app`, so per-app skeleton swap and
/// service rebuild are automatic.
fn handle_app_card_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(app_id) = attr_value(hit, "data-app") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    guard.switch_active_app(Some(app_id))
}

/// Click on a schema-designer row — moves the chevron cursor
/// (`builder.schema.selected_field`) onto the row's `data-target-id`.
/// The trash button visibility / aria-selected flag derive from the
/// cursor, so the row re-paints with its affordances on the next
/// frame.
fn handle_schema_row_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    guard.state.builder.select_schema_field(target)
}

/// Click on a signals-panel connection row — moves the chevron cursor
/// (`builder.selected_connection`) onto the row's `data-target-id`.
/// `signals.delete-selected-connection` reads the cursor when the
/// trash button fires.
fn handle_signal_connection_row_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    guard.state.builder.select_signal_connection(target)
}

/// Click on an activity-bar nav-button — flips
/// `ChromeSlot::nav_buttons[*].selected` so the radio-style selection
/// follows the click. `select_nav_button` returns `true` only when
/// the selection actually moved, so an idempotent re-click of the
/// already-selected button is a clean no-op.
fn handle_nav_button_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    guard.state.chrome.select_nav_button(target)
}

/// Click on a menu-bar pill — toggles `ChromeSlot::active_menu`.
/// Same pill re-clicked closes the menu; a different pill moves the
/// cursor; clicking outside (the global pre-route blur) doesn't fire
/// this handler at all, which is fine — the menu-dropdown overlay
/// will own its own outside-click dismiss once it's authored.
fn handle_menu_pill_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    guard.state.chrome.select_menu(target)
}

/// Click on the toolbar's zoom-percentage pill — resets canvas zoom
/// to 1.0. The `+` / `−` icon buttons inside the same cluster don't
/// route here; once they grow a `command` prop they dispatch via
/// the action grammar through `route_on_click`.
fn handle_toolbar_zoom_reset_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    let mut guard = inner.borrow_mut();
    if (guard.state.canvas.viewport.zoom - 1.0).abs() < f32::EPSILON {
        return false;
    }
    guard.state.canvas.viewport.zoom = 1.0;
    true
}

// ── Wave 1.6 modifier routes ─────────────────────────────────────

fn parse_modifier_idx(hit: &HitRect) -> Option<usize> {
    attr_value(hit, "data-modifier-idx").and_then(|s| s.parse::<usize>().ok())
}

/// Boilerplate-eliminator for the per-row "click → mutator(target, idx)"
/// shape used by `modifier-toggle` and `modifier-remove`. Both routes
/// share the same `data-target-id` + `data-modifier-idx` parse, the
/// same borrow + registry handoff — only the mutator name differs.
/// Adding a third indexed route is one macro invocation:
///
/// ```ignore
/// indexed_modifier_route!(handle_modifier_toggle_click, toggle_modifier);
/// ```
macro_rules! indexed_modifier_route {
    ($handler:ident, $mutator:ident) => {
        fn $handler(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
            let Some(target) = attr_value(hit, "data-target-id") else {
                return false;
            };
            let Some(idx) = parse_modifier_idx(hit) else {
                return false;
            };
            let mut guard = inner.borrow_mut();
            let g = &mut *guard;
            let registry = g.registry.as_component_registry();
            g.state.$mutator(target, idx, Some(registry))
        }
    };
}

indexed_modifier_route!(handle_modifier_toggle_click, toggle_modifier);
indexed_modifier_route!(handle_modifier_remove_click, detach_modifier);

/// Wave 1.6 — `data-role="modifier-reorder"`. Today the route is a
/// hook for the Wave 3 pointer-drag gesture; without that gesture,
/// the only useful one-click behaviour is to nudge the modifier up
/// one position (and wrap to the bottom from the top). Keeps the
/// behaviour observable end-to-end through one mutator call until
/// the drag handle lands.
fn handle_modifier_reorder_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let Some(idx) = parse_modifier_idx(hit) else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let len = g
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find(target))
        .map(|n| n.modifiers.len())
        .unwrap_or(0);
    if len <= 1 {
        return false;
    }
    let to = if idx == 0 { len - 1 } else { idx - 1 };
    let registry = g.registry.as_component_registry();
    g.state.reorder_modifier(target, idx, to, Some(registry))
}

fn handle_add_modifier_open_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let attached_raw = attr_value(hit, "data-attached").unwrap_or("[]");
    let attached: Vec<String> = serde_json::from_str(attached_raw).unwrap_or_default();
    let mut guard = inner.borrow_mut();
    guard.state.overlay.modifier_picker = crate::state::ModifierPicker {
        open: true,
        target_id: target.to_string(),
        attached,
    };
    true
}

fn handle_modifier_picker_select_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let Some(modifier_id) = attr_value(hit, "data-modifier-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    let attached = g.state.attach_modifier(target, modifier_id, Some(registry));
    g.state.overlay.modifier_picker = crate::state::ModifierPicker::default();
    attached
}

/// B4: commit the active text-input focus session when the user
/// clicks anywhere that isn't the *same* field. The router calls
/// this *before* every other pointer-down route, so the next route
/// runs against a freshly cleared focus. Returns `true` when a focus
/// session ended (any focus mutation requests a redraw).
fn pre_route_field_focus_blur(inner: &Rc<RefCell<ShellInner>>, hit: Option<&HitRect>) -> bool {
    let mut guard = inner.borrow_mut();
    let Some(focus) = guard.state.field_focus.as_ref() else {
        return false;
    };
    // Same-field click → keep focus open. The router's own field-edit
    // handler will see the hit and (idempotently) re-focus the same
    // row; without this guard, double-clicking a focused field would
    // commit + re-open every time.
    let staying = hit
        .map(|h| {
            attr_value(h, "data-role") == Some("field-edit")
                && attr_value(h, "data-target-id").map(|s| s == focus.target_id) == Some(true)
                && attr_value(h, "data-key").map(|s| s == focus.key) == Some(true)
        })
        .unwrap_or(false);
    if staying {
        return false;
    }
    // Commit (the draft is already flushed into the prop after every
    // keystroke). No `Esc`-style restore here — clicking away is the
    // canonical "I'm done" intent.
    guard.state.commit_field_focus()
}

/// B4: on pointer-up over a number/integer field-edit, if the
/// scrubber never moved past the threshold, treat the release as a
/// click and step the bound prop by `+1` (clamped to the field's
/// `data-min` / `data-max`). Preserves the existing click-to-step
/// behaviour the cursor router shipped before drag-scrub landed.
fn step_number_on_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let role = attr_value(hit, "data-role");
    if role != Some("field-edit") {
        return false;
    }
    let kind = attr_value(hit, "data-kind").unwrap_or("");
    if kind != "number" && kind != "integer" {
        return false;
    }
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let Some(key) = attr_value(hit, "data-key") else {
        return false;
    };
    let cur = attr_value(hit, "data-value")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let min = attr_value(hit, "data-min").and_then(|s| s.parse::<f64>().ok());
    let max = attr_value(hit, "data-max").and_then(|s| s.parse::<f64>().ok());
    let stepped = cur + 1.0;
    let clamped = match (min, max) {
        (Some(mn), Some(mx)) => stepped.clamp(mn, mx),
        (Some(mn), None) => stepped.max(mn),
        (None, Some(mx)) => stepped.min(mx),
        (None, None) => stepped,
    };
    let value = if kind == "integer" {
        serde_json::Value::from(clamped.round() as i64)
    } else {
        serde_json::json!(clamped)
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    write_field_value(g, hit, target, key, value)
}

/// Inline-template third slice — dispatch a field-editor write to
/// either [`AppState::set_node_prop`] (regular node selection) or
/// [`AppState::set_facet_template_prop`] (facet-template descendant
/// selection) based on whether `data-template-path` is present on
/// the hit-test rect. Single seam for every click-driven write, so
/// the routing logic doesn't drift across the seven-or-so commit
/// sites in `handle_field_edit_click` / drag-commit / text-input
/// commit.
fn write_field_value(
    g: &mut crate::shell::ShellInner,
    hit: &HitRect,
    target: &str,
    key: &str,
    value: serde_json::Value,
) -> bool {
    let template_path = attr_value(hit, "data-template-path")
        .unwrap_or("")
        .to_string();
    // Split borrow: name `registry` and `state` as disjoint fields
    // of `*g` so the mut borrow on `state` and the immut borrow on
    // `registry` don't overlap. Sound — Rust's borrow checker
    // understands struct field splits.
    let crate::shell::ShellInner {
        registry, state, ..
    } = g;
    let reg = registry.as_component_registry();
    if !template_path.is_empty() {
        return state.set_facet_template_prop(target, &template_path, key, value, Some(reg));
    }
    state.set_node_prop(target, key, value, Some(reg))
}

/// §43 A1: pointer-down on a container that carries a
/// `data-on-click="<action>"` semantic attribute dispatches the
/// parsed action. Today two verbs ship handlers:
///
/// * `emit <signal>` cascades through `SignalsService::fire_signal`
///   against the hit's container id (`source_node = hit.id`),
/// * `cmd <command-id>` invokes the same command table the palette
///   uses, so any registered shell command is one-attribute away.
///
/// `set` / `toggle` / `navigate` / `play` / `luau` parse cleanly
/// (see `prism_builder::signal::parse_action`) but the executor is
/// a no-op pending their owning subsystems — the parse step is
/// what keeps `.prism-ui` source author-clean today.
///
/// **Wave 14.3 — event modifiers.** Every `data-on-click*` attr on
/// the hit is dispatched in attribute order; the modifier suffix
/// (`-once`, `-stop`, `-prevent`) is parsed from the attr key and
/// applied around the action:
///
/// * **`.once`** — the `(hit-id, attr-key)` pair is recorded in
///   [`AppState::once_fired`]; subsequent dispatches no-op until the
///   set is cleared (selection change, document reload).
/// * **`.stop` / `.prevent`** — the router returns `true` after the
///   handler runs regardless of whether the action did work, so the
///   rest of the pointer-down chain (canvas selection, palette drag,
///   gizmo capture) is suppressed. In a retained-mode tree there is
///   no parent-bubbling to halt; "propagation" here means the
///   pointer-down fallback chain.
fn route_on_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    use prism_builder::signal::{parse_action, ParsedAction};

    // Gather every `data-on-click*` candidate up front. Each entry is
    // (attr-key, raw action body, parsed modifier set). Multiple
    // candidates on one element are unusual but legal — `on:click=`
    // and `on:click.once=` coexist as distinct attrs and both fire.
    let candidates: Vec<(String, String, EventModifiers)> = hit
        .attrs
        .iter()
        .filter_map(|(k, v)| {
            let suffix = k.strip_prefix("data-on-click")?;
            let mods = EventModifiers::parse(suffix.trim_start_matches('-'));
            Some((k.clone(), v.clone(), mods))
        })
        .collect();
    if candidates.is_empty() {
        return false;
    }

    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let viewport = g.viewport;
    let registry = g.registry.as_component_registry();
    let modifier_registry: &prism_builder::ModifierRegistry = g.modifier_registry.as_ref();
    let services = &g.services;

    let mut any_fired = false;
    let mut consume = false;
    for (attr_key, raw, mods) in candidates {
        // `.once` gate: skip if this exact handler already fired
        // against this hit-id in a prior dispatch.
        if mods.once {
            let key = (hit.id.clone(), attr_key.clone());
            if g.state.once_fired.contains(&key) {
                // Still respect `.stop`/`.prevent` so a `.once.stop`
                // handler keeps blocking the fallback chain even
                // after firing once.
                if mods.stop || mods.prevent {
                    consume = true;
                }
                continue;
            }
        }
        let Some(action) = parse_action(&raw) else {
            continue;
        };
        let fired = {
            let mut ctx = crate::services::MutCtx {
                state: &mut g.state,
                viewport,
                undo: &mut g.undo,
                vfs: g.vfs.as_mut(),
                luau: g.luau.as_mut(),
                clipboard: &mut g.clipboard,
                registry: Some(registry),
                modifier_registry: Some(modifier_registry),
            };
            match action {
                ParsedAction::Emit { signal } => {
                    crate::services::signals::fire_signal(
                        &mut ctx,
                        hit.id.as_str(),
                        signal.as_str(),
                        &serde_json::Value::Null,
                        0,
                    ) > 0
                }
                ParsedAction::Command { id } => services.commands().run(id.as_str(), &mut ctx),
                ParsedAction::Navigate { .. }
                | ParsedAction::SetProperty { .. }
                | ParsedAction::Toggle { .. }
                | ParsedAction::Play { .. }
                | ParsedAction::Luau { .. }
                | ParsedAction::Bind { .. }
                | ParsedAction::Unsupported { .. } => false,
            }
        };
        if mods.once && fired {
            g.state
                .once_fired
                .insert((hit.id.clone(), attr_key.clone()));
        }
        any_fired = any_fired || fired;
        if mods.stop || mods.prevent {
            consume = true;
        }
    }
    any_fired || consume
}

/// **Wave 14.3** — `bind:value="<node-id>.<key>"` on an `<input>`
/// lowers to a `data-bind-value` semantic attr on the input's hit.
/// When the click lands on a hit carrying that attr (and the chrome
/// roles haven't already claimed the press), parse the source path
/// and start a field-focus session against the bound doc node. The
/// existing [`crate::services::field_focus`] service then routes the
/// next `Event::Text` / `Event::Key` keystrokes into
/// [`crate::state::AppState::set_node_prop`] so typing in the input
/// writes back to the underlying prop. Returns `true` when a focus
/// session actually started.
///
/// Source grammar accepted today is `"<node-id>.<key>"` — the same
/// shape `prism_builder::DocumentBindings::install_for` recognises
/// for `ActionKind::Bind` connections. Other shapes (`$selection.name`
/// selector refs, literals) round-trip in the data attr but don't
/// start a focus session — the host can plug additional resolvers
/// without rewiring this seam.
fn route_bind_input_focus(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(source) = attr_value(hit, "data-bind-value") else {
        return false;
    };
    let Some((node, key)) = source.split_once('.') else {
        return false;
    };
    let node = node.trim();
    let key = key.trim();
    if node.is_empty() || key.is_empty() || node.starts_with('$') {
        return false;
    }
    // Validate the source node lives in the canvas document before
    // starting focus. Without the check, a typo in the bind path
    // would leave a stuck focus session pointing at nothing.
    let node = node.to_string();
    let key = key.to_string();
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let exists = g
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find(&node))
        .is_some();
    if !exists {
        return false;
    }
    g.state.begin_field_focus(&node, &key, "text")
}

/// Parsed event-modifier suffix attached to a `data-on-<event>` attr
/// key. Authors write `on:click.once.stop="cmd save"` — the lowering
/// pass joins the dotted suffix with dashes (`data-on-click-once-stop`)
/// so this struct just splits the suffix on `-` and matches each
/// segment against the supported set. Unknown segments are dropped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct EventModifiers {
    once: bool,
    stop: bool,
    prevent: bool,
}

impl EventModifiers {
    fn parse(suffix: &str) -> Self {
        let mut m = Self::default();
        if suffix.is_empty() {
            return m;
        }
        for part in suffix.split('-') {
            match part {
                "once" => m.once = true,
                "stop" => m.stop = true,
                "prevent" => m.prevent = true,
                _ => {}
            }
        }
        m
    }
}

/// §43 B5: pointer-down on a `data-canvas-node`-tagged container
/// updates the canvas selection. `select_node` validates the id
/// against the active document and re-derives the inspector tree +
/// property rows for the new selection. Returns true when the
/// selection actually moved (so the frame needs to redraw).
///
/// Wave 3.3 — also captures the hit's bounding rect into
/// `state.canvas.selection_bbox` so the next frame paints the
/// selection outline + 8-handle ring around the actual click rect.
fn route_canvas_node_select(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(node_id) = attr_value(hit, "data-canvas-node") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    let moved = g.state.select_node(node_id, Some(registry));
    // Capture the bbox unconditionally — a click that lands on the
    // already-selected node still re-grounds the gizmo against the
    // current layout (which may have drifted since the last click,
    // e.g. after a window resize). `set_selection_bbox` is a pure
    // data write so re-setting an equal value is cheap.
    g.state
        .set_selection_bbox(Some(crate::state::SelectionBbox {
            x: hit.bounds.x,
            y: hit.bounds.y,
            width: hit.bounds.width,
            height: hit.bounds.height,
        }));
    moved
}

/// Wave 3.3 — pointer-down on one of the 8 resize handles painted by
/// `build_selection_layer` around the selected canvas node. The
/// handle carries `data-direction` (`tl|t|tr|r|br|b|bl|l`); the
/// handler snapshots the selection's transform and captures the
/// origin. The follow-up pointer-moves run through
/// `state.update_resize_drag(x, y)` in the PointerMove arm.
fn handle_resize_handle_press(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(direction) = attr_value(hit, "data-direction") else {
        return false;
    };
    // Use the hit's bbox center as the origin so the drag delta is
    // measured from the handle's anchor point, not an arbitrary
    // click position inside the handle. The handle is an 8x8 rect;
    // sampling the centre keeps the math agnostic to where inside
    // the handle the user pressed.
    let origin_x = hit.bounds.x + hit.bounds.width * 0.5;
    let origin_y = hit.bounds.y + hit.bounds.height * 0.5;
    let mut guard = inner.borrow_mut();
    guard.state.begin_resize_drag(direction, origin_x, origin_y)
}

/// Wave 4.3 — click on one of the picker's three field rows. The
/// row carries `data-field` ∈ {source-signal, action-kind,
/// target-label}; `action-kind` cycles in place, the two text
/// fields surface today as no-ops awaiting Wave 10's text-input
/// primitive (the hover affordance + routing attr land now so the
/// upgrade is one mutator wiring later).
fn handle_connection_picker_field_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(field) = attr_value(hit, "data-field") else {
        return false;
    };
    match field {
        "action-kind" => inner
            .borrow_mut()
            .state
            .cycle_connection_picker_action_kind(),
        // `source-signal` / `target-label` text-input UX lands
        // with Wave 10's `<text-input>` primitive. Report the
        // click as consumed (no fall-through to canvas-tool drag)
        // so the picker stays open and the user can finish
        // confirming the rest of the form.
        "source-signal" | "target-label" => true,
        _ => false,
    }
}

/// Wave 4.3 — confirm the picker → create the connection, close.
fn handle_connection_picker_add_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    g.state.confirm_connection_picker(Some(registry)).is_some()
}

/// Wave 4.3 — cancel the picker without inserting.
fn handle_connection_picker_cancel_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    inner.borrow_mut().state.close_connection_picker()
}

/// Wave 2.5 — file-kind property row's "Browse…" button. Reads
/// `data-target-id` + `data-key` + (optional) `data-accept` off the
/// hit, invokes `Vfs::pick_file` against the shell's live vfs, and
/// commits the first picked path through `set_node_prop`. On
/// `VfsError::Cancelled` / `Unsupported` the handler is a noop —
/// the user dismissed the dialog (or the host didn't wire one), so
/// no mutation, no toast, no redraw.
/// Wave 2.4 — color-kind property row's swatch button. Reads
/// `data-target-id` + `data-key` + `data-value` (the current color
/// hex, mirrored off the field-edit row's `data-value` attr by the
/// field-editor lower) and seeds the `OverlaySlot::color_picker`
/// state. The overlay's rendering is handled by the bound
/// `shell.color-picker` block on the next frame.
fn handle_color_swatch_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let Some(key) = attr_value(hit, "data-key") else {
        return false;
    };
    let value = attr_value(hit, "data-value").unwrap_or("");
    inner
        .borrow_mut()
        .state
        .open_color_picker(target, key, value)
}

/// Wave 2.4 — a preset swatch inside `shell.color-picker`. Reads
/// `data-color` (the hex literal) and commits it through
/// `set_color_picker_value` so the bound prop on the picker's
/// (target_id, key) target moves immediately. The picker stays
/// open so the user can preview multiple presets without
/// reopening.
fn handle_color_preset_select_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(color) = attr_value(hit, "data-color") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    g.state.set_color_picker_value(color, Some(registry))
}

/// Wave 2.4 — close button on the color picker. Mirrors Esc.
fn handle_color_picker_close_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    inner.borrow_mut().state.close_color_picker()
}

/// Wave 2.3 — option row inside `shell.select-dropdown`. Reads
/// `data-value` (the option's value) and commits through
/// `commit_select_dropdown_value`, which writes via `set_node_prop`
/// and closes the overlay. Idempotent against the currently-selected
/// option (no mutation, no resync, but still consumes the click).
fn handle_select_dropdown_option_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(value) = attr_value(hit, "data-value") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    g.state.commit_select_dropdown_value(value, Some(registry))
}

/// Wave 2.3 — explicit close button on the select dropdown.
fn handle_select_dropdown_close_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    inner.borrow_mut().state.close_select_dropdown()
}

/// Process-wide hit-tester for editor text. cosmic-text's
/// `FontSystem` is heavy to construct (font enumeration), so we
/// reuse a single one across every click. The text system never
/// bakes glyphs here — `byte_at` only walks shaped runs — so the
/// glyph cache the type maintains for the femtovg path stays
/// empty in this instance.
fn editor_hit_tester() -> &'static std::sync::Mutex<prism_ui_runtime::text::TextSystem> {
    use std::sync::{Mutex, OnceLock};
    static HIT_TESTER: OnceLock<Mutex<prism_ui_runtime::text::TextSystem>> = OnceLock::new();
    HIT_TESTER.get_or_init(|| Mutex::new(prism_ui_runtime::text::TextSystem::new()))
}

/// Resolve a click on the code-editor body to a byte offset within
/// the current buffer through cosmic-text's real glyph spans — works
/// under both monospace and proportional fonts.
#[allow(clippy::too_many_arguments)]
fn resolve_editor_byte_at(
    text: &str,
    bounds_x: f32,
    bounds_y: f32,
    bounds_w: f32,
    click_x: f32,
    click_y: f32,
    scroll_x: f32,
    scroll_y: f32,
    font_size: f32,
) -> usize {
    const TEXT_PAD_X: f32 = 6.0;
    const TEXT_PAD_Y: f32 = 4.0;
    let local_x = (click_x - bounds_x - TEXT_PAD_X + scroll_x).max(0.0);
    let local_y = (click_y - bounds_y - TEXT_PAD_Y + scroll_y).max(0.0);
    let shape_width = (bounds_w - TEXT_PAD_X * 2.0).max(0.0);
    let mut sys = editor_hit_tester()
        .lock()
        .expect("editor hit-tester poisoned");
    sys.byte_at(text, font_size, shape_width, local_x, local_y)
}

/// Monotonic millis since program start. Drives the editor's
/// multi-click cascade so double/triple clicks register correctly.
fn click_millis() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = *EPOCH.get_or_init(Instant::now);
    epoch.elapsed().as_millis() as u64
}

/// Pointer-down on the code-editor body. Sets keyboard focus, then
/// resolves the click to a byte via cosmic-text. Modifier-aware:
/// shift held extends the existing selection. Multi-click cascade
/// turns 2 / 3 clicks at the same byte within ~500 ms into word /
/// line selection. Plain single clicks open a drag — pointer-move +
/// pointer-up close the loop through
/// `route_code_editor_body_drag` / `_release`.
fn route_code_editor_body_press(
    inner: &Rc<RefCell<ShellInner>>,
    hit: &HitRect,
    x: f32,
    y: f32,
    modifiers: prism_ui_runtime::event::Modifiers,
) -> bool {
    if attr_value(hit, "data-role") != Some("code-editor-body") {
        return false;
    }
    const FONT_SIZE: f32 = 13.0;
    let mut guard = inner.borrow_mut();
    let ShellInner {
        state, registry, ..
    } = &mut *guard;
    state.cancel_field_focus(Some(registry.as_component_registry()));
    state.code_editor_focused = true;
    let text = state.canvas.code_buffer.source().to_string();
    let scroll_x = state.canvas.code_buffer.scroll_x;
    let scroll_y = state.canvas.code_buffer.scroll_y;
    let byte = resolve_editor_byte_at(
        &text,
        hit.bounds.x,
        hit.bounds.y,
        hit.bounds.width,
        x,
        y,
        scroll_x,
        scroll_y,
        FONT_SIZE,
    );
    let now = click_millis();
    let click_kind = state.canvas.code_buffer.editor.register_click(byte, now);
    use prism_ui_runtime::editor::ClickKind;
    match click_kind {
        ClickKind::Single => {
            state
                .canvas
                .code_buffer
                .editor
                .place_caret_at(byte, modifiers.shift);
            // Shift-click extends a selection — don't start a drag.
            state.editor_drag = if modifiers.shift {
                None
            } else {
                Some(crate::state::EditorDrag {
                    anchor_byte: byte,
                    bounds_x: hit.bounds.x,
                    bounds_y: hit.bounds.y,
                    bounds_w: hit.bounds.width,
                    scroll_x,
                    scroll_y,
                    font_size: FONT_SIZE,
                })
            };
        }
        ClickKind::DoubleWord => {
            state.canvas.code_buffer.editor.select_word_at(byte);
            state.editor_drag = None;
        }
        ClickKind::TripleLine => {
            state.canvas.code_buffer.editor.select_line_at(byte);
            state.editor_drag = None;
        }
    }
    true
}

/// Pointer-move while an editor drag is in flight. Resolves the
/// move position to a byte using the drag's anchored bounds + the
/// current buffer text, then extends the selection so the editor's
/// caret jumps to the new byte while the anchor stays put.
fn route_code_editor_body_drag(inner: &Rc<RefCell<ShellInner>>, x: f32, y: f32) -> bool {
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let Some(drag) = g.state.editor_drag.clone() else {
        return false;
    };
    let text = g.state.canvas.code_buffer.source().to_string();
    let byte = resolve_editor_byte_at(
        &text,
        drag.bounds_x,
        drag.bounds_y,
        drag.bounds_w,
        x,
        y,
        drag.scroll_x,
        drag.scroll_y,
        drag.font_size,
    );
    // First move after press: anchor at the click byte so subsequent
    // extends pivot from there. (The Single arm above placed the
    // caret without an anchor.)
    if g.state.canvas.code_buffer.editor.selection().is_none() {
        g.state
            .canvas
            .code_buffer
            .editor
            .place_caret_at(drag.anchor_byte, false);
    }
    g.state.canvas.code_buffer.editor.place_caret_at(byte, true);
    true
}

/// Pointer-up — drop any active editor drag.
fn route_code_editor_body_release(inner: &Rc<RefCell<ShellInner>>) -> bool {
    inner.borrow_mut().state.editor_drag.take().is_some()
}

/// Pointer-down on an editor tab → switch active to that tab.
fn handle_editor_tab_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(idx_str) = attr_value(hit, "data-tab-index") else {
        return false;
    };
    let Ok(idx) = idx_str.parse::<usize>() else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    // Switching tabs implies the editor wants keyboard focus too.
    guard.state.code_editor_focused = true;
    guard.state.canvas.switch_editor_tab(idx)
}

/// Pointer-down on a tab's close button → close that specific tab.
/// If the closed tab is the active one, the next tab slides in;
/// otherwise the active slot stays where it is. The hit's
/// `data-tab-index` carries the target.
fn handle_editor_tab_close_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(idx_str) = attr_value(hit, "data-tab-index") else {
        return false;
    };
    let Ok(idx) = idx_str.parse::<usize>() else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    if idx == g.state.canvas.code_active_tab {
        g.state.canvas.close_active_editor_tab()
    } else {
        // Close an inactive tab — translate the display index to a
        // `code_tabs` slot and remove it. The active slot stays put.
        let pop_at = if idx < g.state.canvas.code_active_tab {
            idx
        } else {
            idx.saturating_sub(1)
        };
        if pop_at >= g.state.canvas.code_tabs.len() {
            return false;
        }
        g.state.canvas.code_tabs.remove(pop_at);
        if idx < g.state.canvas.code_active_tab {
            g.state.canvas.code_active_tab = g.state.canvas.code_active_tab.saturating_sub(1);
        }
        true
    }
}

/// `+` button next to the tab list → open a fresh Untitled tab.
fn handle_editor_tab_new_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    let mut guard = inner.borrow_mut();
    guard.state.canvas.new_editor_tab();
    guard.state.code_editor_focused = true;
    true
}

/// Process-wide help registry for editor hover. Loaded lazily on
/// first hover; entries cover Luau / Rust / JavaScript keywords.
/// `OnceLock` (not `Mutex<OnceLock>`) — the registry is read-only
/// after construction.
fn editor_help_registry() -> &'static prism_core::HelpRegistry {
    use std::sync::OnceLock;
    static REG: OnceLock<prism_core::HelpRegistry> = OnceLock::new();
    REG.get_or_init(crate::editor_help::editor_help_registry)
}

/// Pointer-move over the code editor — resolve the byte under the
/// pointer to a token and, if the lexeme has a help entry, push a
/// tooltip into `state.overlay.help_tooltip`. Misses clear the
/// tooltip so it doesn't linger after the pointer leaves a keyword.
fn route_code_editor_body_hover(
    inner: &Rc<RefCell<ShellInner>>,
    hit: Option<&HitRect>,
    x: f32,
    y: f32,
) {
    let Some(h) = hit.filter(|h| attr_value(h, "data-role") == Some("code-editor-body")) else {
        // Not over an editor body — if we have an editor tooltip up,
        // clear it. We tag editor tooltips via a sentinel title
        // prefix so we don't clobber unrelated help (palette help,
        // help-menu hover, …).
        let mut guard = inner.borrow_mut();
        if let Some(tip) = &guard.state.overlay.help_tooltip {
            if tip.title.starts_with("editor:") {
                guard.state.overlay.help_tooltip = None;
            }
        }
        return;
    };
    const FONT_SIZE: f32 = 13.0;
    let (text, language, scroll_x, scroll_y) = {
        let g = inner.borrow();
        (
            g.state.canvas.code_buffer.source().to_string(),
            g.state.canvas.code_buffer.language.clone(),
            g.state.canvas.code_buffer.scroll_x,
            g.state.canvas.code_buffer.scroll_y,
        )
    };
    let byte = resolve_editor_byte_at(
        &text,
        h.bounds.x,
        h.bounds.y,
        h.bounds.width,
        x,
        y,
        scroll_x,
        scroll_y,
        FONT_SIZE,
    );
    let lang = if language.is_empty() {
        "luau"
    } else {
        language.as_str()
    };
    let Some((_, lexeme)) = prism_ui_runtime::syntax::token_at(&text, lang, byte) else {
        let mut guard = inner.borrow_mut();
        if let Some(tip) = &guard.state.overlay.help_tooltip {
            if tip.title.starts_with("editor:") {
                guard.state.overlay.help_tooltip = None;
            }
        }
        return;
    };
    let Some(help_id) = crate::editor_help::help_id_for(lang, lexeme) else {
        return;
    };
    let entry = editor_help_registry().get(&help_id);
    let mut guard = inner.borrow_mut();
    match entry {
        Some(entry) => {
            // Stamp `editor:` onto the title so the "clear when
            // hover leaves the editor" path above can distinguish
            // our tooltips from other consumers'.
            guard.state.overlay.help_tooltip = Some(crate::state::HelpTooltip {
                title: format!("editor:{}", entry.title),
                summary: entry.summary.clone(),
            });
        }
        None => {
            if let Some(tip) = &guard.state.overlay.help_tooltip {
                if tip.title.starts_with("editor:") {
                    guard.state.overlay.help_tooltip = None;
                }
            }
        }
    }
}

/// Wave 2.4 HSL — `data-role="color-hsl-slider"` press. Reads
/// `data-channel` (one of `h` / `s` / `l`), computes the normalised
/// position the click landed at `(event.x - bounds.x) / bounds.width`,
/// converts the picker's current hex through `rgb_to_hsl`, replaces
/// the targeted channel, and writes the new hex back through
/// `set_color_picker_value`. Returns `true` when the bound prop
/// actually moved.
fn route_color_slider_press(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect, x: f32) -> bool {
    if attr_value(hit, "data-role") != Some("color-hsl-slider") {
        return false;
    }
    let Some(channel_attr) = attr_value(hit, "data-channel") else {
        return false;
    };
    let Some(channel) = crate::state::ColorChannel::from_attr(channel_attr) else {
        return false;
    };
    let track_x = hit.bounds.x;
    let track_width = hit.bounds.width;
    let mut guard = inner.borrow_mut();
    guard.state.overlay.color_picker.slider_drag = Some(crate::state::ColorSliderDrag {
        channel,
        track_x,
        track_width,
    });
    drop(guard);
    apply_color_slider_at(inner, x)
}

/// Wave 2.4 HSL — recompute the captured channel from the live
/// pointer-x and write the new hex through `set_color_picker_value`.
/// Called once from the press handler and again from every
/// pointer-move while the drag is active.
fn apply_color_slider_at(inner: &Rc<RefCell<ShellInner>>, x: f32) -> bool {
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let Some(drag) = g.state.overlay.color_picker.slider_drag.clone() else {
        return false;
    };
    let width = drag.track_width.max(1.0);
    let fraction = ((x - drag.track_x) / width).clamp(0.0, 1.0);
    let current = g.state.overlay.color_picker.value.clone();
    let parsed =
        prism_builder::color::parse_hex(&current).unwrap_or(prism_builder::color::Rgba::BLACK);
    let (mut h, mut s, mut l) = prism_builder::color::rgb_to_hsl(parsed);
    match drag.channel {
        crate::state::ColorChannel::Hue => h = fraction * 360.0,
        crate::state::ColorChannel::Saturation => s = fraction * 100.0,
        crate::state::ColorChannel::Lightness => l = fraction * 100.0,
    }
    let next = prism_builder::color::hsl_to_rgb(h, s, l, parsed.a);
    let hex = prism_builder::color::format_hex(next);
    let registry = g.registry.as_component_registry();
    g.state.set_color_picker_value(&hex, Some(registry))
}

fn handle_file_browse_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let Some(key) = attr_value(hit, "data-key") else {
        return false;
    };
    let accept = attr_value(hit, "data-accept");
    let mut spec = crate::services::FilePickerSpec::open("Choose a file");
    if let Some(accept) = accept {
        spec = spec.with_accept_attr(accept);
    }
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let picked = match g.vfs.pick_file(&spec) {
        Ok(paths) if !paths.is_empty() => paths,
        _ => return false,
    };
    let path_str = picked[0].to_string_lossy().into_owned();
    let registry = g.registry.as_component_registry();
    g.state.set_node_prop(
        target,
        key,
        serde_json::Value::String(path_str),
        Some(registry),
    )
}

/// Wave 3.2: hit-tests for whether a pointer-down landed on the
/// canvas (any of the canvas's tagged frames, the page rect, the
/// preview layer, or any preview node). When the catalog has a
/// palette item armed, this opens a palette drag at the cursor.
/// Returns true when the drag actually started.
fn route_palette_drag_begin(
    inner: &Rc<RefCell<ShellInner>>,
    hit: &HitRect,
    x: f32,
    y: f32,
) -> bool {
    if !is_canvas_hit(hit) {
        return false;
    }
    let target = attr_value(hit, "data-canvas-node").map(str::to_string);
    inner
        .borrow_mut()
        .state
        .begin_palette_drag(x, y, target.as_deref())
}

/// Wave 3.2: a hit lands "on the canvas" when it sits inside the
/// canvas frame — either the frame container itself, the page
/// rect, the preview layer's host container, or any container the
/// preview-tagging pass marked with `data-canvas-node`. The four
/// chrome roles cover the empty-canvas drop case; the
/// canvas-node attr covers the "drop under an existing node" case.
fn is_canvas_hit(hit: &HitRect) -> bool {
    if attr_value(hit, "data-canvas-node").is_some() {
        return true;
    }
    matches!(
        attr_value(hit, "data-role"),
        Some("builder-canvas")
            | Some("canvas-page")
            | Some("canvas-preview")
            | Some("canvas-overlay")
    )
}

/// Wave 3.4: a right-click on a canvas-resident hit opens the
/// context menu populated with that node's actions. A right-click
/// on the empty canvas falls back to the document-level actions
/// (paste-from-clipboard). Returns true when the menu actually
/// opened.
fn route_context_menu_open(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect, x: f32, y: f32) -> bool {
    if !is_canvas_hit(hit) {
        return false;
    }
    let target = attr_value(hit, "data-canvas-node").map(str::to_string);
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    g.state
        .open_context_menu(x, y, target.as_deref(), Some(registry))
}

/// Wave 3.4: a primary click anywhere except a menu-item dismisses
/// an open context menu. Menu-item clicks reach `route_on_click`
/// (which routes through `data-on-click="cmd <id>"`) before this
/// check, so the activation path stays the priority.
fn route_context_menu_dismiss(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    // Don't dismiss while the click is inside the menu itself —
    // the dispatch on the item will close it (or leave it open
    // when the item is a non-activating separator).
    let inside_menu = matches!(
        attr_value(hit, "data-role"),
        Some("context-menu") | Some("menu-dropdown")
    ) || attr_value(hit, "role") == Some("menuitem");
    if inside_menu {
        return false;
    }
    let mut guard = inner.borrow_mut();
    guard.state.close_context_menu()
}

fn attr_value<'a>(hit: &'a HitRect, key: &str) -> Option<&'a str> {
    hit.attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::Shell;
    use prism_ui_runtime::command::Rect;
    use prism_ui_runtime::event::Modifiers;

    fn hit_with(role: &str, target: &str, extra: &[(&str, &str)]) -> HitRect {
        let mut attrs = vec![
            ("data-role".to_string(), role.into()),
            ("data-target-id".to_string(), target.into()),
        ];
        for (k, v) in extra {
            attrs.push(((*k).into(), (*v).into()));
        }
        HitRect {
            id: format!("hit-{role}"),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            attrs,
        }
    }

    #[test]
    fn resize_updates_viewport_and_requests_redraw() {
        let shell = Shell::new().expect("boot");
        let dirty = dispatch_event(
            &shell.inner,
            &Event::Resize {
                width: 1024,
                height: 600,
            },
            None,
        );
        assert!(dirty, "resize must request a redraw");
        let vp = shell.inner.borrow().viewport;
        assert_eq!(vp.width, 1024.0);
        assert_eq!(vp.height, 600.0);
    }

    #[test]
    fn unhandled_events_are_no_redraw() {
        let shell = Shell::new().expect("boot");
        let dirty = dispatch_event(&shell.inner, &Event::Wheel { dx: 0.0, dy: 1.0 }, None);
        assert!(!dirty);
    }

    #[test]
    fn pointer_events_route_through_canvas_slot_under_active_tool() {
        // §22 keystone: the router knows pointer phases, the slot
        // knows the tool. A down/move/up trio against a populated
        // canvas mutates the document via `apply_gizmo_delta` without
        // the router ever growing tool-mode awareness.
        use prism_builder::{BuilderDocument, Node};
        use prism_core::foundation::spatial::Transform2D;
        use prism_ui_runtime::event::PointerButton;

        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.canvas.document = BuilderDocument {
                root: Some(Node {
                    id: "root".into(),
                    component: "container".into(),
                    transform: Transform2D {
                        position: [100.0, 100.0],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                ..Default::default()
            };
            guard.state.canvas.selection = Some("root".into());
            guard.state.canvas.tool = crate::state::ToolMode::Move;
        }
        let down = Event::PointerDown {
            x: 100.0,
            y: 100.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        };
        let mv = Event::PointerMove {
            x: 160.0,
            y: 140.0,
            modifiers: Modifiers::default(),
        };
        let up = Event::PointerUp {
            x: 160.0,
            y: 140.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        };
        assert!(
            !dispatch_event(&shell.inner, &down, None),
            "capture is silent"
        );
        assert!(
            dispatch_event(&shell.inner, &mv, None),
            "move triggers redraw"
        );
        assert!(
            dispatch_event(&shell.inner, &up, None),
            "up triggers redraw"
        );
        let pos = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .transform
            .position;
        assert_eq!(pos, [160.0, 140.0]);
    }

    #[test]
    fn pointer_down_on_inspector_row_selects_target_node() {
        // §43 C3: a hit on a `data-role="inspector-row"` container
        // moves the canvas selection to its `data-target-id` and
        // resyncs the builder slot. The boot seed pre-selects
        // `demo-heading`; this test asserts the selection moves to
        // `demo-paragraph` after the click.
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        assert_eq!(
            shell.inner.borrow().state.canvas.selection.as_deref(),
            Some("demo-heading"),
            "boot seed pre-selects the heading"
        );
        let hit = hit_with("inspector-row", "demo-paragraph", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "selection mutation requests a redraw");
        assert_eq!(
            shell.inner.borrow().state.canvas.selection.as_deref(),
            Some("demo-paragraph"),
        );
    }

    #[test]
    fn pointer_down_on_boolean_field_edit_toggles_prop() {
        // §43 C2: a hit on a `data-role="field-edit"` boolean row
        // toggles the bound prop on the target doc node. The seed
        // document has a `demo-button` with no `visible` prop yet;
        // a click sets it to `false`. A second click flips it back
        // to `true`.
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = hit_with(
            "field-edit",
            "demo-button",
            &[
                ("data-key", "visible"),
                ("data-kind", "boolean"),
                ("data-value", "true"),
            ],
        );
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        let visible = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("demo-button"))
            .and_then(|n| n.props.get("visible").cloned())
            .expect("visible prop set");
        assert_eq!(visible, serde_json::Value::Bool(false));
    }

    /// Wave 2.3 — pointer-down on a `data-role="field-edit"` hit
    /// whose kind is `select` opens the anchored dropdown overlay
    /// (replaces the legacy click-to-cycle behaviour). The dropdown's
    /// option rows route through `select-dropdown-option` for the
    /// actual commit (see the next test for that contract).
    #[test]
    fn pointer_down_on_select_field_edit_opens_dropdown_overlay() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = hit_with(
            "field-edit",
            "demo-heading",
            &[
                ("data-key", "level"),
                ("data-kind", "select"),
                ("data-value", "h1"),
                ("data-options", "h1,h2,h3"),
            ],
        );
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        let dropdown = shell.inner.borrow().state.overlay.select_dropdown.clone();
        assert!(dropdown.open);
        assert_eq!(dropdown.target_id, "demo-heading");
        assert_eq!(dropdown.key, "level");
        assert_eq!(dropdown.value, "h1");
        assert_eq!(dropdown.options.len(), 3);
        // Verify the bare-value parsing branch (no `:label`) — when
        // `data-options` carries plain values they round-trip with
        // `value == label`.
        let first = dropdown.options[0]
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(first, "h1");
    }

    /// Wave 2.3 — pointer-down on a `data-role="select-dropdown-option"`
    /// commits the option's `data-value` through `set_node_prop` and
    /// closes the dropdown.
    #[test]
    fn pointer_down_on_select_dropdown_option_commits_and_closes() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        shell.inner.borrow_mut().state.open_select_dropdown(
            "demo-heading",
            "level",
            "h1",
            vec![
                serde_json::json!({ "value": "h1", "label": "H1" }),
                serde_json::json!({ "value": "h2", "label": "H2" }),
            ],
        );
        let hit = hit_with(
            "select-dropdown-option",
            "ignored-target",
            &[("data-value", "h2")],
        );
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        let guard = shell.inner.borrow();
        assert!(!guard.state.overlay.select_dropdown.open);
        let level = guard
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("demo-heading"))
            .and_then(|n| n.props.get("level").cloned())
            .expect("level prop set");
        assert_eq!(level, serde_json::Value::String("h2".into()));
    }

    /// Wave 2.3 — pointer-down on `data-role="select-dropdown-close"`
    /// dismisses the overlay without committing.
    #[test]
    fn pointer_down_on_select_dropdown_close_dismisses_overlay() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        shell.inner.borrow_mut().state.open_select_dropdown(
            "demo-heading",
            "level",
            "h1",
            vec![serde_json::json!({ "value": "h1", "label": "H1" })],
        );
        let hit = hit_with("select-dropdown-close", "", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        assert!(!shell.inner.borrow().state.overlay.select_dropdown.open);
    }

    /// Wave 2.4 — pointer-down on a `data-role="color-swatch"` hit
    /// opens the color-picker overlay against the swatch's
    /// (target-id, key, value) triple. The overlay is rendered by
    /// the bound `shell.color-picker` block on the next frame.
    #[test]
    fn pointer_down_on_color_swatch_opens_color_picker() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = hit_with(
            "color-swatch",
            "demo-heading",
            &[("data-key", "color"), ("data-value", "#ff0000")],
        );
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        let picker = &shell.inner.borrow().state.overlay.color_picker;
        assert!(picker.open);
        assert_eq!(picker.target_id, "demo-heading");
        assert_eq!(picker.key, "color");
        assert_eq!(picker.value, "#ff0000");
    }

    /// Wave 2.4 — pointer-down on a `data-role="color-preset-select"`
    /// hit commits the preset's `data-color` through `set_node_prop`,
    /// leaving the picker open for further preview.
    #[test]
    fn pointer_down_on_color_preset_commits_through_set_node_prop() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        shell
            .inner
            .borrow_mut()
            .state
            .open_color_picker("demo-heading", "color", "#000000");
        let hit = hit_with(
            "color-preset-select",
            "ignored-target",
            &[("data-color", "#0060c0")],
        );
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        let guard = shell.inner.borrow();
        // Picker stays open so the user can preview multiple presets.
        assert!(guard.state.overlay.color_picker.open);
        assert_eq!(guard.state.overlay.color_picker.value, "#0060c0");
        // Doc node received the new color.
        let color = guard
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("demo-heading"))
            .and_then(|n| n.props.get("color").cloned())
            .expect("color prop set");
        assert_eq!(color, serde_json::Value::String("#0060c0".into()));
    }

    /// Wave 2.4 — pointer-down on `data-role="color-picker-close"`
    /// dismisses the overlay without committing.
    #[test]
    fn pointer_down_on_color_picker_close_dismisses_overlay() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        shell
            .inner
            .borrow_mut()
            .state
            .open_color_picker("demo-heading", "color", "#000000");
        let hit = hit_with("color-picker-close", "", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        assert!(!shell.inner.borrow().state.overlay.color_picker.open);
    }

    /// Number field-edit shape under the B4 drag-scrubber: pointer-down
    /// opens a scrub session (no prop write yet), pointer-up with no
    /// pointer-move in between falls through to the legacy `+1` step.
    /// The test sends the full {down, up} pair to exercise the click
    /// path through to the prop mutation.
    #[test]
    fn click_on_number_field_edit_increments_clamped_to_max() {
        use prism_builder::Node;
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            let g = &mut *guard;
            let root = g.state.canvas.document.root.as_mut().expect("canvas root");
            root.children.push(Node {
                id: "num-target".into(),
                component: prism_builder::ComponentId::from("text"),
                props: serde_json::json!({ "count": 4.0 }),
                children: vec![],
                layout_mode: Default::default(),
                transform: Default::default(),
                modifiers: vec![],
                style: Default::default(),
            });
        }
        let hit = hit_with(
            "field-edit",
            "num-target",
            &[
                ("data-key", "count"),
                ("data-kind", "number"),
                ("data-value", "4"),
                ("data-max", "5"),
            ],
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit.clone()),
        );
        // Pointer-up at the same coordinates → no drag → click step.
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerUp {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        let count = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("num-target"))
            .and_then(|n| n.props.get("count").cloned())
            .expect("count prop set");
        assert_eq!(count.as_f64(), Some(5.0));
        // Click again: hit value attribute still reflects pre-state but
        // dispatch uses the attr's value — re-fire confirms the clamp
        // sticks (5+1 → max-clamped to 5).
        let hit2 = hit_with(
            "field-edit",
            "num-target",
            &[
                ("data-key", "count"),
                ("data-kind", "number"),
                ("data-value", "5"),
                ("data-max", "5"),
            ],
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit2.clone()),
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerUp {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit2),
        );
        let count2 = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("num-target"))
            .and_then(|n| n.props.get("count").cloned())
            .expect("count prop set");
        assert_eq!(count2.as_f64(), Some(5.0));
    }

    #[test]
    fn click_on_integer_field_edit_emits_integer_json() {
        use prism_builder::Node;
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            let g = &mut *guard;
            let root = g.state.canvas.document.root.as_mut().expect("canvas root");
            root.children.push(Node {
                id: "int-target".into(),
                component: prism_builder::ComponentId::from("text"),
                props: serde_json::json!({ "ord": 2 }),
                children: vec![],
                layout_mode: Default::default(),
                transform: Default::default(),
                modifiers: vec![],
                style: Default::default(),
            });
        }
        let hit = hit_with(
            "field-edit",
            "int-target",
            &[
                ("data-key", "ord"),
                ("data-kind", "integer"),
                ("data-value", "2"),
            ],
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit.clone()),
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerUp {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        let ord = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("int-target"))
            .and_then(|n| n.props.get("ord").cloned())
            .expect("ord prop set");
        // Integer kind keeps it integral, not a float — important because
        // serde round-trips treat the two differently.
        assert_eq!(ord, serde_json::Value::from(3i64));
    }

    /// Drag-scrub variant: pointer-down opens a session, pointer-move
    /// past the threshold mutates the prop, pointer-up ends the
    /// session *without* dispatching the click-step (because `moved`
    /// is true).
    #[test]
    fn drag_on_number_field_edit_scrubs_value_proportional_to_delta() {
        use prism_builder::Node;
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            let g = &mut *guard;
            let root = g.state.canvas.document.root.as_mut().expect("canvas root");
            root.children.push(Node {
                id: "scrub-target".into(),
                component: prism_builder::ComponentId::from("text"),
                props: serde_json::json!({ "count": 10.0 }),
                children: vec![],
                layout_mode: Default::default(),
                transform: Default::default(),
                modifiers: vec![],
                style: Default::default(),
            });
        }
        let hit = HitRect {
            id: "fe".into(),
            bounds: Rect {
                x: 100.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![
                ("data-role".into(), "field-edit".into()),
                ("data-target-id".into(), "scrub-target".into()),
                ("data-key".into(), "count".into()),
                ("data-kind".into(), "number".into()),
                ("data-value".into(), "10".into()),
            ],
        };
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 140.0,
                y: 12.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit.clone()),
        );
        // Move +40px → 40/4 = +10 → value should be 20.0.
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerMove {
                x: 180.0,
                y: 12.0,
                modifiers: Modifiers::default(),
            },
            Some(hit.clone()),
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerUp {
                x: 180.0,
                y: 12.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        let count = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("scrub-target"))
            .and_then(|n| n.props.get("count").cloned())
            .expect("count prop set");
        assert_eq!(count.as_f64(), Some(20.0));
        // Drag session must have ended.
        assert!(shell.inner.borrow().state.number_drag.is_none());
    }

    #[test]
    fn click_on_text_field_edit_opens_focus_session() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = hit_with(
            "field-edit",
            "demo-heading",
            &[
                ("data-key", "body"),
                ("data-kind", "text"),
                ("data-value", "Welcome to Studio"),
            ],
        );
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "opening focus requests a redraw");
        let focus = shell
            .inner
            .borrow()
            .state
            .field_focus
            .clone()
            .expect("focus opened");
        assert_eq!(focus.target_id, "demo-heading");
        assert_eq!(focus.key, "body");
        assert_eq!(focus.kind, "text");
        // Original is the current prop value — restored on Esc.
        assert_eq!(focus.original, "Welcome to Studio");
    }

    #[test]
    fn click_elsewhere_commits_active_focus_session() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        // Open focus on the heading body.
        let open = hit_with(
            "field-edit",
            "demo-heading",
            &[
                ("data-key", "body"),
                ("data-kind", "text"),
                ("data-value", "Welcome to Studio"),
            ],
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(open),
        );
        assert!(shell.inner.borrow().state.field_focus.is_some());
        // Click on an unrelated chrome row.
        let blur_hit = hit_with("inspector-row", "demo-paragraph", &[]);
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(blur_hit),
        );
        assert!(
            shell.inner.borrow().state.field_focus.is_none(),
            "clicking elsewhere commits + clears the focus"
        );
    }

    #[test]
    fn pointer_down_with_unknown_role_falls_through_to_canvas() {
        // A hit with no recognised `data-role` doesn't mutate
        // selection / props; the §22 canvas path still gets to
        // attempt a drag capture.
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let baseline = shell.inner.borrow().state.canvas.selection.clone();
        let hit = hit_with("toolbar-align-cluster", "noop", &[]);
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert_eq!(shell.inner.borrow().state.canvas.selection, baseline);
    }

    /// §43 E3 — the named end-to-end verification script.
    ///
    /// Mirrors the user-facing flow: "click Heading in palette, drop
    /// in canvas, click it, edit text in properties → tree updates."
    ///
    /// Drives a booted `Shell` through the same code paths the
    /// femtovg backend hits at runtime:
    ///
    /// 1. **Palette pick** — set `palette_selected` to the chosen
    ///    builtin id. The component-palette block reads this on the
    ///    next frame and paints the selected pill.
    /// 2. **Drop in canvas** — call `insert_at_offset` to add a new
    ///    `text` heading node. This is the data mutation a future
    ///    palette→canvas drop router will call once the wiring lands;
    ///    the underlying mutator is the contract.
    /// 3. **Click it** — dispatch a synthetic `PointerDown` with a
    ///    `data-role="inspector-row"` hit targeting the new node.
    ///    The §43 C3 router fans this through `select_node`, which
    ///    moves `canvas.selection` and re-derives the inspector +
    ///    properties.
    /// 4. **Edit in properties** — dispatch a synthetic `PointerDown`
    ///    with a `data-role="field-edit"` boolean hit. The §43 C2
    ///    router toggles the bound prop on the selected doc node.
    ///    (Text-kind edit UX is deferred; the boolean toggle proves
    ///    the dispatch chain end-to-end.)
    /// 5. **Tree updates** — the inspector tree carries the new node;
    ///    the properties form rebuilt against its schema; the edited
    ///    prop made it through to `doc.find(id).props`.
    #[test]
    fn e2e_palette_pick_drop_select_edit_updates_tree() {
        use prism_builder::Node;
        use prism_ui_runtime::event::PointerButton;
        use serde_json::json;

        let shell = Shell::new().expect("boot");

        // §43 D1 landed the registry merge in `Shell::new` itself,
        // so the boot resync already populates the right rail for
        // the pre-selected `demo-heading`. This used to require a
        // hand-rolled `for spec in BUILTINS` extension here.
        assert!(
            !shell.inner.borrow().state.builder.property_rows.is_empty(),
            "boot resync with builder builtins populates the right rail"
        );

        let initial_count = shell.inner.borrow().state.canvas.node_count();
        let new_id = "e2e-heading";

        // 1. Palette pick — the palette item "text" is the `Heading`
        //    family in the seed (heading-shaped `text` node with
        //    `level: h1`).
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.catalog.palette_selected = Some("text".into());
        }
        assert_eq!(
            shell
                .inner
                .borrow()
                .state
                .catalog
                .palette_selected
                .as_deref(),
            Some("text"),
            "palette pick records the selected builtin id"
        );

        // 2. Drop in canvas — insert a new text node into the root.
        //    The seed pre-selects `demo-heading` so the §43-style
        //    insert-after-selection lands the new node as a sibling.
        let drop_value = serde_json::to_value(Node {
            id: new_id.into(),
            component: "text".into(),
            props: json!({ "body": "Drop heading", "level": "h1" }),
            children: Vec::new(),
            layout_mode: Default::default(),
            transform: Default::default(),
            modifiers: Vec::new(),
            style: Default::default(),
        })
        .expect("serialize node");
        let inserted = {
            let mut guard = shell.inner.borrow_mut();
            guard.state.canvas.insert_at_offset(drop_value, 0)
        };
        assert!(inserted.is_some(), "drop must succeed");
        // The mutator may rewrite the id to dodge collisions; capture
        // whatever id it returned for the downstream steps.
        let landed_id = inserted.unwrap();
        assert_eq!(
            shell.inner.borrow().state.canvas.node_count(),
            initial_count + 1,
            "node-count rises by one after the drop"
        );

        // After the drop the right rail still reflects the
        // pre-selected `demo-heading`. Run one resync against the live
        // registry so the inspector tree picks up the freshly-inserted
        // row before the click finds it.
        {
            let mut guard = shell.inner.borrow_mut();
            let g = &mut *guard;
            let registry = g.registry.as_component_registry();
            g.state.resync_builder_for_selection(Some(registry));
        }
        assert!(
            shell
                .inner
                .borrow()
                .state
                .builder
                .inspector
                .iter()
                .any(|n| n.id == landed_id),
            "inspector tree must include the new node"
        );

        // 3. Click it — synthetic inspector-row PointerDown.
        let click_hit = hit_with("inspector-row", &landed_id, &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(click_hit),
        );
        assert!(dirty, "inspector-row click requests a redraw");
        assert_eq!(
            shell.inner.borrow().state.canvas.selection.as_deref(),
            Some(landed_id.as_str()),
            "selection moves to the newly-dropped node"
        );

        // Property rows now match the `text` schema for the new node.
        {
            let guard = shell.inner.borrow();
            let rows = &guard.state.builder.property_rows;
            assert!(
                !rows.is_empty(),
                "property rows must rebuild for the new selection"
            );
            assert_eq!(rows[0].component, "shell.section-header");
            let keys: Vec<&str> = rows
                .iter()
                .filter(|r| r.component == "shell.field-editor")
                .filter_map(|r| r.props.get("key").and_then(|v| v.as_str()))
                .collect();
            assert!(
                keys.contains(&"body"),
                "text schema must include `body`, got {keys:?}"
            );
        }

        // 4. Edit in properties — synthetic field-edit PointerDown.
        //    Boolean kind is the only kind wired through the event
        //    router today; the underlying `set_node_prop` is the same
        //    mutator that text/number/select edits will call when
        //    their UX lands. We target an arbitrary `visible` key —
        //    not on the `text` schema, so the write proves the
        //    mutator path through pure dispatch.
        let edit_hit = hit_with(
            "field-edit",
            &landed_id,
            &[
                ("data-key", "visible"),
                ("data-kind", "boolean"),
                ("data-value", "true"),
            ],
        );
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(edit_hit),
        );
        assert!(dirty, "field-edit click requests a redraw");

        // 5. Tree updates — the prop write reached the doc node, and
        //    the inspector still flags it as selected.
        let guard = shell.inner.borrow();
        let landed = guard
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find(&landed_id))
            .expect("new node still in tree");
        assert_eq!(
            landed.props.get("visible").cloned(),
            Some(serde_json::Value::Bool(false)),
            "field-edit toggle landed on the doc node"
        );
        let selected_row = guard
            .state
            .builder
            .inspector
            .iter()
            .find(|n| n.selected)
            .expect("inspector still flags a selection");
        assert_eq!(
            selected_row.id, landed_id,
            "selected inspector row is the edited node"
        );
    }

    /// §43 B5: a pointer-down on a `data-canvas-node`-tagged container
    /// moves the canvas selection to that node and re-derives the
    /// inspector tree. The hit is shaped like the runtime would emit
    /// it for a button rendered inside the builder canvas.
    #[test]
    fn pointer_down_on_canvas_node_routes_to_select_node() {
        use prism_builder::Node;
        use prism_ui_runtime::event::PointerButton;
        use serde_json::json;

        let shell = Shell::new().expect("boot");

        // Extend the seeded canvas doc with a button sibling so we can
        // assert selection moves *between* nodes (the boot already
        // pre-selects `demo-heading`).
        let button_id = "canvas-click-target".to_string();
        {
            let mut guard = shell.inner.borrow_mut();
            let g = &mut *guard;
            let root = g.state.canvas.document.root.as_mut().expect("canvas root");
            root.children.push(Node {
                id: button_id.clone(),
                component: prism_builder::ComponentId::from("button"),
                props: json!({ "label": "click me" }),
                children: vec![],
                layout_mode: Default::default(),
                transform: Default::default(),
                modifiers: vec![],
                style: Default::default(),
            });
        }
        // Sanity-check the pre-condition: the boot selection is
        // `demo-heading`, not our new button.
        assert_ne!(
            shell.inner.borrow().state.canvas.selection.as_deref(),
            Some(button_id.as_str()),
        );

        let hit = HitRect {
            id: button_id.clone(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![
                ("data-role".into(), "button".into()),
                ("data-canvas-node".into(), button_id.clone()),
            ],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 10.0,
                y: 10.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "canvas-node click moved the selection → redraw");
        assert_eq!(
            shell.inner.borrow().state.canvas.selection.as_deref(),
            Some(button_id.as_str()),
            "selection follows the clicked canvas node"
        );
    }

    /// §43 B5: a hit whose id matches a chrome container ("root" is
    /// the canonical collision — both `<shell.app-window>` and the
    /// canvas `BuilderDocument::page_shell()` use it) must NOT move
    /// the canvas selection. The `data-canvas-node` attribute is the
    /// disambiguator.
    #[test]
    fn pointer_down_on_chrome_container_does_not_select_canvas_node() {
        use prism_ui_runtime::event::PointerButton;

        let shell = Shell::new().expect("boot");
        let baseline = shell.inner.borrow().state.canvas.selection.clone();
        // The hit shape mirrors `<shell.app-window id="root">` — same id
        // as the canvas root, but no `data-canvas-node` attribute, so
        // routing must leave the selection alone.
        let hit = HitRect {
            id: "root".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 1280.0,
                height: 800.0,
            },
            attrs: vec![],
        };
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 100.0,
                y: 100.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert_eq!(
            shell.inner.borrow().state.canvas.selection,
            baseline,
            "chrome click left the canvas selection alone"
        );
    }

    /// §43 A1: a hit carrying `data-on-click="cmd <id>"` invokes the
    /// matching shell command. Verifies the parse → execute chain
    /// end-to-end through the same command table the palette uses.
    #[test]
    fn pointer_down_on_data_on_click_cmd_runs_the_shell_command() {
        use prism_ui_runtime::event::PointerButton;

        let shell = Shell::new().expect("boot");
        // `signals.fire-mounted` is a registered command (see
        // `SignalsService::commands`); pick it because it's
        // side-effect-light. Pre-state: command palette idle.
        // Post-condition: the command must be resolvable through
        // the shared CommandTable, which is what `route_on_click`
        // dispatches into. We don't observe state here — the
        // command's body fires `mounted` on the current selection,
        // which is fine.
        let hit = HitRect {
            id: "demo-heading".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![("data-on-click".into(), "cmd signals.fire-mounted".into())],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 10.0,
                y: 10.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        // The command ran (it requested a redraw via the standard
        // mutating path) — the precise downstream effects matter
        // less than proving the dispatch chain reached the command.
        assert!(
            dirty,
            "registered command dispatched through data-on-click should request a redraw"
        );
    }

    /// §43 A1: `data-on-click="emit <signal>"` fires the signal on
    /// the hit's container id, which cascades through any matching
    /// canvas-doc connections. We seed a single connection so the
    /// dispatch chain has a concrete sink: `clicked` on `btn` →
    /// `set-property visible=false` on `target`. After the click,
    /// the target node's `visible` prop should flip.
    #[test]
    fn pointer_down_on_data_on_click_emit_cascades_through_canvas_connections() {
        use prism_builder::{ActionKind, Connection, Node};
        use prism_ui_runtime::event::PointerButton;
        use serde_json::{json, Value};

        let shell = Shell::new().expect("boot");
        // Seed a button + target node + the connection that wires
        // the click to a visibility toggle. The exact action shape
        // doesn't matter — we just need an observable mutation we
        // can probe after `dispatch_event`.
        {
            let mut guard = shell.inner.borrow_mut();
            let g = &mut *guard;
            let root = g.state.canvas.document.root.as_mut().expect("canvas root");
            root.children.push(Node {
                id: "target".into(),
                component: prism_builder::ComponentId::from("container"),
                props: json!({ "visible": true }),
                children: vec![],
                layout_mode: Default::default(),
                transform: Default::default(),
                modifiers: vec![],
                style: Default::default(),
            });
            g.state.canvas.document.connections.push(Connection {
                id: "c-toggle".into(),
                source_node: "btn".into(),
                signal: "clicked".into(),
                target_node: "target".into(),
                action: ActionKind::SetProperty {
                    key: "visible".into(),
                    value: Value::Bool(false),
                },
                params: json!({}),
            });
        }
        let hit = HitRect {
            id: "btn".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![("data-on-click".into(), "emit clicked".into())],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 10.0,
                y: 10.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "an emit that matched a connection requests a redraw");
        let guard = shell.inner.borrow();
        let target = guard
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("target"))
            .expect("target still in tree");
        assert_eq!(
            target.props.get("visible"),
            Some(&Value::Bool(false)),
            "emit cascade ran the connection's SetProperty action"
        );
    }

    /// Wave 14.3 — modifier suffix parser splits a dash-joined chain
    /// into a typed bool set. Order doesn't matter; unknown segments
    /// drop silently.
    #[test]
    fn event_modifier_parser_recognises_known_segments() {
        assert_eq!(EventModifiers::parse(""), EventModifiers::default());
        assert_eq!(
            EventModifiers::parse("once"),
            EventModifiers {
                once: true,
                ..Default::default()
            }
        );
        assert_eq!(
            EventModifiers::parse("once-stop-prevent"),
            EventModifiers {
                once: true,
                stop: true,
                prevent: true,
            }
        );
        assert_eq!(
            EventModifiers::parse("stop-once"),
            EventModifiers {
                once: true,
                stop: true,
                ..Default::default()
            },
            "modifier segments are commutative",
        );
        assert_eq!(
            EventModifiers::parse("once-bogus"),
            EventModifiers {
                once: true,
                ..Default::default()
            },
            "unknown segments are dropped without affecting recognised ones",
        );
    }

    /// `.once` fires the handler the first time, then no-ops on every
    /// subsequent click against the same hit-id + attr-key pair. We
    /// observe through the `signals.fire-mounted` command — its
    /// per-frame redraw signal is the proxy for "did the handler run?"
    #[test]
    fn pointer_down_on_data_on_click_once_fires_only_first_time() {
        use prism_ui_runtime::event::PointerButton;

        let shell = Shell::new().expect("boot");
        let hit = || HitRect {
            id: "demo-once".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![(
                "data-on-click-once".into(),
                "cmd signals.fire-mounted".into(),
            )],
        };
        let press = || Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        };
        let dirty_first = dispatch_event(&shell.inner, &press(), Some(hit()));
        assert!(
            dirty_first,
            "first dispatch should fire the once-gated command"
        );
        // Once-fired registry should now contain the (hit-id, attr-key).
        assert!(shell
            .inner
            .borrow()
            .state
            .once_fired
            .contains(&("demo-once".to_string(), "data-on-click-once".to_string())));
        let dirty_second = dispatch_event(&shell.inner, &press(), Some(hit()));
        assert!(
            !dirty_second,
            "second dispatch must be a no-op — `.once` removes the handler"
        );
    }

    /// `.stop` returns `true` from `route_on_click` regardless of
    /// whether any connection fired — the rest of the pointer-down
    /// fallback chain (canvas selection, palette drag) is suppressed.
    /// We prove this by clicking a `data-on-click-stop` attr whose
    /// action emits a signal nobody subscribed to: without `.stop`
    /// that would fall through to the canvas. With `.stop`, the
    /// router consumes the press and reports dirty.
    #[test]
    fn pointer_down_on_data_on_click_stop_consumes_even_without_fire() {
        use prism_ui_runtime::event::PointerButton;

        let shell = Shell::new().expect("boot");
        let hit = HitRect {
            id: "demo-stop".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![(
                "data-on-click-stop".into(),
                // No connection wired — `fire_signal` returns 0.
                "emit nobody-listens".into(),
            )],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 10.0,
                y: 10.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(
            dirty,
            "`.stop` consumes the press so the router reports redraw"
        );
    }

    /// Bare `on:click` (no modifier) preserves the legacy semantics:
    /// when the action fires no observable mutation, the router
    /// returns `false` so the rest of the pointer-down chain runs.
    /// Pair with the `.stop` test above to prove the modifier is the
    /// thing that flipped the consume bit.
    #[test]
    fn pointer_down_on_bare_data_on_click_with_no_subscriber_falls_through() {
        use prism_ui_runtime::event::PointerButton;

        let shell = Shell::new().expect("boot");
        let hit = HitRect {
            id: "demo-bare".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![("data-on-click".into(), "emit nobody-listens".into())],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 10.0,
                y: 10.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        // Falls through to canvas pointer_down — which on an empty
        // doc + no selection is itself a no-op, so `dirty` stays
        // false. The contract under test is `route_on_click` returning
        // false; observing dirty is the visible witness.
        assert!(
            !dirty,
            "bare on:click without a connection nor canvas hit must not consume"
        );
    }

    /// Wave 14.3 — clicking an `<input>` whose `bind:value`
    /// resolves to `<node-id>.<key>` opens a field-focus session
    /// against that doc node. The subsequent `Event::Text` then
    /// flows through `FieldFocusService` and writes back to the
    /// node's prop via `set_node_prop`.
    #[test]
    fn pointer_down_on_input_with_bind_value_opens_field_focus() {
        use prism_builder::Node;
        use prism_ui_runtime::event::PointerButton;
        use serde_json::{json, Value};

        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            let root = guard
                .state
                .canvas
                .document
                .root
                .as_mut()
                .expect("canvas root");
            root.children.push(Node {
                id: "form-email".into(),
                component: prism_builder::ComponentId::from("container"),
                props: json!({ "value": "" }),
                children: vec![],
                layout_mode: Default::default(),
                transform: Default::default(),
                modifiers: vec![],
                style: Default::default(),
            });
        }
        let hit = HitRect {
            id: "email-input".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 24.0,
            },
            attrs: vec![("data-bind-value".into(), "form-email.value".into())],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 10.0,
                y: 10.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "bind:value click should request a redraw");
        let focus = shell
            .inner
            .borrow()
            .state
            .field_focus
            .clone()
            .expect("field focus session active");
        assert_eq!(focus.target_id, "form-email");
        assert_eq!(focus.key, "value");
        assert_eq!(focus.kind, "text");

        // Now simulate a keystroke landing on the focused input.
        let dirty = dispatch_event(&shell.inner, &Event::Text { text: "hi".into() }, None);
        assert!(dirty, "typed text writes back through field-focus");
        let val = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("form-email"))
            .and_then(|n| n.props.get("value").cloned())
            .unwrap_or(Value::Null);
        assert_eq!(val, Value::String("hi".into()));
    }

    /// Clicking an input whose bind path points at a non-existent
    /// node is a clean no-op — the router falls through to the
    /// canvas chain instead of getting wedged on a phantom focus.
    #[test]
    fn pointer_down_on_input_with_bogus_bind_path_falls_through() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = HitRect {
            id: "input-x".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 24.0,
            },
            attrs: vec![("data-bind-value".into(), "missing-node.value".into())],
        };
        dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 10.0,
                y: 10.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(shell.inner.borrow().state.field_focus.is_none());
    }

    #[test]
    fn pointer_down_on_workflow_page_button_switches_active_page() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        // Boot lands on the first page (index 0). Pick a non-active
        // page id so the click actually moves the state.
        let pages: Vec<String> = shell
            .inner
            .borrow()
            .state
            .workspace
            .workspace
            .pages()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let target = pages
            .get(1)
            .expect("workspace has at least two pages")
            .clone();
        let hit = hit_with("workflow-page-button", &target, &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "switching pages requests a redraw");
        assert_eq!(
            shell
                .inner
                .borrow()
                .state
                .workspace
                .workspace
                .active_page()
                .id,
            target,
        );
    }

    #[test]
    fn pointer_down_on_dock_tab_activates_panel_inside_its_tab_group() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        // Pick a panel id that exists somewhere in the workspace —
        // `inspector` ships in the default "edit" page.
        let target = "inspector".to_string();
        let hit = hit_with("dock-tab", &target, &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        // `navigate_to_panel` returns true when the panel exists
        // anywhere in the workspace, so any click on a real panel id
        // should request a redraw.
        assert!(dirty, "panel navigation requests a redraw");
    }

    #[test]
    fn pointer_down_on_device_pill_switches_canvas_device() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        assert_eq!(
            shell.inner.borrow().state.canvas.device,
            crate::state::Device::Desktop,
            "boot defaults to Desktop"
        );
        let hit = HitRect {
            id: "pill".into(),
            bounds: prism_ui_runtime::command::Rect {
                x: 0.0,
                y: 0.0,
                width: 60.0,
                height: 24.0,
            },
            attrs: vec![
                ("data-role".into(), "toolbar-device-pill".into()),
                ("data-device".into(), "tablet".into()),
            ],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "device switch requests a redraw");
        assert_eq!(
            shell.inner.borrow().state.canvas.device,
            crate::state::Device::Tablet,
        );
    }

    #[test]
    fn pointer_down_on_zoom_reset_pill_resets_canvas_zoom() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.canvas.viewport.zoom = 2.5;
        }
        let hit = HitRect {
            id: "pill".into(),
            bounds: prism_ui_runtime::command::Rect {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 24.0,
            },
            attrs: vec![("data-role".into(), "toolbar-zoom-reset".into())],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "zoom reset requests a redraw");
        assert_eq!(shell.inner.borrow().state.canvas.viewport.zoom, 1.0);
    }

    #[test]
    fn pointer_down_on_nav_page_row_moves_active_nav_page() {
        use crate::state::{NavPage, NavigationSlot};
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.navigation = NavigationSlot {
                pages: vec![
                    NavPage {
                        id: "home".into(),
                        title: "Home".into(),
                        route: "/".into(),
                        x: 0.0,
                        y: 0.0,
                        node_count: 0,
                        link_count: 0,
                        is_active: true,
                    },
                    NavPage {
                        id: "about".into(),
                        title: "About".into(),
                        route: "/about".into(),
                        x: 0.0,
                        y: 0.0,
                        node_count: 0,
                        link_count: 0,
                        is_active: false,
                    },
                ],
                edges: vec![],
                ..Default::default()
            };
        }
        let hit = hit_with("nav-page-row", "about", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "active nav page switch requests a redraw");
        let nav = &shell.inner.borrow().state.navigation;
        assert!(nav.pages[1].is_active);
        assert!(!nav.pages[0].is_active);
    }

    #[test]
    fn pointer_down_on_nav_page_row_also_moves_chevron_cursor() {
        use crate::state::{NavPage, NavigationSlot};
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.navigation = NavigationSlot {
                pages: vec![NavPage {
                    id: "home".into(),
                    title: "Home".into(),
                    route: "/".into(),
                    x: 0.0,
                    y: 0.0,
                    node_count: 0,
                    link_count: 0,
                    is_active: true,
                }],
                edges: vec![],
                ..Default::default()
            };
        }
        let hit = hit_with("nav-page-row", "home", &[]);
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 1.0,
                y: 1.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert_eq!(
            shell
                .inner
                .borrow()
                .state
                .navigation
                .selected_page
                .as_deref(),
            Some("home"),
        );
    }

    #[test]
    fn pointer_down_on_app_card_sets_workspace_active_app() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = HitRect {
            id: "card-lattice".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 160.0,
                height: 160.0,
            },
            attrs: vec![
                ("data-role".into(), "app-card".into()),
                ("data-app".into(), "lattice".into()),
            ],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "app-card click requests a redraw");
        assert_eq!(
            shell.inner.borrow().state.workspace.active_app.as_deref(),
            Some("lattice"),
        );
    }

    #[test]
    fn pointer_down_on_app_card_drives_full_swap_chain() {
        // ADR-009 + ADR-010 wiring: a launchpad click must flow
        // through the same `switch_active_app` path that the public
        // shell API uses — cursor update + service rebuild +
        // render scope dirty. Previously the handler only moved the
        // cursor (`WorkspaceSlot::set_active_app`), bypassing the
        // service rebuild.
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        // Drain any pre-existing dirty state from boot.
        let _ = shell.render();
        assert!(
            !shell.inner.borrow().render_scope.needs_redraw(),
            "render-scope should be clean after a successful render"
        );

        let hit = HitRect {
            id: "card-flux".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 160.0,
                height: 160.0,
            },
            attrs: vec![
                ("data-role".into(), "app-card".into()),
                ("data-app".into(), "flux".into()),
            ],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        // Cursor moved.
        assert_eq!(
            shell.inner.borrow().state.workspace.active_app.as_deref(),
            Some("flux"),
        );
        // Render scope was marked dirty by `switch_active_app`'s
        // FRAME_DIRTY_SENTINEL — proves the full chain fired, not
        // just the cursor write.
        assert!(
            shell.inner.borrow().render_scope.needs_redraw(),
            "click on app-card should mark the render scope dirty via switch_active_app"
        );
    }

    #[test]
    fn pointer_down_on_already_active_app_card_is_idempotent() {
        // The full chain is idempotent: clicking the already-active
        // app's tile returns `false` from `switch_active_app` (no
        // rebuild, no dirty bump). The dispatcher still reports
        // `dirty` because clicking *something* counts as activity in
        // its conservative redraw model, but the underlying state
        // didn't move.
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        shell.switch_active_app(Some("musica"));
        let _ = shell.render();

        let hit = HitRect {
            id: "card-musica".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 160.0,
                height: 160.0,
            },
            attrs: vec![
                ("data-role".into(), "app-card".into()),
                ("data-app".into(), "musica".into()),
            ],
        };
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        // Cursor stays put.
        assert_eq!(
            shell.inner.borrow().state.workspace.active_app.as_deref(),
            Some("musica"),
        );
    }

    #[test]
    fn pointer_down_on_create_card_without_data_app_is_a_no_op() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = HitRect {
            id: "card-create".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 160.0,
                height: 160.0,
            },
            attrs: vec![
                ("data-role".into(), "app-card".into()),
                ("data-create".into(), "true".into()),
            ],
        };
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(shell.inner.borrow().state.workspace.active_app.is_none());
    }

    #[test]
    fn pointer_down_on_schema_row_moves_schema_field_cursor() {
        use crate::state::{SchemaDoc, SchemaField};
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.builder.schema = SchemaDoc {
                fields: vec![
                    SchemaField {
                        name: "title".into(),
                        kind: "text".into(),
                        required: false,
                    },
                    SchemaField {
                        name: "body".into(),
                        kind: "rich-text".into(),
                        required: false,
                    },
                ],
                ..Default::default()
            };
        }
        let hit = hit_with("schema-row", "body", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "schema-row click moves the cursor → redraw");
        assert_eq!(
            shell
                .inner
                .borrow()
                .state
                .builder
                .schema
                .selected_field
                .as_deref(),
            Some("body"),
        );
    }

    #[test]
    fn pointer_down_on_signal_connection_row_moves_connection_cursor() {
        use crate::state::SignalConnection;
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            guard
                .state
                .builder
                .signal_connections
                .push(SignalConnection {
                    id: "c1".into(),
                    source_signal: "clicked".into(),
                    action_kind: "EmitSignal".into(),
                    target_label: "x".into(),
                });
            guard
                .state
                .builder
                .signal_connections
                .push(SignalConnection {
                    id: "c2".into(),
                    source_signal: "hovered".into(),
                    action_kind: "SetProperty".into(),
                    target_label: "y".into(),
                });
        }
        let hit = hit_with("signal-connection-row", "c2", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "signal-connection-row click moves cursor → redraw");
        assert_eq!(
            shell
                .inner
                .borrow()
                .state
                .builder
                .selected_connection
                .as_deref(),
            Some("c2"),
        );
    }

    #[test]
    fn pointer_down_on_palette_item_sets_palette_selected() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = hit_with("palette-item", "text", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "palette pick mutates state and requests a redraw");
        assert_eq!(
            shell
                .inner
                .borrow()
                .state
                .catalog
                .palette_selected
                .as_deref(),
            Some("text"),
        );
    }

    /// §43 A1: an unsupported / malformed action string falls
    /// through cleanly — the rest of the pointer-down chain (canvas
    /// selection, drag capture) still runs. The contract is "parsed
    /// actions without a shipped executor are no-ops, never panics."
    #[test]
    fn pointer_down_on_unsupported_action_does_not_break_chain() {
        use prism_ui_runtime::event::PointerButton;

        let shell = Shell::new().expect("boot");
        let baseline_selection = shell.inner.borrow().state.canvas.selection.clone();
        let hit = HitRect {
            id: "demo-heading".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![("data-on-click".into(), "yodel loud".into())],
        };
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 10.0,
                y: 10.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        // No actor fired — the canvas selection stayed exactly where
        // it was (the boot pre-selection on `demo-heading`).
        assert_eq!(
            shell.inner.borrow().state.canvas.selection,
            baseline_selection,
        );
    }

    // ── Wave 1.6 modifier route tests ───────────────────────────────

    fn attach_tooltip_to(shell: &Shell, node_id: &str) {
        // Helper: attach a tooltip modifier to the named doc node so
        // the route tests have a target to toggle / remove / reorder.
        use prism_builder::{Modifier, ModifierKind};
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        if let Some(n) = g
            .state
            .canvas
            .document
            .root
            .as_mut()
            .and_then(|r| r.find_mut(node_id))
        {
            n.modifiers.push(Modifier::from_kind(ModifierKind::Tooltip));
        }
        let registry = g.registry.as_component_registry();
        g.state.resync_builder_for_selection(Some(registry));
    }

    #[test]
    fn pointer_down_on_modifier_toggle_flips_enabled() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        attach_tooltip_to(&shell, "demo-button");
        // Select the button so the inspector sees its modifiers.
        {
            let mut guard = shell.inner.borrow_mut();
            let g = &mut *guard;
            let registry = g.registry.as_component_registry();
            g.state.select_node("demo-button", Some(registry));
        }

        let hit = hit_with(
            "modifier-toggle",
            "demo-button",
            &[("data-modifier-idx", "0")],
        );
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 1.0,
                y: 1.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        let enabled = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .find("demo-button")
            .unwrap()
            .modifiers[0]
            .enabled;
        assert!(!enabled, "first click disables the modifier");
    }

    #[test]
    fn pointer_down_on_modifier_remove_detaches_entry() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        attach_tooltip_to(&shell, "demo-button");
        assert_eq!(
            shell
                .inner
                .borrow()
                .state
                .canvas
                .document
                .root
                .as_ref()
                .unwrap()
                .find("demo-button")
                .unwrap()
                .modifiers
                .len(),
            1
        );

        let hit = hit_with(
            "modifier-remove",
            "demo-button",
            &[("data-modifier-idx", "0")],
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 1.0,
                y: 1.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        let len = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .find("demo-button")
            .unwrap()
            .modifiers
            .len();
        assert_eq!(len, 0);
    }

    #[test]
    fn pointer_down_on_add_modifier_open_seeds_picker_state() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = hit_with(
            "add-modifier-open",
            "demo-button",
            &[("data-attached", r#"["tooltip"]"#)],
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 1.0,
                y: 1.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        let picker = shell.inner.borrow().state.overlay.modifier_picker.clone();
        assert!(picker.open);
        assert_eq!(picker.target_id, "demo-button");
        assert_eq!(picker.attached, vec!["tooltip".to_string()]);
    }

    #[test]
    fn pointer_down_on_modifier_picker_select_attaches_and_closes_picker() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        // Open the picker first.
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.overlay.modifier_picker = crate::state::ModifierPicker {
                open: true,
                target_id: "demo-button".into(),
                attached: vec![],
            };
        }
        let hit = hit_with(
            "modifier-picker-select",
            "demo-button",
            &[("data-modifier-id", "tooltip")],
        );
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 1.0,
                y: 1.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        let inner = shell.inner.borrow();
        assert_eq!(
            inner
                .state
                .canvas
                .document
                .root
                .as_ref()
                .unwrap()
                .find("demo-button")
                .unwrap()
                .modifiers
                .len(),
            1
        );
        assert!(!inner.state.overlay.modifier_picker.open, "picker closes");
    }

    // ── Wave 3.2 palette drag → drop tests ──────────────────────────

    /// A canvas hit while `palette_selected` is armed must capture
    /// a palette drag (recording the cursor + drop target) without
    /// also re-selecting the canvas node under the cursor.
    #[test]
    fn pointer_down_on_canvas_with_palette_armed_begins_palette_drag() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.catalog.palette_selected = Some("text".into());
        }
        // A canvas hit shaped like the lowered `demo-heading` preview
        // node — `data-canvas-node="demo-heading"` is the disambiguator.
        let hit = HitRect {
            id: "demo-heading".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![
                ("data-role".into(), "canvas-preview".into()),
                ("data-canvas-node".into(), "demo-heading".into()),
            ],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 100.0,
                y: 200.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "palette drag start requests redraw");
        let inner = shell.inner.borrow();
        let drag = inner
            .state
            .catalog
            .palette_drag
            .as_ref()
            .expect("drag captured");
        assert_eq!(drag.kind, "text");
        assert_eq!(drag.pointer, (100.0, 200.0));
        assert_eq!(drag.drop_target.as_deref(), Some("demo-heading"));
    }

    /// A palette drag released over a canvas node inserts the new
    /// node under that target, clears `palette_selected`, and moves
    /// the canvas selection onto the freshly-dropped node.
    #[test]
    fn pointer_up_with_active_palette_drag_inserts_node_under_target() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let before = shell.inner.borrow().state.canvas.node_count();
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.catalog.palette_selected = Some("button".into());
        }
        // Step 1: PointerDown on a canvas-preview hit captures the
        // drag and records the drop target.
        let target_id = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .id
            .clone();
        let hit = HitRect {
            id: target_id.clone(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 200.0,
            },
            attrs: vec![
                ("data-role".into(), "canvas-preview".into()),
                ("data-canvas-node".into(), target_id.clone()),
            ],
        };
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 12.0,
                y: 12.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit.clone()),
        );
        // Step 2: PointerUp commits the insert.
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerUp {
                x: 20.0,
                y: 20.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "drop commits the insert → redraw");
        let after = shell.inner.borrow().state.canvas.node_count();
        assert_eq!(after, before + 1, "node-count rises by one");
        let inner = shell.inner.borrow();
        assert!(
            inner.state.catalog.palette_drag.is_none(),
            "drag state is consumed"
        );
        assert!(
            inner.state.catalog.palette_selected.is_none(),
            "palette pill clears after drop"
        );
        // Selection moves to the new node (its id is "button-new" +
        // a unique suffix because the target's children may already
        // hold a `button-new`).
        assert!(
            inner
                .state
                .canvas
                .selection
                .as_deref()
                .map(|s| s.starts_with("button-new"))
                .unwrap_or(false),
            "selection moves to the dropped node"
        );
    }

    /// With no palette item armed, a canvas click falls through to
    /// the existing select-canvas-node path — the Wave 3.2 hook
    /// must not steal clicks that aren't actually palette-driven.
    #[test]
    fn pointer_down_on_canvas_without_palette_armed_falls_through_to_select() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        // No palette pick.
        assert!(shell
            .inner
            .borrow()
            .state
            .catalog
            .palette_selected
            .is_none());
        let hit = HitRect {
            id: "demo-heading".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![
                ("data-role".into(), "canvas-preview".into()),
                ("data-canvas-node".into(), "demo-heading".into()),
            ],
        };
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 10.0,
                y: 10.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        let inner = shell.inner.borrow();
        assert!(
            inner.state.catalog.palette_drag.is_none(),
            "no drag started"
        );
        assert_eq!(
            inner.state.canvas.selection.as_deref(),
            Some("demo-heading"),
            "ordinary canvas click still selects"
        );
    }

    // ── Wave 3.4 right-click context menu tests ─────────────────────

    /// A right-click on a canvas-resident hit opens the context menu
    /// with the node-mutation triad plus clipboard rows; the canvas
    /// selection moves to the right-clicked node so command
    /// activation operates against it.
    #[test]
    fn right_click_on_canvas_node_opens_context_menu_with_actions() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = HitRect {
            id: "demo-heading".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 24.0,
            },
            attrs: vec![
                ("data-role".into(), "canvas-preview".into()),
                ("data-canvas-node".into(), "demo-heading".into()),
            ],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 50.0,
                y: 50.0,
                button: PointerButton::Secondary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "right-click opens the menu → redraw");
        let inner = shell.inner.borrow();
        assert_eq!(
            inner.state.canvas.selection.as_deref(),
            Some("demo-heading"),
            "right-click moves the selection cursor",
        );
        let labels: Vec<&str> = inner
            .state
            .menus
            .context
            .iter()
            .filter(|m| !m.separator)
            .map(|m| m.label.as_str())
            .collect();
        assert!(
            labels.contains(&"Delete"),
            "menu carries the Delete row, got {labels:?}"
        );
        assert!(
            labels.contains(&"Move Up"),
            "menu carries the Move Up row, got {labels:?}"
        );
    }

    /// A right-click on the empty canvas (no `data-canvas-node`) still
    /// opens the menu, falling back to document-level actions.
    #[test]
    fn right_click_on_empty_canvas_opens_paste_only_menu() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        // Clear selection so the menu reflects "no node selected."
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.canvas.selection = None;
        }
        let hit = HitRect {
            id: "canvas".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            },
            attrs: vec![("data-role".into(), "canvas-page".into())],
        };
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 400.0,
                y: 300.0,
                button: PointerButton::Secondary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        let inner = shell.inner.borrow();
        let labels: Vec<&str> = inner
            .state
            .menus
            .context
            .iter()
            .filter(|m| !m.separator)
            .map(|m| m.label.as_str())
            .collect();
        assert_eq!(labels, vec!["Paste"]);
    }

    /// A primary click outside the menu closes an open context menu.
    /// The hit need not match any chrome — even an idle canvas
    /// surface dismisses it.
    #[test]
    fn primary_click_outside_menu_dismisses_open_context_menu() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        // Seed an open menu.
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.menus.context.push(crate::state::MenuItem {
                label: "Foo".into(),
                shortcut: None,
                command: Some("noop".into()),
                separator: false,
                enabled: true,
            });
        }
        let hit = HitRect {
            id: "elsewhere".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            attrs: vec![],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "dismiss requests a redraw");
        assert!(shell.inner.borrow().state.menus.context.is_empty());
    }

    // ── Wave 3.3 selection-gizmo + resize tests ─────────────────────

    /// A canvas-node click captures the hit's bounding rect into
    /// `state.canvas.selection_bbox` so the next frame paints the
    /// selection outline + 8-handle ring at the real layout rect.
    #[test]
    fn pointer_down_on_canvas_node_captures_selection_bbox() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let hit = HitRect {
            id: "demo-heading".into(),
            bounds: Rect {
                x: 24.0,
                y: 48.0,
                width: 160.0,
                height: 32.0,
            },
            attrs: vec![
                ("data-role".into(), "canvas-preview".into()),
                ("data-canvas-node".into(), "demo-heading".into()),
            ],
        };
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 25.0,
                y: 49.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        let bbox = shell
            .inner
            .borrow()
            .state
            .canvas
            .selection_bbox
            .expect("bbox captured");
        assert_eq!(bbox.x, 24.0);
        assert_eq!(bbox.y, 48.0);
        assert_eq!(bbox.width, 160.0);
        assert_eq!(bbox.height, 32.0);
    }

    /// A pointer-down on a resize-handle hit captures the direction
    /// and snapshots the selection's transform; a follow-up
    /// pointer-move translates the node along the handle's axes.
    #[test]
    fn resize_handle_press_then_move_translates_selection_transform() {
        use prism_builder::Node;
        use prism_core::foundation::spatial::Transform2D;
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        // Seed a doc with a known-position node and select it so
        // the handle handler has something to drag.
        {
            let mut guard = shell.inner.borrow_mut();
            let g = &mut *guard;
            g.state.canvas.document = prism_builder::BuilderDocument {
                root: Some(Node {
                    id: "root".into(),
                    component: "container".into(),
                    transform: Transform2D {
                        position: [100.0, 100.0],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                ..Default::default()
            };
            g.state.canvas.selection = Some("root".into());
        }
        let handle_hit = hit_with("resize-handle", "", &[("data-direction", "br")]);
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(handle_hit),
        );
        assert!(
            shell.inner.borrow().state.canvas.resize_drag.is_some(),
            "press captures the drag session"
        );
        // PointerMove with no hit (resize drag doesn't need one):
        // delta (50, 30) under bottom-right handle adds positively.
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerMove {
                x: 50.0,
                y: 30.0,
                modifiers: Modifiers::default(),
            },
            None,
        );
        let pos = shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .transform
            .position;
        // Origin was (handle bbox center = 5, 5). Delta = (45, 25)
        // against zoom 1.0. Snapshot = (100, 100). After drag:
        // (100 + 45, 100 + 25) = (145, 125).
        assert!((pos[0] - 145.0).abs() < 0.5, "x translated: {pos:?}");
        assert!((pos[1] - 125.0).abs() < 0.5, "y translated: {pos:?}");
        // PointerUp commits the drag (clears the session).
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerUp {
                x: 50.0,
                y: 30.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            None,
        );
        assert!(
            shell.inner.borrow().state.canvas.resize_drag.is_none(),
            "release commits the drag"
        );
    }

    /// A resize-handle press without a selected canvas node is a
    /// clean no-op — clicking a stale handle (e.g. one painted from
    /// a prior selection that the user then deselected) doesn't
    /// capture an empty drag.
    #[test]
    fn resize_handle_press_without_selection_is_a_noop() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.canvas.selection = None;
        }
        let handle_hit = hit_with("resize-handle", "", &[("data-direction", "br")]);
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(handle_hit),
        );
        assert!(shell.inner.borrow().state.canvas.resize_drag.is_none());
    }

    /// An unknown `data-direction` value falls through cleanly; the
    /// 8-direction whitelist guards against malformed authoring.
    #[test]
    fn resize_handle_press_with_unknown_direction_falls_through() {
        use prism_builder::Node;
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            let g = &mut *guard;
            g.state.canvas.document = prism_builder::BuilderDocument {
                root: Some(Node {
                    id: "root".into(),
                    component: "container".into(),
                    ..Default::default()
                }),
                ..Default::default()
            };
            g.state.canvas.selection = Some("root".into());
        }
        let handle_hit = hit_with("resize-handle", "", &[("data-direction", "???")]);
        let _ = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(handle_hit),
        );
        assert!(shell.inner.borrow().state.canvas.resize_drag.is_none());
    }

    // ── Wave 4.3 connection picker route tests ──────────────────────

    /// Clicking the connection-picker's `action-kind` field cycles
    /// through the declared `ActionKind` variant list. Each click
    /// advances one step; the picker stays open so the user can
    /// also edit source/target before confirming.
    #[test]
    fn pointer_down_on_connection_picker_action_kind_cycles() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        {
            let mut guard = shell.inner.borrow_mut();
            assert!(guard.state.open_connection_picker());
        }
        let baseline = shell
            .inner
            .borrow()
            .state
            .overlay
            .connection_picker
            .action_kind
            .clone();
        let hit = hit_with(
            "connection-picker-field",
            "",
            &[("data-field", "action-kind")],
        );
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "cycle moves the field → redraw");
        let after = shell
            .inner
            .borrow()
            .state
            .overlay
            .connection_picker
            .action_kind
            .clone();
        assert_ne!(after, baseline, "action-kind advanced");
    }

    /// The Add button confirms the picker → a fresh
    /// `SignalConnection` lands and the picker closes.
    #[test]
    fn pointer_down_on_connection_picker_add_inserts_and_closes() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let before = shell.inner.borrow().state.builder.signal_connections.len();
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.open_connection_picker();
            guard.state.overlay.connection_picker.source_signal = "clicked".into();
            guard.state.overlay.connection_picker.target_label = "x".into();
        }
        let hit = hit_with("connection-picker-add", "", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty, "add → redraw");
        let inner = shell.inner.borrow();
        assert_eq!(
            inner.state.builder.signal_connections.len(),
            before + 1,
            "one connection added"
        );
        assert!(
            !inner.state.overlay.connection_picker.open,
            "picker closes after add"
        );
    }

    /// The Cancel button closes the picker without inserting.
    #[test]
    fn pointer_down_on_connection_picker_cancel_closes_without_insert() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        let before = shell.inner.borrow().state.builder.signal_connections.len();
        {
            let mut guard = shell.inner.borrow_mut();
            guard.state.open_connection_picker();
            guard.state.overlay.connection_picker.source_signal = "clicked".into();
        }
        let hit = hit_with("connection-picker-cancel", "", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        assert_eq!(
            shell.inner.borrow().state.builder.signal_connections.len(),
            before,
            "no insert on cancel"
        );
        assert!(!shell.inner.borrow().state.overlay.connection_picker.open);
    }

    /// The "+ Add Connection" footer in the signals panel dispatches
    /// `signals.open-connection-picker` via the existing
    /// `data-on-click="cmd <id>"` path. Verify that the route fires
    /// the command and the picker actually opens.
    #[test]
    fn data_on_click_open_picker_dispatches_through_command_table() {
        use prism_ui_runtime::event::PointerButton;
        let shell = Shell::new().expect("boot");
        assert!(!shell.inner.borrow().state.overlay.connection_picker.open);
        let hit = HitRect {
            id: "add-button".into(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 30.0,
            },
            attrs: vec![(
                "data-on-click".into(),
                "cmd signals.open-connection-picker".into(),
            )],
        };
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 1.0,
                y: 1.0,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            Some(hit),
        );
        assert!(dirty);
        assert!(
            shell.inner.borrow().state.overlay.connection_picker.open,
            "picker opened via command dispatch"
        );
    }
}
