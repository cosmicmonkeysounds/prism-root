//! `Shell` — the §17 terminal boot path.
//!
//! ```text
//! Shell::new   → parse skeleton + build registry + bindings + Surface
//! Shell::run   → backend::run with one event handler that re-renders
//!                via `render_tree` whenever `dispatch_event` returns true
//! ```
//!
//! Per-feature wiring (every command, every mutation, every panel)
//! lands as it's ported off the legacy `app/` modules onto the new
//! `props` / `render` / `events` contract — but the supervisor surface
//! stays exactly this shape: one `render_tree` call, one
//! `dispatch_event` arm per runtime variant.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use prism_ui_runtime::event::Event;
use prism_ui_runtime::interpret::TagResolver;
use prism_ui_runtime::layout::{HitRect, Node as UiNode, Surface, Viewport};

use crate::components::{
    register_document_builtins, register_shell_builtins, ShellComponentRegistry,
};
use crate::events::dispatch_event;
use crate::props::{PropCtx, ShellPropBindings};
use crate::render::{render_tree, Skeleton};
use crate::services::{
    Clipboard, LuauHost, MutCtx, NoopLuauHost, OsVfs, ServiceRegistry, UndoStack, Vfs,
};

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("skeleton parse: {0}")]
    Skeleton(String),
    #[error("registry: {0}")]
    Registry(String),
    #[error("runtime: {0}")]
    Runtime(String),
}

/// Per-frame shared state. Currently the registry + bindings + the
/// reloadable `AppState`; legacy modules (store, undo, persistence,
/// VFS, …) re-introduce themselves as fields here as they're ported.
pub struct ShellInner {
    pub registry: ShellComponentRegistry,
    pub resolver: Arc<dyn TagResolver>,
    pub bindings: ShellPropBindings,
    pub services: ServiceRegistry,
    pub state: crate::AppState,
    pub viewport: Viewport,
    pub undo: UndoStack,
    /// IO seam — `OsVfs` in production, mock in tests. Borrowed
    /// `&mut` into every `MutCtx` so `PersistenceService` /
    /// `ProjectService` can read/write without owning a fs handle.
    pub vfs: Box<dyn Vfs>,
    /// Luau seam — `NoopLuauHost` until the `mlua`-backed runtime
    /// lands. `SignalsService::Custom` and `LuauService::run-selection`
    /// reach Luau through this single resource.
    pub luau: Box<dyn LuauHost>,
    /// In-memory clipboard cell — `ClipboardService` (§25) is the
    /// only consumer.
    pub clipboard: Clipboard,
}

impl ShellInner {
    /// Build the per-frame `PropCtx` borrow-pack. Every binding closure
    /// destructures the fields it needs; adding a new datum is one
    /// field on `PropCtx` and one assignment here.
    pub fn prop_ctx(&self) -> PropCtx<'_> {
        PropCtx {
            state: &self.state,
            viewport_w: self.viewport.width,
            viewport_h: self.viewport.height,
            canvas_zoom: 1.0,
            // §43 B3: bindings that lower host-side trees (currently
            // `shell.builder-canvas` rendering `state.canvas.document`)
            // call into the live registry through this field. Pure
            // slot-accessor bindings ignore it.
            registry: Some(self.registry.as_component_registry()),
        }
    }

    /// Sister to [`Self::prop_ctx`] for the §24 write side. Every
    /// service handler and every command body takes one of these.
    /// Adding a new datum = one field on `MutCtx` and one assignment
    /// here.
    pub fn mut_ctx(&mut self) -> MutCtx<'_> {
        // Split-borrow: the registry comes from `self.registry`
        // (immutable), every other field comes from `self` (mutable).
        // Re-borrow explicitly so the borrow checker sees the disjoint
        // slices.
        let registry = self.registry.as_component_registry();
        MutCtx {
            state: &mut self.state,
            viewport: self.viewport,
            undo: &mut self.undo,
            vfs: self.vfs.as_mut(),
            luau: self.luau.as_mut(),
            clipboard: &mut self.clipboard,
            registry: Some(registry),
        }
    }
}

pub struct Shell {
    pub inner: Rc<RefCell<ShellInner>>,
    pub skeleton: Skeleton,
}

