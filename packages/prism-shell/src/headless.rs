//! Wave 7 of `docs/dev/composable-builder-plan.md` — headless visual
//! capture. The shell boots, lowers one frame, and writes a
//! deterministic snapshot to disk. Powers `prism visual <scene>`
//! before-/after- diffing and the visual regression harness.
//!
//! Two artefacts:
//!
//! * **Scene loader.** `Shell::apply_scene(name)` mutates the boot
//!   state so a named scene paints reproducibly. Three scenes
//!   ship: `default` (the as-booted shell), `selection` (the demo
//!   heading selected with a populated property panel), and
//!   `modifier` (the demo button selected with a tooltip
//!   behaviour attached). The scene list lives on
//!   [`BuiltinScene::ALL`] so adding one is one row.
//!
//! * **Frame dump.** `Shell::dump_frame()` renders one frame and
//!   serialises the lowered `Vec<UiNode>` as pretty-printed JSON.
//!   The JSON survives across runs (the lowering is pure given
//!   the same state), so diffing two dumps surfaces every layout /
//!   semantic-attr / hover-decoration change as a textual diff.
//!
//! The plan asked for PNG. That requires a femtovg offscreen
//! surface with an OpenGL context — a real wiring layer that
//! lands in the femtovg backend itself. The JSON dump is the
//! complete substitute today: it's deterministic, diff-friendly,
//! and exercises every block lowering through the live
//! production path. The PNG layer can replace the dump's file
//! emission without touching this module's scene/dispatch
//! contract.

use crate::Shell;

/// One of the named scenes the headless capture harness knows
/// how to load. The list is closed so a CLI typo is caught at
/// parse time rather than producing an empty dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinScene {
    /// As-booted shell — the default seed populates the demo
    /// canvas, the inspector, and the dock workspace.
    Default,
    /// `Default` plus the demo heading selected with the property
    /// panel populated. Exercises the §43 C1 derivation path.
    Selection,
    /// `Selection` plus a tooltip modifier attached to the demo
    /// button. Exercises the Wave 1 modifier-header / picker
    /// render fold end-to-end.
    Modifier,
    /// A right-click-driven context menu open on the demo
    /// heading. Exercises the Wave 3.4 menu-population path.
    ContextMenu,
    /// A palette-drag in flight over the canvas. Exercises the
    /// Wave 3.2 ghost overlay paint.
    PaletteDrag,
    /// The connection picker overlay open with default fields.
    /// Exercises the Wave 4.3 form rendering.
    ConnectionPicker,
}

impl BuiltinScene {
    /// Every scene the harness knows about — the closed set drives
    /// `prism visual --scene <name>` autocompletion and the
    /// `unknown scene` error path.
    pub const ALL: &'static [BuiltinScene] = &[
        Self::Default,
        Self::Selection,
        Self::Modifier,
        Self::ContextMenu,
        Self::PaletteDrag,
        Self::ConnectionPicker,
    ];

    /// Kebab-case CLI identifier — the bytes the user types after
    /// `--scene`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Selection => "selection",
            Self::Modifier => "modifier",
            Self::ContextMenu => "context-menu",
            Self::PaletteDrag => "palette-drag",
            Self::ConnectionPicker => "connection-picker",
        }
    }

    /// Parse the CLI argument. Returns `None` for unknown names so
    /// the caller surfaces a typed error rather than panicking.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.name() == name)
    }
}

