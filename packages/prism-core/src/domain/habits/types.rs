//! Pure data types for the habits engine.
//!
//! Port of `@helm/habits` type definitions. Habit definitions,
//! completion logs, streak tracking, and wellness scoring.

use serde::{Deserialize, Serialize};

// ── Habit Frequency ──────────────────────────────────────────────

/// How often a habit should be performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HabitFrequency {
    Daily,
    Weekly,
    Monthly,
}

// ── Habit Category ───────────────────────────────────────────────

/// Broad classification for habit grouping and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HabitCategory {
    Health,
    Fitness,
    Mindfulness,
    Productivity,
    Social,
    Custom,
}

// ── Habit Definition ─────────────────────────────────────────────

/// A habit the user wants to track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitDef {
    pub id: String,
    pub name: String,
    pub frequency: HabitFrequency,
    /// Target completions per period (e.g. 3 times per week).
    pub target_count: u32,
    pub category: HabitCategory,
}

// ── Habit Log ────────────────────────────────────────────────────

/// A single completion record for a habit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitLog {
    pub id: String,
    pub habit_id: String,
    /// ISO-8601 date string (YYYY-MM-DD).
    pub date: String,
    pub completed: bool,
    /// Optional quantitative value (e.g. glasses of water, minutes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

// ── Streak Info ──────────────────────────────────────────────────

/// Computed streak statistics for a single habit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreakInfo {
    pub current_streak: u32,
    pub longest_streak: u32,
    pub total_completions: u32,
    /// ISO-8601 date of the most recent completion, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_completed: Option<String>,
}

// ── Wellness ─────────────────────────────────────────────────────

/// A scored dimension contributing to overall wellness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WellnessCategory {
    pub name: String,
    /// Score from 0.0 to 1.0.
    pub score: f64,
    /// Relative weight for composite calculation.
    pub weight: f64,
}

/// Aggregated wellness across multiple categories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WellnessSummary {
    /// Weighted average of category scores (0.0 to 1.0).
    pub composite_score: f64,
    pub categories: Vec<WellnessCategory>,
}
