//! Root application state + top-level layout. Everything reloadable
//! lives behind one struct so §7's hot-reload story is one serde call.

use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use prism_core::shell_mode::{Permission, ShellMode, ShellModeContext};

use crate::panels::{Panel, identity::IdentityPanel};

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

/// Top-level layout function. Called by the renderer once per frame.
///
/// Returns the number of draw commands the (stubbed) Clay pass would
/// emit — real hookup lands when `clay-layout` is wired in Phase 0
/// spike #1. Until then this is a pure dispatcher that every panel
/// can be tested against.
pub fn render_app(state: &AppState) -> usize {
    match state.active_panel {
        ActivePanel::Identity => IdentityPanel::new().render(state),
    }
}
