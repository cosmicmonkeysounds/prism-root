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
        Event::PointerDown { x, y, .. } => {
            let routed = hit
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
            // §43 B5: a click on a rendered canvas document node
            // (tagged `data-canvas-node="<id>"` by `builder_canvas`)
            // routes to `select_node` *before* the canvas-tool drag
            // capture runs. Chrome clicks that already matched a
            // `data-role` route (inspector-row / field-edit) skip
            // this — `routed` short-circuits the chain.
            let selected = !routed
                && !acted
                && hit
                    .as_ref()
                    .map(|h| route_canvas_node_select(inner, h))
                    .unwrap_or(false);
            let captured = inner.borrow_mut().state.canvas.pointer_down(*x, *y);
            routed || acted || selected || captured
        }
        Event::PointerMove { x, y } => inner.borrow_mut().state.canvas.pointer_move(*x, *y),
        Event::PointerUp { x, y, .. } => inner.borrow_mut().state.canvas.pointer_up(*x, *y),
        // §24: every other event variant fans out through the service
        // registry. Services declare their interest via `on_event`;
        // the first to return `Handled` short-circuits. Adding a new
        // feature *does not touch this match*.
        Event::Wheel { .. } | Event::Key { .. } | Event::Text { .. } | Event::Focus { .. } => {
            let mut guard = inner.borrow_mut();
            // Split-borrow: we need `&services` and `&mut MutCtx{state, undo, viewport}`
            // simultaneously. Re-borrow the fields explicitly so the
            // borrow checker sees the disjoint slices.
            let g = &mut *guard;
            let viewport = g.viewport;
            let services = &g.services;
            let registry = g.registry.as_component_registry();
            let mut ctx = crate::services::MutCtx {
                state: &mut g.state,
                viewport,
                undo: &mut g.undo,
                vfs: g.vfs.as_mut(),
                luau: g.luau.as_mut(),
                clipboard: &mut g.clipboard,
                registry: Some(registry),
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
/// property. Boolean rows toggle the value; other kinds carry the
/// routing attrs through but defer their edit UX (text-input,
/// drag-number, color-picker, …) to follow-ups that need real focus
/// / IME / drag plumbing. The single-click toggle path proves the
/// data flow end-to-end today.
fn handle_field_edit_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let Some(key) = attr_value(hit, "data-key") else {
        return false;
    };
    let kind = attr_value(hit, "data-kind").unwrap_or("");
    let new_value = match kind {
        "boolean" => {
            let cur = attr_value(hit, "data-value").unwrap_or("false") == "true";
            serde_json::Value::Bool(!cur)
        }
        _ => return false,
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    g.state
        .set_node_prop(target, key, new_value, Some(registry))
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

/// Click on a navigation-panel page row — marks that page as the
/// active one inside the navigation slot. Returns true when the
/// active page actually moved.
fn handle_nav_page_row_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(target) = attr_value(hit, "data-target-id") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    guard.state.navigation.select_page_by_id(target)
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
fn route_on_click(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    use prism_builder::signal::{parse_action, ParsedAction};

    let Some(raw) = attr_value(hit, "data-on-click") else {
        return false;
    };
    let Some(action) = parse_action(raw) else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let viewport = g.viewport;
    let registry = g.registry.as_component_registry();
    // Split-borrow: `services` reads the command table while `ctx`
    // borrows every mutable shell resource. Re-borrowing each field
    // through `g` keeps the borrow checker happy — the same pattern
    // the keyboard / wheel fan-out uses upstream.
    let services = &g.services;
    let mut ctx = crate::services::MutCtx {
        state: &mut g.state,
        viewport,
        undo: &mut g.undo,
        vfs: g.vfs.as_mut(),
        luau: g.luau.as_mut(),
        clipboard: &mut g.clipboard,
        registry: Some(registry),
    };
    match action {
        ParsedAction::Emit { signal } => {
            // `fire_signal` returns the count of connections fired —
            // when zero, nothing observable changed, so the
            // pointer-down chain falls through to canvas / drag
            // handlers exactly as if no `on:click` had been authored.
            crate::services::signals::fire_signal(
                &mut ctx,
                hit.id.as_str(),
                signal.as_str(),
                &serde_json::Value::Null,
                0,
            ) > 0
        }
        ParsedAction::Command { id } => services.commands().run(id.as_str(), &mut ctx),
        // Parsed but no shipped handler yet — see `parse_action`
        // docstring. Returning `false` lets the rest of the
        // pointer-down chain (canvas selection / drag capture) still
        // run, matching the "no handler authored" case.
        ParsedAction::Navigate { .. }
        | ParsedAction::SetProperty { .. }
        | ParsedAction::Toggle { .. }
        | ParsedAction::Play { .. }
        | ParsedAction::Luau { .. }
        | ParsedAction::Unsupported { .. } => false,
    }
}

/// §43 B5: pointer-down on a `data-canvas-node`-tagged container
/// updates the canvas selection. `select_node` validates the id
/// against the active document and re-derives the inspector tree +
/// property rows for the new selection. Returns true when the
/// selection actually moved (so the frame needs to redraw).
fn route_canvas_node_select(inner: &Rc<RefCell<ShellInner>>, hit: &HitRect) -> bool {
    let Some(node_id) = attr_value(hit, "data-canvas-node") else {
        return false;
    };
    let mut guard = inner.borrow_mut();
    let g = &mut *guard;
    let registry = g.registry.as_component_registry();
    g.state.select_node(node_id, Some(registry))
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
        };
        let mv = Event::PointerMove { x: 160.0, y: 140.0 };
        let up = Event::PointerUp {
            x: 160.0,
            y: 140.0,
            button: PointerButton::Primary,
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
            };
        }
        let hit = hit_with("nav-page-row", "about", &[]);
        let dirty = dispatch_event(
            &shell.inner,
            &Event::PointerDown {
                x: 5.0,
                y: 5.0,
                button: PointerButton::Primary,
            },
            Some(hit),
        );
        assert!(dirty, "active nav page switch requests a redraw");
        let nav = &shell.inner.borrow().state.navigation;
        assert!(nav.pages[1].is_active);
        assert!(!nav.pages[0].is_active);
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
}
