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

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use prism_ui_runtime::event::Event;
use prism_ui_runtime::layout::HitRect;

use crate::services::EventOutcome;
use crate::shell::ShellInner;

mod event_modifiers;
use event_modifiers::EventModifiers;

thread_local! {
    /// Viewport-space pointer-x at the most recent `PointerDown`.
    /// Stashed before the routing fan-out so handlers that need the
    /// exact click x (number-field drag-scrub) can read it without
    /// changing every `PointerHandler` signature. Resets on every
    /// `PointerDown`; pointer-move / pointer-up don't write to it.
    static LAST_PRESS_X: Cell<f32> = const { Cell::new(0.0) };
}

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
            // Number-field drag-scrub needs the actual click x so the
            // delta is measured from where the user pressed, not the
            // hit rect's center. Stash it here before any routing runs.
            LAST_PRESS_X.with(|c| c.set(*x));
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
            // §4.3 — fire any `data-probe-<name>` the hit carries
            // through the retained per-document `LuauScopeFrame`.
            // Independent of `routed` (a probe is observational —
            // it rides alongside whatever else claimed the click) and
            // recorded into the bounded `probe_log` for the Inspector
            // surface. A probe handler that writes reactive state has
            // already marked its block dirty via Phase 5.
            #[cfg(feature = "native")]
            let probed = hit.as_ref().map(|h| route_probe(inner, h)).unwrap_or(false);
            #[cfg(not(feature = "native"))]
            let probed = false;
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
                || probed
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
    // **IDE-mode Phase 1** — explorer file/folder click. File rows
    // route to the editor's open-by-path path; directory rows are a
    // no-op until folding lands (the walker emits a flat depth-encoded
    // list today, so every file is already visible).
    ("explorer-row", handle_explorer_row_click),
    // **IDE-mode Phase 2** — "Go to Symbol" palette row. Reads
    // `data-path` + `data-offset`, opens the file in the code editor
    // and drops the caret on the definition.
    ("symbol-row", handle_symbol_row_click),
    // **IDE-mode Phase 6** — find-in-files. A result row click sets
    // the selection + opens it (file hits jump via `open_at_offset`,
    // document hits select the builder node); the scope pill flips
    // Document ⇄ Project.
    ("search-result", handle_search_result_click),
    ("search-scope-toggle", handle_search_scope_toggle_click),
    // Replace-in-files focus routing + apply.
    ("search-replace-input", handle_search_replace_focus_click),
    ("search-query-input", handle_search_query_focus_click),
    ("search-replace-apply", handle_search_replace_apply_click),
    // **IDE-mode Phase 3** — a diagnostics row jumps to the problem
    // site via the shared editor-jump seam.
    ("diagnostics-row", handle_diagnostics_row_click),
    // **IDE-mode Phase 4** — DevTools panel routes. Tabs switch the
    // active lens; clicking the filter input focuses it so the
    // declarative text-input dispatch claims keystrokes; clicking a
    // document-row routes to canvas selection so the lens doubles as
    // a jump-to-selection surface.
    ("devtools-tab", handle_devtools_tab_click),
    ("devtools-filter", handle_devtools_filter_click),
    ("devtools-doc-row", handle_devtools_doc_row_click),
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

