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

use prism_ui_runtime::layout::Node as UiNode;
use serde_json::Value;

use crate::AppState;

/// Borrow-pack of every typed datum a binding closure might read.
/// Built once per frame from `ShellInner` and threaded into every
/// binding — closures destructure exactly the fields they need and
/// ignore the rest.
pub struct PropCtx<'a> {
    pub state: &'a AppState,
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub canvas_zoom: f32,
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
const SLOT_BINDINGS: &[(&str, SlotAccessor)] = &[
    // Chrome slot — app frame, menu bar, status bar.
    ("shell.app-window", |s| {
        s.chrome.app_window_props(&s.workspace)
    }),
    ("shell.menu-bar-row", |s| {
        s.chrome.menu_bar_row_props(&s.workspace)
    }),
    ("shell.status-bar", |s| s.chrome.status_bar_props()),
    // Workspace slot — workflow tabs + recursive dock tree. Adding a
    // panel is one row in `prism_dock::PanelKind::ALL`, never a
    // binding edit.
    ("shell.workflow-page-bar", |s| {
        s.workspace.workflow_page_bar_props()
    }),
    ("shell.dock-workspace", |s| {
        s.workspace.dock_workspace_props()
    }),
    // Overlay slot — toasts, command palette, help tooltip. Floating
    // chrome that paints over the app-window via the skeleton's
    // overlay siblings.
    ("shell.toast-stack", |s| s.overlay.toast_stack_props()),
    ("shell.command-palette", |s| {
        s.overlay.command_palette_props()
    }),
    ("shell.help-tooltip", |s| s.overlay.help_tooltip_props()),
    // Builder slot — inspector / properties / signals / schema. All
    // four read from the same `selection`-driven model, so cross-panel
    // consistency is automatic: the slot owns the resolution path
    // once and every binding pulls from it.
    ("shell.inspector-tree", |s| s.builder.inspector_tree_props()),
    ("shell.properties-panel", |s| {
        s.builder.properties_panel_props()
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
    ("shell.gizmo-move", |s| s.canvas.gizmo_move_props()),
    ("shell.gizmo-rotate", |s| s.canvas.gizmo_rotate_props()),
    ("shell.gizmo-scale", |s| s.canvas.gizmo_scale_props()),
    ("shell.resize-handle", |s| s.canvas.resize_handle_props()),
    ("shell.component-picker", |s| {
        s.canvas.component_picker_props()
    }),
];

fn register_builtin_bindings(reg: &mut ShellPropBindings) {
    use crate::components::SHELL_BUILTINS;

    // Live bindings from the declarative `SLOT_BINDINGS` table.
    for (id, accessor) in SLOT_BINDINGS {
        let accessor = *accessor;
        reg.register(
            id,
            Box::new(move |ctx| PropEmission::from_props(accessor(ctx.state))),
        );
    }

    // Stub bindings — derived, not maintained. Every id in
    // `SHELL_BUILTINS` that doesn't appear in `SLOT_BINDINGS` is a
    // *per-row* block (`shell.dock-tab`, `shell.menu-item`,
    // `shell.signal-connection-row`, …) whose data flows down inside
    // a parent JSON array. Authoring such a block needs no second
    // edit here: registering it in `SHELL_BUILTINS` automatically
    // gives it an empty stub binding, and the row's parent slot owns
    // the actual JSON shape. Promoting a stub to live data is one
    // row added to `SLOT_BINDINGS` plus one method on the owning
    // slot; the auto-stub vanishes because the id is now claimed.
    for spec in SHELL_BUILTINS {
        if SLOT_BINDINGS.iter().any(|(id, _)| *id == spec.id) {
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
        // Load-bearing invariant: every id in `SHELL_BUILTINS` has a
        // matching binding (live or derived stub). Since
        // `register_builtin_bindings` walks `SHELL_BUILTINS` for its
        // stub pass, this is now a structural truth — but the assertion
        // pins it so that any future refactor that breaks the link is a
        // test failure, not a silent blank panel at runtime.
        use crate::components::SHELL_BUILTINS;
        let bindings = ShellPropBindings::with_builtins();
        let binding_ids: std::collections::HashSet<&str> = bindings.ids().collect();
        let builtin_ids: std::collections::HashSet<&str> =
            SHELL_BUILTINS.iter().map(|s| s.id).collect();
        assert_eq!(
            binding_ids, builtin_ids,
            "every SHELL_BUILTINS id must have a binding (and vice versa)"
        );
    }

    #[test]
    fn slot_bindings_are_subset_of_shell_builtins() {
        // Adding a row to SLOT_BINDINGS for a non-existent shell block
        // would silently register a dead binding (and shadow the stub
        // derivation). Catch that at test time.
        use crate::components::SHELL_BUILTINS;
        let builtin_ids: std::collections::HashSet<&str> =
            SHELL_BUILTINS.iter().map(|s| s.id).collect();
        for (id, _) in SLOT_BINDINGS {
            assert!(
                builtin_ids.contains(id),
                "SLOT_BINDINGS row `{id}` has no matching block in SHELL_BUILTINS"
            );
        }
    }
}
