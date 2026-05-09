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

use crate::shell::ShellInner;

pub fn dispatch_event(inner: &Rc<RefCell<ShellInner>>, event: &Event) -> bool {
    match event {
        Event::Resize { width, height } => {
            let mut guard = inner.borrow_mut();
            guard.viewport.width = *width as f32;
            guard.viewport.height = *height as f32;
            true
        }
        // Pointer / Key / Text / Wheel / Focus arms land as the legacy
        // `app/callbacks/{builder,chrome,editor,navigation,overlay,
        // properties}.rs` bodies are ported. Each becomes one arm here
        // forwarding to a `ShellInner::*` mutator. Until the mutators
        // re-introduce themselves on the new shell, every other event
        // is a no-op (no redraw needed — nothing observable changed).
        Event::PointerMove { .. }
        | Event::PointerDown { .. }
        | Event::PointerUp { .. }
        | Event::Wheel { .. }
        | Event::Key { .. }
        | Event::Text { .. }
        | Event::Focus { .. } => false,
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
        let dirty = dispatch_event(&shell.inner, &Event::PointerMove { x: 10.0, y: 10.0 });
        assert!(!dirty);
    }
}