/// **§4.3** — dispatch every `data-probe-<name>` the hit carries
/// through the retained per-document `LuauScopeFrame`'s `fire_probe`,
/// recording each into the bounded `probe_log`. The payload is the
/// hit's other `data-*` attributes as a JSON object so a handler
/// gets interaction context. Returns `true` when at least one probe
/// was present (so the caller can fold it into the redraw bit — a
/// handler that wrote reactive state already marked its block, but
/// an empty-handler probe still warrants a frame so the Inspector
/// reflects the fire).
#[cfg(feature = "native")]
fn route_probe(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let probes: Vec<String> = hit
        .attrs
        .iter()
        .filter_map(|(k, _)| k.strip_prefix("data-probe-").map(str::to_string))
        .collect();
    if probes.is_empty() {
        return false;
    }
    // Payload = every non-probe `data-*` attr (strip the `data-`
    // prefix so handlers read `role` / `target-id` not the wire key).
    let mut payload = serde_json::Map::new();
    for (k, v) in &hit.attrs {
        if let Some(rest) = k.strip_prefix("data-") {
            if !rest.starts_with("probe-") {
                payload.insert(rest.to_string(), serde_json::Value::String(v.clone()));
            }
        }
    }
    let payload = serde_json::Value::Object(payload);

    // Fire each probe through the Luau frame + record into the
    // Inspector's `probe_log`, collecting a `ProbeEvent` per probe so
    // the DevTools Probes lens (`state.devtools.probes`) shows the
    // same stream. Two borrow phases: the frame dispatch needs an
    // immutable `inner`; the lens push needs `&mut state`.
    let fired: Vec<String> = {
        let g = inner.borrow();
        let frame = g.active_luau_frame.borrow();
        const PROBE_LOG_CAP: usize = 256;
        for name in &probes {
            let (ok, error) = match frame.as_ref().and_then(|f| f.fire_probe(name, &payload)) {
                Some(Ok(())) => (true, None),
                Some(Err(e)) => (false, Some(e)),
                None => (false, None),
            };
            let mut log = g.probe_log.borrow_mut();
            if log.len() >= PROBE_LOG_CAP {
                log.pop_front();
            }
            log.push_back(crate::shell::ProbeFire {
                name: name.clone(),
                payload: payload.clone(),
                ok,
                error,
            });
        }
        probes
    };
    // §4.3 — surface the same fires in the DevTools Probes lens.
    let ts = click_millis();
    let source = (!hit.id.is_empty()).then(|| hit.id.clone());
    let mut g = inner.borrow_mut();
    for name in fired {
        g.state.devtools.record_probe(crate::state::ProbeEvent {
            name,
            payload: payload.clone(),
            timestamp_ms: ts,
            source_node_id: source.clone(),
        });
    }
    true
}

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

/// IDE-mode Phase 1: a click on a `shell.explorer` row. For file
/// entries (`data-kind="file"`), reads the absolute path from
/// `data-path`, loads the source via `Vfs`, and opens the result as
/// a new editor tab through `CanvasSlot::open_editor_tab` — the
/// same code path `editor.file.open` uses after the file-picker.
/// Directory rows are a no-op (folding lands in a follow-up).
///
/// Returns `true` when the click was consumed; the caller uses this
/// to decide whether to request a redraw.
fn handle_explorer_row_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let kind = attr_value(hit, "data-kind").unwrap_or("");
    if kind != "file" {
        // Directory rows + any unrecognised kind: consumed (so the
        // click doesn't fall through to a deeper handler), but no
        // state change.
        return false;
    }
    let path_str = match attr_value(hit, "data-path") {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => return false,
    };
    let path = std::path::PathBuf::from(path_str);
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let bytes = match g.vfs.read(&path) {
        Ok(b) => b,
        Err(_) => {
            // Surface a toast so the user knows the click registered
            // but the file couldn't be read. Mirrors the
            // `editor.file.open` error path.
            g.state.overlay.toasts.push(crate::state::Toast {
                title: "Open failed".into(),
                body: format!("could not read {}", path.display()),
                kind: crate::state::ToastKind::Error,
            });
            return true;
        }
    };
    let source = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            g.state.overlay.toasts.push(crate::state::Toast {
                title: "Open failed".into(),
                body: "File contains non-UTF-8 bytes".into(),
                kind: crate::state::ToastKind::Error,
            });
            return true;
        }
    };
    let language = language_from_path(&path);
    g.state.canvas.open_editor_tab(path, source, language);
    // IDE-mode usability: clicking a file in the explorer should
    // route the user into the code editor so they see the file
    // they just opened.
    g.state.code_editor_focused = true;
    true
}

/// IDE-mode Phase 3: a click on a `shell.diagnostics-panel` row.
/// Reads `data-path` + byte `data-offset` and jumps to the problem
/// site through the shared editor-jump seam.
fn handle_diagnostics_row_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let path_str = match attr_value(hit, "data-path") {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => return false,
    };
    let offset: usize = attr_value(hit, "data-offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let path = std::path::PathBuf::from(path_str);
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    if let Err(e) =
        crate::services::editor_files::open_at_offset(&mut g.state, &*g.vfs, path, offset)
    {
        g.state.overlay.toasts.push(crate::state::Toast {
            title: "Open diagnostic failed".into(),
            body: e,
            kind: crate::state::ToastKind::Error,
        });
    }
    true
}

