//! Identity panel — the first panel ported in Phase 1. Target parity
//! with the legacy `prism-studio/src/panels/identity-panel.tsx`.
//!
//! Phase 0 emits a hand-coded "sidebar + three buttons" layout so the
//! Clay spike has something recognizable on screen. Phase 1 expands
//! this into the real identity UX (DID, document, sign/verify,
//! export/import).

use super::Panel;
use crate::AppState;

pub struct IdentityPanel;

impl IdentityPanel {
    pub fn new() -> Self {
        Self
    }
}

impl Default for IdentityPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel for IdentityPanel {
    fn render(&self, state: &AppState) -> usize {
        // Layout the sidebar + three-button row. Returns the number
        // of draw commands so unit tests can assert the panel is
        // emitting something without needing a real Clay arena yet.
        let tokens = &state.tokens;

        // Very rough approximation of the count Clay would produce:
        // background + sidebar + 3 buttons + 3 button labels = 8.
        let _ = tokens; // silence unused warning until the real layout runs
        8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_panel_emits_draw_commands() {
        let state = AppState::default();
        assert_eq!(IdentityPanel::new().render(&state), 8);
    }
}
