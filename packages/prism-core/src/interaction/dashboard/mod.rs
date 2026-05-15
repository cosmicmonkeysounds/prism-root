//! `dashboard` — widget registration, dashboard presets, and layout engine.
//!
//! - [`types`] — `WidgetSlot`, `DashboardTab`, `DashboardPreset`. Pure
//!   data describing how widgets are laid out inside a tab; widget
//!   definitions themselves are [`crate::widget::WidgetContribution`]s.
//! - [`controller`] — `DashboardController` (preset/tab/widget CRUD
//!   with subscriber notifications), layout helpers (`layout_rows`,
//!   `grid_row_count`, `clamp_span`), `create_default_presets`, and
//!   the `widget_contributions` declaration set (12 widgets — same
//!   path as every other engine, see
//!   `prism-builder::core_widget::collect_all_contributions`).

pub mod controller;
pub mod types;

pub use controller::{
    clamp_span, create_default_presets, grid_row_count, layout_rows, widget_contributions,
    DashboardController, NewWidgetSlot, WidgetPatch,
};
pub use types::{DashboardPreset, DashboardTab, WidgetSlot};