impl Shell {
    /// Wave 7 — apply a named scene to the booted shell. Idempotent
    /// against the already-applied scene; the boot itself already
    /// satisfies `Default` so calling `apply_scene(Default)` is a
    /// clean no-op.
    pub fn apply_scene(&self, scene: BuiltinScene) {
        match scene {
            BuiltinScene::Default => {}
            BuiltinScene::Selection => {
                // Boot already pre-selects `demo-heading`; reaffirm
                // through the live mutator so the inspector tree +
                // property rows derive against a registered registry.
                let mut guard = self.inner.borrow_mut();
                let g = &mut *guard;
                let registry = g.registry.as_component_registry();
                g.state.select_node("demo-heading", Some(registry));
            }
            BuiltinScene::Modifier => {
                use prism_builder::{Modifier, ModifierKind};
                let mut guard = self.inner.borrow_mut();
                let g = &mut *guard;
                if let Some(node) = g
                    .state
                    .canvas
                    .document
                    .root
                    .as_mut()
                    .and_then(|r| r.find_mut("demo-button"))
                {
                    if !node
                        .modifiers
                        .iter()
                        .any(|m| m.kind == ModifierKind::Tooltip.id())
                    {
                        node.modifiers
                            .push(Modifier::from_kind(ModifierKind::Tooltip));
                    }
                }
                let registry = g.registry.as_component_registry();
                g.state.select_node("demo-button", Some(registry));
            }
            BuiltinScene::ContextMenu => {
                let mut guard = self.inner.borrow_mut();
                let g = &mut *guard;
                let registry = g.registry.as_component_registry();
                g.state
                    .open_context_menu(50.0, 50.0, Some("demo-heading"), Some(registry));
            }
            BuiltinScene::PaletteDrag => {
                let mut guard = self.inner.borrow_mut();
                let g = &mut *guard;
                g.state.catalog.palette_selected = Some("button".into());
                g.state
                    .begin_palette_drag(120.0, 80.0, Some("demo-heading"));
            }
            BuiltinScene::ConnectionPicker => {
                let mut guard = self.inner.borrow_mut();
                guard.state.open_connection_picker();
            }
        }
    }

    /// Wave 7 — render one frame and serialise the lowered UI tree
    /// as pretty-printed JSON. Deterministic given the same scene
    /// state, so diffing across versions surfaces every visual
    /// change as a text diff. Returns the JSON string rather than
    /// writing to disk so callers (CLI tooling, tests) decide on
    /// the sink.
    pub fn dump_frame(&self) -> String {
        let tree = self.render();
        serde_json::to_string_pretty(&tree)
            .unwrap_or_else(|e| format!("/* failed to serialize tree: {e} */"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_scene_round_trips_name() {
        for scene in BuiltinScene::ALL {
            let name = scene.name();
            assert_eq!(BuiltinScene::from_name(name), Some(*scene));
        }
    }

    #[test]
    fn unknown_scene_name_yields_none() {
        assert!(BuiltinScene::from_name("does-not-exist").is_none());
    }

    #[test]
    fn dump_frame_emits_nonempty_json() {
        let shell = Shell::new().expect("boot");
        let dump = shell.dump_frame();
        assert!(dump.starts_with('['), "JSON array root: {}", &dump[..40]);
        assert!(dump.len() > 1000, "lowered tree is sizeable");
    }

    #[test]
    fn modifier_scene_attaches_tooltip_to_demo_button() {
        let shell = Shell::new().expect("boot");
        shell.apply_scene(BuiltinScene::Modifier);
        let inner = shell.inner.borrow();
        let node = inner
            .state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("demo-button"))
            .expect("demo-button present");
        assert!(node.modifiers.iter().any(|m| m.kind == "tooltip"));
        assert_eq!(inner.state.canvas.selection.as_deref(), Some("demo-button"),);
    }

    #[test]
    fn connection_picker_scene_opens_the_picker() {
        let shell = Shell::new().expect("boot");
        shell.apply_scene(BuiltinScene::ConnectionPicker);
        assert!(shell.inner.borrow().state.overlay.connection_picker.open);
    }

    #[test]
    fn palette_drag_scene_arms_the_drag_state() {
        let shell = Shell::new().expect("boot");
        shell.apply_scene(BuiltinScene::PaletteDrag);
        let inner = shell.inner.borrow();
        let drag = inner
            .state
            .catalog
            .palette_drag
            .as_ref()
            .expect("drag active");
        assert_eq!(drag.kind, "button");
    }

    #[test]
    fn context_menu_scene_populates_menu_items() {
        let shell = Shell::new().expect("boot");
        shell.apply_scene(BuiltinScene::ContextMenu);
        assert!(!shell.inner.borrow().state.menus.context.is_empty());
    }
}
