//! `domain::habits` — habit tracking and wellness engine.
//!
//! Port of `@helm/habits` TypeScript module. Streak computation,
//! completion rates, and composite wellness scoring across habit
//! categories.

pub mod engine;
pub mod types;

pub use engine::{completion_rate, compute_streak, wellness_summary, widget_contributions};
pub use types::{
    HabitCategory, HabitDef, HabitFrequency, HabitLog, StreakInfo, WellnessCategory,
    WellnessSummary,
};
