//! Identity panel — the first panel ported in Phase 1. Target parity
//! with the legacy `prism-studio/src/panels/identity-panel.tsx`.
//!
//! Phase 0 emits a sidebar + three-button column so the Slint spike
//! has something recognizable on screen. Phase 1 expands this into
//! the real identity UX (DID, document, sign/verify, export/import).
//! Layout + rendering lives in `ui/app.slint`; this file only
//! supplies the data the Slint properties bind to.

use super::Panel;

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

const ACTIONS: &[&str] = &["Create Identity", "Load Vault", "Sign Document"];

impl Panel for IdentityPanel {
    fn title(&self) -> &'static str {
        "Identity"
    }

    fn hint(&self) -> &'static str {
        "Select an action from the sidebar."
    }

    fn actions(&self) -> &'static [&'static str] {
        ACTIONS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_panel_exposes_three_actions() {
        let panel = IdentityPanel::new();
        assert_eq!(panel.actions().len(), 3);
        assert_eq!(panel.title(), "Identity");
    }
}
