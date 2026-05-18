//! `ShellPropBindings` — the host-side mirror of `register_shell_builtins`.
//!
//! For every shell block id registered in
//! [`crate::components::register_shell_builtins`], there is exactly one
//! row in [`ShellPropBindings::with_builtins`] that knows how to derive
//! its prop bag from the live host state. The two tables together are
//! the entire "what blocks exist and what data drives them" contract.
//!
//! See `docs/dev/clay-migration-plan.md` §17 for the rationale.

use std::collections::HashMap;

use prism_builder::ui_lower::BlockInvalidator;
use prism_builder::ComponentRegistry;
use prism_ui_runtime::layout::Node as UiNode;
use serde_json::Value;

use crate::AppState;

/// Borrow-pack of every typed datum a binding closure might read.
/// Built once per frame from `ShellInner` and threaded into every
/// binding — closures destructure exactly the fields they need and
/// ignore the rest.
///
/// `registry` is the live `ComponentRegistry` for binding closures
/// that need to lower a host-side tree (currently
/// `shell.builder-canvas` rendering `state.canvas.document`). The
/// pure slot-accessor bindings ignore the field entirely.
pub struct PropCtx<'a> {
    pub state: &'a AppState,
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub canvas_zoom: f32,
    pub registry: Option<&'a ComponentRegistry>,
    /// **Phase 3b** of `docs/dev/dioxus-inspiration.md`: per-block
    /// reactive invalidator from `ShellInner::render_scope`. Passed
    /// down so the canvas binding can plumb it into the builder's
    /// document lowering, giving every `BuilderDocument` block a
    /// per-NodeId reactive context. `None` in headless / test paths
    /// where no reactive tracking is wanted.
    pub block_invalidator: Option<&'a BlockInvalidator>,
    /// **Wave 1** of `docs/dev/composable-builder-plan.md`: open
    /// registry of `ModifierBehaviour` impls. The canvas binding
    /// plumbs this into the builder's `LowerCtx::with_modifier_registry`
    /// so attached `node.modifiers` fold over each block's lowered
    /// output. `None` on headless / pure-slot-accessor paths.
    pub modifier_registry: Option<&'a prism_builder::ModifierRegistry>,
    /// DSL self-bootstrap Loop 2: frozen `DockCatalog` snapshot built
    /// during `Shell::new` from built-ins + every loaded app's
    /// `panels.add`. Bindings that emit dock metadata
    /// (`shell.dock-workspace`) read labels + content tags from here
    /// so the lower fns stay catalog-free. `None` in headless / test
    /// paths that don't construct a registrar.
    pub dock_catalog: Option<&'a prism_dock::DockCatalog>,
}

/// What a single binding emits for one frame:
/// - `props` populates the matching `<shell.foo>` element's attributes
///   (most blocks read JSON arrays from here),
/// - `children` fills the `host_children` slot when the block is a
///   composition (§14). Leaves return `vec![]`.
#[derive(Default)]
pub struct PropEmission {
    pub props: Value,
    pub children: Vec<UiNode>,
}

impl PropEmission {
    pub fn from_props(props: Value) -> Self {
        Self {
            props,
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<UiNode>) -> Self {
        self.children = children;
        self
    }
}

pub type PropBinding = Box<dyn Fn(&PropCtx) -> PropEmission + Send + Sync>;

/// Registration table: one entry per shell block id. Mirrors
/// `register_shell_builtins`'s shape — adding a new block is one row
/// in each table and nothing else.
#[derive(Default)]
pub struct ShellPropBindings {
    table: HashMap<&'static str, PropBinding>,
}

impl ShellPropBindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, id: &'static str, binding: PropBinding) {
        self.table.insert(id, binding);
    }

    pub fn get(&self, id: &str) -> Option<&PropBinding> {
        self.table.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.table.keys().copied()
    }

    /// Walk every registered binding once and collect emissions keyed
    /// by block id. Used by [`crate::render::render_tree`] to fold
    /// emissions into the parsed `app.prism-ui` skeleton.
    pub fn snapshot(&self, ctx: &PropCtx) -> HashMap<&'static str, PropEmission> {
        self.table
            .iter()
            .map(|(id, binding)| (*id, binding(ctx)))
            .collect()
    }

    /// Populate every binding for the 47 registered shell blocks. Each
    /// row is a one-line forwarder to a function in
    /// [`crate::panel_props`] (or an inline closure for trivial cases).
    pub fn with_builtins() -> Self {
        let mut bindings = Self::new();
        register_builtin_bindings(&mut bindings);
        bindings
    }
}

