//! [`ShellComponentRegistry`] — sibling of [`prism_builder::ComponentRegistry`]
//! for chrome / shell primitives (IconButton, ToolbarSeparator, MenuBarRow,
//! …) that must NOT appear in the user-visible document component palette.
//!
//! The registry is a thin newtype around `ComponentRegistry` so we reuse:
//!
//! * the [`prism_builder::Block`] trait surface (one render method per
//!   target — `lower_ui` for the unified Taffy/SSR pipeline, `render_slint`
//!   during the parallel-build period),
//! * the [`prism_builder::ui_lower::LowerCtx`] cascade machinery,
//! * the existing `register_block` flow,
//!
//! …and gain the *type distinction* that keeps shell primitives out of
//! `ComponentRegistry::iter()` consumers like the Studio component
//! palette and the document help index.
//!
//! Adding a new shell primitive is the same three-step recipe as adding
//! a builder block (see `prism-builder/CLAUDE.md`):
//!
//! 1. `impl Block for MyShellComponent { … }` — schema + `lower_ui`.
//! 2. Add a row to [`register_shell_builtins`]'s `reg!(…)` table.
//! 3. Author it inside `.prism-ui` source as `<my-shell-component …/>`.
//!
//! See `docs/dev/clay-migration-plan.md` §12 for the broader strategy.

use std::sync::Arc;

use prism_builder::{Block, Component, ComponentRegistry, RegistryError};

/// Component registry for shell-only primitives. Distinct type from
/// `ComponentRegistry` so shell components and document blocks never
/// share a namespace by accident, but mechanically delegates so adding
/// new primitives needs zero new infrastructure.
#[derive(Default)]
pub struct ShellComponentRegistry {
    inner: ComponentRegistry,
}

impl ShellComponentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a shell block. Same shape as
    /// [`prism_builder::register_block`] — the input is a `Block` impl,
    /// the [`Component`] blanket impl makes it `ComponentRegistry`-shaped
    /// for free.
    pub fn register<T: Block + 'static>(&mut self, block: Arc<T>) -> Result<(), RegistryError> {
        self.inner.register(block as Arc<dyn Component>)
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Component>> {
        self.inner.get(id)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Borrow the underlying `ComponentRegistry`. The relay / SSR walker
    /// and the Taffy lowering both expect a `&ComponentRegistry` — this
    /// is the seam that lets shell components plug in without a parallel
    /// walker. `prism_builder::ui_runtime::lower_*_with_registry` accepts
    /// the borrow returned here.
    pub fn as_component_registry(&self) -> &ComponentRegistry {
        &self.inner
    }
}

/// Register every built-in shell component. Mirrors
/// `prism_builder::starter::register_builtins` — adding a new shell
/// primitive is one line in the `reg!` macro table.
pub fn register_shell_builtins(reg: &mut ShellComponentRegistry) -> Result<(), RegistryError> {
    macro_rules! reg {
        ($id:literal, $ty:ident) => {
            reg.register(Arc::new(super::$ty { id: $id.into() }))?;
        };
    }

    reg!("shell.icon-button", IconButton);
    reg!("shell.toolbar-separator", ToolbarSeparator);
    reg!("shell.section-header", SectionHeader);
    reg!("shell.nav-button", NavButton);
    reg!("shell.toast", Toast);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_icon_button() {
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        assert!(reg.get("shell.icon-button").is_some());
        assert!(reg.get("shell.toolbar-separator").is_some());
        assert!(reg.get("shell.section-header").is_some());
        assert!(reg.get("shell.nav-button").is_some());
        assert!(reg.get("shell.toast").is_some());
        assert_eq!(reg.len(), 5);
    }

    #[test]
    fn rejects_double_registration() {
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("first");
        let err = register_shell_builtins(&mut reg).expect_err("dup");
        assert!(matches!(err, RegistryError::AlreadyRegistered(_)));
    }

    #[test]
    fn underlying_component_registry_is_borrowable() {
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        // The relay-shaped consumer that takes `&ComponentRegistry` works.
        let cr = reg.as_component_registry();
        assert!(cr.get("shell.icon-button").is_some());
    }
}
