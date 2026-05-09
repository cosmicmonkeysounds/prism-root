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

use prism_builder::{
    ui_resolver::RegistryTagResolver, Block, Component, ComponentRegistry, RegistryError,
};
use prism_ui_runtime::interpret::TagResolver;

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

    /// Build a [`TagResolver`] for `.prism-ui` source that references
    /// any registered shell primitive by tag (e.g.
    /// `<shell.icon-button …/>`). Hand the returned `Arc` to
    /// [`prism_ui_runtime::interpret::LowerScope::with_resolver`] and
    /// the runtime will dispatch unknown tags through this registry's
    /// `Component::lower_ui` impls.
    ///
    /// Smart-pattern note: this is one method, not a separate
    /// "shell-DSL renderer" — the runtime extension seam is the same
    /// `TagResolver` trait every other host (relay SSR, future
    /// plugin-provided component sets) plugs into.
    pub fn tag_resolver(&self) -> Arc<dyn TagResolver> {
        Arc::new(RegistryTagResolver::new(Arc::new(self.inner.clone())))
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
    reg!("shell.docs-content", DocsContent);
    reg!("shell.app-card", AppCard);
    reg!("shell.drag-number-field", DragNumberField);
    reg!("shell.inspector-row", InspectorRow);
    reg!("shell.transform-editor", TransformEditor);
    reg!("shell.menu-bar-row", MenuBarRow);
    reg!("shell.field-editor", FieldEditor);
    reg!("shell.app-window", AppWindow);

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
        assert!(reg.get("shell.docs-content").is_some());
        assert!(reg.get("shell.app-card").is_some());
        assert!(reg.get("shell.drag-number-field").is_some());
        assert!(reg.get("shell.inspector-row").is_some());
        assert!(reg.get("shell.transform-editor").is_some());
        assert!(reg.get("shell.menu-bar-row").is_some());
        assert!(reg.get("shell.field-editor").is_some());
        assert!(reg.get("shell.app-window").is_some());
        assert_eq!(reg.len(), 13);
    }

    #[test]
    fn rejects_double_registration() {
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("first");
        let err = register_shell_builtins(&mut reg).expect_err("dup");
        assert!(matches!(err, RegistryError::AlreadyRegistered(_)));
    }

    #[test]
    fn tag_resolver_lowers_shell_icon_button_from_prism_ui_source() {
        use prism_core::language::prism_ui::parse;
        use prism_ui_runtime::interpret::{lower_document_with_scope, LowerScope};
        use prism_ui_runtime::layout::Node as UiNode;

        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");

        let scope = LowerScope::default().with_resolver(reg.tag_resolver());
        let (doc, errs) = parse(
            r##"<container>
                <shell.icon-button id="ib" icon="icons/x.svg" tooltip-text="Close"/>
            </container>"##,
        );
        assert!(errs.is_empty(), "parse errors: {errs:?}");

        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { children, .. } = &nodes[0] else {
            panic!("root not a container")
        };
        assert_eq!(children.len(), 1);
        // Resolver dispatched into IconButton::lower_ui — the result
        // is the 28×28 icon-button frame from `chrome.rs`.
        let UiNode::Container { id, props, .. } = &children[0] else {
            panic!("icon button did not lower to a container")
        };
        assert_eq!(id, "ib");
        assert_eq!(props.semantic.tag.as_deref(), Some("button"));
        assert_eq!(props.semantic.aria_label.as_deref(), Some("Close"));
    }

    #[test]
    fn tag_resolver_unknown_tag_falls_through_to_runtime_default() {
        use prism_core::language::prism_ui::parse;
        use prism_ui_runtime::interpret::{lower_document_with_scope, LowerScope};
        use prism_ui_runtime::layout::Node as UiNode;

        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");

        let (doc, _) = parse(r##"<scene><text>kept</text></scene>"##);
        let scope = LowerScope::default().with_resolver(reg.tag_resolver());
        let nodes = lower_document_with_scope(&doc, &scope);
        // Unknown tag drops the wrapper, surfaces children.
        assert!(matches!(nodes[0], UiNode::Text { .. }));
    }

    #[test]
    fn tag_resolver_lowers_app_window_with_prism_ui_authored_children() {
        // Composition-style block: `<shell.app-window>…</shell.app-window>`
        // hosting real subtrees from `.prism-ui` source. Exercises the
        // resolver-children seam (RegistryTagResolver pre-lowers AST
        // children → LowerCtx::host_children → AppWindow consumes).
        // 12/13 chrome blocks ignore the slot; AppWindow is the
        // canonical opt-in consumer.
        use prism_core::language::prism_ui::parse;
        use prism_ui_runtime::interpret::{lower_document_with_scope, LowerScope};
        use prism_ui_runtime::layout::Node as UiNode;

        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");

        let (doc, errs) = parse(
            r##"<shell.app-window id="aw" status="Ready">
                <text>greeting</text>
                <text>tagline</text>
            </shell.app-window>"##,
        );
        assert!(errs.is_empty(), "parse errors: {errs:?}");

        let scope = LowerScope::default().with_resolver(reg.tag_resolver());
        let nodes = lower_document_with_scope(&doc, &scope);
        let UiNode::Container { id, children, .. } = &nodes[0] else {
            panic!("expected app-window container")
        };
        assert_eq!(id, "aw");
        assert_eq!(children.len(), 3, "menu + body + status");
        // Body row → [activity-bar, content]; content adopts the
        // pre-lowered AST children verbatim.
        let UiNode::Container {
            children: body_kids,
            ..
        } = &children[1]
        else {
            panic!()
        };
        let UiNode::Container {
            children: content_kids,
            props: content_props,
            ..
        } = &body_kids[1]
        else {
            panic!("content area not a container")
        };
        assert_eq!(content_props.semantic.tag.as_deref(), Some("main"));
        assert_eq!(content_kids.len(), 2);
        assert!(matches!(content_kids[0], UiNode::Text { .. }));
        assert!(matches!(content_kids[1], UiNode::Text { .. }));
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
