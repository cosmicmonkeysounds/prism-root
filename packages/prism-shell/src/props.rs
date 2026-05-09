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
    bind_slot!(reg, "shell.app-window", |s: &AppState| s
        .chrome
        .app_window_props());
    bind_slot!(reg, "shell.status-bar", |s: &AppState| s
        .chrome
        .status_bar_props());

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
        "shell.menu-bar-row",
        "shell.field-editor",
        "shell.workflow-page-button",
        "shell.workflow-page-bar",
        "shell.dock-divider",
        "shell.dock-tab",
        "shell.dock-tab-bar",
        "shell.dock-panel",
        "shell.toast-stack",
        "shell.inspector-tree",
        "shell.launchpad",
        "shell.command-palette",
        "shell.help-tooltip",
        "shell.menu-item",
        "shell.menu-dropdown",
        "shell.context-menu",
        "shell.docs-sidebar",
        "shell.docs-view",
        "shell.properties-panel",
        "shell.component-palette",
        "shell.explorer",
        "shell.signal-connection-row",
        "shell.signals-panel",
        "shell.schema-row",
        "shell.schema-designer",
        "shell.nav-page-row",
        "shell.nav-page-list",
        "shell.nav-graph",
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
