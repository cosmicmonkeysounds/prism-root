//! `query` — pure filter / sort / group pipeline over `GraphObject`.
//!
//! Port of the useful half of `interaction/view/view-config.ts`.
//! Deliberately renamed from `view` per the project feedback rule
//! that "every view is a `prism_builder::Component`, not a ViewMode
//! enum" — this module is the data-layer query each builder
//! component uses to materialise the collection it displays, not a
//! view mode registry. The ViewMode enum, `SavedView`, and `LiveView`
//! from the legacy tree are intentionally **not** ported.

pub mod pipeline;

pub use pipeline::{
    apply_filters, apply_groups, apply_query, apply_sorts, get_field_value, FilterConfig,
    FilterOp, GroupConfig, GroupedResult, Query, SortConfig, SortDir,
};
