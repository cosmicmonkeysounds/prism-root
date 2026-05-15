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
    /// The in-shell code editor open on a sample multi-line Luau
    /// script with the caret + a selection live. Exercises the
    /// `shell.code-editor` block and the runtime's caret-at-byte
    /// + selection rendering end-to-end.
    CodeEditor,
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
        Self::CodeEditor,
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
            Self::CodeEditor => "code-editor",
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
            BuiltinScene::CodeEditor => {
                let mut guard = self.inner.borrow_mut();
                let g = &mut *guard;
                // Switch to the Code workflow page (Explorer | CodeEditor)
                // so the editor panel is actually visible. Routing also
                // works via `navigate_to_panel("code-editor")`.
                g.state.workspace.workspace.navigate_to_panel("code-editor");
                let source = "local function greet(name)\n  print(\"hello \" .. name)\n  return name\nend\n\ngreet(\"prism\")\n";
                g.state.canvas.code_buffer.load(source, "luau");
                // Selection covers the literal "hello" on line 2.
                let sel_start = source.find("hello").unwrap_or(0);
                let sel_end = sel_start + 5;
                g.state
                    .canvas
                    .code_buffer
                    .editor
                    .place_caret_at(sel_start, false);
                g.state
                    .canvas
                    .code_buffer
                    .editor
                    .place_caret_at(sel_end, true);
                g.state.code_editor_focused = true;
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

    /// Wave 7.2 — render one frame headlessly and encode it as a
    /// PNG byte buffer. Walks the lowered UI tree through
    /// `prism_ui_runtime::layout::compute` (the same Taffy pass the
    /// femtovg backend uses) to recover laid-out
    /// [`prism_ui_runtime::command::RenderCommand`]s, then
    /// rasterises them through a CPU-side painter into an RGBA
    /// buffer and PNG-encodes via the workspace `image` crate.
    ///
    /// The painter is intentionally simple: solid rectangles,
    /// borders, scissor clipping, text-bounding-box placeholders
    /// (one accented strip at the baseline). It is not pixel-equal
    /// to the femtovg backend's output (no glyph rasterisation, no
    /// image decoding, no antialiased corner radii) — but the
    /// **layout** is identical, so before/after diffs catch every
    /// box-model regression the visual harness exists to surface.
    /// Path forward to pixel-equal: a real `femtovg` offscreen
    /// surface land replaces this body without changing the seam.
    pub fn dump_png(&self, width: u32, height: u32) -> Result<Vec<u8>, String> {
        use prism_ui_runtime::layout::{
            self, ContainerProps, Direction, Node as UiNode, Sizing, Viewport,
        };
        let children = self.render();
        let root = UiNode::Container {
            id: String::new(),
            props: ContainerProps {
                direction: Direction::Column,
                width: Sizing::Grow,
                height: Sizing::Grow,
                ..Default::default()
            },
            children,
        };
        let viewport = Viewport {
            width: width as f32,
            height: height as f32,
        };
        let commands = layout::compute(&root, viewport);
        let buffer = crate::png_paint::rasterize(&commands, width, height);
        let mut out: Vec<u8> = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut out);
        image::ImageEncoder::write_image(
            encoder,
            &buffer,
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("png encode failed: {e}"))?;
        Ok(out)
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

    /// Wave 7.2 — `dump_png` emits a non-empty PNG byte buffer
    /// starting with the standard 8-byte PNG magic. Renders against
    /// a small viewport to keep the test fast.
    #[test]
    fn dump_png_emits_png_magic_bytes() {
        let shell = Shell::new().expect("boot");
        let bytes = shell.dump_png(64, 48).expect("png encode");
        assert!(bytes.len() > 50, "encoded PNG should be non-trivial");
        // PNG magic: 89 50 4E 47 0D 0A 1A 0A
        assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n");
    }

    /// Wave 7.2 — running `dump_png` twice on the same scene yields
    /// byte-identical output. Pins the determinism contract the
    /// visual harness diffs against.
    #[test]
    fn dump_png_is_deterministic_across_runs() {
        let shell = Shell::new().expect("boot");
        let first = shell.dump_png(64, 48).expect("png encode");
        let second = shell.dump_png(64, 48).expect("png encode");
        assert_eq!(first, second);
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
