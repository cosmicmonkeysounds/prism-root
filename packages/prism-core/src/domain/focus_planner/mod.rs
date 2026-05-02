//! `domain::focus_planner` — daily context engine.
//!
//! Port of `@helm/focus-planner`. Planning methods (MIT, 3-things,
//! time-blocking), check-ins, brain dump, and daily context
//! generation.

pub mod engine;
pub mod types;

pub use engine::{
    generate_daily_context, plan_completion_rate, score_plan, sort_by_priority,
    suggested_item_count, widget_contributions,
};
pub use types::{
    CheckIn, ContextSource, DailyContext, DailyContextItem, DailyPlan, PlanItem, PlanScore,
    PlanningMethod, TimeBlock,
};
