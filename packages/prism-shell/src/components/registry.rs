//! [`ShellComponentRegistry`] — sibling of [`prism_builder::ComponentRegistry`]
//! for chrome / shell primitives (icon-button, toolbar-separator,
//! menu-bar-row, …) that must NOT appear in the user-visible document
//! component palette.
//!
//! The registry is a thin newtype around `ComponentRegistry` so we reuse:
//!
//! * the [`prism_builder::Block`] trait surface (one render method,
//!   `lower_ui`, feeding the unified Taffy + SSR pipeline),
//! * the [`prism_builder::BlockSpec`] declarative form (one `const SPEC`
//!   per primitive, no per-component struct or trait impl),
//! * the [`prism_builder::ui_lower::LowerCtx`] cascade machinery,
//!
//! …and gain the *type distinction* that keeps shell primitives out of
//! `ComponentRegistry::iter()` consumers like the Studio component
//! palette and the document help index.
//!
//! Adding a new shell primitive is **two lines of changes**:
//!
//! 1. In `components/foo.rs`, write a `foo_lower(ctx, node, style) ->
//!    UiNode` free function and a
//!    `pub const FOO_SPEC: BlockSpec = BlockSpec::new("shell.foo", foo_schema).lower(foo_lower)`.
//! 2. Add `&super::foo::FOO_SPEC` to the [`SHELL_BUILTINS`] table.
//!
//! See `docs/dev/clay-migration-plan.md` §12 + §33 for the broader strategy.

use std::sync::Arc;