/// Slot accessor: pure function from `&AppState` to JSON props.
///
/// Coerced from a non-capturing closure literal in [`SLOT_BINDINGS`].
/// Keeping the function-pointer shape (rather than a `Box<dyn Fn>`)
/// means the table is a `'static` const — no allocation, no per-row
/// ceremony, and the compiler can inline through the indirection at
/// each call site.
type SlotAccessor = fn(&AppState) -> Value;

/// Single declarative table — one row per shell block id whose props
/// derive from a slot method on [`AppState`]. Adding a binding is one
/// new row plus one method on the owning slot; nothing else moves.
///
/// JSON construction lives on the slot method (§19 discipline), so
/// every closure here is a one-liner forwarder. The closure body MUST
/// stay non-capturing so it coerces to `SlotAccessor`.
/// IDE-mode Phase 4: the static list of built-in shell binding tags
/// (`shell.foo` ids) the [`DevToolsSlot`](crate::state::DevToolsSlot)
/// Bindings lens iterates. Materialised by walking [`SLOT_BINDINGS`]
/// — same list, no second source of truth.
pub fn builtin_binding_tags() -> Vec<&'static str> {
    SLOT_BINDINGS.iter().map(|(id, _)| *id).collect()
}

const SLOT_BINDINGS: &[(&str, SlotAccessor)] = &[
    // Chrome slot — app frame, menu bar, status bar.
    ("shell.app-window", |s| {
        s.chrome.app_window_props(&s.workspace)
    }),
    ("shell.menu-bar-row", |s| {
        s.chrome.menu_bar_row_props(&s.workspace)
    }),
    ("shell.status-bar", |s| {
        s.chrome.status_bar_props(&s.workspace, &s.canvas)
    }),
    // Workspace slot — workflow tabs + recursive dock tree. Adding a
    // panel is one row in `prism_dock::catalog::register_builtins`,
    // never a binding edit.
    ("shell.workflow-page-bar", |s| {
        s.workspace.workflow_page_bar_props()
    }),
    // `shell.dock-workspace` moved to the closure-style registration
    // below — it needs the live `DockCatalog` from `PropCtx` to emit
    // labels + tags sidecar maps (DSL self-bootstrap Loop 2 / 4).
    // Overlay slot — toasts, command palette, help tooltip. Floating
    // chrome that paints over the app-window via the skeleton's
    // overlay siblings.
    ("shell.toast-stack", |s| s.overlay.toast_stack_props()),
    ("shell.command-palette", |s| {
        s.overlay.command_palette_props()
    }),
    // Find-in-document overlay. Reads from `state.search`, which the
    // `SearchService` mutates through the shared text-input dispatch.
    ("shell.search-overlay", |s| s.search.search_overlay_props()),
    // IDE-mode Phase 2 — "Go to Symbol" palette. Reads from
    // `state.index`, mutated through the shared text-input dispatch.
    ("shell.symbol-palette", |s| s.index.symbol_palette_props()),
    // IDE-mode Phase 3 — the Luau "Problems" panel.
    ("shell.diagnostics-panel", |s| {
        s.diagnostics.diagnostics_panel_props()
    }),
    // IDE-mode Phase 4 / cross-cutting §4.3 — Inspector / DevTools.
    // Walks the builder document for the Document lens; the other
    // three lenses read pre-populated buffers off `state.devtools`.
    ("shell.devtools", |s| {
        s.devtools.devtools_props(&s.canvas.document)
    }),
    ("shell.help-tooltip", |s| s.overlay.help_tooltip_props()),
    // Builder slot — inspector / properties / signals / schema. All
    // four read from the same `selection`-driven model, so cross-panel
    // consistency is automatic: the slot owns the resolution path
    // once and every binding pulls from it.
    ("shell.inspector-tree", |s| s.builder.inspector_tree_props()),
    ("shell.properties-panel", |s| {
        s.builder
            .properties_panel_props_with(s.field_focus.as_ref())
    }),
    ("shell.signals-panel", |s| s.builder.signals_panel_props()),
    ("shell.schema-designer", |s| {
        s.builder.schema_designer_props()
    }),
    // Navigation slot — page list and graph. Two lenses on the same
    // page array; the graph adds positions + edges on top.
    ("shell.nav-page-list", |s| {
        s.navigation.nav_page_list_props()
    }),
    ("shell.nav-graph", |s| s.navigation.nav_graph_props()),
    // Catalog slot — launchpad apps, explorer files, component palette.
    // Three disjoint shapes, no shared helpers (rule-of-three not met
    // — the underlying data types are genuinely different).
    ("shell.launchpad", |s| s.catalog.launchpad_props()),
    ("shell.explorer", |s| s.catalog.explorer_props()),
    ("shell.component-palette", |s| {
        s.catalog.component_palette_props()
    }),
    // Docs slot — view + sidebar share the same `DocsTopic` shape via
    // the slot's `topic_props` helper; only `mode` differs.
    ("shell.docs-view", |s| s.docs.docs_view_props()),
    ("shell.docs-sidebar", |s| s.docs.docs_sidebar_props()),
    // Menu slot — dropdown + context-menu share `items_json`. Two
    // consumers, identical keys; the slot owns the helper.
    ("shell.menu-dropdown", |s| s.menus.menu_dropdown_props()),
    ("shell.context-menu", |s| s.menus.context_menu_props()),
    // Canvas slot — code editor, canvas surface, three gizmos, resize
    // handles, component-picker popup. All seven read from the same
    // selection-driven model on `CanvasSlot`. The three gizmo
    // emissions share `gizmo_props(kind)` — drift in the gizmo shape
    // edits one site, not three.
    ("shell.code-editor", |s| s.canvas.code_editor_props()),
    ("shell.builder-canvas", |s| s.canvas.builder_canvas_props()),
    ("shell.builder-toolbar", |s| {
        s.canvas.builder_toolbar_props()
    }),
    ("shell.gizmo-move", |s| s.canvas.gizmo_move_props()),
    ("shell.gizmo-rotate", |s| s.canvas.gizmo_rotate_props()),
    ("shell.gizmo-scale", |s| s.canvas.gizmo_scale_props()),
    ("shell.resize-handle", |s| s.canvas.resize_handle_props()),
    ("shell.component-picker", |s| {
        s.canvas.component_picker_props()
    }),
];