/// IDE-mode Phase 6: a click on a `shell.search-overlay` result row.
/// Reads `data-idx`, makes it the selection, and activates the hit
/// through the shared `activate_search_hit` (project hits jump to the
/// file; document hits select the builder node).
fn handle_search_result_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(idx) = attr_value(hit, "data-idx").and_then(|s| s.parse::<usize>().ok()) else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let sh = {
        let s = &mut guard.state.search;
        s.selected_index = idx;
        s.results.get(idx).cloned()
    };
    let Some(sh) = sh else {
        return true;
    };
    let mut ctx = guard.mut_ctx();
    crate::services::search::activate_search_hit(&mut ctx, &sh);
    true
}

/// IDE-mode Phase 6: clicking the replace input claims keyboard for
/// the `search-replace` text-input declaration (the query
/// declaration yields via its `!replace_focused` guard).
fn handle_search_replace_focus_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    inner.borrow_mut().state.search.replace_focused = true;
    true
}

/// Clicking the query input releases the replace field's focus so
/// keystrokes flow back to the query declaration.
fn handle_search_query_focus_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    let mut guard = inner.borrow_mut();
    if guard.state.search.replace_focused {
        guard.state.search.replace_focused = false;
        return true;
    }
    // Not consumed when already focused on query — let the press
    // fall through to normal caret placement in the input.
    false
}

/// The "Replace All" button — apply the replacement across the
/// current result set via the shared `search_replace_all`.
fn handle_search_replace_apply_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    let mut guard = inner.borrow_mut();
    let mut ctx = guard.mut_ctx();
    crate::services::text_input::search_replace_all(&mut ctx);
    true
}

/// IDE-mode Phase 6: the scope pill — flips Document ⇄ Project. The
/// query is preserved; the now-stale result list is cleared so the
/// next keystroke re-derives it in the new scope.
fn handle_search_scope_toggle_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    let mut guard = inner.borrow_mut();
    let s = &mut guard.state.search;
    s.scope = s.scope.toggled();
    s.results.clear();
    s.selected_index = 0;
    true
}

/// IDE-mode Phase 2: a click on a `shell.symbol-palette` result row.
/// Reads the absolute `data-path` + byte `data-offset`, opens the
/// file in the code editor (de-duped to an existing tab) and places
/// the caret on the definition, then closes the palette. Mirrors the
/// Enter-commit hook so click and keyboard land identically.
fn handle_symbol_row_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let path_str = match attr_value(hit, "data-path") {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => return false,
    };
    let offset: usize = attr_value(hit, "data-offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let path = std::path::PathBuf::from(path_str);
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    if let Err(e) =
        crate::services::editor_files::open_at_offset(&mut g.state, &*g.vfs, path, offset)
    {
        g.state.overlay.toasts.push(crate::state::Toast {
            title: "Go to Symbol failed".into(),
            body: e,
            kind: crate::state::ToastKind::Error,
        });
    }
    let idx = &mut g.state.index;
    idx.palette_open = false;
    idx.query.set_text("");
    idx.selected_index = 0;
    true
}

/// IDE-mode Phase 4: tab click on the DevTools panel. Reads the
/// `data-tab-id` attribute and switches `state.devtools.active_lens`
/// through `DevToolsLens::from_id`. Unknown ids are ignored.
fn handle_devtools_tab_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(tab_id) = attr_value(hit, "data-tab-id") else {
        return false;
    };
    let Some(lens) = crate::state::DevToolsLens::from_id(tab_id) else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    guard.state.devtools.switch_lens(lens);
    true
}

/// IDE-mode Phase 4: clicking the DevTools filter input focuses it
/// so the declarative `text_input::dispatch_text_input` claims
/// keystrokes for the filter `TextEditor`. The filter declaration's
/// `active_when` predicate gates on this flag.
fn handle_devtools_filter_click(inner: &Rc<RefCell<ShellInner>>, _hit: &HitRect) -> bool {
    let mut guard = inner.borrow_mut();
    guard.state.devtools.filter_focused = true;
    // Clear other modal-overlay focuses so this declaration wins the
    // text-input race.
    guard.state.overlay.command_palette.open = false;
    guard.state.search.open = false;
    true
}

