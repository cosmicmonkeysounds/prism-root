//! `dashboard` — widget registry, dashboard presets, and layout engine.
//!
//! Port of `interaction/dashboard/*`:
//! - [`types`] — `WidgetDef`, `WidgetSlot`, `DashboardTab`, `DashboardPreset`
//!   and supporting enums/structs.
//! - [`controller`] — `WidgetRegistry` (insertion-ordered widget definition
//!   store), `DashboardController` (preset/tab/widget CRUD with subscriber
//!   notifications), layout helpers (`layout_rows`, `grid_row_count`,
//!   `clamp_span`), and `built_in_widgets` / `create_default_presets`.

pub mod controller;
pub mod types;

pub use controller::{
    built_in_widgets, clamp_span, create_default_presets, grid_row_count, layout_rows,
    widget_contributions, DashboardController, NewWidgetSlot, WidgetPatch, WidgetRegistry,
};
pub use types::{DashboardPreset, DashboardTab, WidgetDef, WidgetSlot};