fn register_builtin_bindings(reg: &mut ShellPropBindings) {
    use crate::components::prism_ui_loader::SHELL_PRISM_UI_COMPONENTS;
    use crate::components::SHELL_BUILTINS;

    // Live bindings from the declarative `SLOT_BINDINGS` table.
    for (id, accessor) in SLOT_BINDINGS {
        let accessor = *accessor;
        reg.register(
            id,
            Box::new(move |ctx| PropEmission::from_props(accessor(ctx.state))),
        );
    }

    // Special-case override: `shell.builder-canvas` is the one binding
    // whose emission carries children alongside props. The host's
    // active `BuilderDocument` lowers through the live registry, and
    // the result threads into the canvas via the §43 B2
    // `host_children_by_tag` injection seam. The slot's
    // `builder_canvas_props` is still the single source of metadata —
    // the children-emission path layers on top, it doesn't replace it.
    reg.register(
        "shell.builder-canvas",
        Box::new(|ctx| {
            let mut props = ctx.state.canvas.builder_canvas_props();
            // Wave 3.2 polish: thread the in-flight palette-drag
            // state into the canvas's prop bag so the overlay layer
            // paints a ghost rect at the cursor. The drag lives on
            // `CatalogSlot`, the canvas reads only `CanvasSlot` —
            // one binding row merges them so the canvas block stays
            // single-source.
            props["palette-drag"] = crate::state::CanvasSlot::palette_drag_overlay(
                ctx.state.catalog.palette_drag.as_ref(),
            );
            // Wave 1: thread the shell's modifier registry into the
            // builder's `LowerCtx::with_modifier_registry` so attached
            // `node.modifiers` fold over each block's output during
            // the canvas's document walk.
            let children = ctx.state.canvas.lower_document_to_ui_full(
                ctx.registry,
                ctx.block_invalidator.cloned(),
                ctx.modifier_registry,
            );
            PropEmission::from_props(props).with_children(children)
        }),
    );

    // Wave 1.6 — `shell.modifier-picker` binding: needs the live
    // modifier registry to project options, so it lives on the
    // closure-style path (not `SLOT_BINDINGS`).
    reg.register(
        "shell.modifier-picker",
        Box::new(|ctx| {
            PropEmission::from_props(
                ctx.state
                    .overlay
                    .modifier_picker_props(ctx.modifier_registry),
            )
        }),
    );

    // Wave 4.3 — `shell.connection-picker` binding. The overlay's
    // four props mirror its schema (open + the three form fields).
    reg.register(
        "shell.connection-picker",
        Box::new(|ctx| PropEmission::from_props(ctx.state.overlay.connection_picker_props())),
    );

    // Wave 2.4 — `shell.color-picker` binding. Preset list is seeded
    // from `ColorPicker::PRESETS`; `(target-id, key, value)` reflect
    // the swatch that opened the overlay.
    reg.register(
        "shell.color-picker",
        Box::new(|ctx| PropEmission::from_props(ctx.state.overlay.color_picker_props())),
    );

    // Wave 2.3 — `shell.select-dropdown` binding. Options come from
    // the `data-options` attr the field-editor row carried; the
    // dropdown renders one selectable row per option and writes the
    // chosen value through `commit_select_dropdown_value`.
    reg.register(
        "shell.select-dropdown",
        Box::new(|ctx| PropEmission::from_props(ctx.state.overlay.select_dropdown_props())),
    );

    // DSL self-bootstrap Loop 2 / 4 — `shell.dock-workspace` emits the
    // dock tree plus `labels` and `tags` sidecar maps resolved from
    // the live `DockCatalog`. Falls back to the catalog-less variant
    // in headless tests where no registrar was constructed.
    reg.register(
        "shell.dock-workspace",
        Box::new(|ctx| {
            let props = match ctx.dock_catalog {
                Some(cat) => ctx.state.workspace.dock_workspace_props_with_catalog(cat),
                None => ctx.state.workspace.dock_workspace_props(),
            };
            PropEmission::from_props(props)
        }),
    );

    // Stub bindings — derived, not maintained. Every id in
    // `SHELL_BUILTINS` that doesn't appear in `SLOT_BINDINGS` (or in
    // the explicit closure-style registrations above) is a *per-row*
    // block (`shell.dock-tab`, `shell.menu-item`,
    // `shell.signal-connection-row`, …) whose data flows down inside
    // a parent JSON array. Authoring such a block needs no second
    // edit here: registering it in `SHELL_BUILTINS` automatically
    // gives it an empty stub binding, and the row's parent slot owns
    // the actual JSON shape. Promoting a stub to live data is one
    // row added to `SLOT_BINDINGS` (or one closure-style block above)
    // plus one method on the owning slot; the auto-stub vanishes
    // because the id is now claimed.
    for spec in SHELL_BUILTINS {
        if SLOT_BINDINGS.iter().any(|(id, _)| *id == spec.id) || reg.get(spec.id).is_some() {
            continue;
        }
        reg.register(
            spec.id,
            Box::new(|_| PropEmission::from_props(Value::Object(Default::default()))),
        );
    }

    // Wave 11.2 — every `.prism-ui`-authored shell component lands in
    // the registry alongside the native `SHELL_BUILTINS` rows, so the
    // auto-stub binding pass must walk the DSL table too. Without
    // this, a migrated component renders with no prop snapshot at all
    // (binding lookup returns `None`, the per-frame props closure
    // never fires). Same rule as native: a row in `SLOT_BINDINGS`
    // wins over the stub.
    for spec in SHELL_PRISM_UI_COMPONENTS {
        if SLOT_BINDINGS.iter().any(|(id, _)| *id == spec.id) || reg.get(spec.id).is_some() {
            continue;
        }
        if SHELL_BUILTINS.iter().any(|s| s.id == spec.id) {
            // Defence-in-depth — the loader's
            // `prism_ui_specs_register_disjoint_from_native_builtins`
            // test pins this can't happen, but if a future change
            // bypasses that pin we'd panic on double-registration
            // instead of double-writing. Skip silently here; the
            // dedicated test surfaces the literal collision.
            continue;
        }
        reg.register(
            spec.id,
            Box::new(|_| PropEmission::from_props(Value::Object(Default::default()))),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keystone parity check — every id in `register_shell_builtins`
    /// has a matching entry in `ShellPropBindings::with_builtins`.
    /// Forgetting to wire up a new block is a test failure here, not
    /// a silent blank panel at runtime.
    fn ctx_for(state: &AppState) -> PropCtx<'_> {
        PropCtx {
            state,
            viewport_w: 1280.0,
            viewport_h: 800.0,
            canvas_zoom: 1.0,
            registry: None,
            block_invalidator: None,
            modifier_registry: None,
            dock_catalog: None,
        }
    }

    #[test]
    fn docs_topic_shape_propagates_to_view_and_sidebar_bindings() {
        // §21 cross-binding parity: changing `docs.topic.title` shows
        // up in *both* `shell.docs-view` and `shell.docs-sidebar`
        // emissions through the same `topic_props` helper. This is
        // the load-bearing duplication check for the docs slot.
        let mut state = AppState::default();
        state.docs.topic.title = "Hello".into();
        state.docs.topic.summary = "Topic".into();
        let bindings = ShellPropBindings::with_builtins();
        let ctx = ctx_for(&state);
        let snap = bindings.snapshot(&ctx);
        assert_eq!(snap["shell.docs-view"].props["title"], "Hello");
        assert_eq!(snap["shell.docs-sidebar"].props["title"], "Hello");
        assert_eq!(snap["shell.docs-view"].props["summary"], "Topic");
        assert_eq!(snap["shell.docs-sidebar"].props["summary"], "Topic");
    }

    #[test]
    fn menu_items_shape_matches_across_dropdown_and_context_bindings() {
        // §21 cross-binding parity: `items_json` is the single emitter
        // for both menu shells. Adding a key to one site is impossible
        // — they share the helper.
        let mut state = AppState::default();
        state.menus.dropdown.push(crate::state::MenuItem {
            label: "A".into(),
            shortcut: Some("Ctrl+A".into()),
            command: Some("a".into()),
            separator: false,
            enabled: true,
        });
        state.menus.context.push(crate::state::MenuItem {
            label: "B".into(),
            shortcut: None,
            command: Some("b".into()),
            separator: false,
            enabled: true,
        });
        let bindings = ShellPropBindings::with_builtins();
        let ctx = ctx_for(&state);
        let snap = bindings.snapshot(&ctx);
        let drop_item = &snap["shell.menu-dropdown"].props["items"][0];
        let ctx_item = &snap["shell.context-menu"].props["items"][0];
        // Common keys present on both
        for key in ["label", "separator", "enabled", "command"] {
            assert!(drop_item.get(key).is_some(), "dropdown missing {key}");
            assert!(ctx_item.get(key).is_some(), "context missing {key}");
        }
    }

    #[test]
    fn selection_center_drives_gizmo_and_handle_bindings() {
        // §22 cross-binding parity: changing the selected node's
        // transform shows up in *both* `shell.gizmo-move` and
        // `shell.resize-handle` emissions through the same
        // `selection_center()` helper. The load-bearing duplication
        // check for the canvas slot.
        use prism_builder::{BuilderDocument, Node};
        use prism_core::foundation::spatial::Transform2D;
        let mut state = AppState::default();
        state.canvas.document = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                transform: Transform2D {
                    position: [320.0, 240.0],
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        state.canvas.selection = Some("root".into());
        state.canvas.tool = crate::state::ToolMode::Move;
        let bindings = ShellPropBindings::with_builtins();
        let snap = bindings.snapshot(&ctx_for(&state));
        let g = &snap["shell.gizmo-move"].props;
        let handles = snap["shell.resize-handle"].props["handles"]
            .as_array()
            .unwrap();
        let top = handles.iter().find(|h| h["id"] == "t").unwrap();
        assert_eq!(g["center-x"], 320.0);
        assert_eq!(g["center-y"], 240.0);
        assert_eq!(top["x"], 320.0, "handle mid-x is gizmo center-x");
        assert_eq!(g["visible"], true);
    }

    #[test]
    fn bindings_cover_every_registered_shell_block() {
        // Load-bearing invariant: every shell-block id (native
        // `SHELL_BUILTINS` plus DSL-authored
        // `SHELL_PRISM_UI_COMPONENTS`) has a matching binding (live
        // or derived stub). The stub-pass loop in
        // `register_builtin_bindings` walks both tables; this
        // assertion pins the parity so a future migration that adds a
        // `.prism-ui` row without seeding a binding surfaces as a
        // test failure, not a blank panel at runtime.
        use crate::components::prism_ui_loader::SHELL_PRISM_UI_COMPONENTS;
        use crate::components::SHELL_BUILTINS;
        let bindings = ShellPropBindings::with_builtins();
        let binding_ids: std::collections::HashSet<&str> = bindings.ids().collect();
        let mut block_ids: std::collections::HashSet<&str> =
            SHELL_BUILTINS.iter().map(|s| s.id).collect();
        for spec in SHELL_PRISM_UI_COMPONENTS {
            block_ids.insert(spec.id);
        }
        assert_eq!(
            binding_ids, block_ids,
            "every registered shell block (native or .prism-ui) must have a binding (and vice versa)"
        );
    }

    #[test]
    fn canvas_emits_lowered_document_as_host_children() {
        // §43 E2: the named verification test for Phase B. The
        // `shell.builder-canvas` binding is the one row that emits
        // `PropEmission::children` — the lowered `BuilderDocument`
        // threads into the canvas tag through the §43 B2
        // `host_children_by_tag` seam. Empty without a registry
        // (headless path); non-empty with one (live render path).
        use prism_builder::ComponentRegistry;
        use prism_ui_runtime::layout::Node as UiNode;

        let state = crate::seed::initial_state();

        // Headless path: no registry → no children, but props still
        // emit so the canvas frame paints.
        let ctx_headless = ctx_for(&state);
        let bindings = ShellPropBindings::with_builtins();
        let snap_h = bindings.snapshot(&ctx_headless);
        let canvas_h = snap_h
            .get("shell.builder-canvas")
            .expect("canvas binding registered");
        assert!(
            canvas_h.children.is_empty(),
            "no registry → canvas emits no host children"
        );
        assert!(
            canvas_h.props.get("selection-id").is_some(),
            "props still emit"
        );

        // Live path: with a registry, the seed document lowers and
        // surfaces under the canvas binding's `children`.
        let mut reg = ComponentRegistry::new();
        prism_builder::starter::register_builtins(&mut reg).expect("builtins");
        let ctx_live = PropCtx {
            state: &state,
            viewport_w: 1280.0,
            viewport_h: 800.0,
            canvas_zoom: 1.0,
            registry: Some(&reg),
            block_invalidator: None,
            modifier_registry: None,
            dock_catalog: None,
        };
        let snap_l = bindings.snapshot(&ctx_live);
        let canvas_l = snap_l
            .get("shell.builder-canvas")
            .expect("canvas binding registered");
        assert!(
            !canvas_l.children.is_empty(),
            "registry + seeded document → canvas emits lowered children"
        );

        // The emitted child tree contains the demo nodes from the
        // seed (`demo-heading`, `demo-paragraph`, `demo-button`).
        fn collect_ids(node: &UiNode, out: &mut Vec<String>) {
            match node {
                UiNode::Container { id, children, .. } => {
                    out.push(id.clone());
                    for c in children {
                        collect_ids(c, out);
                    }
                }
                UiNode::Text { id, .. }
                | UiNode::TextInput { id, .. }
                | UiNode::Image { id, .. }
                | UiNode::Spacer { id, .. } => {
                    out.push(id.clone());
                }
            }
        }
        let mut ids = Vec::new();
        for n in &canvas_l.children {
            collect_ids(n, &mut ids);
        }
        // The lowered tree wraps node ids with descendant scopes;
        // require the prefix to appear on at least one child.
        let demo_seen = ids.iter().any(|s| s.contains("demo-heading"));
        assert!(
            demo_seen,
            "expected `demo-heading` somewhere in lowered children, got {ids:?}"
        );
    }

    #[test]
    fn slot_bindings_are_subset_of_shell_builtins() {
        // Adding a row to SLOT_BINDINGS for a non-existent shell block
        // would silently register a dead binding (and shadow the stub
        // derivation). Catch that at test time. A SLOT_BINDINGS row
        // can target either a native (`SHELL_BUILTINS`) or
        // `.prism-ui`-authored (`SHELL_PRISM_UI_COMPONENTS`) block.
        use crate::components::prism_ui_loader::SHELL_PRISM_UI_COMPONENTS;
        use crate::components::SHELL_BUILTINS;
        let mut block_ids: std::collections::HashSet<&str> =
            SHELL_BUILTINS.iter().map(|s| s.id).collect();
        for spec in SHELL_PRISM_UI_COMPONENTS {
            block_ids.insert(spec.id);
        }
        for (id, _) in SLOT_BINDINGS {
            assert!(
                block_ids.contains(id),
                "SLOT_BINDINGS row `{id}` has no matching block in SHELL_BUILTINS or SHELL_PRISM_UI_COMPONENTS"
            );
        }
    }
}
