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

use prism_ui_runtime::interpret::TagResolver;
use prism_ui_runtime::layout::{Node as UiNode, Surface, Viewport};

use crate::components::{register_shell_builtins, ShellComponentRegistry};
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
        }
    }

    /// Sister to [`Self::prop_ctx`] for the §24 write side. Every
    /// service handler and every command body takes one of these.
    /// Adding a new datum = one field on `MutCtx` and one assignment
    /// here.
    pub fn mut_ctx(&mut self) -> MutCtx<'_> {
        MutCtx {
            state: &mut self.state,
            viewport: self.viewport,
            undo: &mut self.undo,
            vfs: self.vfs.as_mut(),
            luau: self.luau.as_mut(),
            clipboard: &mut self.clipboard,
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
            if dispatch_event(&inner, event) {
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

        prism_ui_runtime::backends::femtovg::run(surface, handler)
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
/// flat `Vec<Node>` (app-window + overlay siblings); wrap them in an
/// anonymous container so the surface has one entry point.
fn wrap_root(children: Vec<UiNode>) -> UiNode {
    UiNode::Container {
        id: String::new(),
        props: Default::default(),
        children,
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
}
