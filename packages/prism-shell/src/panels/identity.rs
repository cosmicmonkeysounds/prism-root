//! Identity panel — the Phase-0 spike. Supplies a title + hint + a
//! flat list of sidebar actions the Slint window wires into its
//! identity-panel surface. Parity target with
//! `prism-studio/src/panels/identity-panel.tsx`. Phase 4 will grow
//! this into the real identity UX (DID, document, sign/verify,
//! export/import).

use super::Panel;

pub struct IdentityPanel;

impl IdentityPanel {
    pub const ID: i32 = 0;
    pub fn new() -> Self {
        Self
    }
    pub fn actions(&self) -> &'static [&'static str] {
        &["Create Identity", "Load Vault", "Sign Document"]
    }
}

impl Default for IdentityPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel for IdentityPanel {
    fn id(&self) -> i32 {
        Self::ID
    }
    fn label(&self) -> &'static str {
        "Identity"
    }
    fn title(&self) -> &'static str {
        "Identity"
    }
    fn hint(&self) -> &'static str {
        "Select an action from the sidebar."
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
        assert_eq!(panel.id(), 0);
    }
}
