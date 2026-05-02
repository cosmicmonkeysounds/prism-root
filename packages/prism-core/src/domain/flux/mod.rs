//! `domain::flux` — operational hub registry (productivity, people,
//! finance, inventory).
//!
//! Port of `packages/prism-core/src/domain/flux/*` at commit 8426588.
//! Splits the original two-file TS module (`flux-types.ts` +
//! `flux.ts`) into a `types` data module and an `engine` factory
//! module. The `FluxRegistry` trait keeps the original method surface
//! (entity / edge / preset lookups + import/export) but is exposed as
//! a concrete struct since the Rust port needs no interface
//! inheritance.

pub mod bus;
pub mod engine;
pub mod routines;
pub mod seed;
pub mod types;

pub use bus::{default_rules, event_type_str, resolve_actions, BusRule, FluxAction, FluxEvent};
pub use engine::{create_flux_registry, FluxImportError, FluxRegistry};
pub use routines::{
    default_routines, find_routine, routines_by_category, Routine, RoutineAction, RoutineCategory,
    RoutineStep,
};
pub use seed::{generate_seed_data, SeedData, SeedEdge, SeedObject};
pub use types::{
    flux_categories, flux_edges, flux_types, FluxAutomationAction, FluxAutomationActionKind,
    FluxAutomationPreset, FluxExportFormat, FluxExportOptions, FluxImportResult, FluxTriggerKind,
    StatusOption, CONTACT_TYPES, GOAL_STATUSES, INVOICE_STATUSES, ITEM_STATUSES, PROJECT_STATUSES,
    TASK_STATUSES, TRANSACTION_TYPES,
};
