//! Panel modules. One file per panel, each exporting a struct that
//! implements [`Panel::render`]. The legacy TS tree had ~40 of these
//! — we add them back one at a time as the Phase-1/2 port advances.

pub mod identity;

use crate::AppState;

pub trait Panel {
    /// Emit the panel's Clay nodes against `state`. Stub returns
    /// the draw-command count until the `clay-layout` binding lands.
    fn render(&self, state: &AppState) -> usize;
}
