//! Shell-only components — toolbar / chrome / overlay primitives that
//! must NOT show up in the user-visible document component palette.
//!
//! Every primitive is declared as a [`prism_builder::BlockSpec`] const
//! and registered into a sibling [`ShellComponentRegistry`] via the
//! `SHELL_BUILTINS` table in `registry.rs`. The strategy and rationale
//! are documented in `docs/dev/clay-migration-plan.md` §12 + §33.

// `builder_canvas` migrated to `ui/components/builder-canvas.prui`
// 2026-05-13 (Wave 11.4 / Tier-3). The imperative `tag_canvas_subtree`
// walk (the only non-declarative piece) lives in the
// `prism.builder-host` primitive body in `prism-builder/src/primitives.rs`;
// the chrome (toolbar, grid overlay, selection ring + handles, palette
// ghost, gizmo dispatch) is now declarative.
// `chrome` retired 2026-05-13 (Wave 11.3) — the last consumer was
// `field_editor::build_number_body`, which migrated to DSL alongside
// the rest of the field editor. Pre-migration helpers
// (`drag_number_field_node`, `format_drag_value`,
// `DRAG_NUMBER_LABEL_COLOR`, `hidden_overlay`, `color_or_transparent`)
// either inlined into the binding (`format_drag_value` →
// `state::format_drag_value` for `drag-display-value`) or became
// unused as the DSL overlay-gate pattern (Wave 11.2 batch 5)
// replaced `hidden_overlay`.
// `code_editor` migrated to `ui/components/code-editor.prui`
// 2026-05-13 (Wave 11.4 / Tier-3). The body was already declarative;
// the binding (`state::code_editor_props`) now pre-derives `lines` /
// `cursor-line` / `cursor-column` / `status-label` so the DSL is one
// for-loop over the gutter + body columns. Keystroke handling lands
// on the `prism.text-buffer` primitive when a Luau-authored editor
// dispatches against it.
// `dock_panel` migrated to `ui/components/dock-panel.prui`
// 2026-05-13 (Wave 11.3). `prism_ui_loader::SHELL_PRISM_UI_COMPONENTS`
// owns the contract; the dock-workspace binding pre-resolves
// `content-tag` so the DSL block dispatches dynamic content via
// `<dispatch component="{content-tag}"/>` without a Rust router.
//
// `dock_workspace` migrated to `ui/components/dock-workspace.prui`
// + the new recursive `ui/components/dock-node.prui` helper
// 2026-05-13 (Wave 11.3). `state::enrich_dock_node` pre-resolves
// every `TabGroup` leaf's `panel-id` / `content-tag` / `tabs`
// shape so the DSL recursion is one expression-light file per
// tree variant.
// `field_editor` migrated to `ui/components/field-editor.prui`
// 2026-05-13 (Wave 11.3). The kind-dispatch table collapsed to an
// `if`/`else-if` chain; substrate fields are pre-computed by
// `state::property_row_from_spec`.
// `nav_graph` migrated to `ui/components/nav-graph.prui`
// 2026-05-13 (Wave 11.4 / Tier-3). Body was already declarative —
// the Tier-3 designation was nominal pending the
// `prism.canvas-paint` primitive landing. The edges + cards
// now iterate via `for=` and the renderer paints the strokes
// against `data-x1`/`data-y1`/`data-x2`/`data-y2` exactly as
// before.
pub mod prism_ui_loader;
pub mod registry;

pub use registry::{
    register_document_builtins, register_shell_builtins, ShellComponentRegistry, SHELL_BUILTINS,
};

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

    // Wave 11.4 — every Rust-authored shell block migrated to DSL,
    // so these helpers have no live consumers in-tree today. Kept
    // as test infrastructure for any future Rust-side block that
    // needs to round-trip `lower_ui` against a synthetic
    // `BuilderNode`. Tagged `dead_code`-allowed at the module
    // boundary so future consumers don't need any registration
    // ceremony to pick them back up.

    /// Synthesise a `BuilderNode` for a single shell block under test.
    /// Children empty, default cascade, default transform.
    #[allow(dead_code)]
    pub fn test_node(id: &str, component: &str, props: Value) -> BuilderNode {
        test_node_with_children(id, component, props, vec![])
    }

    /// Variant with explicit children — used historically by blocks
    /// (`shell.dock-panel`, `shell.app-window`) whose tests drove
    /// nested AST shapes through the resolver.
    #[allow(dead_code)]
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
    /// cascade with no resolver.
    #[allow(dead_code)]
    pub fn lower_with<F>(node: &BuilderNode, f: F) -> UiNode
    where
        F: FnOnce(&LowerCtx<'_>, &BuilderNode, &StyleProperties) -> UiNode,
    {
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        f(&ctx, node, &cascade)
    }
}
