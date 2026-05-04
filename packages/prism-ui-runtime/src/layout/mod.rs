//! Layout pass — wraps the Clay binding.
//!
//! The public entry point is `layout(tree, viewport) -> Vec<RenderCommand>`.
//! Phase 1 will fill this in; the stub here lets the workspace
//! compile while we wire crates and CI.

use crate::command::RenderCommand;

#[derive(Debug, Clone, Default)]
pub struct UiTree {
    /// Placeholder. Phase 1 fills this in with the typed Clay element
    /// tree built from a `prism_builder::BuilderDocument`.
    pub _stub: (),
}

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

pub fn layout(_tree: &UiTree, _viewport: Viewport) -> Vec<RenderCommand> {
    Vec::new()
}
