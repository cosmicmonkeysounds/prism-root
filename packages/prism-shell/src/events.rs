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

use crate::services::EventOutcome;
use crate::shell::ShellInner;

pub fn dispatch_event(inner: &Rc<RefCell<ShellInner>>, event: &Event) -> bool {
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
        Event::PointerDown { x, y, .. } => inner.borrow_mut().state.canvas.pointer_down(*x, *y),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::Shell;

    #[test]
    fn resize_updates_viewport_and_requests_redraw() {
        let shell = Shell::new().expect("boot");
        let dirty = dispatch_event(
            &shell.inner,
            &Event::Resize {
                width: 1024,
                height: 600,
            },
        );
        assert!(dirty, "resize must request a redraw");
        let vp = shell.inner.borrow().viewport;
        assert_eq!(vp.width, 1024.0);
        assert_eq!(vp.height, 600.0);
    }

    #[test]
    fn unhandled_events_are_no_redraw() {
        let shell = Shell::new().expect("boot");
        let dirty = dispatch_event(&shell.inner, &Event::Wheel { dx: 0.0, dy: 1.0 });
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
        assert!(!dispatch_event(&shell.inner, &down), "capture is silent");
        assert!(dispatch_event(&shell.inner, &mv), "move triggers redraw");
        assert!(dispatch_event(&shell.inner, &up), "up triggers redraw");
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
}
