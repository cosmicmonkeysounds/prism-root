//! `domain::goals` — goal hierarchy and progress engine.
//!
//! Port of `@helm/goals` TypeScript module. Computes progress from
//! milestones or child goals, rolls up through hierarchy, and checks
//! deadline proximity.

pub mod engine;
pub mod types;

pub use engine::{
    check_deadline_alerts, compute_goal_progress, compute_hierarchy_progress,
    milestone_completion_rate, widget_contributions,
};
pub use types::{
    DeadlineAlert, Goal, GoalAlert, GoalProgress, GoalStatus, Milestone, ProgressMode,
};
