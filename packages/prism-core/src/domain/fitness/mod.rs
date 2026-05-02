//! `domain::fitness` — fitness tracking engine.
//!
//! Port of `@helm/fitness`. MET-based calorie estimation, personal
//! bests tracking, and aggregate fitness summaries.

pub mod engine;
pub mod types;

pub use engine::{
    compute_personal_bests, estimate_calories, fitness_summary, lookup_met_value,
    widget_contributions,
};
pub use types::{
    CalorieEstimate, ExerciseSet, FitnessLog, FitnessSummary, PersonalBest, MET_CYCLING,
    MET_RUNNING, MET_SWIMMING, MET_WALKING, MET_WEIGHT_TRAINING, MET_YOGA,
};