impl Shell {
    pub fn new() -> Result<Self, ShellError> {
        let mut registry = ShellComponentRegistry::new();
        register_shell_builtins(&mut registry).map_err(|e| ShellError::Registry(e.to_string()))?;
        register_document_builtins(&mut registry)
            .map_err(|e| ShellError::Registry(e.to_string()))?;
        let resolver = registry.tag_resolver();
        let bindings = ShellPropBindings::with_builtins();
        let services = ServiceRegistry::with_builtins();
        let skeleton = Skeleton::load().map_err(ShellError::Skeleton)?;
        let inner = Rc::new(RefCell::new(ShellInner {
            registry,
            resolver,
            bindings,
            services,
            // §43 A1: hydrated boot state. `AppState::default()` is the
            // zero-data shape for tests and headless renders;
            // `Shell::new` boots into a populated catalog + canvas
            // document so the first frame looks like Studio.
            state: crate::seed::initial_state(),
            viewport: Viewport {
                width: 1280.0,
                height: 800.0,
            },
            undo: UndoStack::default(),
            vfs: Box::new(OsVfs),
            luau: Box::new(NoopLuauHost::default()),
            clipboard: Clipboard::default(),
        }));
        // §43 C1: one-shot post-boot resync. The seed sets selection
        // and the inspector tree, but `derive_property_rows` needs the
        // live registry — which `seed::initial_state` doesn't have.
        // Running it once here means the boot frame's properties panel
        // is already populated for the pre-selected node.
        {
            let mut guard = inner.borrow_mut();
            let g = &mut *guard;
            let registry = g.registry.as_component_registry();
            g.state.resync_builder_for_selection(Some(registry));
        }
        Ok(Self { inner, skeleton })
    }

    /// Build the initial runtime tree. Pure function of `(skeleton,
    /// bindings, resolver, ctx)` — exposed so tests, alternate hosts,
    /// and the per-frame redraw closure all hit the same path.
    pub fn render(&self) -> Vec<UiNode> {
        let inner = self.inner.borrow();
        render_tree(
            &self.skeleton,
            &inner.bindings,
            Arc::clone(&inner.resolver),
            &inner.prop_ctx(),
        )
    }

    #[cfg(feature = "native")]
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let initial = wrap_root(self.render());
        let viewport = self.inner.borrow().viewport;
        let surface = Surface::new(initial, viewport);

        let inner = Rc::clone(&self.inner);
        let skeleton = self.skeleton.clone();
        let handler: prism_ui_runtime::event::EventHandler = Box::new(move |event, surface| {
            // Single hit-test per pointer event. Reused for: hover
            // paint (`set_hovered` on PointerMove), click routing
            // (PointerDown chrome / canvas-node routes), and the
            // click-without-drag step on PointerUp. Non-pointer
            // events yield `None` and pass through cleanly.
            let hit = compute_hit(event, surface);
            // Hover paint: PointerMove syncs `Surface::hovered_id`
            // so every container declaring `props.hover` (inspector
            // rows, icon buttons, nav buttons, menu items, tabs, app
            // cards, drag-number fields, ...) tints as the cursor
            // passes. `set_hovered` is a no-op when the id hasn't
            // changed and only marks the surface dirty when at
            // least one side of the transition has hover overrides
            // — clean hovers stay clean.
            if matches!(event, Event::PointerMove { .. }) {
                surface.set_hovered(hit.as_ref().map(|h| h.id.clone()));
            }
            if dispatch_event(&inner, event, hit) {
                let guard = inner.borrow();
                let tree = render_tree(
                    &skeleton,
                    &guard.bindings,
                    Arc::clone(&guard.resolver),
                    &guard.prop_ctx(),
                );
                surface.set_tree(wrap_root(tree));
            }
        });

        prism_ui_runtime::backends::femtovg::run(surface, handler, crate::assets::loader())
            .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))
    }

    /// On the web target the femtovg backend isn't compiled in; the
    /// stub returns immediately so `web_start` links until the
    /// `prism-ui-runtime/web` backend's `run` is wired up.
    #[cfg(not(feature = "native"))]
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.render();
        Ok(())
    }
}

/// `Surface` takes a single root `Node`. The skeleton lowers to a
/// flat `Vec<Node>` (app-window + workflow-page-bar + overlay
/// siblings); wrap them in an anonymous Column container that fills
/// the viewport so the app-window (`Sizing::Grow`) and the
/// workflow-page-bar (`Sizing::Fixed(32)`) both land at their
/// intended sizes.
///
/// Without `Sizing::Grow` on both axes here, the wrapper would
/// collapse to its content's intrinsic size (`Sizing::Fit`), and a
/// Taffy flex column anchored at viewport(1280×800) with auto-width
/// children would have ambiguous cross-axis stretching — visible as
/// the menu bar text wrapping mid-word when label widths exceed the
/// shrunk column.
fn wrap_root(children: Vec<UiNode>) -> UiNode {
    UiNode::Container {
        id: String::new(),
        props: prism_ui_runtime::layout::ContainerProps {
            direction: prism_ui_runtime::layout::Direction::Column,
            width: prism_ui_runtime::layout::Sizing::Grow,
            height: prism_ui_runtime::layout::Sizing::Grow,
            ..Default::default()
        },
        children,
    }
}

