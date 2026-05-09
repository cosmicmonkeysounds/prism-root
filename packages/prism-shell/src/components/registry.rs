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
    reg!("shell.status-bar", StatusBar);
    reg!("shell.workflow-page-button", WorkflowPageButton);
    reg!("shell.workflow-page-bar", WorkflowPageBar);
    reg!("shell.app-window", AppWindow);
    reg!("shell.dock-divider", DockDivider);
    reg!("shell.dock-tab", DockTab);
    reg!("shell.dock-tab-bar", DockTabBar);
    reg!("shell.dock-panel", DockPanel);
    reg!("shell.toast-stack", ToastStack);
    reg!("shell.inspector-tree", InspectorTree);
    reg!("shell.launchpad", Launchpad);
    reg!("shell.command-palette", CommandPalette);
    reg!("shell.help-tooltip", HelpTooltip);
    reg!("shell.menu-item", MenuItem);
    reg!("shell.menu-dropdown", MenuDropdown);
    reg!("shell.context-menu", ContextMenu);
    reg!("shell.docs-sidebar", DocsSidebar);
    reg!("shell.docs-view", DocsView);
    reg!("shell.properties-panel", PropertiesPanel);
    reg!("shell.component-palette", ComponentPalette);
    reg!("shell.explorer", Explorer);
    reg!("shell.signal-connection-row", SignalConnectionRow);
    reg!("shell.signals-panel", SignalsPanel);
    reg!("shell.schema-row", SchemaRow);
    reg!("shell.schema-designer", SchemaDesigner);
    reg!("shell.nav-page-row", NavPageRow);
    reg!("shell.nav-page-list", NavPageList);
    reg!("shell.nav-graph", NavGraph);
    reg!("shell.code-editor", CodeEditor);
    reg!("shell.gizmo-move", GizmoMove);
    reg!("shell.gizmo-rotate", GizmoRotate);
    reg!("shell.gizmo-scale", GizmoScale);
    reg!("shell.resize-handle", ResizeHandle);
    reg!("shell.builder-canvas", BuilderCanvas);
    reg!("shell.component-picker", ComponentPicker);

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
        assert!(reg.get("shell.status-bar").is_some());
        assert!(reg.get("shell.workflow-page-button").is_some());
        assert!(reg.get("shell.workflow-page-bar").is_some());
        assert!(reg.get("shell.app-window").is_some());
        assert!(reg.get("shell.dock-divider").is_some());
        assert!(reg.get("shell.dock-tab").is_some());
        assert!(reg.get("shell.dock-tab-bar").is_some());
        assert!(reg.get("shell.dock-panel").is_some());
        assert!(reg.get("shell.toast-stack").is_some());
        assert!(reg.get("shell.inspector-tree").is_some());
        assert!(reg.get("shell.launchpad").is_some());
        assert!(reg.get("shell.command-palette").is_some());
        assert!(reg.get("shell.help-tooltip").is_some());
        assert!(reg.get("shell.menu-item").is_some());
        assert!(reg.get("shell.menu-dropdown").is_some());
        assert!(reg.get("shell.context-menu").is_some());
        assert!(reg.get("shell.docs-sidebar").is_some());
        assert!(reg.get("shell.docs-view").is_some());
        assert!(reg.get("shell.properties-panel").is_some());
        assert!(reg.get("shell.component-palette").is_some());
        assert!(reg.get("shell.explorer").is_some());
        assert!(reg.get("shell.signal-connection-row").is_some());
        assert!(reg.get("shell.signals-panel").is_some());
        assert!(reg.get("shell.schema-row").is_some());
        assert!(reg.get("shell.schema-designer").is_some());
        assert!(reg.get("shell.nav-page-row").is_some());
        assert!(reg.get("shell.nav-page-list").is_some());
        assert!(reg.get("shell.nav-graph").is_some());
        assert!(reg.get("shell.code-editor").is_some());
        assert!(reg.get("shell.gizmo-move").is_some());
        assert!(reg.get("shell.gizmo-rotate").is_some());
        assert!(reg.get("shell.gizmo-scale").is_some());
        assert!(reg.get("shell.resize-handle").is_some());
        assert!(reg.get("shell.builder-canvas").is_some());
        assert!(reg.get("shell.component-picker").is_some());
        assert_eq!(reg.len(), 47);
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
    fn canonical_app_prism_ui_skeleton_lowers_end_to_end() {
        // The keystone artifact: `ui/app.prism-ui` is the source-driven
        // replacement for `ui/app.slint`. This test loads it from disk
        // (via `include_str!`) and proves the full pipeline — parse →
        // resolver → AppWindow lowering with host_children — produces
        // a coherent UI tree with the inner subtree flowing into the
        // `<main>` content area.
        use prism_core::language::prism_ui::parse;
        use prism_ui_runtime::interpret::{lower_document_with_scope, LowerScope};
        use prism_ui_runtime::layout::Node as UiNode;

        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");

        let source = include_str!("../../ui/app.prism-ui");
        let (doc, errs) = parse(source);
        assert!(errs.is_empty(), "parse errors in app.prism-ui: {errs:?}");

        let scope = LowerScope::default().with_resolver(reg.tag_resolver());
        let nodes = lower_document_with_scope(&doc, &scope);

        // Outer node is the AppWindow (column with menu + body + status).
        let UiNode::Container { id, children, .. } = &nodes[0] else {
            panic!("root not a container")
        };
        assert_eq!(id, "root");
        assert_eq!(children.len(), 3, "menu + body + status");

        // Body row → [activity-bar, content]; the content area carries
        // the `<container id="content-root">` from source.
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
        assert_eq!(content_kids.len(), 1);
        // The content area now hosts a `<shell.dock-panel>` which itself
        // adopts the `<container id="content-root">` author wrote inside
        // it via `host_children` (§14). Walk through the dock-panel to
        // confirm the inner subtree round-trips.
        let UiNode::Container {
            id: dock_id,
            children: dock_kids,
            ..
        } = &content_kids[0]
        else {
            panic!("dock-panel not a container")
        };
        assert_eq!(dock_id, "body");
        // dock-panel body is the last (or only) child since no `tabs` prop.
        let body_section = dock_kids.last().expect("dock-panel body");
        let UiNode::Container {
            children: body_section_kids,
            ..
        } = body_section
        else {
            panic!()
        };
        let UiNode::Container { id: cr_id, .. } = &body_section_kids[0] else {
            panic!("content-root not a container")
        };
        assert_eq!(cr_id, "content-root");

        // The workflow page bar is a sibling of the app-window —
        // overlays / window-relative chrome live as top-level
        // siblings rather than `host_children` of `<shell.app-window>`.
        // §16 frame-chrome step locks this in.
        assert!(
            nodes.len() >= 2,
            "skeleton has at least app-window + workflow-page-bar"
        );
        let UiNode::Container {
            id: wf_id, props, ..
        } = &nodes[1]
        else {
            panic!("workflow-page-bar not a container")
        };
        assert_eq!(wf_id, "workflow");
        assert_eq!(props.semantic.tag.as_deref(), Some("nav"));
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
