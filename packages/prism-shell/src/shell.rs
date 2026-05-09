//! Minimal `Shell` boot — the §17 terminal shape.
//!
//! ```text
//! Shell::new   → parse skeleton + build registry + bindings + Surface
//! Shell::run   → femtovg::run with a handler that re-renders on event
//! ```
//!
//! Per-feature wiring (every command, every mutation, every panel)
//! lands as it's ported off the legacy `app/` modules onto the new
//! `props` / `render` / `events` contract.

use std::cell::RefCell;
use std::rc::Rc;

use crate::components::{register_shell_builtins, ShellComponentRegistry};
use crate::props::ShellPropBindings;
use crate::render::Skeleton;

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("skeleton parse: {0}")]
    Skeleton(String),
    #[error("registry: {0}")]
    Registry(String),
}

/// Per-frame shared state. Currently the registry + bindings + the
/// reloadable `AppState`; legacy modules (store, undo, persistence,
/// VFS, …) re-introduce themselves here as they're ported.
pub struct ShellInner {
    pub registry: ShellComponentRegistry,
    pub bindings: ShellPropBindings,
    pub state: crate::AppState,
}

pub struct Shell {
    pub inner: Rc<RefCell<ShellInner>>,
    pub skeleton: Skeleton,
}

impl Shell {
    pub fn new() -> Result<Self, ShellError> {
        let mut registry = ShellComponentRegistry::new();
        register_shell_builtins(&mut registry).map_err(|e| ShellError::Registry(e.to_string()))?;
        let bindings = ShellPropBindings::with_builtins();
        let skeleton = Skeleton::load().map_err(ShellError::Skeleton)?;
        let inner = Rc::new(RefCell::new(ShellInner {
            registry,
            bindings,
            state: crate::AppState,
        }));
        Ok(Self { inner, skeleton })
    }

    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // TODO(§17): construct `prism_ui_runtime::layout::Surface` from
        // `render::render_tree(&inner, &skeleton, registry, &ctx)`,
        // wire `events::dispatch_event` into the EventHandler closure,
        // then call `prism_ui_runtime::backends::femtovg::run(surface, handler)`.
        // Stubbed until the panel ports land — boot today is a no-op
        // so prism-studio can still link.
        let _ = self.inner;
        let _ = self.skeleton;
        Ok(())
    }
}
