//! Widget system — shared vocabulary for core engine widgets.
//!
//! Core engines declare widgets via [`WidgetContribution`]; the builder
//! wraps each contribution in a `CoreWidgetComponent` that implements
//! `Component` and renders through the existing Slint pipeline.
//!
//! [`FieldSpec`] is the unified field descriptor used by property panels,
//! widget config, facet schemas, and dashboard settings. Moved here from
//! `prism-builder::registry` so `prism-core` modules can declare widget
//! contributions without depending on the builder.

pub mod contribution;
pub mod field;

pub use contribution::{
    get_json_field, json_sort_key, DataQuery, FilterOp, LayoutDirection, QueryFilter, QuerySort,
    SignalSpec, TemplateNode, ToolbarAction, ToolbarActionKind, VariantOptionSpec, VariantSpec,
    WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
};
pub use field::{
    prop_bool, prop_f64, prop_str, prop_u64, FieldKind, FieldSpec, FieldValue, FileFieldConfig,
    NumericBounds, SelectOption,
};