use prism_builder::{
    register_specs, ui_resolver::RegistryTagResolver, Block, BlockSpec, Component,
    ComponentRegistry, RegistryError,
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
    /// walker. `prism_builder::ui_runtime::lower_*` accepts
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

/// Single source of truth for the shell primitive catalog. Each row is
/// a `&'static BlockSpec` declared in the matching `components/foo.rs`
/// file. Adding a primitive = one new const + one row here.
pub static SHELL_BUILTINS: &[&BlockSpec] = &[
    &super::icon_button::ICON_BUTTON_SPEC,
    &super::toolbar_separator::TOOLBAR_SEPARATOR_SPEC,
    &super::section_header::SECTION_HEADER_SPEC,
    &super::nav_button::NAV_BUTTON_SPEC,
    &super::toast::TOAST_SPEC,
    &super::docs_content::DOCS_CONTENT_SPEC,
    &super::app_card::APP_CARD_SPEC,
    &super::drag_number_field::DRAG_NUMBER_FIELD_SPEC,
    &super::inspector_row::INSPECTOR_ROW_SPEC,
    &super::transform_editor::TRANSFORM_EDITOR_SPEC,
    &super::menu_bar_row::MENU_BAR_ROW_SPEC,
    &super::field_editor::FIELD_EDITOR_SPEC,
    &super::status_bar::STATUS_BAR_SPEC,
    &super::workflow_page_button::WORKFLOW_PAGE_BUTTON_SPEC,
    &super::workflow_page_bar::WORKFLOW_PAGE_BAR_SPEC,
    &super::app_window::APP_WINDOW_SPEC,
    &super::dock_divider::DOCK_DIVIDER_SPEC,
    &super::dock_tab::DOCK_TAB_SPEC,
    &super::dock_tab_bar::DOCK_TAB_BAR_SPEC,
    &super::dock_panel::DOCK_PANEL_SPEC,
    &super::dock_workspace::DOCK_WORKSPACE_SPEC,
    &super::toast_stack::TOAST_STACK_SPEC,
    &super::inspector_tree::INSPECTOR_TREE_SPEC,
    &super::launchpad::LAUNCHPAD_SPEC,
    &super::command_palette::COMMAND_PALETTE_SPEC,
    &super::help_tooltip::HELP_TOOLTIP_SPEC,
    &super::menu_item::MENU_ITEM_SPEC,
    &super::menu_dropdown::MENU_DROPDOWN_SPEC,
    &super::context_menu::CONTEXT_MENU_SPEC,
    &super::docs_sidebar::DOCS_SIDEBAR_SPEC,
    &super::docs_view::DOCS_VIEW_SPEC,
    &super::properties_panel::PROPERTIES_PANEL_SPEC,
    &super::component_palette::COMPONENT_PALETTE_SPEC,
    &super::explorer::EXPLORER_SPEC,
    &super::signal_connection_row::SIGNAL_CONNECTION_ROW_SPEC,
    &super::signals_panel::SIGNALS_PANEL_SPEC,
    &super::schema_row::SCHEMA_ROW_SPEC,
    &super::schema_designer::SCHEMA_DESIGNER_SPEC,
    &super::nav_page_row::NAV_PAGE_ROW_SPEC,
    &super::nav_page_list::NAV_PAGE_LIST_SPEC,
    &super::nav_graph::NAV_GRAPH_SPEC,
    &super::code_editor::CODE_EDITOR_SPEC,
    &super::gizmo_move::GIZMO_MOVE_SPEC,
    &super::gizmo_rotate::GIZMO_ROTATE_SPEC,
    &super::gizmo_scale::GIZMO_SCALE_SPEC,
    &super::resize_handle::RESIZE_HANDLE_SPEC,
    &super::builder_canvas::BUILDER_CANVAS_SPEC,
    &super::builder_toolbar::BUILDER_TOOLBAR_SPEC,
    &super::component_picker::COMPONENT_PICKER_SPEC,
    // Wave 1 — `docs/dev/composable-builder-plan.md`. The composable-
    // inspector trio: one section header per attached behaviour, an
    // add-modifier footer, and an overlay picker.
    &super::modifier_header::MODIFIER_HEADER_SPEC,
    &super::add_modifier_button::ADD_MODIFIER_BUTTON_SPEC,
    &super::modifier_picker::MODIFIER_PICKER_SPEC,
];

/// Register every spec in [`SHELL_BUILTINS`]. One-line fan-out via
/// [`register_specs`].
pub fn register_shell_builtins(reg: &mut ShellComponentRegistry) -> Result<(), RegistryError> {
    register_specs(&mut reg.inner, SHELL_BUILTINS)
}

/// Merge the document-side builder catalog (`prism_builder::starter::BUILTINS`
/// plus the `card` prefab and `facet` component) into the live shell
/// registry. Required so `select_node` →
/// `resync_builder_for_selection` can resolve schemas for builder
/// nodes (`text` / `button` / …) and populate the inspector property
/// rows. The shell registry and the document builtins share a flat id
/// namespace; `register_specs` rejects duplicates by returning
/// `RegistryError::AlreadyRegistered`, so collisions surface at boot
/// rather than as silent shadowing.
pub fn register_document_builtins(reg: &mut ShellComponentRegistry) -> Result<(), RegistryError> {
    prism_builder::starter::register_builtins(&mut reg.inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shell_builtin_resolves_after_registration() {
        // Drive directly from the source-of-truth `SHELL_BUILTINS`
        // table — adding a row there means this test covers it
        // automatically. The previous shape (48 hand-rolled `get`
        // calls) drifted the moment a primitive landed without its
        // assertion; the table-driven shape can't drift.
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        for spec in SHELL_BUILTINS {
            assert!(
                reg.get(spec.id).is_some(),
                "`{}` declared in SHELL_BUILTINS but not resolvable post-registration",
                spec.id
            );
        }
        assert_eq!(reg.len(), SHELL_BUILTINS.len());
    }

    #[test]
    fn shell_builtin_ids_are_namespaced_and_unique() {
        // Every row in the table belongs to the `shell.*` namespace
        // (so the chrome registry never bleeds into document-side
        // palettes), and ids are unique. The dedup check overlaps
        // with `rejects_double_registration` but operates on the
        // *table* — failures here point at the literal source, not
        // the registration code.
        let mut seen = std::collections::HashSet::new();
        for spec in SHELL_BUILTINS {
            assert!(
                spec.id.starts_with("shell."),
                "id `{}` must use the shell.* namespace",
                spec.id
            );
            assert!(
                seen.insert(spec.id),
                "duplicate id `{}` in SHELL_BUILTINS",
                spec.id
            );
        }
    }

    #[test]
    fn rejects_double_registration() {
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("first");
        let err = register_shell_builtins(&mut reg).expect_err("dup");
        assert!(matches!(err, RegistryError::AlreadyRegistered(_)));
    }

    /// Wave 5.2 of `docs/dev/composable-builder-plan.md` — boot-time
    /// contract that the shell namespace (`shell.*`) and the builder
    /// document catalog (`prism_builder::starter::BUILTINS` + the
    /// `card` prefab + the `facet` component) carry disjoint ids.
    /// `register_specs` rejects duplicates at runtime via
    /// `RegistryError::AlreadyRegistered`, so a collision *would*
    /// crash `Shell::new` at the second registration call — but a
    /// named pin makes the contract searchable and surfaces the
    /// offending id directly instead of as a boot-failure backtrace.
    #[test]
    fn no_overlapping_block_ids_between_shell_and_starter() {
        use std::collections::HashSet;

        let shell_ids: HashSet<&str> = SHELL_BUILTINS.iter().map(|spec| spec.id).collect();

        let mut doc_ids: HashSet<&str> = prism_builder::starter::BUILTINS
            .iter()
            .map(|spec| spec.id)
            .collect();
        doc_ids.insert("card");
        doc_ids.insert("facet");

        let collisions: Vec<&str> = shell_ids.intersection(&doc_ids).copied().collect();
        assert!(
            collisions.is_empty(),
            "shell and document catalogs must carry disjoint ids; \
             collisions: {collisions:?}",
        );

        // And the merge actually succeeds — the runtime guarantee
        // backing the static set check.
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("shell");
        register_document_builtins(&mut reg).expect("document");
        assert_eq!(
            reg.len(),
            shell_ids.len() + doc_ids.len(),
            "merged registry size must equal the sum of disjoint catalogs",
        );
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
        // The content area now hosts a `<shell.dock-workspace>` that
        // recursively walks the active page's DockNode tree. Its
        // outer container is tagged `data-role="dock-workspace"`.
        let UiNode::Container {
            id: ws_id,
            props: ws_props,
            ..
        } = &content_kids[0]
        else {
            panic!("dock-workspace not a container")
        };
        assert_eq!(ws_id, "dock");
        assert!(ws_props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "dock-workspace"));

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
    fn document_builtins_resolve_alongside_shell_builtins() {
        // §43 D1: merging `prism_builder::starter::register_builtins`
        // into the shell registry is what lets `select_node` →
        // `resync_builder_for_selection` find schemas for document
        // nodes (`text` / `button` / …). The two id sets are disjoint
        // (`shell.*` vs unprefixed), so the merge can't shadow.
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("shell");
        register_document_builtins(&mut reg).expect("document");
        // A representative shell tag still resolves…
        assert!(reg.get("shell.icon-button").is_some());
        // …and the merged builder builtins do too.
        for id in ["text", "button", "image", "container", "form", "list"] {
            assert!(
                reg.get(id).is_some(),
                "builder builtin `{id}` not resolvable after document merge"
            );
        }
    }

    #[test]
    fn shell_and_document_builtin_ids_are_disjoint() {
        // Lock the namespace invariant: a future shell-side primitive
        // that drops the `shell.` prefix (or a document-side block that
        // adopts it) would collide. `register_document_builtins`
        // surfaces collisions as `RegistryError::AlreadyRegistered`,
        // but this test catches them at the table level so failures
        // point at the literal id, not the registration order.
        let shell_ids: std::collections::HashSet<&str> =
            SHELL_BUILTINS.iter().map(|s| s.id).collect();
        for spec in prism_builder::starter::BUILTINS {
            assert!(
                !shell_ids.contains(spec.id),
                "document builtin `{}` collides with a shell.* id",
                spec.id
            );
        }
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
