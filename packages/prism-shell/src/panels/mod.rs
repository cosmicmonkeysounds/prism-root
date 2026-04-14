//! Panel modules. One file per panel, each exporting a struct that
//! implements [`Panel::declare`]. The legacy TS tree had ~40 of these
//! — we add them back one at a time as the Phase-1/2 port advances.

pub mod identity;

use crate::AppState;

#[cfg(feature = "clay")]
use clay_layout::ClayLayoutScope;

/// A panel is a single full-window region that emits its own Clay
/// layout. Only ever one is active at a time for now (Phase 0);
/// Phase 1 grows a workspace splitter that composes several.
pub trait Panel {
    /// Declare the panel into an open Clay layout scope. The caller
    /// has already opened a root element, so `declare` is free to
    /// nest whatever children it needs directly.
    #[cfg(feature = "clay")]
    fn declare<'clay>(&self, state: &AppState, scope: &mut ClayLayoutScope<'clay, 'clay, (), ()>);
}
