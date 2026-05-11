//! Shell-only components — toolbar / chrome / overlay primitives that
//! must NOT show up in the user-visible document component palette.
//!
//! Every primitive is declared as a [`prism_builder::BlockSpec`] const
//! and registered into a sibling [`ShellComponentRegistry`] via the
//! `SHELL_BUILTINS` table in `registry.rs`. The strategy and rationale
//! are documented in `docs/dev/clay-migration-plan.md` §12 + §33.

pub mod app_card;
pub mod app_window;
pub mod builder_canvas;
pub mod builder_toolbar;
pub mod chrome;
pub mod code_editor;
pub mod command_palette;
pub mod component_palette;
pub mod component_picker;
pub mod context_menu;
pub mod dock_divider;
pub mod dock_panel;
pub mod dock_tab;
pub mod dock_tab_bar;
pub mod dock_workspace;
pub mod docs_content;
pub mod docs_sidebar;
pub mod docs_view;
pub mod drag_number_field;
pub mod explorer;
pub mod field_editor;
pub mod gizmo_move;
pub mod gizmo_rotate;
pub mod gizmo_scale;
pub mod help_tooltip;
pub mod icon_button;
pub mod inspector_row;
pub mod inspector_tree;
pub mod launchpad;
pub mod menu_bar_row;
pub mod menu_dropdown;
pub mod menu_item;
pub mod nav_button;
pub mod nav_graph;
pub mod nav_page_list;
pub mod nav_page_row;
pub mod properties_panel;
pub mod registry;
pub mod resize_handle;
pub mod schema_designer;
pub mod schema_row;
pub mod section_header;
pub mod signal_connection_row;
pub mod signals_panel;
pub mod status_bar;
pub mod toast;
pub mod toast_stack;
pub mod toolbar_separator;
pub mod transform_editor;
pub mod workflow_page_bar;
pub mod workflow_page_button;

pub use registry::{register_shell_builtins, ShellComponentRegistry, SHELL_BUILTINS};

/// Test-only helpers shared across every component's `#[cfg(test)]`
/// module. The "build a `BuilderNode` with these props" boilerplate
/// was repeated ~50 times before §39; collapsed here once.
#[cfg(test)]
pub(crate) mod testing {
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_builder::style::StyleProperties;
    use prism_builder::ui_lower::LowerCtx;
    use prism_core::foundation::spatial::Transform2D;
    use prism_ui_runtime::layout::Node as UiNode;
    use serde_json::Value;

    /// Synthesise a `BuilderNode` for a single shell block under test.
    /// Children empty, default cascade, default transform.
    pub fn test_node(id: &str, component: &str, props: Value) -> BuilderNode {
        test_node_with_children(id, component, props, vec![])
    }

    /// Variant with explicit children — used by the handful of blocks
    /// (`shell.dock-panel`, `shell.app-window`) whose tests need to
    /// drive nested AST shapes through the resolver.
    pub fn test_node_with_children(
        id: &str,
        component: &str,
        props: Value,
        children: Vec<BuilderNode>,
    ) -> BuilderNode {
        BuilderNode {
            id: id.into(),
            component: component.into(),
            props,
            children,
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        }
    }

    /// Run a block's `lower_ui`-style free function against a default
    /// cascade with no resolver. Most shell-block tests want exactly
    /// this shape.
    pub fn lower_with<F>(node: &BuilderNode, f: F) -> UiNode
    where
        F: FnOnce(&LowerCtx<'_>, &BuilderNode, &StyleProperties) -> UiNode,
    {
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        f(&ctx, node, &cascade)
    }
}