/// Pointer-event hit-test. One closed-form helper so every pointer
/// variant — `PointerMove` for hover paint, `PointerDown` for click
/// routing, `PointerUp` for the no-drag click-step fall-through —
/// resolves through the same `Surface::hit_test_at` call. Non-pointer
/// events (Wheel / Key / Text / Focus / Resize) yield `None` and pass
/// through dispatch unchanged.
#[cfg(feature = "native")]
fn compute_hit(event: &Event, surface: &mut Surface) -> Option<HitRect> {
    let (x, y) = pointer_xy(event)?;
    surface.hit_test_at(x, y).cloned()
}

fn pointer_xy(event: &Event) -> Option<(f32, f32)> {
    match event {
        Event::PointerMove { x, y }
        | Event::PointerDown { x, y, .. }
        | Event::PointerUp { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_new_succeeds() {
        let shell = Shell::new().expect("boot");
        let nodes = shell.render();
        assert!(!nodes.is_empty());
    }

    #[test]
    fn render_is_deterministic() {
        let shell = Shell::new().expect("boot");
        let a = shell.render();
        let b = shell.render();
        assert_eq!(a, b, "two consecutive renders must be equal");
    }

    #[test]
    fn pointer_xy_extracts_position_for_every_pointer_variant() {
        use prism_ui_runtime::event::PointerButton;
        // The shell's hover + click routing assumes every pointer
        // variant yields a position; non-pointer events explicitly
        // pass through dispatch unchanged.
        assert_eq!(
            pointer_xy(&Event::PointerMove { x: 1.0, y: 2.0 }),
            Some((1.0, 2.0))
        );
        assert_eq!(
            pointer_xy(&Event::PointerDown {
                x: 3.0,
                y: 4.0,
                button: PointerButton::Primary,
            }),
            Some((3.0, 4.0))
        );
        assert_eq!(
            pointer_xy(&Event::PointerUp {
                x: 5.0,
                y: 6.0,
                button: PointerButton::Primary,
            }),
            Some((5.0, 6.0))
        );
        assert_eq!(pointer_xy(&Event::Wheel { dx: 0.0, dy: 1.0 }), None);
        assert_eq!(pointer_xy(&Event::Focus { gained: true }), None);
    }

    #[test]
    fn hover_pump_marks_surface_dirty_when_passing_over_hover_aware_node() {
        // The femtovg backend's redraw loop polls `Surface::is_dirty()`
        // after every event. The shell's hover pump must therefore
        // request a redraw when the cursor enters a container whose
        // `props.hover` is set — without this, every chrome tint that
        // declares a hover override stays dead in production.
        use prism_ui_runtime::command::{Color, CornerRadius};
        use prism_ui_runtime::layout::{ContainerProps, HoverOverrides, Padding, Sizing};
        let tree = UiNode::Container {
            id: String::new(),
            props: ContainerProps::default(),
            children: vec![UiNode::Container {
                id: "hot-button".into(),
                props: ContainerProps {
                    width: Sizing::Fixed(100.0),
                    height: Sizing::Fixed(40.0),
                    padding: Padding::default(),
                    hover: Some(HoverOverrides {
                        background: Some(Color {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 32,
                        }),
                        radius: Some(CornerRadius {
                            tl: 4.0,
                            tr: 4.0,
                            br: 4.0,
                            bl: 4.0,
                        }),
                    }),
                    ..Default::default()
                },
                children: Vec::new(),
            }],
        };
        let mut surface = Surface::new(
            tree,
            Viewport {
                width: 200.0,
                height: 200.0,
            },
        );
        // Prime the layout cache; this is what the femtovg backend does
        // on first redraw, after which `is_dirty()` returns false until
        // something mutates.
        let _ = surface.commands();
        assert!(!surface.is_dirty());

        // Simulate the shell's hover-pump path: compute a hit at a
        // point inside the button's bounds and call `set_hovered`.
        let event = Event::PointerMove { x: 10.0, y: 10.0 };
        let hit = compute_hit(&event, &mut surface);
        assert_eq!(hit.as_ref().map(|h| h.id.as_str()), Some("hot-button"));
        surface.set_hovered(hit.as_ref().map(|h| h.id.clone()));
        assert!(
            surface.is_dirty(),
            "entering a hover-aware node must dirty the surface so the \
             backend redraws with the tint applied"
        );

        // Leaving the node back to nowhere should re-dirty the surface
        // so the tint clears.
        let _ = surface.commands();
        let event = Event::PointerMove { x: 199.0, y: 199.0 };
        let hit = compute_hit(&event, &mut surface);
        assert!(hit.is_none());
        surface.set_hovered(None);
        assert!(
            surface.is_dirty(),
            "leaving a hover-aware node must re-dirty so the tint clears"
        );
    }
}
