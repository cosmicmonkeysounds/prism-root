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

/// One-line registration sugar mirroring `register_shell_builtins`'s
/// `reg!` macro.
#[macro_export]
macro_rules! bind {
    ($reg:expr, $id:literal, $body:expr) => {
        $reg.register($id, Box::new($body))
    };
}

/// Slot-forwarder sugar: `bind_slot!(reg, "shell.foo", |s| s.chrome.foo_props())`.
/// Expands to a `bind!` whose closure pulls `&AppState` from `PropCtx`
/// and hands it to the user's slot accessor — no per-binding ceremony,
/// no JSON construction inside the closure body. See §19.
#[macro_export]
macro_rules! bind_slot {
    ($reg:expr, $id:literal, $accessor:expr) => {
        $crate::bind!($reg, $id, |ctx: &$crate::props::PropCtx| {
            $crate::props::PropEmission::from_props(($accessor)(ctx.state))
        })
    };
}

/// The bindings table proper. Every entry forwards to a function in
/// [`crate::panel_props`] — keep that module the single source of
/// truth for "typed substate → JSON shape."
fn register_builtin_bindings(reg: &mut ShellPropBindings) {
    use serde_json::json;

    // Real bindings — each forwards through one slot method on
    // `AppState`. Adding another is one row here + one method on the
    // owning slot. JSON construction lives on the slot, not in the
    // closure (§19 discipline).
    // Cross-slot composition takes the secondary slot as a `&` arg
    // — chrome owns the row, workspace tabs flow in by reference.
    // The closure stays declarative; the JSON shape lives on
    // exactly one method (§19).
    bind_slot!(reg, "shell.app-window", |s: &AppState| s
        .chrome
        .app_window_props(&s.workspace));
    bind_slot!(reg, "shell.menu-bar-row", |s: &AppState| s
        .chrome
        .menu_bar_row_props(&s.workspace));
    bind_slot!(reg, "shell.status-bar", |s: &AppState| s
        .chrome
        .status_bar_props());
    bind_slot!(reg, "shell.workflow-page-bar", |s: &AppState| s
        .workspace
        .workflow_page_bar_props());

    // Overlay slot — toasts, command palette, help tooltip. Floating
    // chrome that paints over the app-window via the skeleton's
    // overlay siblings.
    bind_slot!(reg, "shell.toast-stack", |s: &AppState| s
        .overlay
        .toast_stack_props());
    bind_slot!(reg, "shell.command-palette", |s: &AppState| s
        .overlay
        .command_palette_props());
    bind_slot!(reg, "shell.help-tooltip", |s: &AppState| s
        .overlay
        .help_tooltip_props());

    // Builder slot — inspector / properties / signals / schema. All
    // four read from the same `selection`-driven model, so cross-panel
    // consistency is automatic: the slot owns the resolution path
    // once and every binding pulls from it.
    bind_slot!(reg, "shell.inspector-tree", |s: &AppState| s
        .builder
        .inspector_tree_props());
    bind_slot!(reg, "shell.properties-panel", |s: &AppState| s
        .builder
        .properties_panel_props());
    bind_slot!(reg, "shell.signals-panel", |s: &AppState| s
        .builder
        .signals_panel_props());
    bind_slot!(reg, "shell.schema-designer", |s: &AppState| s
        .builder
        .schema_designer_props());

    // Navigation slot — page list and graph. Two lenses on the same
    // page array; the graph adds positions + edges on top.
    bind_slot!(reg, "shell.nav-page-list", |s: &AppState| s
        .navigation
        .nav_page_list_props());
    bind_slot!(reg, "shell.nav-graph", |s: &AppState| s
        .navigation
        .nav_graph_props());

    // Catalog slot — launchpad apps, explorer files, component palette.
    // Three disjoint shapes, no shared helpers (rule-of-three not met
    // — the underlying data types are genuinely different).
    bind_slot!(reg, "shell.launchpad", |s: &AppState| s
        .catalog
        .launchpad_props());
    bind_slot!(reg, "shell.explorer", |s: &AppState| s
        .catalog
        .explorer_props());
    bind_slot!(reg, "shell.component-palette", |s: &AppState| s
        .catalog
        .component_palette_props());

    // Docs slot — view + sidebar share the same `DocsTopic` shape via
    // the slot's `topic_props` helper; only `mode` differs.
    bind_slot!(reg, "shell.docs-view", |s: &AppState| s
        .docs
        .docs_view_props());
    bind_slot!(reg, "shell.docs-sidebar", |s: &AppState| s
        .docs
        .docs_sidebar_props());

    // Menu slot — dropdown + context-menu share `items_json`. Two
    // consumers, identical keys: rule-of-three justifies the helper
    // on landing.
    bind_slot!(reg, "shell.menu-dropdown", |s: &AppState| s
        .menus
        .menu_dropdown_props());
    bind_slot!(reg, "shell.context-menu", |s: &AppState| s
        .menus
        .context_menu_props());

    // Stub bindings — emit an empty prop bag until the owning slot
    // lands. The skeleton's author-supplied attrs still render, so
    // these blocks paint as a coherent (data-empty) chrome shell.
    // Promote a row out of this list when its slot ports in.
    for id in [
        "shell.icon-button",
        "shell.toolbar-separator",
        "shell.section-header",
        "shell.nav-button",
        "shell.toast",
        "shell.docs-content",
        "shell.app-card",
        "shell.drag-number-field",
        "shell.inspector-row",
        "shell.transform-editor",
        "shell.field-editor",
        "shell.workflow-page-button",
        "shell.dock-divider",
        "shell.dock-tab",
        "shell.dock-tab-bar",
        "shell.dock-panel",
        "shell.menu-item",
        "shell.signal-connection-row",
        "shell.schema-row",
        "shell.nav-page-row",
        "shell.code-editor",
        "shell.gizmo-move",
        "shell.gizmo-rotate",
        "shell.gizmo-scale",
        "shell.resize-handle",
        "shell.builder-canvas",
        "shell.component-picker",
    ] {
        reg.register(
            id,
            Box::new(move |_ctx| PropEmission::from_props(json!({}))),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::register_shell_builtins;

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
    fn bindings_cover_every_registered_shell_block() {
        let mut reg = crate::components::ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let bindings = ShellPropBindings::with_builtins();
        // ShellComponentRegistry doesn't expose ids() — once it does,
        // assert set equality. For now, assert count parity via the
        // hard-coded 47 below; updates require touching both tables.
        assert_eq!(bindings.ids().count(), 47);
        assert_eq!(reg.len(), 47);
    }
}
