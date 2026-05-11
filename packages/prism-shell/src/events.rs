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
            let captured = inner.borrow_mut().state.canvas.pointer_down(*x, *y);
            routed || captured
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
        use prism_builder::block::SpecBlock;
        use prism_builder::starter::BUILTINS;
        use prism_builder::Node;
        use prism_ui_runtime::event::PointerButton;
        use serde_json::json;

        let shell = Shell::new().expect("boot");

        // The boot `ShellComponentRegistry` carries `shell.*` blocks
        // only; the builder builtins (`text`, `button`, …) the canvas
        // document references aren't registered there yet. Property-
        // row derivation needs them visible to the registry — extend
        // the live registry with the builder builtins for the
        // duration of this script so the derivation passes the
        // post-click resync actually pin field shapes.
        {
            let mut guard = shell.inner.borrow_mut();
            for spec in BUILTINS {
                let _ = guard.registry.register(SpecBlock::arc(spec));
            }
            // Re-derive the property rows for the pre-selected
            // `demo-heading` now that its component is resolvable.
            let g = &mut *guard;
            let registry = g.registry.as_component_registry();
            g.state.resync_builder_for_selection(Some(registry));
        }
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
}
