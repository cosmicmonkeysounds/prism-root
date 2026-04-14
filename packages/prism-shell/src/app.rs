//! Root application state + top-level layout. Everything reloadable
//! lives behind one struct so §7's hot-reload story is one serde call.

use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use prism_core::shell_mode::{Permission, ShellMode, ShellModeContext};

use crate::panels::{identity::IdentityPanel, Panel};

#[cfg(feature = "clay")]
use clay_layout::{math::Dimensions, render_commands::RenderCommand, Clay};

/// The single reloadable root state. Extend by adding fields here;
/// do *not* stash runtime state in `lazy_static`s or `OnceCell`s —
/// that breaks the hot-reload loop.
pub struct AppState {
    pub tokens: DesignTokens,
    pub context: ShellModeContext,
    pub active_panel: ActivePanel,
}

#[derive(Debug, Clone, Copy)]
pub enum ActivePanel {
    Identity,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            tokens: DEFAULT_TOKENS,
            context: ShellModeContext {
                shell_mode: ShellMode::Build,
                permission: Permission::Dev,
            },
            active_panel: ActivePanel::Identity,
        }
    }
}

/// Declare the active panel into a Clay layout and return the
/// resulting render commands. Called by every backend (native
/// wgpu, the future WASM canvas) once per frame.
///
/// The returned `Vec` borrows from `clay` — Clay owns the backing
/// string storage that [`RenderCommand::Text`] variants point into,
/// so the caller must consume the commands before the next
/// `render_app` call.
#[cfg(feature = "clay")]
pub fn render_app<'clay>(
    state: &AppState,
    clay: &'clay mut Clay,
) -> Vec<RenderCommand<'clay, (), ()>> {
    let mut scope: clay_layout::ClayLayoutScope<'clay, 'clay, (), ()> = clay.begin();

    match state.active_panel {
        ActivePanel::Identity => IdentityPanel::new().declare(state, &mut scope),
    }

    scope.end().collect()
}

/// Install a placeholder text measurement callback on a fresh `Clay`.
///
/// Clay's C core panics on any layout containing text unless a measurer
/// is registered. Phase 0 has no glyph shaper wired up yet, so we cheat:
/// width ≈ `chars * font_size * 0.5`, height == `font_size`. The real
/// glyphon-backed measurer lands with the wgpu renderer in task #2.
#[cfg(feature = "clay")]
pub fn install_stub_text_measurer(clay: &mut Clay) {
    clay.set_measure_text_function(|text, config| {
        let font_size = config.font_size as f32;
        Dimensions::new(text.chars().count() as f32 * font_size * 0.5, font_size)
    });
}