/// IDE-mode Phase 4: clicking a document-lens row routes the
/// `data-target-id` to canvas selection — the inspector doubles as a
/// jump-to-selection surface for the live builder tree.
fn handle_devtools_doc_row_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    g.state.select_node(target, Some(registry))
}

/// Extension → language tag for the code editor. Matches the lookup
/// in `services::editor_files::language_from_path`. Kept local so the
/// router doesn't reach into the file service's private helpers.
fn language_from_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "luau" | "lua" => "luau",
        "rs" => "rust",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "py" => "python",
        "sh" | "bash" => "shell",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "md" => "markdown",
        "css" => "css",
        "html" => "html",
        _ => "",
    }
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
        // Anchor the drag at the actual pointer-down x. Using the
        // hit-rect center instead made the first move jump the value
        // by however far the click landed from center — felt random.
        let start_x = LAST_PRESS_X.with(|c| c.get());
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

/// Single seam for every click-driven field-editor write, so the
/// routing logic doesn't drift across the seven-or-so commit sites in
/// `handle_field_edit_click` / drag-commit / text-input commit. Facet
/// template descendants are ordinary nodes now, so every write is a
/// plain [`AppState::set_node_prop`] on the target node id.
fn write_field_value(
    g: &mut crate::shell::ShellInner,
    _hit: &HitRect,
    target: &str,
    key: &str,
    value: serde_json::Value,
) -> bool {
    // Split borrow: name `registry` and `state` as disjoint fields
    // of `*g` so the mut borrow on `state` and the immut borrow on
    // `registry` don't overlap. Sound — Rust's borrow checker
    // understands struct field splits.
    let crate::shell::ShellInner {
        registry, state, ..
    } = g;
    let reg = registry.as_component_registry();
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
/// what keeps `.prui` source author-clean today.
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
/// Resolve the identifier under a Ctrl/Cmd+click on the code-editor
/// body and jump to its definition via the project symbol index.
/// Returns `true` when a symbol was found *and* the jump fired; a
/// miss returns `false` so the caller can fall back to a normal
/// caret-placement press. Mirrors the hover path's
/// `resolve_editor_byte_at` + `syntax::token_at` lexeme extraction,
/// then `SymbolIndex::lookup` + the shared `open_at_offset` jump seam.
fn try_ctrl_jump_to_def(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect, x: f32, y: f32) -> bool {
    const FONT_SIZE: f32 = 13.0;
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let text = g.state.canvas.code_buffer.source().to_string();
    let language = g.state.canvas.code_buffer.language.clone();
    let scroll_x = g.state.canvas.code_buffer.scroll_x;
    let scroll_y = g.state.canvas.code_buffer.scroll_y;
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
    let lang = if language.is_empty() {
        "luau"
    } else {
        language.as_str()
    };
    let Some((_, lexeme)) = prism_ui_runtime::syntax::token_at(&text, lang, byte) else {
        return false;
    };
    let Some(sym) = g
        .state
        .index
        .symbols
        .lookup(lexeme)
        .into_iter()
        .next()
        .cloned()
    else {
        return false;
    };
    if let Err(e) = crate::services::editor_files::open_at_offset(
        &mut g.state,
        &*g.vfs,
        sym.path.clone(),
        sym.offset,
    ) {
        g.state.overlay.toasts.push(crate::state::Toast {
            title: "Go to Definition failed".into(),
            body: e,
            kind: crate::state::ToastKind::Error,
        });
    }
    true
}

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
    // **IDE Phase 2** — Ctrl/Cmd+click is jump-to-definition: resolve
    // the identifier under the click against the project symbol index
    // and, on a hit, open its file with the caret on the def. Misses
    // fall through to the normal caret-placement press below.
    if (modifiers.ctrl || modifiers.meta) && try_ctrl_jump_to_def(inner, hit, x, y) {
        return true;
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
mod tests;
