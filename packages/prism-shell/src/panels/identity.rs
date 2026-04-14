//! Identity panel — the first panel ported in Phase 1. Target parity
//! with the legacy `prism-studio/src/panels/identity-panel.tsx`.
//!
//! Phase 0 emits a sidebar + three-button column so the Clay spike
//! has something recognizable on screen. Phase 1 expands this into
//! the real identity UX (DID, document, sign/verify, export/import).

use super::Panel;
use crate::AppState;

#[cfg(feature = "clay")]
use clay_layout::{
    color::Color,
    fixed, grow,
    layout::{Alignment, LayoutAlignmentX, LayoutAlignmentY, LayoutDirection, Padding},
    text::{TextAlignment, TextConfig, TextElementConfigWrapMode},
    ClayLayoutScope, Declaration,
};

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

#[cfg(feature = "clay")]
const BACKGROUND: Color = Color::u_rgb(18, 19, 26);
#[cfg(feature = "clay")]
const SIDEBAR_BG: Color = Color::u_rgb(30, 32, 44);
#[cfg(feature = "clay")]
const BUTTON_BG: Color = Color::u_rgb(60, 120, 220);
#[cfg(feature = "clay")]
const TEXT_FG: Color = Color::u_rgb(240, 240, 245);

#[cfg(feature = "clay")]
const BUTTON_LABELS: [&str; 3] = ["Create Identity", "Load Vault", "Sign Document"];

impl Panel for IdentityPanel {
    #[cfg(feature = "clay")]
    fn declare<'clay>(&self, _state: &AppState, scope: &mut ClayLayoutScope<'clay, 'clay, (), ()>) {
        // Outer row: sidebar + main content. `with` borrows the
        // Declaration, built inline with the chained-builder API
        // taken from clay-layout's wgpu example.
        scope.with(
            Declaration::new()
                .layout()
                .width(grow!())
                .height(grow!())
                .direction(LayoutDirection::LeftToRight)
                .end()
                .background_color(BACKGROUND),
            |scope| {
                // Sidebar — fixed 240px column.
                scope.with(
                    Declaration::new()
                        .layout()
                        .width(fixed!(240.0))
                        .height(grow!())
                        .direction(LayoutDirection::TopToBottom)
                        .padding(Padding::all(16))
                        .child_gap(12)
                        .end()
                        .background_color(SIDEBAR_BG),
                    |scope| {
                        scope.text(
                            "Identity",
                            TextConfig::new()
                                .color(TEXT_FG)
                                .font_id(0)
                                .font_size(18)
                                .alignment(TextAlignment::Left)
                                .wrap_mode(TextElementConfigWrapMode::None)
                                .end(),
                        );

                        for label in BUTTON_LABELS {
                            scope.with(
                                Declaration::new()
                                    .layout()
                                    .width(grow!())
                                    .height(fixed!(36.0))
                                    .padding(Padding::horizontal(12))
                                    .child_alignment(Alignment::new(
                                        LayoutAlignmentX::Left,
                                        LayoutAlignmentY::Center,
                                    ))
                                    .end()
                                    .background_color(BUTTON_BG),
                                |scope| {
                                    scope.text(
                                        label,
                                        TextConfig::new()
                                            .color(TEXT_FG)
                                            .font_id(0)
                                            .font_size(14)
                                            .wrap_mode(TextElementConfigWrapMode::None)
                                            .end(),
                                    );
                                },
                            );
                        }
                    },
                );

                // Main content placeholder.
                scope.with(
                    Declaration::new()
                        .layout()
                        .width(grow!())
                        .height(grow!())
                        .padding(Padding::all(24))
                        .end(),
                    |scope| {
                        scope.text(
                            "Select an action from the sidebar.",
                            TextConfig::new()
                                .color(TEXT_FG)
                                .font_id(0)
                                .font_size(16)
                                .wrap_mode(TextElementConfigWrapMode::Words)
                                .end(),
                        );
                    },
                );
            },
        );
    }
}

#[cfg(all(test, feature = "clay"))]
mod tests {
    use super::*;
    use crate::app::{install_stub_text_measurer, render_app};
    use clay_layout::{render_commands::RenderCommandConfig, Clay};

    #[test]
    fn identity_panel_emits_rectangles_and_text() {
        let mut clay = Clay::new((800.0, 600.0).into());
        install_stub_text_measurer(&mut clay);
        let state = AppState::default();
        let commands = render_app(&state, &mut clay);

        let rect_count = commands
            .iter()
            .filter(|c| matches!(c.config, RenderCommandConfig::Rectangle(_)))
            .count();
        let text_count = commands
            .iter()
            .filter(|c| matches!(c.config, RenderCommandConfig::Text(_)))
            .count();

        assert!(
            rect_count >= 2,
            "expected at least 2 rectangles, got {rect_count}"
        );
        assert!(
            text_count >= 1,
            "expected at least 1 text command, got {text_count}"
        );
    }
}
